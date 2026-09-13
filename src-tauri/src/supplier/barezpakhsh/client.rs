//! The Barezpakhsh session (spec §4): anti-forgery login, ledger paging, invoice pages, price
//! search. Every request logs method, path and status; never the credentials.

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
/// Earliest date the statement form is asked for — "every invoice to date".
const LEDGER_FROM: &str = "1390/01/01";
const USER_AGENT: &str = "LandauBridge/0.1 (+https://landau.ir)";

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

    async fn get(&self, path: &str) -> Result<(Url, String)> {
        let res = self
            .http
            .get(self.url(path)?)
            .send()
            .await
            .map_err(Self::unreachable)?;
        let status = res.status();
        let url = res.url().clone();
        let body = res.text().await.map_err(Self::unreachable)?;
        tracing::debug!(method = "GET", path, %status, "barezpakhsh");
        if status.as_u16() == 403 && parse::is_blocked_page(&body) {
            return Err(BridgeError::SupplierBlocked);
        }
        Ok((url, body))
    }

    async fn post_form(&self, path: &str, fields: &[(String, String)]) -> Result<(Url, String)> {
        let res = self
            .http
            .post(self.url(path)?)
            .form(fields)
            .send()
            .await
            .map_err(Self::unreachable)?;
        let status = res.status();
        let url = res.url().clone();
        let body = res.text().await.map_err(Self::unreachable)?;
        tracing::debug!(method = "POST", path, %status, "barezpakhsh");
        if status.as_u16() == 403 && parse::is_blocked_page(&body) {
            return Err(BridgeError::SupplierBlocked);
        }
        Ok((url, body))
    }

    async fn post_json(&self, path: &str, body: &serde_json::Value) -> Result<String> {
        let res = self
            .http
            .post(self.url(path)?)
            .header("content-type", "application/json; charset=utf-8")
            .header("x-requested-with", "XMLHttpRequest")
            .body(body.to_string())
            .send()
            .await
            .map_err(Self::unreachable)?;
        let status = res.status();
        tracing::debug!(method = "POST", path, %status, "barezpakhsh json");
        let text = res.text().await.map_err(Self::unreachable)?;
        if !status.is_success() {
            return Err(BridgeError::SupplierParse {
                page: path.into(),
                field: format!("status {status}"),
            });
        }
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

    /// Replace (or add) a field value.
    fn set_field(fields: &mut Vec<(String, String)>, name: &str, value: &str) {
        fields.retain(|(n, _)| n != name);
        fields.push((name.to_string(), value.to_string()));
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
        let (_, html) = self.post_form("/Shop/Buy/Index", &fields).await?;
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

    async fn ledger(&self, progress: Progress) -> Result<Vec<InvoiceHeader>> {
        let page = self.get_authed("/Shop/Report/Bill").await?;
        let mut fields = parse::form_fields(&page, "/Shop/Report/Bill", "bill")?;
        Self::set_field(&mut fields, "FromDate", LEDGER_FROM);
        Self::set_field(&mut fields, "ToDate", "");
        Self::set_field(&mut fields, "command", "moein2");
        let (_, html) = self.post_form("/Shop/Report/Bill", &fields).await?;
        let ctx = parse::ledger_context(&html)?;
        let pages = ctx.page_count();
        let mut headers = Vec::new();
        for index in 1..=pages {
            let body = self
                .post_json(
                    "/Shop/Share/GradualMoein2Load",
                    &json!({
                        "pageIndex": index,
                        "yearId": ctx.year_id,
                        "businessCode": ctx.business_code,
                        "fromDate": LEDGER_FROM,
                        "toDate": ""
                    }),
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
        headers.sort_by(|a, b| {
            a.date_jalali
                .cmp(&b.date_jalali)
                .then(a.number.cmp(&b.number))
        });
        headers.dedup_by(|a, b| a.number == b.number && a.kind == b.kind);
        Ok(headers)
    }

    async fn invoice_lines(&self, url: &str) -> Result<Vec<InvoiceLine>> {
        let html = self.get_authed(url).await?;
        parse::invoice_lines(&html)
    }

    async fn price_of(&self, code: &str) -> Result<Option<PriceQuote>> {
        let s = self.buy_session().await?;
        let body = self
            .post_json(
                "/Shop/Share/ItemsSearchGradual",
                &json!({
                    "selectedGroupCode": s.group,
                    "selectedCarCode": s.car,
                    "selectedBrandCode": s.brand,
                    "SpecialItemsId": "",
                    "searchTerm": code,
                    "businessId": s.ctx.business_id,
                    "pageIndex": 1,
                    "userGroupId": s.ctx.user_group_id,
                    "justAvailable": false,
                    "pageSize": "50"
                }),
            )
            .await?;
        let quotes = parse::price_quotes(&body)?;
        Ok(quotes.into_iter().find(|q| q.code == code))
    }
}
