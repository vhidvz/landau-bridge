//! Feature 1 (spec §7.1): preflight → fetch → aggregate → match → [REVIEW] → price → write →
//! reprice → report. `start` runs up to the review pause; `commit` runs the rest.

use std::collections::HashSet;

use futures_util::stream::{FuturesUnordered, StreamExt};
use serde_json::json;

use super::planner::{aggregate, find_row, floored, plan_write, shortfall, WriteAction};
use super::pricing;
use super::{
    fatal, i, opt_i, preflight, quote_for, report, reprice_items, t, JobCtx, Services,
    VatMismatches,
};
use crate::domain::{Authenticity, ImportMode};
use crate::errors::{kinds, BridgeError, Result};
use crate::landau::{AttachBody, PatchBody, ShelfRow};
use crate::matching::{Decision, Matcher};
use crate::settings::SUPPLIER_ID;
use crate::supplier::Progress;

#[derive(Debug, Clone)]
pub struct ImportParams {
    pub mode: ImportMode,
    pub since: Option<String>,
    /// Ask the supplier only for invoices dated on/after `since` (quantity mode); otherwise the
    /// whole ledger is listed and cached, as a zero-mode run always does.
    pub fetch_since_only: bool,
    /// At most this many not-yet-fetched invoices this run (`None` = all). Invoices are cached, so
    /// a capped run simply continues next time; the live e2e test uses it to stay short.
    pub fetch_limit: Option<usize>,
}

impl ImportParams {
    /// The date the ledger request is bounded to, if any.
    pub fn ledger_from(&self) -> Option<&str> {
        match self.mode {
            ImportMode::Quantity if self.fetch_since_only => self.since.as_deref(),
            _ => None,
        }
    }
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
    fetch(ctx, svc, params.ledger_from(), params.fetch_limit).await?;
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

/// Ledger headers (from `from` on, or all of them) → unseen invoices' lines in that range.
/// Per-invoice parse failures are recorded and skipped; session/transport failures stop the job.
async fn fetch(
    ctx: &JobCtx,
    svc: &Services,
    from: Option<&str>,
    limit: Option<usize>,
) -> Result<()> {
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
    let headers = svc.supplier.ledger(from, progress).await?;
    ctx.check_cancel()?;
    ctx.db.upsert_invoices(SUPPLIER_ID, &headers)?;
    let unpaid = headers.iter().filter(|h| !h.paid).count();
    ctx.info(
        "FETCH",
        &format!("{} ledger row(s), {unpaid} unpaid", headers.len()),
        None,
    );
    let mut todo = ctx.db.unfetched_invoice_ids(SUPPLIER_ID, from)?;
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
    // An invoice that fails on its own (the site's error page past every retry, a page that did not
    // parse) is not reported yet: it gets one more pass once the rest are in, minutes or hours
    // later, and only what fails twice reaches the report. A page the site served badly once is
    // not a layout change (2026-10-07: four such reports, each invoice fine on the next fetch).
    let again = fetch_all(ctx, svc, todo, "جزئیات فاکتورها").await?;
    for (invoice_id, e) in again.iter() {
        tracing::warn!(job = ctx.job_id, invoice_id, error = %e, "invoice fetch failed; trying again at the end of the pass");
    }
    let again = again.into_iter().map(|(id, _)| id).collect();
    for (invoice_id, e) in fetch_all(ctx, svc, again, "تلاش دوباره برای فاکتورهای ناموفق").await?
    {
        ctx.warn(
            "FETCH",
            e.kind(),
            &format!(
                "فاکتور {}: {}",
                ctx.db.invoice_url(invoice_id)?,
                e.message_fa()
            ),
            Some(json!({"invoice_id": invoice_id})),
        );
    }
    Ok(())
}

/// Fetches `ids` with up to [`crate::supplier::Supplier::parallelism`] in flight — the supplier
/// decides how many of those actually run at once — and returns the invoices that failed on their
/// own. A cancel starts nothing new and lets what is in flight land (as the one-at-a-time loop
/// let its current invoice land); a fatal failure stops at once, and whatever was in flight is
/// fetched by the next run, since only a stored invoice counts as fetched.
async fn fetch_all(
    ctx: &JobCtx,
    svc: &Services,
    ids: Vec<i64>,
    message: &str,
) -> Result<Vec<(i64, BridgeError)>> {
    let parallel = svc.supplier.parallelism().max(1);
    let total = ids.len() as u32;
    let mut ids = ids.into_iter();
    let mut running = FuturesUnordered::new();
    let (mut failed, mut done, mut cancelled) = (Vec::new(), 0, false);
    loop {
        while !cancelled && running.len() < parallel {
            let Some(id) = ids.next() else { break };
            if ctx.check_cancel().is_err() {
                cancelled = true;
                break;
            }
            running.push(async move { (id, fetch_lines(ctx, svc, id).await) });
        }
        let Some((id, outcome)) = running.next().await else {
            break;
        };
        if let Some(e) = outcome? {
            failed.push((id, e));
        }
        done += 1;
        ctx.progress("FETCH", done, total, message);
    }
    ctx.check_cancel()?;
    Ok(failed)
}

/// One invoice's lines into the cache. `Ok(Some(e))` is a failure of that invoice alone; `Err` is
/// one that stops the job ([`fatal`], or the database).
async fn fetch_lines(ctx: &JobCtx, svc: &Services, invoice_id: i64) -> Result<Option<BridgeError>> {
    let url = ctx.db.invoice_url(invoice_id)?;
    match svc.supplier.invoice_lines(&url).await {
        Ok(lines) => {
            ctx.db.replace_lines(invoice_id, &lines)?;
            Ok(None)
        }
        Err(e) if fatal(&e) => Err(e),
        Err(e) => Ok(Some(e)),
    }
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
        // One failed search (a rejected query, a parse slip) must not take the whole job down:
        // the item goes to review with no candidates and the seller re-searches by hand.
        // Session/transport failures still stop the job.
        let decision = match matcher.decide(&search, &item.code, &item.name).await {
            Ok(d) => d,
            Err(e) if fatal(&e) => return Err(e),
            Err(e) => {
                ctx.error("MATCH", &e, Some(json!({"item_id": item.id})));
                Decision::Review(Vec::new())
            }
        };
        match decision {
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
                        ("error_msg", t("این قطعه در لاندا نیست (تصمیم قبلی).")),
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
        ctx.progress("MATCH", n as u32 + 1, total, "تطبیق با فهرست لاندا");
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
    let mut vat = VatMismatches::default();
    for (n, item) in items.iter().enumerate() {
        ctx.check_cancel()?;
        match quote_for(ctx, svc, &item.name, &item.code, &mut vat).await {
            Ok(Some(c)) => {
                if !item.code.is_empty() && c.code != item.code {
                    ctx.db.add_item_note(
                        item.id,
                        &format!("کد بارزپخش عوض شد: {} ← {}", item.code, c.code),
                    )?;
                }
                if c.renamed {
                    ctx.db
                        .add_item_note(item.id, &format!("نام در بارزپخش عوض شد: «{}»", c.name))?;
                }
                if !c.available {
                    ctx.db.add_item_note(item.id, "ناموجود در بارزپخش")?;
                }
                ctx.item(
                    item.id,
                    &[("cost", i(c.cost)), ("available", i(c.available as i64))],
                )?
            }
            Ok(None) => {
                let e = BridgeError::NoCurrentPrice {
                    code: if item.code.is_empty() {
                        item.name.clone()
                    } else {
                        item.code.clone()
                    },
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
    vat.warn(ctx, "PRICE", svc.settings.vat_percent);

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
        let authenticity = item
            .authenticity
            .as_deref()
            .and_then(Authenticity::parse)
            .unwrap_or(svc.settings.default_authenticity);
        let existing = find_row(&shelf, &product, authenticity.as_str());
        // §14 d: an unavailable quote never lowers a row's cost.
        let (cost, held) = pricing::effective_cost(
            item.cost.unwrap_or(0),
            item.available.unwrap_or(true),
            existing.and_then(|r| r.cost),
        );
        if held {
            ctx.db
                .add_item_note(item.id, "ناموجود؛ قیمت پایین‌تر نادیده گرفته شد")?;
            ctx.item(item.id, &[("cost", i(cost))])?;
        }
        let action = plan_write(existing, item.qty, mode);
        let result: Result<Written> = match &action {
            WriteAction::Attach { inventory } => {
                match svc
                    .landau
                    .attach(&AttachBody {
                        product: product.clone(),
                        store: store.clone(),
                        inventory: *inventory,
                        cost,
                        price: pricing::floor_price(cost, svc.settings.min_margin_percent),
                        authenticity,
                    })
                    .await
                {
                    Ok(row) => {
                        shelf.push(row.clone());
                        Ok(Written {
                            stock_id: row.id,
                            qty_before: None,
                            qty_after: Some(*inventory),
                            price_before: None,
                            price_after: row.price,
                            verb: "ATTACH",
                            shortfall: shortfall(0, item.qty),
                        })
                    }
                    Err(BridgeError::DuplicateRow) => {
                        // The mirror lagged behind a row that exists: re-read the live shelf and
                        // patch that row instead. Only the row found is merged into the local
                        // shelf — replacing the whole mirror would drop the figures this run has
                        // already written to sibling rows the lagging read does not show yet.
                        let fresh = svc.landau.shelf(&store).await?;
                        match find_row(&fresh, &product, authenticity.as_str()).cloned() {
                            Some(row) => {
                                merge_row(&mut shelf, row.clone());
                                let inv = (mode == ImportMode::Quantity)
                                    .then_some(floored(row.inventory, item.qty));
                                svc.landau
                                    .patch(
                                        &row.id,
                                        &PatchBody {
                                            inventory: inv,
                                            cost: Some(cost),
                                        },
                                    )
                                    .await
                                    .map(|reply| Written {
                                        stock_id: row.id.clone(),
                                        qty_before: Some(row.inventory),
                                        qty_after: inv.or(Some(row.inventory)),
                                        price_before: row.price,
                                        price_after: reply.price,
                                        verb: "PATCH",
                                        shortfall: shortfall(row.inventory, item.qty),
                                    })
                            }
                            // Not live either: a seller who removed this exact part earlier and is
                            // now importing it again. Landau's uniqueness index on (product, store,
                            // …) still holds the soft-deleted row, so a fresh attach 409s forever —
                            // the row must be restored before it can be patched (spec §2.1
                            // amendment, discovered live: "a removed stock row in this store still
                            // holds this part; restore it instead of attaching a second one").
                            // A restored row is a fresh attach: whatever it held when it was removed
                            // is gone from the seller's point of view, so its inventory is set to
                            // this run's quantity (zero in zero mode), not added to.
                            None => {
                                let trash = svc.landau.shelf_deleted(&store).await?;
                                match find_row(&trash, &product, authenticity.as_str()).cloned() {
                                    Some(removed) => {
                                        svc.landau.restore(&removed.id).await?;
                                        let inv = if mode == ImportMode::Quantity {
                                            floored(0, item.qty)
                                        } else {
                                            0
                                        };
                                        let outcome = svc
                                            .landau
                                            .patch(
                                                &removed.id,
                                                &PatchBody {
                                                    inventory: Some(inv),
                                                    cost: Some(cost),
                                                },
                                            )
                                            .await
                                            .map(|reply| Written {
                                                stock_id: removed.id.clone(),
                                                qty_before: Some(removed.inventory),
                                                qty_after: Some(inv),
                                                price_before: removed.price,
                                                price_after: reply.price,
                                                verb: "PATCH",
                                                shortfall: shortfall(0, item.qty),
                                            });
                                        if outcome.is_ok() {
                                            ctx.info(
                                                "WRITE",
                                                "ردیف حذف‌شده بازیابی شد.",
                                                Some(
                                                    json!({"item_id": item.id, "stock_id": removed.id}),
                                                ),
                                            );
                                            let mut restored = removed;
                                            restored.inventory = inv;
                                            restored.cost = Some(cost);
                                            merge_row(&mut shelf, restored);
                                        }
                                        outcome
                                    }
                                    None => Err(BridgeError::DuplicateRow),
                                }
                            }
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
                    .map(|reply| Written {
                        stock_id: stock_id.clone(),
                        qty_before: Some(*qty_before),
                        qty_after: inventory.or(Some(*qty_before)),
                        price_before,
                        price_after: reply.price,
                        verb: "PATCH",
                        shortfall: shortfall(*qty_before, item.qty),
                    })
            }
        };
        match result {
            Ok(Written {
                stock_id,
                qty_before: before,
                qty_after: after,
                price_before,
                price_after,
                verb,
                shortfall: short,
            }) => {
                // keep the local mirror current: a second item mapped to the same
                // (product, authenticity) tuple must see this write, not the stale figure.
                if let Some(new_qty) = after {
                    if let Some(pos) = shelf.iter().position(|r| r.id == stock_id) {
                        shelf[pos].inventory = new_qty;
                        shelf[pos].cost = Some(cost);
                    }
                }
                if mode == ImportMode::Quantity && short > 0 {
                    ctx.warn(
                        "WRITE",
                        kinds::NEGATIVE_QUANTITY,
                        &format!(
                            "«{}»: مرجوعی‌ها {short} عدد بیشتر از موجودی قفسه بود؛ موجودی صفر شد.",
                            item.name
                        ),
                        Some(json!({"item_id": item.id, "stock_id": stock_id, "shortfall": short, "qty": item.qty, "qty_before": before})),
                    );
                }
                // the transfer guard is written first, before the item is stamped WRITTEN: if
                // the process dies between the two SQLite writes, the guard alone is enough to
                // stop a re-aggregation from double-adding this (invoice, code) pair next run —
                // an orphaned guard only loses this item's report status, never inventory.
                if mode == ImportMode::Quantity {
                    let lines: Vec<(i64, String)> = item
                        .invoice_ids
                        .iter()
                        .map(|invoice_id| (*invoice_id, item.code.clone()))
                        .collect();
                    ctx.db.mark_lines_imported(&lines, ctx.job_id)?;
                }
                ctx.item(
                    item.id,
                    &[
                        ("stock_id", t(&stock_id)),
                        ("action", t(verb)),
                        ("qty_before", opt_i(before)),
                        ("qty_after", opt_i(after)),
                        ("price_before", opt_i(price_before)),
                        ("price_after", opt_i(price_after)),
                        ("authenticity", t(authenticity.as_str())),
                        ("status", t("WRITTEN")),
                    ],
                )?;
                written.push((item.id, stock_id));
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
        ctx.progress("WRITE", n as u32 + 1, total, "نوشتن روی قفسه لاندا");
    }

    reprice_items(ctx, svc, &written).await?;
    report(ctx, svc, &written)?;

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

/// What one successful shelf write left behind, for the item's report columns.
struct Written {
    stock_id: String,
    qty_before: Option<i64>,
    qty_after: Option<i64>,
    price_before: Option<i64>,
    /// The price Landau answered on the write (plan 107); the REPRICE phase confirms it.
    price_after: Option<i64>,
    verb: &'static str,
    /// Units of a negative transfer the shelf could not absorb (0 unless the floor bit).
    shortfall: i64,
}

/// Put a freshly read row into the local shelf mirror: replace the entry with the same id, or
/// add it. Never replaces the whole mirror (see the 409 branch of `commit`).
fn merge_row(shelf: &mut Vec<ShelfRow>, row: ShelfRow) {
    match shelf.iter().position(|r| r.id == row.id) {
        Some(pos) => shelf[pos] = row,
        None => shelf.push(row),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, inventory: i64) -> ShelfRow {
        ShelfRow {
            id: id.into(),
            product: "p".into(),
            product_name: None,
            brand: None,
            store: None,
            inventory,
            price: None,
            cost: None,
            condition: Some("NEW".into()),
            authenticity: Some("ORIGINAL".into()),
            price_source: None,
        }
    }

    #[test]
    fn the_ledger_is_bounded_only_for_a_quantity_run_that_asks_for_it() {
        let params = |mode, fetch_since_only| ImportParams {
            mode,
            since: Some("1405/01/01".into()),
            fetch_since_only,
            fetch_limit: None,
        };
        assert_eq!(
            params(ImportMode::Quantity, true).ledger_from(),
            Some("1405/01/01")
        );
        assert_eq!(params(ImportMode::Quantity, false).ledger_from(), None);
        assert_eq!(params(ImportMode::Zero, true).ledger_from(), None);
    }

    /// Serves a three-invoice ledger: `/f/once` fails on its first fetch only, `/f/never` on
    /// every fetch, `/f/ok` never. Records every invoice URL asked for.
    #[derive(Default)]
    struct FlakySupplier {
        asked: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl crate::supplier::Supplier for FlakySupplier {
        fn id(&self) -> &'static str {
            "flaky"
        }
        async fn login(&self) -> Result<()> {
            Ok(())
        }
        async fn ledger(
            &self,
            _from: Option<&str>,
            _progress: Progress,
        ) -> Result<Vec<crate::domain::InvoiceHeader>> {
            Ok(["once", "never", "ok"]
                .iter()
                .enumerate()
                .map(|(n, name)| crate::domain::InvoiceHeader {
                    number: format!("{}", 100 + n),
                    kind: crate::domain::InvoiceKind::Sale,
                    date_jalali: "1405/01/01".into(),
                    date_iso: "2026-03-21".into(),
                    paid: true,
                    url: format!("/f/{name}"),
                })
                .collect())
        }
        async fn invoice_lines(&self, url: &str) -> Result<Vec<crate::domain::InvoiceLine>> {
            let nth = {
                let mut asked = self.asked.lock().unwrap();
                asked.push(url.to_string());
                asked.iter().filter(|u| *u == url).count()
            };
            let parse = || BridgeError::SupplierParse {
                page: "factor".into(),
                field: "lines table".into(),
            };
            match (url, nth) {
                ("/f/once", 1) | ("/f/never", _) => Err(parse()),
                _ => Ok(vec![crate::domain::InvoiceLine {
                    line_no: 1,
                    code: "3137006".into(),
                    name: "لنت جلو".into(),
                    qty: 1,
                    unit: "دست".into(),
                    unit_price: 1,
                }]),
            }
        }
        async fn price_of(&self, _code: &str) -> Result<Option<crate::domain::PriceQuote>> {
            Err(BridgeError::Other("offline".into()))
        }
        async fn quotes_by_name(&self, _name: &str) -> Result<Vec<crate::domain::PriceQuote>> {
            Err(BridgeError::Other("offline".into()))
        }
    }

    fn flaky_services() -> Services {
        services(Box::new(FlakySupplier::default()))
    }

    fn services(supplier: Box<dyn crate::supplier::Supplier>) -> Services {
        Services {
            landau: crate::landau::LandauClient::new("http://127.0.0.1:1", "apt-test").unwrap(),
            supplier,
            settings: crate::settings::Settings {
                landau_url: "http://127.0.0.1:1".into(),
                landau_token: Some("apt-test".into()),
                store: Some("store1".into()),
                store_name: None,
                supplier_user: None,
                supplier_password: None,
                default_authenticity: Authenticity::Original,
                threshold: 85,
                vat_percent: 10.0,
                min_margin_percent: 0.0,
                overwrite_manual: false,
                fetch_since_only: true,
                auto_update: true,
            },
        }
    }

    /// The 2026-10-07 report: a page the site failed to serve once was reported as a layout change
    /// and left out of the run. Every item-level failure gets one more pass at the end of FETCH;
    /// only what fails twice reaches the report.
    fn job_ctx() -> JobCtx {
        let db = crate::db::Db::in_memory().unwrap();
        let job_id = db
            .create_job("IMPORT", Some("ZERO"), None, "store1", false)
            .unwrap();
        JobCtx {
            db,
            app: None,
            job_id,
            cancel: Default::default(),
        }
    }

    #[tokio::test]
    async fn fetch_retries_failed_invoices_once_before_reporting_them() {
        let ctx = job_ctx();
        let svc = flaky_services();
        fetch(&ctx, &svc, None, None).await.unwrap();

        let left: Vec<String> = ctx
            .db
            .unfetched_invoice_ids(SUPPLIER_ID, None)
            .unwrap()
            .into_iter()
            .map(|id| ctx.db.invoice_url(id).unwrap())
            .collect();
        assert_eq!(left, vec!["/f/never".to_string()]);

        let errors = ctx.db.list_errors(10).unwrap();
        assert_eq!(
            errors.len(),
            1,
            "only the invoice that failed twice is reported"
        );
        assert!(
            errors[0].message.contains("/f/never"),
            "{}",
            errors[0].message
        );
    }

    /// Serves `/f/<name>` invoices `parallel` at a time, each after `delay`, counting how many ran
    /// together. `once…` fails on its first fetch, `never…` on every one, `fatal` stops the job;
    /// `cancel_on` raises the job's cancel flag the first time that invoice is asked for.
    struct ParallelSupplier {
        names: Vec<String>,
        parallel: usize,
        delay: std::time::Duration,
        running: std::sync::atomic::AtomicUsize,
        max_running: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        asked: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        cancel_on: Option<(String, std::sync::Arc<std::sync::atomic::AtomicBool>)>,
    }

    impl ParallelSupplier {
        fn new(names: &[&str], parallel: usize) -> Self {
            ParallelSupplier {
                names: names.iter().map(|n| n.to_string()).collect(),
                parallel,
                delay: std::time::Duration::from_millis(20),
                running: Default::default(),
                max_running: Default::default(),
                asked: Default::default(),
                cancel_on: None,
            }
        }
    }

    #[async_trait::async_trait]
    impl crate::supplier::Supplier for ParallelSupplier {
        fn id(&self) -> &'static str {
            "parallel"
        }
        async fn login(&self) -> Result<()> {
            Ok(())
        }
        async fn ledger(
            &self,
            _from: Option<&str>,
            _progress: Progress,
        ) -> Result<Vec<crate::domain::InvoiceHeader>> {
            Ok(self
                .names
                .iter()
                .enumerate()
                .map(|(n, name)| crate::domain::InvoiceHeader {
                    number: format!("{}", 1000 + n),
                    kind: crate::domain::InvoiceKind::Sale,
                    date_jalali: "1405/01/01".into(),
                    date_iso: "2026-03-21".into(),
                    paid: true,
                    url: format!("/f/{name}"),
                })
                .collect())
        }
        async fn invoice_lines(&self, url: &str) -> Result<Vec<crate::domain::InvoiceLine>> {
            use std::sync::atomic::Ordering::SeqCst;
            let nth = {
                let mut asked = self.asked.lock().unwrap();
                asked.push(url.to_string());
                asked.iter().filter(|u| *u == url).count()
            };
            if let Some((on, flag)) = &self.cancel_on {
                if url == on {
                    flag.store(true, SeqCst);
                }
            }
            let now = self.running.fetch_add(1, SeqCst) + 1;
            self.max_running.fetch_max(now, SeqCst);
            tokio::time::sleep(self.delay).await;
            self.running.fetch_sub(1, SeqCst);
            let name = url.trim_start_matches("/f/");
            if name == "fatal" {
                return Err(BridgeError::SupplierUnreachable("connection reset".into()));
            }
            if name.starts_with("never") || (name.starts_with("once") && nth == 1) {
                return Err(BridgeError::SupplierFaulted { path: url.into() });
            }
            Ok(vec![crate::domain::InvoiceLine {
                line_no: 1,
                code: "3137006".into(),
                name: "لنت جلو".into(),
                qty: 1,
                unit: "دست".into(),
                unit_price: 1,
            }])
        }
        async fn price_of(&self, _code: &str) -> Result<Option<crate::domain::PriceQuote>> {
            Err(BridgeError::Other("offline".into()))
        }
        async fn quotes_by_name(&self, _name: &str) -> Result<Vec<crate::domain::PriceQuote>> {
            Err(BridgeError::Other("offline".into()))
        }
        fn parallelism(&self) -> usize {
            self.parallel
        }
    }

    fn unfetched(ctx: &JobCtx) -> Vec<String> {
        ctx.db
            .unfetched_invoice_ids(SUPPLIER_ID, None)
            .unwrap()
            .into_iter()
            .map(|id| ctx.db.invoice_url(id).unwrap())
            .collect()
    }

    /// The parallel fetch keeps LND-Q01's contract: per-invoice failures stay per invoice, get one
    /// more pass, and only what fails twice is reported — now with several invoices in flight.
    #[tokio::test]
    async fn fetch_runs_as_many_as_the_supplier_allows_and_keeps_the_second_pass() {
        let mut names: Vec<String> = (0..17).map(|n| format!("ok{n}")).collect();
        names.extend(["once1", "once2", "never1"].map(String::from));
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let supplier = ParallelSupplier::new(&names, 4);
        let (max_running, asked) = (supplier.max_running.clone(), supplier.asked.clone());
        let ctx = job_ctx();
        let started = std::time::Instant::now();
        fetch(&ctx, &services(Box::new(supplier)), None, None)
            .await
            .unwrap();
        let took = started.elapsed();

        assert_eq!(
            max_running.load(std::sync::atomic::Ordering::SeqCst),
            4,
            "never more than the supplier's parallelism, and all of it used"
        );
        assert_eq!(unfetched(&ctx), vec!["/f/never1".to_string()]);
        let errors = ctx.db.list_errors(10).unwrap();
        assert_eq!(
            errors.len(),
            1,
            "only the invoice that failed twice is reported"
        );
        assert!(errors[0].message.contains("/f/never1"));
        assert_eq!(
            asked.lock().unwrap().len(),
            20 + 3,
            "the three failures asked once more"
        );
        // 23 fetches of 20 ms, four at a time: ~0.12 s; one at a time would take 0.46 s.
        assert!(took < std::time::Duration::from_millis(350), "{took:?}");
    }

    /// As the one-at-a-time loop let its current invoice land, a cancel lets every invoice in
    /// flight land — and starts nothing new.
    #[tokio::test]
    async fn a_cancel_lets_what_is_in_flight_land_and_starts_nothing_new() {
        let names: Vec<String> = (0..10).map(|n| format!("ok{n}")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let ctx = job_ctx();
        let mut supplier = ParallelSupplier::new(&names, 3);
        supplier.cancel_on = Some(("/f/ok1".into(), ctx.cancel.clone()));
        let asked = supplier.asked.clone();
        let result = fetch(&ctx, &services(Box::new(supplier)), None, None).await;
        assert_eq!(result, Err(BridgeError::JobCancelled));
        assert_eq!(
            asked.lock().unwrap().len(),
            3,
            "only the first three were started"
        );
        assert_eq!(unfetched(&ctx).len(), 10 - 3, "and all three were stored");
    }

    #[tokio::test]
    async fn a_fatal_failure_still_stops_the_fetch() {
        let ctx = job_ctx();
        let supplier = ParallelSupplier::new(&["ok0", "fatal", "ok1", "ok2", "ok3", "ok4"], 2);
        let result = fetch(&ctx, &services(Box::new(supplier)), None, None).await;
        assert!(
            matches!(result, Err(BridgeError::SupplierUnreachable(_))),
            "{result:?}"
        );
        assert!(
            ctx.db.list_errors(10).unwrap().is_empty(),
            "a stop, not a report"
        );
    }

    #[test]
    fn merge_row_replaces_by_id_or_appends_and_keeps_the_rest() {
        let mut shelf = vec![row("a", 1), row("b", 2)];
        merge_row(&mut shelf, row("b", 7));
        merge_row(&mut shelf, row("c", 3));
        assert_eq!(shelf.len(), 3);
        assert_eq!(shelf[0].inventory, 1, "untouched sibling keeps its figure");
        assert_eq!(shelf[1].inventory, 7, "same id replaced in place");
        assert_eq!(shelf[2].id, "c");
    }
}
