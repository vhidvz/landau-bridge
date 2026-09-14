//! Job runners (spec §7). One job at a time; every step persisted; progress as Tauri events.

pub mod import;
pub mod planner;
pub mod refresh;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
pub const MIRROR_WAIT: Duration = Duration::from_secs(15);

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
/// zero active rules is a warning (spec §7.1).
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
        ctx.warn(
            "PREFLIGHT",
            kinds::NO_PRICING_RULE,
            "در لاندا هیچ قاعده قیمت‌گذاری فعالی ندارید؛ قیمت قفسه برابر قیمت خرید خواهد بود.",
            None,
        );
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

/// Today's cost of one code; records the snapshot, and notes the code in `vat` when the site's
/// rate differs from the setting (the setting wins — spec Global Constraints; the caller warns
/// once per run through [`VatMismatches::warn`]).
pub async fn cost_for(
    ctx: &JobCtx,
    svc: &Services,
    code: &str,
    vat: &mut VatMismatches,
) -> Result<Option<i64>> {
    let Some(q) = svc.supplier.price_of(code).await? else {
        return Ok(None);
    };
    let setting = svc.settings.vat_percent;
    let applied = if q.has_vat { setting } else { 0.0 };
    if q.has_vat {
        if let Some(site) = q.vat_percent {
            if (site - setting).abs() > 0.01 {
                tracing::warn!(code, site, setting, "vat rate differs from the setting");
                vat.codes.push((code.to_string(), site));
            }
        }
    }
    let cost = cost_of(q.list_price, applied);
    ctx.db.add_price_snapshot(
        SUPPLIER_ID,
        code,
        q.list_price,
        cost,
        q.vat_percent,
        q.available,
        &crate::db::now(),
    )?;
    Ok(Some(cost))
}

/// Wait for the mirror, run Landau's rules over the store, and stamp each item's verdict.
/// `items` = (item_id, stock_id).
pub async fn reprice_items(
    ctx: &JobCtx,
    svc: &Services,
    store: &str,
    items: &[(i64, String)],
) -> Result<()> {
    ctx.phase("REPRICE")?;
    let ids: Vec<String> = items.iter().map(|(_, s)| s.clone()).collect();
    if !ids.is_empty() {
        let missing = svc.landau.await_visible(store, &ids, MIRROR_WAIT).await?;
        if !missing.is_empty() {
            ctx.warn(
                "REPRICE",
                kinds::MIRROR_TIMEOUT,
                &BridgeError::MirrorTimeout(missing.len()).message_fa(),
                Some(json!({"missing": missing})),
            );
        }
    }
    let job = ctx
        .db
        .job(ctx.job_id)?
        .ok_or(BridgeError::JobNotFound(ctx.job_id))?;
    let rows: Vec<RepricedRow> = svc.landau.reprice(store, job.overwrite).await?;
    ctx.info(
        "REPRICE",
        &format!("reprice answered {} row(s)", rows.len()),
        None,
    );
    let mut skipped: HashMap<String, Vec<String>> = HashMap::new();
    for (item_id, stock_id) in items {
        ctx.check_cancel()?;
        let row_match = rows.iter().find(|r| &r.stock == stock_id);
        let verdict = row_match.map(|r| match (&r.next, &r.skipped) {
            (Some(next), _) => format!("REPRICED:{next}"),
            (None, Some(why)) => format!("SKIPPED:{why}"),
            (None, None) => "UNCHANGED".to_string(),
        });
        if let Some(why) = row_match.and_then(|r| r.skipped.clone()) {
            // Landau can also report `skipped = "UNCHANGED"`, which is not a rejection.
            if why != "UNCHANGED" {
                skipped.entry(why).or_default().push(stock_id.clone());
            }
        }
        let mut patch: Vec<(&str, rusqlite::types::Value)> = vec![(
            "reprice_verdict",
            verdict
                .as_deref()
                .map(t)
                .unwrap_or(rusqlite::types::Value::Null),
        )];
        // the item's final price is already known when Landau just repriced it; `readback`
        // fills the rest (rows Landau left UNCHANGED or SKIPPED) from a fresh shelf read.
        if let Some(next) = row_match.and_then(|r| r.next) {
            patch.push(("price_after", i(next)));
        }
        ctx.item(*item_id, &patch)?;
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
    Ok(())
}

/// Wait out mirror lag, read the shelf once more, and fill any `price_after` still missing —
/// `reprice_items` already stamped it for rows Landau just repriced. Mark items DONE either way.
pub async fn readback(
    ctx: &JobCtx,
    svc: &Services,
    store: &str,
    items: &[(i64, String)],
) -> Result<()> {
    ctx.phase("REPORT")?;
    if items.is_empty() {
        return Ok(());
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    let shelf = svc.landau.shelf(store).await?;
    for (item_id, stock_id) in items {
        let already_priced = ctx
            .db
            .job_item(*item_id)?
            .and_then(|row| row.price_after)
            .is_some();
        let mut patch: Vec<(&str, rusqlite::types::Value)> = vec![("status", t("DONE"))];
        if !already_priced {
            let price = shelf
                .iter()
                .find(|r| &r.id == stock_id)
                .and_then(|r| r.price);
            patch.push(("price_after", opt_i(price)));
        }
        ctx.item(*item_id, &patch)?;
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
