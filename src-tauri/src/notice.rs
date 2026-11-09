//! The one-time upgrade notice (plan 130 task 14, R-j, R-n): what an install upgraded from before
//! plan 130 — or one whose threshold the floor just raised — is told once, until the seller
//! dismisses it.
//!
//! - the threshold raised, from the stored value to the floor ([`SIMILARITY_FLOOR`]);
//! - the links that wait, unverified, for the next job's VALIDATE;
//! - the audit page, where every doubtful row and every automatic write is listed;
//! - every other `landau-bridge*.AppImage` in the usual places: a build from before plan 130 finds
//!   no links on the migrated database (R-n), and one from before 1.0.0 has no schema guard.
//!
//! [`prepare`] runs at every start-up and does all of it in one transaction — the raise, the
//! notice and consuming the migration's record — so a launch cut in between leaves either the old
//! state or the new one, never a raised threshold with no word of it.

use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{now, Db, UPGRADED_FROM_SCHEMA};
use crate::errors::Result;
use crate::matching::gate::SIMILARITY_FLOOR;

/// The settings key the notice is kept under, as JSON, until dismissed.
pub const NOTICE: &str = "upgrade_notice";
/// The last schema written by a build without plan 130's proof (v1.1.1): an install migrated from
/// it, or from earlier, is told what changed.
pub const LAST_UNPROVEN_SCHEMA: i64 = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpgradeNotice {
    /// The schema the install was migrated from, when it predates plan 130.
    pub from_schema: Option<i64>,
    /// The threshold the setting held before the floor raised it.
    pub threshold_from: Option<u8>,
    pub threshold_to: u8,
    /// Links (a product named) waiting unverified for the next job's VALIDATE when the notice was built.
    pub links_unverified: i64,
    /// Other `landau-bridge*.AppImage` files found in the usual places, the running one left out.
    pub appimages: Vec<String>,
    pub built_at: String,
}

/// Raise a threshold stored below the floor, and build the notice when the install came from before
/// plan 130 or the threshold was raised; answers the notice kept (a new one, or one built at an
/// earlier start-up and not yet dismissed). `appimages` is what [`find_appimages`] found.
pub fn prepare(db: &Db, appimages: &[String]) -> Result<Option<UpgradeNotice>> {
    let floor = SIMILARITY_FLOOR;
    db.with(|c| {
        let tx = c.unchecked_transaction()?;
        let get = |k: &str| -> rusqlite::Result<Option<String>> {
            tx.query_row("SELECT value FROM settings WHERE key = ?1", [k], |r| r.get(0))
                .optional()
        };
        let from_schema = get(UPGRADED_FROM_SCHEMA)?
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|v| *v <= LAST_UNPROVEN_SCHEMA);
        let raised_from = get("threshold")?
            .and_then(|v| v.trim().parse::<u8>().ok())
            .filter(|v| *v < floor);
        let kept = get(NOTICE)?.and_then(|v| parse(&v));
        tx.execute("DELETE FROM settings WHERE key = ?1", [UPGRADED_FROM_SCHEMA])?;
        if from_schema.is_none() && raised_from.is_none() {
            tx.commit()?;
            return Ok(kept);
        }
        if raised_from.is_some() {
            set(&tx, "threshold", &floor.to_string())?;
        }
        let links_unverified: i64 = tx.query_row(
            "SELECT count(*) FROM supplier_links WHERE state = 'UNVERIFIED' AND product_id IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        // A notice not yet dismissed keeps what it already said; this start-up adds to it.
        let notice = match kept {
            Some(k) => UpgradeNotice {
                from_schema: k.from_schema.or(from_schema),
                threshold_from: raised_from.or(k.threshold_from),
                threshold_to: floor,
                links_unverified: k.links_unverified,
                appimages: k.appimages,
                built_at: k.built_at,
            },
            None => UpgradeNotice {
                from_schema,
                threshold_from: raised_from,
                threshold_to: floor,
                links_unverified,
                appimages: appimages.to_vec(),
                built_at: now(),
            },
        };
        let json = serde_json::to_string(&notice)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        set(&tx, NOTICE, &json)?;
        tx.commit()?;
        Ok(Some(notice))
    })
}

fn set(tx: &rusqlite::Transaction<'_>, key: &str, value: &str) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map(|_| ())
}

fn parse(v: &str) -> Option<UpgradeNotice> {
    match serde_json::from_str(v) {
        Ok(n) => Some(n),
        Err(e) => {
            tracing::warn!("upgrade notice unreadable, dropped: {e}");
            None
        }
    }
}

/// The notice kept, if any. A value that no longer parses is treated as none.
pub fn current(db: &Db) -> Result<Option<UpgradeNotice>> {
    Ok(db.setting(NOTICE)?.and_then(|v| parse(&v)))
}

/// The seller has read it: it is not shown again.
pub fn dismiss(db: &Db) -> Result<()> {
    db.delete_setting(NOTICE)
}

/// What the dashboard shows: the notice as things stand now (see [`view`]).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoticeView {
    pub from_schema: Option<i64>,
    pub threshold_from: Option<u8>,
    pub threshold_to: u8,
    /// Links (a product named) not yet judged by any job, counted now.
    pub links_unverified: i64,
    /// Other copies of the app still on disk, neither the running one nor the same build.
    pub appimages: Vec<String>,
    /// The applications menu's copy when it is older than the running AppImage: «نصب» replaces it.
    pub menu_copy: Option<String>,
}

/// The notice as the dashboard shows it now: the links still waiting counted live (a job judges
/// those it uses), and the stored list of other copies read again — a file since deleted, the
/// running image, or one of the same build drops out. When the app runs from an AppImage, the
/// applications menu's copy is named apart: «نصب» replaces it, so it is never one to delete.
pub fn view(
    db: &Db,
    n: UpgradeNotice,
    running: Option<&Path>,
    menu_copy: &Path,
) -> Result<NoticeView> {
    let links_unverified = db.with(|c| {
        c.query_row(
            "SELECT count(*) FROM supplier_links WHERE state = 'UNVERIFIED' AND product_id IS NOT NULL",
            [],
            |r| r.get(0),
        )
    })?;
    let canon = |p: &Path| p.canonicalize().ok();
    let mut appimages = Vec::new();
    let mut menu = None;
    for f in n.appimages {
        let p = Path::new(&f);
        if !p.is_file() {
            continue;
        }
        if let Some(r) = running {
            if crate::appimage::same_build(p, r) {
                continue;
            }
            if canon(p).is_some() && canon(p) == canon(menu_copy) {
                menu = Some(f);
                continue;
            }
        }
        appimages.push(f);
    }
    Ok(NoticeView {
        from_schema: n.from_schema,
        threshold_from: n.threshold_from,
        threshold_to: n.threshold_to,
        links_unverified,
        appimages,
        menu_copy: menu,
    })
}

/// Whether this start-up's [`prepare`] will build a new notice, so the folders are read only
/// then: never on an ordinary launch, never again while a notice is kept.
pub fn will_build(db: &Db) -> Result<bool> {
    let upgraded = db
        .setting(UPGRADED_FROM_SCHEMA)?
        .and_then(|v| v.parse::<i64>().ok())
        .is_some_and(|v| v <= LAST_UNPROVEN_SCHEMA);
    let low = db
        .setting("threshold")?
        .and_then(|v| v.trim().parse::<u8>().ok())
        .is_some_and(|v| v < SIMILARITY_FLOOR);
    Ok((upgraded || low) && db.setting(NOTICE)?.is_none())
}

/// The usual places a seller keeps an AppImage: the home folder, Downloads and Desktop (as the
/// desktop names them — localised on a Persian desktop), `~/Applications`, `~/AppImages`,
/// `~/.local/bin` and `~/bin`. Duplicates dropped, order kept.
pub fn usual_places(
    home: &Path,
    download: Option<PathBuf>,
    desktop: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let candidates = [
        Some(home.to_path_buf()),
        download,
        Some(home.join("Downloads")),
        desktop,
        Some(home.join("Desktop")),
        Some(home.join("Applications")),
        Some(home.join("AppImages")),
        Some(home.join(".local/bin")),
        Some(home.join("bin")),
    ];
    for p in candidates.into_iter().flatten() {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Every `landau-bridge*.AppImage` file directly inside `places` (any letter case, top level only),
/// except `running`. Unreadable or missing folders are skipped. Sorted.
pub fn find_appimages(places: &[PathBuf], running: Option<&Path>) -> Vec<String> {
    let running = running.map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()));
    let mut found: Vec<PathBuf> = Vec::new();
    for dir in places {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if !name.starts_with("landau-bridge") || !name.ends_with(".appimage") {
                continue;
            }
            let path = e.path();
            if !path.is_file() {
                continue;
            }
            let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
            if running.as_ref() == Some(&canon)
                || found
                    .iter()
                    .any(|f| f.canonicalize().map(|c| c == canon).unwrap_or(false))
            {
                continue;
            }
            found.push(path);
        }
    }
    let mut out: Vec<String> = found.iter().map(|p| p.display().to_string()).collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(db: &Db, key: &str, product: Option<&str>, how: &str, state: &str) {
        db.with(|c| {
            c.execute(
                "INSERT INTO supplier_links(supplier, name_key, code, product_id, product_name, how, name_seen, decided_at, state)
                 VALUES ('barezpakhsh', ?1, ?1, ?2, ?2, ?3, ?1, '2026-10-07T16:30:00Z', ?4)",
                params![key, product, how, state],
            )
        })
        .unwrap();
    }

    /// An install as migration 0004 leaves v1.1.1's: every link `UNVERIFIED`, the threshold at 70.
    fn upgraded_from_v1_1_1() -> Db {
        let db = Db::in_memory().unwrap();
        db.set_setting(UPGRADED_FROM_SCHEMA, "3").unwrap();
        db.set_setting("threshold", "70").unwrap();
        link(&db, "a", Some("p_auto"), "AUTO", "UNVERIFIED");
        link(&db, "u", Some("p_user"), "USER", "UNVERIFIED");
        link(&db, "n", None, "NONE", "UNVERIFIED");
        db
    }

    #[test]
    fn an_install_from_before_plan_130_is_told_once_what_changed() {
        let db = upgraded_from_v1_1_1();
        let old = vec!["/home/s/Downloads/landau-bridge_0.1.0_amd64.AppImage".to_string()];
        let n = prepare(&db, &old).unwrap().expect("a notice");
        assert_eq!(n.from_schema, Some(3));
        assert_eq!((n.threshold_from, n.threshold_to), (Some(70), 85));
        assert_eq!(n.links_unverified, 2, "a «none» names no product to prove");
        assert_eq!(n.appimages, old);
        assert_eq!(db.setting("threshold").unwrap().as_deref(), Some("85"));
        assert_eq!(
            db.setting(UPGRADED_FROM_SCHEMA).unwrap(),
            None,
            "the record is consumed"
        );
        assert_eq!(current(&db).unwrap(), Some(n.clone()));

        // the next start-up keeps the notice as built: never rebuilt, never a second one
        let again = prepare(&db, &[]).unwrap();
        assert_eq!(again, Some(n));

        dismiss(&db).unwrap();
        assert_eq!(current(&db).unwrap(), None);
        assert_eq!(prepare(&db, &old).unwrap(), None, "dismissed is for good");
    }

    #[test]
    fn a_threshold_below_the_floor_is_raised_on_any_install() {
        let db = Db::in_memory().unwrap();
        db.set_setting("threshold", "70").unwrap();
        let n = prepare(&db, &[]).unwrap().expect("a notice for the raise");
        assert_eq!(
            (n.from_schema, n.threshold_from, n.threshold_to),
            (None, Some(70), 85)
        );
        assert_eq!(db.setting("threshold").unwrap().as_deref(), Some("85"));
    }

    #[test]
    fn an_install_with_nothing_changed_is_told_nothing() {
        let db = Db::in_memory().unwrap();
        assert_eq!(
            prepare(&db, &["/x/landau-bridge.AppImage".into()]).unwrap(),
            None
        );
        db.set_setting("threshold", "92").unwrap();
        assert_eq!(prepare(&db, &[]).unwrap(), None);
        assert_eq!(
            db.setting("threshold").unwrap().as_deref(),
            Some("92"),
            "left alone"
        );

        // migrated from a plan-130 build: the record is consumed, nothing to say
        db.set_setting(UPGRADED_FROM_SCHEMA, "5").unwrap();
        assert_eq!(prepare(&db, &[]).unwrap(), None);
        assert_eq!(db.setting(UPGRADED_FROM_SCHEMA).unwrap(), None);
    }

    #[test]
    fn a_threshold_raised_after_an_earlier_notice_keeps_both_facts() {
        let db = upgraded_from_v1_1_1();
        db.set_setting("threshold", "90").unwrap();
        let first = prepare(&db, &[]).unwrap().unwrap();
        assert_eq!((first.from_schema, first.threshold_from), (Some(3), None));
        // a value below the floor written behind the app's back (a restored settings row)
        db.set_setting("threshold", "60").unwrap();
        let n = prepare(&db, &[]).unwrap().unwrap();
        assert_eq!(
            (n.from_schema, n.threshold_from, n.threshold_to),
            (Some(3), Some(60), 85)
        );
    }

    #[test]
    fn a_notice_that_no_longer_parses_reads_as_none() {
        let db = Db::in_memory().unwrap();
        db.set_setting(NOTICE, "{not json").unwrap();
        assert_eq!(current(&db).unwrap(), None);
    }

    #[test]
    fn older_appimages_are_found_in_the_usual_places_but_not_the_running_one() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        let dl = h.join("دانلودها");
        let bin = h.join(".local/bin");
        for d in [&dl, &bin, &h.join("sub")] {
            std::fs::create_dir_all(d).unwrap();
        }
        let touch = |p: PathBuf| std::fs::write(p, b"x").unwrap();
        touch(dl.join("landau-bridge_0.1.0_amd64.AppImage"));
        touch(dl.join("Landau-Bridge_1.1.1_amd64.appimage"));
        touch(dl.join("other_1.0.AppImage"));
        touch(dl.join("landau-bridge_1.1.1_amd64.deb"));
        touch(h.join("sub/landau-bridge.AppImage")); // not a usual place
        std::fs::create_dir_all(h.join("landau-bridge-src.AppImage")).unwrap(); // a folder
        touch(bin.join("landau-bridge.AppImage")); // the running one

        let places = usual_places(h, Some(dl.clone()), Some(h.join("Desktop")));
        assert_eq!(places[0], h.to_path_buf());
        assert!(places.contains(&dl) && places.contains(&bin) && places.contains(&h.join("bin")));
        assert_eq!(places.iter().filter(|p| **p == dl).count(), 1);

        let found = find_appimages(&places, Some(&bin.join("landau-bridge.AppImage")));
        let d = dl.display();
        assert_eq!(
            found,
            [
                format!("{d}/Landau-Bridge_1.1.1_amd64.appimage"),
                format!("{d}/landau-bridge_0.1.0_amd64.AppImage"),
            ]
        );
        // not from an AppImage (a deb, or a dev run): every copy is named
        assert_eq!(find_appimages(&places, None).len(), 3);
    }

    /// The slice review's H1: the stored list is read as it stands. A file since deleted, the running
    /// image and a copy of the same build drop out; the applications menu's copy, older than the
    /// running AppImage, is named apart (replaced by «نصب», never deleted); the links count live.
    #[test]
    fn the_notice_is_shown_as_things_stand_now() {
        let db = upgraded_from_v1_1_1();
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        std::fs::create_dir_all(h.join(".local/bin")).unwrap();
        std::fs::create_dir_all(h.join("Downloads")).unwrap();
        let menu = h.join(".local/bin/landau-bridge.AppImage");
        let running = h.join("Downloads/landau-bridge_1.2.0_amd64.AppImage");
        let old = h.join("Downloads/landau-bridge_0.1.0_amd64.AppImage");
        let same = h.join("landau-bridge-copy.AppImage");
        let gone = h.join("landau-bridge_1.0.0_amd64.AppImage");
        std::fs::write(&menu, b"v1.1.1 build").unwrap();
        std::fs::write(&running, b"the v1.2.0 build, longer").unwrap();
        std::fs::write(&old, b"0.1").unwrap();
        std::fs::write(&same, b"the v1.2.0 build, longer").unwrap();
        let listed: Vec<String> = [&menu, &old, &same, &gone]
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        let n = prepare(&db, &listed).unwrap().unwrap();

        let v = view(&db, n.clone(), Some(&running), &menu).unwrap();
        assert_eq!(v.appimages, [old.display().to_string()]);
        assert_eq!(v.menu_copy, Some(menu.display().to_string()));
        assert_eq!(v.links_unverified, 2);
        assert_eq!(
            (v.from_schema, v.threshold_from, v.threshold_to),
            (Some(3), Some(70), 85)
        );

        // after «نصب» the menu copy is this build: nothing to say about it
        std::fs::copy(&running, &menu).unwrap();
        let v = view(&db, n.clone(), Some(&running), &menu).unwrap();
        assert_eq!(v.menu_copy, None);
        // a link a job has since judged no longer waits
        db.with(|c| {
            c.execute(
                "UPDATE supplier_links SET state = 'VALID' WHERE name_key = 'a'",
                [],
            )
        })
        .unwrap();
        assert_eq!(
            view(&db, n.clone(), Some(&running), &menu)
                .unwrap()
                .links_unverified,
            1
        );

        // not from an AppImage (a deb): every other copy is listed, the menu copy among them
        let v = view(&db, n, None, &menu).unwrap();
        assert_eq!(v.menu_copy, None);
        assert_eq!(v.appimages.len(), 3, "{:?}", v.appimages);
    }

    /// The slice review's L4: the folders are read only when a new notice is about to be built.
    #[test]
    fn the_folders_are_read_only_when_a_new_notice_will_be_built() {
        let db = Db::in_memory().unwrap();
        assert!(!will_build(&db).unwrap(), "nothing to say");
        db.set_setting("threshold", "70").unwrap();
        assert!(will_build(&db).unwrap());
        db.set_setting("threshold", "90").unwrap();
        db.set_setting(UPGRADED_FROM_SCHEMA, "6").unwrap();
        assert!(!will_build(&db).unwrap(), "a plan-130 build");
        db.set_setting(UPGRADED_FROM_SCHEMA, "3").unwrap();
        assert!(will_build(&db).unwrap());
        prepare(&db, &[]).unwrap();
        db.set_setting("threshold", "60").unwrap();
        assert!(!will_build(&db).unwrap(), "a kept notice keeps its list");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_the_running_image_is_the_running_image() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        let running = h.join("landau-bridge-real.AppImage");
        std::fs::write(&running, b"x").unwrap();
        std::os::unix::fs::symlink(&running, h.join("landau-bridge.AppImage")).unwrap();
        assert!(find_appimages(&[h.to_path_buf()], Some(&running)).is_empty());
    }
}
