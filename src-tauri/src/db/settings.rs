use rusqlite::{params, OptionalExtension};

use super::Db;
use crate::errors::Result;

impl Db {
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        self.with(|c| {
            c.query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()
        })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map(|_| ())
        })
    }

    pub fn delete_setting(&self, key: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM settings WHERE key = ?1", params![key])
                .map(|_| ())
        })
    }
}
