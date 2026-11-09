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
    /// The data folder was written by a newer build (schema `found`); this one knows `supported`.
    #[error("database schema {found} is newer than {supported}")]
    DbNewer { found: i64, supported: i64 },
    #[error("keyring unavailable: {0}")]
    KeyringUnavailable(String),
    /// Another copy of the app holds this data folder (plan 130 R-m).
    #[error("another instance holds the data folder")]
    AlreadyRunning,
    /// The registry could not be read whole, so no link may be judged (plan 130 R-b).
    #[error("registry read incomplete: {read} of {total}")]
    RegistryIncomplete { read: usize, total: usize },
    /// A write Landau may have applied without its reply arriving, and the row does not show it
    /// yet: its intent stays open for the next job's reconciliation (plan 130 task 10).
    #[error("write unconfirmed: {0}")]
    WriteUnconfirmed(String),
    // Identity failures (plan 130 R-h): the row is held for the seller, nothing is written.
    /// The link's product is no longer in Landau's registry.
    #[error("stale link: product {product} is gone")]
    StaleLink { product: String },
    /// The write table did not let the link write; `reasons` is its JSON.
    #[error("link needs review: {reasons}")]
    NeedsReview { reasons: String },
    /// The evidence contradicts the link (a number, an opposite word, the brand).
    #[error("evidence conflicts with the link: {reasons}")]
    EvidenceConflict { reasons: String },
    /// Several links with different costs point at one shelf row (Q-3).
    #[error("{links} links with different costs on one row")]
    LinkConflict { links: usize },
    /// The link was changed after this job proved it.
    #[error("link changed since it was proven")]
    LinkChanged,
    /// The shelf row is not what this job planned against (or is gone).
    #[error("shelf row changed: {0}")]
    RowChanged(String),
    /// A seller's pick against a conflict, sent without confirming exactly these reasons (R-d).
    /// `reasons` is the write table's JSON; the screen shows them and asks again.
    #[error("{reasons}")]
    ConfirmRequired { reasons: String },
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
            DbNewer { .. } => "DbNewer",
            KeyringUnavailable(_) => "KeyringUnavailable",
            AlreadyRunning => "AlreadyRunning",
            RegistryIncomplete { .. } => "RegistryIncomplete",
            WriteUnconfirmed(_) => "WriteUnconfirmed",
            StaleLink { .. } => "StaleLink",
            NeedsReview { .. } => "NeedsReview",
            EvidenceConflict { .. } => "EvidenceConflict",
            LinkConflict { .. } => "Conflict",
            LinkChanged => "LinkChanged",
            RowChanged(_) => "RowChanged",
            ConfirmRequired { .. } => "ConfirmRequired",
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
            DbNewer { found, supported } => format!("این داده‌ها را نسخهٔ جدیدتری از برنامه ساخته است (نسخهٔ داده {found}، این برنامه تا {supported}). نسخهٔ جدیدتر را نصب کنید؛ پشتیبان پیش از ارتقا کنار پایگاه داده است."),
            KeyringUnavailable(_) => "ذخیره‌ساز امن سیستم در دسترس نیست؛ رمزها در پرونده برنامه نگه داشته می‌شوند.".into(),
            AlreadyRunning => "پل لاندا همین حالا باز است. همان پنجره را به کار ببرید؛ دو نسخهٔ هم‌زمان روی یک قفسه می‌نویسند.".into(),
            RegistryIncomplete { read, total } => format!("فهرست قطعه‌های لاندا کامل خوانده نشد ({read} از {total})؛ بدون فهرست کامل هیچ پیوندی سنجیده نمی‌شود و چیزی نوشته نشد. دوباره اجرا کنید."),
            WriteUnconfirmed(_) => "پاسخ لاندا به این نوشتن نرسید و ردیف هنوز دیده نمی‌شود؛ اجرای بعدی پیش از هر کاری ردیف را می‌خواند و نتیجه را روشن می‌کند. چیزی دوباره فرستاده نشد.".into(),
            StaleLink { .. } => "قطعهٔ لاندای این پیوند دیگر در فهرست لاندا نیست؛ پیوند را در «تطبیق‌ها» دوباره انتخاب کنید.".into(),
            NeedsReview { .. } => "شواهد این پیوند برای نوشتن خودکار کافی نیست؛ پیوند را بررسی و تأیید کنید.".into(),
            EvidenceConflict { .. } => "نام بارزپخش با قطعهٔ لاندای این پیوند جور نیست (عدد، جهت یا برند)؛ پیوند را بررسی کنید.".into(),
            LinkConflict { links } => format!("{links} نام بارزپخش با قیمت‌های متفاوت به یک ردیف قفسه وصل‌اند؛ تا یکی را انتخاب نکنید این ردیف نوشته نمی‌شود."),
            LinkChanged => "این پیوند در میانهٔ عملیات تغییر کرد؛ این ردیف در اجرای بعدی با پیوند تازه نوشته می‌شود.".into(),
            RowChanged(_) => "ردیف قفسه پس از خواندن تغییر کرد یا دیگر نیست؛ چیزی روی آن نوشته نشد.".into(),
            ConfirmRequired { .. } => "نام بارزپخش با این قطعه جور نیست؛ ناسازگاری‌ها و تغییر قیمت خرید را ببینید و دوباره تأیید کنید.".into(),
            Other(d) => d.clone(),
        }
    }
}

impl BridgeError {
    /// Identity failures hold a row for the seller; everything else that fails one item is a
    /// transfer failure. The report counts them apart (plan 130 R-h).
    pub fn is_identity(&self) -> bool {
        use BridgeError::*;
        matches!(
            self,
            StaleLink { .. }
                | NeedsReview { .. }
                | EvidenceConflict { .. }
                | LinkConflict { .. }
                | LinkChanged
                | RowChanged(_)
        )
    }
}

/// The identity kinds, for counting stored items (plan 130 R-h).
pub const IDENTITY_KINDS: &[&str] = &[
    "StaleLink",
    "NeedsReview",
    "EvidenceConflict",
    "Conflict",
    "LinkChanged",
    "RowChanged",
];

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
            BridgeError::AlreadyRunning,
            BridgeError::RegistryIncomplete {
                read: 52_800,
                total: 52_899,
            },
            BridgeError::WriteUnconfirmed("409 after a lost reply".into()),
            BridgeError::StaleLink {
                product: "6ac52427".into(),
            },
            BridgeError::NeedsReview {
                reasons: "[{\"kind\":\"TIED\"}]".into(),
            },
            BridgeError::EvidenceConflict {
                reasons: "[{\"kind\":\"NUMBER_CONFLICT\"}]".into(),
            },
            BridgeError::LinkConflict { links: 2 },
            BridgeError::LinkChanged,
            BridgeError::RowChanged("404".into()),
            BridgeError::ConfirmRequired {
                reasons: "[{\"kind\":\"NUMBER_CONFLICT\"}]".into(),
            },
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

    #[test]
    fn identity_failures_are_told_apart_from_transfer_failures() {
        let identity = [
            BridgeError::StaleLink {
                product: "p".into(),
            },
            BridgeError::NeedsReview {
                reasons: "[]".into(),
            },
            BridgeError::EvidenceConflict {
                reasons: "[]".into(),
            },
            BridgeError::LinkConflict { links: 2 },
            BridgeError::LinkChanged,
            BridgeError::RowChanged("gone".into()),
        ];
        for e in &identity {
            assert!(e.is_identity(), "{}", e.kind());
            assert!(IDENTITY_KINDS.contains(&e.kind()), "{}", e.kind());
        }
        for e in [
            BridgeError::LandauRejected {
                status: 500,
                body: String::new(),
            },
            BridgeError::LandauUnreachable("x".into()),
            BridgeError::NoCurrentPrice { code: "1".into() },
            BridgeError::WriteUnconfirmed("x".into()),
        ] {
            assert!(!e.is_identity(), "{}", e.kind());
            assert!(!IDENTITY_KINDS.contains(&e.kind()));
        }
    }
}
