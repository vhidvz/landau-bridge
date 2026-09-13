//! Tauri commands (spec §3). Thin by design: no business logic lives here.

pub mod dto;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::db::errors::ErrorRow;
use crate::db::jobs::{JobDetail, JobItemRow, JobRow};
use crate::db::matches::MatchRow;
use crate::db::Db;
use crate::domain::ImportMode;
use crate::errors::{kinds, BridgeError, Result};
use crate::jobs::{self, JobCtx, JobRegistry, Services};
use crate::landau::LandauClient;
use crate::matching::{candidates, score, Matcher, Scored};
use crate::secrets::SecretStore;
use crate::settings::{Settings, SettingsPatch, SUPPLIER_ID};
use crate::supplier;
use dto::*;

pub struct AppState {
    pub db: Db,
    pub secrets: SecretStore,
    pub jobs: Arc<JobRegistry>,
    pub data_dir: PathBuf,
    pub _log_guard: tracing_appender::non_blocking::WorkerGuard,
}

fn settings(state: &AppState) -> Result<Settings> {
    Settings::load(&state.db, &state.secrets)
}

fn landau(s: &Settings) -> Result<LandauClient> {
    let token = s
        .landau_token
        .as_deref()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| BridgeError::SettingsIncomplete("کلید API لندو".into()))?;
    LandauClient::new(&s.landau_url, token)
}

fn services(state: &AppState) -> Result<Services> {
    let settings = settings(state)?;
    let landau = landau(&settings)?;
    let supplier = supplier::open(SUPPLIER_ID, settings.require_supplier()?)?;
    Ok(Services {
        landau,
        supplier,
        settings,
    })
}

// ------------------------------------------------------------ settings

#[tauri::command(rename_all = "snake_case")]
pub async fn settings_get(state: State<'_, AppState>) -> Result<SettingsView> {
    let s = settings(&state)?;
    Ok(SettingsView {
        landau_url: s.landau_url,
        has_landau_token: s
            .landau_token
            .as_deref()
            .map(|t| !t.is_empty())
            .unwrap_or(false),
        store: s.store,
        store_name: s.store_name,
        supplier_user: s.supplier_user,
        has_supplier_password: s
            .supplier_password
            .as_deref()
            .map(|p| !p.is_empty())
            .unwrap_or(false),
        default_authenticity: s.default_authenticity,
        threshold: s.threshold,
        vat_percent: s.vat_percent,
        overwrite_manual: s.overwrite_manual,
        secret_backend: state.secrets.backend().into(),
        data_dir: state.data_dir.display().to_string(),
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn settings_save(
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<SettingsView> {
    Settings::save(&state.db, &state.secrets, &patch)?;
    settings_get(state).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn landau_validate(state: State<'_, AppState>) -> Result<LandauStatus> {
    let s = settings(&state)?;
    let client = landau(&s)?;
    let memberships = client.memberships().await?;
    let mine = memberships
        .iter()
        .find(|m| m.approved)
        .or_else(|| memberships.first())
        .ok_or(BridgeError::NoStore)?;
    if !mine.approved {
        return Err(BridgeError::BusinessNotApproved);
    }
    let stores: Vec<_> = mine
        .stores
        .iter()
        .filter(|st| st.state.as_deref() != Some("REJECTED"))
        .cloned()
        .collect();
    if stores.is_empty() {
        return Err(BridgeError::NoStore);
    }
    let active_rules = client
        .pricing_rules()
        .await
        .map(|r| r.iter().filter(|x| x.active).count())
        .unwrap_or(0);
    Ok(LandauStatus {
        business: mine.business.clone(),
        business_name: mine.name.clone(),
        approved: mine.approved,
        stores,
        active_rules,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn supplier_test_login(state: State<'_, AppState>) -> Result<()> {
    let s = settings(&state)?;
    let sup = supplier::open(SUPPLIER_ID, s.require_supplier()?)?;
    sup.login().await
}

// ------------------------------------------------------------ dashboard

#[tauri::command(rename_all = "snake_case")]
pub async fn dashboard_get(state: State<'_, AppState>) -> Result<Dashboard> {
    let s = settings(&state)?;
    let jobs = state.db.list_jobs()?;
    let (invoices_cached, invoices_unpaid) = state.db.count_invoices(SUPPLIER_ID)?;
    let running = state.db.running_job()?;
    let pending_review = match &running {
        Some(j) if j.status == "REVIEW" => state
            .db
            .job_items(j.id)?
            .iter()
            .filter(|i| i.status == "REVIEW")
            .count() as i64,
        _ => 0,
    };
    Ok(Dashboard {
        settings_complete: s.require_landau().is_ok() && s.require_supplier().is_ok(),
        store_name: s.store_name,
        supplier_user: s.supplier_user,
        invoices_cached,
        invoices_unpaid,
        last_import: jobs.iter().find(|j| j.kind == "IMPORT").cloned(),
        last_refresh: jobs.iter().find(|j| j.kind == "REFRESH").cloned(),
        running,
        pending_review,
        recent_errors: state.db.count_unresolved_errors()?,
        decided_matches: state.db.list_matches(SUPPLIER_ID)?.len() as i64,
    })
}

// ------------------------------------------------------------ jobs

fn spawn<F, Fut>(app: AppHandle, state: &AppState, job_id: i64, run: F) -> Result<()>
where
    F: FnOnce(JobCtx, Services) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<()>> + Send,
{
    let cancel = state.jobs.begin(job_id)?;
    let ctx = JobCtx {
        db: state.db.clone(),
        app: Some(app),
        job_id,
        cancel,
    };
    let registry = state.jobs.clone();
    let svc = match services(state) {
        Ok(s) => s,
        Err(e) => {
            ctx.finish(&Err(e.clone()), "PREFLIGHT");
            registry.finish(job_id);
            return Err(e);
        }
    };
    tauri::async_runtime::spawn(async move {
        let outcome = run(ctx, svc).await;
        registry.finish(job_id);
        outcome
    });
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn import_start(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: String,
    since_date: Option<String>,
    fetch_limit: Option<usize>,
) -> Result<i64> {
    let s = settings(&state)?;
    let (_, _, store) = s.require_landau()?;
    let mode = ImportMode::parse(&mode)
        .ok_or_else(|| BridgeError::Other(format!("unknown mode {mode}")))?;
    let since = since_date
        .as_deref()
        .and_then(crate::domain::normalize_jalali);
    if mode == ImportMode::Quantity && since.is_none() {
        return Err(BridgeError::SettingsIncomplete(
            "تاریخ شروع فاکتورها (مثلاً ۱۴۰۳/۰۱/۰۱)".into(),
        ));
    }
    let job_id = state.db.create_job(
        "IMPORT",
        Some(mode.as_str()),
        since.as_deref(),
        store,
        s.overwrite_manual,
    )?;
    let params = jobs::import::ImportParams {
        mode,
        since,
        fetch_limit,
    };
    spawn(app, &state, job_id, move |ctx, svc| async move {
        let phase = ctx
            .db
            .job(ctx.job_id)
            .ok()
            .flatten()
            .map(|j| j.phase)
            .unwrap_or_default();
        match jobs::import::start(&ctx, &svc, &params).await {
            Ok(jobs::import::Outcome::Review(_)) => Ok(()), // paused; status already REVIEW
            Ok(jobs::import::Outcome::Done) => {
                ctx.finish(&Ok(()), "REPORT");
                Ok(())
            }
            Err(e) => {
                let phase = ctx
                    .db
                    .job(ctx.job_id)
                    .ok()
                    .flatten()
                    .map(|j| j.phase)
                    .unwrap_or(phase);
                ctx.finish(&Err(e.clone()), &phase);
                Err(e)
            }
        }
    })?;
    Ok(job_id)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn import_review(state: State<'_, AppState>, job_id: i64) -> Result<Vec<JobItemRow>> {
    Ok(state
        .db
        .job_items(job_id)?
        .into_iter()
        .filter(|i| i.status == "REVIEW")
        .collect())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn review_decide(state: State<'_, AppState>, decision: ReviewDecision) -> Result<()> {
    let item = state
        .db
        .job_item(decision.item_id)?
        .ok_or(BridgeError::JobNotFound(decision.item_id))?;
    let s = settings(&state)?;
    let matcher = Matcher::new(state.db.clone(), SUPPLIER_ID, s.threshold);
    if decision.later {
        return Ok(()); // stays REVIEW; commit() skips it and the next run asks again
    }
    if decision.none {
        matcher.record_none(&item.code, &item.name)?;
        state.db.record_error(
            Some(item.job_id),
            kinds::NONE_DECISION,
            &format!(
                "«{}» در فهرست لندو نیست؛ برای افزودن آن یک تیکت برای پشتیبانی لندو ثبت کنید.",
                item.name
            ),
            Some(serde_json::json!({"code": item.code})),
        )?;
        return state.db.update_job_item(
            item.id,
            &[
                ("status", jobs::t("SKIPPED")),
                ("error_kind", jobs::t(kinds::NONE_DECISION)),
                ("error_msg", jobs::t("این قطعه در لندو نیست.")),
            ],
        );
    }
    let product_id = decision
        .product_id
        .ok_or_else(|| BridgeError::Other("no product picked".into()))?;
    let product_name = decision.product_name.unwrap_or_default();
    matcher.record_user_pick(
        &item.code,
        &item.name,
        &product_id,
        &product_name,
        decision.similarity,
    )?;
    let mut patch = vec![
        ("product_id", jobs::t(&product_id)),
        ("product_name", jobs::t(&product_name)),
        ("status", jobs::t("PLANNED")),
    ];
    if let Some(a) = decision.authenticity {
        patch.push(("authenticity", jobs::t(a.as_str())));
    }
    state.db.update_job_item(item.id, &patch)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn review_search(state: State<'_, AppState>, term: String) -> Result<Vec<Scored>> {
    let s = settings(&state)?;
    let client = Arc::new(landau(&s)?);
    let search = move |q: String, limit: usize, skip: usize| {
        let client = client.clone();
        async move { client.registry_search(&q, limit, skip).await }
    };
    let hits = candidates(&search, &term).await?;
    Ok(score(&term, &hits)
        .into_iter()
        .take(crate::matching::CANDIDATES)
        .collect())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn import_continue(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: i64,
) -> Result<()> {
    let job = state
        .db
        .job(job_id)?
        .ok_or(BridgeError::JobNotFound(job_id))?;
    if job.status != "REVIEW" {
        return Err(BridgeError::JobBusy(format!("#{job_id} is {}", job.status)));
    }
    spawn(app, &state, job_id, move |ctx, svc| async move {
        let outcome = jobs::import::commit(&ctx, &svc).await;
        let phase = ctx
            .db
            .job(ctx.job_id)
            .ok()
            .flatten()
            .map(|j| j.phase)
            .unwrap_or_default();
        ctx.finish(&outcome, &phase);
        outcome
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn refresh_start(app: AppHandle, state: State<'_, AppState>) -> Result<i64> {
    let s = settings(&state)?;
    let (_, _, store) = s.require_landau()?;
    s.require_supplier()?;
    let job_id = state
        .db
        .create_job("REFRESH", None, None, store, s.overwrite_manual)?;
    spawn(app, &state, job_id, move |ctx, svc| async move {
        let outcome = jobs::refresh::run(&ctx, &svc).await;
        let phase = ctx
            .db
            .job(ctx.job_id)
            .ok()
            .flatten()
            .map(|j| j.phase)
            .unwrap_or_default();
        ctx.finish(&outcome, &phase);
        outcome
    })?;
    Ok(job_id)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn job_cancel(state: State<'_, AppState>, job_id: i64) -> Result<bool> {
    Ok(state.jobs.cancel(job_id))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn job_current(state: State<'_, AppState>) -> Result<Option<JobRow>> {
    state.db.running_job()
}

#[tauri::command(rename_all = "snake_case")]
pub async fn jobs_list(state: State<'_, AppState>) -> Result<Vec<JobRow>> {
    state.db.list_jobs()
}

#[tauri::command(rename_all = "snake_case")]
pub async fn job_get(state: State<'_, AppState>, job_id: i64) -> Result<JobDetail> {
    state.db.job_detail(job_id)
}

// ------------------------------------------------------------ matches & errors

#[tauri::command(rename_all = "snake_case")]
pub async fn matches_list(state: State<'_, AppState>) -> Result<Vec<MatchRow>> {
    state.db.list_matches(SUPPLIER_ID)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn match_set(
    state: State<'_, AppState>,
    code: String,
    product_id: String,
    product_name: String,
    name_seen: String,
) -> Result<()> {
    state.db.set_match(
        SUPPLIER_ID,
        &code,
        Some((&product_id, &product_name)),
        "USER",
        None,
        &name_seen,
    )
}

#[tauri::command(rename_all = "snake_case")]
pub async fn match_none(state: State<'_, AppState>, code: String, name_seen: String) -> Result<()> {
    state
        .db
        .set_match(SUPPLIER_ID, &code, None, "NONE", None, &name_seen)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn match_clear(state: State<'_, AppState>, code: String) -> Result<()> {
    state.db.clear_match(SUPPLIER_ID, &code)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn errors_list(state: State<'_, AppState>, limit: Option<i64>) -> Result<Vec<ErrorRow>> {
    state.db.list_errors(limit.unwrap_or(200))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_log_folder(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let dir = state.data_dir.join("logs");
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| BridgeError::Other(e.to_string()))
}
