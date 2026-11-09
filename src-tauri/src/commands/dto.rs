//! Wire shapes between the Rust core and the Nuxt pages. Mirrored in app/types/bridge.ts.

use serde::{Deserialize, Serialize};

use crate::db::jobs::JobRow;
use crate::domain::Authenticity;
use crate::landau::Store;
use crate::matching::gate::Reason;
use crate::update::{InstallKind, UpdateState};

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
    pub secret_backend: String,
    pub data_dir: String,
}

/// The Settings card «نسخهٔ برنامه».
#[derive(Debug, Clone, Serialize)]
pub struct UpdateView {
    pub current: String,
    pub state: UpdateState,
    pub kind: InstallKind,
    pub auto_update: bool,
    pub last_check: Option<String>,
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
    /// Links a job held for the seller: `NEEDS_REVIEW` or `STALE` (plan 130 task 11).
    pub links_to_review: i64,
    /// Shelf writes held until the seller settles them (plan 130 R-o).
    pub held_writes: i64,
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
    /// The reasons the seller confirmed in the second confirm (R-d) — exactly the ones the core
    /// answered `ConfirmRequired` with; absent for a first, unconfirmed attempt.
    pub confirmed: Option<Vec<Reason>>,
}

/// A held shelf write as the dashboard lists it (plan 130 R-o): the intent and the part's name.
#[derive(Debug, Clone, Serialize)]
pub struct HeldWrite {
    #[serde(flatten)]
    pub intent: crate::db::intents::IntentRow,
    pub product_name: Option<String>,
    pub supplier_names: Vec<String>,
}
