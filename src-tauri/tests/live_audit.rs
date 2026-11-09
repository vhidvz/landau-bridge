//! The audit against the running Landau and Barezpakhsh (feature `live`, plan 130 task 9): the
//! incident's bearing row staged on the deck seller's shelf at the hurried pick's 5,500,000, the two
//! incident links on a scratch install, and the audit run over the real registry, shelf and today's
//! quotes. The audit itself must write nothing: the shelf and the install read the same after it.
//! Env: LND_API_URL, LND_APT_TOKEN, LND_STORE, BAREZ_USER, BAREZ_PASS; AUDIT_KEEP_DIR keeps the
//! scratch install and the CSV for a driven check.
#![cfg(feature = "live")]

use landau_bridge_lib::audit::{self, Doubt, Sources};
use landau_bridge_lib::db::Db;
use landau_bridge_lib::domain::Authenticity;
use landau_bridge_lib::errors::BridgeError;
use landau_bridge_lib::inspect::Read;
use landau_bridge_lib::landau::{fixture, AttachBody, LandauClient, PatchBody, ShelfRow};
use landau_bridge_lib::matching::registry::read_whole;
use landau_bridge_lib::settings::SUPPLIER_ID;
use landau_bridge_lib::supplier::{self, Credentials};

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

const RIGHT: (&str, &str) = (
    "33102018",
    "بلبرينگ بزرگ چرخ عقب پرايد جدبد (بلبرينگ بزرگ چرخ جلو پيکان) 44649 PSN عظام (96) 3154028",
);
const WRONG: (&str, &str) = (
    "33102017",
    "بلبرينگ چرخ جلو پرايد قديم 69349 PSN عظام (54) 3154027",
);

fn shelf_key(rows: &[ShelfRow]) -> Vec<(String, i64, Option<i64>, Option<i64>)> {
    let mut v: Vec<_> = rows
        .iter()
        .map(|r| (r.id.clone(), r.inventory, r.cost, r.price))
        .collect();
    v.sort();
    v
}

#[tokio::test]
async fn the_audit_lists_the_staged_incident_row_and_writes_nothing() {
    let (Some(url), Some(token), Some(store), Some(user), Some(pass)) = (
        env("LND_API_URL"),
        env("LND_APT_TOKEN"),
        env("LND_STORE"),
        env("BAREZ_USER"),
        env("BAREZ_PASS"),
    ) else {
        println!("skipped: LND_API_URL, LND_APT_TOKEN, LND_STORE, BAREZ_USER, BAREZ_PASS");
        return;
    };
    let client = LandauClient::new(&url, &token).unwrap();
    let page = |skip: usize| {
        let client = &client;
        async move { client.registry_search("", 100, skip).await }
    };
    let registry = read_whole(&page).await.expect("the registry read whole");
    let bearing = registry
        .search("بلبرینگ بزرگ چرخ عقب پراید و چرخ جلو پیکان", 100, 0)
        .items
        .into_iter()
        .find(|c| c.brand.as_deref() == Some("PSN عظام"))
        .expect("the 44649 PSN عظام bearing is in the registry");
    println!("bearing {} «{}»", bearing.id, bearing.name);

    // the incident's row, staged through the live suites' door — or, when an earlier run left it
    // in the trash (Landau's index still holds a removed row), restored and set to the same figures
    let body = AttachBody {
        product: bearing.id.clone(),
        store: store.clone(),
        inventory: 0,
        cost: 5_500_000,
        price: 6_325_000,
        authenticity: Authenticity::Original,
    };
    let staged = match fixture::attach(&client, &body).await {
        Ok(row) => row,
        Err(BridgeError::DuplicateRow) => {
            let trash = client.shelf_deleted(&store).await.unwrap();
            let removed = trash
                .iter()
                .find(|r| r.product == bearing.id && r.authenticity.as_deref() == Some("ORIGINAL"))
                .expect("the deck holds the row in its trash")
                .clone();
            fixture::restore(&client, &removed.id).await.unwrap();
            let patch = PatchBody {
                inventory: Some(0),
                cost: Some(5_500_000),
            };
            fixture::patch(&client, &removed.id, &patch).await.unwrap()
        }
        Err(e) => panic!("staging refused: {e:?}"),
    };

    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("bridge.sqlite")).unwrap();
    for (item, how) in [(RIGHT, "USER"), (WRONG, "USER")] {
        db.set_match(
            SUPPLIER_ID,
            item.1,
            item.0,
            Some((&bearing.id, &bearing.name)),
            how,
            Some(56),
        )
        .unwrap();
    }
    // the incident's provenance: the hurried pick wrote the row
    let job = db
        .create_job("IMPORT", Some("ZERO"), None, &store, false)
        .unwrap();
    let id = db.add_job_item(job, WRONG.0, WRONG.1, 0).unwrap();
    db.update_job_item(
        id,
        &[
            ("stock_id", rusqlite::types::Value::Text(staged.id.clone())),
            ("cost", rusqlite::types::Value::Integer(5_500_000)),
            ("status", rusqlite::types::Value::Text("DONE".into())),
        ],
    )
    .unwrap();
    db.set_job_status(job, "DONE", "REPORT").unwrap();

    // the shelf is read from a mirror about a second behind the write: wait for the staged row
    let mut shelf = Vec::new();
    for _ in 0..30 {
        shelf = client.shelf(&store).await.unwrap();
        if shelf.iter().any(|r| r.id == staged.id) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    assert!(
        shelf.iter().any(|r| r.id == staged.id),
        "the staged row reached the mirror"
    );
    let shelf_before = shelf_key(&shelf);
    let links_before = db.list_matches(SUPPLIER_ID).unwrap();
    let sup = supplier::open(
        SUPPLIER_ID,
        Credentials {
            username: user,
            password: pass,
        },
    )
    .unwrap();
    let started = std::time::Instant::now();
    let outcome = audit::audit(&Sources {
        db: &db,
        registry: &registry,
        registry_read_at: "now",
        landau: &client,
        store: &store,
        supplier: Ok(sup.as_ref()),
        quote_lock: None,
        vat_percent: 10.0,
        threshold: 85,
    })
    .await;
    let shelf_after = shelf_key(&client.shelf(&store).await.unwrap());
    let links_after = db.list_matches(SUPPLIER_ID).unwrap();
    let csv: Option<String> = outcome.as_ref().ok().map(audit::csv);
    if let Some(keep) = env("AUDIT_KEEP_DIR") {
        std::fs::create_dir_all(&keep).unwrap();
        db.with(|c| c.execute("VACUUM INTO ?1", [format!("{keep}/bridge.sqlite")]))
            .unwrap();
        if let Some(text) = &csv {
            std::fs::write(format!("{keep}/audit.csv"), text).unwrap();
        }
    }
    // the stage leaves nothing behind, whatever the audit answered
    fixture::delete(&client, &staged.id).await.unwrap();

    let a = outcome.expect("the audit ran");
    println!(
        "audit in {:?}: examined {}, consistent {}, unlinked {}, listed {}",
        started.elapsed(),
        a.examined,
        a.consistent,
        a.unlinked,
        a.rows.len()
    );
    println!("{}", csv.unwrap_or_default());
    assert_eq!(
        shelf_after, shelf_before,
        "the audit wrote nothing to Landau"
    );
    assert_eq!(links_after, links_before, "nor to the install");

    let row = a
        .rows
        .iter()
        .find(|r| r.stock_id == staged.id)
        .expect("the staged row is listed");
    assert_eq!(row.doubts, [Doubt::UnprovenLink]);
    let by = |code: &str| row.links.iter().find(|l| l.code == code).unwrap();
    assert!(by(RIGHT.0).proven, "{:?}", by(RIGHT.0));
    assert!(!by(WRONG.0).proven);
    // the evidence's cost is the right bearing's quote today, whatever Barezpakhsh lists now
    let Some(Read::Known(Some(q))) = &by(RIGHT.0).today else {
        panic!("no quote for the right bearing: {:?}", by(RIGHT.0).today)
    };
    assert_eq!(
        row.should_cost,
        Some(q.cost.max(if q.available { 0 } else { 5_500_000 }))
    );
    assert_eq!(row.difference, row.should_cost.map(|s| s - 5_500_000));
    assert_eq!(
        a.examined, 1,
        "the deck's own rows carry no link of this install"
    );
}
