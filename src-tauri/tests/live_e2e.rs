//! Whole-flow test against the real Barezpakhsh and the running Landau (feature `live`).
//! Env: LND_API_URL, LND_APT_TOKEN, LND_STORE, BAREZ_USER, BAREZ_PASS.
#![cfg(feature = "live")]

use landau_bridge_lib::db::Db;
use landau_bridge_lib::domain::ImportMode;
use landau_bridge_lib::jobs::import::{self, ImportParams, Outcome};
use landau_bridge_lib::jobs::{refresh, JobCtx, Services};
use landau_bridge_lib::landau::LandauClient;
use landau_bridge_lib::matching::Matcher;
use landau_bridge_lib::secrets::SecretStore;
use landau_bridge_lib::settings::{Settings, SettingsPatch, SUPPLIER_ID};
use landau_bridge_lib::supplier;

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
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
        let matcher = Matcher::new(db.clone(), SUPPLIER_ID, settings.threshold);
        let mut first = true;
        for item in db
            .job_items(job_id)
            .unwrap()
            .into_iter()
            .filter(|i| i.status == "REVIEW")
        {
            let top = item
                .candidates
                .as_ref()
                .and_then(|c| c.as_array())
                .and_then(|a| a.first())
                .cloned();
            // stays REVIEW → skipped by commit unless this is the first item, which gets the
            // top candidate accepted (single-arm match rewritten as `if let` for clippy).
            if let (true, Some(top)) = (first, top) {
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
                first = false;
            }
        }
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
