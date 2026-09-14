//! Offer to put a running AppImage into the desktop menu (README, "Other Linux"). Pure paths in,
//! files out — the Tauri command only supplies `$APPIMAGE`, `$APPDIR`, `$HOME` and the version.

use std::path::{Path, PathBuf};

use crate::errors::{BridgeError, Result};

/// Where the AppImage runtime says we are: `$APPIMAGE` is the `.AppImage` file, `$APPDIR` its
/// mount point. `None` when not running from an AppImage (deb, rpm, Windows, `tauri dev`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppImage {
    pub image: PathBuf,
    pub appdir: PathBuf,
}

pub fn running(image: Option<&str>, appdir: Option<&str>) -> Option<AppImage> {
    let image = image.filter(|s| !s.is_empty())?;
    Some(AppImage {
        image: PathBuf::from(image),
        appdir: PathBuf::from(appdir.unwrap_or_default()),
    })
}

/// The freedesktop per-user layout under `$HOME`: the copy lives in `~/.local/bin`, the menu
/// entry in `~/.local/share/applications`, the icons in `~/.local/share/icons/hicolor`.
pub struct Layout {
    home: PathBuf,
}

impl Layout {
    pub fn new(home: impl Into<PathBuf>) -> Layout {
        Layout { home: home.into() }
    }

    pub fn image(&self) -> PathBuf {
        self.home.join(".local/bin/landau-bridge.AppImage")
    }

    pub fn desktop(&self) -> PathBuf {
        self.home
            .join(".local/share/applications/ir.landau.bridge.desktop")
    }

    fn icons(&self) -> PathBuf {
        self.home.join(".local/share/icons/hicolor")
    }
}

/// Copy the image (executable) and its icons under `$HOME/.local`, then write the menu entry.
/// Re-running over an earlier copy replaces it — that is how an upgrade lands. Returns where the
/// copy went.
pub fn install(running: &AppImage, layout: &Layout, version: &str) -> Result<PathBuf> {
    let target = layout.image();
    let dir = target.parent().expect("image path has a parent");
    std::fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    // Copy next to the target then rename, so a crash mid-copy never leaves a half AppImage in
    // the menu's path.
    let tmp = dir.join("landau-bridge.AppImage.part");
    std::fs::copy(&running.image, &tmp).map_err(|e| io_err(&running.image, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| io_err(&tmp, e))?;
    }
    std::fs::rename(&tmp, &target).map_err(|e| io_err(&target, e))?;

    copy_icons(
        &running.appdir.join("usr/share/icons/hicolor"),
        &layout.icons(),
    )?;

    let desktop = layout.desktop();
    let dir = desktop.parent().expect("desktop path has a parent");
    std::fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    std::fs::write(&desktop, desktop_entry(&target, version)).map_err(|e| io_err(&desktop, e))?;
    Ok(target)
}

fn desktop_entry(image: &Path, version: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Landau Bridge\n\
         Name[fa]=پل لاندا\n\
         Comment=Barezpakhsh invoices onto a Landau shelf\n\
         Comment[fa]=فاکتورهای بارزپخش روی قفسهٔ لاندا\n\
         Exec=\"{}\"\n\
         Icon=landau-bridge\n\
         Terminal=false\n\
         Categories=Office;\n\
         StartupWMClass=landau-bridge\n\
         X-AppImage-Version={version}\n",
        image.display()
    )
}

/// Mirror `<size>/apps/landau-bridge.png` for every size the AppDir ships. A missing tree is not
/// an error: the entry still works, the menu just shows a generic icon.
fn copy_icons(from: &Path, to: &Path) -> Result<()> {
    let Ok(sizes) = std::fs::read_dir(from) else {
        return Ok(());
    };
    for size in sizes.flatten() {
        let src = size.path().join("apps/landau-bridge.png");
        if !src.is_file() {
            continue;
        }
        let dst_dir = to.join(size.file_name()).join("apps");
        std::fs::create_dir_all(&dst_dir).map_err(|e| io_err(&dst_dir, e))?;
        let dst = dst_dir.join("landau-bridge.png");
        std::fs::copy(&src, &dst).map_err(|e| io_err(&dst, e))?;
    }
    Ok(())
}

fn io_err(path: &Path, e: std::io::Error) -> BridgeError {
    BridgeError::Other(format!("{}: {e}", path.display()))
}

/// Ask when the running image is not the installed copy and the seller has not said "don't ask".
/// "The installed copy" is the same file or one of the same size: a re-run of the file that was
/// installed a minute ago stays quiet, a newer build (which changes size) is offered again.
pub fn should_offer(running: Option<&AppImage>, layout: &Layout, dismissed: bool) -> bool {
    let Some(r) = running else { return false };
    !dismissed && !same_build(&r.image, &layout.image())
}

fn same_build(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    if canon(a) == canon(b) {
        return true;
    }
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.len() == y.len(),
        _ => false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_is_none_without_the_appimage_variable() {
        assert!(running(None, Some("/tmp/.mount_x")).is_none());
        assert!(running(Some(""), Some("/tmp/.mount_x")).is_none());
    }

    #[test]
    fn running_reads_the_image_and_its_mount() {
        let r = running(Some("/home/s/Downloads/lb.AppImage"), Some("/tmp/.mount_x")).unwrap();
        assert_eq!(r.image, PathBuf::from("/home/s/Downloads/lb.AppImage"));
        assert_eq!(r.appdir, PathBuf::from("/tmp/.mount_x"));
    }

    #[test]
    fn should_offer_is_false_when_not_running_from_an_appimage() {
        let layout = Layout::new("/home/s");
        assert!(!should_offer(None, &layout, false));
    }

    #[test]
    fn should_offer_is_false_when_the_seller_said_do_not_ask() {
        let layout = Layout::new("/home/s");
        let r = running(Some("/home/s/Downloads/lb.AppImage"), Some("/tmp/.mount_x")).unwrap();
        assert!(!should_offer(Some(&r), &layout, true));
    }

    #[test]
    fn should_offer_is_false_when_running_the_installed_copy() {
        let layout = Layout::new("/home/s");
        let installed = layout.image().display().to_string();
        let r = running(Some(&installed), Some("/tmp/.mount_x")).unwrap();
        assert!(!should_offer(Some(&r), &layout, false));
    }

    #[test]
    fn should_offer_is_false_when_an_identical_copy_is_already_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let r = fake_appimage(tmp.path(), b"png");
        let layout = Layout::new(tmp.path());
        install(&r, &layout, "0.1.0").unwrap();
        assert!(
            !should_offer(Some(&r), &layout, false),
            "same bytes, still offered"
        );
    }

    #[test]
    fn should_offer_is_true_when_the_installed_copy_is_a_different_build() {
        let tmp = tempfile::tempdir().unwrap();
        let r = fake_appimage(tmp.path(), b"png");
        let layout = Layout::new(tmp.path());
        std::fs::create_dir_all(layout.image().parent().unwrap()).unwrap();
        std::fs::write(layout.image(), b"an older, shorter build").unwrap();
        assert!(should_offer(Some(&r), &layout, false));
    }

    #[test]
    fn should_offer_is_true_for_a_fresh_appimage() {
        let layout = Layout::new("/home/s");
        let r = running(Some("/home/s/Downloads/lb.AppImage"), Some("/tmp/.mount_x")).unwrap();
        assert!(should_offer(Some(&r), &layout, false));
    }

    fn fake_appimage(dir: &Path, icon_bytes: &[u8]) -> AppImage {
        let image = dir.join("Downloads/lb.AppImage");
        std::fs::create_dir_all(image.parent().unwrap()).unwrap();
        std::fs::write(&image, b"AppImage bytes").unwrap();
        let appdir = dir.join(".mount_x");
        let icon = appdir.join("usr/share/icons/hicolor/32x32/apps/landau-bridge.png");
        std::fs::create_dir_all(icon.parent().unwrap()).unwrap();
        std::fs::write(&icon, icon_bytes).unwrap();
        AppImage { image, appdir }
    }

    #[test]
    fn install_copies_the_image_executable_into_local_bin() {
        let tmp = tempfile::tempdir().unwrap();
        let r = fake_appimage(tmp.path(), b"png");
        let layout = Layout::new(tmp.path());
        let installed = install(&r, &layout, "0.1.0").unwrap();
        assert_eq!(installed, layout.image());
        assert_eq!(std::fs::read(&installed).unwrap(), b"AppImage bytes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&installed).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "mode {mode:o} is not executable");
        }
    }

    #[test]
    fn install_writes_a_desktop_entry_that_launches_the_installed_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let r = fake_appimage(tmp.path(), b"png");
        let layout = Layout::new(tmp.path());
        install(&r, &layout, "0.1.0").unwrap();
        let entry = std::fs::read_to_string(layout.desktop()).unwrap();
        assert!(entry.starts_with("[Desktop Entry]\n"), "{entry}");
        assert!(entry.contains("Type=Application\n"), "{entry}");
        assert!(entry.contains("Name=Landau Bridge\n"), "{entry}");
        assert!(entry.contains("Name[fa]=پل لاندا\n"), "{entry}");
        assert!(
            entry.contains(&format!("Exec=\"{}\"\n", layout.image().display())),
            "{entry}"
        );
        assert!(entry.contains("Icon=landau-bridge\n"), "{entry}");
        assert!(entry.contains("X-AppImage-Version=0.1.0\n"), "{entry}");
        assert!(entry.contains("StartupWMClass=landau-bridge\n"), "{entry}");
    }

    #[test]
    fn install_copies_the_hicolor_icons() {
        let tmp = tempfile::tempdir().unwrap();
        let r = fake_appimage(tmp.path(), b"png");
        let layout = Layout::new(tmp.path());
        install(&r, &layout, "0.1.0").unwrap();
        let icon = tmp
            .path()
            .join(".local/share/icons/hicolor/32x32/apps/landau-bridge.png");
        assert_eq!(std::fs::read(icon).unwrap(), b"png");
    }

    #[test]
    fn install_replaces_an_earlier_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let r = fake_appimage(tmp.path(), b"png");
        let layout = Layout::new(tmp.path());
        std::fs::create_dir_all(layout.image().parent().unwrap()).unwrap();
        std::fs::write(layout.image(), b"old version").unwrap();
        install(&r, &layout, "0.2.0").unwrap();
        assert_eq!(std::fs::read(layout.image()).unwrap(), b"AppImage bytes");
    }

    #[test]
    fn install_still_succeeds_when_the_appdir_ships_no_icons() {
        let tmp = tempfile::tempdir().unwrap();
        let r = fake_appimage(tmp.path(), b"png");
        std::fs::remove_dir_all(r.appdir.join("usr")).unwrap();
        let layout = Layout::new(tmp.path());
        install(&r, &layout, "0.1.0").unwrap();
        assert!(layout.desktop().is_file());
    }
}
