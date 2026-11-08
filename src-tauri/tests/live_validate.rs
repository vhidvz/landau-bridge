//! VALIDATE against the running Landau, read-only to it (feature `live`, plan 130 slice 2): the
//! registry read whole through the seller's picker, the bridge's local picker compared with
//! Landau's own, and — given a *copy* of an install's database — every stored link judged.
//! Env: LND_API_URL, LND_APT_TOKEN; BRIDGE_DB_COPY for the second test (a copy, never the live
//! file: VALIDATE writes link states into it).
#![cfg(feature = "live")]

use std::collections::BTreeMap;

use landau_bridge_lib::db::Db;
use landau_bridge_lib::jobs::{refresh, JobCtx};
use landau_bridge_lib::landau::LandauClient;
use landau_bridge_lib::matching::registry::{read_whole, Registry};
use landau_bridge_lib::settings::SUPPLIER_ID;

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

async fn registry(client: &LandauClient) -> Registry {
    let page = |skip: usize| async move { client.registry_search("", 100, skip).await };
    let started = std::time::Instant::now();
    let r = read_whole(&page).await.expect("the registry read whole");
    println!(
        "registry: {} definitions in {:?}",
        r.len(),
        started.elapsed()
    );
    r
}

/// Terms of every shape the matcher sends: whole names, bigrams, single words, Latin codes,
/// digits, a brand inside another, and Arabic letterforms the picker reads as Persian.
const TERMS: &[&str] = &[
    "بلبرینگ 44649",
    "لنت جلو",
    "لنت جلو پرايد",
    "پراید",
    "عظام",
    "PSN عظام",
    "ELMA7",
    "elma7",
    "الما۷",
    "بست کمری",
    "cw",
    "6006",
    "405",
    "تسمه تایم",
    "واتر پمپ",
    "فیلتر روغن",
    "یاتاقان",
    "سنسور",
    "کلاچ",
    "بوش",
    "آراد پارت",
    "زیپ تای",
];

#[tokio::test]
async fn the_local_picker_answers_as_landau_does() {
    let (Some(url), Some(token)) = (env("LND_API_URL"), env("LND_APT_TOKEN")) else {
        eprintln!("skipped: LND_API_URL / LND_APT_TOKEN not set");
        return;
    };
    let client = LandauClient::new(&url, &token).unwrap();
    let r = registry(&client).await;
    for term in TERMS {
        for skip in [0, 100] {
            let remote = client.registry_search(term, 100, skip).await.unwrap();
            let local = r.search(term, 100, skip);
            let ids = |p: &landau_bridge_lib::landau::RegistryPage| {
                p.items.iter().map(|c| c.id.clone()).collect::<Vec<_>>()
            };
            assert_eq!(remote.total, local.total, "total for «{term}»");
            assert_eq!(ids(&remote), ids(&local), "page at {skip} for «{term}»");
        }
        println!(
            "«{term}»: {} hit(s), same both ways",
            r.search(term, 1, 0).total
        );
    }
}

#[tokio::test]
async fn every_link_of_an_install_copy_is_judged_against_the_live_registry() {
    let (Some(url), Some(token), Some(path)) = (
        env("LND_API_URL"),
        env("LND_APT_TOKEN"),
        env("BRIDGE_DB_COPY"),
    ) else {
        eprintln!("skipped: LND_API_URL / LND_APT_TOKEN / BRIDGE_DB_COPY not set");
        return;
    };
    assert!(
        !path.contains("ir.landau.bridge"),
        "BRIDGE_DB_COPY must be a copy, never the live data folder"
    );
    let db = Db::open(std::path::Path::new(&path)).unwrap();
    db.with(|c| {
        c.execute_batch(
            "UPDATE jobs SET status = 'CANCELLED' WHERE status IN ('RUNNING', 'REVIEW')",
        )
    })
    .unwrap();
    let before: Vec<_> = db.list_matches(SUPPLIER_ID).unwrap();
    let job_id = db.create_job("REFRESH", None, None, "copy", false).unwrap();
    let ctx = JobCtx {
        db: db.clone(),
        app: None,
        job_id,
        cancel: Default::default(),
    };
    let client = LandauClient::new(&url, &token).unwrap();
    let r = registry(&client).await;
    let threshold = landau_bridge_lib::settings::Settings::load(
        &db,
        &landau_bridge_lib::secrets::SecretStore::open(std::path::Path::new("/nonexistent")),
    )
    .map(|s| s.threshold)
    .unwrap_or(85);
    let started = std::time::Instant::now();
    let by_product = refresh::validate_links(&ctx, &r, threshold).await.unwrap();
    println!(
        "validated {} link(s) in {:?}; {} product(s) carry a link",
        before.iter().filter(|m| m.product_id.is_some()).count(),
        started.elapsed(),
        by_product.len()
    );
    let after = db.list_matches(SUPPLIER_ID).unwrap();
    assert_eq!(after.len(), before.len(), "no link appears or disappears");
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for m in &after {
        *counts.entry((m.how.clone(), m.state.clone())).or_default() += 1;
    }
    for ((how, state), n) in &counts {
        println!("{how:5} {state:13} {n}");
    }
    let rebuilt = after.iter().filter(|m| m.prev_product_id.is_some()).count();
    println!("rebuilt onto a new id: {rebuilt}");
    let several = by_product.values().filter(|l| l.len() > 1).count();
    println!("products with several links (collision candidates): {several}");
    for m in after
        .iter()
        .filter(|m| m.state == "VALID" || m.state == "REBUILT")
    {
        assert!(
            r.get(m.product_id.as_deref().unwrap()).is_some(),
            "a proven link's product exists"
        );
        assert_eq!(m.validated_job, Some(job_id));
    }
}
