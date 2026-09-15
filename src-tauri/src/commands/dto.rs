//! Wire shapes between the Rust core and the Nuxt pages. Mirrored in app/types/bridge.ts.

use serde::{Deserialize, Serialize};

use crate::db::name_map::NameMapInfo;
use crate::db::jobs::JobRow;
use crate::domain::Authenticity;
use crate::landau::Store;

#[derive(Debug, Clone, Serialize)]
pub struct SettingsView {
    pub landau_url: String,
    pub has_landau_token: bool,
    pub store: Option<String>,
    pub store_name: Option<String>,
    pub supplier_user: Option<String>,
    pub has_supplier_password: bool,
    pub default_authenticity: Authenticity,
    pub threshold: u8,
    pub vat_percent: f64,
    pub min_margin_percent: f64,
    pub overwrite_manual: bool,
    pub fetch_since_only: bool,
    /// The landau-parts map loaded from Settings (spec §14 c).
    pub name_map: NameMapInfo,
    pub secret_backend: String,
    pub data_dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LandauStatus {
    pub business: String,
    pub business_name: Option<String>,
    pub approved: bool,
    pub stores: Vec<Store>,
    pub active_rules: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Dashboard {
    pub settings_complete: bool,
    pub store_name: Option<String>,
    pub supplier_user: Option<String>,
    pub invoices_cached: i64,
    pub invoices_unpaid: i64,
    pub last_import: Option<JobRow>,
    pub last_refresh: Option<JobRow>,
    pub running: Option<JobRow>,
    pub pending_review: i64,
    pub recent_errors: i64,
    pub decided_matches: i64,
    /// Linux only: running from an AppImage that is not in the applications menu yet.
    pub appimage_offer: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewDecision {
    pub item_id: i64,
    /// Pick this registry part…
    pub product_id: Option<String>,
    pub product_name: Option<String>,
    pub similarity: Option<i64>,
    /// …or «هیچ‌کدام» (open a Landau ticket)…
    #[serde(default)]
    pub none: bool,
    /// …or leave it for next time.
    #[serde(default)]
    pub later: bool,
    pub authenticity: Option<Authenticity>,
}
