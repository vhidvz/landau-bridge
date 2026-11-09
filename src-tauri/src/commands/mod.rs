//! Tauri commands (spec §3). Thin by design: no business logic lives here.

pub mod dto;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::appimage;
use crate::db::errors::ErrorRow;
use crate::db::jobs::{JobDetail, JobItemRow, JobRow};
use crate::db::matches::{MatchRow, UndecidedRow};
use crate::db::Db;
use crate::decisions::{self, ImportReport};
use crate::domain::ImportMode;
use crate::errors::{kinds, BridgeError, Result};
use crate::inspect::{self, Inspection};
use crate::jobs::{self, JobCtx, JobRegistry, Services};
use crate::landau::LandauClient;
use crate::matching::gate::Reason;
use crate::matching::registry::{Kept, Snapshot};
use crate::matching::{candidates, score, Matcher, Scored};
use crate::notice;
use crate::secrets::SecretStore;
use crate::settings::{Settings, SettingsPatch, SUPPLIER_ID};
use crate::supplier::{self, Credentials, Supplier};
use crate::update::{self, PluginSource, UpdateState, Updater};
use dto::*;

pub struct AppState {
    pub db: Db,
    pub secrets: SecretStore,
    pub jobs: Arc<JobRegistry>,
    pub data_dir: PathBuf,
    pub updater: Arc<Updater<PluginSource>>,
    pub _log_guard: tracing_appender::non_blocking::WorkerGuard,
    /// The data folder's single-instance lock, held for the life of the app (plan 130 R-m).
    pub _instance: crate::instance::InstanceLock,
    /// The latest whole-registry read, kept for the review and Matches screens (slice 4).
    pub snapshot: Arc<Snapshot>,
    /// The supplier session the screens' price reads share, with the credentials it was opened
    /// with — reopened when the seller changes them.
    pub inspector: tokio::sync::Mutex<Option<(Credentials, Arc<dyn Supplier>)>>,
    /// One inspection at a time: the site serves one request per session at a time, and the
    /// screens share one session (three cards ask at once).
    pub inspecting: tokio::sync::Mutex<()>,
    /// The last audit (plan 130 R-i), kept for its CSV export.
    pub audit: tokio::sync::Mutex<Option<crate::audit::Audit>>,
}

fn settings(state: &AppState) -> Result<Settings> {
    Settings::load(&state.db, &state.secrets)
}

fn landau(s: &Settings) -> Result<LandauClient> {
    let token = s
        .landau_token
        .as_deref()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| BridgeError::SettingsIncomplete("کلید API لاندا".into()))?;
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
        snapshot: state.snapshot.clone(),
    })
}

/// The kept registry snapshot, or one read now (about 90 s) when none is kept — a screen judging a
/// pick never judges on less than the whole registry.
async fn kept_snapshot(state: &AppState) -> Result<Kept> {
    state
        .snapshot
        .get_or_read(|| async {
            let s = settings(state)?;
            let client = landau(&s)?;
            jobs::read_checked(&state.db, &client).await
        })
        .await
}

/// The screens' supplier session: opened once, with the short retry budget of a check the seller
/// waits on.
async fn inspector(state: &AppState, s: &Settings) -> Result<Arc<dyn Supplier>> {
    let creds = s.require_supplier()?;
    let mut slot = state.inspector.lock().await;
    if let Some((c, sup)) = slot.as_ref() {
        if *c == creds {
            return Ok(sup.clone());
        }
    }
    let sup: Arc<dyn Supplier> = Arc::from(supplier::open_for_check(SUPPLIER_ID, creds.clone())?);
    *slot = Some((creds, sup.clone()));
    Ok(sup)
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
        min_margin_percent: s.min_margin_percent,
        overwrite_manual: s.overwrite_manual,
        fetch_since_only: s.fetch_since_only,
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
    let sup = supplier::open_for_check(SUPPLIER_ID, s.require_supplier()?)?;
    sup.login().await
}

// ------------------------------------------------------------ dashboard

#[tauri::command(rename_all = "snake_case")]
pub async fn dashboard_get(app: AppHandle, state: State<'_, AppState>) -> Result<Dashboard> {
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
        links_to_review: state.db.count_links_needing_review(SUPPLIER_ID)?,
        held_writes: state.db.unresolved_superseded()?.len() as i64,
        appimage_offer: appimage_offer(&app, &state),
        upgrade_notice: upgrade_notice_view(&app, &state.db)?,
    })
}

/// The kept upgrade notice as things stand now (plan 130 task 14): see [`notice::view`].
fn upgrade_notice_view(app: &AppHandle, db: &Db) -> Result<Option<notice::NoticeView>> {
    let Some(n) = notice::current(db)? else {
        return Ok(None);
    };
    let running = appimage_running().map(|a| a.image);
    let menu = appimage_layout(app).map(|l| l.image()).unwrap_or_default();
    notice::view(db, n, running.as_deref(), &menu).map(Some)
}

/// The seller has read the upgrade notice: it is not shown again (plan 130 task 14).
#[tauri::command(rename_all = "snake_case")]
pub async fn upgrade_notice_dismiss(state: State<'_, AppState>) -> Result<()> {
    notice::dismiss(&state.db)
}

/// At start-up: raise a threshold below the floor and build the upgrade notice when there is
/// something to say (plan 130 task 14, R-j, R-n). Best effort — a notice is never worth failing a
/// launch for; the gate reads the setting as the floor whatever happens here.
pub fn prepare_upgrade_notice(app: &AppHandle, db: &Db) {
    // The folders are read only when a new notice is about to be built, and never for longer than
    // a few seconds: a stale network mount under Downloads must not keep the app from opening.
    let found = match app.path().home_dir() {
        Ok(home) if cfg!(target_os = "linux") && notice::will_build(db).unwrap_or(false) => {
            let places = notice::usual_places(
                &home,
                app.path().download_dir().ok(),
                app.path().desktop_dir().ok(),
            );
            let running = appimage_running().map(|a| a.image);
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(notice::find_appimages(&places, running.as_deref()));
            });
            rx.recv_timeout(std::time::Duration::from_secs(3))
                .unwrap_or_else(|_| {
                    tracing::warn!(
                        "older AppImages not looked for: the folders did not answer in 3 s"
                    );
                    Vec::new()
                })
        }
        _ => Vec::new(),
    };
    match notice::prepare(db, &found) {
        Ok(Some(n)) => tracing::info!(
            "upgrade notice kept: from schema {:?}, threshold {:?} → {}, {} links unverified, {} other AppImages",
            n.from_schema,
            n.threshold_from,
            n.threshold_to,
            n.links_unverified,
            n.appimages.len()
        ),
        Ok(None) => {}
        Err(e) => tracing::warn!("upgrade notice not prepared: {e}"),
    }
}

// ------------------------------------------------------------ appimage

/// Settings key set once the seller answers "don't ask again" to the AppImage menu offer.
const APPIMAGE_DISMISSED: &str = "appimage_install_dismissed";

/// After the updater replaced the installed AppImage in place, point its menu entry at the new
/// version. Best effort: a menu entry is never worth failing a launch for.
pub fn refresh_appimage_entry(app: &AppHandle) {
    let (Some(running), Ok(layout)) = (appimage_running(), appimage_layout(app)) else {
        return;
    };
    let canon = |p: &std::path::Path| p.canonicalize().ok();
    if canon(&running.image).is_none() || canon(&running.image) != canon(&layout.image()) {
        return;
    }
    let version = app.package_info().version.to_string();
    match appimage::refresh_entry(&layout, &version) {
        Ok(true) => tracing::info!("menu entry now names {version}"),
        Ok(false) => {}
        Err(e) => tracing::warn!("menu entry not refreshed: {e}"),
    }
}

/// The AppImage runtime exports `APPIMAGE`/`APPDIR`; nothing else sets them, so their presence
/// on Linux is the whole "which install am I" detection.
fn appimage_running() -> Option<appimage::AppImage> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let image = std::env::var("APPIMAGE").ok();
    let appdir = std::env::var("APPDIR").ok();
    appimage::running(image.as_deref(), appdir.as_deref())
}

fn appimage_layout(app: &AppHandle) -> Result<appimage::Layout> {
    let home = app
        .path()
        .home_dir()
        .map_err(|e| BridgeError::Other(format!("home dir: {e}")))?;
    Ok(appimage::Layout::new(home))
}

fn appimage_offer(app: &AppHandle, state: &AppState) -> bool {
    let running = appimage_running();
    let Ok(layout) = appimage_layout(app) else {
        return false;
    };
    let dismissed = state
        .db
        .setting(APPIMAGE_DISMISSED)
        .ok()
        .flatten()
        .is_some();
    appimage::should_offer(running.as_ref(), &layout, dismissed)
}

/// Copy the running AppImage under `~/.local` and add it to the applications menu. Returns the
/// path the copy landed at.
#[tauri::command(rename_all = "snake_case")]
pub async fn appimage_install(app: AppHandle) -> Result<String> {
    let running = appimage_running()
        .ok_or_else(|| BridgeError::Other("برنامه از فایل AppImage اجرا نشده است.".into()))?;
    let layout = appimage_layout(&app)?;
    let version = app.package_info().version.to_string();
    let installed = appimage::install(&running, &layout, &version)?;
    // Most desktops watch the folder; the ones that keep a cache need a nudge. Best effort.
    if let Some(dir) = layout.desktop().parent() {
        let _ = std::process::Command::new("update-desktop-database")
            .arg(dir)
            .output();
    }
    tracing::info!("appimage installed at {}", installed.display());
    Ok(installed.display().to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn appimage_dismiss(state: State<'_, AppState>) -> Result<()> {
    state.db.set_setting(APPIMAGE_DISMISSED, "1")
}

// ------------------------------------------------------------ updates

const UPDATE_LAST_CHECK: &str = "update_last_check";

/// One update cycle, broadcast on `update://state`; the time is kept for Settings.
pub async fn run_update_cycle(app: &AppHandle, state: &AppState) -> UpdateState {
    let emit =
        |s: &UpdateState| jobs::emit_event(&Some(app.clone()), update::EVENT_UPDATE, s.clone());
    let s = state.updater.check_and_stage(&emit).await;
    if let Err(e) = state.db.set_setting(UPDATE_LAST_CHECK, &crate::db::now()) {
        tracing::warn!("update check time not kept: {e}");
    }
    s
}

fn update_view(app: &AppHandle, state: &AppState) -> Result<UpdateView> {
    Ok(UpdateView {
        current: app.package_info().version.to_string(),
        state: state.updater.state(),
        kind: update::install_kind(),
        auto_update: settings(state)?.auto_update,
        last_check: state.db.setting(UPDATE_LAST_CHECK)?,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_status(app: AppHandle, state: State<'_, AppState>) -> Result<UpdateView> {
    update_view(&app, &state)
}

/// The seller's «بررسی نسخهٔ جدید»: one cycle now, whatever `auto_update` says. It runs in the
/// background — a download can take long on a slow link — and reports on `update://state`.
#[tauri::command(rename_all = "snake_case")]
pub async fn update_check(app: AppHandle, state: State<'_, AppState>) -> Result<UpdateView> {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = handle.state::<AppState>();
        run_update_cycle(&handle, &state).await;
    });
    update_view(&app, &state)
}

/// «نصب و راه‌اندازی دوباره». Refused while a job is active; on Linux the new copy is started
/// here, on Windows the installer has already closed the app and starts it again.
#[tauri::command(rename_all = "snake_case")]
pub async fn update_install(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let job_active = state.db.running_job()?.is_some();
    let installed = state.updater.install(job_active, true).await;
    jobs::emit_event(
        &Some(app.clone()),
        update::EVENT_UPDATE,
        state.updater.state(),
    );
    installed?;
    app.restart()
}

// ------------------------------------------------------------ jobs

/// One job at a time (spec §7.3): a job paused in REVIEW still counts as active and must be
/// continued or cancelled before another can start.
fn assert_no_active_job(db: &Db) -> Result<()> {
    if let Some(running) = db.running_job()? {
        return Err(BridgeError::JobBusy(format!("#{}", running.id)));
    }
    Ok(())
}

/// `owns_row`: `true` when `job_id` is a row this call just created (`import_start`,
/// `refresh_start`) — on a lost `begin` race it must not stay an orphan RUNNING row, so it is
/// marked FAILED with the error recorded. `false` when the row is shared with whichever call
/// actually won the race (`import_continue`, resuming a job already sitting in REVIEW) — on a
/// lost race that row belongs to the winner and must be left untouched; only the `JobBusy` error
/// is returned.
fn spawn<F, Fut>(
    app: AppHandle,
    state: &AppState,
    job_id: i64,
    owns_row: bool,
    run: F,
) -> Result<()>
where
    F: FnOnce(JobCtx, Services) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<()>> + Send,
{
    let cancel = match state.jobs.begin(job_id) {
        Ok(c) => c,
        Err(e) => {
            if owns_row {
                state.db.set_job_status(job_id, "FAILED", "PREFLIGHT")?;
                state
                    .db
                    .record_error(Some(job_id), e.kind(), &e.message_fa(), None)?;
            }
            return Err(e);
        }
    };
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
    assert_no_active_job(&state.db)?;
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
        fetch_since_only: s.fetch_since_only,
        fetch_limit,
    };
    spawn(app, &state, job_id, true, move |ctx, svc| async move {
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
    if item.status != "REVIEW" {
        return Err(BridgeError::Other(
            "این قلم دیگر در انتظار تصمیم نیست.".into(),
        ));
    }
    let matcher = Matcher::new(state.db.clone(), SUPPLIER_ID);
    if decision.later {
        return Ok(()); // stays REVIEW; commit() skips it and the next run asks again
    }
    if decision.none {
        matcher.record_none(&item.name, &item.code)?;
        state.db.record_error(
            Some(item.job_id),
            kinds::NONE_DECISION,
            &format!(
                "«{}» در فهرست لاندا نیست؛ برای افزودن آن یک تیکت برای پشتیبانی لاندا ثبت کنید.",
                item.name
            ),
            Some(serde_json::json!({"code": item.code})),
        )?;
        // «not in Landau» resolves the item: its old link no longer holds a row (plan 130 R-e)
        return state.db.update_job_item(
            item.id,
            &[
                ("status", jobs::t("SKIPPED")),
                ("error_kind", jobs::t(kinds::NONE_DECISION)),
                ("error_msg", jobs::t("این قطعه در لاندا نیست.")),
                ("link_product", rusqlite::types::Value::Null),
            ],
        );
    }
    let product_id = decision
        .product_id
        .ok_or_else(|| BridgeError::Other("no product picked".into()))?;
    // R-d: judged by the core against the whole registry; a conflict needs its exact reasons
    // confirmed, whatever the screen sent
    let threshold = settings(&state)?.threshold;
    let kept = kept_snapshot(&state).await?;
    let judged = inspect::judge_pick(
        &kept.registry,
        &item.name,
        &item.code,
        &product_id,
        threshold,
    )?;
    jobs::import::pick(
        &state.db,
        &item,
        &judged,
        decision.confirmed.as_deref(),
        decision.authenticity,
    )
}

/// What a pick would be (R-d, R-k): for the supplier item `name`/`code`, each of `product_ids`
/// judged against the kept registry snapshot, with Barezpakhsh's price today, the cost the bridge
/// would write, and each candidate's row as Landau holds it. Reads only; nothing here is written.
#[tauri::command(rename_all = "snake_case")]
pub async fn review_inspect(
    state: State<'_, AppState>,
    name: String,
    code: String,
    product_ids: Vec<String>,
    authenticity: Option<crate::domain::Authenticity>,
) -> Result<Inspection> {
    let s = settings(&state)?;
    let (_, _, store) = s.require_landau()?;
    let client = landau(&s)?;
    let kept = kept_snapshot(&state).await?;
    let _one = state.inspecting.lock().await;
    let sup = inspector(&state, &s).await;
    Ok(inspect::inspect(
        &kept.registry,
        &kept.read_at,
        sup.as_deref().map_err(Clone::clone),
        &client,
        store,
        s.vat_percent,
        s.threshold,
        (&name, &code),
        &product_ids,
        authenticity.unwrap_or(s.default_authenticity),
    )
    .await)
}

/// The seller's free-text re-search. The typed text goes through the same normalisation as the
/// automatic pass (`Matcher::term`, with no code to strip) so pack markers and Arabic letterforms
/// do not skew the picker queries or the scores.
#[tauri::command(rename_all = "snake_case")]
pub async fn review_search(state: State<'_, AppState>, term: String) -> Result<Vec<Scored>> {
    let term = Matcher::term(&term, "");
    // the kept snapshot's local picker when there is one (slice 2 measured it equal to Landau's),
    // so every candidate found can be judged; else Landau's own
    let hits = if let Some(kept) = state.snapshot.latest() {
        let registry = kept.registry;
        let search = move |q: String, limit: usize, skip: usize| {
            std::future::ready(Ok(registry.search(&q, limit, skip)))
        };
        candidates(&search, &term).await?
    } else {
        let s = settings(&state)?;
        let client = Arc::new(landau(&s)?);
        let search = move |q: String, limit: usize, skip: usize| {
            let client = client.clone();
            async move { client.registry_search(&q, limit, skip).await }
        };
        candidates(&search, &term).await?
    };
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
    spawn(app, &state, job_id, false, move |ctx, svc| async move {
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
    assert_no_active_job(&state.db)?;
    let job_id = state
        .db
        .create_job("REFRESH", None, None, store, s.overwrite_manual)?;
    spawn(app, &state, job_id, true, move |ctx, svc| async move {
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
pub async fn job_cancel(app: AppHandle, state: State<'_, AppState>, job_id: i64) -> Result<bool> {
    if state.jobs.cancel(job_id) {
        return Ok(true);
    }
    // No registry entry for a job paused in REVIEW; cancel its row directly.
    let Some(job) = state.db.job(job_id)? else {
        return Ok(false);
    };
    if job.status != "REVIEW" {
        return Ok(false);
    }
    state.db.set_job_status(job_id, "CANCELLED", &job.phase)?;
    state.db.add_job_event(
        job_id,
        "INFO",
        &job.phase,
        "عملیات به درخواست شما متوقف شد",
        None,
    )?;
    jobs::emit_event(
        &Some(app),
        jobs::EVENT_DONE,
        serde_json::json!({"job_id": job_id, "status": "CANCELLED"}),
    );
    Ok(true)
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
    name_seen: String,
    code: String,
    product_id: String,
    confirmed: Option<Vec<Reason>>,
) -> Result<()> {
    // R-d as in review: judged by the core, a conflict confirmed exactly; the definition recorded
    // from the snapshot, so a re-import rebuilds the pick instead of losing it. The next job's
    // VALIDATE proves it like any stored pick.
    let threshold = settings(&state)?.threshold;
    let kept = kept_snapshot(&state).await?;
    let judged = inspect::judge_pick(&kept.registry, &name_seen, &code, &product_id, threshold)?;
    jobs::import::matches_pick(&state.db, &name_seen, &code, &judged, confirmed.as_deref())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn match_none(state: State<'_, AppState>, name_seen: String, code: String) -> Result<()> {
    state
        .db
        .set_match(SUPPLIER_ID, &name_seen, &code, None, "NONE", None)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn match_clear(state: State<'_, AppState>, name_seen: String) -> Result<()> {
    state.db.clear_match(SUPPLIER_ID, &name_seen)
}

/// Writes held for the seller (plan 130 R-o): a shelf row changed outside the bridge after a write
/// whose outcome was unknown. Every job holds the row and its invoice lines until one is settled.
#[tauri::command(rename_all = "snake_case")]
pub async fn intents_held(state: State<'_, AppState>) -> Result<Vec<HeldWrite>> {
    let mut out = Vec::new();
    for intent in state.db.unresolved_superseded()? {
        let mut product_name = None;
        let mut supplier_names = Vec::new();
        for id in &intent.items {
            if let Some(item) = state.db.job_item(*id)? {
                product_name = product_name.or(item.product_name.clone());
                supplier_names.push(item.name);
            }
        }
        out.push(HeldWrite {
            intent,
            product_name,
            supplier_names,
        });
    }
    Ok(out)
}

/// The seller settles a held write: `ON_SHELF` (the quantity is on the shelf: its invoice lines
/// count as transferred) or `TRANSFER_AGAIN` (the next import transfers them). Refused while a
/// job runs.
#[tauri::command(rename_all = "snake_case")]
pub async fn intent_resolve(
    state: State<'_, AppState>,
    id: i64,
    resolution: String,
) -> Result<bool> {
    state.db.resolve_intent(id, &resolution)
}

/// The audit (plan 130 R-i): every shelf row weighed against the evidence, read only — the registry
/// read whole now (kept for the screens when newer), the links judged on a copy of the database,
/// Barezpakhsh quoted under the screens' one-inspection lock where a quote decides something.
#[tauri::command(rename_all = "snake_case")]
pub async fn audit_run(state: State<'_, AppState>) -> Result<crate::audit::Audit> {
    let s = settings(&state)?;
    let (_, _, store) = s.require_landau()?;
    let client = landau(&s)?;
    let started = crate::db::now();
    let registry = Arc::new(jobs::read_checked(&state.db, &client).await?);
    state.snapshot.keep(registry.clone(), started.clone());
    let sup = inspector(&state, &s).await;
    let audit = crate::audit::audit(&crate::audit::Sources {
        db: &state.db,
        registry: &registry,
        registry_read_at: &started,
        landau: &client,
        store,
        supplier: sup.as_deref().map_err(Clone::clone),
        // only each quote waits for the screens' one-at-a-time session, not the whole audit
        quote_lock: Some(&state.inspecting),
        vat_percent: s.vat_percent,
        threshold: s.threshold,
    })
    .await?;
    *state.audit.lock().await = Some(audit.clone());
    Ok(audit)
}

/// The last audit as CSV into `path` (from the native save dialog); answers how many rows: the
/// doubts and the automatic writes, a row in both counted twice, as the file lists it.
#[tauri::command(rename_all = "snake_case")]
pub async fn audit_export(state: State<'_, AppState>, path: String) -> Result<usize> {
    let kept = state.audit.lock().await;
    let audit = kept
        .as_ref()
        .ok_or_else(|| BridgeError::Other("هنوز بازبینی‌ای انجام نشده است.".into()))?;
    std::fs::write(&path, crate::audit::csv(audit))
        .map_err(|e| BridgeError::Other(format!("ذخیرهٔ فایل بازبینی ممکن نشد: {e}")))?;
    Ok(audit.rows.len() + audit.automatic.len())
}

/// The «بدون تصمیم» list: parts ended jobs left undecided, with the candidates review showed.
#[tauri::command(rename_all = "snake_case")]
pub async fn matches_undecided(state: State<'_, AppState>) -> Result<Vec<UndecidedRow>> {
    state.db.undecided_matches(SUPPLIER_ID)
}

/// Every decision into `path` (from the native save dialog) as a versioned JSON file; answers how
/// many were written.
#[tauri::command(rename_all = "snake_case")]
pub async fn decisions_export(state: State<'_, AppState>, path: String) -> Result<usize> {
    let rows = state.db.list_matches(SUPPLIER_ID)?;
    let file = decisions::export(SUPPLIER_ID, &rows, &crate::db::now());
    std::fs::write(&path, serde_json::to_string_pretty(&file)?)
        .map_err(|e| BridgeError::Other(format!("ذخیرهٔ فایل تصمیم‌ها ممکن نشد: {e}")))?;
    Ok(rows.len())
}

/// Merges a decision file (from the native open dialog) into this installation — see
/// [`decisions`] for the rules. Products are checked against this Landau's registry before
/// anything is written, and the write is one transaction, so a Landau out of reach leaves the
/// decisions here exactly as they were.
#[tauri::command(rename_all = "snake_case")]
pub async fn decisions_import(
    state: State<'_, AppState>,
    path: String,
    prefer_file: bool,
) -> Result<ImportReport> {
    let text = std::fs::read_to_string(&path)
        .map_err(|e| BridgeError::Other(format!("خواندن فایل تصمیم‌ها ممکن نشد: {e}")))?;
    let file = decisions::parse(&text, SUPPLIER_ID)?;
    let plan = decisions::plan(
        &file,
        |name| state.db.match_for_name(SUPPLIER_ID, name),
        prefer_file,
    )?;
    let plan = if plan.writes.iter().any(|(d, _)| d.product_id.is_some()) {
        let client = landau(&settings(&state)?)?;
        let client = &client;
        let missing = decisions::missing_products(
            &plan.writes,
            |term, skip| async move { client.registry_search(&term, 100, skip).await },
            4,
        )
        .await?;
        decisions::without_missing(plan, &missing)
    } else {
        plan
    };
    let writes: Vec<decisions::Decision> = plan.writes.iter().map(|(d, _)| d.clone()).collect();
    state.db.apply_decisions(SUPPLIER_ID, &writes)?;
    Ok(plan.report)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_no_active_job_rejects_a_job_paused_in_review() {
        let db = Db::in_memory().unwrap();
        assert!(assert_no_active_job(&db).is_ok());
        let job_id = db
            .create_job("IMPORT", Some("ZERO"), None, "store1", false)
            .unwrap();
        db.set_job_status(job_id, "REVIEW", "MATCH").unwrap();
        assert!(matches!(
            assert_no_active_job(&db).unwrap_err(),
            BridgeError::JobBusy(_)
        ));
    }
}
