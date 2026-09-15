//! The landau-parts map (spec §14 c): every raw Barezpakhsh name that `landau-parts` imported
//! under a Landau product name + brand. Loaded from `output/parts_map.csv` by the seller.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{now, Db};
use crate::errors::{BridgeError, Result};
use crate::matching::normalize::name_key;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NameMapInfo {
    pub rows: i64,
    pub products: i64,
    pub loaded_at: Option<String>,
    pub file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapRow {
    pub product_name: String,
    pub brand: String,
    pub raw_name: String,
    pub code: String,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MapEntry {
    pub raw_name: String,
    pub code: String,
    pub available: bool,
}

#[derive(Debug, Deserialize)]
struct Csv {
    code: String,
    raw_name: String,
    stock_status: String,
    product_name: String,
    brand: String,
}

/// Rows of `parts_map.csv` that name a product (state `done`/`alias`); a BOM is tolerated.
pub fn parse_parts_map<R: std::io::Read>(r: R) -> Result<Vec<MapRow>> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(r);
    let mut out = Vec::new();
    for rec in reader.deserialize::<Csv>() {
        let rec = rec.map_err(|e| BridgeError::Other(format!("parts_map.csv: {e}")))?;
        if rec.product_name.trim().is_empty() || rec.raw_name.trim().is_empty() {
            continue;
        }
        out.push(MapRow {
            product_name: rec.product_name.trim().to_string(),
            brand: rec.brand.trim().to_string(),
            raw_name: rec.raw_name.trim().to_string(),
            code: rec.code.trim().to_string(),
            available: rec.stock_status.trim() == "موجود",
        });
    }
    Ok(out)
}

impl Db {
    /// Replace the whole map and record where and when it came from.
    pub fn replace_name_map(&self, rows: &[MapRow], file: &str) -> Result<NameMapInfo> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            tx.execute_batch("DELETE FROM name_map")?;
            for (seq, r) in rows.iter().enumerate() {
                tx.execute(
                    "INSERT OR REPLACE INTO name_map(product_key, brand_key, raw_name, code, available, seq)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        name_key(&r.product_name),
                        name_key(&r.brand),
                        r.raw_name,
                        r.code,
                        r.available as i64,
                        seq as i64
                    ],
                )?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO settings(key, value) VALUES ('name_map_file', ?1)",
                params![file],
            )?;
            tx.execute(
                "INSERT OR REPLACE INTO settings(key, value) VALUES ('name_map_loaded_at', ?1)",
                params![now()],
            )?;
            tx.commit()?;
            Ok(())
        })?;
        self.name_map_info()
    }

    pub fn name_map_info(&self) -> Result<NameMapInfo> {
        self.with(|c| {
            let rows: i64 = c.query_row("SELECT count(*) FROM name_map", [], |r| r.get(0))?;
            let products: i64 = c.query_row(
                "SELECT count(*) FROM (SELECT DISTINCT product_key, brand_key FROM name_map)",
                [],
                |r| r.get(0),
            )?;
            let get = |k: &str| -> rusqlite::Result<Option<String>> {
                c.query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    params![k],
                    |r| r.get(0),
                )
                .optional()
            };
            Ok(NameMapInfo {
                rows,
                products,
                loaded_at: get("name_map_loaded_at")?,
                file: get("name_map_file")?,
            })
        })
    }

    /// The raw names imported under this product name + brand, available first, then file order.
    pub fn raw_names_for(&self, product_name: &str, brand: &str) -> Result<Vec<MapEntry>> {
        let (pk, bk) = (name_key(product_name), name_key(brand));
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT raw_name, code, available FROM name_map WHERE product_key = ?1 AND brand_key = ?2
                 ORDER BY available DESC, seq ASC",
            )?;
            let rows = st.query_map(params![pk, bk], |r| {
                Ok(MapEntry {
                    raw_name: r.get(0)?,
                    code: r.get(1)?,
                    available: r.get::<_, i64>(2)? != 0,
                })
            })?;
            rows.collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}code,raw_name,stock_status,price,state,attempts,last_error,product_id,product_name,brand,pack_qty,alias_of,duplicate_of_code\n\
CR39150201,ECU ای سی یو E5 EOBD موتور ECMT EC5 DAE AT6 پژو 207 کروز,موجود,118373640,done,0,,80,ECU موتور پژو 207 اتوماتیک AT6,کروز,1,,\n\
FIT0002601,آرمیچر استارت پراید AZE1540 کروز,ناموجود,1,done,0,,90,آرمیچر استارت پراید,کروز,1,,\n\
4100118,آرمیچر استارت پراید ایسکرا (کروز) 16360422,موجود,2,done,0,,90,آرمیچر استارت پراید,کروز,1,,\n\
9999,بدون محصول,موجود,3,pending,0,,,,,,,\n";

    #[test]
    fn parses_only_rows_that_name_a_product() {
        let rows = parse_parts_map(SAMPLE.as_bytes()).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].product_name, "ECU موتور پژو 207 اتوماتیک AT6");
        assert_eq!(rows[0].brand, "کروز");
        assert!(rows[0].available);
        assert!(!rows[1].available);
    }

    #[test]
    fn a_loaded_map_answers_raw_names_available_first_in_file_order() {
        let db = Db::in_memory().unwrap();
        let info = db
            .replace_name_map(
                &parse_parts_map(SAMPLE.as_bytes()).unwrap(),
                "/x/parts_map.csv",
            )
            .unwrap();
        assert_eq!((info.rows, info.products), (3, 2));
        assert_eq!(info.file.as_deref(), Some("/x/parts_map.csv"));
        assert!(info.loaded_at.is_some());
        let names = db.raw_names_for("آرميچر استارت پرايد", "كروز").unwrap();
        assert_eq!(
            names
                .iter()
                .map(|e| e.raw_name.as_str())
                .collect::<Vec<_>>(),
            [
                "آرمیچر استارت پراید ایسکرا (کروز) 16360422",
                "آرمیچر استارت پراید AZE1540 کروز"
            ]
        );
        assert_eq!(names[0].code, "4100118");
        assert!(db.raw_names_for("چیزی", "").unwrap().is_empty());
        // a second load replaces the first
        let info = db.replace_name_map(&[], "/y.csv").unwrap();
        assert_eq!(info.rows, 0);
        assert_eq!(db.name_map_info().unwrap().rows, 0);
    }
}
