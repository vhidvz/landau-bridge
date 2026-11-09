//! A Landau for the tests: a loopback HTTP server answering each request from a closure, and
//! recording every request it received. [`Reply::Drop`] closes the connection without answering —
//! the lost reply of plan 130's PI-8.

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub struct Req {
    pub method: String,
    pub path: String,
    pub body: String,
}

impl Req {
    /// "METHOD path", the path with its query.
    pub fn line(&self) -> String {
        format!("{} {}", self.method, self.path)
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

pub enum Reply {
    Status(u16, String),
    /// Read the request, then close without a reply.
    Drop,
}

pub type Seen = Arc<Mutex<Vec<Req>>>;

/// Start the stub; answers `answer(request)`. Returns its base URL and the request log.
pub fn serve(answer: impl Fn(&Req) -> Reply + Send + 'static) -> (String, Seen) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen: Seen = Arc::default();
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (method, path) = (
                parts.next().unwrap_or("").to_string(),
                parts.next().unwrap_or("").to_string(),
            );
            let mut length = 0usize;
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let req = Req {
                method,
                path,
                body: String::from_utf8_lossy(&body).into_owned(),
            };
            log.lock().unwrap().push(req.clone());
            match answer(&req) {
                Reply::Drop => drop(stream),
                Reply::Status(status, reply) => {
                    let mut out = stream;
                    let _ = write!(
                        out,
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                        reply.len()
                    );
                }
            }
        }
    });
    (url, seen)
}

/// A shelf row as Landau's shelf projection and its write replies carry it.
pub fn row_json(id: &str, product: &str, inventory: i64, cost: i64) -> String {
    format!(
        r#"{{"id":"{id}","product":"{product}","inventory":{inventory},"price":{},"cost":{cost},"condition":"NEW","authenticity":"ORIGINAL","updated_at":"2026-11-09T09:00:00.000Z"}}"#,
        cost + 100
    )
}

/// A shelf page holding `rows` (each from [`row_json`]).
pub fn page(rows: &[String]) -> String {
    format!(r#"{{"items":[{}],"total":{}}}"#, rows.join(","), rows.len())
}

/// The lines a request log holds, "METHOD path" each.
pub fn lines(seen: &Seen) -> Vec<String> {
    seen.lock().unwrap().iter().map(Req::line).collect()
}

/// How many requests were writes (anything but a GET).
pub fn writes(seen: &Seen) -> Vec<Req> {
    seen.lock()
        .unwrap()
        .iter()
        .filter(|r| r.method != "GET")
        .cloned()
        .collect()
}
