//! Self-update (.superpowers/specs/2026-10-30-lnd-bridge-release-and-update-design.md §4): find a
//! newer release, download and stage it, install it when no job runs. The plugin's network and
//! install calls sit behind [`Source`], so every decision here is tested without a network.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use serde::Serialize;

use crate::errors::{BridgeError, Result};

/// How this copy was installed — it decides whether an update may go in as the app closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallKind {
    AppImage,
    Nsis,
    Deb,
    Rpm,
    Unknown,
}

pub fn install_kind() -> InstallKind {
    use tauri::utils::config::BundleType;
    match tauri::utils::platform::bundle_type() {
        Some(BundleType::AppImage) => InstallKind::AppImage,
        Some(BundleType::Nsis) => InstallKind::Nsis,
        Some(BundleType::Deb) => InstallKind::Deb,
        Some(BundleType::Rpm) => InstallKind::Rpm,
        _ => InstallKind::Unknown,
    }
}

/// AppImage and NSIS install without asking anything; deb and rpm need an administrator's
/// password, so they install only when the seller presses the button.
pub fn installs_on_quit(kind: InstallKind) -> bool {
    matches!(kind, InstallKind::AppImage | InstallKind::Nsis)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state")]
pub enum UpdateState {
    Idle,
    Checking,
    Available {
        version: String,
    },
    Downloading {
        version: String,
        percent: u8,
    },
    Ready {
        version: String,
    },
    Installing {
        version: String,
    },
    Failed {
        kind: String,
        message: String,
        at: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailKind {
    /// GitHub out of reach, a timeout, a cut download.
    Network,
    /// The package does not match the release key: never installed, never retried silently.
    Signature,
    /// The seller cancelled the administrator prompt (deb/rpm).
    Declined,
    Install,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub kind: FailKind,
    pub detail: String,
}

impl Failure {
    pub fn new(kind: FailKind, detail: impl Into<String>) -> Failure {
        Failure {
            kind,
            detail: detail.into(),
        }
    }

    pub fn message_fa(&self) -> String {
        match self.kind {
            FailKind::Network => "اتصال به GitHub برقرار نشد؛ بعداً دوباره بررسی می‌شود.",
            FailKind::Signature => "امضای فایل دریافتی معتبر نبود؛ نصب نشد.",
            FailKind::Declined => "نصب لغو شد؛ نسخهٔ فعلی سالم است.",
            FailKind::Install => "نصب نسخهٔ جدید انجام نشد؛ نسخهٔ فعلی سالم است.",
            FailKind::Other => "بررسی نسخهٔ جدید انجام نشد؛ بعداً دوباره بررسی می‌شود.",
        }
        .into()
    }
}

#[async_trait]
pub trait Source: Send + Sync {
    async fn check(&self) -> std::result::Result<Option<Offer>, Failure>;
    async fn download(
        &self,
        offer: &Offer,
        progress: &(dyn Fn(u8) + Send + Sync),
    ) -> std::result::Result<Vec<u8>, Failure>;
    /// Check a staged package against the offer's signature before trusting it again.
    async fn verify(&self, offer: &Offer, bytes: &[u8]) -> std::result::Result<(), Failure>;
    async fn install(
        &self,
        offer: &Offer,
        bytes: Vec<u8>,
        restart: bool,
    ) -> std::result::Result<(), Failure>;
}

/// Downloaded packages, `<data dir>/updates/<version>.pkg`, so a slow link pays for a version
/// once. A file is complete only under that name (written as `.part`, then renamed), and the
/// updater verifies its signature again before installing it.
pub struct Staging {
    dir: PathBuf,
}

impl Staging {
    pub fn new(data_dir: &Path) -> Staging {
        Staging {
            dir: data_dir.join("updates"),
        }
    }

    fn path(&self, version: &str) -> PathBuf {
        self.dir.join(format!("{version}.pkg"))
    }

    pub fn store(&self, version: &str, bytes: &[u8]) -> Result<PathBuf> {
        use std::io::Write;
        let io = |e: std::io::Error| BridgeError::Other(format!("staging {version}: {e}"));
        std::fs::create_dir_all(&self.dir).map_err(io)?;
        let part = self.dir.join(format!("{version}.part"));
        let mut f = std::fs::File::create(&part).map_err(io)?;
        f.write_all(bytes).map_err(io)?;
        f.sync_all().map_err(io)?;
        let target = self.path(version);
        std::fs::rename(&part, &target).map_err(io)?;
        Ok(target)
    }

    pub fn load(&self, version: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(version)).ok()
    }

    /// Delete every staged file except `keep`'s.
    pub fn prune(&self, keep: Option<&str>) {
        let keep = keep.map(|v| format!("{v}.pkg"));
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for e in entries.flatten() {
            if Some(e.file_name().to_string_lossy().as_ref()) != keep.as_deref() {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

pub struct Updater<S: Source> {
    source: S,
    staging: Staging,
    state: Mutex<UpdateState>,
    offer: Mutex<Option<Offer>>,
    declined: Mutex<Option<String>>,
    /// A version whose package failed its signature: not fetched again this run.
    rejected: Mutex<Option<String>>,
    cycle: tokio::sync::Mutex<()>,
    /// Held for the whole install, so closing the app can wait for one in flight.
    installing: tokio::sync::Mutex<()>,
}

impl<S: Source> Updater<S> {
    pub fn new(source: S, staging: Staging) -> Updater<S> {
        Updater {
            source,
            staging,
            state: Mutex::new(UpdateState::Idle),
            offer: Mutex::new(None),
            declined: Mutex::new(None),
            rejected: Mutex::new(None),
            cycle: tokio::sync::Mutex::new(()),
            installing: tokio::sync::Mutex::new(()),
        }
    }

    pub fn state(&self) -> UpdateState {
        lock(&self.state).clone()
    }

    fn set(&self, s: UpdateState, on_state: &(dyn Fn(&UpdateState) + Send + Sync)) -> UpdateState {
        *lock(&self.state) = s.clone();
        on_state(&s);
        s
    }

    fn failed(&self, f: &Failure) -> UpdateState {
        tracing::warn!(kind = ?f.kind, detail = %f.detail, "update failed");
        UpdateState::Failed {
            kind: format!("{:?}", f.kind),
            message: f.message_fa(),
            at: crate::db::now(),
        }
    }

    /// The staged version failed to install (or was declined) this run: closing the app must not
    /// try it again.
    pub fn declined(&self) -> bool {
        match self.state() {
            UpdateState::Ready { version } => lock(&self.declined).as_deref() == Some(&version),
            _ => false,
        }
    }

    /// One cycle: check, then download and stage a newer version. Never fails — a failure becomes
    /// [`UpdateState::Failed`] and the current version keeps running. A cycle already running is
    /// not doubled; the caller gets the state as it stands.
    pub async fn check_and_stage(
        &self,
        on_state: &(dyn Fn(&UpdateState) + Send + Sync),
    ) -> UpdateState {
        let Ok(_cycle) = self.cycle.try_lock() else {
            return self.state();
        };
        if matches!(self.state(), UpdateState::Installing { .. }) {
            return self.state();
        }
        self.set(UpdateState::Checking, on_state);
        let offer = match self.source.check().await {
            Err(f) => return self.set(self.failed(&f), on_state),
            Ok(None) => {
                *lock(&self.offer) = None;
                self.staging.prune(None);
                return self.set(UpdateState::Idle, on_state);
            }
            Ok(Some(offer)) => offer,
        };
        let version = offer.version.clone();
        *lock(&self.offer) = Some(offer.clone());
        self.set(
            UpdateState::Available {
                version: version.clone(),
            },
            on_state,
        );
        if lock(&self.rejected).as_deref() == Some(&version) {
            let f = Failure::new(
                FailKind::Signature,
                format!("{version} was rejected this run"),
            );
            return self.set(self.failed(&f), on_state);
        }
        // A staged package is trusted only after it verifies against this offer's signature: a
        // file that rotted on disk, or a release whose assets were replaced, is fetched again.
        let staged = match self.staging.load(&version) {
            Some(bytes) => match self.source.verify(&offer, &bytes).await {
                Ok(()) => true,
                Err(f) => {
                    tracing::warn!(detail = %f.detail, "staged {version} failed verification; fetching it again");
                    self.staging.prune(None);
                    false
                }
            },
            None => false,
        };
        if !staged {
            let last = Mutex::new(None::<u8>);
            let progress = |percent: u8| {
                let mut last = lock(&last);
                if *last != Some(percent) {
                    *last = Some(percent);
                    self.set(
                        UpdateState::Downloading {
                            version: version.clone(),
                            percent,
                        },
                        on_state,
                    );
                }
            };
            progress(0);
            let bytes = match self.source.download(&offer, &progress).await {
                Ok(bytes) => bytes,
                Err(f) => {
                    if f.kind == FailKind::Signature {
                        *lock(&self.rejected) = Some(version);
                    }
                    return self.set(self.failed(&f), on_state);
                }
            };
            if let Err(e) = self.staging.store(&version, &bytes) {
                let f = Failure::new(FailKind::Other, e.to_string());
                return self.set(self.failed(&f), on_state);
            }
        }
        self.staging.prune(Some(&version));
        self.set(UpdateState::Ready { version }, on_state)
    }

    /// Wait for an install in flight to finish (closing the app mid-write would leave a cut file).
    pub async fn settle(&self) {
        let _done = self.installing.lock().await;
    }

    /// Install the staged version. Refused while a job is active (one paused in review included)
    /// and while a check runs (it could overwrite the state this sets).
    pub async fn install(&self, job_active: bool, restart: bool) -> Result<()> {
        if job_active {
            return Err(BridgeError::JobBusy(
                "نسخهٔ جدید را پس از پایان یا لغو آن نصب کنید.".into(),
            ));
        }
        let Ok(_installing) = self.installing.try_lock() else {
            return Err(BridgeError::Other("نسخهٔ جدید در حال نصب است.".into()));
        };
        let Ok(_no_check) = self.cycle.try_lock() else {
            return Err(BridgeError::Other(
                "بررسی نسخهٔ جدید در جریان است؛ چند لحظهٔ دیگر دوباره امتحان کنید.".into(),
            ));
        };
        let not_ready = || BridgeError::Other("نسخهٔ آماده‌ای برای نصب نیست.".into());
        let UpdateState::Ready { version } = self.state() else {
            return Err(not_ready());
        };
        let offer = lock(&self.offer)
            .clone()
            .filter(|o| o.version == version)
            .ok_or_else(not_ready)?;
        let bytes = self.staging.load(&version).ok_or_else(not_ready)?;
        let quiet = |_: &UpdateState| {};
        self.set(
            UpdateState::Installing {
                version: version.clone(),
            },
            &quiet,
        );
        match self.source.install(&offer, bytes, restart).await {
            Ok(()) => {
                self.set(UpdateState::Idle, &quiet);
                Ok(())
            }
            Err(f) => {
                if f.kind == FailKind::Signature {
                    // never offered again this run, and the bad file goes
                    self.staging.prune(None);
                    *lock(&self.rejected) = Some(version.clone());
                }
                *lock(&self.declined) = Some(version);
                self.set(self.failed(&f), &quiet);
                Err(BridgeError::Other(f.message_fa()))
            }
        }
    }
}

/// The event every state change is broadcast on.
pub const EVENT_UPDATE: &str = "update://state";
/// The first check waits for the app to settle; later ones follow every six hours while it runs.
pub const FIRST_CHECK: std::time::Duration = std::time::Duration::from_secs(60);
pub const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// Answers a check within this; a release page that does not answer is not waited on.
const CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// A connection must open within this, and a download that stalls this long is dropped; there is
/// no cap on the whole download — a ~110 MB package over a throttled link may take hours.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const STALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
/// Overrides the configured endpoint (any build): how a prerelease is tried without offering it to
/// sellers. Safe, because the embedded public key still verifies whatever it serves.
pub const ENDPOINT_ENV: &str = "LANDAU_BRIDGE_UPDATE_ENDPOINT";

/// [`Source`] over `tauri-plugin-updater`, which checks the endpoint's `latest.json`, downloads
/// and verifies the package for this install kind, and installs it.
pub struct PluginSource {
    app: tauri::AppHandle,
    update: Mutex<Option<tauri_plugin_updater::Update>>,
}

impl PluginSource {
    pub fn new(app: tauri::AppHandle) -> PluginSource {
        PluginSource {
            app,
            update: Mutex::new(None),
        }
    }

    fn update_for(
        &self,
        offer: &Offer,
    ) -> std::result::Result<tauri_plugin_updater::Update, Failure> {
        lock(&self.update)
            .clone()
            .filter(|u| u.version == offer.version)
            .ok_or_else(|| Failure::new(FailKind::Other, "no checked update for this version"))
    }
}

fn failure(e: tauri_plugin_updater::Error, otherwise: FailKind) -> Failure {
    use tauri_plugin_updater::Error as E;
    let kind = match &e {
        E::Reqwest(_) | E::Network(_) => FailKind::Network,
        E::Minisign(_)
        | E::SignedVersionMismatch { .. }
        | E::MissingSignedVersion
        | E::Base64(_)
        | E::SignatureUtf8(_) => FailKind::Signature,
        E::AuthenticationFailed => FailKind::Declined,
        _ => otherwise,
    };
    Failure::new(kind, e.to_string())
}

#[async_trait]
impl Source for PluginSource {
    async fn check(&self) -> std::result::Result<Option<Offer>, Failure> {
        use tauri_plugin_updater::UpdaterExt;
        let other = |e: tauri_plugin_updater::Error| failure(e, FailKind::Other);
        let mut builder = self
            .app
            .updater_builder()
            .timeout(CHECK_TIMEOUT)
            .configure_client(|c| {
                c.connect_timeout(CONNECT_TIMEOUT)
                    .read_timeout(STALL_TIMEOUT)
            });
        if let Ok(url) = std::env::var(ENDPOINT_ENV) {
            let url = url
                .parse()
                .map_err(|e| Failure::new(FailKind::Other, format!("{ENDPOINT_ENV}: {e}")))?;
            builder = builder.endpoints(vec![url]).map_err(other)?;
        }
        let found = builder
            .build()
            .map_err(other)?
            .check()
            .await
            .map_err(other)?;
        let offer = found.as_ref().map(|u| Offer {
            version: u.version.clone(),
        });
        *lock(&self.update) = found.map(|mut u| {
            u.timeout = None; // the check's total cap; the download is bounded by stalls only
            u
        });
        Ok(offer)
    }

    async fn download(
        &self,
        offer: &Offer,
        progress: &(dyn Fn(u8) + Send + Sync),
    ) -> std::result::Result<Vec<u8>, Failure> {
        let update = self.update_for(offer)?;
        let mut received: u64 = 0;
        update
            .download(
                |chunk, total| {
                    received += chunk as u64;
                    if let Some(total) = total.filter(|t| *t > 0) {
                        progress((received * 100 / total).min(100) as u8);
                    }
                },
                || {},
            )
            .await
            .map_err(|e| failure(e, FailKind::Network))
    }

    async fn verify(&self, offer: &Offer, bytes: &[u8]) -> std::result::Result<(), Failure> {
        self.update_for(offer)?
            .verify(bytes)
            .map_err(|e| failure(e, FailKind::Signature))
    }

    async fn install(
        &self,
        offer: &Offer,
        bytes: Vec<u8>,
        restart: bool,
    ) -> std::result::Result<(), Failure> {
        let update = self.update_for(offer)?.restart_after_install(restart);
        // Writes the package (an AppImage is ~100 MB) or runs the installer: off the async threads.
        tauri::async_runtime::spawn_blocking(move || update.install(bytes))
            .await
            .map_err(|e| Failure::new(FailKind::Install, e.to_string()))?
            .map_err(|e| failure(e, FailKind::Install))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitAction {
    Exit,
    InstallThenExit,
    /// An install started from the button is still writing: let it finish first.
    WaitForInstall,
}

/// What closing the app does with a staged update. With `auto_update` off nothing installs on
/// its own — that is how a seller who rolled back keeps the older version.
pub fn quit_action(
    kind: InstallKind,
    state: &UpdateState,
    job_active: bool,
    declined: bool,
    auto_update: bool,
) -> QuitAction {
    if matches!(state, UpdateState::Installing { .. }) {
        return QuitAction::WaitForInstall;
    }
    let ready = matches!(state, UpdateState::Ready { .. });
    if ready && auto_update && !job_active && !declined && installs_on_quit(kind) {
        QuitAction::InstallThenExit
    } else {
        QuitAction::Exit
    }
}

/// A poisoned lock still holds a usable value here (plain state, no invariant spans two fields).
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Default)]
    struct FakeSource {
        offer: Option<String>,
        check_fails: Option<FailKind>,
        download_fails: Option<FailKind>,
        /// Packages whose bytes fail verification.
        bad_bytes: Option<Vec<u8>>,
        install_fails: Option<FailKind>,
        install_takes: Option<std::time::Duration>,
        downloads: Arc<AtomicUsize>,
        installs: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Source for FakeSource {
        async fn check(&self) -> std::result::Result<Option<Offer>, Failure> {
            if let Some(k) = self.check_fails {
                return Err(Failure::new(k, "scripted"));
            }
            Ok(self.offer.clone().map(|version| Offer { version }))
        }
        async fn download(
            &self,
            offer: &Offer,
            progress: &(dyn Fn(u8) + Send + Sync),
        ) -> std::result::Result<Vec<u8>, Failure> {
            self.downloads.fetch_add(1, Ordering::SeqCst);
            if let Some(k) = self.download_fails {
                return Err(Failure::new(k, "scripted"));
            }
            progress(50);
            progress(100);
            Ok(format!("package {}", offer.version).into_bytes())
        }
        async fn verify(&self, _offer: &Offer, bytes: &[u8]) -> std::result::Result<(), Failure> {
            match &self.bad_bytes {
                Some(bad) if bad == bytes => Err(Failure::new(FailKind::Signature, "scripted")),
                _ => Ok(()),
            }
        }
        async fn install(
            &self,
            _offer: &Offer,
            _bytes: Vec<u8>,
            _restart: bool,
        ) -> std::result::Result<(), Failure> {
            if let Some(t) = self.install_takes {
                tokio::time::sleep(t).await;
            }
            self.installs.fetch_add(1, Ordering::SeqCst);
            match self.install_fails {
                Some(k) => Err(Failure::new(k, "scripted")),
                None => Ok(()),
            }
        }
    }

    fn updater(source: FakeSource) -> (Updater<FakeSource>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (Updater::new(source, Staging::new(dir.path())), dir)
    }

    fn offering(version: &str) -> FakeSource {
        FakeSource {
            offer: Some(version.into()),
            ..Default::default()
        }
    }

    async fn cycle(u: &Updater<FakeSource>) -> (UpdateState, Vec<UpdateState>) {
        let seen = Mutex::new(Vec::new());
        let last = u
            .check_and_stage(&|s: &UpdateState| seen.lock().unwrap().push(s.clone()))
            .await;
        (last, seen.into_inner().unwrap())
    }

    #[tokio::test]
    async fn no_offer_is_idle() {
        let (u, _d) = updater(FakeSource::default());
        assert_eq!(cycle(&u).await.0, UpdateState::Idle);
    }

    #[tokio::test]
    async fn offline_check_fails_quietly() {
        let source = FakeSource {
            check_fails: Some(FailKind::Network),
            ..Default::default()
        };
        let downloads = source.downloads.clone();
        let (u, _d) = updater(source);
        match cycle(&u).await.0 {
            UpdateState::Failed { kind, message, at } => {
                assert_eq!(kind, "Network");
                assert_eq!(
                    message,
                    "اتصال به GitHub برقرار نشد؛ بعداً دوباره بررسی می‌شود."
                );
                assert!(!at.is_empty());
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(downloads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn an_offer_is_downloaded_and_staged() {
        let (u, dir) = updater(offering("1.0.1"));
        let (last, seen) = cycle(&u).await;
        assert_eq!(
            last,
            UpdateState::Ready {
                version: "1.0.1".into()
            }
        );
        assert!(dir.path().join("updates/1.0.1.pkg").is_file());
        assert!(seen.contains(&UpdateState::Downloading {
            version: "1.0.1".into(),
            percent: 50
        }));
        assert_eq!(u.state(), last);
    }

    #[tokio::test]
    async fn a_staged_package_is_reused() {
        let source = offering("1.0.1");
        let downloads = source.downloads.clone();
        let (u, dir) = updater(source);
        Staging::new(dir.path()).store("1.0.1", b"earlier").unwrap();
        assert_eq!(
            cycle(&u).await.0,
            UpdateState::Ready {
                version: "1.0.1".into()
            }
        );
        assert_eq!(downloads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_partial_file_is_never_loaded() {
        let source = offering("1.0.1");
        let downloads = source.downloads.clone();
        let (u, dir) = updater(source);
        std::fs::create_dir_all(dir.path().join("updates")).unwrap();
        std::fs::write(dir.path().join("updates/1.0.1.part"), b"cut off").unwrap();
        assert_eq!(Staging::new(dir.path()).load("1.0.1"), None);
        cycle(&u).await;
        assert_eq!(downloads.load(Ordering::SeqCst), 1);
        assert_eq!(
            Staging::new(dir.path()).load("1.0.1").as_deref(),
            Some(&b"package 1.0.1"[..])
        );
    }

    #[test]
    fn prune_keeps_only_the_offer() {
        let dir = tempfile::tempdir().unwrap();
        let s = Staging::new(dir.path());
        s.store("1.0.0", b"a").unwrap();
        s.store("1.0.1", b"b").unwrap();
        std::fs::write(dir.path().join("updates/1.0.2.part"), b"c").unwrap();
        s.prune(Some("1.0.1"));
        let mut left: Vec<String> = std::fs::read_dir(dir.path().join("updates"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, vec!["1.0.1.pkg"]);
        s.prune(None);
        assert_eq!(s.load("1.0.1"), None);
    }

    #[tokio::test]
    async fn install_refuses_while_a_job_is_active() {
        let source = offering("1.0.1");
        let installs = source.installs.clone();
        let (u, _d) = updater(source);
        cycle(&u).await;
        let err = u.install(true, true).await.unwrap_err();
        assert_eq!(err.kind(), "JobBusy");
        assert_eq!(installs.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_failed_install_keeps_the_staged_file_and_reports() {
        let source = FakeSource {
            install_fails: Some(FailKind::Install),
            ..offering("1.0.1")
        };
        let (u, dir) = updater(source);
        cycle(&u).await;
        assert!(u.install(false, true).await.is_err());
        assert!(matches!(u.state(), UpdateState::Failed { ref kind, .. } if kind == "Install"));
        assert!(dir.path().join("updates/1.0.1.pkg").is_file());
    }

    #[tokio::test]
    async fn a_declined_version_is_not_installed_again_this_run() {
        let source = FakeSource {
            install_fails: Some(FailKind::Declined),
            ..offering("1.0.1")
        };
        let (u, _d) = updater(source);
        cycle(&u).await;
        assert!(!u.declined());
        assert!(u.install(false, true).await.is_err());
        cycle(&u).await; // the next check finds the same version, staged
        assert!(u.declined());
        assert_eq!(
            quit_action(InstallKind::AppImage, &u.state(), false, u.declined(), true),
            QuitAction::Exit
        );
    }

    #[tokio::test]
    async fn a_staged_package_that_fails_verification_is_downloaded_again() {
        let source = FakeSource {
            bad_bytes: Some(b"rotten".to_vec()),
            ..offering("1.0.1")
        };
        let downloads = source.downloads.clone();
        let (u, dir) = updater(source);
        Staging::new(dir.path()).store("1.0.1", b"rotten").unwrap();
        assert_eq!(
            cycle(&u).await.0,
            UpdateState::Ready {
                version: "1.0.1".into()
            }
        );
        assert_eq!(downloads.load(Ordering::SeqCst), 1);
        assert_eq!(
            Staging::new(dir.path()).load("1.0.1").as_deref(),
            Some(&b"package 1.0.1"[..])
        );
    }

    #[tokio::test]
    async fn a_version_failing_its_signature_is_not_downloaded_again_this_run() {
        let source = FakeSource {
            download_fails: Some(FailKind::Signature),
            ..offering("1.0.1")
        };
        let downloads = source.downloads.clone();
        let (u, _d) = updater(source);
        assert!(
            matches!(cycle(&u).await.0, UpdateState::Failed { ref kind, .. } if kind == "Signature")
        );
        assert!(
            matches!(cycle(&u).await.0, UpdateState::Failed { ref kind, .. } if kind == "Signature")
        );
        assert_eq!(downloads.load(Ordering::SeqCst), 1, "never fetched twice");
    }

    #[tokio::test]
    async fn a_signature_failure_at_install_discards_the_package() {
        let source = FakeSource {
            install_fails: Some(FailKind::Signature),
            ..offering("1.0.1")
        };
        let downloads = source.downloads.clone();
        let (u, dir) = updater(source);
        cycle(&u).await;
        assert!(u.install(false, true).await.is_err());
        assert_eq!(Staging::new(dir.path()).load("1.0.1"), None);
        assert!(
            matches!(cycle(&u).await.0, UpdateState::Failed { ref kind, .. } if kind == "Signature")
        );
        assert_eq!(downloads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn settle_waits_for_an_install_in_flight() {
        let source = FakeSource {
            install_takes: Some(std::time::Duration::from_millis(200)),
            ..offering("1.0.1")
        };
        let installs = source.installs.clone();
        let (u, _d) = updater(source);
        cycle(&u).await;
        let u = Arc::new(u);
        let running = {
            let u = u.clone();
            tokio::spawn(async move { u.install(false, false).await })
        };
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert!(matches!(u.state(), UpdateState::Installing { .. }));
        u.settle().await;
        assert_eq!(
            installs.load(Ordering::SeqCst),
            1,
            "settle returned only after the install"
        );
        running.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn install_refuses_while_a_check_runs() {
        let source = offering("1.0.1");
        let installs = source.installs.clone();
        let (u, _d) = updater(source);
        cycle(&u).await;
        let _check = u.cycle.lock().await;
        assert!(u.install(false, true).await.is_err());
        assert_eq!(installs.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn quitting_mid_install_waits_for_it() {
        let installing = UpdateState::Installing {
            version: "1.0.1".into(),
        };
        assert_eq!(
            quit_action(InstallKind::AppImage, &installing, false, false, true),
            QuitAction::WaitForInstall
        );
        assert_eq!(
            quit_action(InstallKind::Deb, &installing, true, true, true),
            QuitAction::WaitForInstall
        );
    }

    #[test]
    fn installs_on_quit_only_for_appimage_and_nsis() {
        assert!(installs_on_quit(InstallKind::AppImage));
        assert!(installs_on_quit(InstallKind::Nsis));
        assert!(!installs_on_quit(InstallKind::Deb));
        assert!(!installs_on_quit(InstallKind::Rpm));
        assert!(!installs_on_quit(InstallKind::Unknown));
    }

    fn ready() -> UpdateState {
        UpdateState::Ready {
            version: "1.0.1".into(),
        }
    }

    #[test]
    fn quit_install_skipped_while_a_job_waits_in_review() {
        assert_eq!(
            quit_action(InstallKind::AppImage, &ready(), true, false, true),
            QuitAction::Exit
        );
    }

    #[test]
    fn quit_installs_a_ready_appimage() {
        assert_eq!(
            quit_action(InstallKind::AppImage, &ready(), false, false, true),
            QuitAction::InstallThenExit
        );
        assert_eq!(
            quit_action(
                InstallKind::AppImage,
                &UpdateState::Idle,
                false,
                false,
                true
            ),
            QuitAction::Exit
        );
    }

    #[test]
    fn quit_never_installs_with_auto_update_off() {
        // a seller who rolled back switches it off: closing must not bring the newer one back
        assert_eq!(
            quit_action(InstallKind::AppImage, &ready(), false, false, false),
            QuitAction::Exit
        );
    }

    #[test]
    fn quit_never_installs_a_deb() {
        assert_eq!(
            quit_action(InstallKind::Deb, &ready(), false, false, true),
            QuitAction::Exit
        );
    }
}
