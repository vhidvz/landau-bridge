//! Feature 2 (spec §7.2): every shelf row with a supplier link gets today's cost, then Landau's
//! rules run over the store.

use std::collections::HashMap;

use serde_json::json;

use super::{cost_for, fatal, i, opt_i, preflight, readback, reprice_items, t, JobCtx, Services};
use crate::errors::{kinds, BridgeError, Result};
use crate::landau::PatchBody;
use crate::settings::SUPPLIER_ID;

/// product_id → the code to price it by. Several codes for one product: the most recently
/// invoiced one, and a warning.
fn links(ctx: &JobCtx) -> Result<HashMap<String, String>> {
    let rows = ctx.db.linked_matches(SUPPLIER_ID)?;
    let lines = ctx.db.lines_with_invoices(SUPPLIER_ID)?;
    let last_seen: HashMap<&str, &str> = lines
        .iter()
        .map(|l| (l.code.as_str(), l.date_jalali.as_str()))
        .collect(); // oldest first → last write wins
    let mut by_product: HashMap<String, Vec<String>> = HashMap::new();
    for r in &rows {
        if let Some(p) = &r.product_id {
            by_product
                .entry(p.clone())
                .or_default()
                .push(r.code.clone());
        }
    }
    let mut out = HashMap::new();
    for (product, mut codes) in by_product {
        if codes.len() > 1 {
            codes.sort_by(|a, b| last_seen.get(b.as_str()).cmp(&last_seen.get(a.as_str())));
            ctx.warn(
                "PRICE",
                kinds::AMBIGUOUS_SUPPLIER_LINK,
                &format!(
                    "قطعه {product} به {} کد بارزپخش وصل است؛ {} (آخرین خرید) ملاک شد.",
                    codes.len(),
                    codes[0]
                ),
                Some(json!({"product": product, "codes": codes})),
            );
        }
        out.insert(product, codes[0].clone());
    }
    Ok(out)
}

pub async fn run(ctx: &JobCtx, svc: &Services) -> Result<()> {
    let (_, _, store) = svc.settings.require_landau()?;
    let store = store.to_string();
    preflight(ctx, svc, &store).await?;

    ctx.phase("PRICE")?;
    let shelf = svc.landau.shelf(&store).await?;
    let links = links(ctx)?;
    let total = shelf.len() as u32;
    let mut to_write: Vec<(i64, String, i64, bool)> = Vec::new(); // item, stock, cost, unchanged
    for (n, row) in shelf.iter().enumerate() {
        ctx.check_cancel()?;
        let name = row
            .product_name
            .clone()
            .unwrap_or_else(|| row.product.clone());
        let Some(code) = links.get(&row.product) else {
            let id = ctx.db.add_job_item(ctx.job_id, "", &name, row.inventory)?;
            ctx.item(
                id,
                &[
                    ("stock_id", t(&row.id)),
                    ("product_id", t(&row.product)),
                    ("price_before", opt_i(row.price)),
                    ("action", t("SKIP")),
                    ("status", t("SKIPPED")),
                    ("error_kind", t(kinds::NO_SUPPLIER_LINK)),
                    ("error_msg", t("این ردیف به هیچ کد بارزپخش وصل نیست.")),
                ],
            )?;
            ctx.progress("PRICE", n as u32 + 1, total, "قیمت روز بارزپخش");
            continue;
        };
        let id = ctx
            .db
            .add_job_item(ctx.job_id, code, &name, row.inventory)?;
        ctx.item(
            id,
            &[
                ("stock_id", t(&row.id)),
                ("product_id", t(&row.product)),
                ("price_before", opt_i(row.price)),
                ("qty_before", i(row.inventory)),
            ],
        )?;
        match cost_for(ctx, svc, "PRICE", code).await {
            Ok(Some(cost)) => {
                ctx.item(id, &[("cost", i(cost))])?;
                // still repriced below (a rule may have changed even when the cost has not).
                to_write.push((id, row.id.clone(), cost, row.cost == Some(cost)));
            }
            Ok(None) => {
                let e = BridgeError::NoCurrentPrice { code: code.clone() };
                ctx.error("PRICE", &e, Some(json!({"item_id": id})));
                ctx.item(
                    id,
                    &[
                        ("status", t("SKIPPED")),
                        ("error_kind", t(e.kind())),
                        ("error_msg", t(&e.message_fa())),
                    ],
                )?;
            }
            Err(e) if fatal(&e) => return Err(e),
            Err(e) => {
                ctx.error("PRICE", &e, Some(json!({"item_id": id})));
                ctx.item(
                    id,
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
    let mut written: Vec<(i64, String)> = Vec::new();
    let total = to_write.len() as u32;
    for (n, (item_id, stock_id, cost, unchanged)) in to_write.iter().enumerate() {
        ctx.check_cancel()?;
        let ok = if *unchanged {
            ctx.item(*item_id, &[("action", t("SKIP")), ("status", t("WRITTEN"))])?;
            true
        } else {
            match svc
                .landau
                .patch(
                    stock_id,
                    &PatchBody {
                        inventory: None,
                        cost: Some(*cost),
                    },
                )
                .await
            {
                Ok(()) => {
                    ctx.item(
                        *item_id,
                        &[("action", t("PATCH")), ("status", t("WRITTEN"))],
                    )?;
                    true
                }
                Err(e) if fatal(&e) => return Err(e),
                Err(e) => {
                    ctx.error("WRITE", &e, Some(json!({"item_id": item_id})));
                    ctx.item(
                        *item_id,
                        &[
                            ("status", t("FAILED")),
                            ("error_kind", t(e.kind())),
                            ("error_msg", t(&e.message_fa())),
                        ],
                    )?;
                    false
                }
            }
        };
        if ok {
            written.push((*item_id, stock_id.clone()));
        }
        ctx.progress(
            "WRITE",
            n as u32 + 1,
            total,
            "به‌روزرسانی قیمت خرید روی قفسه",
        );
    }

    reprice_items(ctx, svc, &store, &written).await?;
    readback(ctx, svc, &store, &written).await?;

    let all = ctx.db.job_items(ctx.job_id)?;
    let summary = json!({
        "rows": all.len(),
        "linked": all.iter().filter(|i| !i.code.is_empty()).count(),
        "patched": all.iter().filter(|i| i.action.as_deref() == Some("PATCH")).count(),
        "unchanged": all.iter().filter(|i| i.action.as_deref() == Some("SKIP") && i.status == "WRITTEN").count(),
        "repriced": all.iter().filter(|i| i.reprice_verdict.as_deref().map(|v| v.starts_with("REPRICED:")).unwrap_or(false)).count(),
        "skipped": all.iter().filter(|i| i.status == "SKIPPED").count(),
        "failed": all.iter().filter(|i| i.status == "FAILED").count(),
    });
    ctx.db.set_job_summary(ctx.job_id, &summary)?;
    ctx.info("REPORT", &summary.to_string(), None);
    Ok(())
}
