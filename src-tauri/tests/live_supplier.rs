//! Runs only with `--features live` and BAREZ_USER / BAREZ_PASS set. Never carries a value.
#![cfg(feature = "live")]

use std::sync::Arc;

use landau_bridge_lib::domain::InvoiceKind;
use landau_bridge_lib::supplier::{open, Credentials};

fn creds() -> Option<Credentials> {
    let username = std::env::var("BAREZ_USER").ok()?;
    let password = std::env::var("BAREZ_PASS").ok()?;
    Some(Credentials { username, password })
}

#[tokio::test]
async fn logs_in_reads_the_ledger_an_invoice_and_a_price() {
    let Some(creds) = creds() else {
        eprintln!("skipped: BAREZ_USER/BAREZ_PASS not set");
        return;
    };
    let supplier = open("barezpakhsh", creds).unwrap();
    supplier.login().await.expect("login");

    let headers = supplier
        .ledger(
            None,
            Arc::new(|done, total| eprintln!("ledger {done}/{total}")),
        )
        .await
        .expect("ledger");
    assert!(
        headers.len() > 100,
        "the fixture account has thousands of ledger rows"
    );
    assert!(headers
        .iter()
        .any(|h| h.kind == InvoiceKind::Sale && h.paid));
    let sale = headers
        .iter()
        .find(|h| h.kind == InvoiceKind::Sale && h.paid)
        .unwrap();
    assert!(sale.date_jalali.len() == 10, "{}", sale.date_jalali);

    let lines = supplier.invoice_lines(&sale.url).await.expect("lines");
    assert!(!lines.is_empty());
    assert!(lines.iter().all(|l| !l.code.is_empty() && l.qty > 0));

    let quote = supplier.price_of(&lines[0].code).await.expect("price");
    let quote = quote.expect("a purchased part is still in the price list");
    assert_eq!(quote.code, lines[0].code);
    assert!(quote.list_price > 0);
    assert_eq!(quote.vat_percent, Some(10.0));

    // The site's search is a substring match that does not rank the exact code first: «1012» sits
    // inside hundreds of longer codes (page 7 of the search on 2026-09-15), so a first-page-only
    // lookup answers `None` for a listed part. This walks the pages.
    let deep = supplier
        .price_of("1012")
        .await
        .expect("price")
        .expect("«1012» is still in the price list");
    assert_eq!(deep.code, "1012");

    // §14 b: the site's search is a per-word AND over at most seven words; the equal-name row is
    // what the bridge prices by. Measured 2026-09-15 on 150 raw names from the landau-parts map.
    let by_name = supplier
        .quotes_by_name(&lines[0].name)
        .await
        .expect("name search");
    assert!(
        by_name.iter().any(|q| q.code == lines[0].code),
        "the invoice line's own row is among {} rows for «{}»",
        by_name.len(),
        lines[0].name
    );
    assert!(!supplier
        .quotes_by_name("بلبرینگ کوچک بیرونی چرخ عقب 32011 نیسان NTN Z24 ژاپن")
        .await
        .expect("an over-long name is cut to seven words, never the error page")
        .is_empty());

    assert!(supplier
        .price_of("00000000-does-not-exist")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn wrong_password_is_bad_credentials() {
    let Some(creds) = creds() else {
        eprintln!("skipped: BAREZ_USER/BAREZ_PASS not set");
        return;
    };
    let supplier = open(
        "barezpakhsh",
        Credentials {
            password: "definitely-wrong".into(),
            ..creds
        },
    )
    .unwrap();
    let err = supplier.login().await.unwrap_err();
    assert_eq!(err.kind(), "SupplierBadCredentials");
}
