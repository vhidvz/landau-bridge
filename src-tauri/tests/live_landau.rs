//! Runs only with `--features live` and LND_API_URL / LND_APT_TOKEN / LND_STORE set.
#![cfg(feature = "live")]

use std::time::Duration;

use landau_bridge_lib::domain::Authenticity;
use landau_bridge_lib::errors::BridgeError;
use landau_bridge_lib::landau::{AttachBody, LandauClient, PatchBody};

fn client() -> Option<(LandauClient, String)> {
    let url = std::env::var("LND_API_URL").ok()?;
    let token = std::env::var("LND_APT_TOKEN").ok()?;
    let store = std::env::var("LND_STORE").ok()?;
    Some((LandauClient::new(&url, &token).unwrap(), store))
}

#[tokio::test]
async fn memberships_shelf_search_attach_patch_reprice_delete() {
    let Some((landau, store)) = client() else {
        eprintln!("skipped: LND_API_URL/LND_APT_TOKEN/LND_STORE not set");
        return;
    };
    let memberships = landau.memberships().await.expect("memberships");
    let mine = memberships
        .iter()
        .find(|m| m.stores.iter().any(|s| s.id == store))
        .expect("the store belongs to the key's business");
    assert!(mine.approved);

    let before = landau.shelf(&store).await.expect("shelf");

    let page = landau
        .registry_search("سیبک فرمان پراید", 100, 0)
        .await
        .expect("registry search");
    assert!(
        page.total >= 1 && !page.items.is_empty(),
        "the registry must be seeded (see docs/RUNBOOK.md)"
    );
    assert!(page.items.iter().any(|c| c.name.contains("سیبک")));
    let broad = landau
        .registry_search("پراید", 1, 0)
        .await
        .expect("registry search");
    assert!(
        broad.total > page.total,
        "a single generic token matches far more rows than a phrase"
    );
    // pick a product this store does not already hold as NEW|FAKE
    let held: std::collections::HashSet<_> = before
        .iter()
        .filter(|r| r.authenticity.as_deref() == Some("FAKE"))
        .map(|r| r.product.clone())
        .collect();
    let mut candidates = page
        .items
        .iter()
        .filter(|h| !held.contains(&h.id))
        .peekable();
    assert!(candidates.peek().is_some(), "an unstocked hit");

    // A store's earlier live run soft-deletes its attached row on cleanup, and Landau's own
    // uniqueness index (product, store) never frees up on a soft delete — a second attach of the
    // very same product 409s with "a removed stock row in this store still holds this part;
    // restore it instead" until someone restores it. The import job handles that by reading the
    // trash (`shelf_deleted`) and restoring the row (`restore`, exercised by `live_e2e`); this
    // test only checks the plain attach path, so on a 409 it simply moves on to the next
    // unstocked candidate.
    let (row, product) = loop {
        let product = candidates.next().expect(
            "every candidate in this search term was already attached-then-removed by a prior run",
        );
        match landau
            .attach(&AttachBody {
                product: product.id.clone(),
                store: store.clone(),
                inventory: 1,
                cost: 150_000,
                authenticity: Authenticity::Fake,
            })
            .await
        {
            Ok(row) => break (row, product),
            Err(BridgeError::DuplicateRow) => continue,
            Err(e) => panic!("attach: {e}"),
        }
    };
    assert_eq!(row.product, product.id);

    let missing = landau
        .await_visible(
            &store,
            std::slice::from_ref(&row.id),
            Duration::from_secs(15),
        )
        .await
        .expect("await");
    assert!(
        missing.is_empty(),
        "row not visible on the mirror within 15 s"
    );

    let visible = landau
        .shelf(&store)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.id == row.id)
        .unwrap();
    assert_eq!(visible.cost, Some(150_000));
    assert_eq!(
        visible.price,
        Some(150_000),
        "price = cost before any rule runs"
    );
    assert_ne!(visible.price_source.as_deref(), Some("MANUAL"));

    landau
        .patch(
            &row.id,
            &PatchBody {
                inventory: Some(4),
                cost: Some(160_000),
            },
        )
        .await
        .expect("patch");
    let _rules = landau.pricing_rules().await.expect("rules");
    let repriced = landau.reprice(&store, false).await.expect("reprice");
    assert!(
        repriced.iter().any(|r| r.stock == row.id),
        "reprice pages the whole store"
    );

    landau.delete(&row.id).await.expect("delete");
}

#[tokio::test]
async fn a_bad_key_is_unauthorized() {
    let Some((_, _)) = client() else {
        eprintln!("skipped");
        return;
    };
    let url = std::env::var("LND_API_URL").unwrap();
    let bad = LandauClient::new(&url, "apt-not-a-real-key").unwrap();
    assert_eq!(
        bad.memberships().await.unwrap_err().kind(),
        "LandauUnauthorized"
    );
}
