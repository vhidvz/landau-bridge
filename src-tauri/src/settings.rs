//! Typed view over the `settings` table plus the secret store (spec §3, §10 screen 1).

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::domain::Authenticity;
use crate::errors::{BridgeError, Result};
use crate::secrets::{SecretKey, SecretStore};
use crate::supplier::Credentials;

/// Replaced by the calibrated value in Task 10 (docs/matching-threshold.md).
pub const DEFAULT_THRESHOLD: u8 = 80;
pub const DEFAULT_VAT_PERCENT: f64 = 10.0;
pub const SUPPLIER_ID: &str = "barezpakhsh";

#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    pub landau_url: String,
    #[serde(skip_serializing)]
    pub landau_token: Option<String>,
    pub store: Option<String>,
    pub store_name: Option<String>,
    pub supplier_user: Option<String>,
    #[serde(skip_serializing)]
    pub supplier_password: Option<String>,
    pub default_authenticity: Authenticity,
    pub threshold: u8,
    pub vat_percent: f64,
    pub overwrite_manual: bool,
}

/// Fields a save may carry. `None` = untouched; an empty string on a secret = untouched too.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SettingsPatch {
    pub landau_url: Option<String>,
    pub landau_token: Option<String>,
    pub store: Option<String>,
    pub store_name: Option<String>,
    pub supplier_user: Option<String>,
    pub supplier_password: Option<String>,
    pub default_authenticity: Option<Authenticity>,
    pub threshold: Option<u8>,
    pub vat_percent: Option<f64>,
    pub overwrite_manual: Option<bool>,
}

impl Settings {
    pub fn load(db: &Db, secrets: &SecretStore) -> Result<Settings> {
        let get = |k: &str| db.setting(k);
        Ok(Settings {
            landau_url: get("landau_url")?.unwrap_or_else(|| "http://localhost:6150".into()),
            landau_token: secrets.get(SecretKey::LandauToken)?,
            store: get("store")?,
            store_name: get("store_name")?,
            supplier_user: get("supplier_user")?,
            supplier_password: secrets.get(SecretKey::SupplierPassword)?,
            default_authenticity: get("default_authenticity")?
                .and_then(|v| Authenticity::parse(&v))
                .unwrap_or(Authenticity::Original),
            threshold: get("threshold")?
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_THRESHOLD),
            vat_percent: get("vat_percent")?
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_VAT_PERCENT),
            overwrite_manual: get("overwrite_manual")?.map(|v| v == "1").unwrap_or(false),
        })
    }

    pub fn save(db: &Db, secrets: &SecretStore, p: &SettingsPatch) -> Result<()> {
        if let Some(v) = &p.landau_url {
            db.set_setting("landau_url", v.trim().trim_end_matches('/'))?;
        }
        if let Some(v) = &p.landau_token {
            if !v.trim().is_empty() {
                secrets.set(SecretKey::LandauToken, v.trim())?;
            }
        }
        if let Some(v) = &p.store {
            db.set_setting("store", v)?;
        }
        if let Some(v) = &p.store_name {
            db.set_setting("store_name", v)?;
        }
        if let Some(v) = &p.supplier_user {
            db.set_setting("supplier_user", v.trim())?;
        }
        if let Some(v) = &p.supplier_password {
            if !v.is_empty() {
                secrets.set(SecretKey::SupplierPassword, v)?;
            }
        }
        if let Some(v) = p.default_authenticity {
            db.set_setting("default_authenticity", v.as_str())?;
        }
        if let Some(v) = p.threshold {
            db.set_setting("threshold", &v.min(100).to_string())?;
        }
        if let Some(v) = p.vat_percent {
            db.set_setting("vat_percent", &v.to_string())?;
        }
        if let Some(v) = p.overwrite_manual {
            db.set_setting("overwrite_manual", if v { "1" } else { "0" })?;
        }
        Ok(())
    }

    /// `(url, token, store)` or `SettingsIncomplete` naming the missing piece.
    pub fn require_landau(&self) -> Result<(&str, &str, &str)> {
        let token = self
            .landau_token
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| BridgeError::SettingsIncomplete("کلید API لندو".into()))?;
        let store = self
            .store
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| BridgeError::SettingsIncomplete("فروشگاه مقصد در لندو".into()))?;
        Ok((self.landau_url.as_str(), token, store))
    }

    pub fn require_supplier(&self) -> Result<Credentials> {
        let username = self
            .supplier_user
            .clone()
            .filter(|u| !u.is_empty())
            .ok_or_else(|| BridgeError::SettingsIncomplete("نام کاربری بارزپخش".into()))?;
        let password = self
            .supplier_password
            .clone()
            .filter(|p| !p.is_empty())
            .ok_or_else(|| BridgeError::SettingsIncomplete("کلمه عبور بارزپخش".into()))?;
        Ok(Credentials { username, password })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::secrets::SecretStore;

    fn stores() -> (Db, SecretStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (
            Db::in_memory().unwrap(),
            SecretStore::file_only(dir.path()),
            dir,
        )
    }

    #[test]
    fn defaults_are_the_spec_values() {
        let (db, sec, _d) = stores();
        let s = Settings::load(&db, &sec).unwrap();
        assert_eq!(s.threshold, DEFAULT_THRESHOLD);
        assert_eq!(s.vat_percent, 10.0);
        assert_eq!(s.default_authenticity, Authenticity::Original);
        assert!(!s.overwrite_manual);
        assert!(s.landau_token.is_none());
        assert!(s.require_landau().is_err());
    }

    #[test]
    fn save_keeps_secrets_out_of_the_settings_table_and_empty_means_keep() {
        let (db, sec, _d) = stores();
        Settings::save(
            &db,
            &sec,
            &SettingsPatch {
                landau_url: Some("http://localhost:6150".into()),
                landau_token: Some("apt-abc".into()),
                store: Some("s1".into()),
                supplier_user: Some("0913".into()),
                supplier_password: Some("pw".into()),
                threshold: Some(77),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(db.setting("landau_token").unwrap(), None);
        let s = Settings::load(&db, &sec).unwrap();
        assert_eq!(s.landau_token.as_deref(), Some("apt-abc"));
        assert_eq!(s.threshold, 77);
        let (url, tok, store) = s.require_landau().unwrap();
        assert_eq!(
            (url, tok, store),
            ("http://localhost:6150", "apt-abc", "s1")
        );

        Settings::save(
            &db,
            &sec,
            &SettingsPatch {
                landau_token: Some(String::new()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            Settings::load(&db, &sec).unwrap().landau_token.as_deref(),
            Some("apt-abc")
        );
        let creds = Settings::load(&db, &sec)
            .unwrap()
            .require_supplier()
            .unwrap();
        assert_eq!(creds.username, "0913");
    }
}
