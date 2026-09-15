//! The Barezpakhsh session (spec §4): anti-forgery login, ledger paging, invoice pages, price
//! search. Transient failures are retried (spec §3) and an expired session triggers one
//! automatic re-login on every authenticated request. Every request logs method, path and
//! status; never the credentials.

use std::collections::HashSet;
use std::time::Duration;

use reqwest::{Client as Http, Url};
use serde_json::json;
use tokio::sync::Mutex;

use super::parse::{self, BuyContext};
use crate::domain::{InvoiceHeader, InvoiceLine, PriceQuote};
use crate::errors::{BridgeError, Result};
use crate::supplier::{Credentials, Progress, Supplier};

pub const ID: &str = "barezpakhsh";
pub const BASE_URL: &str = "https://33314466.ir";
/// Earliest date the statement form is asked for when no bound is given — "every invoice to date".
const LEDGER_FROM: &str = "1390/01/01";
const USER_AGENT: &str = "LandauBridge/0.1 (+https://landau.ir)";

/// Up to 3 attempts total, sleeping 1s then 3s between them; see [`retryable`] for what qualifies.
const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(3)];

/// Rows per `/Shop/Share/ItemsSearchGradual` page — the largest size the site's own form offers.
const SEARCH_PAGE_SIZE: usize = 50;
/// Upper bound on search pages walked for one code. The catalogue's most substring-prone code
/// («A7») sits inside ~2 100 other codes and names, i.e. 43 pages, so this is never reached in
/// practice; it only guards against a server that keeps answering full pages.
pub(crate) const MAX_SEARCH_PAGES: u32 = 100;

/// Retry policy for the transport layer (spec §3): a transport error (`None`) and statuses 404,
/// 408, 429 and any 5xx are retried. A redirect/success is never retried, and neither is a 403 —
/// a 403 with the block page becomes [`BridgeError::SupplierBlocked`] immediately and any other
/// 403 is returned as-is; retrying either would not change the outcome.
pub(crate) fn retryable(status: Option<u16>) -> bool {
    match status {
        None => true,
        Some(404) | Some(408) | Some(429) => true,
        Some(s) => (500..600).contains(&s),
    }
}

/// Keeps the first (oldest, by `date_jalali` then `number`) occurrence of each `(number, kind)`
/// pair — pages read around a boundary can print the same invoice twice.
pub(crate) fn dedup_headers(mut headers: Vec<InvoiceHeader>) -> Vec<InvoiceHeader> {
    headers.sort_by(|a, b| {
        a.date_jalali
            .cmp(&b.date_jalali)
            .then(a.number.cmp(&b.number))
    });
    let mut seen: HashSet<(String, &'static str)> = HashSet::new();
    headers.retain(|h| seen.insert((h.number.clone(), h.kind.as_str())));
    headers
}

/// The row whose code is exactly `code`, walking the search pages in order. The site's search is
/// a substring match over codes and names that does not rank the exact code first — verified
/// live on 2026-09-15: «50053» sat on page 2 behind fifty longer codes containing it, «1012» on
/// page 7 — so the first page alone answers `None` for a code the site does list. Stops at a page
/// shorter than `page_size` (the results ran out) or at [`MAX_SEARCH_PAGES`].
pub(crate) async fn find_across_pages<F, Fut>(
    code: &str,
    page_size: usize,
    fetch: F,
) -> Result<Option<PriceQuote>>
where
    F: Fn(u32) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<PriceQuote>>>,
{
    for index in 1..=MAX_SEARCH_PAGES {
        let page = fetch(index).await?;
        let rows = page.len();
        if let Some(quote) = page.into_iter().find(|q| q.code == code) {
            return Ok(Some(quote));
        }
        if rows < page_size {
            return Ok(None);
        }
    }
    tracing::warn!(
        code,
        pages = MAX_SEARCH_PAGES,
        "barezpakhsh price search gave up: every page was full"
    );
    Ok(None)
}

/// Defaults the price search needs, learned once per session from the «ثبت فاکتور» page.
#[derive(Debug, Clone)]
struct BuySession {
    ctx: BuyContext,
    car: String,
    brand: String,
    group: String,
}

pub struct Client {
    http: Http,
    base: Url,
    creds: Credentials,
    logged_in: Mutex<bool>,
    buy: Mutex<Option<BuySession>>,
}

impl Client {
    pub fn new(creds: Credentials) -> Result<Self> {
        Self::with_base(creds, BASE_URL)
    }

    pub fn with_base(creds: Credentials, base: &str) -> Result<Self> {
        let http = Http::builder()
            .cookie_store(true)
            .no_proxy()
            .gzip(true)
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(90))
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| BridgeError::SupplierUnreachable(e.to_string()))?;
        let base = Url::parse(base).map_err(|e| BridgeError::Other(format!("base url: {e}")))?;
        Ok(Client {
            http,
            base,
            creds,
            logged_in: Mutex::new(false),
            buy: Mutex::new(None),
        })
    }

    fn url(&self, path: &str) -> Result<Url> {
        self.base
            .join(path)
            .map_err(|e| BridgeError::Other(format!("url {path}: {e}")))
    }

    fn unreachable(e: reqwest::Error) -> BridgeError {
        BridgeError::SupplierUnreachable(e.without_url().to_string())
    }

    /// The status reqwest ends on after following redirects. Anything but a 2xx that survived
    /// the retries (a 401, a 403 without the block page, a 5xx on the last attempt) is the site
    /// failing to answer, not a page to parse. The login flow keeps reading the *final URL* to
    /// tell a successful login (redirected to `/Shop`) from a 200 login page (bad credentials);
    /// both are 2xx and pass here.
    fn check_status(path: &str, status: reqwest::StatusCode) -> Result<()> {
        if status.is_success() {
            Ok(())
        } else {
            Err(BridgeError::SupplierUnreachable(format!(
                "{path} answered {status}"
            )))
        }
    }

    /// Sends a request built fresh on each attempt (so a form/JSON body can be resent),
    /// retrying per [`retryable`]. Never interprets the response itself — block detection,
    /// session-expiry detection and success handling all stay with the caller.
    async fn send_with_retry<B>(&self, path: &str, build: B) -> Result<reqwest::Response>
    where
        B: Fn() -> reqwest::RequestBuilder,
    {
        let mut attempt: usize = 0;
        loop {
            attempt += 1;
            let outcome = build().send().await;
            let status = outcome.as_ref().ok().map(|r| r.status().as_u16());
            if attempt <= RETRY_DELAYS.len() && retryable(status) {
                match &outcome {
                    Ok(res) => {
                        tracing::warn!(
                            path,
                            attempt,
                            status = res.status().as_u16(),
                            "barezpakhsh retrying"
                        );
                    }
                    Err(e) => tracing::warn!(path, attempt, error = %e, "barezpakhsh retrying"),
                }
                tokio::time::sleep(RETRY_DELAYS[attempt - 1]).await;
                continue;
            }
            return outcome.map_err(Self::unreachable);
        }
    }

    async fn get(&self, path: &str) -> Result<(Url, String)> {
        let req_url = self.url(path)?;
        let res = self
            .send_with_retry(path, || self.http.get(req_url.clone()))
            .await?;
        let status = res.status();
        let url = res.url().clone();
        let body = res.text().await.map_err(Self::unreachable)?;
        tracing::debug!(method = "GET", path, %status, "barezpakhsh");
        if status.as_u16() == 403 && parse::is_blocked_page(&body) {
            return Err(BridgeError::SupplierBlocked);
        }
        Self::check_status(path, status)?;
        Ok((url, body))
    }

    async fn post_form(&self, path: &str, fields: &[(String, String)]) -> Result<(Url, String)> {
        let req_url = self.url(path)?;
        let res = self
            .send_with_retry(path, || self.http.post(req_url.clone()).form(fields))
            .await?;
        let status = res.status();
        let url = res.url().clone();
        let body = res.text().await.map_err(Self::unreachable)?;
        tracing::debug!(method = "POST", path, %status, "barezpakhsh");
        if status.as_u16() == 403 && parse::is_blocked_page(&body) {
            return Err(BridgeError::SupplierBlocked);
        }
        Self::check_status(path, status)?;
        Ok((url, body))
    }

    async fn post_json(&self, path: &str, body: &serde_json::Value) -> Result<String> {
        let req_url = self.url(path)?;
        let payload = body.to_string();
        let res = self
            .send_with_retry(path, || {
                self.http
                    .post(req_url.clone())
                    .header("content-type", "application/json; charset=utf-8")
                    .header("x-requested-with", "XMLHttpRequest")
                    .body(payload.clone())
            })
            .await?;
        let status = res.status();
        tracing::debug!(method = "POST", path, %status, "barezpakhsh json");
        let text = res.text().await.map_err(Self::unreachable)?;
        if status.as_u16() == 403 && parse::is_blocked_page(&text) {
            return Err(BridgeError::SupplierBlocked);
        }
        Self::check_status(path, status)?;
        Ok(text)
    }

    async fn do_login(&self) -> Result<()> {
        let (_, page) = self.get("/Home/Login").await?;
        let token = parse::antiforgery_token(&page, "login")?;
        let fields = [
            ("__RequestVerificationToken".to_string(), token),
            ("Username".to_string(), self.creds.username.clone()),
            ("Password".to_string(), self.creds.password.clone()),
        ];
        let (url, body) = self.post_form("/Home/Login", &fields).await?;
        if url.path().starts_with("/Shop") {
            tracing::info!("barezpakhsh login ok");
            *self.logged_in.lock().await = true;
            return Ok(());
        }
        if parse::is_login_page(&body) {
            return Err(BridgeError::SupplierBadCredentials);
        }
        Err(BridgeError::SupplierParse {
            page: "login".into(),
            field: "outcome".into(),
        })
    }

    async fn ensure_session(&self) -> Result<()> {
        if *self.logged_in.lock().await {
            return Ok(());
        }
        self.do_login().await
    }

    /// GET an authenticated page; on the login page, log in once more and retry.
    async fn get_authed(&self, path: &str) -> Result<String> {
        self.ensure_session().await?;
        let (_, body) = self.get(path).await?;
        if !parse::is_login_page(&body) {
            return Ok(body);
        }
        tracing::warn!("barezpakhsh session expired; logging in again");
        *self.logged_in.lock().await = false;
        self.do_login()
            .await
            .map_err(|_| BridgeError::SupplierSessionExpired)?;
        let (_, body) = self.get(path).await?;
        if parse::is_login_page(&body) {
            return Err(BridgeError::SupplierSessionExpired);
        }
        Ok(body)
    }

    /// POST an authenticated form; on the login page, log in once more and resubmit the same
    /// fields (the statement form carries no anti-forgery token, so this is a plain re-post).
    async fn post_form_authed(&self, path: &str, fields: &[(String, String)]) -> Result<String> {
        self.ensure_session().await?;
        let (_, body) = self.post_form(path, fields).await?;
        if !parse::is_login_page(&body) {
            return Ok(body);
        }
        tracing::warn!("barezpakhsh session expired; logging in again");
        *self.logged_in.lock().await = false;
        self.do_login()
            .await
            .map_err(|_| BridgeError::SupplierSessionExpired)?;
        let (_, body) = self.post_form(path, fields).await?;
        if parse::is_login_page(&body) {
            return Err(BridgeError::SupplierSessionExpired);
        }
        Ok(body)
    }

    /// POST authenticated JSON; a session that has expired answers with the login page's HTML
    /// instead of the usual JSON-string body, which `is_login_page` also recognises directly.
    async fn post_json_authed(&self, path: &str, body: &serde_json::Value) -> Result<String> {
        self.ensure_session().await?;
        let text = self.post_json(path, body).await?;
        if !parse::is_login_page(&text) {
            return Ok(text);
        }
        tracing::warn!("barezpakhsh session expired; logging in again");
        *self.logged_in.lock().await = false;
        self.do_login()
            .await
            .map_err(|_| BridgeError::SupplierSessionExpired)?;
        let text = self.post_json(path, body).await?;
        if parse::is_login_page(&text) {
            return Err(BridgeError::SupplierSessionExpired);
        }
        Ok(text)
    }

    /// Replace (or add) a field value.
    fn set_field(fields: &mut Vec<(String, String)>, name: &str, value: &str) {
        fields.retain(|(n, _)| n != name);
        fields.push((name.to_string(), value.to_string()));
    }

    /// The statement form asking for «معین فاکتور» from `from` (every invoice when `None`) to
    /// date. The site filters server-side on this POST; the paging endpoint's own `fromDate` is
    /// ignored on its own, so both carry the same value.
    fn ledger_form(mut fields: Vec<(String, String)>, from: Option<&str>) -> Vec<(String, String)> {
        Self::set_field(&mut fields, "FromDate", from.unwrap_or(LEDGER_FROM));
        Self::set_field(&mut fields, "ToDate", "");
        Self::set_field(&mut fields, "command", "moein2");
        fields
    }

    /// One page of `/Shop/Share/GradualMoein2Load` for the statement posted by [`Self::ledger_form`].
    fn ledger_page(
        ctx: &parse::LedgerContext,
        index: u32,
        from: Option<&str>,
    ) -> serde_json::Value {
        json!({
            "pageIndex": index,
            "yearId": ctx.year_id,
            "businessCode": ctx.business_code,
            "fromDate": from.unwrap_or(LEDGER_FROM),
            "toDate": ""
        })
    }

    async fn buy_session(&self) -> Result<BuySession> {
        if let Some(s) = self.buy.lock().await.clone() {
            return Ok(s);
        }
        let page = self.get_authed("/Shop/Buy/Index").await?;
        let mut fields = parse::form_fields(&page, "/Shop/Buy/Index", "buy")?;
        let pick = |fields: &[(String, String)], k: &str| {
            fields
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let car = pick(&fields, "SelectedCarCode");
        let brand = pick(&fields, "SelectedBrandCode");
        let group = pick(&fields, "SelectedGroupCode");
        Self::set_field(&mut fields, "SearchTerm", "-");
        Self::set_field(&mut fields, "JustAvailable", "false");
        Self::set_field(&mut fields, "SelectedPageSize", "50");
        Self::set_field(&mut fields, "command", "searchItem");
        let html = self.post_form_authed("/Shop/Buy/Index", &fields).await?;
        let ctx = parse::buy_context(&html)?;
        let session = BuySession {
            ctx,
            car,
            brand,
            group,
        };
        *self.buy.lock().await = Some(session.clone());
        Ok(session)
    }
}

#[async_trait::async_trait]
impl Supplier for Client {
    fn id(&self) -> &'static str {
        ID
    }

    async fn login(&self) -> Result<()> {
        *self.logged_in.lock().await = false;
        self.do_login().await
    }

    async fn ledger(&self, from: Option<&str>, progress: Progress) -> Result<Vec<InvoiceHeader>> {
        let page = self.get_authed("/Shop/Report/Bill").await?;
        let fields = parse::form_fields(&page, "/Shop/Report/Bill", "bill")?;
        let fields = Self::ledger_form(fields, from);
        let html = self.post_form_authed("/Shop/Report/Bill", &fields).await?;
        let ctx = parse::ledger_context(&html)?;
        let pages = ctx.page_count();
        let mut headers = Vec::new();
        for index in 1..=pages {
            let body = self
                .post_json_authed(
                    "/Shop/Share/GradualMoein2Load",
                    &Self::ledger_page(&ctx, index, from),
                )
                .await?;
            headers.extend(
                parse::ledger_rows(&body)?
                    .iter()
                    .filter_map(parse::header_of),
            );
            progress(index, pages);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(dedup_headers(headers))
    }

    async fn invoice_lines(&self, url: &str) -> Result<Vec<InvoiceLine>> {
        let html = self.get_authed(url).await?;
        parse::invoice_lines(&html)
    }

    async fn price_of(&self, code: &str) -> Result<Option<PriceQuote>> {
        let s = self.buy_session().await?;
        find_across_pages(code, SEARCH_PAGE_SIZE, |index| {
            let s = s.clone();
            async move {
                if index > 1 {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                let body = self
                    .post_json_authed(
                        "/Shop/Share/ItemsSearchGradual",
                        &json!({
                            "selectedGroupCode": s.group,
                            "selectedCarCode": s.car,
                            "selectedBrandCode": s.brand,
                            "SpecialItemsId": "",
                            "searchTerm": code,
                            "businessId": s.ctx.business_id,
                            "pageIndex": index,
                            "userGroupId": s.ctx.user_group_id,
                            "justAvailable": false,
                            "pageSize": SEARCH_PAGE_SIZE.to_string()
                        }),
                    )
                    .await?;
                parse::price_quotes(&body)
            }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::InvoiceKind;

    fn header(number: &str, kind: InvoiceKind, date: &str) -> InvoiceHeader {
        InvoiceHeader {
            number: number.into(),
            kind,
            date_jalali: date.into(),
            date_iso: String::new(),
            paid: true,
            url: String::new(),
        }
    }

    #[test]
    fn a_non_2xx_final_status_is_unreachable_and_a_2xx_passes() {
        use reqwest::StatusCode;
        assert!(Client::check_status("/x", StatusCode::OK).is_ok());
        assert!(Client::check_status("/x", StatusCode::NO_CONTENT).is_ok());
        for code in [
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::BAD_GATEWAY,
        ] {
            match Client::check_status("/Shop/Report/Bill", code) {
                Err(BridgeError::SupplierUnreachable(detail)) => {
                    assert!(detail.contains("/Shop/Report/Bill"), "{detail}");
                    assert!(detail.contains(&code.as_u16().to_string()), "{detail}");
                }
                other => panic!("{code}: {other:?}"),
            }
        }
    }

    #[test]
    fn retry_policy_covers_transport_errors_and_5xx_but_not_403_or_success() {
        assert!(!retryable(Some(200)));
        assert!(!retryable(Some(302)));
        assert!(!retryable(Some(403)));
        assert!(retryable(Some(404)));
        assert!(retryable(Some(429)));
        assert!(retryable(Some(500)));
        assert!(retryable(None));
    }

    fn field<'a>(fields: &'a [(String, String)], name: &str) -> Option<&'a str> {
        fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn ctx() -> parse::LedgerContext {
        parse::LedgerContext {
            total: 30,
            page_size: 15,
            year_id: 0,
            business_code: "97100251".into(),
        }
    }

    #[test]
    fn ledger_requests_carry_the_start_date() {
        let page_fields = vec![
            ("BusinessId".to_string(), "1".to_string()),
            ("FromDate".to_string(), "1405/4/24".to_string()),
        ];
        let fields = Client::ledger_form(page_fields, Some("1405/01/01"));
        assert_eq!(field(&fields, "BusinessId"), Some("1"));
        assert_eq!(field(&fields, "FromDate"), Some("1405/01/01"));
        assert_eq!(field(&fields, "ToDate"), Some(""));
        assert_eq!(field(&fields, "command"), Some("moein2"));
        let body = Client::ledger_page(&ctx(), 2, Some("1405/01/01"));
        assert_eq!(body["fromDate"], "1405/01/01");
        assert_eq!(body["toDate"], "");
        assert_eq!(body["pageIndex"], 2);
        assert_eq!(body["yearId"], 0);
        assert_eq!(body["businessCode"], "97100251");
    }

    #[test]
    fn ledger_requests_ask_for_every_invoice_when_unbounded() {
        let fields = Client::ledger_form(vec![], None);
        assert_eq!(field(&fields, "FromDate"), Some(LEDGER_FROM));
        let body = Client::ledger_page(&ctx(), 1, None);
        assert_eq!(body["fromDate"], LEDGER_FROM);
    }

    #[test]
    fn dedup_headers_keeps_the_first_of_each_number_and_kind() {
        // A same-numbered RETURN sits between two duplicate SALE rows (the ledger printed the
        // same sale twice across a page boundary).
        let headers = vec![
            header("100", InvoiceKind::Sale, "1401/01/01"),
            header("100", InvoiceKind::Return, "1401/01/02"),
            header("100", InvoiceKind::Sale, "1401/01/03"),
        ];
        let result = dedup_headers(headers);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].kind, InvoiceKind::Sale);
        assert_eq!(result[0].date_jalali, "1401/01/01");
        assert_eq!(result[1].kind, InvoiceKind::Return);
        assert_eq!(result[1].date_jalali, "1401/01/02");
    }

    fn quote(code: &str) -> PriceQuote {
        PriceQuote {
            code: code.into(),
            name: String::new(),
            list_price: 1,
            vat_percent: None,
            has_vat: false,
            available: true,
            unit: String::new(),
        }
    }

    /// Fifty quotes whose codes all contain `stem`, none equal to it.
    fn full_page(stem: &str) -> Vec<PriceQuote> {
        (0..50).map(|i| quote(&format!("{stem}{i:02}"))).collect()
    }

    /// Verified live on 2026-09-15: the site's search is a substring match that does not rank the
    /// exact code first — «50053» sat on page 2 behind fifty longer codes containing it.
    #[tokio::test]
    async fn price_search_walks_past_the_first_page_for_a_code_hidden_among_longer_ones() {
        let mut second = full_page("50053");
        second[7] = quote("50053");
        let pages = [full_page("50053"), second, vec![quote("never-asked")]];
        let asked = std::sync::Mutex::new(Vec::new());
        let found = find_across_pages("50053", 50, |index| {
            asked.lock().unwrap().push(index);
            let page = pages.get(index as usize - 1).cloned().unwrap_or_default();
            async move { Ok(page) }
        })
        .await
        .unwrap();
        assert_eq!(found.map(|q| q.code), Some("50053".to_string()));
        assert_eq!(*asked.lock().unwrap(), vec![1, 2]);
    }

    #[tokio::test]
    async fn price_search_ends_on_a_short_page_without_a_match() {
        let pages = [full_page("88002"), vec![quote("8800283x"), quote("8800283y")]];
        let asked = std::sync::Mutex::new(Vec::new());
        let found = find_across_pages("8800283", 50, |index| {
            asked.lock().unwrap().push(index);
            let page = pages.get(index as usize - 1).cloned().unwrap_or_default();
            async move { Ok(page) }
        })
        .await
        .unwrap();
        assert_eq!(found, None);
        assert_eq!(*asked.lock().unwrap(), vec![1, 2]);
    }

    #[tokio::test]
    async fn price_search_is_none_after_an_empty_first_page() {
        let asked = std::sync::Mutex::new(Vec::new());
        let found = find_across_pages("00000000-does-not-exist", 50, |index| {
            asked.lock().unwrap().push(index);
            async move { Ok(Vec::new()) }
        })
        .await
        .unwrap();
        assert_eq!(found, None);
        assert_eq!(*asked.lock().unwrap(), vec![1]);
    }

    #[tokio::test]
    async fn price_search_gives_up_after_the_page_cap() {
        let asked = std::sync::Mutex::new(Vec::new());
        let found = find_across_pages("36", 50, |index| {
            asked.lock().unwrap().push(index);
            async move { Ok(full_page("36")) }
        })
        .await
        .unwrap();
        assert_eq!(found, None);
        assert_eq!(asked.lock().unwrap().len(), MAX_SEARCH_PAGES as usize);
    }
}
