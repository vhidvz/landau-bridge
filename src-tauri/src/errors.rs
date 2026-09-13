//! Typed failures (spec §9). Each variant has a stable `kind` for the UI/DB and a Persian message.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, BridgeError>;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum BridgeError {
    #[error("supplier unreachable: {0}")]
    SupplierUnreachable(String),
    #[error("supplier blocked this network")]
    SupplierBlocked,
    #[error("supplier rejected the credentials")]
    SupplierBadCredentials,
    #[error("supplier session expired")]
    SupplierSessionExpired,
    #[error("supplier page {page}: missing {field}")]
    SupplierParse { page: String, field: String },
    #[error("no current price for {code}")]
    NoCurrentPrice { code: String },
    #[error("landau unauthorized")]
    LandauUnauthorized,
    #[error("landau forbidden: {0}")]
    LandauForbidden(String),
    #[error("landau unreachable: {0}")]
    LandauUnreachable(String),
    #[error("landau answered {status}: {body}")]
    LandauRejected { status: u16, body: String },
    #[error("business not approved")]
    BusinessNotApproved,
    #[error("no store")]
    NoStore,
    #[error("product not approved: {0}")]
    ProductNotApproved(String),
    #[error("duplicate shelf row")]
    DuplicateRow,
    #[error("mirror timeout: {0} row(s) not visible")]
    MirrorTimeout(usize),
    #[error("job cancelled")]
    JobCancelled,
    #[error("job not found: {0}")]
    JobNotFound(i64),
    #[error("job is busy: {0}")]
    JobBusy(String),
    #[error("settings incomplete: {0}")]
    SettingsIncomplete(String),
    #[error("database: {0}")]
    Db(String),
    #[error("keyring unavailable: {0}")]
    KeyringUnavailable(String),
    #[error("{0}")]
    Other(String),
}

/// Warning kinds recorded in `errors`/`job_events` that are not failures of a call.
pub mod kinds {
    pub const NO_PRICING_RULE: &str = "NoPricingRule";
    pub const REPRICE_SKIPPED: &str = "RepriceSkipped";
    pub const INVOICE_UNPAID: &str = "InvoiceUnpaid";
    pub const INVOICE_ALREADY_IMPORTED: &str = "InvoiceAlreadyImported";
    pub const LINE_NON_POSITIVE_QTY: &str = "LineNonPositiveQty";
    pub const UNMATCHED: &str = "Unmatched";
    pub const NONE_DECISION: &str = "NoneDecision";
    pub const JOB_INTERRUPTED: &str = "JobInterrupted";
    pub const VAT_RATE_MISMATCH: &str = "VatRateMismatch";
    pub const NEGATIVE_QUANTITY: &str = "NegativeQuantity";
    pub const AMBIGUOUS_SUPPLIER_LINK: &str = "AmbiguousSupplierLink";
    pub const NO_SUPPLIER_LINK: &str = "NoSupplierLink";
    pub const MIRROR_TIMEOUT: &str = "MirrorTimeout";
}

impl BridgeError {
    pub fn kind(&self) -> &'static str {
        use BridgeError::*;
        match self {
            SupplierUnreachable(_) => "SupplierUnreachable",
            SupplierBlocked => "SupplierBlocked",
            SupplierBadCredentials => "SupplierBadCredentials",
            SupplierSessionExpired => "SupplierSessionExpired",
            SupplierParse { .. } => "SupplierParse",
            NoCurrentPrice { .. } => "NoCurrentPrice",
            LandauUnauthorized => "LandauUnauthorized",
            LandauForbidden(_) => "LandauForbidden",
            LandauUnreachable(_) => "LandauUnreachable",
            LandauRejected { .. } => "LandauRejected",
            BusinessNotApproved => "BusinessNotApproved",
            NoStore => "NoStore",
            ProductNotApproved(_) => "ProductNotApproved",
            DuplicateRow => "DuplicateRow",
            MirrorTimeout(_) => "MirrorTimeout",
            JobCancelled => "JobCancelled",
            JobNotFound(_) => "JobNotFound",
            JobBusy(_) => "JobBusy",
            SettingsIncomplete(_) => "SettingsIncomplete",
            Db(_) => "Db",
            KeyringUnavailable(_) => "KeyringUnavailable",
            Other(_) => "Other",
        }
    }

    /// What the seller reads. Keep these sentences actionable.
    pub fn message_fa(&self) -> String {
        use BridgeError::*;
        match self {
            SupplierUnreachable(d) => format!("اتصال به سایت بارزپخش برقرار نشد ({d}). اینترنت را بررسی کنید."),
            SupplierBlocked => "سایت بارزپخش این شبکه را مسدود کرده است. پروکسی یا VPN را خاموش کنید.".into(),
            SupplierBadCredentials => "نام کاربری یا کلمه عبور بارزپخش درست نیست.".into(),
            SupplierSessionExpired => "نشست بارزپخش منقضی شد و ورود دوباره هم ناموفق بود.".into(),
            SupplierParse { page, field } => format!("ساختار صفحه «{page}» تغییر کرده است (بخش «{field}» پیدا نشد). به‌روزرسانی برنامه لازم است."),
            NoCurrentPrice { code } => format!("برای کد {code} قیمت روز در بارزپخش پیدا نشد."),
            LandauUnauthorized => "کلید API لندو نامعتبر یا منقضی است. کلید تازه‌ای در لندو بسازید.".into(),
            LandauForbidden(d) => format!("لندو این درخواست را مجاز نمی‌داند ({d}). کلید باید از حساب فروشنده و از طریق درگاه لندو باشد."),
            LandauUnreachable(d) => format!("اتصال به لندو برقرار نشد ({d})."),
            LandauRejected { status, body } => format!("لندو درخواست را رد کرد (کد {status}): {body}"),
            BusinessNotApproved => "کسب‌وکار شما در لندو هنوز تأیید نشده است.".into(),
            NoStore => "هیچ فروشگاهی برای این حساب در لندو ثبت نشده است.".into(),
            ProductNotApproved(p) => format!("قطعه {p} در فهرست لندو تأیید نشده است."),
            DuplicateRow => "این قطعه با همین مشخصات از قبل روی قفسه است.".into(),
            MirrorTimeout(n) => format!("{n} ردیف هنوز در قفسه دیده نمی‌شود؛ قیمت‌گذاری آن‌ها در اجرای بعدی انجام می‌شود."),
            JobCancelled => "عملیات به درخواست شما متوقف شد.".into(),
            JobNotFound(id) => format!("عملیات شماره {id} پیدا نشد."),
            JobBusy(w) => format!("عملیات دیگری در حال اجراست: {w}"),
            SettingsIncomplete(w) => format!("تنظیمات ناقص است: {w}"),
            Db(d) => format!("خطای پایگاه داده: {d}"),
            KeyringUnavailable(d) => format!("ذخیره‌ساز امن سیستم در دسترس نیست ({d})؛ رمزها در پرونده برنامه نگه داشته می‌شوند."),
            Other(d) => d.clone(),
        }
    }
}

impl Serialize for BridgeError {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("BridgeError", 3)?;
        st.serialize_field("kind", self.kind())?;
        st.serialize_field("message", &self.message_fa())?;
        st.serialize_field("detail", &self.to_string())?;
        st.end()
    }
}

impl From<rusqlite::Error> for BridgeError {
    fn from(e: rusqlite::Error) -> Self {
        BridgeError::Db(e.to_string())
    }
}

impl From<serde_json::Error> for BridgeError {
    fn from(e: serde_json::Error) -> Self {
        BridgeError::Other(format!("json: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_kind_message_detail() {
        let e = BridgeError::NoCurrentPrice {
            code: "3137006".into(),
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "NoCurrentPrice");
        assert!(v["message"].as_str().unwrap().contains("3137006"));
        assert_eq!(v["detail"], "no current price for 3137006");
    }

    #[test]
    fn every_variant_has_a_persian_message() {
        for e in [
            BridgeError::SupplierBlocked,
            BridgeError::SupplierBadCredentials,
            BridgeError::LandauUnauthorized,
            BridgeError::MirrorTimeout(3),
        ] {
            assert!(
                e.message_fa()
                    .chars()
                    .any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)),
                "{}",
                e.kind()
            );
        }
    }
}
