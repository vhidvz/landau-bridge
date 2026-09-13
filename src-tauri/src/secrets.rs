//! Where the Landau key and the supplier password live. Keyring when the OS offers one, else a
//! 0600 JSON file next to the database — and the settings screen says which (spec §3).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::errors::{BridgeError, Result};

const SERVICE: &str = "ir.landau.bridge";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKey {
    LandauToken,
    SupplierPassword,
}

impl SecretKey {
    fn name(self) -> &'static str {
        match self {
            SecretKey::LandauToken => "landau_token",
            SecretKey::SupplierPassword => "supplier_password",
        }
    }
}

#[derive(Clone)]
pub struct SecretStore {
    keyring: bool,
    file: PathBuf,
}

impl SecretStore {
    /// Probe the keyring once; fall back to the file store when it is unusable.
    pub fn open(data_dir: &Path) -> SecretStore {
        let file = data_dir.join("secrets.json");
        let keyring = match keyring::Entry::new(SERVICE, "probe") {
            Ok(entry) => entry
                .set_password("probe")
                .and_then(|_| entry.delete_credential())
                .is_ok(),
            Err(_) => false,
        };
        if !keyring {
            tracing::warn!("keyring unavailable; secrets stored in {}", file.display());
        }
        SecretStore { keyring, file }
    }

    /// For tests and for machines with no secret service.
    pub fn file_only(data_dir: &Path) -> SecretStore {
        SecretStore {
            keyring: false,
            file: data_dir.join("secrets.json"),
        }
    }

    pub fn backend(&self) -> &'static str {
        if self.keyring {
            "keyring"
        } else {
            "file"
        }
    }

    pub fn get(&self, key: SecretKey) -> Result<Option<String>> {
        if self.keyring {
            return match keyring::Entry::new(SERVICE, key.name()).and_then(|e| e.get_password()) {
                Ok(v) => Ok(Some(v)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(BridgeError::KeyringUnavailable(e.to_string())),
            };
        }
        Ok(self.read_file()?.get(key.name()).cloned())
    }

    pub fn set(&self, key: SecretKey, value: &str) -> Result<()> {
        if self.keyring {
            return keyring::Entry::new(SERVICE, key.name())
                .and_then(|e| e.set_password(value))
                .map_err(|e| BridgeError::KeyringUnavailable(e.to_string()));
        }
        let mut map = self.read_file()?;
        map.insert(key.name().to_string(), value.to_string());
        self.write_file(&map)
    }

    pub fn delete(&self, key: SecretKey) -> Result<()> {
        if self.keyring {
            return match keyring::Entry::new(SERVICE, key.name())
                .and_then(|e| e.delete_credential())
            {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(BridgeError::KeyringUnavailable(e.to_string())),
            };
        }
        let mut map = self.read_file()?;
        map.remove(key.name());
        self.write_file(&map)
    }

    fn read_file(&self) -> Result<BTreeMap<String, String>> {
        match std::fs::read_to_string(&self.file) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| BridgeError::Other(format!("secrets.json: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(BridgeError::Other(format!("secrets.json: {e}"))),
        }
    }

    fn write_file(&self, map: &BTreeMap<String, String>) -> Result<()> {
        use std::io::Write;
        let parent = self
            .file
            .parent()
            .ok_or_else(|| BridgeError::Other("secrets.json has no parent directory".into()))?;
        std::fs::create_dir_all(parent).map_err(|e| BridgeError::Other(e.to_string()))?;
        let text = serde_json::to_string_pretty(map)?;
        let tmp = parent.join("secrets.json.tmp");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options
            .open(&tmp)
            .map_err(|e| BridgeError::Other(e.to_string()))?;
        f.write_all(text.as_bytes())
            .and_then(|_| f.sync_all())
            .map_err(|e| BridgeError::Other(e.to_string()))?;
        drop(f);
        std::fs::rename(&tmp, &self.file).map_err(|e| BridgeError::Other(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.file, std::fs::Permissions::from_mode(0o600));
            // belt and braces for a pre-existing file
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_store_round_trips_and_is_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let store = SecretStore::file_only(dir.path());
        assert_eq!(store.backend(), "file");
        assert_eq!(store.get(SecretKey::LandauToken).unwrap(), None);
        store.set(SecretKey::LandauToken, "apt-secret").unwrap();
        store.set(SecretKey::SupplierPassword, "pw").unwrap();
        assert_eq!(
            store.get(SecretKey::LandauToken).unwrap().as_deref(),
            Some("apt-secret")
        );
        store.delete(SecretKey::LandauToken).unwrap();
        assert_eq!(store.get(SecretKey::LandauToken).unwrap(), None);
        assert_eq!(
            store.get(SecretKey::SupplierPassword).unwrap().as_deref(),
            Some("pw")
        );
        assert!(
            !dir.path().join("secrets.json.tmp").exists(),
            "temp file is renamed away"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("secrets.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "secrets.json must be owner-only");
        }
    }
}
