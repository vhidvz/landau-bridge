//! The Barezpakhsh client (spec §4): anti-forgery login, ledger paging, invoice pages, price
//! search. Transient failures are retried (spec §3) and an expired session triggers one
//! automatic re-login on every authenticated request. The ledger and the search run on one
//! primary session; invoice pages, which are independent, are read in parallel on a pool of
//! sessions sized by [`crate::supplier::adaptive`]. Every request logs method, path and status;
//! never the credentials.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use reqwest::{Client as Http, Url};
use serde_json::json;
use tokio::sync::Mutex;

use super::parse::{self, BuyContext};
use crate::domain::{InvoiceHeader, InvoiceLine, PriceQuote};
use crate::errors::{BridgeError, Result};
use crate::matching::normalize::search_words;
use crate::supplier::adaptive::{Limiter, Pool, Sample};
use crate::supplier::{Credentials, Progress, Supplier};

pub const ID: &str = "barezpakhsh";
pub const BASE_URL: &str = "https://33314466.ir";
/// Earliest date the statement form is asked for when no bound is given — "every invoice to date".
const LEDGER_FROM: &str = "1390/01/01";
const USER_AGENT: &str = "LandauBridge/0.1 (+https://landau.ir)";

/// Pauses between attempts at one request — six attempts over ~1¾ minutes, so a dropped
/// connection or a site that stumbles for a minute is ridden out rather than reported; see
/// [`Transient`] for what qualifies. A job that fetches for hours meets both (2026-10-07: six
/// transport retries and four error pages in one 1 499-invoice run).
const RETRY_DELAYS: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(3),
    Duration::from_secs(10),
    Duration::from_secs(30),
    Duration::from_secs(60),
];
/// The budget before LND-Q01 — three attempts over four seconds — for a check the seller sits
/// waiting on (Settings → «آزمایش ورود»), where a dead connection must answer in seconds.
const QUICK_RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(3)];

/// Sessions the invoice-page pool starts with and may grow to. Measured 2026-10-07 on the live
/// site: four sessions answered four pages in 31 s — no slower than one — and eight answered
/// eight in 39–42 s, the server starting to strain; the limiter finds the point in between, and
/// the ceiling keeps one seller's import from leaning harder than that on the supplier.
const INITIAL_SESSIONS: usize = 2;
const MAX_SESSIONS: usize = 8;

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

/// The site's way of saying a page threw on its side: ASP.NET's custom-error redirect to
/// `/Error?aspxerrorpath=…`, which reqwest follows to an HTTP **200** «بروز خطا» page, or that page
/// answered in place of the content. Verified live 2026-10-07 — an invalid invoice id redirects
/// there — and it is what reached the invoice parser as «lines table» missing in 4 of 1 499
/// fetches that day, each of which parsed when fetched again.
pub(crate) fn site_error(url: &Url, body: &str) -> bool {
    url.path().eq_ignore_ascii_case("/Error") || parse::is_error_page(body)
}

/// Why one attempt did not yield a page — every one of them worth another attempt.
#[derive(Debug)]
enum Transient {
    /// No response, or the body was cut off on the way: the network, not the site.
    Network(String),
    /// A status [`retryable`] accepts.
    Status(reqwest::StatusCode),
    /// [`site_error`].
    SiteError,
}

impl Transient {
    /// What the caller sees once the retries are spent.
    fn into_error(self, path: &str) -> BridgeError {
        match self {
            Transient::Network(detail) => BridgeError::SupplierUnreachable(detail),
            Transient::Status(status) => {
                BridgeError::SupplierUnreachable(format!("{path} answered {status}"))
            }
            Transient::SiteError => BridgeError::SupplierFaulted { path: path.into() },
        }
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

/// Every row of a search, page after page, until a page comes back short of `page_size` or
/// [`MAX_SEARCH_PAGES`] is reached (spec §14 b).
pub(crate) async fn collect_pages<F, Fut>(page_size: usize, fetch: F) -> Result<Vec<PriceQuote>>
where
    F: Fn(u32) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<PriceQuote>>>,
{
    let mut all = Vec::new();
    for index in 1..=MAX_SEARCH_PAGES {
        let page = fetch(index).await?;
        let rows = page.len();
        all.extend(page);
        if rows < page_size {
            return Ok(all);
        }
    }
    tracing::warn!(
        pages = MAX_SEARCH_PAGES,
        "barezpakhsh search stopped at the page cap"
    );
    Ok(all)
}

/// Defaults the price search needs, learned once per session from the «ثبت فاکتور» page.
#[derive(Debug, Clone)]
struct BuySession {
    ctx: BuyContext,
    car: String,
    brand: String,
    group: String,
}

/// One conversation with the site, with a cookie jar of its own. The site serves one request per
/// session at a time: four sent at once on one session came back at 32, 60 and 89 s and then a
/// 504 at the gateway's 100 s, while four sessions answered four pages in 31 s (measured
/// 2026-10-07). So a session never carries two requests at once — the primary one is used in
/// sequence, a pooled one only under its lease.
struct Session {
    http: Http,
    logged_in: Mutex<bool>,
    /// How its requests went since the last [`Session::take_sample`].
    signal: std::sync::Mutex<Signal>,
}

#[derive(Debug, Default)]
struct Signal {
    strained: bool,
    rtt: Option<Duration>,
}

impl Session {
    fn open() -> Result<Self> {
        let http = Http::builder()
            .cookie_store(true)
            .no_proxy()
            .gzip(true)
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(90))
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| BridgeError::SupplierUnreachable(e.to_string()))?;
        Ok(Session {
            http,
            logged_in: Mutex::new(false),
            signal: Default::default(),
        })
    }

    /// For the pool's limiter: any retry is strain, else the latency of the final request;
    /// nothing when no request was answered (a refusal says nothing about the connection).
    fn take_sample(&self) -> Option<Sample> {
        let signal = std::mem::take(&mut *self.signal.lock().unwrap());
        if signal.strained {
            Some(Sample::Strained)
        } else {
            signal.rtt.map(Sample::Ok)
        }
    }
}

pub struct Client {
    base: Url,
    creds: Credentials,
    /// The ledger and the price search: flows that keep state in the site's session.
    primary: Session,
    /// Invoice pages, one per session at a time.
    pages: Pool<Session>,
    buy: Mutex<Option<BuySession>>,
    /// [`RETRY_DELAYS`]; tests shorten it.
    retry_delays: Vec<Duration>,
}

impl Client {
    pub fn new(creds: Credentials) -> Result<Self> {
        Self::with_base(creds, BASE_URL)
    }

    pub fn with_base(creds: Credentials, base: &str) -> Result<Self> {
        let base = Url::parse(base).map_err(|e| BridgeError::Other(format!("base url: {e}")))?;
        Ok(Client {
            base,
            creds,
            primary: Session::open()?,
            pages: Pool::new(
                "barezpakhsh pages",
                Limiter::new(INITIAL_SESSIONS, MAX_SESSIONS),
            ),
            buy: Mutex::new(None),
            retry_delays: RETRY_DELAYS.to_vec(),
        })
    }

    /// The same client with [`QUICK_RETRY_DELAYS`]: for a check the seller is waiting on.
    pub fn impatient(mut self) -> Self {
        self.retry_delays = QUICK_RETRY_DELAYS.to_vec();
        self
    }

    fn url(&self, path: &str) -> Result<Url> {
        self.base
            .join(path)
            .map_err(|e| BridgeError::Other(format!("url {path}: {e}")))
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

    /// Sends a request built fresh on each attempt (so a form/JSON body can be resent) and reads
    /// its body, retrying with [`RETRY_DELAYS`] whatever [`Transient`] names — for every kind of
    /// request alike, since the site's failures do not care which one it was serving. Returns the
    /// final URL and body of the first answer that is neither; session-expiry detection and parsing
    /// stay with the caller. Spent retries end as [`Transient::into_error`] says: a network or
    /// status failure is [`crate::jobs::fatal`] (the site is out of reach), a site error is not.
    async fn fetch<B>(
        &self,
        s: &Session,
        method: &str,
        path: &str,
        build: B,
    ) -> Result<(Url, String)>
    where
        B: Fn(&Http) -> reqwest::RequestBuilder,
    {
        let mut attempt: usize = 0;
        loop {
            attempt += 1;
            let started = Instant::now();
            let failure = match build(&s.http).send().await {
                Err(e) => Transient::Network(e.without_url().to_string()),
                Ok(res) if retryable(Some(res.status().as_u16())) => {
                    Transient::Status(res.status())
                }
                Ok(res) => {
                    let status = res.status();
                    let url = res.url().clone();
                    match res.text().await {
                        Err(e) => Transient::Network(e.without_url().to_string()),
                        Ok(body) => {
                            tracing::debug!(method, path, %status, "barezpakhsh");
                            if status.as_u16() == 403 && parse::is_blocked_page(&body) {
                                return Err(BridgeError::SupplierBlocked);
                            }
                            if !site_error(&url, &body) {
                                Self::check_status(path, status)?;
                                s.signal.lock().unwrap().rtt = Some(started.elapsed());
                                return Ok((url, body));
                            }
                            Transient::SiteError
                        }
                    }
                }
            };
            s.signal.lock().unwrap().strained = true;
            let Some(delay) = self.retry_delays.get(attempt - 1) else {
                return Err(failure.into_error(path));
            };
            tracing::warn!(method, path, attempt, reason = ?failure, "barezpakhsh retrying");
            tokio::time::sleep(*delay).await;
        }
    }

    async fn get(&self, s: &Session, path: &str) -> Result<(Url, String)> {
        let req_url = self.url(path)?;
        self.fetch(s, "GET", path, |http| http.get(req_url.clone()))
            .await
    }

    async fn post_form(
        &self,
        s: &Session,
        path: &str,
        fields: &[(String, String)],
    ) -> Result<(Url, String)> {
        let req_url = self.url(path)?;
        self.fetch(s, "POST", path, |http| {
            http.post(req_url.clone()).form(fields)
        })
        .await
    }

    /// POST JSON. The site's «خطا» page (HTTP 200, HTML) comes back transiently to any request and
    /// always to an over-long search term (spec §14 b); [`Self::fetch`] retries it and then fails
    /// the one item as [`BridgeError::SupplierFaulted`], never the job.
    async fn post_json(&self, s: &Session, path: &str, body: &serde_json::Value) -> Result<String> {
        let req_url = self.url(path)?;
        let payload = body.to_string();
        let (_, text) = self
            .fetch(s, "POST", path, |http| {
                http.post(req_url.clone())
                    .header("content-type", "application/json; charset=utf-8")
                    .header("x-requested-with", "XMLHttpRequest")
                    .body(payload.clone())
            })
            .await?;
        Ok(text)
    }

    async fn do_login(&self, s: &Session) -> Result<()> {
        let (_, page) = self.get(s, "/Home/Login").await?;
        let token = parse::antiforgery_token(&page, "login")?;
        let fields = [
            ("__RequestVerificationToken".to_string(), token),
            ("Username".to_string(), self.creds.username.clone()),
            ("Password".to_string(), self.creds.password.clone()),
        ];
        let (url, body) = self.post_form(s, "/Home/Login", &fields).await?;
        if url.path().starts_with("/Shop") {
            tracing::info!("barezpakhsh login ok");
            *s.logged_in.lock().await = true;
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

    async fn ensure_session(&self, s: &Session) -> Result<()> {
        if *s.logged_in.lock().await {
            return Ok(());
        }
        self.do_login(s).await
    }

    /// GET an authenticated page; on the login page, log in once more and retry.
    async fn get_authed(&self, s: &Session, path: &str) -> Result<String> {
        self.ensure_session(s).await?;
        let (_, body) = self.get(s, path).await?;
        if !parse::is_login_page(&body) {
            return Ok(body);
        }
        tracing::warn!("barezpakhsh session expired; logging in again");
        *s.logged_in.lock().await = false;
        self.do_login(s)
            .await
            .map_err(|_| BridgeError::SupplierSessionExpired)?;
        let (_, body) = self.get(s, path).await?;
        if parse::is_login_page(&body) {
            return Err(BridgeError::SupplierSessionExpired);
        }
        Ok(body)
    }

    /// POST an authenticated form; on the login page, log in once more and resubmit the same
    /// fields (the statement form carries no anti-forgery token, so this is a plain re-post).
    async fn post_form_authed(
        &self,
        s: &Session,
        path: &str,
        fields: &[(String, String)],
    ) -> Result<String> {
        self.ensure_session(s).await?;
        let (_, body) = self.post_form(s, path, fields).await?;
        if !parse::is_login_page(&body) {
            return Ok(body);
        }
        tracing::warn!("barezpakhsh session expired; logging in again");
        *s.logged_in.lock().await = false;
        self.do_login(s)
            .await
            .map_err(|_| BridgeError::SupplierSessionExpired)?;
        let (_, body) = self.post_form(s, path, fields).await?;
        if parse::is_login_page(&body) {
            return Err(BridgeError::SupplierSessionExpired);
        }
        Ok(body)
    }

    /// POST authenticated JSON; a session that has expired answers with the login page's HTML
    /// instead of the usual JSON-string body, which `is_login_page` also recognises directly.
    async fn post_json_authed(
        &self,
        s: &Session,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<String> {
        self.ensure_session(s).await?;
        let text = self.post_json(s, path, body).await?;
        if !parse::is_login_page(&text) {
            return Ok(text);
        }
        tracing::warn!("barezpakhsh session expired; logging in again");
        *s.logged_in.lock().await = false;
        self.do_login(s)
            .await
            .map_err(|_| BridgeError::SupplierSessionExpired)?;
        let text = self.post_json(s, path, body).await?;
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
        let page = self.get_authed(&self.primary, "/Shop/Buy/Index").await?;
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
        let html = self
            .post_form_authed(&self.primary, "/Shop/Buy/Index", &fields)
            .await?;
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

impl Client {
    /// One page of `/Shop/Share/ItemsSearchGradual` for `term`, 100 ms after the previous one.
    async fn search_page(&self, s: &BuySession, term: &str, index: u32) -> Result<Vec<PriceQuote>> {
        if index > 1 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let body = self
            .post_json_authed(
                &self.primary,
                "/Shop/Share/ItemsSearchGradual",
                &json!({
                    "selectedGroupCode": s.group,
                    "selectedCarCode": s.car,
                    "selectedBrandCode": s.brand,
                    "SpecialItemsId": "",
                    "searchTerm": term,
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
}

#[async_trait::async_trait]
impl Supplier for Client {
    fn id(&self) -> &'static str {
        ID
    }

    async fn login(&self) -> Result<()> {
        *self.primary.logged_in.lock().await = false;
        self.do_login(&self.primary).await
    }

    async fn ledger(&self, from: Option<&str>, progress: Progress) -> Result<Vec<InvoiceHeader>> {
        let page = self.get_authed(&self.primary, "/Shop/Report/Bill").await?;
        let fields = parse::form_fields(&page, "/Shop/Report/Bill", "bill")?;
        let fields = Self::ledger_form(fields, from);
        let html = self
            .post_form_authed(&self.primary, "/Shop/Report/Bill", &fields)
            .await?;
        let ctx = parse::ledger_context(&html)?;
        let pages = ctx.page_count();
        let mut headers = Vec::new();
        for index in 1..=pages {
            let body = self
                .post_json_authed(
                    &self.primary,
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

    /// On a pooled session of its own: invoice pages are independent, and the site serves one
    /// request per session at a time.
    async fn invoice_lines(&self, url: &str) -> Result<Vec<InvoiceLine>> {
        let lease = self.pages.lease(Session::open).await?;
        let html = self.get_authed(lease.get(), url).await;
        let sample = lease.get().take_sample();
        lease.finish(sample);
        parse::invoice_lines(&html?)
    }

    fn parallelism(&self) -> usize {
        MAX_SESSIONS
    }

    async fn price_of(&self, code: &str) -> Result<Option<PriceQuote>> {
        let s = self.buy_session().await?;
        find_across_pages(code, SEARCH_PAGE_SIZE, |index| {
            self.search_page(&s, code, index)
        })
        .await
    }

    async fn quotes_by_name(&self, name: &str) -> Result<Vec<PriceQuote>> {
        let term = search_words(name);
        if term.is_empty() {
            return Ok(Vec::new());
        }
        let s = self.buy_session().await?;
        collect_pages(SEARCH_PAGE_SIZE, |index| self.search_page(&s, &term, index)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::InvoiceKind;
    use futures_util::StreamExt;

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
        let pages = [
            full_page("88002"),
            vec![quote("8800283x"), quote("8800283y")],
        ];
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

    #[tokio::test]
    async fn search_pages_collects_every_page_until_a_short_one() {
        let pages = [
            full_page("سیبک"),
            vec![quote("سیبک فرمان چپ"), quote("سیبک فرمان راست")],
        ];
        let asked = std::sync::Mutex::new(Vec::new());
        let all = collect_pages(50, |index| {
            asked.lock().unwrap().push(index);
            let page = pages.get(index as usize - 1).cloned().unwrap_or_default();
            async move { Ok(page) }
        })
        .await
        .unwrap();
        assert_eq!(all.len(), 52);
        assert_eq!(*asked.lock().unwrap(), vec![1, 2]);
    }

    #[tokio::test]
    async fn search_pages_stops_at_the_page_cap() {
        let all = collect_pages(50, |_| async { Ok(full_page("36")) })
            .await
            .unwrap();
        assert_eq!(all.len(), 50 * MAX_SEARCH_PAGES as usize);
    }

    // ------------------------------------------------------------ transient answers, end to end

    const FACTOR: &str = include_str!("../../../fixtures/barezpakhsh/factor-8818975.html");
    const ERROR_PAGE: &str = include_str!("../../../fixtures/barezpakhsh/error-page.html");
    const FACTOR_PATH: &str = "/Shop/Preview/Factor/8818975/11000";

    type Seen = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

    /// A loopback HTTP/1.1 server answering connection N with `script[N]` and closing it, so every
    /// attempt — redirects included — is one scripted answer. Records each request line.
    fn serve(script: Vec<String>) -> (String, Seen) {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen: Seen = Default::default();
        let log = seen.clone();
        std::thread::spawn(move || {
            for raw in script {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                log.lock().unwrap().push(line.trim_end().to_string());
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap() == 0 || header == "\r\n" {
                        break;
                    }
                }
                let _ = stream.write_all(raw.as_bytes());
            }
        });
        (base, seen)
    }

    fn ok(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    /// What the site answers when a page throws on its side (verified live 2026-10-07 on
    /// `/Shop/Preview/Factor/1/11750`): ASP.NET's custom-error redirect, then a 200 error page.
    fn error_redirect(path: &str) -> Vec<String> {
        vec![
            format!("HTTP/1.1 302 Found\r\nLocation: /Error?aspxerrorpath={path}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
            ok(ERROR_PAGE),
        ]
    }

    /// Headers promising more body than is sent before the connection drops.
    fn cut_off() -> String {
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 100000\r\nConnection: close\r\n\r\n<!DOCTYPE html><html><body><table>".into()
    }

    /// Logged in, with every retry pause cut to zero (the attempt count is unchanged).
    async fn client_at(base: &str) -> Client {
        let mut c = Client::with_base(
            Credentials {
                username: "u".into(),
                password: "p".into(),
            },
            base,
        )
        .unwrap();
        c.retry_delays = vec![Duration::ZERO; RETRY_DELAYS.len()];
        *c.primary.logged_in.lock().await = true;
        let mut page_session = Session::open().unwrap();
        *page_session.logged_in.get_mut() = true;
        c.pages.seed(page_session);
        c
    }

    const ATTEMPTS: usize = RETRY_DELAYS.len() + 1;

    #[tokio::test]
    async fn a_site_that_keeps_failing_is_reported_as_its_fault_not_as_a_layout_change() {
        let (base, seen) = serve(
            (0..ATTEMPTS)
                .flat_map(|_| error_redirect(FACTOR_PATH))
                .collect(),
        );
        let err = client_at(&base)
            .await
            .invoice_lines(FACTOR_PATH)
            .await
            .unwrap_err();
        assert_eq!(
            err,
            BridgeError::SupplierFaulted {
                path: FACTOR_PATH.into()
            }
        );
        assert!(!crate::jobs::fatal(&err), "one invoice, never the job");
        assert_eq!(seen.lock().unwrap().len(), 2 * ATTEMPTS);
    }

    #[tokio::test]
    async fn a_network_that_stays_down_still_stops_the_job_after_every_attempt() {
        let (base, seen) = serve((0..ATTEMPTS).map(|_| cut_off()).collect());
        let err = client_at(&base)
            .await
            .invoice_lines(FACTOR_PATH)
            .await
            .unwrap_err();
        assert!(
            matches!(err, BridgeError::SupplierUnreachable(_)),
            "{err:?}"
        );
        assert!(crate::jobs::fatal(&err));
        assert_eq!(seen.lock().unwrap().len(), ATTEMPTS);
    }

    /// The JSON endpoints meet the same error page (spec §14 b); it used to surface as a «layout
    /// change» of `items-search` once its retries ran out.
    #[tokio::test]
    async fn a_search_that_keeps_meeting_the_error_page_is_its_fault_too() {
        let (base, _) = serve((0..ATTEMPTS).map(|_| ok(ERROR_PAGE)).collect());
        let c = client_at(&base).await;
        let err = c
            .post_json_authed(&c.primary, "/Shop/Share/ItemsSearchGradual", &json!({}))
            .await
            .unwrap_err();
        assert_eq!(
            err,
            BridgeError::SupplierFaulted {
                path: "/Shop/Share/ItemsSearchGradual".into()
            }
        );
    }

    /// The 2026-10-07 report: four invoices out of 1 499 failed as «ساختار صفحه factor تغییر کرده
    /// است» while each parsed fine when fetched again — the site's error page, reached through its
    /// redirect with HTTP 200, had been handed to the parser.
    #[tokio::test]
    async fn an_error_redirect_on_an_invoice_page_is_retried_not_read_as_a_layout_change() {
        let mut script = error_redirect(FACTOR_PATH);
        script.push(ok(FACTOR));
        let (base, seen) = serve(script);
        let lines = client_at(&base)
            .await
            .invoice_lines(FACTOR_PATH)
            .await
            .unwrap();
        assert_eq!(lines.len(), 36);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 3, "{seen:?}");
        assert!(seen[1].starts_with("GET /Error?"), "{seen:?}");
    }

    #[tokio::test]
    async fn the_error_page_answered_directly_is_retried() {
        let (base, _) = serve(vec![ok(ERROR_PAGE), ok(FACTOR)]);
        let lines = client_at(&base)
            .await
            .invoice_lines(FACTOR_PATH)
            .await
            .unwrap();
        assert_eq!(lines.len(), 36);
    }

    /// A connection that drops while the body is still arriving is a network failure like any
    /// other, not the end of the job.
    #[tokio::test]
    async fn a_body_cut_off_mid_download_is_retried() {
        let (base, seen) = serve(vec![cut_off(), ok(FACTOR)]);
        let lines = client_at(&base)
            .await
            .invoice_lines(FACTOR_PATH)
            .await
            .unwrap();
        assert_eq!(lines.len(), 36);
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    // ------------------------------------------------------------ parallel invoice pages

    const LOGIN: &str = include_str!("../../../fixtures/barezpakhsh/login.html");

    #[derive(Default)]
    struct SiteStats {
        logins: usize,
        in_flight: usize,
        max_in_flight: usize,
        /// Requests running per session cookie right now.
        per_session: std::collections::HashMap<String, usize>,
        /// Times a session was given a request while another of its own was running — what the
        /// real site answers with a queue and, past 100 s, a 504.
        overlaps: usize,
        /// ∫ in_flight dt over the run, for the time-weighted mean, and when it last moved.
        area_ms: f64,
        since: Option<Instant>,
        moved: Option<Instant>,
    }

    impl SiteStats {
        /// Account for the time since the last move, then apply `delta` pages in flight.
        fn shift(&mut self, delta: isize) {
            let now = Instant::now();
            if let Some(moved) = self.moved {
                self.area_ms += self.in_flight as f64 * (now - moved).as_secs_f64() * 1000.0;
            }
            self.since.get_or_insert(now);
            self.moved = Some(now);
            self.in_flight = self.in_flight.checked_add_signed(delta).unwrap();
            self.max_in_flight = self.max_in_flight.max(self.in_flight);
        }

        fn mean_in_flight(&self) -> f64 {
            let span = (self.moved.unwrap() - self.since.unwrap()).as_secs_f64() * 1000.0;
            self.area_ms / span
        }
    }

    type Stats = std::sync::Arc<std::sync::Mutex<SiteStats>>;

    /// A stand-in for the site that serves connections concurrently: a login hands out a session
    /// cookie, and an invoice page is served to a logged-in session once `work` is done over a
    /// link `width` pages wide — shared evenly, as a link shares its bandwidth, so beyond `width`
    /// pages at once every one of them slows and the sum does not grow.
    fn fake_site(work: Duration, width: usize) -> (String, Stats) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let stats: Stats = Default::default();
        let st = stats.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let st = st.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let (mut len, mut sid) = (0usize, None::<String>);
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).unwrap() == 0 || header == "\r\n" {
                            break;
                        }
                        let lower = header.to_ascii_lowercase();
                        if let Some(v) = lower.strip_prefix("content-length:") {
                            len = v.trim().parse().unwrap();
                        }
                        if lower.starts_with("cookie:") {
                            sid = header
                                .split("sid=")
                                .nth(1)
                                .map(|v| v.split(';').next().unwrap().trim().to_string());
                        }
                    }
                    let mut body = vec![0; len];
                    reader.read_exact(&mut body).unwrap();
                    let mut parts = line.split_whitespace();
                    let (method, path) = (parts.next().unwrap(), parts.next().unwrap());
                    let reply = match (method, path, sid) {
                        ("GET", "/Home/Login", _) => ok(LOGIN),
                        ("POST", "/Home/Login", _) => {
                            let n = {
                                let mut st = st.lock().unwrap();
                                st.logins += 1;
                                st.logins
                            };
                            format!("HTTP/1.1 302 Found\r\nLocation: /Shop/Home\r\nSet-Cookie: sid=s{n}; Path=/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        }
                        ("GET", "/Shop/Home", _) => ok("<html>shop</html>"),
                        ("GET", p, Some(sid)) if p.starts_with("/Shop/Preview/Factor/") => {
                            {
                                let mut st = st.lock().unwrap();
                                st.shift(1);
                                let mine = st.per_session.entry(sid.clone()).or_default();
                                *mine += 1;
                                if *mine > 1 {
                                    st.overlaps += 1;
                                }
                            }
                            let mut left = work.as_secs_f64();
                            while left > 0.0 {
                                let active = st.lock().unwrap().in_flight;
                                let share = width as f64 / active.max(width) as f64;
                                std::thread::sleep(Duration::from_millis(5));
                                left -= 0.005 * share;
                            }
                            let mut st = st.lock().unwrap();
                            st.shift(-1);
                            *st.per_session.get_mut(&sid).unwrap() -= 1;
                            ok(FACTOR)
                        }
                        ("GET", _, None) => ok(LOGIN),
                        _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
                    };
                    let _ = stream.write_all(reply.as_bytes());
                });
            }
        });
        (base, stats)
    }

    fn fresh_client(base: &str, limiter: Limiter) -> Client {
        let mut c = Client::with_base(
            Credentials {
                username: "u".into(),
                password: "p".into(),
            },
            base,
        )
        .unwrap();
        c.retry_delays = vec![Duration::ZERO; RETRY_DELAYS.len()];
        c.pages = Pool::new("test", limiter);
        c
    }

    fn factor_urls(n: usize) -> Vec<String> {
        (0..n)
            .map(|i| format!("/Shop/Preview/Factor/{i}/11750"))
            .collect()
    }

    #[tokio::test]
    async fn invoice_pages_run_in_parallel_one_request_per_session() {
        let (base, stats) = fake_site(Duration::from_millis(150), usize::MAX);
        let c = fresh_client(&base, Limiter::new(4, 4));
        let started = Instant::now();
        let urls = factor_urls(16);
        let pages = futures_util::future::join_all(urls.iter().map(|u| c.invoice_lines(u))).await;
        let took = started.elapsed();
        assert!(pages.iter().all(|p| p.as_ref().map(Vec::len) == Ok(36)));
        let st = stats.lock().unwrap();
        assert_eq!(st.overlaps, 0, "a session never carries two requests");
        assert_eq!(st.max_in_flight, 4, "as many at once as the pool allows");
        assert_eq!(
            st.logins, 4,
            "each pooled session logs in once and is reused"
        );
        // One at a time: 16 × 150 ms = 2.4 s; four at a time: ~0.6 s plus the logins.
        assert!(took < Duration::from_millis(1800), "{took:?}");
    }

    /// LND-Q04: the long budget made Settings' «آزمایش ورود» spin for minutes on a dead connection.
    #[tokio::test]
    async fn an_interactive_check_gives_up_after_the_short_budget() {
        let (base, seen) = serve((0..3).map(|_| cut_off()).collect());
        let creds = Credentials {
            username: "u".into(),
            password: "p".into(),
        };
        let c = Client::with_base(creds, &base).unwrap().impatient();
        let err = c.login().await.unwrap_err();
        assert!(
            matches!(err, BridgeError::SupplierUnreachable(_)),
            "{err:?}"
        );
        assert_eq!(seen.lock().unwrap().len(), QUICK_RETRY_DELAYS.len() + 1);
    }

    /// The retry signal reaches the limiter, read as a rate: one page that needed a retry leaves
    /// the pool alone, a third within the window halves it.
    #[tokio::test]
    async fn an_isolated_retry_keeps_the_pool_and_a_burst_halves_it() {
        let (base, _) = serve((0..3).flat_map(|_| [ok(ERROR_PAGE), ok(FACTOR)]).collect());
        let c = client_at(&base).await;
        let before = c.pages.permits();
        c.invoice_lines(FACTOR_PATH).await.unwrap();
        assert_eq!(
            c.pages.permits(),
            before,
            "one retry is the site's background noise"
        );
        c.invoice_lines(FACTOR_PATH).await.unwrap();
        c.invoice_lines(FACTOR_PATH).await.unwrap();
        assert_eq!(c.pages.permits(), (before / 2).max(1));
    }

    /// End to end on a link three pages fill (beyond three, every page slows and the sum stays
    /// put): real HTTP latencies drive the pool, and its sessions stay one request each while it
    /// moves. *Where* it settles — at the link's width, where plain growth would run to eight — is
    /// `adaptive`'s virtual-time simulation to prove: a real-time run this short cannot tell the
    /// two apart reliably (checked 2026-10-07: mean 2.4–3.0 against 3.4–4.3 for plain growth, and
    /// scheduling noise under load moves both).
    #[tokio::test]
    async fn a_saturated_link_drives_the_pool_end_to_end() {
        let (base, stats) = fake_site(Duration::from_millis(200), 3);
        let c = fresh_client(&base, Limiter::new(INITIAL_SESSIONS, MAX_SESSIONS));
        let urls = factor_urls(90);
        let pages: Vec<_> = futures_util::stream::iter(urls.iter().map(|u| c.invoice_lines(u)))
            .buffer_unordered(MAX_SESSIONS)
            .collect()
            .await;
        assert!(pages.iter().all(|p| p.is_ok()));
        let st = stats.lock().unwrap();
        assert_eq!(st.overlaps, 0, "a session never carries two requests");
        assert!(
            st.max_in_flight >= 3,
            "the latency signal reaches the limiter"
        );
        assert!(st.max_in_flight <= MAX_SESSIONS);
        assert!(st.mean_in_flight() <= 4.0, "{}", st.mean_in_flight());
    }
}
