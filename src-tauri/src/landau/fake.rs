//! A Landau that keeps state, for the offline end-to-end tests (plan 130 task 12): the registry,
//! one store's live shelf and trash, and every route a job calls — memberships, pricing rules, the
//! picker, the shelf read, attach, patch, restore and reprice. It is served over loopback by
//! [`super::stub::serve`], so every request goes through the real client, retries and envelope
//! reading included. Its rules are the ones the bridge depends on: a tuple `(product, store, NEW,
//! authenticity)` is unique across the live shelf and the trash (409), an unknown product answers
//! Landau's own 404 sentence, and the rule prices a written cost at +15 % to the nearest 1 000.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::stub::{self, Reply, Req, Seen};
use super::Candidate;

pub const BUSINESS: &str = "b-fake";

/// One shelf row as the fake holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: String,
    pub product: String,
    pub inventory: i64,
    pub cost: Option<i64>,
    pub price: Option<i64>,
    pub authenticity: String,
    pub deleted: bool,
    /// Bumped on every change, so a fresh read sees a row move.
    pub updated: u64,
}

struct State {
    store: String,
    registry: Vec<Candidate>,
    rows: Vec<Row>,
    next_id: u64,
    clock: u64,
}

pub struct FakeLandau {
    pub url: String,
    pub seen: Seen,
    state: Arc<Mutex<State>>,
}

/// The rule's price for a cost: +15 %, to the nearest 1 000 (the deck's rule, PI-14).
pub fn rule_price(cost: i64) -> i64 {
    ((cost as f64 * 1.15 / 1000.0).round() as i64) * 1000
}

impl FakeLandau {
    pub fn start(store: &str, registry: Vec<Candidate>, rows: Vec<Row>) -> FakeLandau {
        let state = Arc::new(Mutex::new(State {
            store: store.to_string(),
            registry,
            rows,
            next_id: 1,
            clock: 1,
        }));
        let shared = state.clone();
        let (url, seen) = stub::serve(move |r| answer(&shared, r));
        FakeLandau { url, seen, state }
    }

    /// A row as it stands now (live or in the trash).
    pub fn row(&self, id: &str) -> Option<Row> {
        let s = self.state.lock().unwrap();
        s.rows.iter().find(|r| r.id == id).cloned()
    }

    /// The live row for a product, if any.
    pub fn row_of(&self, product: &str) -> Option<Row> {
        let s = self.state.lock().unwrap();
        s.rows
            .iter()
            .find(|r| r.product == product && !r.deleted)
            .cloned()
    }

    /// Every request but a read: `(METHOD path, body)`.
    pub fn writes(&self) -> Vec<Req> {
        stub::writes(&self.seen)
            .into_iter()
            .filter(|r| r.path != "/career/pricing-rules/reprice")
            .collect()
    }

    pub fn forget_requests(&self) {
        self.seen.lock().unwrap().clear();
    }
}

fn query(path: &str) -> (String, Vec<(String, String)>) {
    let (p, q) = path.split_once('?').unwrap_or((path, ""));
    let pairs = url::form_urlencoded::parse(q.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    (p.to_string(), pairs)
}

fn param<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn ok(v: Value) -> Reply {
    Reply::Status(200, v.to_string())
}

fn status(code: u16, message: &str) -> Reply {
    Reply::Status(code, json!({ "message": message }).to_string())
}

fn stock_json(r: &Row, store: &str) -> Value {
    json!({
        "id": r.id,
        "product": r.product,
        "store": store,
        "inventory": r.inventory,
        "cost": r.cost,
        "price": r.price,
        "condition": "NEW",
        "authenticity": r.authenticity,
        "updated_at": format!("2026-11-09T09:00:{:02}.{:03}Z", r.updated / 1000 % 60, r.updated % 1000),
    })
}

fn contains(hay: &Option<String>, needle: &str) -> bool {
    hay.as_deref()
        .is_some_and(|h| h.to_lowercase().contains(&needle.to_lowercase()))
}

fn answer(state: &Arc<Mutex<State>>, req: &Req) -> Reply {
    let mut s = state.lock().unwrap();
    let (path, pairs) = query(&req.path);
    let store = s.store.clone();
    match (req.method.as_str(), path.as_str()) {
        ("GET", "/career/staff/memberships") => ok(json!({"items": [{
            "business": BUSINESS, "name": "فروشگاه آزمایشی", "principal": "p", "approved": true,
            "stores": [{"id": store, "name": "فروشگاه", "type": "RETAIL", "state": "APPROVED"}],
        }]})),
        ("GET", "/career/pricing-rules") => ok(json!({"items": [{
            "id": "rule-1", "active": true, "scope": "SHELF", "basis": "MARKUP_PERCENT", "value": 15,
        }]})),
        ("GET", "/career/stocks/registry") => {
            let term = param(&pairs, "term").unwrap_or("").to_string();
            let limit: usize = param(&pairs, "limit")
                .and_then(|v| v.parse().ok())
                .unwrap_or(100);
            let skip: usize = param(&pairs, "skip")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let mut hits: Vec<&Candidate> = s
                .registry
                .iter()
                .filter(|c| {
                    term.is_empty()
                        || contains(&Some(c.name.clone()), &term)
                        || contains(&c.alias, &term)
                        || contains(&c.brand, &term)
                })
                .collect();
            hits.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
            let total = hits.len();
            let items: Vec<Value> = hits
                .into_iter()
                .skip(skip)
                .take(limit)
                .map(|c| json!({"id": c.id, "name": c.name, "alias": c.alias, "brand": c.brand}))
                .collect();
            ok(json!({"data": {"items": items, "total": total}}))
        }
        ("GET", "/career/stocks/shelf") => {
            let deleted = param(&pairs, "deleted") == Some("true");
            let term = param(&pairs, "term").map(str::to_string);
            let authenticity = param(&pairs, "authenticity").map(str::to_string);
            let limit: usize = param(&pairs, "limit")
                .and_then(|v| v.parse().ok())
                .unwrap_or(100);
            let skip: usize = param(&pairs, "skip")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let hits: Vec<Value> = s
                .rows
                .iter()
                .filter(|r| r.deleted == deleted)
                .filter(|r| authenticity.as_deref().is_none_or(|a| r.authenticity == a))
                .filter_map(|r| {
                    let p = s.registry.iter().find(|c| c.id == r.product);
                    if let Some(t) = &term {
                        let named = p.is_some_and(|p| {
                            contains(&Some(p.name.clone()), t)
                                || contains(&p.alias, t)
                                || contains(&p.brand, t)
                        });
                        if !named {
                            return None;
                        }
                    }
                    Some(json!({
                        "stock": stock_json(r, &store),
                        "product": {"id": r.product, "name": p.map(|p| p.name.clone()), "brand": p.and_then(|p| p.brand.clone())},
                    }))
                })
                .collect();
            let total = hits.len();
            let items: Vec<Value> = hits.into_iter().skip(skip).take(limit).collect();
            ok(json!({"items": items, "total": total}))
        }
        ("POST", "/career/stocks") => {
            let b = req.json();
            let product = b["product"].as_str().unwrap_or("").to_string();
            let authenticity = b["authenticity"].as_str().unwrap_or("ORIGINAL").to_string();
            if !s.registry.iter().any(|c| c.id == product) {
                return status(404, "there is no such part in Landau’s registry");
            }
            if s.rows
                .iter()
                .any(|r| r.product == product && r.authenticity == authenticity)
            {
                return status(409, "this part is already on the shelf");
            }
            s.next_id += 1;
            s.clock += 1;
            let cost = b["cost"].as_i64();
            let row = Row {
                id: format!("new-{}", s.next_id),
                product,
                inventory: b["inventory"].as_i64().unwrap_or(0),
                cost,
                price: cost.map(rule_price),
                authenticity,
                deleted: false,
                updated: s.clock,
            };
            let reply = stock_json(&row, &store);
            s.rows.push(row);
            Reply::Status(201, reply.to_string())
        }
        ("POST", "/career/pricing-rules/reprice") => {
            let b = req.json();
            let ids: Vec<String> = b["stocks"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let mut rows = Vec::new();
            for id in ids {
                if let Some(r) = s.rows.iter_mut().find(|r| r.id == id && !r.deleted) {
                    let next = r.cost.map(rule_price);
                    let changed = next != r.price;
                    r.price = next.or(r.price);
                    rows.push(json!({
                        "stock": r.id, "product": r.product, "price": r.price,
                        "next": if changed { next } else { None }, "cost": r.cost, "rule": "rule-1",
                        "skipped": if changed { None } else { Some("UNCHANGED") },
                    }));
                }
            }
            ok(json!({"rows": rows}))
        }
        (method, p) if p.starts_with("/career/stocks/") => {
            let rest = &p["/career/stocks/".len()..];
            let (id, restore) = match rest.strip_suffix("/restore") {
                Some(id) => (id.to_string(), true),
                None => (rest.to_string(), false),
            };
            s.clock += 1;
            let clock = s.clock;
            let Some(row) = s.rows.iter_mut().find(|r| r.id == id) else {
                return status(404, "stock not found");
            };
            match (method, restore) {
                ("PUT", true) if row.deleted => {
                    row.deleted = false;
                    row.updated = clock;
                }
                ("PATCH", false) if !row.deleted => {
                    let b = req.json();
                    if let Some(i) = b["inventory"].as_i64() {
                        row.inventory = i;
                    }
                    if let Some(c) = b["cost"].as_i64() {
                        row.cost = Some(c);
                        row.price = Some(rule_price(c));
                    }
                    row.updated = clock;
                }
                _ => return status(404, "stock not found"),
            }
            let reply = stock_json(row, &store);
            ok(reply)
        }
        _ => status(404, &format!("Cannot {} {}", req.method, req.path)),
    }
}
