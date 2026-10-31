//! Landau Bridge — Rust core.

pub mod appimage;
pub mod commands;
pub mod db;
pub mod decisions;
pub mod domain;
pub mod errors;
pub mod jobs;
pub mod landau;
pub mod matching;
pub mod secrets;
pub mod settings;
pub mod supplier;
pub mod update;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use commands::AppState;

/// `landau-bridge --self-check <dir>`: open (and so create and migrate) a database in `dir` and
/// answer its schema version, with no window — what CI runs on every built program.
pub fn self_check(dir: &Path) -> errors::Result<i64> {
    db::Db::open(&dir.join("bridge.sqlite"))?.user_version()
}

/// Closing the app installs a staged update when [`update::quit_action`] says so — never during a
/// job, never a deb/rpm (it asks for a password). Runs once, whatever fires the exit.
fn install_on_exit(app: &AppHandle) {
    static DONE: AtomicBool = AtomicBool::new(false);
    if DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        return; // the database was refused: nothing was set up, nothing to install
    };
    let job_active = state.db.running_job().map(|j| j.is_some()).unwrap_or(true);
    let action = update::quit_action(
        update::install_kind(),
        &state.updater.state(),
        job_active,
        state.updater.declined(),
    );
    if action == update::QuitAction::InstallThenExit {
        tracing::info!("installing the staged update before exit");
        if let Err(e) = tauri::async_runtime::block_on(state.updater.install(false, false)) {
            tracing::warn!("update not installed on exit: {e}");
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(data_dir.join("logs")).expect("create data dir");

            let file = tracing_appender::rolling::daily(data_dir.join("logs"), "bridge.log");
            let (writer, guard) = tracing_appender::non_blocking(file);
            let filter = tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,landau_bridge_lib=debug".into());
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(writer)
                .with_ansi(false)
                .init();
            tracing::info!("landau-bridge starting; data dir {}", data_dir.display());

            let db = match db::Db::open(&data_dir.join("bridge.sqlite")) {
                Ok(db) => db,
                Err(e @ errors::BridgeError::DbNewer { .. }) => {
                    // An older build over a newer build's data: say so and leave the file alone.
                    tracing::error!("{e}");
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.hide();
                    }
                    let handle = app.handle().clone();
                    app.dialog()
                        .message(e.message_fa())
                        .title("پل لاندا")
                        .kind(MessageDialogKind::Error)
                        .show(move |_| handle.exit(1));
                    return Ok(());
                }
                Err(e) => panic!("open database: {e}"),
            };
            for id in db.interrupt_stale_jobs().expect("stale jobs") {
                let _ = db.record_error(
                    Some(id),
                    errors::kinds::JOB_INTERRUPTED,
                    "برنامه در میانه این عملیات بسته شده بود؛ می‌توانید دوباره اجرا کنید.",
                    None,
                );
            }
            let secrets = secrets::SecretStore::open(&data_dir);
            let updater = Arc::new(update::Updater::new(
                update::PluginSource::new(app.handle().clone()),
                update::Staging::new(&data_dir),
            ));
            app.manage(AppState {
                db,
                secrets,
                jobs: Arc::new(jobs::JobRegistry::default()),
                data_dir,
                updater,
                _log_guard: guard,
            });
            commands::refresh_appimage_entry(app.handle());

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(update::FIRST_CHECK).await;
                loop {
                    let state = handle.state::<AppState>();
                    let auto = settings::Settings::load(&state.db, &state.secrets)
                        .map(|s| s.auto_update)
                        .unwrap_or(true);
                    if auto {
                        commands::run_update_cycle(&handle, &state).await;
                    }
                    tokio::time::sleep(update::CHECK_EVERY).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings_get,
            commands::settings_save,
            commands::landau_validate,
            commands::supplier_test_login,
            commands::dashboard_get,
            commands::import_start,
            commands::import_review,
            commands::review_decide,
            commands::review_search,
            commands::import_continue,
            commands::refresh_start,
            commands::job_cancel,
            commands::job_current,
            commands::jobs_list,
            commands::job_get,
            commands::matches_list,
            commands::matches_undecided,
            commands::decisions_export,
            commands::decisions_import,
            commands::match_set,
            commands::match_none,
            commands::match_clear,
            commands::errors_list,
            commands::open_log_folder,
            commands::appimage_install,
            commands::appimage_dismiss,
            commands::update_status,
            commands::update_check,
            commands::update_install,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                install_on_exit(app);
            }
        });
}
