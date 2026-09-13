//! Landau Bridge — Rust core. See docs/superpowers/specs/2026-09-13-landau-bridge-design.md.

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
