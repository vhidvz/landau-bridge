//! Landau Bridge — Rust core. See docs/superpowers/specs/2026-09-13-landau-bridge-design.md.

pub mod appimage;
pub mod commands;
pub mod db;
pub mod domain;
pub mod errors;
pub mod jobs;
pub mod landau;
pub mod matching;
pub mod secrets;
pub mod settings;
pub mod supplier;

use std::sync::Arc;

use tauri::Manager;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
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

            let db = db::Db::open(&data_dir.join("bridge.sqlite")).expect("open database");
            for id in db.interrupt_stale_jobs().expect("stale jobs") {
                let _ = db.record_error(
                    Some(id),
                    errors::kinds::JOB_INTERRUPTED,
                    "برنامه در میانه این عملیات بسته شده بود؛ می‌توانید دوباره اجرا کنید.",
                    None,
                );
            }
            let secrets = secrets::SecretStore::open(&data_dir);
            app.manage(AppState {
                db,
                secrets,
                jobs: Arc::new(jobs::JobRegistry::default()),
                data_dir,
                _log_guard: guard,
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
            commands::match_set,
            commands::match_none,
            commands::match_clear,
            commands::errors_list,
            commands::open_log_folder,
            commands::appimage_install,
            commands::appimage_dismiss,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
