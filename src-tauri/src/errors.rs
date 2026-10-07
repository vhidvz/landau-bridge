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
    /// The site kept answering its own error page («بروز خطا») for `path` through every retry —
    /// its failure, not a page whose layout changed.
    #[error("supplier answered its error page for {path}")]
    SupplierFaulted { path: String },
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
    /// The seller has no active pricing rule in Landau (spec §14 e): the job stops in preflight.
    #[error("no active pricing rule")]
    NoPricingRule,
    #[error("database: {0}")]
    Db(String),
    #[error("keyring unavailable: {0}")]
    KeyringUnavailable(String),
    #[error("{0}")]
    Other(String),
}

/// Warning kinds recorded in `errors`/`job_events` that are not failures of a call.
pub mod kinds {
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
    pub const BELOW_MIN_MARGIN: &str = "BelowMinMargin";
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
            SupplierFaulted { .. } => "SupplierFaulted",
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
            NoPricingRule => "NoPricingRule",
            Db(_) => "Db",
            KeyringUnavailable(_) => "KeyringUnavailable",
            Other(_) => "Other",
        }
    }

    /// What the seller reads. Keep these sentences actionable.
    pub fn message_fa(&self) -> String {
        use BridgeError::*;
        match self {
            SupplierUnreachable(_) => "اتصال به سایت بارزپخش برقرار نشد. اینترنت را بررسی کنید و اگر پروکسی یا فیلترشکن روشن است خاموش کنید.".into(),
            SupplierBlocked => "سایت بارزپخش این شبکه را مسدود کرده است. پروکسی یا فیلترشکن را خاموش کنید.".into(),
            SupplierBadCredentials => "نام کاربری یا کلمه عبور بارزپخش درست نیست.".into(),
            SupplierSessionExpired => "نشست بارزپخش منقضی شد و ورود دوباره هم ناموفق بود.".into(),
            SupplierFaulted { .. } => "سایت بارزپخش در چند تلاش پیاپی برای این صفحه پیام «بروز خطا» داد. این مورد در اجرای بعدی دوباره خوانده می‌شود.".into(),
            SupplierParse { page, field } => format!("ساختار صفحه «{page}» تغییر کرده است (بخش «{field}» پیدا نشد). به‌روزرسانی برنامه لازم است."),
            NoCurrentPrice { code } => format!("برای کد {code} قیمت روز در بارزپخش پیدا نشد."),
            LandauUnauthorized => "کلید دسترسی لاندا نامعتبر یا منقضی است. کلید تازه‌ای در لاندا بسازید.".into(),
            LandauForbidden(_) => "لاندا این درخواست را مجاز نمی‌داند. کلید باید از حساب فروشنده باشد و از طریق درگاه لاندا استفاده شود.".into(),
            LandauUnreachable(_) => "اتصال به لاندا برقرار نشد. نشانی درگاه و اینترنت را بررسی کنید.".into(),
            LandauRejected { status, .. } => format!("لاندا درخواست را رد کرد (کد {status}). جزئیات در گزارش خطاها ثبت شد."),
            BusinessNotApproved => "کسب‌وکار شما در لاندا هنوز تأیید نشده است.".into(),
            NoStore => "هیچ فروشگاهی برای این حساب در لاندا ثبت نشده است.".into(),
            ProductNotApproved(p) => format!("قطعه {p} در فهرست لاندا تأیید نشده است."),
            DuplicateRow => "این قطعه با همین مشخصات از قبل روی قفسه است.".into(),
            MirrorTimeout(n) => format!("{n} ردیف هنوز در قفسه دیده نمی‌شود؛ قیمت‌گذاری آن‌ها در اجرای بعدی انجام می‌شود."),
            JobCancelled => "عملیات به درخواست شما متوقف شد.".into(),
            JobNotFound(id) => format!("عملیات شماره {id} پیدا نشد."),
            JobBusy(w) => format!("عملیات دیگری در حال اجراست: {w}"),
            SettingsIncomplete(w) => format!("تنظیمات ناقص است: {w}"),
            NoPricingRule => "در لاندا هیچ قاعدهٔ قیمت‌گذاری فعالی ندارید. اول یک قاعده بسازید تا قفسه بدون سود نماند.".into(),
            Db(_) => "خطای پایگاه داده برنامه. جزئیات در گزارش خطاها ثبت شد.".into(),
            KeyringUnavailable(_) => "ذخیره‌ساز امن سیستم در دسترس نیست؛ رمزها در پرونده برنامه نگه داشته می‌شوند.".into(),
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
    fn messages_are_persian_and_carry_no_raw_detail() {
        let cases = [
            BridgeError::SupplierUnreachable("error sending request for url".into()),
            BridgeError::SupplierBlocked,
            BridgeError::SupplierBadCredentials,
            BridgeError::SupplierSessionExpired,
            BridgeError::SupplierFaulted {
                path: "/Shop/Preview/Factor/13087656/11750".into(),
            },
            BridgeError::LandauUnauthorized,
            BridgeError::LandauForbidden("Forbidden resource".into()),
            BridgeError::LandauUnreachable("connection refused".into()),
            BridgeError::LandauRejected {
                status: 400,
                body: "limit must not be greater than 100".into(),
            },
            BridgeError::BusinessNotApproved,
            BridgeError::NoStore,
            BridgeError::DuplicateRow,
            BridgeError::MirrorTimeout(3),
            BridgeError::JobCancelled,
            BridgeError::Db("no such table: invoices".into()),
            BridgeError::KeyringUnavailable("Platform secure storage failure".into()),
        ];
        for e in cases {
            let m = e.message_fa();
            assert!(
                m.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)),
                "{} has no Persian",
                e.kind()
            );
            assert!(
                !m.chars().any(|c| c.is_ascii_alphabetic()),
                "{} leaks raw detail: {m}",
                e.kind()
            );
            assert_eq!(
                serde_json::to_value(&e).unwrap()["detail"],
                e.to_string(),
                "{} keeps the raw detail in `detail`",
                e.kind()
            );
        }
    }
}
