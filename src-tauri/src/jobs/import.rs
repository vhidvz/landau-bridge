//! Feature 1 (spec §7.1): preflight → fetch → aggregate → match → [REVIEW] → price → write →
//! reprice → report. `start` runs up to the review pause; `commit` runs the rest.

use std::collections::HashSet;

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
            Ok(Some(c)) => ctx.item(item.id, &[("cost", i(c.cost))])?,
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
        let cost = item.cost.unwrap_or(0);
        let authenticity = item
            .authenticity
            .as_deref()
            .and_then(Authenticity::parse)
            .unwrap_or(svc.settings.default_authenticity);
        let existing = find_row(&shelf, &product, authenticity.as_str());
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
                                    .map(|_| Written {
                                        stock_id: row.id.clone(),
                                        qty_before: Some(row.inventory),
                                        qty_after: inv.or(Some(row.inventory)),
                                        price_before: row.price,
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
                                            .map(|_| Written {
                                                stock_id: removed.id.clone(),
                                                qty_before: Some(removed.inventory),
                                                qty_after: Some(inv),
                                                price_before: removed.price,
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
                    .map(|_| Written {
                        stock_id: stock_id.clone(),
                        qty_before: Some(*qty_before),
                        qty_after: inventory.or(Some(*qty_before)),
                        price_before,
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
