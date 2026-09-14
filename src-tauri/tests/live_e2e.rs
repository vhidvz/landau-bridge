//! Whole-flow test against the real Barezpakhsh and the running Landau (feature `live`).
//! Env: LND_API_URL, LND_APT_TOKEN, LND_STORE, BAREZ_USER, BAREZ_PASS.
#![cfg(feature = "live")]

use landau_bridge_lib::db::Db;
use landau_bridge_lib::domain::ImportMode;
use landau_bridge_lib::jobs::import::{self, ImportParams, Outcome};
use landau_bridge_lib::jobs::{refresh, JobCtx, Services};
use landau_bridge_lib::landau::{LandauClient, PatchBody};
use landau_bridge_lib::matching::Matcher;
use landau_bridge_lib::secrets::SecretStore;
use landau_bridge_lib::settings::{Settings, SettingsPatch, SUPPLIER_ID};
use landau_bridge_lib::supplier;

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

/// Resolve a paused review the way a seller in a hurry would: the top candidate of the first
/// `picks` items is accepted (a USER decision, persisted), every other item is left in REVIEW so
/// `commit` skips it and the next run asks again.
fn accept_top_candidates(db: &Db, settings: &Settings, job_id: i64, picks: usize) {
    let matcher = Matcher::new(db.clone(), SUPPLIER_ID, settings.threshold);
    let mut accepted = 0usize;
    for item in db
        .job_items(job_id)
        .unwrap()
        .into_iter()
        .filter(|i| i.status == "REVIEW")
    {
        if accepted >= picks {
            break;
        }
        let Some(top) = item
            .candidates
            .as_ref()
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .cloned()
        else {
            continue;
        };
        let id = top["candidate"]["id"].as_str().unwrap().to_string();
        let name = top["candidate"]["name"].as_str().unwrap().to_string();
        matcher
            .record_user_pick(
                &item.code,
                &item.name,
                &id,
                &name,
                top["similarity"].as_i64(),
            )
            .unwrap();
        db.update_job_item(
            item.id,
            &[
                ("product_id", landau_bridge_lib::jobs::t(&id)),
                ("product_name", landau_bridge_lib::jobs::t(&name)),
                ("status", landau_bridge_lib::jobs::t("PLANNED")),
            ],
        )
        .unwrap();
        accepted += 1;
    }
}

#[tokio::test]
async fn zero_import_then_refresh_round_trip() {
    let (Some(url), Some(token), Some(store), Some(user), Some(pass)) = (
        env("LND_API_URL"),
        env("LND_APT_TOKEN"),
        env("LND_STORE"),
        env("BAREZ_USER"),
        env("BAREZ_PASS"),
    ) else {
        eprintln!("skipped: live env not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("bridge.sqlite")).unwrap();
    let secrets = SecretStore::file_only(dir.path());
    Settings::save(
        &db,
        &secrets,
        &SettingsPatch {
            landau_url: Some(url.clone()),
            landau_token: Some(token.clone()),
            store: Some(store.clone()),
            supplier_user: Some(user.clone()),
            supplier_password: Some(pass.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let settings = Settings::load(&db, &secrets).unwrap();
    let services = || Services {
        landau: LandauClient::new(&url, &token).unwrap(),
        supplier: supplier::open(SUPPLIER_ID, settings.require_supplier().unwrap()).unwrap(),
        settings: settings.clone(),
    };

    // ---- import, zero mode, three invoices
    let job_id = db
        .create_job("IMPORT", Some("ZERO"), None, &store, false)
        .unwrap();
    let ctx = JobCtx {
        db: db.clone(),
        app: None,
        job_id,
        cancel: Default::default(),
    };
    let svc = services();
    let outcome = import::start(
        &ctx,
        &svc,
        &ImportParams {
            mode: ImportMode::Zero,
            since: None,
            fetch_limit: Some(3),
        },
    )
    .await
    .expect("import start");

    if let Outcome::Review(n) = outcome {
        eprintln!(
            "{n} item(s) in review; accepting the top candidate of the first, deferring the rest"
        );
        accept_top_candidates(&db, &settings, job_id, 1);
        import::commit(&ctx, &svc).await.expect("import commit");
    }
    let detail = db.job_detail(job_id).unwrap();
    let done: Vec<_> = detail.items.iter().filter(|i| i.status == "DONE").collect();
    for i in &done {
        eprintln!(
            "DONE code={} cost={:?} price_after={:?} verdict={:?}",
            i.code, i.cost, i.price_after, i.reprice_verdict
        );
    }
    assert!(
        !done.is_empty(),
        "at least one row reached the shelf: {:?}",
        detail
            .items
            .iter()
            .map(|i| (&i.status, &i.error_kind))
            .collect::<Vec<_>>()
    );
    assert!(done
        .iter()
        .all(|i| i.stock_id.is_some() && i.cost.is_some()));
    assert!(
        done.iter()
            .all(|i| i.qty_after == Some(0) || i.action.as_deref() == Some("PATCH")),
        "zero mode"
    );
    assert!(detail.events.iter().any(|e| e.phase == "REPRICE"));
    // a `REPRICED:<next>` verdict names Landau's own new price; its arithmetic is not
    // re-implemented here, so the check stays loose: whole-thousand and never below cost.
    for i in &done {
        if let Some(next) = i
            .reprice_verdict
            .as_deref()
            .and_then(|v| v.strip_prefix("REPRICED:"))
            .and_then(|n| n.parse::<i64>().ok())
        {
            assert_eq!(i.price_after, Some(next), "REPRICED price_after == next");
            assert!(
                next % 1000 == 0 && next >= i.cost.unwrap_or(0),
                "next={next} cost={:?}",
                i.cost
            );
        }
    }

    // ---- refresh
    let job_id = db.create_job("REFRESH", None, None, &store, false).unwrap();
    let ctx = JobCtx {
        db: db.clone(),
        app: None,
        job_id,
        cancel: Default::default(),
    };
    let svc = services();
    refresh::run(&ctx, &svc).await.expect("refresh");
    let detail = db.job_detail(job_id).unwrap();
    assert!(
        detail
            .items
            .iter()
            .any(|i| !i.code.is_empty() && i.status == "DONE"),
        "linked rows were refreshed"
    );

    // ---- cleanup: soft-delete rows this test attached
    let landau = LandauClient::new(&url, &token).unwrap();
    for item in db
        .job_items(job_id - 1)
        .unwrap()
        .iter()
        .filter(|i| i.action.as_deref() == Some("ATTACH"))
    {
        if let Some(id) = &item.stock_id {
            let _ = landau.delete(id).await;
        }
    }
}

/// Spec §7.3: quantity addition is guarded per `(invoice, code)` line — a second run over the
/// same invoices must not add the same quantities again.
#[tokio::test]
async fn quantity_import_is_idempotent_across_runs() {
    let (Some(url), Some(token), Some(store), Some(user), Some(pass)) = (
        env("LND_API_URL"),
        env("LND_APT_TOKEN"),
        env("LND_STORE"),
        env("BAREZ_USER"),
        env("BAREZ_PASS"),
    ) else {
        eprintln!("skipped: live env not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("bridge.sqlite")).unwrap();
    let secrets = SecretStore::file_only(dir.path());
    Settings::save(
        &db,
        &secrets,
        &SettingsPatch {
            landau_url: Some(url.clone()),
            landau_token: Some(token.clone()),
            store: Some(store.clone()),
            supplier_user: Some(user.clone()),
            supplier_password: Some(pass.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let settings = Settings::load(&db, &secrets).unwrap();
    let services = || Services {
        landau: LandauClient::new(&url, &token).unwrap(),
        supplier: supplier::open(SUPPLIER_ID, settings.require_supplier().unwrap()).unwrap(),
        settings: settings.clone(),
    };
    const SINCE: &str = "1390/01/01";
    let run = |fetch_limit: usize, picks: usize| {
        let db = db.clone();
        let store = store.clone();
        let settings = settings.clone();
        let svc = services();
        async move {
            let job_id = db
                .create_job("IMPORT", Some("QUANTITY"), Some(SINCE), &store, false)
                .unwrap();
            let ctx = JobCtx {
                db: db.clone(),
                app: None,
                job_id,
                cancel: Default::default(),
            };
            let outcome = import::start(
                &ctx,
                &svc,
                &ImportParams {
                    mode: ImportMode::Quantity,
                    since: Some(SINCE.into()),
                    fetch_limit: Some(fetch_limit),
                },
            )
            .await
            .expect("import start");
            if let Outcome::Review(n) = outcome {
                eprintln!(
                    "run #{job_id}: {n} item(s) in review; accepting {picks}, deferring the rest"
                );
                accept_top_candidates(&db, &settings, job_id, picks);
                import::commit(&ctx, &svc).await.expect("import commit");
            }
            job_id
        }
    };

    // ---- run 1: the two oldest invoices, quantities added to the shelf
    let first = run(2, 3).await;
    let detail1 = db.job_detail(first).unwrap();
    let done1: Vec<_> = detail1
        .items
        .iter()
        .filter(|i| i.status == "DONE")
        .cloned()
        .collect();
    for i in &done1 {
        eprintln!(
            "run 1 DONE code={} qty={} action={:?} qty_before={:?} qty_after={:?} stock={:?}",
            i.code, i.qty, i.action, i.qty_before, i.qty_after, i.stock_id
        );
    }
    assert!(
        done1.iter().any(|i| i.qty_after.unwrap_or(0) > 0),
        "at least one quantity landed on the shelf: {:?}",
        detail1
            .items
            .iter()
            .map(|i| (&i.code, &i.status, &i.error_kind))
            .collect::<Vec<_>>()
    );
    // a row restored from the trash is a fresh attach (its old figure is gone); every other row
    // is the shelf figure plus the net quantity, floored at zero.
    let restored: Vec<i64> = detail1
        .events
        .iter()
        .filter(|e| e.level == "INFO" && e.message.contains("بازیابی"))
        .filter_map(|e| e.detail.as_ref()?.get("item_id")?.as_i64())
        .collect();
    for i in &done1 {
        let base = if restored.contains(&i.id) {
            0
        } else {
            i.qty_before.unwrap_or(0)
        };
        assert_eq!(
            i.qty_after,
            Some((base + i.qty).max(0)),
            "{}: inventory after the write (restored: {})",
            i.code,
            restored.contains(&i.id)
        );
    }

    // ---- run 2: same window, nothing new fetched (a capped fetch would continue with the next
    // invoices, whose lines legitimately add to the same codes); every line of run 1's written
    // items is guarded, so they must not be counted again.
    let second = run(0, 0).await;
    let detail2 = db.job_detail(second).unwrap();
    let guarded = db
        .list_errors(1000)
        .unwrap()
        .into_iter()
        .any(|e| e.job_id == Some(second) && e.kind == "InvoiceAlreadyImported")
        || detail2.events.iter().any(|e| {
            e.level == "WARN"
                && e.detail
                    .as_ref()
                    .and_then(|d| d.get("invoice_ids"))
                    .and_then(|v| v.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false)
                && e.message.contains("قبلاً منتقل شده")
        });
    assert!(
        guarded,
        "run 2 warns InvoiceAlreadyImported: {:?}",
        detail2
            .events
            .iter()
            .map(|e| (&e.level, &e.message))
            .collect::<Vec<_>>()
    );
    for i in &done1 {
        match detail2.items.iter().find(|j| j.code == i.code) {
            None => {}
            Some(j) => assert!(
                j.status == "SKIPPED" || j.qty_after == i.qty_after,
                "{}: run 2 must not add again (run 1 {:?}, run 2 {:?}/{:?})",
                i.code,
                i.qty_after,
                j.status,
                j.qty_after
            ),
        }
    }
    // the shelf itself, after run 2, still shows run 1's figures
    let landau = LandauClient::new(&url, &token).unwrap();
    let shelf = landau.shelf(&store).await.expect("shelf");
    for i in &done1 {
        let stock_id = i.stock_id.as_deref().unwrap();
        let row = shelf
            .iter()
            .find(|r| r.id == stock_id)
            .unwrap_or_else(|| panic!("{}: row {stock_id} still on the shelf", i.code));
        assert_eq!(
            Some(row.inventory),
            i.qty_after,
            "{}: shelf inventory unchanged by run 2",
            i.code
        );
    }

    // ---- cleanup: soft-delete rows these runs attached (a restored row is one of them) and put
    // the quantities the runs added to existing rows back, so the fixture store is left where it
    // was found.
    for job_id in [first, second] {
        let detail = db.job_detail(job_id).unwrap();
        let restored: Vec<i64> = detail
            .events
            .iter()
            .filter(|e| e.level == "INFO" && e.message.contains("بازیابی"))
            .filter_map(|e| e.detail.as_ref()?.get("item_id")?.as_i64())
            .collect();
        for item in detail.items.iter().filter(|i| i.status == "DONE") {
            let Some(id) = &item.stock_id else { continue };
            match (item.action.as_deref(), item.qty_before) {
                (Some("ATTACH"), _) => {
                    let _ = landau.delete(id).await;
                }
                (Some("PATCH"), _) if restored.contains(&item.id) => {
                    let _ = landau.delete(id).await;
                }
                (Some("PATCH"), Some(before)) if item.qty_after != Some(before) => {
                    let _ = landau
                        .patch(
                            id,
                            &PatchBody {
                                inventory: Some(before),
                                cost: None,
                            },
                        )
                        .await;
                }
                _ => {}
            }
        }
    }
}
