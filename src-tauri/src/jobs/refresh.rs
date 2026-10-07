//! Feature 2 (spec §7.2, §14): every shelf row gets today's cost — by its stored link, else
//! through the landau-parts name map — and Landau's rule prices each write.

use std::collections::HashMap;

use serde_json::json;

use super::pricing;
use super::{
    fatal, i, opt_i, preflight, quote_for, report, reprice_items, t, Costed, JobCtx, Services,
    VatMismatches,
};
use crate::db::matches::MatchRow;
use crate::errors::{kinds, BridgeError, Result};
use crate::landau::{PatchBody, ShelfRow};
use crate::settings::SUPPLIER_ID;

/// product_id → the link to price it by, and how many links the product had. Several links for
/// one product: the most recently priced wins, then the most recently invoiced code (§14 c).
pub fn choose_link(
    rows: Vec<MatchRow>,
    last_invoiced: &HashMap<String, String>,
) -> HashMap<String, (MatchRow, usize)> {
    let mut by_product: HashMap<String, Vec<MatchRow>> = HashMap::new();
    for r in rows {
        if let Some(p) = r.product_id.clone() {
            by_product.entry(p).or_default().push(r);
        }
    }
    by_product
        .into_iter()
        .map(|(product, mut links)| {
            let n = links.len();
            links.sort_by(|a, b| {
                b.priced_at
                    .cmp(&a.priced_at)
                    .then_with(|| last_invoiced.get(&b.code).cmp(&last_invoiced.get(&a.code)))
            });
            (product, (links.swap_remove(0), n))
        })
        .collect()
}

fn links(ctx: &JobCtx) -> Result<HashMap<String, (MatchRow, usize)>> {
    let rows = ctx.db.linked_matches(SUPPLIER_ID)?;
    let lines = ctx.db.lines_with_invoices(SUPPLIER_ID)?;
    let last_invoiced: HashMap<String, String> = lines
        .iter()
        .map(|l| (l.code.clone(), l.date_jalali.clone()))
        .collect(); // oldest first → last write wins
    Ok(choose_link(rows, &last_invoiced))
}

/// How one shelf row reached (or failed to reach) a supplier quote.
enum Resolved {
    /// `(name, code, quote, from_map)`
    Quoted(String, String, Costed, bool),
    /// The link or every map name answered no price: `(name, code)`.
    NoPrice(String, String),
    /// Neither a link nor a map entry.
    NoLink,
}

/// A stored link first; else the landau-parts map (§14 c), trying its raw names available-first
/// until one prices, and recording that name as a `MAP` link. Transport/session failures stop the
/// job; a per-item failure surfaces as the error.
async fn resolve(
    ctx: &JobCtx,
    svc: &Services,
    row: &ShelfRow,
    name: &str,
    links: &HashMap<String, (MatchRow, usize)>,
    vat: &mut VatMismatches,
) -> Result<Resolved> {
    if let Some((link, n)) = links.get(&row.product) {
        if *n > 1 {
            ctx.warn(
                "PRICE",
                kinds::AMBIGUOUS_SUPPLIER_LINK,
                &format!(
                    "قطعه {name} به {n} نام بارزپخش وصل است؛ «{}» (آخرین قیمت) ملاک شد.",
                    link.name_seen
                ),
                Some(json!({"product": row.product, "chosen": link.name_seen})),
            );
        }
        return Ok(
            match quote_for(ctx, svc, &link.name_seen, &link.code, vat).await? {
                Some(q) => Resolved::Quoted(link.name_seen.clone(), link.code.clone(), q, false),
                None => Resolved::NoPrice(link.name_seen.clone(), link.code.clone()),
            },
        );
    }
    let entries = ctx
        .db
        .raw_names_for(name, row.brand.as_deref().unwrap_or(""))?;
    if entries.is_empty() {
        return Ok(Resolved::NoLink);
    }
    for e in &entries {
        match quote_for(ctx, svc, &e.raw_name, &e.code, vat).await {
            Ok(Some(q)) => {
                ctx.db.set_match(
                    SUPPLIER_ID,
                    &e.raw_name,
                    &e.code,
                    Some((&row.product, name)),
                    "MAP",
                    None,
                )?;
                return Ok(Resolved::Quoted(
                    e.raw_name.clone(),
                    e.code.clone(),
                    q,
                    true,
                ));
            }
            Ok(None) => continue,
            Err(err) if fatal(&err) => return Err(err),
            Err(err) => {
                tracing::warn!(name = %e.raw_name, error = %err, "map name failed; trying the next");
                continue;
            }
        }
    }
    let first = &entries[0];
    Ok(Resolved::NoPrice(
        first.raw_name.clone(),
        first.code.clone(),
    ))
}

pub async fn run(ctx: &JobCtx, svc: &Services) -> Result<()> {
    let (_, _, store) = svc.settings.require_landau()?;
    let store = store.to_string();
    preflight(ctx, svc, &store).await?;

    ctx.phase("PRICE")?;
    let shelf = svc.landau.shelf(&store).await?;
    let links = links(ctx)?;
    let total = shelf.len() as u32;
    // item, stock, cost, unchanged, current price
    let mut to_write: Vec<(i64, String, i64, bool, Option<i64>)> = Vec::new();
    let mut vat = VatMismatches::default();
    for (n, row) in shelf.iter().enumerate() {
        ctx.check_cancel()?;
        let name = row
            .product_name
            .clone()
            .unwrap_or_else(|| row.product.clone());
        let resolved = match resolve(ctx, svc, row, &name, &links, &mut vat).await {
            Ok(r) => r,
            Err(e) if fatal(&e) => return Err(e),
            Err(e) => {
                let id = ctx.db.add_job_item(ctx.job_id, "", &name, row.inventory)?;
                ctx.error("PRICE", &e, Some(json!({"item_id": id})));
                ctx.item(
                    id,
                    &[
                        ("stock_id", t(&row.id)),
                        ("product_id", t(&row.product)),
                        ("price_before", opt_i(row.price)),
                        ("qty_before", i(row.inventory)),
                        ("status", t("FAILED")),
                        ("error_kind", t(e.kind())),
                        ("error_msg", t(&e.message_fa())),
                    ],
                )?;
                ctx.progress("PRICE", n as u32 + 1, total, "قیمت روز بارزپخش");
                continue;
            }
        };
        let (link_name, link_code, quoted, from_map) = match resolved {
            Resolved::Quoted(name, code, q, from_map) => (name, code, Some(q), from_map),
            Resolved::NoPrice(name, code) => (name, code, None, false),
            Resolved::NoLink => {
                let id = ctx.db.add_job_item(ctx.job_id, "", &name, row.inventory)?;
                ctx.item(
                    id,
                    &[
                        ("stock_id", t(&row.id)),
                        ("product_id", t(&row.product)),
                        ("price_before", opt_i(row.price)),
                        ("qty_before", i(row.inventory)),
                        ("action", t("SKIP")),
                        ("status", t("SKIPPED")),
                        ("error_kind", t(kinds::NO_SUPPLIER_LINK)),
                        (
                            "error_msg",
                            t("این ردیف به هیچ نام بارزپخش وصل نیست و در نگاشت نام‌ها هم نبود."),
                        ),
                    ],
                )?;
                ctx.progress("PRICE", n as u32 + 1, total, "قیمت روز بارزپخش");
                continue;
            }
        };
        let id = ctx
            .db
            .add_job_item(ctx.job_id, &link_code, &link_name, row.inventory)?;
        ctx.item(
            id,
            &[
                ("stock_id", t(&row.id)),
                ("product_id", t(&row.product)),
                ("product_name", t(&name)),
                ("price_before", opt_i(row.price)),
                ("qty_before", i(row.inventory)),
            ],
        )?;
        if from_map {
            ctx.db.add_item_note(id, "از نگاشت نام‌ها وصل شد")?;
        }
        match quoted {
            Some(c) => {
                // §14 d: an unavailable quote never lowers a row's cost.
                let (cost, held) = pricing::effective_cost(c.cost, c.available, row.cost);
                if held {
                    ctx.db
                        .add_item_note(id, "ناموجود؛ قیمت پایین‌تر نادیده گرفته شد")?;
                } else if !c.available {
                    ctx.db.add_item_note(id, "ناموجود در بارزپخش")?;
                }
                if c.renamed {
                    ctx.db
                        .add_item_note(id, &format!("نام در بارزپخش عوض شد: «{}»", c.name))?;
                }
                if !link_code.is_empty() && c.code != link_code {
                    ctx.db.add_item_note(
                        id,
                        &format!("کد بارزپخش عوض شد: {link_code} ← {}", c.code),
                    )?;
                }
                ctx.item(
                    id,
                    &[("cost", i(cost)), ("available", i(c.available as i64))],
                )?;
                // still repriced below (a rule may have changed even when the cost has not).
                to_write.push((id, row.id.clone(), cost, row.cost == Some(cost), row.price));
            }
            None => {
                let e = BridgeError::NoCurrentPrice {
                    code: if link_code.is_empty() {
                        link_name.clone()
                    } else {
                        link_code.clone()
                    },
                };
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
        }
        ctx.progress("PRICE", n as u32 + 1, total, "قیمت روز بارزپخش");
    }
    vat.warn(ctx, "PRICE", svc.settings.vat_percent);

    ctx.phase("WRITE")?;
    let mut written: Vec<(i64, String)> = Vec::new();
    let total = to_write.len() as u32;
    for (n, (item_id, stock_id, cost, unchanged, current_price)) in to_write.iter().enumerate() {
        ctx.check_cancel()?;
        let ok = if *unchanged {
            ctx.item(
                *item_id,
                &[
                    ("action", t("SKIP")),
                    ("price_after", opt_i(*current_price)),
                    ("status", t("WRITTEN")),
                ],
            )?;
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
                Ok(reply) => {
                    ctx.item(
                        *item_id,
                        &[
                            ("action", t("PATCH")),
                            ("price_after", opt_i(reply.price)),
                            ("status", t("WRITTEN")),
                        ],
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

    reprice_items(ctx, svc, &written).await?;
    report(ctx, svc, &written)?;

    let all = ctx.db.job_items(ctx.job_id)?;
    let summary = json!({
        "rows": all.len(),
        "linked": all.iter().filter(|i| i.error_kind.as_deref() != Some(kinds::NO_SUPPLIER_LINK)).count(),
        "from_map": all.iter().filter(|i| i.note.as_deref().map(|n| n.contains("از نگاشت")).unwrap_or(false)).count(),
        "patched": all.iter().filter(|i| i.action.as_deref() == Some("PATCH")).count(),
        "unchanged": all.iter().filter(|i| i.action.as_deref() == Some("SKIP") && i.status == "DONE").count(),
        "repriced": all.iter().filter(|i| i.reprice_verdict.as_deref().map(|v| v.starts_with("REPRICED:")).unwrap_or(false)).count(),
        "skipped": all.iter().filter(|i| i.status == "SKIPPED").count(),
        "failed": all.iter().filter(|i| i.status == "FAILED").count(),
    });
    ctx.db.set_job_summary(ctx.job_id, &summary)?;
    ctx.info("REPORT", &summary.to_string(), None);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::matches::MatchRow;

    fn link(name: &str, code: &str, product: &str, priced_at: Option<&str>) -> MatchRow {
        MatchRow {
            supplier: "s".into(),
            name_key: name.into(),
            code: code.into(),
            product_id: Some(product.into()),
            product_name: Some("p".into()),
            how: "AUTO".into(),
            similarity: None,
            name_seen: name.into(),
            priced_at: priced_at.map(str::to_string),
            decided_at: "2026-09-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn the_most_recently_priced_link_wins_then_the_most_recently_invoiced() {
        let rows = vec![
            link("قدیمی", "1", "p1", Some("2026-09-01T00:00:00Z")),
            link("تازه", "2", "p1", Some("2026-09-15T00:00:00Z")),
            link("الف", "3", "p2", None),
            link("ب", "4", "p2", None),
        ];
        let mut invoiced = HashMap::new();
        invoiced.insert("3".to_string(), "1404/01/01".to_string());
        invoiced.insert("4".to_string(), "1405/01/01".to_string());
        let chosen = choose_link(rows, &invoiced);
        assert_eq!(
            (chosen["p1"].0.name_seen.as_str(), chosen["p1"].1),
            ("تازه", 2)
        );
        assert_eq!(
            (chosen["p2"].0.name_seen.as_str(), chosen["p2"].1),
            ("ب", 2)
        );
    }
}
