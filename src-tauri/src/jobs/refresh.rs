//! Feature 2 (spec §7.2, §14; plan 130 tasks 3, 7, 8): every shelf row gets today's cost from the
//! links on its product — each one validated by this job against the registry read whole, a dead
//! product's link rebuilt only on its exact definition — and only when every link on the row is
//! proven and they all agree on the cost (Q-3, R-e). A row with no link is reported, never searched
//! for; a held row keeps its cost. Writes go through [`ShelfWriter`] alone.

use std::collections::HashMap;

use serde_json::json;

use super::proof::{prove_refresh, Judgement, Planned, ProvenWrite, Prover};
use super::{
    fatal, held, i, opt_i, preflight, quote_for, read_registry, report, reprice_items, t, JobCtx,
    Services, VatMismatches,
};
use crate::db::matches::MatchRow;
use crate::errors::{kinds, BridgeError, Result, IDENTITY_KINDS};
use crate::landau::writer::{Refreshed, ShelfWriter};
use crate::landau::ShelfRow;
use crate::matching::registry::Registry;
use crate::settings::SUPPLIER_ID;

/// product id → its links, each with whether this job proved it.
pub type LinksByProduct = HashMap<String, Vec<(MatchRow, bool)>>;

/// VALIDATE over every stored link with a product (R-b): judged by the write table for its origin,
/// rebuilt when its product is gone (Q-1). A link left pointing at nothing current is not listed.
pub async fn validate_links(
    ctx: &JobCtx,
    registry: &Registry,
    threshold: u8,
) -> Result<LinksByProduct> {
    let prover = Prover {
        db: &ctx.db,
        registry,
        supplier: SUPPLIER_ID,
        job: ctx.job_id,
        threshold,
    };
    let links = ctx.db.linked_matches(SUPPLIER_ID)?;
    let total = links.len() as u32;
    let mut out: LinksByProduct = HashMap::new();
    for (n, link) in links.iter().enumerate() {
        ctx.check_cancel()?;
        let judged = prover.stored(link, &link.name_seen, &link.code).await?;
        if let Some(now) = ctx.db.link_by_key(SUPPLIER_ID, &link.name_key)? {
            match judged {
                Judgement::Proven { product, .. } => {
                    out.entry(product.id).or_default().push((now, true))
                }
                Judgement::Held {
                    link_product: Some(p),
                    ..
                } => out.entry(p).or_default().push((now, false)),
                _ => {}
            }
        }
        ctx.progress("PRICE", n as u32 + 1, total, "سنجش پیوندها با فهرست لاندا");
    }
    for links in out.values_mut() {
        links.sort_by(|a, b| a.0.name_key.cmp(&b.0.name_key));
    }
    Ok(out)
}

/// The identity failure a stored refusal stands for, from the reasons it recorded.
fn refusal(link: &MatchRow) -> BridgeError {
    let reasons = link.state_reason.clone().unwrap_or_else(|| "[]".into());
    let conflict = ["NUMBER_CONFLICT", "OPPOSITE_WORDS", "BRAND_MISMATCH"]
        .iter()
        .any(|k| reasons.contains(k));
    if conflict {
        BridgeError::EvidenceConflict { reasons }
    } else {
        BridgeError::NeedsReview { reasons }
    }
}

/// One shelf row priced from its links: the proven write, or `None` once its item is recorded as
/// skipped or held.
pub async fn price_row(
    ctx: &JobCtx,
    svc: &Services,
    row: &ShelfRow,
    registry: &Registry,
    links: &LinksByProduct,
    vat: &mut VatMismatches,
) -> Result<Option<(i64, ProvenWrite, bool)>> {
    let name = row
        .product_name
        .clone()
        .unwrap_or_else(|| row.product.clone());
    let links = links.get(&row.product).map(Vec::as_slice).unwrap_or(&[]);
    let Some((lead, _)) = links.first() else {
        let id = ctx.db.add_job_item(ctx.job_id, "", &name, row.inventory)?;
        let gone = registry.get(&row.product).is_none();
        let (kind, msg) = if gone {
            (
                "StaleLink",
                "قطعهٔ این ردیف دیگر در فهرست لاندا نیست؛ قیمت آن به‌روز نمی‌شود.",
            )
        } else {
            (
                kinds::NO_SUPPLIER_LINK,
                "این ردیف به هیچ نام بارزپخش وصل نیست؛ وقتی این قطعه در فاکتوری از بارزپخش وارد شود، وصل می‌شود.",
            )
        };
        ctx.item(
            id,
            &[
                ("stock_id", t(&row.id)),
                ("product_id", t(&row.product)),
                ("price_before", opt_i(row.price)),
                ("qty_before", i(row.inventory)),
                ("action", t("SKIP")),
                ("status", t("SKIPPED")),
                ("error_kind", t(kind)),
                ("error_msg", t(msg)),
            ],
        )?;
        return Ok(None);
    };
    let id = ctx
        .db
        .add_job_item(ctx.job_id, &lead.code, &lead.name_seen, row.inventory)?;
    ctx.item(
        id,
        &[
            ("stock_id", t(&row.id)),
            ("product_id", t(&row.product)),
            ("product_name", t(&name)),
            ("price_before", opt_i(row.price)),
            ("qty_before", i(row.inventory)),
            ("link_key", t(&lead.name_key)),
            ("link_version", i(lead.link_version)),
        ],
    )?;
    if links.len() > 1 {
        let names: Vec<&str> = links.iter().map(|(l, _)| l.name_seen.as_str()).collect();
        ctx.db.add_item_note(
            id,
            &format!(
                "این ردیف به {} نام بارزپخش وصل است: {}",
                links.len(),
                names.join(" | ")
            ),
        )?;
    }
    if let Some((unproven, _)) = links.iter().find(|(_, ok)| !ok) {
        held(ctx, &[id], &refusal(unproven))?;
        return Ok(None);
    }
    let mut priced: Vec<(MatchRow, Option<i64>)> = Vec::new();
    let mut available = true;
    for (link, _) in links {
        // a failed quote fails this row's own item — never a second item beside it
        let quoted = match quote_for(ctx, svc, &link.name_seen, &link.code, vat).await {
            Ok(q) => q,
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
                return Ok(None);
            }
        };
        match quoted {
            Some(c) => {
                if c.renamed {
                    ctx.db
                        .add_item_note(id, &format!("نام در بارزپخش عوض شد: «{}»", c.name))?;
                }
                if !link.code.is_empty() && c.code != link.code {
                    ctx.db.add_item_note(
                        id,
                        &format!("کد بارزپخش عوض شد: {} ← {}", link.code, c.code),
                    )?;
                }
                available &= c.available;
                priced.push((link.clone(), Some(c.cost)));
            }
            None => priced.push((link.clone(), None)),
        }
    }
    if let [(link, None)] = priced.as_slice() {
        let e = BridgeError::NoCurrentPrice {
            code: if link.code.is_empty() {
                link.name_seen.clone()
            } else {
                link.code.clone()
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
        return Ok(None);
    }
    match prove_refresh(
        &ctx.db,
        SUPPLIER_ID,
        ctx.job_id,
        id,
        &row.id,
        &row.product,
        &priced,
    )? {
        Planned::Write(w) => {
            ctx.item(id, &[("available", i(available as i64))])?;
            Ok(Some((id, w, available)))
        }
        Planned::Held { items, error } => {
            if matches!(error, BridgeError::LinkConflict { .. }) {
                let each: Vec<String> = priced
                    .iter()
                    .map(|(l, c)| match c {
                        Some(c) => format!("«{}» {c} ریال", l.name_seen),
                        None => format!("«{}» بی‌قیمت", l.name_seen),
                    })
                    .collect();
                ctx.db.add_item_note(id, &each.join(" | "))?;
            }
            held(ctx, &items, &error)?;
            Ok(None)
        }
    }
}

pub async fn run(ctx: &JobCtx, svc: &Services) -> Result<()> {
    let (_, _, store) = svc.settings.require_landau()?;
    let store = store.to_string();
    preflight(ctx, svc, &store).await?;
    // R-o: settle every earlier write whose outcome is unknown before any row is planned
    super::reconcile::reconcile(ctx, &svc.landau, &store).await?;

    ctx.phase("PRICE")?;
    let registry = read_registry(ctx, svc, "PRICE").await?;
    let shelf = svc.landau.shelf(&store).await?;
    let links = validate_links(ctx, &registry, svc.settings.threshold).await?;
    let total = shelf.len() as u32;
    let mut to_write: Vec<(i64, ProvenWrite, bool, &ShelfRow)> = Vec::new();
    let mut vat = VatMismatches::default();
    for (n, row) in shelf.iter().enumerate() {
        ctx.check_cancel()?;
        match price_row(ctx, svc, row, &registry, &links, &mut vat).await {
            Ok(Some((id, w, available))) => to_write.push((id, w, available, row)),
            Ok(None) => {}
            Err(e) if fatal(&e) => return Err(e),
            Err(e) => {
                let name = row
                    .product_name
                    .clone()
                    .unwrap_or_else(|| row.product.clone());
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
            }
        }
        ctx.progress("PRICE", n as u32 + 1, total, "قیمت روز بارزپخش");
    }
    vat.warn(ctx, "PRICE", svc.settings.vat_percent);

    ctx.phase("WRITE")?;
    let writer = ShelfWriter::new(&svc.landau, &ctx.db, SUPPLIER_ID);
    let mut written: Vec<(i64, String)> = Vec::new();
    let total = to_write.len() as u32;
    for (n, (item_id, w, available, row)) in to_write.iter().enumerate() {
        ctx.check_cancel()?;
        match writer.refresh(w, &store, *available, row).await {
            Ok(outcome) => {
                // the cost before is the one the write replaced, as the writer's fresh read found it
                // (plan 130 R-i); an unchanged row had nothing replaced
                let (cost, was_held, action, price_after, before) = match outcome {
                    Refreshed::Unchanged { cost, held } => {
                        (cost, held, "SKIP", row.price, Some(cost))
                    }
                    Refreshed::Patched {
                        cost,
                        held,
                        before,
                        price_after,
                        replanned,
                    } => {
                        if replanned {
                            ctx.db.add_item_note(
                                *item_id,
                                "ردیف قفسه پس از خواندن تغییر کرده بود؛ با مقدار تازهٔ آن نوشته شد.",
                            )?;
                        }
                        (cost, held, "PATCH", price_after, before)
                    }
                };
                if was_held {
                    ctx.db
                        .add_item_note(*item_id, "ناموجود؛ قیمت پایین‌تر نادیده گرفته شد")?;
                } else if !available {
                    ctx.db.add_item_note(*item_id, "ناموجود در بارزپخش")?;
                }
                ctx.item(
                    *item_id,
                    &[
                        ("cost", i(cost)),
                        ("cost_before", opt_i(before)),
                        ("action", t(action)),
                        ("price_after", opt_i(price_after)),
                        ("status", t("WRITTEN")),
                    ],
                )?;
                written.push((*item_id, row.id.clone()));
            }
            Err(e) if fatal(&e) => return Err(e),
            Err(e) => held(ctx, &[*item_id], &e)?,
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
        // held rows are counted apart (plan 130 R-h), never twice
        "skipped": all.iter().filter(|i| i.status == "SKIPPED" && !i.error_kind.as_deref().is_some_and(|k| IDENTITY_KINDS.contains(&k))).count(),
        "held": all.iter().filter(|i| i.error_kind.as_deref().is_some_and(|k| IDENTITY_KINDS.contains(&k))).count(),
        "failed": all.iter().filter(|i| i.status == "FAILED").count(),
    });
    ctx.db.set_job_summary(ctx.job_id, &summary)?;
    ctx.info("REPORT", &summary.to_string(), None);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::matches::{state, LinkProduct};
    use crate::matching::evidence::tests::cand;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Counts every price lookup; a name search answers that name's quote from `quotes`.
    #[derive(Default)]
    struct CountingSupplier {
        calls: Arc<AtomicUsize>,
        quotes: HashMap<String, i64>,
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
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }
        async fn quotes_by_name(&self, name: &str) -> Result<Vec<crate::domain::PriceQuote>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self
                .quotes
                .get(name)
                .map(|&list_price| crate::domain::PriceQuote {
                    code: "C".into(),
                    name: name.into(),
                    list_price,
                    vat_percent: Some(10.0),
                    has_vat: true,
                    available: true,
                    unit: "عدد".into(),
                })
                .into_iter()
                .collect())
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
            snapshot: Default::default(),
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
                auto_update: true,
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
            condition: Some("NEW".into()),
            authenticity: Some("ORIGINAL".into()),
            price_source: None,
            updated_at: None,
        }
    }

    /// A link on `product`, proven by this job when `proven`.
    fn link(ctx: &JobCtx, name: &str, product: &str, proven: bool) -> (MatchRow, bool) {
        ctx.db
            .set_match(SUPPLIER_ID, name, "", Some((product, "x")), "USER", None)
            .unwrap();
        let p = LinkProduct {
            id: product,
            name: "x",
            brand: Some(""),
            alias: Some(""),
        };
        let st = if proven {
            state::VALID
        } else {
            state::NEEDS_REVIEW
        };
        let reason =
            (!proven).then(|| r#"[{"kind":"NUMBER_CONFLICT","lacking":["69349"]}]"#.to_string());
        let key = crate::matching::normalize::name_key(name);
        let version = ctx
            .db
            .link_by_key(SUPPLIER_ID, &key)
            .unwrap()
            .unwrap()
            .link_version;
        ctx.db
            .validate_link(SUPPLIER_ID, &key, version, p, st, reason, ctx.job_id)
            .unwrap()
            .unwrap();
        (
            ctx.db.match_for_name(SUPPLIER_ID, name).unwrap().unwrap(),
            proven,
        )
    }

    fn registry(product: &str) -> Registry {
        Registry::from_products(vec![cand(
            product,
            "آرمیچر استارت پراید",
            None,
            Some("کروز"),
        )])
    }

    #[tokio::test]
    async fn an_unlinked_row_is_skipped_without_quoting() {
        let supplier = CountingSupplier::default();
        let calls = supplier.calls.clone();
        let (ctx, svc) = fixture(supplier);
        let out = price_row(
            &ctx,
            &svc,
            &shelf_row("p"),
            &registry("p"),
            &HashMap::new(),
            &mut VatMismatches::default(),
        )
        .await
        .unwrap();
        assert!(out.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let item = &ctx.db.job_items(ctx.job_id).unwrap()[0];
        assert_eq!(item.error_kind.as_deref(), Some(kinds::NO_SUPPLIER_LINK));
        // a row whose own product left the registry says so
        price_row(
            &ctx,
            &svc,
            &shelf_row("gone"),
            &registry("p"),
            &HashMap::new(),
            &mut VatMismatches::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            ctx.db.job_items(ctx.job_id).unwrap()[1]
                .error_kind
                .as_deref(),
            Some("StaleLink")
        );
    }

    /// Q-3 in REFRESH: two links on one row with two costs hold it, naming each cost; the old
    /// "most recently priced wins" is gone.
    #[tokio::test]
    async fn a_row_whose_links_disagree_on_cost_is_held() {
        let supplier = CountingSupplier {
            quotes: [
                ("بست CW3.6".to_string(), 83_000),
                ("بست CW4.8".to_string(), 88_000),
            ]
            .into(),
            ..Default::default()
        };
        let (ctx, svc) = fixture(supplier);
        let links: LinksByProduct = [(
            "p".to_string(),
            vec![
                link(&ctx, "بست CW3.6", "p", true),
                link(&ctx, "بست CW4.8", "p", true),
            ],
        )]
        .into();
        let out = price_row(
            &ctx,
            &svc,
            &shelf_row("p"),
            &registry("p"),
            &links,
            &mut VatMismatches::default(),
        )
        .await
        .unwrap();
        assert!(out.is_none());
        let item = &ctx.db.job_items(ctx.job_id).unwrap()[0];
        assert_eq!(
            (item.status.as_str(), item.error_kind.as_deref()),
            ("SKIPPED", Some("Conflict"))
        );
        let note = item.note.clone().unwrap();
        assert!(note.contains("91300") && note.contains("96800"), "{note}");
    }

    /// Task 12's shape: one proven link and one awaiting review on the same row hold it.
    #[tokio::test]
    async fn an_unproven_link_on_the_row_holds_it_without_quoting() {
        let supplier = CountingSupplier {
            quotes: [("درست".to_string(), 5_631_000)].into(),
            ..Default::default()
        };
        let calls = supplier.calls.clone();
        let (ctx, svc) = fixture(supplier);
        let links: LinksByProduct = [(
            "p".to_string(),
            vec![
                link(&ctx, "درست", "p", true),
                link(&ctx, "شتابزده", "p", false),
            ],
        )]
        .into();
        let out = price_row(
            &ctx,
            &svc,
            &shelf_row("p"),
            &registry("p"),
            &links,
            &mut VatMismatches::default(),
        )
        .await
        .unwrap();
        assert!(out.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let item = &ctx.db.job_items(ctx.job_id).unwrap()[0];
        assert_eq!(item.error_kind.as_deref(), Some("EvidenceConflict"));
    }

    #[tokio::test]
    async fn links_that_agree_make_one_proven_write_at_the_supplier_cost() {
        let supplier = CountingSupplier {
            quotes: [("الف".to_string(), 100_000), ("ب".to_string(), 100_000)].into(),
            ..Default::default()
        };
        let (ctx, svc) = fixture(supplier);
        let links: LinksByProduct = [(
            "p".to_string(),
            vec![link(&ctx, "الف", "p", true), link(&ctx, "ب", "p", true)],
        )]
        .into();
        let (_, w, available) = price_row(
            &ctx,
            &svc,
            &shelf_row("p"),
            &registry("p"),
            &links,
            &mut VatMismatches::default(),
        )
        .await
        .unwrap()
        .expect("a write");
        assert_eq!((w.cost(), w.links().len(), available), (110_000, 2, true));
    }

    /// Slice 2's review: a quote that fails fails the row's own item, and only that one.
    #[tokio::test]
    async fn a_failed_quote_fails_the_rows_one_item() {
        struct Failing;
        #[async_trait::async_trait]
        impl crate::supplier::Supplier for Failing {
            fn id(&self) -> &'static str {
                "failing"
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
                Ok(None)
            }
            async fn quotes_by_name(&self, _name: &str) -> Result<Vec<crate::domain::PriceQuote>> {
                Err(BridgeError::SupplierParse {
                    page: "search".into(),
                    field: "price".into(),
                })
            }
        }
        let (ctx, mut svc) = fixture(CountingSupplier::default());
        svc.supplier = Box::new(Failing);
        let links: LinksByProduct = [("p".to_string(), vec![link(&ctx, "الف", "p", true)])].into();
        let out = price_row(
            &ctx,
            &svc,
            &shelf_row("p"),
            &registry("p"),
            &links,
            &mut VatMismatches::default(),
        )
        .await
        .unwrap();
        assert!(out.is_none());
        let items = ctx.db.job_items(ctx.job_id).unwrap();
        assert_eq!(items.len(), 1, "one item per row");
        assert_eq!(
            (items[0].status.as_str(), items[0].error_kind.as_deref()),
            ("FAILED", Some("SupplierParse"))
        );
    }
}
