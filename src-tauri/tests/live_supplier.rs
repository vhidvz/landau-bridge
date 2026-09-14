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
