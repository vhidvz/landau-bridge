//! The pure half of the jobs (spec §7.1 Aggregate/Write, §7.3): what to add, what to write.

use std::collections::{BTreeMap, HashSet};

use crate::db::invoices::LineWithInvoice;
use crate::domain::{ImportMode, InvoiceKind};
use crate::landau::ShelfRow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregatedItem {
    pub code: String,
    /// The most recent invoice's spelling — the search term is built from it.
    pub name: String,
    pub qty: i64,
    pub invoice_ids: Vec<i64>,
    /// Returns exceeded purchases; `qty` was clamped to 0.
    pub negative: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Aggregation {
    pub items: Vec<AggregatedItem>,
    pub skipped_unpaid: Vec<i64>,
    pub skipped_imported: Vec<i64>,
    pub non_positive_lines: usize,
}

/// Quantity mode: paid invoices dated on/after `since` and not transferred before, sales minus
/// returns. Zero mode: every paid invoice, quantity 0. Lines are expected oldest first.
pub fn aggregate(
    lines: &[LineWithInvoice],
    mode: ImportMode,
    since: Option<&str>,
    already: &HashSet<i64>,
) -> Aggregation {
    let mut agg = Aggregation::default();
    let mut items: BTreeMap<String, AggregatedItem> = BTreeMap::new();
    let mut unpaid: HashSet<i64> = HashSet::new();
    let mut imported: HashSet<i64> = HashSet::new();

    for l in lines {
        if !l.paid {
            unpaid.insert(l.invoice_id);
            continue;
        }
        if mode == ImportMode::Quantity {
            if let Some(since) = since {
                if l.date_jalali.as_str() < since {
                    continue;
                }
            }
            if already.contains(&l.invoice_id) {
                imported.insert(l.invoice_id);
                continue;
            }
        }
        if l.qty <= 0 {
            agg.non_positive_lines += 1;
            continue;
        }
        let signed = if l.kind == InvoiceKind::Return {
            -l.qty
        } else {
            l.qty
        };
        let item = items
            .entry(l.code.clone())
            .or_insert_with(|| AggregatedItem {
                code: l.code.clone(),
                name: l.name.clone(),
                qty: 0,
                invoice_ids: Vec::new(),
                negative: false,
            });
        item.qty += signed;
        item.name = l.name.clone();
        if !item.invoice_ids.contains(&l.invoice_id) {
            item.invoice_ids.push(l.invoice_id);
        }
    }

    agg.items = items
        .into_values()
        .map(|mut item| {
            if mode == ImportMode::Zero {
                item.qty = 0;
            } else if item.qty < 0 {
                item.negative = true;
                item.qty = 0;
            }
            item
        })
        .collect();
    agg.items.sort_by(|a, b| a.name.cmp(&b.name));
    agg.skipped_unpaid = unpaid.into_iter().collect();
    agg.skipped_unpaid.sort_unstable();
    agg.skipped_imported = imported.into_iter().collect();
    agg.skipped_imported.sort_unstable();
    agg
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteAction {
    Attach {
        inventory: i64,
    },
    Patch {
        stock_id: String,
        inventory: Option<i64>,
        qty_before: i64,
    },
}

/// The row for `(product, NEW, authenticity)` — the whole unique tuple, never a subset (spec §2.1).
pub fn find_row<'a>(
    shelf: &'a [ShelfRow],
    product_id: &str,
    authenticity: &str,
) -> Option<&'a ShelfRow> {
    shelf.iter().find(|r| {
        r.product == product_id
            && r.condition.as_deref() == Some("NEW")
            && r.authenticity.as_deref() == Some(authenticity)
    })
}

pub fn plan_write(existing: Option<&ShelfRow>, qty: i64, mode: ImportMode) -> WriteAction {
    match existing {
        Some(row) => WriteAction::Patch {
            stock_id: row.id.clone(),
            inventory: (mode == ImportMode::Quantity).then_some(row.inventory + qty),
            qty_before: row.inventory,
        },
        None => WriteAction::Attach {
            inventory: if mode == ImportMode::Quantity { qty } else { 0 },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn line(
        invoice_id: i64,
        kind: InvoiceKind,
        paid: bool,
        date: &str,
        code: &str,
        name: &str,
        qty: i64,
    ) -> LineWithInvoice {
        LineWithInvoice {
            invoice_id,
            number: invoice_id.to_string(),
            kind,
            paid,
            date_jalali: date.into(),
            code: code.into(),
            name: name.into(),
            qty,
            unit: "عدد".into(),
            unit_price: 1000,
        }
    }

    #[test]
    fn quantity_mode_sums_paid_sales_minus_returns_since_the_date() {
        let lines = vec![
            line(1, InvoiceKind::Sale, true, "1401/10/05", "A", "old name", 5),
            line(2, InvoiceKind::Sale, true, "1403/01/10", "A", "new name", 3),
            line(
                3,
                InvoiceKind::Return,
                true,
                "1403/02/01",
                "A",
                "new name",
                1,
            ),
            line(
                4,
                InvoiceKind::Sale,
                false,
                "1403/03/01",
                "A",
                "new name",
                9,
            ),
            line(5, InvoiceKind::Sale, true, "1403/03/02", "B", "b", 0),
            line(6, InvoiceKind::Sale, true, "1403/03/03", "C", "c", 2),
        ];
        let already = HashSet::from([6]);
        let agg = aggregate(&lines, ImportMode::Quantity, Some("1402/01/01"), &already);
        let a = agg.items.iter().find(|i| i.code == "A").unwrap();
        assert_eq!(
            a.qty, 2,
            "3 bought minus 1 returned; the 1401 invoice is before the date"
        );
        assert_eq!(a.name, "new name", "the most recent name wins");
        assert_eq!(a.invoice_ids, vec![2, 3]);
        assert!(
            agg.items.iter().all(|i| i.code != "C"),
            "invoice 6 was transferred before"
        );
        assert_eq!(agg.skipped_unpaid, vec![4]);
        assert_eq!(agg.skipped_imported, vec![6]);
        assert_eq!(agg.non_positive_lines, 1);
        assert!(
            agg.items.iter().all(|i| i.code != "B"),
            "a zero-quantity line contributes nothing"
        );
    }

    #[test]
    fn zero_mode_takes_every_paid_invoice_at_zero() {
        let lines = vec![
            line(1, InvoiceKind::Sale, true, "1401/10/05", "A", "a", 5),
            line(2, InvoiceKind::Sale, false, "1403/01/10", "B", "b", 3),
        ];
        let agg = aggregate(
            &lines,
            ImportMode::Zero,
            Some("1402/01/01"),
            &HashSet::from([1]),
        );
        assert_eq!(agg.items.len(), 1);
        assert_eq!(agg.items[0].code, "A");
        assert_eq!(agg.items[0].qty, 0);
        assert!(
            agg.skipped_imported.is_empty(),
            "zero mode ignores the transfer guard"
        );
    }

    #[test]
    fn net_negative_becomes_zero_and_is_flagged() {
        let lines = vec![
            line(1, InvoiceKind::Sale, true, "1403/01/01", "A", "a", 1),
            line(2, InvoiceKind::Return, true, "1403/01/02", "A", "a", 3),
        ];
        let agg = aggregate(&lines, ImportMode::Quantity, None, &HashSet::new());
        assert_eq!(agg.items[0].qty, 0);
        assert!(agg.items[0].negative);
    }

    fn row(id: &str, product: &str, auth: Option<&str>, inventory: i64) -> ShelfRow {
        ShelfRow {
            id: id.into(),
            product: product.into(),
            product_name: None,
            store: None,
            inventory,
            price: Some(1000),
            cost: None,
            condition: Some("NEW".into()),
            authenticity: auth.map(str::to_string),
            price_source: None,
        }
    }

    #[test]
    fn find_row_matches_the_whole_variant_tuple() {
        let shelf = vec![
            row("s1", "p1", Some("ORIGINAL"), 2),
            row("s2", "p1", Some("FAKE"), 0),
            row("s3", "p2", None, 1),
        ];
        assert_eq!(
            find_row(&shelf, "p1", "FAKE").map(|r| r.id.as_str()),
            Some("s2")
        );
        assert_eq!(
            find_row(&shelf, "p1", "ORIGINAL").map(|r| r.id.as_str()),
            Some("s1")
        );
        assert!(
            find_row(&shelf, "p2", "ORIGINAL").is_none(),
            "a legacy row without a variant is not the same tuple"
        );
    }

    #[test]
    fn plan_write_adds_quantities_and_never_touches_inventory_in_zero_mode() {
        let existing = row("s1", "p1", Some("ORIGINAL"), 2);
        assert_eq!(
            plan_write(Some(&existing), 3, ImportMode::Quantity),
            WriteAction::Patch {
                stock_id: "s1".into(),
                inventory: Some(5),
                qty_before: 2
            }
        );
        assert_eq!(
            plan_write(Some(&existing), 0, ImportMode::Zero),
            WriteAction::Patch {
                stock_id: "s1".into(),
                inventory: None,
                qty_before: 2
            }
        );
        assert_eq!(
            plan_write(None, 3, ImportMode::Quantity),
            WriteAction::Attach { inventory: 3 }
        );
        assert_eq!(
            plan_write(None, 0, ImportMode::Zero),
            WriteAction::Attach { inventory: 0 }
        );
    }
}
