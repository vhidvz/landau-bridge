//! Job runners (spec §7). One job at a time; every step persisted; progress as Tauri events.

pub mod import;
pub mod planner;
pub mod pricing;
pub mod refresh;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::db::jobs::ItemPatch;
use crate::db::Db;
use crate::domain::cost_of;
use crate::errors::{kinds, BridgeError, Result};
use crate::landau::{LandauClient, RepricedRow};
use crate::settings::{Settings, SUPPLIER_ID};
use crate::supplier::Supplier;

pub const EVENT_PROGRESS: &str = "job://progress";
pub const EVENT_EVENT: &str = "job://event";
pub const EVENT_DONE: &str = "job://done";

/// Short constructors for [`ItemPatch`] values.
pub fn t(s: &str) -> rusqlite::types::Value {
    rusqlite::types::Value::Text(s.to_string())
}
pub fn i(n: i64) -> rusqlite::types::Value {
    rusqlite::types::Value::Integer(n)
}
pub fn opt_i(n: Option<i64>) -> rusqlite::types::Value {
    n.map(rusqlite::types::Value::Integer)
        .unwrap_or(rusqlite::types::Value::Null)
}

/// Failures that stop a job outright rather than being recorded against one item and continuing.
pub fn fatal(e: &BridgeError) -> bool {
    matches!(
        e,
        BridgeError::SupplierUnreachable(_)
            | BridgeError::SupplierBlocked
            | BridgeError::SupplierBadCredentials
            | BridgeError::SupplierSessionExpired
            | BridgeError::LandauUnauthorized
            | BridgeError::LandauUnreachable(_)
            | BridgeError::JobCancelled
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub job_id: i64,
    pub phase: String,
    pub done: u32,
    pub total: u32,
    pub message: String,
}

/// Everything a job talks to. Built per job by the command layer.
pub struct Services {
    pub landau: LandauClient,
    pub supplier: Box<dyn Supplier>,
    pub settings: Settings,
}

pub fn emit_event<T: Serialize + Clone>(app: &Option<AppHandle>, name: &str, payload: T) {
    if let Some(app) = app {
        if let Err(e) = app.emit(name, payload) {
            tracing::warn!("emit {name}: {e}");
        }
    }
}

pub struct JobCtx {
    pub db: Db,
    pub app: Option<AppHandle>,
    pub job_id: i64,
    pub cancel: Arc<AtomicBool>,
}

impl JobCtx {
    pub fn check_cancel(&self) -> Result<()> {
        if self.cancel.load(Ordering::SeqCst) {
            Err(BridgeError::JobCancelled)
        } else {
            Ok(())
        }
    }

    pub fn phase(&self, phase: &str) -> Result<()> {
        self.db.set_job_phase(self.job_id, phase)?;
        self.progress(phase, 0, 0, "");
        Ok(())
    }

    pub fn progress(&self, phase: &str, done: u32, total: u32, message: &str) {
        emit_event(
            &self.app,
            EVENT_PROGRESS,
            Progress {
                job_id: self.job_id,
                phase: phase.into(),
                done,
                total,
                message: message.into(),
            },
        );
    }

    pub fn info(&self, phase: &str, message: &str, detail: Option<Value>) {
        tracing::info!(job = self.job_id, phase, "{message}");
        let _ = self
            .db
            .add_job_event(self.job_id, "INFO", phase, message, detail.clone());
        emit_event(
            &self.app,
            EVENT_EVENT,
            json!({"job_id": self.job_id, "level": "INFO", "phase": phase, "message": message, "detail": detail}),
        );
    }

    /// A recorded warning: `errors` row + job event + UI event (spec §9).
    pub fn warn(&self, phase: &str, kind: &str, message: &str, detail: Option<Value>) {
        tracing::warn!(job = self.job_id, phase, kind, "{message}");
        let _ = self
            .db
            .record_error(Some(self.job_id), kind, message, detail.clone());
        let _ = self
            .db
            .add_job_event(self.job_id, "WARN", phase, message, detail.clone());
        emit_event(
            &self.app,
            EVENT_EVENT,
            json!({"job_id": self.job_id, "level": "WARN", "kind": kind, "phase": phase, "message": message, "detail": detail}),
        );
    }

    pub fn error(&self, phase: &str, err: &BridgeError, detail: Option<Value>) {
        tracing::error!(job = self.job_id, phase, kind = err.kind(), "{err}");
        let _ = self.db.record_error(
            Some(self.job_id),
            err.kind(),
            &err.message_fa(),
            detail.clone(),
        );
        let _ = self.db.add_job_event(
            self.job_id,
            "ERROR",
            phase,
            &err.message_fa(),
            detail.clone(),
        );
        emit_event(
            &self.app,
            EVENT_EVENT,
            json!({"job_id": self.job_id, "level": "ERROR", "kind": err.kind(), "phase": phase, "message": err.message_fa(), "detail": detail}),
        );
    }

    pub fn item(&self, item_id: i64, patch: &[ItemPatch<'_>]) -> Result<()> {
        self.db.update_job_item(item_id, patch)
    }

    /// Terminal bookkeeping shared by both jobs.
    pub fn finish(&self, outcome: &Result<()>, phase: &str) {
        let status = match outcome {
            Ok(()) => "DONE",
            Err(BridgeError::JobCancelled) => {
                self.info(phase, "عملیات به درخواست شما متوقف شد.", None);
                "CANCELLED"
            }
            Err(e) => {
                self.error(phase, e, None);
                "FAILED"
            }
        };
        let _ = self.db.set_job_status(self.job_id, status, phase);
        emit_event(
            &self.app,
            EVENT_DONE,
            json!({"job_id": self.job_id, "status": status}),
        );
    }
}

/// Which job is running, and its cancel flag. One at a time (spec §7.3).
#[derive(Default)]
pub struct JobRegistry {
    current: Mutex<Option<(i64, Arc<AtomicBool>)>>,
}

impl JobRegistry {
    pub fn begin(&self, job_id: i64) -> Result<Arc<AtomicBool>> {
        let mut cur = self
            .current
            .lock()
            .map_err(|e| BridgeError::Other(e.to_string()))?;
        if let Some((running, _)) = cur.as_ref() {
            return Err(BridgeError::JobBusy(format!("#{running}")));
        }
        let flag = Arc::new(AtomicBool::new(false));
        *cur = Some((job_id, flag.clone()));
        Ok(flag)
    }

    pub fn finish(&self, job_id: i64) {
        if let Ok(mut cur) = self.current.lock() {
            if matches!(cur.as_ref(), Some((id, _)) if *id == job_id) {
                *cur = None;
            }
        }
    }

    pub fn cancel(&self, job_id: i64) -> bool {
        match self.current.lock() {
            Ok(cur) => match cur.as_ref() {
                Some((id, flag)) if *id == job_id => {
                    flag.store(true, Ordering::SeqCst);
                    true
                }
                _ => false,
            },
            Err(_) => false,
        }
    }

    pub fn current(&self) -> Option<i64> {
        self.current
            .lock()
            .ok()
            .and_then(|c| c.as_ref().map(|(id, _)| *id))
    }
}

// ------------------------------------------------------------ shared phases

/// Preflight of both jobs: the key acts for the store's approved business; the supplier logs in;
/// zero active rules stops the job (spec §14 e — the shelf is never at cost).
pub async fn preflight(ctx: &JobCtx, svc: &Services, store: &str) -> Result<()> {
    ctx.phase("PREFLIGHT")?;
    let memberships = svc.landau.memberships().await?;
    let owner = memberships
        .iter()
        .find(|m| m.stores.iter().any(|s| s.id == store))
        .ok_or(BridgeError::NoStore)?;
    if !owner.approved {
        return Err(BridgeError::BusinessNotApproved);
    }
    svc.supplier.login().await?;
    let rules = svc.landau.pricing_rules().await?;
    let active = rules.iter().filter(|r| r.active).count();
    if active == 0 {
        return Err(BridgeError::NoPricingRule);
    }
    ctx.info(
        "PREFLIGHT",
        &format!(
            "business {} · {active} active pricing rule(s)",
            owner.business
        ),
        None,
    );
    Ok(())
}

/// The codes whose site VAT rate differed from the setting during one PRICE loop. Reported as a
/// single `VatRateMismatch` warning per run (a site-wide rate change would otherwise repeat the
/// same warning once per item).
#[derive(Debug, Default)]
pub struct VatMismatches {
    /// (code, site rate)
    codes: Vec<(String, f64)>,
}

impl VatMismatches {
    pub fn is_empty(&self) -> bool {
        self.codes.is_empty()
    }

    /// One warning naming how many codes differed, the rates seen, and the setting applied.
    pub fn warn(&self, ctx: &JobCtx, phase: &str, setting: f64) {
        if self.codes.is_empty() {
            return;
        }
        let mut rates: Vec<String> = self.codes.iter().map(|(_, r)| r.to_string()).collect();
        rates.sort();
        rates.dedup();
        let codes: Vec<&str> = self.codes.iter().map(|(c, _)| c.as_str()).collect();
        ctx.warn(
            phase,
            kinds::VAT_RATE_MISMATCH,
            &format!(
                "نرخ ارزش افزوده سایت برای {} کد ({}٪) با تنظیمات ({setting}٪) فرق دارد؛ تنظیمات اعمال شد.",
                codes.len(),
                rates.join("، ")
            ),
            Some(json!({"codes": codes, "site_rates": rates, "setting": setting})),
        );
    }
}

/// One priced supplier row for an item (spec §14 b): by name first, then by code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Costed {
    /// Whole rials, VAT applied per the setting (before the §14 d hold, which needs the row).
    pub cost: i64,
    pub available: bool,
    /// The row's code and name as the site lists them today.
    pub code: String,
    pub name: String,
    /// Found by code under a different name — the site renamed the SKU.
    pub renamed: bool,
}

/// Today's quote for `name`/`code`: the equal-name row of a name search, else the exact code
/// (§14 b). Records the snapshot, notes a VAT-rate difference in `vat`, and stamps the link as
/// priced. `None` when the site lists neither.
pub async fn quote_for(
    ctx: &JobCtx,
    svc: &Services,
    name: &str,
    code: &str,
    vat: &mut VatMismatches,
) -> Result<Option<Costed>> {
    let by_name = pricing::pick_by_name(name, svc.supplier.quotes_by_name(name).await?);
    let (q, renamed) = match by_name {
        Some(q) => (q, false),
        None if code.is_empty() => return Ok(None),
        None => match svc.supplier.price_of(code).await? {
            Some(q) => (q, true),
            None => return Ok(None),
        },
    };
    let setting = svc.settings.vat_percent;
    let applied = if q.has_vat { setting } else { 0.0 };
    if q.has_vat {
        if let Some(site) = q.vat_percent {
            if (site - setting).abs() > 0.01 {
                tracing::warn!(code = %q.code, site, setting, "vat rate differs from the setting");
                vat.codes.push((q.code.clone(), site));
            }
        }
    }
    let cost = cost_of(q.list_price, applied);
    ctx.db.add_price_snapshot(
        SUPPLIER_ID,
        &q.code,
        q.list_price,
        cost,
        q.vat_percent,
        q.available,
        &crate::db::now(),
    )?;
    ctx.db.touch_priced(SUPPLIER_ID, name)?;
    Ok(Some(Costed {
        cost,
        available: q.available,
        code: q.code,
        name: q.name,
        renamed,
    }))
}

/// `(reprice_verdict, final price)` of one row Landau answered (spec §14 f).
pub fn verdict_of(row: &RepricedRow) -> (String, Option<i64>) {
    let verdict = match (&row.next, row.skipped.as_deref()) {
        (Some(next), _) => format!("REPRICED:{next}"),
        (None, Some("UNCHANGED")) | (None, None) => "UNCHANGED".to_string(),
        (None, Some(why)) => format!("SKIPPED:{why}"),
    };
    (verdict, row.next.or(row.price))
}

/// Price the written rows by Landau's rules from the authoritative store (plan 107), stamp each
/// item's verdict and final price. `items` = (item_id, stock_id). A row Landau no longer has is
/// `MISSING`, warned once.
pub async fn reprice_items(ctx: &JobCtx, svc: &Services, items: &[(i64, String)]) -> Result<()> {
    ctx.phase("REPRICE")?;
    if items.is_empty() {
        return Ok(());
    }
    let job = ctx
        .db
        .job(ctx.job_id)?
        .ok_or(BridgeError::JobNotFound(ctx.job_id))?;
    let ids: Vec<String> = items.iter().map(|(_, s)| s.clone()).collect();
    let rows = svc.landau.reprice_stocks(&ids, job.overwrite).await?;
    ctx.info(
        "REPRICE",
        &format!("reprice answered {} of {} row(s)", rows.len(), ids.len()),
        None,
    );
    let mut skipped: HashMap<String, Vec<String>> = HashMap::new();
    let mut missing: Vec<String> = Vec::new();
    for (item_id, stock_id) in items {
        ctx.check_cancel()?;
        let Some(row) = rows.iter().find(|r| &r.stock == stock_id) else {
            missing.push(stock_id.clone());
            ctx.item(*item_id, &[("reprice_verdict", t("MISSING"))])?;
            continue;
        };
        if let Some(why) = row.skipped.as_deref().filter(|w| *w != "UNCHANGED") {
            skipped
                .entry(why.to_string())
                .or_default()
                .push(stock_id.clone());
        }
        let (verdict, price) = verdict_of(row);
        ctx.item(
            *item_id,
            &[
                ("reprice_verdict", t(&verdict)),
                ("price_after", opt_i(price)),
            ],
        )?;
    }
    if !skipped.is_empty() {
        let count: usize = skipped.values().map(Vec::len).sum();
        ctx.warn(
            "REPRICE",
            kinds::REPRICE_SKIPPED,
            &format!("{count} ردیف در قیمت‌گذاری رد شد."),
            Some(json!(skipped)),
        );
    }
    if !missing.is_empty() {
        ctx.warn(
            "REPRICE",
            kinds::REPRICE_SKIPPED,
            &format!("{} ردیف دیگر روی قفسه نیست.", missing.len()),
            Some(json!({"missing": missing})),
        );
    }
    Ok(())
}

/// Mark the written items DONE and audit the margin (spec §14 e): a final price under the
/// minimum is noted on the item and counted in one warning.
pub fn report(ctx: &JobCtx, svc: &Services, items: &[(i64, String)]) -> Result<()> {
    ctx.phase("REPORT")?;
    let margin = svc.settings.min_margin_percent;
    let mut thin: Vec<Value> = Vec::new();
    for (item_id, stock_id) in items {
        let Some(row) = ctx.db.job_item(*item_id)? else {
            continue;
        };
        if let (Some(cost), Some(price)) = (row.cost, row.price_after) {
            if pricing::below_min_margin(price, cost, margin) {
                ctx.db.add_item_note(*item_id, "زیر حداقل سود")?;
                thin.push(
                    json!({"item_id": item_id, "stock_id": stock_id, "cost": cost, "price": price}),
                );
            }
        }
        ctx.item(*item_id, &[("status", t("DONE"))])?;
    }
    if !thin.is_empty() {
        ctx.warn(
            "REPORT",
            kinds::BELOW_MIN_MARGIN,
            &format!(
                "{} ردیف زیر حداقل سود ({margin}٪) قیمت خورده است؛ قیمت دستی یا قاعدهٔ کم‌سود را بررسی کنید.",
                thin.len()
            ),
            Some(json!(thin)),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> JobCtx {
        let db = Db::in_memory().unwrap();
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

    /// A `Supplier` that is never reached: `report` is offline, and `Services` needs one.
    struct NoSupplier;

    #[async_trait::async_trait]
    impl Supplier for NoSupplier {
        fn id(&self) -> &'static str {
            "none"
        }
        async fn login(&self) -> Result<()> {
            Err(BridgeError::Other("offline".into()))
        }
        async fn ledger(
            &self,
            _from: Option<&str>,
            _progress: crate::supplier::Progress,
        ) -> Result<Vec<crate::domain::InvoiceHeader>> {
            Err(BridgeError::Other("offline".into()))
        }
        async fn invoice_lines(&self, _url: &str) -> Result<Vec<crate::domain::InvoiceLine>> {
            Err(BridgeError::Other("offline".into()))
        }
        async fn price_of(&self, _code: &str) -> Result<Option<crate::domain::PriceQuote>> {
            Err(BridgeError::Other("offline".into()))
        }
        async fn quotes_by_name(&self, _name: &str) -> Result<Vec<crate::domain::PriceQuote>> {
            Err(BridgeError::Other("offline".into()))
        }
    }

    fn test_services(min_margin_percent: f64) -> Services {
        Services {
            landau: LandauClient::new("http://127.0.0.1:1", "apt-test").unwrap(),
            supplier: Box::new(NoSupplier),
            settings: Settings {
                landau_url: "http://127.0.0.1:1".into(),
                landau_token: Some("apt-test".into()),
                store: Some("store1".into()),
                store_name: None,
                supplier_user: None,
                supplier_password: None,
                default_authenticity: crate::domain::Authenticity::Original,
                threshold: 85,
                vat_percent: 10.0,
                min_margin_percent,
                overwrite_manual: false,
                fetch_since_only: true,
            },
        }
    }

    fn repriced(
        stock: &str,
        price: Option<i64>,
        next: Option<i64>,
        skipped: Option<&str>,
    ) -> RepricedRow {
        RepricedRow {
            stock: stock.into(),
            product: "p".into(),
            price,
            next,
            cost: Some(1),
            rule: None,
            skipped: skipped.map(str::to_string),
        }
    }

    #[test]
    fn a_reprice_row_yields_a_verdict_and_the_final_price() {
        assert_eq!(
            verdict_of(&repriced("a", Some(100), Some(115), None)),
            ("REPRICED:115".into(), Some(115))
        );
        assert_eq!(
            verdict_of(&repriced("a", Some(100), None, Some("MANUAL"))),
            ("SKIPPED:MANUAL".into(), Some(100))
        );
        assert_eq!(
            verdict_of(&repriced("a", Some(100), None, Some("UNCHANGED"))),
            ("UNCHANGED".into(), Some(100))
        );
        assert_eq!(
            verdict_of(&repriced("a", Some(100), None, None)),
            ("UNCHANGED".into(), Some(100))
        );
    }

    #[test]
    fn the_report_marks_items_done_and_flags_a_thin_margin() {
        let ctx = ctx();
        let svc = test_services(15.0);
        let db = &ctx.db;
        let fine = db.add_job_item(ctx.job_id, "1", "خوب", 0).unwrap();
        db.update_job_item(
            fine,
            &[
                ("cost", i(1_000_000)),
                ("price_after", i(1_150_000)),
                ("status", t("WRITTEN")),
            ],
        )
        .unwrap();
        let thin = db.add_job_item(ctx.job_id, "2", "کم‌سود", 0).unwrap();
        db.update_job_item(
            thin,
            &[
                ("cost", i(1_000_000)),
                ("price_after", i(1_050_000)),
                ("status", t("WRITTEN")),
            ],
        )
        .unwrap();
        report(&ctx, &svc, &[(fine, "sa".into()), (thin, "sb".into())]).unwrap();
        let rows = db.job_items(ctx.job_id).unwrap();
        assert!(rows.iter().all(|r| r.status == "DONE"));
        assert_eq!(rows[0].note, None);
        assert_eq!(rows[1].note.as_deref(), Some("زیر حداقل سود"));
        let events = db.job_detail(ctx.job_id).unwrap().events;
        assert!(events
            .iter()
            .any(|e| e.level == "WARN" && e.message.contains("زیر حداقل سود")));
    }

    #[test]
    fn vat_mismatches_warn_once_per_run_naming_every_code() {
        let ctx = ctx();
        let mut vat = VatMismatches::default();
        assert!(vat.is_empty());
        vat.warn(&ctx, "PRICE", 10.0);
        assert!(
            ctx.db.list_errors(10).unwrap().is_empty(),
            "nothing to report, nothing recorded"
        );

        vat.codes.push(("A1".into(), 9.0));
        vat.codes.push(("B2".into(), 9.0));
        vat.codes.push(("C3".into(), 12.5));
        vat.warn(&ctx, "PRICE", 10.0);
        let errors = ctx.db.list_errors(10).unwrap();
        assert_eq!(errors.len(), 1, "one warning for three codes");
        assert_eq!(errors[0].kind, kinds::VAT_RATE_MISMATCH);
        assert!(errors[0].message.contains("3 کد"), "{}", errors[0].message);
        let detail = errors[0].context.clone().unwrap();
        assert_eq!(detail["codes"], json!(["A1", "B2", "C3"]));
        assert_eq!(detail["site_rates"], json!(["12.5", "9"]));
        assert_eq!(detail["setting"], json!(10.0));
        let events = ctx.db.job_detail(ctx.job_id).unwrap().events;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].level, "WARN");
        assert_eq!(events[0].phase, "PRICE");
    }
}
