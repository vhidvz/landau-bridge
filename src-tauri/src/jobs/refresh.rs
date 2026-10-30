//! Feature 2 (spec §7.2, §14): every shelf row gets today's cost by its stored link, and Landau's
//! rule prices each write. A row with no link is reported, never searched for.

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
    /// `(name, code, quote)`
    Quoted(String, String, Costed),
    /// The link answered no price: `(name, code)`.
    NoPrice(String, String),
    /// No stored link.
    NoLink,
}

/// The stored link's quote. Transport/session failures stop the job; a per-item failure surfaces
/// as the error.
async fn resolve(
    ctx: &JobCtx,
    svc: &Services,
    row: &ShelfRow,
    name: &str,
    links: &HashMap<String, (MatchRow, usize)>,
    vat: &mut VatMismatches,
) -> Result<Resolved> {
    let Some((link, n)) = links.get(&row.product) else {
        return Ok(Resolved::NoLink);
    };
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
    Ok(
        match quote_for(ctx, svc, &link.name_seen, &link.code, vat).await? {
            Some(q) => Resolved::Quoted(link.name_seen.clone(), link.code.clone(), q),
            None => Resolved::NoPrice(link.name_seen.clone(), link.code.clone()),
        },
    )
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
        let (link_name, link_code, quoted) = match resolved {
            Resolved::Quoted(name, code, q) => (name, code, Some(q)),
            Resolved::NoPrice(name, code) => (name, code, None),
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
                            t("این ردیف به هیچ نام بارزپخش وصل نیست؛ وقتی این قطعه در فاکتوری از بارزپخش وارد شود، وصل می‌شود."),
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

    /// Counts every price lookup; a name search answers `quote` (none by default).
    #[derive(Default)]
    struct CountingSupplier {
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        quote: Option<crate::domain::PriceQuote>,
    }

    #[async_trait::async_trait]
    impl crate::supplier::Supplier for CountingSupplier {
        fn id(&self) -> &'static str {
            "counting"
        }
        async fn login(&self) -> Result<()> {
            Ok(())
        }
        async fn ledger(
            &self,
            _from: Option<&str>,
            _progress: crate::supplier::Progress,
        ) -> Result<Vec<crate::domain::InvoiceHeader>> {
            Ok(vec![])
        }
        async fn invoice_lines(&self, _url: &str) -> Result<Vec<crate::domain::InvoiceLine>> {
            Ok(vec![])
        }
        async fn price_of(&self, _code: &str) -> Result<Option<crate::domain::PriceQuote>> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(None)
        }
        async fn quotes_by_name(&self, _name: &str) -> Result<Vec<crate::domain::PriceQuote>> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(self.quote.clone().into_iter().collect())
        }
    }

    fn fixture(supplier: CountingSupplier) -> (JobCtx, Services) {
        let db = crate::db::Db::in_memory().unwrap();
        let job_id = db
            .create_job("REFRESH", None, None, "store1", false)
            .unwrap();
        let ctx = JobCtx {
            db,
            app: None,
            job_id,
            cancel: Default::default(),
        };
        let svc = Services {
            landau: crate::landau::LandauClient::new("http://127.0.0.1:1", "apt-test").unwrap(),
            supplier: Box::new(supplier),
            settings: crate::settings::Settings {
                landau_url: "http://127.0.0.1:1".into(),
                landau_token: Some("apt-test".into()),
                store: Some("store1".into()),
                store_name: None,
                supplier_user: None,
                supplier_password: None,
                default_authenticity: crate::domain::Authenticity::Original,
                threshold: 85,
                vat_percent: 10.0,
                min_margin_percent: 15.0,
                overwrite_manual: false,
                fetch_since_only: true,
            },
        };
        (ctx, svc)
    }

    fn shelf_row(product: &str) -> ShelfRow {
        ShelfRow {
            id: "s1".into(),
            product: product.into(),
            product_name: Some("آرمیچر استارت پراید".into()),
            brand: Some("کروز".into()),
            store: Some("store1".into()),
            inventory: 2,
            price: None,
            cost: None,
            condition: None,
            authenticity: None,
            price_source: None,
        }
    }

    #[tokio::test]
    async fn an_unlinked_row_is_skipped_without_searching() {
        let supplier = CountingSupplier::default();
        let calls = supplier.calls.clone();
        let (ctx, svc) = fixture(supplier);
        let resolved = resolve(
            &ctx,
            &svc,
            &shelf_row("p-unlinked"),
            "آرمیچر استارت پراید",
            &HashMap::new(),
            &mut VatMismatches::default(),
        )
        .await
        .unwrap();
        assert!(matches!(resolved, Resolved::NoLink));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    /// Characterises the stored-link path the map removal had to keep: the chosen link's name
    /// and code price the row, and a product with several links is warned about.
    #[tokio::test]
    async fn a_linked_row_is_priced_by_its_stored_link() {
        let supplier = CountingSupplier {
            quote: Some(crate::domain::PriceQuote {
                code: "C9".into(),
                name: "لنت جلو پراید".into(),
                list_price: 100_000,
                vat_percent: Some(10.0),
                has_vat: true,
                available: true,
                unit: "عدد".into(),
            }),
            ..Default::default()
        };
        let (ctx, svc) = fixture(supplier);
        let mut links = HashMap::new();
        links.insert(
            "p-linked".to_string(),
            (link("لنت جلو پراید", "C9", "p-linked", None), 2),
        );
        let resolved = resolve(
            &ctx,
            &svc,
            &shelf_row("p-linked"),
            "لنت جلو",
            &links,
            &mut VatMismatches::default(),
        )
        .await
        .unwrap();
        let Resolved::Quoted(name, code, costed) = resolved else {
            panic!("expected a quote");
        };
        assert_eq!((name.as_str(), code.as_str()), ("لنت جلو پراید", "C9"));
        assert_eq!(costed.cost, 110_000);
        let events = ctx.db.job_detail(ctx.job_id).unwrap().events;
        assert!(
            events
                .iter()
                .any(|e| e.level == "WARN" && e.message.contains("به 2 نام")),
            "{events:?}"
        );
    }
}
