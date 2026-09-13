//! Pure parsers over Barezpakhsh pages (spec §2.2, §4). No network, no state.

use std::sync::LazyLock;

use regex::Regex;
use roxmltree::Document;
use scraper::{ElementRef, Html, Selector};

use crate::domain::{
    latin_digits, normalize_jalali, InvoiceHeader, InvoiceKind, InvoiceLine, PriceQuote,
};
use crate::errors::{BridgeError, Result};

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("static selector")
}

fn missing(page: &str, field: &str) -> BridgeError {
    BridgeError::SupplierParse {
        page: page.into(),
        field: field.into(),
    }
}

static WS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"name="__RequestVerificationToken" type="hidden" value="([^"]+)""#).unwrap()
});

fn js_var(html: &str, name: &str) -> Option<String> {
    Regex::new(&format!(r"var {name} = '([^']*)'"))
        .ok()?
        .captures(html)
        .map(|c| c[1].to_string())
}

/// Visible text of an element, whitespace collapsed.
pub fn text_of(el: ElementRef<'_>) -> String {
    let joined: String = el.text().collect::<Vec<_>>().join(" ");
    WS.replace_all(joined.replace('\u{a0}', " ").trim(), " ")
        .to_string()
}

/// `"1,647,000"` / `"۴"` → number; `None` when there is no digit at all.
pub fn digits(s: &str) -> Option<i64> {
    let d: String = latin_digits(s)
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    d.parse().ok()
}

pub fn is_blocked_page(html: &str) -> bool {
    html.contains("error-section__status") || html.contains("Access Denied")
}

pub fn is_login_page(html: &str) -> bool {
    html.contains(r#"action="/Home/Login""#) && html.contains(r#"name="Username""#)
}

pub fn antiforgery_token(html: &str, page: &str) -> Result<String> {
    TOKEN
        .captures(html)
        .map(|c| c[1].to_string())
        .ok_or_else(|| missing(page, "__RequestVerificationToken"))
}

/// Every `input[name]` (except buttons and unchecked checkboxes) and `select[name]` of the form
/// with `action`, in document order — what a browser would submit before the seller touches it.
pub fn form_fields(html: &str, action: &str, page: &str) -> Result<Vec<(String, String)>> {
    let doc = Html::parse_document(html);
    let form_sel = Selector::parse(&format!(r#"form[action="{action}"]"#))
        .map_err(|_| missing(page, "form"))?;
    let form = doc
        .select(&form_sel)
        .next()
        .ok_or_else(|| missing(page, "form"))?;
    let mut out = Vec::new();
    for el in form.select(&sel("input[name], select[name]")) {
        let v = el.value();
        let name = v.attr("name").unwrap_or_default().to_string();
        match v.name() {
            "input" => {
                let ty = v.attr("type").unwrap_or("text").to_ascii_lowercase();
                if matches!(ty.as_str(), "submit" | "button" | "image" | "reset") {
                    continue;
                }
                if ty == "checkbox" && v.attr("checked").is_none() {
                    continue;
                }
                out.push((name, v.attr("value").unwrap_or_default().to_string()));
            }
            "select" => {
                let selected = el
                    .select(&sel("option[selected]"))
                    .next()
                    .or_else(|| el.select(&sel("option")).next());
                let value = selected.map(|o| {
                    o.value()
                        .attr("value")
                        .map(str::to_string)
                        .unwrap_or_else(|| text_of(o))
                });
                out.push((name, value.unwrap_or_default()));
            }
            _ => {}
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- ledger («معین فاکتور»)

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerContext {
    pub total: u32,
    pub page_size: u32,
    pub year_id: i64,
    pub business_code: String,
}

impl LedgerContext {
    pub fn page_count(&self) -> u32 {
        self.total.div_ceil(self.page_size.max(1))
    }
}

/// The paging constants the page's own script uses for `/Shop/Share/GradualMoein2Load`.
pub fn ledger_context(html: &str) -> Result<LedgerContext> {
    let page = "moein2";
    let total = js_var(html, "tmp")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| missing(page, "tmp"))?;
    let page_size = js_var(html, "tmpPageSize")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| missing(page, "tmpPageSize"))?;
    let year_id = js_var(html, "tmpYearId")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| missing(page, "tmpYearId"))?;
    let business_code = js_var(html, "tmpBusCode").ok_or_else(|| missing(page, "tmpBusCode"))?;
    Ok(LedgerContext {
        total,
        page_size,
        year_id,
        business_code,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LedgerRow {
    pub doc_alias_id: String,
    pub doc_alias: String,
    pub trans_number: String,
    pub voucher_date: String,
    pub voucher_date_old: String,
    pub transaction_id: String,
    pub business_id: String,
    pub status: String,
}

/// The ajax endpoints answer a JSON *string* whose content is XML.
fn unwrap_json_xml(body: &str, page: &str) -> Result<String> {
    serde_json::from_str::<String>(body).map_err(|_| missing(page, "json string body"))
}

fn child_text(node: roxmltree::Node<'_, '_>, tag: &str) -> String {
    node.children()
        .find(|c| c.has_tag_name(tag))
        .and_then(|c| c.text())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Like [`child_text`], but a missing element or empty text is a hard parse failure rather than
/// a silent `""` — for fields whose absence must never be read downstream as a zero/blank value.
fn required_text(node: roxmltree::Node<'_, '_>, tag: &str, page: &str) -> Result<String> {
    let text = child_text(node, tag);
    if text.is_empty() {
        return Err(missing(page, tag));
    }
    Ok(text)
}

pub fn ledger_rows(body: &str) -> Result<Vec<LedgerRow>> {
    let page = "moein2-page";
    let xml = unwrap_json_xml(body, page)?;
    let doc = Document::parse(&xml).map_err(|_| missing(page, "xml"))?;
    doc.descendants()
        .filter(|n| n.has_tag_name("result"))
        .map(|n| {
            Ok(LedgerRow {
                doc_alias_id: required_text(n, "DocAliasID", page)?,
                doc_alias: child_text(n, "DocAliasDescription"),
                trans_number: required_text(n, "TransNumber", page)?,
                voucher_date: required_text(n, "VoucherDate", page)?,
                voucher_date_old: child_text(n, "VoucherDateOld"),
                transaction_id: required_text(n, "TransactionID", page)?,
                business_id: required_text(n, "BusinessID", page)?,
                status: child_text(n, "status"),
            })
        })
        .collect()
}

/// `DocAliasID` 2 = «فاکتور فروش», 6 = «مرجوع از فروش»; 67 («دريافت», a payment) and anything else
/// is not an invoice. Payment state comes from `status`.
pub fn header_of(row: &LedgerRow) -> Option<InvoiceHeader> {
    let kind = match row.doc_alias_id.as_str() {
        "2" => InvoiceKind::Sale,
        "6" => InvoiceKind::Return,
        _ => return None,
    };
    Some(InvoiceHeader {
        number: row.trans_number.clone(),
        kind,
        date_jalali: normalize_jalali(&row.voucher_date)
            .unwrap_or_else(|| row.voucher_date.clone()),
        date_iso: row.voucher_date_old.chars().take(10).collect(),
        paid: row.status.trim() == "پرداخت شده",
        url: format!(
            "/Shop/Preview/Factor/{}/{}",
            row.transaction_id, row.business_id
        ),
    })
}

// ---------------------------------------------------------------- invoice pages

/// Lines of a sale or a return page. Both print the same five leading cells once the mobile-only
/// `td.visible-xs` is dropped: row no, code, name, quantity, unit. The unit-price column comes
/// right after on a sale page («مبلغ واحد») but a return page inserts a warehouse column
/// («انبار») first and calls it «فی» instead — so its index is read from the header row rather
/// than assumed.
pub fn invoice_lines(html: &str) -> Result<Vec<InvoiceLine>> {
    let page = "factor";
    let doc = Html::parse_document(html);
    let table = doc
        .select(&sel("table"))
        .find(|t| text_of(*t).contains("کد کالا"))
        .ok_or_else(|| missing(page, "lines table"))?;
    let not_mobile = |el: &ElementRef| {
        !el.value()
            .attr("class")
            .unwrap_or("")
            .contains("visible-xs")
    };
    let headers: Vec<String> = table
        .select(&sel(r#"th[scope="col"]"#))
        .filter(not_mobile)
        .map(text_of)
        .collect();
    let price_idx = headers
        .iter()
        .position(|h| h == "فی" || h.starts_with("مبلغ واحد"))
        .ok_or_else(|| missing(page, "unit price column"))?;
    let mut lines = Vec::new();
    for tr in table.select(&sel("tr")) {
        let cells: Vec<String> = tr
            .select(&sel("td"))
            .filter(not_mobile)
            .map(text_of)
            .collect();
        if cells.len() <= price_idx.max(4) {
            continue;
        }
        let Some(line_no) = digits(&cells[0])
            .filter(|_| latin_digits(&cells[0]).chars().all(|c| c.is_ascii_digit()))
        else {
            continue; // the «جمع» row and any header row
        };
        let qty = digits(&cells[3]).ok_or_else(|| missing(page, "quantity"))?;
        let unit_price = digits(&cells[price_idx]).ok_or_else(|| missing(page, "unit price"))?;
        lines.push(InvoiceLine {
            line_no: line_no as u32,
            code: latin_digits(&cells[1]),
            name: cells[2].clone(),
            qty,
            unit: cells[4].clone(),
            unit_price,
        });
    }
    if lines.is_empty() {
        return Err(missing(page, "lines"));
    }
    Ok(lines)
}

// ---------------------------------------------------------------- price list («ثبت فاکتور»)

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuyContext {
    pub business_id: String,
    pub user_group_id: String,
}

/// From a `POST /Shop/Buy/Index` reply (the ids only appear after a search).
pub fn buy_context(html: &str) -> Result<BuyContext> {
    let page = "buy";
    Ok(BuyContext {
        business_id: js_var(html, "tmpBusId").ok_or_else(|| missing(page, "tmpBusId"))?,
        user_group_id: js_var(html, "UserGroupId").ok_or_else(|| missing(page, "UserGroupId"))?,
    })
}

pub fn price_quotes(body: &str) -> Result<Vec<PriceQuote>> {
    let page = "items-search";
    let xml = unwrap_json_xml(body, page)?;
    if xml.trim().is_empty() {
        return Ok(Vec::new());
    }
    let doc = Document::parse(&xml).map_err(|_| missing(page, "xml"))?;
    doc.descendants()
        .filter(|n| n.has_tag_name("result"))
        .map(|n| {
            let list_price = required_text(n, "FinalPrice", page)?
                .parse::<f64>()
                .map_err(|_| missing(page, "FinalPrice"))?
                .round() as i64;
            let qty: f64 = required_text(n, "QtyAvailable", page)?
                .parse()
                .map_err(|_| missing(page, "QtyAvailable"))?;
            Ok(PriceQuote {
                code: latin_digits(&required_text(n, "ItemCode", page)?),
                name: child_text(n, "ItemName"),
                list_price,
                vat_percent: child_text(n, "TaxAndTollPercent").parse().ok(),
                has_vat: child_text(n, "HasTaxAndToll") == "true",
                available: qty > 0.0,
                unit: child_text(n, "UnitName"),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const LOGIN: &str = include_str!("../../../fixtures/barezpakhsh/login.html");
    const LOGIN_FAILED: &str = include_str!("../../../fixtures/barezpakhsh/login-failed.html");
    const BLOCKED: &str = include_str!("../../../fixtures/barezpakhsh/blocked-403.html");
    const BILL: &str = include_str!("../../../fixtures/barezpakhsh/bill.html");
    const MOEIN: &str = include_str!("../../../fixtures/barezpakhsh/bill-moein2.html");
    const MOEIN_PAGE2: &str = include_str!("../../../fixtures/barezpakhsh/moein2-page2.json");
    const FACTOR: &str = include_str!("../../../fixtures/barezpakhsh/factor-8818975.html");
    const RETURN: &str = include_str!("../../../fixtures/barezpakhsh/return-9066582.html");
    const BUY_SEARCH: &str = include_str!("../../../fixtures/barezpakhsh/buy-search-3137006.html");
    const ITEMS: &str = include_str!("../../../fixtures/barezpakhsh/items-3137006.json");

    #[test]
    fn recognises_login_and_blocked_pages() {
        assert!(is_login_page(LOGIN));
        assert!(is_login_page(LOGIN_FAILED));
        assert!(!is_login_page(BILL));
        assert!(is_blocked_page(BLOCKED));
        assert!(!is_blocked_page(LOGIN));
        assert_eq!(antiforgery_token(LOGIN, "login").unwrap(), "TOKEN-FIXTURE");
        assert!(antiforgery_token(BLOCKED, "login").is_err());
    }

    #[test]
    fn reads_the_statement_form_without_buttons() {
        let fields = form_fields(BILL, "/Shop/Report/Bill", "bill").unwrap();
        let get = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("BusinessCode"), Some("97100000"));
        assert_eq!(get("BusinessId"), Some("11000"));
        assert_eq!(get("FromDate"), Some("1405/4/24"));
        assert_eq!(get("SelectedFiscalYear.fiscalYearID"), Some("0"));
        assert_eq!(get("command"), None);
        // Deviation from the brief (see task-6-report.md): confirmed byte-for-byte that
        // bill.html carries no `__RequestVerificationToken` field anywhere in the file, unlike
        // login.html/login-failed.html/buy.html/buy-search-3137006.html which all do. `form_fields`
        // reads only what a page actually contains, so this asserts the fixture's real (tokenless) shape.
        assert_eq!(get("__RequestVerificationToken"), None);
    }

    #[test]
    fn reads_the_ledger_paging_context() {
        let ctx = ledger_context(MOEIN).unwrap();
        assert_eq!(
            ctx,
            LedgerContext {
                total: 3107,
                page_size: 15,
                year_id: 0,
                business_code: "97100000".into()
            }
        );
        assert_eq!(ctx.page_count(), 208);
    }

    #[test]
    fn parses_ledger_rows_and_keeps_only_invoices_and_returns() {
        let rows = ledger_rows(MOEIN_PAGE2).unwrap();
        assert_eq!(rows.len(), 15);
        let first = &rows[0];
        assert_eq!(first.trans_number, "226456");
        assert_eq!(first.doc_alias_id, "2");
        let h = header_of(first).unwrap();
        assert_eq!(
            h,
            InvoiceHeader {
                number: "226456".into(),
                kind: InvoiceKind::Sale,
                date_jalali: "1401/10/14".into(),
                date_iso: "2023-01-04".into(),
                paid: true,
                url: "/Shop/Preview/Factor/8818975/11000".into(),
            }
        );
        let headers: Vec<_> = rows.iter().filter_map(header_of).collect();
        assert_eq!(headers.len(), 10, "five receipts (alias 67) are dropped");
        let receipt = rows.iter().find(|r| r.doc_alias_id == "67").unwrap();
        assert!(header_of(receipt).is_none());
        let ret = LedgerRow {
            doc_alias_id: "6".into(),
            status: "پرداخت شده".into(),
            ..first.clone()
        };
        assert_eq!(header_of(&ret).unwrap().kind, InvoiceKind::Return);
        let unpaid = LedgerRow {
            status: "پرداخت نشده".into(),
            ..first.clone()
        };
        assert!(!header_of(&unpaid).unwrap().paid);
    }

    #[test]
    fn ledger_rows_fail_when_transaction_id_is_missing() {
        let xml = "<NewDataSet><result><DocAliasID>2</DocAliasID><TransNumber>226456</TransNumber>\
                   <VoucherDate>1401/10/14</VoucherDate><BusinessID>11000</BusinessID></result></NewDataSet>";
        let body = serde_json::to_string(&xml).unwrap();
        match ledger_rows(&body) {
            Err(BridgeError::SupplierParse { field, .. }) => assert_eq!(field, "TransactionID"),
            other => {
                panic!("expected SupplierParse{{ field: \"TransactionID\", .. }}, got {other:?}")
            }
        }
    }

    #[test]
    fn parses_sale_invoice_lines() {
        let lines = invoice_lines(FACTOR).unwrap();
        assert_eq!(lines.len(), 36);
        assert_eq!(
            lines[0],
            InvoiceLine {
                line_no: 1,
                code: "3137006".into(),
                name: "لنت جلو پژو 405 عظام* (16)".into(),
                qty: 4,
                unit: "دست".into(),
                unit_price: 1_647_000,
            }
        );
        assert_eq!(lines[35].code, "3167247");
        assert_eq!(
            lines.iter().map(|l| l.qty).sum::<i64>(),
            135,
            "matches the invoice's «جمع» row"
        );
    }

    #[test]
    fn parses_return_lines_with_the_same_reader() {
        let lines = invoice_lines(RETURN).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].code, "1023002010");
        assert_eq!(lines[0].qty, 8);
        assert_eq!(lines[0].unit_price, 852_000);
        assert_eq!(lines[1].line_no, 2);
    }

    #[test]
    fn reads_the_price_search_context_and_quotes() {
        assert_eq!(
            buy_context(BUY_SEARCH).unwrap(),
            BuyContext {
                business_id: "11000".into(),
                user_group_id: "70".into()
            }
        );
        let quotes = price_quotes(ITEMS).unwrap();
        assert_eq!(quotes.len(), 1);
        assert_eq!(
            quotes[0],
            PriceQuote {
                code: "3137006".into(),
                name: "لنت جلو پژو 405 عظام* (16)".into(),
                list_price: 10_731_000,
                vat_percent: Some(10.0),
                has_vat: true,
                available: true,
                unit: "دست".into(),
            }
        );
    }

    #[test]
    fn price_quotes_fail_when_item_code_is_missing() {
        let xml = "<NewDataSet><result><FinalPrice>10731000</FinalPrice><QtyAvailable>1</QtyAvailable></result></NewDataSet>";
        let body = serde_json::to_string(&xml).unwrap();
        match price_quotes(&body) {
            Err(BridgeError::SupplierParse { field, .. }) => assert_eq!(field, "ItemCode"),
            other => panic!("expected SupplierParse{{ field: \"ItemCode\", .. }}, got {other:?}"),
        }
    }

    #[test]
    fn digits_reads_persian_and_grouped_numbers() {
        assert_eq!(digits("1,647,000"), Some(1_647_000));
        assert_eq!(digits("۴"), Some(4));
        assert_eq!(digits(" 7,429,440 "), Some(7_429_440));
        assert_eq!(digits("&nbsp;"), None);
    }
}
