use rusqlite::params;

use super::{now, Db};
use crate::domain::{InvoiceHeader, InvoiceKind, InvoiceLine};
use crate::errors::Result;

/// One invoice line joined with the header facts the planner needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineWithInvoice {
    pub invoice_id: i64,
    pub number: String,
    pub kind: InvoiceKind,
    pub paid: bool,
    pub date_jalali: String,
    pub code: String,
    pub name: String,
    pub qty: i64,
    pub unit: String,
    pub unit_price: i64,
}

impl Db {
    /// Insert or find the header. Returns the row id.
    pub fn upsert_invoice(&self, supplier: &str, h: &InvoiceHeader) -> Result<i64> {
        self.with(|c| {
            c.execute(
                "INSERT INTO invoices(supplier, number, kind, date_jalali, date_iso, paid, url)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(supplier, number, kind) DO UPDATE SET paid = excluded.paid, date_jalali = excluded.date_jalali",
                params![supplier, h.number, h.kind.as_str(), h.date_jalali, h.date_iso, h.paid as i64, h.url],
            )?;
            c.query_row(
                "SELECT id FROM invoices WHERE supplier = ?1 AND number = ?2 AND kind = ?3",
                params![supplier, h.number, h.kind.as_str()],
                |r| r.get(0),
            )
        })
    }

    pub fn invoice_url(&self, invoice_id: i64) -> Result<String> {
        self.with(|c| {
            c.query_row(
                "SELECT url FROM invoices WHERE id = ?1",
                params![invoice_id],
                |r| r.get(0),
            )
        })
    }

    /// Headers whose lines were never fetched.
    pub fn unfetched_invoice_ids(&self, supplier: &str) -> Result<Vec<i64>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id FROM invoices WHERE supplier = ?1 AND fetched_at IS NULL ORDER BY id",
            )?;
            let rows = st.query_map(params![supplier], |r| r.get(0))?;
            rows.collect()
        })
    }

    /// Replace the invoice's lines and stamp `fetched_at`. One transaction.
    pub fn replace_lines(&self, invoice_id: i64, lines: &[InvoiceLine]) -> Result<()> {
        self.with(|c| {
            c.execute_batch("BEGIN")?;
            let result = (|| {
                c.execute("DELETE FROM invoice_lines WHERE invoice_id = ?1", params![invoice_id])?;
                let mut st = c.prepare(
                    "INSERT INTO invoice_lines(invoice_id, line_no, code, name, qty, unit, unit_price) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )?;
                for l in lines {
                    st.execute(params![invoice_id, l.line_no, l.code, l.name, l.qty, l.unit, l.unit_price])?;
                }
                c.execute("UPDATE invoices SET fetched_at = ?2 WHERE id = ?1", params![invoice_id, now()])?;
                Ok(())
            })();
            match result {
                Ok(()) => c.execute_batch("COMMIT"),
                Err(e) => {
                    let _ = c.execute_batch("ROLLBACK");
                    Err(e)
                }
            }
        })
    }

    /// Every fetched line of the supplier, oldest invoice first.
    pub fn lines_with_invoices(&self, supplier: &str) -> Result<Vec<LineWithInvoice>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT i.id, i.number, i.kind, i.paid, i.date_jalali, l.code, l.name, l.qty, l.unit, l.unit_price
                 FROM invoice_lines l JOIN invoices i ON i.id = l.invoice_id
                 WHERE i.supplier = ?1 ORDER BY i.date_jalali, i.id, l.line_no",
            )?;
            let rows = st.query_map(params![supplier], |r| {
                let kind: String = r.get(2)?;
                Ok(LineWithInvoice {
                    invoice_id: r.get(0)?,
                    number: r.get(1)?,
                    kind: InvoiceKind::parse(&kind).unwrap_or(InvoiceKind::Sale),
                    paid: r.get::<_, i64>(3)? != 0,
                    date_jalali: r.get(4)?,
                    code: r.get(5)?,
                    name: r.get(6)?,
                    qty: r.get(7)?,
                    unit: r.get(8)?,
                    unit_price: r.get(9)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn count_invoices(&self, supplier: &str) -> Result<(i64, i64)> {
        self.with(|c| {
            c.query_row(
                "SELECT COUNT(*), COALESCE(SUM(paid = 0), 0) FROM invoices WHERE supplier = ?1",
                params![supplier],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
    }

    pub fn imported_invoice_ids(&self) -> Result<std::collections::HashSet<i64>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT invoice_id FROM invoice_imports")?;
            let rows = st.query_map([], |r| r.get::<_, i64>(0))?;
            rows.collect()
        })
    }

    pub fn mark_invoices_imported(&self, invoice_ids: &[i64], job_id: i64) -> Result<()> {
        self.with(|c| {
            let mut st = c.prepare(
                "INSERT OR IGNORE INTO invoice_imports(invoice_id, job_id) VALUES (?1, ?2)",
            )?;
            for id in invoice_ids {
                st.execute(params![id, job_id])?;
            }
            Ok(())
        })
    }
}
