//! Landau client gateway calls (spec §2.1, §5). Every reply is read through [`peel`] so a
//! `{data: …}` envelope and a flat body are the same thing to the rest of the app.

use std::time::Duration;

use reqwest::{Client as Http, Method, StatusCode, Url};
use serde::Serialize;
use serde_json::{json, Value};

use crate::domain::Authenticity;
use crate::errors::{BridgeError, Result};

const SHELF_PAGE: usize = 100;

/// Up to 3 attempts total, sleeping 1s then 3s between them; see [`retryable`] for what qualifies.
const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(3)];

/// Retry policy for the transport layer: a transport error (`None`) and statuses 408, 429 and any
/// 5xx are retried — the picker is called thousands of times per import run, so a transient
/// backend blip (seen live: a 500 from the registry search) must not fail the whole request.
/// Every other 4xx and every 2xx/3xx is returned as-is; retrying would not change the outcome.
pub(crate) fn retryable(status: Option<u16>) -> bool {
    match status {
        None => true,
        Some(408) | Some(429) => true,
        Some(s) => (500..600).contains(&s),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Store {
    pub id: String,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub state: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Membership {
    pub business: String,
    pub name: Option<String>,
    pub principal: String,
    pub approved: bool,
    pub stores: Vec<Store>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShelfRow {
    pub id: String,
    pub product: String,
    pub product_name: Option<String>,
    pub store: Option<String>,
    pub inventory: i64,
    pub price: Option<i64>,
    pub cost: Option<i64>,
    pub condition: Option<String>,
    pub authenticity: Option<String>,
    /// `MANUAL` | `RULE` | absent — from `priced.source`.
    pub price_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    pub alias: Option<String>,
    pub brand: Option<String>,
    /// Kept for a search that ranks; the registry picker ranks nothing, so it is `0.0` there.
    pub score: f64,
}

/// One page of the seller's picker (`GET /career/stocks/registry`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegistryPage {
    pub total: usize,
    pub items: Vec<Candidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PricingRule {
    pub id: String,
    pub active: bool,
    pub scope: Option<String>,
    pub basis: Option<String>,
    pub value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RepricedRow {
    pub stock: String,
    pub product: String,
    pub price: Option<i64>,
    pub next: Option<i64>,
    pub cost: Option<i64>,
    pub rule: Option<String>,
    pub skipped: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttachBody {
    pub product: String,
    pub store: String,
    pub inventory: i64,
    pub cost: i64,
    pub authenticity: Authenticity,
}

impl AttachBody {
    /// `price = cost` is what Landau's own delivery writes before rules run (spec §2.1).
    pub fn to_json(&self) -> Value {
        json!({
            "product": self.product,
            "store": self.store,
            "type": "NORMAL",
            "inventory": self.inventory,
            "price": self.cost,
            "cost": self.cost,
            "condition": "NEW",
            "authenticity": self.authenticity.as_str(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PatchBody {
    pub inventory: Option<i64>,
    pub cost: Option<i64>,
}

impl PatchBody {
    /// Never `price` — a PATCHed price is stamped MANUAL and Landau's rules skip the row for good.
    pub fn to_json(&self) -> Value {
        let mut v = serde_json::Map::new();
        if let Some(i) = self.inventory {
            v.insert("inventory".into(), json!(i));
        }
        if let Some(c) = self.cost {
            v.insert("cost".into(), json!(c));
        }
        Value::Object(v)
    }
}

pub fn peel(v: &Value) -> &Value {
    match v.get("data") {
        Some(inner) if inner.is_object() || inner.is_array() => inner,
        _ => v,
    }
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(str::to_string)
}

fn n(v: &Value, key: &str) -> Option<i64> {
    v.get(key)
        .and_then(|x| x.as_f64())
        .map(|f| f.round() as i64)
}

/// The seller-facing text of an error reply: `message` when the body is a JSON object, the raw
/// string when the body is plain text (not JSON-quoted), else the value's own JSON text.
fn message_of(value: &Value) -> String {
    value
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

impl ShelfRow {
    pub fn from_item(item: &Value) -> Option<ShelfRow> {
        let stock = item.get("stock").unwrap_or(item);
        Some(ShelfRow {
            id: s(stock, "id")?,
            product: s(stock, "product")?,
            product_name: item.get("product").and_then(|p| s(p, "name")),
            store: s(stock, "store"),
            inventory: n(stock, "inventory").unwrap_or(0),
            price: n(stock, "price"),
            cost: n(stock, "cost"),
            condition: s(stock, "condition"),
            authenticity: s(stock, "authenticity"),
            price_source: stock.get("priced").and_then(|p| s(p, "source")),
        })
    }
}

pub struct LandauClient {
    http: Http,
    base: Url,
    token: String,
}

impl LandauClient {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        let http = Http::builder()
            .no_proxy()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| BridgeError::LandauUnreachable(e.to_string()))?;
        let base = Url::parse(&format!("{}/", base_url.trim_end_matches('/')))
            .map_err(|e| BridgeError::SettingsIncomplete(format!("نشانی لندو نامعتبر است: {e}")))?;
        Ok(LandauClient {
            http,
            base,
            token: token.to_string(),
        })
    }

    fn url(&self, path: &str) -> Result<Url> {
        self.base
            .join(path.trim_start_matches('/'))
            .map_err(|e| BridgeError::Other(format!("url {path}: {e}")))
    }

    async fn call(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        let url = self.url(path)?;
        self.call_url(method, url, body).await
    }

    /// Sends a request built fresh on each attempt (so a JSON body can be resent), retrying per
    /// [`retryable`]. Never logs the token — only the path, method, attempt and status/error.
    async fn send_with_retry(
        &self,
        method: &Method,
        url: &Url,
        path: &str,
        body: Option<&Value>,
    ) -> Result<reqwest::Response> {
        let mut attempt: usize = 0;
        loop {
            attempt += 1;
            let mut req = self
                .http
                .request(method.clone(), url.clone())
                .bearer_auth(&self.token);
            if let Some(b) = body {
                req = req.json(b);
            }
            let outcome = req.send().await;
            let status = outcome.as_ref().ok().map(|r| r.status().as_u16());
            if attempt <= RETRY_DELAYS.len() && retryable(status) {
                match &outcome {
                    Ok(res) => tracing::warn!(
                        %method,
                        path,
                        attempt,
                        status = res.status().as_u16(),
                        "landau retrying"
                    ),
                    Err(e) => tracing::warn!(%method, path, attempt, error = %e, "landau retrying"),
                }
                tokio::time::sleep(RETRY_DELAYS[attempt - 1]).await;
                continue;
            }
            return outcome
                .map_err(|e| BridgeError::LandauUnreachable(e.without_url().to_string()));
        }
    }

    async fn call_url(&self, method: Method, url: Url, body: Option<&Value>) -> Result<Value> {
        let path = url.path().to_string();
        let res = self.send_with_retry(&method, &url, &path, body).await?;
        let status = res.status();
        let text = res
            .text()
            .await
            .map_err(|e| BridgeError::LandauUnreachable(e.to_string()))?;
        tracing::debug!(%method, path, %status, "landau");
        let value: Value =
            serde_json::from_str(&text).unwrap_or(Value::String(text.chars().take(300).collect()));
        if status.is_success() {
            return Ok(value);
        }
        let message = message_of(&value);
        Err(match status {
            StatusCode::UNAUTHORIZED => BridgeError::LandauUnauthorized,
            StatusCode::FORBIDDEN => BridgeError::LandauForbidden(message),
            StatusCode::CONFLICT => BridgeError::DuplicateRow,
            _ => BridgeError::LandauRejected {
                status: status.as_u16(),
                body: message.chars().take(300).collect(),
            },
        })
    }

    /// `GET /career/staff/memberships` — what the key's owner acts for.
    pub async fn memberships(&self) -> Result<Vec<Membership>> {
        let v = self
            .call(Method::GET, "/career/staff/memberships", None)
            .await?;
        let items = peel(&v)
            .get("items")
            .and_then(|i| i.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(items
            .iter()
            .filter_map(|m| {
                Some(Membership {
                    business: s(m, "business")?,
                    name: s(m, "name"),
                    principal: s(m, "principal").unwrap_or_default(),
                    approved: m.get("approved").and_then(|a| a.as_bool()).unwrap_or(false),
                    stores: m
                        .get("stores")
                        .and_then(|st| st.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|st| {
                                    Some(Store {
                                        id: s(st, "id")?,
                                        name: s(st, "name"),
                                        kind: s(st, "type"),
                                        state: s(st, "state"),
                                        status: s(st, "status"),
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                })
            })
            .collect())
    }

    /// `GET /career/stocks/shelf?store=&limit=100&skip=` paged to `total`.
    pub async fn shelf(&self, store: &str) -> Result<Vec<ShelfRow>> {
        let mut rows = Vec::new();
        let mut skip = 0usize;
        loop {
            let mut url = self.url("career/stocks/shelf")?;
            url.query_pairs_mut()
                .append_pair("store", store)
                .append_pair("limit", &SHELF_PAGE.to_string())
                .append_pair("skip", &skip.to_string());
            let v = self.call_url(Method::GET, url, None).await?;
            let page = peel(&v);
            let items = page
                .get("items")
                .and_then(|i| i.as_array())
                .cloned()
                .unwrap_or_default();
            let total = page.get("total").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
            rows.extend(items.iter().filter_map(ShelfRow::from_item));
            skip += SHELF_PAGE;
            if items.is_empty() || skip >= total {
                break;
            }
        }
        Ok(rows)
    }

    /// The seller's picker (spec §13): a case-insensitive substring match over `name`/`alias`/
    /// `brand` of every APPROVED definition, alphabetical, `limit` ≤ 100, with `total`.
    pub async fn registry_search(
        &self,
        term: &str,
        limit: usize,
        skip: usize,
    ) -> Result<RegistryPage> {
        let mut url = self.url("career/stocks/registry")?;
        url.query_pairs_mut()
            .append_pair("term", term)
            .append_pair("limit", &limit.clamp(1, 100).to_string())
            .append_pair("skip", &skip.to_string());
        let v = self.call_url(Method::GET, url, None).await?;
        let page = peel(&v);
        let total = page.get("total").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
        let items = page
            .get("items")
            .and_then(|i| i.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(RegistryPage {
            total,
            items: items
                .iter()
                .filter_map(|it| {
                    let p = it.get("product").unwrap_or(it);
                    Some(Candidate {
                        id: s(p, "id")?,
                        name: s(p, "name")?,
                        alias: s(p, "alias"),
                        brand: s(p, "brand"),
                        score: 0.0,
                    })
                })
                .collect(),
        })
    }

    /// `POST /career/stocks` — reply is the row, flat or enveloped.
    pub async fn attach(&self, body: &AttachBody) -> Result<ShelfRow> {
        let v = self
            .call(Method::POST, "/career/stocks", Some(&body.to_json()))
            .await?;
        let row = peel(&v);
        ShelfRow::from_item(row).ok_or_else(|| BridgeError::LandauRejected {
            status: 201,
            body: "reply without id".into(),
        })
    }

    pub async fn patch(&self, id: &str, body: &PatchBody) -> Result<()> {
        self.call(
            Method::PATCH,
            &format!("/career/stocks/{id}"),
            Some(&body.to_json()),
        )
        .await
        .map(|_| ())
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        self.call(Method::DELETE, &format!("/career/stocks/{id}"), None)
            .await
            .map(|_| ())
    }

    pub async fn pricing_rules(&self) -> Result<Vec<PricingRule>> {
        let v = self
            .call(Method::GET, "/career/pricing-rules", None)
            .await?;
        let items = peel(&v)
            .get("items")
            .and_then(|i| i.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(items
            .iter()
            .filter_map(|r| {
                let props = r.get("props").unwrap_or(r);
                Some(PricingRule {
                    id: s(r, "id")?,
                    active: props
                        .get("active")
                        .or_else(|| r.get("active"))
                        .and_then(|a| a.as_bool())
                        .unwrap_or(false),
                    scope: s(props, "scope").or_else(|| s(r, "scope")),
                    basis: s(props, "basis").or_else(|| s(r, "basis")),
                    value: props
                        .get("value")
                        .or_else(|| r.get("value"))
                        .and_then(|x| x.as_f64()),
                })
            })
            .collect())
    }

    /// `POST /career/pricing-rules/reprice {store, commit: true, overwrite, skip}` paged to total.
    pub async fn reprice(&self, store: &str, overwrite: bool) -> Result<Vec<RepricedRow>> {
        let mut rows = Vec::new();
        let mut skip = 0u64;
        loop {
            let body =
                json!({"store": store, "commit": true, "overwrite": overwrite, "skip": skip});
            let v = self
                .call(Method::POST, "/career/pricing-rules/reprice", Some(&body))
                .await?;
            let page = peel(&v);
            let items = page
                .get("rows")
                .and_then(|r| r.as_array())
                .cloned()
                .unwrap_or_default();
            let total = page.get("total").and_then(|t| t.as_u64()).unwrap_or(0);
            let limit = page
                .get("limit")
                .and_then(|t| t.as_u64())
                .unwrap_or(100)
                .max(1);
            rows.extend(items.iter().filter_map(|r| {
                Some(RepricedRow {
                    stock: s(r, "stock")?,
                    product: s(r, "product").unwrap_or_default(),
                    price: n(r, "price"),
                    next: n(r, "next"),
                    cost: n(r, "cost"),
                    rule: s(r, "rule"),
                    skipped: s(r, "skipped"),
                })
            }));
            skip += limit;
            if items.is_empty() || skip >= total {
                break;
            }
        }
        Ok(rows)
    }

    /// Poll the shelf until every id is visible. Returns the ids still missing at `timeout`.
    pub async fn await_visible(
        &self,
        store: &str,
        ids: &[String],
        timeout: Duration,
    ) -> Result<Vec<String>> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut missing: Vec<String> = ids.to_vec();
        loop {
            let seen: std::collections::HashSet<String> =
                self.shelf(store).await?.into_iter().map(|r| r.id).collect();
            missing.retain(|id| !seen.contains(id));
            if missing.is_empty() || tokio::time::Instant::now() >= deadline {
                return Ok(missing);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_policy_covers_transport_errors_408_429_and_5xx_only() {
        assert!(retryable(None));
        assert!(retryable(Some(408)));
        assert!(retryable(Some(429)));
        assert!(retryable(Some(500)));
        assert!(retryable(Some(502)));
        assert!(!retryable(Some(200)));
        assert!(!retryable(Some(400)));
        assert!(!retryable(Some(401)));
        assert!(!retryable(Some(403)));
        assert!(!retryable(Some(404)));
    }

    #[test]
    fn peels_the_data_envelope_only_when_present() {
        let enveloped = serde_json::json!({"data": {"id": "1"}});
        let flat = serde_json::json!({"id": "2"});
        assert_eq!(peel(&enveloped)["id"], "1");
        assert_eq!(peel(&flat)["id"], "2");
    }

    #[test]
    fn shelf_rows_read_price_source_and_variant() {
        let item = serde_json::json!({
            "stock": {"id": "s1", "product": "p1", "store": "st", "inventory": 3, "price": 173000.0, "cost": 150000,
                      "condition": "NEW", "authenticity": "ORIGINAL", "priced": {"source": "RULE", "rule": "r1"}},
            "product": {"id": "p1", "name": "لنت جلو"}
        });
        let row = ShelfRow::from_item(&item).unwrap();
        assert_eq!(row.id, "s1");
        assert_eq!(row.price, Some(173000));
        assert_eq!(row.cost, Some(150000));
        assert_eq!(row.price_source.as_deref(), Some("RULE"));
        assert_eq!(row.product_name.as_deref(), Some("لنت جلو"));
        assert_eq!(row.authenticity.as_deref(), Some("ORIGINAL"));
    }

    #[test]
    fn attach_body_is_what_landau_expects() {
        let body = AttachBody {
            product: "p".into(),
            store: "s".into(),
            inventory: 2,
            cost: 1100,
            authenticity: Authenticity::Fake,
        };
        let v = serde_json::to_value(body.to_json()).unwrap();
        assert_eq!(v["type"], "NORMAL");
        assert_eq!(v["condition"], "NEW");
        assert_eq!(v["authenticity"], "FAKE");
        assert_eq!(v["price"], 1100);
        assert_eq!(v["cost"], 1100);
        assert_eq!(v["inventory"], 2);
    }

    #[test]
    fn registry_page_reads_the_picker_shape() {
        let v = serde_json::json!({"items": [{"product": {"id": "p1", "name": "سیبک فرمان پراید", "alias": "سیبک | جعبه فرمان", "brand": "امیرنیا"}, "held": []}], "total": 14, "limit": 100, "skip": 0});
        let page = peel(&v);
        let total = page.get("total").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
        assert_eq!(total, 14);
        let p = page["items"][0].get("product").unwrap();
        assert_eq!(s(p, "id").as_deref(), Some("p1"));
        assert_eq!(s(p, "brand").as_deref(), Some("امیرنیا"));
    }

    #[test]
    fn message_of_reads_plain_text_bodies_without_json_quoting_and_json_message_field() {
        let plain = Value::String("Service Unavailable".into());
        assert_eq!(message_of(&plain), "Service Unavailable");

        let enveloped = serde_json::json!({"message": "limit must not be greater than 100"});
        assert_eq!(message_of(&enveloped), "limit must not be greater than 100");
    }

    #[test]
    fn patch_body_never_carries_price() {
        let v = PatchBody {
            inventory: Some(5),
            cost: Some(1200),
        }
        .to_json();
        assert!(v.get("price").is_none());
        assert_eq!(v["inventory"], 5);
        let v = PatchBody {
            inventory: None,
            cost: Some(1200),
        }
        .to_json();
        assert!(v.get("inventory").is_none());
    }
}
