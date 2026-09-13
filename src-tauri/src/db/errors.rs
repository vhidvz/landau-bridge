use rusqlite::params;
use serde::Serialize;

use super::{now, Db};
use crate::errors::Result;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ErrorRow {
    pub id: i64,
    pub at: String,
    pub job_id: Option<i64>,
    pub kind: String,
    pub message: String,
    pub context: Option<serde_json::Value>,
}

impl Db {
    pub fn record_error(
        &self,
        job_id: Option<i64>,
        kind: &str,
        message: &str,
        context: Option<serde_json::Value>,
    ) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO errors(at, job_id, kind, message, context_json) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![now(), job_id, kind, message, context.map(|v| v.to_string())],
            )
            .map(|_| ())
        })
    }

    pub fn list_errors(&self, limit: i64) -> Result<Vec<ErrorRow>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id, at, job_id, kind, message, context_json FROM errors ORDER BY id DESC LIMIT ?1")?;
            let rows = st.query_map(params![limit], |r| {
                let ctx: Option<String> = r.get(5)?;
                Ok(ErrorRow {
                    id: r.get(0)?,
                    at: r.get(1)?,
                    job_id: r.get(2)?,
                    kind: r.get(3)?,
                    message: r.get(4)?,
                    context: ctx.and_then(|c| serde_json::from_str(&c).ok()),
                })
            })?;
            rows.collect()
        })
    }

    pub fn count_unresolved_errors(&self) -> Result<i64> {
        self.with(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM errors WHERE at > datetime('now', '-7 days')",
                [],
                |r| r.get(0),
            )
        })
    }
}
