//! Feature 1 (spec §7.1): preflight → fetch → aggregate → match → [REVIEW] → price → write →
//! reprice → report. `start` runs up to the review pause; `commit` runs the rest.

use std::collections::HashSet;

use serde_json::json;

use super::planner::{aggregate, find_row, find_row_index, plan_write, WriteAction};
use super::{cost_for, fatal, i, opt_i, preflight, readback, reprice_items, t, JobCtx, Services};
use crate::domain::{Authenticity, ImportMode};
use crate::errors::{kinds, BridgeError, Result};
use crate::landau::{AttachBody, PatchBody};
use crate::matching::{Decision, Matcher};
use crate::settings::SUPPLIER_ID;
use crate::supplier::Progress;

#[derive(Debug, Clone)]
pub struct ImportParams {
    pub mode: ImportMode,
    pub since: Option<String>,
    /// At most this many not-yet-fetched invoices this run (`None` = all). Invoices are cached, so
    /// a capped run simply continues next time; the live e2e test uses it to stay short.
    pub fetch_limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Paused: this many items need the seller.
    Review(usize),
    Done,
}

pub async fn start(ctx: &JobCtx, svc: &Services, params: &ImportParams) -> Result<Outcome> {
    let (_, _, store) = svc.settings.require_landau()?;
    let store = store.to_string();
    preflight(ctx, svc, &store).await?;
    fetch(ctx, svc, params.fetch_limit).await?;
    aggregate_phase(ctx, params)?;
    let pending = match_phase(ctx, svc).await?;
    if pending > 0 {
        ctx.db.set_job_status(ctx.job_id, "REVIEW", "REVIEW")?;
        ctx.info(
            "REVIEW",
            &format!("{pending} item(s) await the seller"),
            None,
        );
        super::emit_event(
            &ctx.app,
            super::EVENT_DONE,
            json!({"job_id": ctx.job_id, "status": "REVIEW"}),
        );
        return Ok(Outcome::Review(pending));
    }
    commit(ctx, svc).await?;
    Ok(Outcome::Done)
}

/// Ledger headers → unseen invoices' lines. Per-invoice parse failures are recorded and skipped;
/// session/transport failures stop the job.
async fn fetch(ctx: &JobCtx, svc: &Services, limit: Option<usize>) -> Result<()> {
    ctx.phase("FETCH")?;
    let progress_ctx = (ctx.app.clone(), ctx.job_id);
    let progress: Progress = std::sync::Arc::new(move |done, total| {
        super::emit_event(
            &progress_ctx.0,
            super::EVENT_PROGRESS,
            super::Progress {
                job_id: progress_ctx.1,
                phase: "FETCH".into(),
                done,
                total,
                message: "فهرست فاکتورها".into(),
            },
        );
    });
    let headers = svc.supplier.ledger(progress).await?;
    ctx.check_cancel()?;
    let mut unpaid = 0;
    for h in &headers {
        ctx.db.upsert_invoice(SUPPLIER_ID, h)?;
        if !h.paid {
            unpaid += 1;
        }
    }
    ctx.info(
        "FETCH",
        &format!("{} ledger row(s), {unpaid} unpaid", headers.len()),
        None,
    );
    let mut todo = ctx.db.unfetched_invoice_ids(SUPPLIER_ID)?;
    if let Some(limit) = limit {
        if todo.len() > limit {
            ctx.info(
                "FETCH",
                &format!(
                    "fetch capped at {limit} of {} pending invoice(s)",
                    todo.len()
                ),
                None,
            );
            todo.truncate(limit);
        }
    }
    let total = todo.len() as u32;
    for (n, invoice_id) in todo.into_iter().enumerate() {
        ctx.check_cancel()?;
        let url = ctx.db.invoice_url(invoice_id)?;
        match svc.supplier.invoice_lines(&url).await {
            Ok(lines) => ctx.db.replace_lines(invoice_id, &lines)?,
            Err(e) if fatal(&e) => return Err(e),
            Err(e) => ctx.warn(
                "FETCH",
                e.kind(),
                &format!("فاکتور {url}: {}", e.message_fa()),
                Some(json!({"invoice_id": invoice_id})),
            ),
        }
        ctx.progress("FETCH", n as u32 + 1, total, "جزئیات فاکتورها");
    }
    Ok(())
}

fn aggregate_phase(ctx: &JobCtx, params: &ImportParams) -> Result<()> {
    ctx.phase("AGGREGATE")?;
    let lines = ctx.db.lines_with_invoices(SUPPLIER_ID)?;
    let already = ctx.db.imported_lines()?;
    let agg = aggregate(&lines, params.mode, params.since.as_deref(), &already);
    if !agg.skipped_unpaid.is_empty() {
        ctx.warn(
            "AGGREGATE",
            kinds::INVOICE_UNPAID,
            &format!(
                "{} فاکتور پرداخت‌نشده نادیده گرفته شد.",
                agg.skipped_unpaid.len()
            ),
            Some(json!({"invoice_ids": agg.skipped_unpaid})),
        );
    }
    if !agg.skipped_imported.is_empty() {
        ctx.warn(
            "AGGREGATE",
            kinds::INVOICE_ALREADY_IMPORTED,
            &format!(
                "{} فاکتور قبلاً منتقل شده بود و دوباره شمرده نشد.",
                agg.skipped_imported.len()
            ),
            Some(json!({"invoice_ids": agg.skipped_imported})),
        );
    }
    if agg.non_positive_lines > 0 {
        ctx.warn(
            "AGGREGATE",
            kinds::LINE_NON_POSITIVE_QTY,
            &format!(
                "{} ردیف با تعداد صفر یا منفی نادیده گرفته شد.",
                agg.non_positive_lines
            ),
            None,
        );
    }
    let mut invoice_ids: HashSet<i64> = HashSet::new();
    for item in &agg.items {
        let id = ctx
            .db
            .add_job_item(ctx.job_id, &item.code, &item.name, item.qty)?;
        ctx.item(
            id,
            &[(
                "invoice_ids_json",
                t(&serde_json::to_string(&item.invoice_ids)?),
            )],
        )?;
        if item.negative {
            ctx.warn(
                "AGGREGATE",
                kinds::NEGATIVE_QUANTITY,
                &format!(
                    "«{}»: مرجوعی‌ها بیشتر از خرید بود؛ تعداد صفر منظور شد.",
                    item.name
                ),
                Some(json!({"item_id": id})),
            );
        }
        invoice_ids.extend(item.invoice_ids.iter().copied());
    }
    let mut ids: Vec<i64> = invoice_ids.into_iter().collect();
    ids.sort_unstable();
    ctx.db.set_job_summary(
        ctx.job_id,
        &json!({"invoice_ids": ids, "items": agg.items.len()}),
    )?;
    ctx.info(
        "AGGREGATE",
        &format!(
            "{} distinct part(s) from {} invoice(s)",
            agg.items.len(),
            ids.len()
        ),
        None,
    );
    Ok(())
}

/// Returns how many items need review.
async fn match_phase(ctx: &JobCtx, svc: &Services) -> Result<usize> {
    ctx.phase("MATCH")?;
    let matcher = Matcher::new(ctx.db.clone(), SUPPLIER_ID, svc.settings.threshold);
    let landau = &svc.landau;
    let search = |q: String, limit: usize, skip: usize| async move {
        landau.registry_search(&q, limit, skip).await
    };
    let items = ctx.db.job_items(ctx.job_id)?;
    let total = items.len() as u32;
    let mut pending = 0usize;
    for (n, item) in items.iter().enumerate() {
        ctx.check_cancel()?;
        match matcher.decide(&search, &item.code, &item.name).await? {
            Decision::Matched {
                product_id,
                product_name,
                ..
            } => {
                ctx.item(
                    item.id,
                    &[
                        ("product_id", t(&product_id)),
                        ("product_name", t(&product_name)),
                        ("status", t("PLANNED")),
                    ],
                )?;
            }
            Decision::None => {
                ctx.item(
                    item.id,
                    &[
                        ("status", t("SKIPPED")),
                        ("error_kind", t(kinds::NONE_DECISION)),
                        ("error_msg", t("این قطعه در لندو نیست (تصمیم قبلی).")),
                    ],
                )?;
            }
            Decision::Review(scored) => {
                pending += 1;
                ctx.item(
                    item.id,
                    &[
                        ("status", t("REVIEW")),
                        ("candidates_json", t(&serde_json::to_string(&scored)?)),
                    ],
                )?;
            }
        }
        ctx.progress("MATCH", n as u32 + 1, total, "تطبیق با فهرست لندو");
    }
    if pending > 0 {
        ctx.warn(
            "MATCH",
            kinds::UNMATCHED,
            &format!("{pending} قطعه نیاز به تأیید شما دارد."),
            None,
        );
    }
    Ok(pending)
}

/// Price → write → reprice → report over every item that is PLANNED with a product.
pub async fn commit(ctx: &JobCtx, svc: &Services) -> Result<()> {
    let (_, _, store) = svc.settings.require_landau()?;
    let store = store.to_string();
    let job = ctx
        .db
        .job(ctx.job_id)?
        .ok_or(BridgeError::JobNotFound(ctx.job_id))?;
    let mode = job
        .mode
        .as_deref()
        .and_then(ImportMode::parse)
        .unwrap_or(ImportMode::Zero);
    ctx.db.set_job_status(ctx.job_id, "RUNNING", "PRICE")?;

    // deferred review items are left out of this run
    for item in ctx
        .db
        .job_items(ctx.job_id)?
        .iter()
        .filter(|i| i.status == "REVIEW")
    {
        ctx.item(
            item.id,
            &[
                ("status", t("SKIPPED")),
                ("error_kind", t(kinds::UNMATCHED)),
                (
                    "error_msg",
                    t("بدون تصمیم؛ در اجرای بعدی دوباره پرسیده می‌شود."),
                ),
            ],
        )?;
    }

    ctx.phase("PRICE")?;
    let items: Vec<_> = ctx
        .db
        .job_items(ctx.job_id)?
        .into_iter()
        .filter(|i| i.status == "PLANNED" && i.product_id.is_some())
        .collect();
    let total = items.len() as u32;
    for (n, item) in items.iter().enumerate() {
        ctx.check_cancel()?;
        match cost_for(ctx, svc, "PRICE", &item.code).await {
            Ok(Some(cost)) => ctx.item(item.id, &[("cost", i(cost))])?,
            Ok(None) => {
                let e = BridgeError::NoCurrentPrice {
                    code: item.code.clone(),
                };
                ctx.error("PRICE", &e, Some(json!({"item_id": item.id})));
                ctx.item(
                    item.id,
                    &[
                        ("status", t("SKIPPED")),
                        ("error_kind", t(e.kind())),
                        ("error_msg", t(&e.message_fa())),
                    ],
                )?;
            }
            Err(e) if fatal(&e) => return Err(e),
            Err(e) => {
                ctx.error("PRICE", &e, Some(json!({"item_id": item.id})));
                ctx.item(
                    item.id,
                    &[
                        ("status", t("FAILED")),
                        ("error_kind", t(e.kind())),
                        ("error_msg", t(&e.message_fa())),
                    ],
                )?;
            }
        }
        ctx.progress("PRICE", n as u32 + 1, total, "قیمت روز بارزپخش");
    }

    ctx.phase("WRITE")?;
    let mut shelf = svc.landau.shelf(&store).await?;
    let items: Vec<_> = ctx
        .db
        .job_items(ctx.job_id)?
        .into_iter()
        .filter(|i| i.status == "PLANNED" && i.cost.is_some())
        .collect();
    let total = items.len() as u32;
    let mut written: Vec<(i64, String)> = Vec::new();
    for (n, item) in items.iter().enumerate() {
        ctx.check_cancel()?;
        let product = item.product_id.clone().unwrap_or_default();
        let cost = item.cost.unwrap_or(0);
        let authenticity = item
            .authenticity
            .as_deref()
            .and_then(Authenticity::parse)
            .unwrap_or(svc.settings.default_authenticity);
        let idx = find_row_index(&shelf, &product, authenticity.as_str());
        let existing = idx.map(|pos| &shelf[pos]);
        let action = plan_write(existing, item.qty, mode);
        // stock_id, qty_before, qty_after, price_before, action verb
        #[allow(clippy::type_complexity)]
        let result: Result<(String, Option<i64>, Option<i64>, Option<i64>, &str)> = match &action {
            WriteAction::Attach { inventory } => {
                match svc
                    .landau
                    .attach(&AttachBody {
                        product: product.clone(),
                        store: store.clone(),
                        inventory: *inventory,
                        cost,
                        authenticity,
                    })
                    .await
                {
                    Ok(row) => {
                        shelf.push(row.clone());
                        Ok((row.id, None, Some(*inventory), None, "ATTACH"))
                    }
                    Err(BridgeError::DuplicateRow) => {
                        // the mirror lagged behind a row that exists: re-read and patch instead
                        shelf = svc.landau.shelf(&store).await?;
                        match find_row(&shelf, &product, authenticity.as_str()) {
                            Some(row) => {
                                let inv = (mode == ImportMode::Quantity)
                                    .then_some(row.inventory + item.qty);
                                svc.landau
                                    .patch(
                                        &row.id,
                                        &PatchBody {
                                            inventory: inv,
                                            cost: Some(cost),
                                        },
                                    )
                                    .await
                                    .map(|_| {
                                        (
                                            row.id.clone(),
                                            Some(row.inventory),
                                            inv.or(Some(row.inventory)),
                                            row.price,
                                            "PATCH",
                                        )
                                    })
                            }
                            None => Err(BridgeError::DuplicateRow),
                        }
                    }
                    Err(e) => Err(e),
                }
            }
            WriteAction::Patch {
                stock_id,
                inventory,
                qty_before,
            } => {
                let price_before = existing.and_then(|r| r.price);
                svc.landau
                    .patch(
                        stock_id,
                        &PatchBody {
                            inventory: *inventory,
                            cost: Some(cost),
                        },
                    )
                    .await
                    .map(|_| {
                        (
                            stock_id.clone(),
                            Some(*qty_before),
                            inventory.or(Some(*qty_before)),
                            price_before,
                            "PATCH",
                        )
                    })
            }
        };
        match result {
            Ok((stock_id, before, after, price_before, verb)) => {
                // keep the local mirror current: a second item mapped to the same
                // (product, authenticity) tuple must see this write, not the stale figure.
                if let Some(new_qty) = after {
                    if let Some(pos) = shelf.iter().position(|r| r.id == stock_id) {
                        shelf[pos].inventory = new_qty;
                        shelf[pos].cost = Some(cost);
                    }
                }
                ctx.item(
                    item.id,
                    &[
                        ("stock_id", t(&stock_id)),
                        ("action", t(verb)),
                        ("qty_before", opt_i(before)),
                        ("qty_after", opt_i(after)),
                        ("price_before", opt_i(price_before)),
                        ("authenticity", t(authenticity.as_str())),
                        ("status", t("WRITTEN")),
                    ],
                )?;
                written.push((item.id, stock_id));
                // the transfer guard is written as each item is written, not deferred to the
                // report step, so a failure or cancel after this point cannot double-add.
                if mode == ImportMode::Quantity {
                    let lines: Vec<(i64, String)> = item
                        .invoice_ids
                        .iter()
                        .map(|invoice_id| (*invoice_id, item.code.clone()))
                        .collect();
                    ctx.db.mark_lines_imported(&lines, ctx.job_id)?;
                }
            }
            Err(e) if fatal(&e) => return Err(e),
            Err(e) => {
                ctx.error(
                    "WRITE",
                    &e,
                    Some(json!({"item_id": item.id, "product": product})),
                );
                ctx.item(
                    item.id,
                    &[
                        ("status", t("FAILED")),
                        ("error_kind", t(e.kind())),
                        ("error_msg", t(&e.message_fa())),
                    ],
                )?;
            }
        }
        ctx.progress("WRITE", n as u32 + 1, total, "نوشتن روی قفسه لندو");
    }

    reprice_items(ctx, svc, &store, &written).await?;
    readback(ctx, svc, &store, &written).await?;

    let all = ctx.db.job_items(ctx.job_id)?;
    let count = |status: &str| all.iter().filter(|i| i.status == status).count();
    let summary = json!({
        "items": all.len(),
        "done": count("DONE"),
        "attached": all.iter().filter(|i| i.action.as_deref() == Some("ATTACH")).count(),
        "patched": all.iter().filter(|i| i.action.as_deref() == Some("PATCH")).count(),
        "skipped": count("SKIPPED"),
        "failed": count("FAILED"),
        "invoice_ids": job.summary.as_ref().and_then(|s| s.get("invoice_ids")).cloned().unwrap_or(json!([])),
    });
    ctx.db.set_job_summary(ctx.job_id, &summary)?;
    ctx.info("REPORT", &summary.to_string(), None);
    Ok(())
}
