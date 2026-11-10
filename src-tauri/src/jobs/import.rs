//! Feature 1 (spec §7.1): preflight → fetch → aggregate → match → [REVIEW] → price → write →
//! reprice → report. `start` runs up to the review pause; `commit` runs the rest.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use futures_util::stream::{FuturesUnordered, StreamExt};
use serde_json::json;

use super::planner::aggregate_carrying;
use super::proof::{
    held_error, plan_import, ImportCandidate, Judgement, OtherLink, Planned, Prover,
};
use super::{
    fatal, forget_waiting_rename, held, i, job_registry, judge_rename, opt_i, preflight, quote_for,
    read_registry, report, reprice_items, t, JobCtx, Rename, Services, VatMismatches,
    RENAME_ASK_NOTE, RENAME_CONFIRMED_NOTE, RENAME_HELD_NOTE,
};
use crate::db::jobs::JobItemRow;
use crate::db::matches::{actor, state, LinkProduct, LinkPut};
use crate::db::Db;
use crate::domain::{Authenticity, ImportMode};
use crate::errors::{kinds, BridgeError, Result, IDENTITY_KINDS};
use crate::inspect::PickJudgement;
use crate::landau::writer::{ImportWrite, ShelfWriter};
use crate::matching::gate::Reason;
use crate::settings::SUPPLIER_ID;
use crate::supplier::Progress;

#[derive(Debug, Clone)]
pub struct ImportParams {
    pub mode: ImportMode,
    pub since: Option<String>,
    /// Ask the supplier only for invoices dated on/after `since` (quantity mode); otherwise the
    /// whole ledger is listed and cached, as a zero-mode run always does.
    pub fetch_since_only: bool,
    /// At most this many not-yet-fetched invoices this run (`None` = all). Invoices are cached, so
    /// a capped run simply continues next time; the live e2e test uses it to stay short.
    pub fetch_limit: Option<usize>,
}

impl ImportParams {
    /// The date the ledger request is bounded to, if any.
    pub fn ledger_from(&self) -> Option<&str> {
        match self.mode {
            ImportMode::Quantity if self.fetch_since_only => self.since.as_deref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Paused: this many items need the seller.
    Review(usize),
    Done,
}

pub async fn start(ctx: &JobCtx, svc: &Services, params: &ImportParams) -> Result<Outcome> {
    let (_, _, store) = svc.settings.require_landau()?;
    let store = store.to_string();
    preflight(ctx, svc, &store).await?;
    // R-o: an earlier write whose outcome is unknown is settled before anything is aggregated —
    // a landed one marks its invoice lines, so they cannot be added to the shelf twice
    super::reconcile::reconcile(ctx, &svc.landau, &store).await?;
    fetch(ctx, svc, params.ledger_from(), params.fetch_limit).await?;
    aggregate_phase(ctx, params)?;
    let pending = match_phase(ctx, svc).await?;
    if pending > 0 {
        ctx.db.set_job_status(ctx.job_id, "REVIEW", "REVIEW")?;
        ctx.info(
            "REVIEW",
            &format!("{pending} item(s) await the seller"),
            None,
        );
        super::emit_event(
            &ctx.app,
            super::EVENT_DONE,
            json!({"job_id": ctx.job_id, "status": "REVIEW"}),
        );
        return Ok(Outcome::Review(pending));
    }
    commit(ctx, svc).await?;
    Ok(Outcome::Done)
}

/// Ledger headers (from `from` on, or all of them) → unseen invoices' lines in that range.
/// Per-invoice parse failures are recorded and skipped; session/transport failures stop the job.
async fn fetch(
    ctx: &JobCtx,
    svc: &Services,
    from: Option<&str>,
    limit: Option<usize>,
) -> Result<()> {
    ctx.phase("FETCH")?;
    let progress_ctx = (ctx.app.clone(), ctx.job_id);
    let progress: Progress = std::sync::Arc::new(move |done, total| {
        super::emit_event(
            &progress_ctx.0,
            super::EVENT_PROGRESS,
            super::Progress {
                job_id: progress_ctx.1,
                phase: "FETCH".into(),
                done,
                total,
                message: "فهرست فاکتورها".into(),
            },
        );
    });
    let headers = svc.supplier.ledger(from, progress).await?;
    ctx.check_cancel()?;
    ctx.db.upsert_invoices(SUPPLIER_ID, &headers)?;
    let unpaid = headers.iter().filter(|h| !h.paid).count();
    ctx.info(
        "FETCH",
        &format!("{} ledger row(s), {unpaid} unpaid", headers.len()),
        None,
    );
    let mut todo = ctx.db.unfetched_invoice_ids(SUPPLIER_ID, from)?;
    if let Some(limit) = limit {
        if todo.len() > limit {
            ctx.info(
                "FETCH",
                &format!(
                    "fetch capped at {limit} of {} pending invoice(s)",
                    todo.len()
                ),
                None,
            );
            todo.truncate(limit);
        }
    }
    // An invoice that fails on its own (the site's error page past every retry, a page that did not
    // parse) is not reported yet: it gets one more pass once the rest are in, minutes or hours
    // later, and only what fails twice reaches the report. A page the site served badly once is
    // not a layout change (2026-10-07: four such reports, each invoice fine on the next fetch).
    let again = fetch_all(ctx, svc, todo, "جزئیات فاکتورها").await?;
    for (invoice_id, e) in again.iter() {
        tracing::warn!(job = ctx.job_id, invoice_id, error = %e, "invoice fetch failed; trying again at the end of the pass");
    }
    let again = again.into_iter().map(|(id, _)| id).collect();
    for (invoice_id, e) in fetch_all(ctx, svc, again, "تلاش دوباره برای فاکتورهای ناموفق").await?
    {
        ctx.warn(
            "FETCH",
            e.kind(),
            &format!(
                "فاکتور {}: {}",
                ctx.db.invoice_url(invoice_id)?,
                e.message_fa()
            ),
            Some(json!({"invoice_id": invoice_id})),
        );
    }
    Ok(())
}

/// Fetches `ids` with up to [`crate::supplier::Supplier::parallelism`] in flight — the supplier
/// decides how many of those actually run at once — and returns the invoices that failed on their
/// own. A cancel starts nothing new and lets what is in flight land (as the one-at-a-time loop
/// let its current invoice land); a fatal failure stops at once, and whatever was in flight is
/// fetched by the next run, since only a stored invoice counts as fetched.
async fn fetch_all(
    ctx: &JobCtx,
    svc: &Services,
    ids: Vec<i64>,
    message: &str,
) -> Result<Vec<(i64, BridgeError)>> {
    let parallel = svc.supplier.parallelism().max(1);
    let total = ids.len() as u32;
    let mut ids = ids.into_iter();
    let mut running = FuturesUnordered::new();
    let (mut failed, mut done, mut cancelled) = (Vec::new(), 0, false);
    loop {
        while !cancelled && running.len() < parallel {
            let Some(id) = ids.next() else { break };
            if ctx.check_cancel().is_err() {
                cancelled = true;
                break;
            }
            running.push(async move { (id, fetch_lines(ctx, svc, id).await) });
        }
        let Some((id, outcome)) = running.next().await else {
            break;
        };
        if let Some(e) = outcome? {
            failed.push((id, e));
        }
        done += 1;
        ctx.progress("FETCH", done, total, message);
    }
    ctx.check_cancel()?;
    Ok(failed)
}

/// One invoice's lines into the cache. `Ok(Some(e))` is a failure of that invoice alone; `Err` is
/// one that stops the job ([`fatal`], or the database).
async fn fetch_lines(ctx: &JobCtx, svc: &Services, invoice_id: i64) -> Result<Option<BridgeError>> {
    let url = ctx.db.invoice_url(invoice_id)?;
    match svc.supplier.invoice_lines(&url).await {
        Ok(lines) => {
            ctx.db.replace_lines(invoice_id, &lines)?;
            Ok(None)
        }
        Err(e) if fatal(&e) => Err(e),
        Err(e) => Ok(Some(e)),
    }
}

fn aggregate_phase(ctx: &JobCtx, params: &ImportParams) -> Result<()> {
    ctx.phase("AGGREGATE")?;
    let lines = ctx.db.lines_with_invoices(SUPPLIER_ID)?;
    let already = ctx.db.imported_lines()?;
    let carried = ctx.db.aggregated_lines(ctx.job_id)?;
    let agg = aggregate_carrying(
        &lines,
        params.mode,
        params.since.as_deref(),
        &already,
        &carried,
    );
    if agg.carried > 0 {
        ctx.info(
            "AGGREGATE",
            &format!(
                "{} ردیف فاکتور که اجرای قبلی نگه داشته بود این بار هم شمرده شد، هرچند تاریخش پیش از «از تاریخ» است: {}",
                agg.carried,
                agg.carried_codes.join("، ")
            ),
            Some(json!({"lines": agg.carried, "codes": agg.carried_codes})),
        );
    }
    if let Some(first) = agg.left_before_since.iter().min() {
        ctx.warn(
            "AGGREGATE",
            kinds::LINES_BEFORE_SINCE,
            &format!(
                "{} ردیف فاکتور از اجرای نیمه‌تمام قبلی پیش از «از تاریخ» است و منتقل نشد؛ قدیمی‌ترینِ آن‌ها {first} است. «از تاریخ» زودتر هر ردیف دیگری از همان تاریخ به بعد را هم منتقل می‌کند.",
                agg.left_before_since.len()
            ),
            Some(json!({"lines": agg.left_before_since.len(), "from": first})),
        );
    }
    if !agg.skipped_unpaid.is_empty() {
        ctx.warn(
            "AGGREGATE",
            kinds::INVOICE_UNPAID,
            &format!(
                "{} فاکتور پرداخت‌نشده نادیده گرفته شد.",
                agg.skipped_unpaid.len()
            ),
            Some(json!({"invoice_ids": agg.skipped_unpaid})),
        );
    }
    if !agg.skipped_imported.is_empty() {
        ctx.warn(
            "AGGREGATE",
            kinds::INVOICE_ALREADY_IMPORTED,
            &format!(
                "{} فاکتور قبلاً منتقل شده بود و دوباره شمرده نشد.",
                agg.skipped_imported.len()
            ),
            Some(json!({"invoice_ids": agg.skipped_imported})),
        );
    }
    if agg.non_positive_lines > 0 {
        ctx.warn(
            "AGGREGATE",
            kinds::LINE_NON_POSITIVE_QTY,
            &format!(
                "{} ردیف با تعداد صفر یا منفی نادیده گرفته شد.",
                agg.non_positive_lines
            ),
            None,
        );
    }
    let mut invoice_ids: HashSet<i64> = HashSet::new();
    for item in &agg.items {
        let id = ctx
            .db
            .add_job_item(ctx.job_id, &item.code, &item.name, item.qty)?;
        ctx.item(
            id,
            &[(
                "invoice_ids_json",
                t(&serde_json::to_string(&item.invoice_ids)?),
            )],
        )?;
        invoice_ids.extend(item.invoice_ids.iter().copied());
    }
    let mut ids: Vec<i64> = invoice_ids.into_iter().collect();
    ids.sort_unstable();
    ctx.db.set_job_summary(
        ctx.job_id,
        &json!({"invoice_ids": ids, "items": agg.items.len()}),
    )?;
    ctx.info(
        "AGGREGATE",
        &format!(
            "{} distinct part(s) from {} invoice(s)",
            agg.items.len(),
            ids.len()
        ),
        None,
    );
    Ok(())
}

/// MATCH with VALIDATE (plan 130 R-b): the registry read whole, then every item's stored link
/// judged against it — a dead product rebuilt only on its exact definition — or a fresh automatic
/// match through the gate. Returns how many items need review.
async fn match_phase(ctx: &JobCtx, svc: &Services) -> Result<usize> {
    ctx.phase("MATCH")?;
    let registry = read_registry(ctx, svc, "MATCH").await?;
    let prover = Prover {
        db: &ctx.db,
        registry: &registry,
        supplier: SUPPLIER_ID,
        job: ctx.job_id,
        threshold: svc.settings.threshold,
    };
    let items = ctx.db.job_items(ctx.job_id)?;
    let total = items.len() as u32;
    let mut pending = 0usize;
    for (n, item) in items.iter().enumerate() {
        ctx.check_cancel()?;
        match prover.item(&item.name, &item.code).await? {
            Judgement::Proven {
                key,
                version,
                product,
            } => {
                ctx.item(
                    item.id,
                    &[
                        ("product_id", t(&product.id)),
                        ("product_name", t(&product.name)),
                        ("link_key", t(&key)),
                        ("link_version", i(version)),
                        ("status", t("PLANNED")),
                    ],
                )?;
            }
            Judgement::NotInLandau => {
                ctx.item(
                    item.id,
                    &[
                        ("status", t("SKIPPED")),
                        ("error_kind", t(kinds::NONE_DECISION)),
                        ("error_msg", t("این قطعه در لاندا نیست (تصمیم قبلی).")),
                    ],
                )?;
            }
            Judgement::Held {
                reasons,
                error,
                candidates,
                link_key,
                link_product,
            } => {
                pending += 1;
                let mut patch = vec![
                    ("status", t("REVIEW")),
                    ("candidates_json", t(&serde_json::to_string(&candidates)?)),
                    ("reasons_json", t(&serde_json::to_string(&reasons)?)),
                ];
                if let Some(k) = &link_key {
                    patch.push(("link_key", t(k)));
                }
                if let Some(p) = &link_product {
                    patch.push(("link_product", t(p)));
                }
                if let Some(e) = &error {
                    patch.push(("error_kind", t(e.kind())));
                    patch.push(("error_msg", t(&e.message_fa())));
                }
                ctx.item(item.id, &patch)?;
            }
        }
        ctx.progress("MATCH", n as u32 + 1, total, "تطبیق با فهرست لاندا");
    }
    if pending > 0 {
        ctx.warn(
            "MATCH",
            kinds::UNMATCHED,
            &format!("{pending} قطعه نیاز به تأیید شما دارد."),
            None,
        );
    }
    Ok(pending)
}

/// The seller's pick in review (spec §7.1; plan 130 R-a, R-b, R-d): a `USER` link to the product,
/// proven for this job, with the definition the job's registry snapshot holds for it. A pick
/// against a conflict is refused unless exactly its reasons are confirmed
/// ([`crate::inspect::confirmation`]); what was confirmed is kept with the link, so a later job
/// does not ask again about the same conflict on the same definition. The item is planned from
/// that link at its new version.
pub fn pick(
    db: &Db,
    item: &JobItemRow,
    judged: &PickJudgement,
    confirmed: Option<&[Reason]>,
    authenticity: Option<Authenticity>,
) -> Result<()> {
    same_item(judged, &item.name)?;
    let confirmed = crate::inspect::confirmation(&judged.verdict, confirmed)?;
    // the item and its job may have moved on while the pick waited (a registry read takes ~90 s):
    // a pick lands only while both still wait in review
    let open = db.job_item(item.id)?.is_some_and(|i| i.status == "REVIEW")
        && db.job(item.job_id)?.is_some_and(|j| j.status == "REVIEW");
    if !open {
        return Err(BridgeError::Other(
            "این قلم دیگر در انتظار تصمیم نیست.".into(),
        ));
    }
    let p = &judged.product;
    let version = db.put_link(
        SUPPLIER_ID,
        &LinkPut {
            name_seen: &item.name,
            code: &item.code,
            product: Some(LinkProduct {
                id: &p.id,
                name: &p.name,
                brand: Some(p.brand.as_deref().unwrap_or("")),
                alias: Some(p.alias.as_deref().unwrap_or("")),
            }),
            how: "USER",
            similarity: Some(judged.similarity as i64),
            state: state::VALID,
            state_reason: None,
            validated_job: Some(item.job_id),
            actor: actor::SELLER,
            evidence: Some("shown"),
            job_id: Some(item.job_id),
            decided_at: None,
            priced_at: None,
            confirmed: confirmed_json(&confirmed)?,
        },
    )?;
    let mut patch = vec![
        ("product_id", t(&p.id)),
        ("product_name", t(&p.name)),
        (
            "link_key",
            t(&crate::matching::normalize::name_key(&item.name)),
        ),
        ("link_version", i(version)),
        ("link_product", rusqlite::types::Value::Null),
        ("error_kind", rusqlite::types::Value::Null),
        ("error_msg", rusqlite::types::Value::Null),
        ("status", t("PLANNED")),
    ];
    if let Some(a) = authenticity {
        patch.push(("authenticity", t(a.as_str())));
    }
    db.update_job_item(item.id, &patch)
}

/// The seller's pick on the Matches screen (R-d, slice 4): refused against a conflict unless exactly
/// its reasons are confirmed, as in review, and recorded with the snapshot's definition so a
/// re-import rebuilds it. It proves nothing yet: the next job's VALIDATE judges it like any stored
/// pick, honouring what was confirmed.
pub fn matches_pick(
    db: &Db,
    name_seen: &str,
    code: &str,
    judged: &PickJudgement,
    confirmed: Option<&[Reason]>,
) -> Result<()> {
    same_item(judged, name_seen)?;
    let confirmed = crate::inspect::confirmation(&judged.verdict, confirmed)?;
    let p = &judged.product;
    db.put_link(
        SUPPLIER_ID,
        &LinkPut {
            name_seen,
            code,
            product: Some(LinkProduct {
                id: &p.id,
                name: &p.name,
                brand: Some(p.brand.as_deref().unwrap_or("")),
                alias: Some(p.alias.as_deref().unwrap_or("")),
            }),
            how: "USER",
            similarity: Some(judged.similarity as i64),
            state: state::UNVERIFIED,
            state_reason: None,
            validated_job: None,
            actor: actor::SELLER,
            evidence: Some("shown"),
            job_id: None,
            decided_at: None,
            priced_at: None,
            confirmed: confirmed_json(&confirmed)?,
        },
    )
    .map(|_| ())
}

/// The seller's word on the Matches screen that the name the site now lists `code` under is the
/// link's product (PI-28, R-d's mechanism). The core judges that name against the product in the
/// kept registry snapshot, as PRICE does (`gate::conflicts`, Q-5), and refuses `ConfirmRequired`
/// unless exactly those reasons come back confirmed. Refused too when the link moved since the
/// screen read it (`version`, `product_id`), when the rename it records is another code or name (a
/// job met a newer one since), or the snapshot lacks the product. A name that agrees needs no word:
/// a waiting record of it is forgotten, and `false` says nothing was confirmed. A word for a record a
/// job forgot since (PI-30) lands as what the seller was shown: that code under that name.
#[allow(clippy::too_many_arguments)]
pub fn confirm_rename(
    db: &Db,
    registry: &crate::matching::registry::Registry,
    name_seen: &str,
    version: i64,
    product_id: &str,
    code: &str,
    site_name: &str,
    confirmed: Option<&[Reason]>,
) -> Result<bool> {
    let moved = || BridgeError::Other(LINK_MOVED.into());
    let link = db
        .match_for_name(SUPPLIER_ID, name_seen)?
        .ok_or_else(moved)?;
    if link.link_version != version || link.product_id.as_deref() != Some(product_id) {
        return Err(moved());
    }
    // the word is for the rename the screen showed: a job that met another code or name since asks
    // again (slice 9's review, finding 2)
    let same = |r: &crate::db::matches::Renamed| {
        r.code == code
            && crate::matching::normalize::name_key(&r.name)
                == crate::matching::normalize::name_key(site_name)
    };
    if link.renamed().is_some_and(|r| !same(&r)) {
        return Err(moved());
    }
    let product = registry
        .get(product_id)
        .ok_or_else(|| BridgeError::Other(crate::inspect::NOT_IN_SNAPSHOT.into()))?;
    let ev = crate::matching::evidence::evidence(site_name, code, product, registry.vocabulary());
    let reasons = crate::matching::gate::conflicts(&ev);
    if reasons.is_empty() {
        if link.renamed().is_some_and(|r| r.confirmed_at.is_none()) {
            db.forget_rename(
                SUPPLIER_ID,
                &link.name_key,
                version,
                link.renamed_json.as_deref(),
            )?;
        }
        return Ok(false);
    }
    let confirmed = crate::inspect::confirmation(
        &crate::matching::gate::Verdict::ConfirmThenWrite(reasons),
        confirmed,
    )?;
    if !db.confirm_rename(
        SUPPLIER_ID,
        &link.name_key,
        version,
        product_id,
        code,
        site_name,
        &confirmed,
    )? {
        return Err(moved());
    }
    Ok(true)
}

/// The seller takes back a rename on the Matches screen (PI-28): the link as the screen read it
/// (`version`) forgets it, and the next run holds the code again and asks.
pub fn withdraw_rename(db: &Db, name_seen: &str, version: i64) -> Result<()> {
    let key = crate::matching::normalize::name_key(name_seen);
    if !db.withdraw_rename(SUPPLIER_ID, &key, version)? {
        return Err(BridgeError::Other(LINK_MOVED.into()));
    }
    Ok(())
}

/// A link edited elsewhere since the screen read it: the seller reopens it.
pub const LINK_MOVED: &str = "این تطبیق پس از باز شدن صفحه عوض شد؛ دوباره بازش کنید.";

/// The link a planned item writes from, as the job planned it: `None` when the seller re-pointed
/// it since (another version or product) — its write is refused then, and nothing about a rename
/// is noted on the seller's new decision.
pub(super) fn item_link(
    ctx: &JobCtx,
    item: &JobItemRow,
) -> Result<Option<crate::db::matches::MatchRow>> {
    let Some(key) = item.link_key.as_deref() else {
        return Ok(None);
    };
    Ok(ctx
        .db
        .link_by_key(SUPPLIER_ID, key)?
        .filter(|l| Some(l.link_version) == item.link_version && l.product_id == item.product_id))
}

/// A judgement made for another supplier item never decides this one.
fn same_item(judged: &PickJudgement, name: &str) -> Result<()> {
    use crate::matching::normalize::name_key;
    if name_key(&judged.name) != name_key(name) {
        return Err(BridgeError::Other(format!(
            "pick judged for «{}», not «{name}»",
            judged.name
        )));
    }
    Ok(())
}

/// What a link records of a confirmation: the reasons' JSON, or nothing when none was needed.
pub fn confirmed_json(confirmed: &[Reason]) -> Result<Option<String>> {
    Ok(if confirmed.is_empty() {
        None
    } else {
        Some(serde_json::to_string(confirmed)?)
    })
}

/// The other stored links on the products a job writes (PI-22), weighed for [`plan_import`].
#[derive(Debug, Default)]
struct Others {
    /// product id → its links no item of this job carries.
    links: BTreeMap<String, Vec<OtherLink>>,
    /// product id → a note naming each such link and what it came to, for the held rows.
    notes: BTreeMap<String, String>,
    /// Products one of those links is quoted unavailable for.
    unavailable: HashSet<String>,
}

/// Every stored link on a product this job is about to write that no item of the job carries
/// (PI-22) — weighed as REFRESH weighs a row's links (Q-3, R-e): judged by VALIDATE against the
/// registry snapshot unless this job judged it already, then quoted if it proves. An unproven one
/// is passed on unpriced, so [`plan_import`] holds the row without a quote; a quote that fails in
/// transfer fails that product's items here, and they leave `candidates`. A link on a product the
/// registry no longer holds is not on the target product and is weighed by the job that rebuilds it.
async fn other_links(
    ctx: &JobCtx,
    svc: &Services,
    candidates: &mut Vec<ImportCandidate>,
    all: &[JobItemRow],
    vat: &mut VatMismatches,
) -> Result<Others> {
    let carried: HashSet<&str> = all.iter().filter_map(|i| i.link_key.as_deref()).collect();
    let products: BTreeSet<String> = candidates.iter().map(|c| c.product.clone()).collect();
    let mut found: Vec<(String, Vec<crate::db::matches::MatchRow>)> = Vec::new();
    for product in products {
        let links: Vec<_> = ctx
            .db
            .links_on_product(SUPPLIER_ID, &product)?
            .into_iter()
            .filter(|l| !carried.contains(l.name_key.as_str()))
            .collect();
        if !links.is_empty() {
            found.push((product, links));
        }
    }
    let mut out = Others::default();
    if found.is_empty() {
        return Ok(out);
    }
    let registry = job_registry(ctx, svc).await?;
    let prover = Prover {
        db: &ctx.db,
        registry: &registry,
        supplier: SUPPLIER_ID,
        job: ctx.job_id,
        threshold: svc.settings.threshold,
    };
    'products: for (product, links) in found {
        let mut weighed = Vec::new();
        let mut said = Vec::new();
        for link in links {
            ctx.check_cancel()?;
            if link.validated_job != Some(ctx.job_id) {
                prover.stored(&link, &link.name_seen, &link.code).await?;
            }
            let now = ctx.db.link_by_key(SUPPLIER_ID, &link.name_key)?;
            let Some(now) = now.filter(|l| l.writable_for(ctx.job_id, l.link_version, &product))
            else {
                said.push(format!("«{}» اثبات‌نشده", link.name_seen));
                weighed.push(OtherLink {
                    key: link.name_key.clone(),
                    version: link.link_version,
                    cost: None,
                });
                continue;
            };
            let cost = match quote_for(ctx, svc, &now.name_seen, &now.code, vat).await {
                Ok(Some(c)) => match judge_rename(&ctx.db, &registry, Some(&now), &product, &c)? {
                    Rename::Holds { asks_seller, .. } => {
                        let ask = if asks_seller {
                            format!(" — {RENAME_ASK_NOTE}")
                        } else {
                            String::new()
                        };
                        said.push(format!(
                            "«{}» نام تازه‌اش جور نیست: «{}»{ask}",
                            now.name_seen, c.name
                        ));
                        None
                    }
                    rename => {
                        if !c.available {
                            out.unavailable.insert(product.clone());
                        }
                        let confirmed = if rename == Rename::Confirmed {
                            format!("، نام تازه «{}» را فروشنده تأیید کرده", c.name)
                        } else {
                            String::new()
                        };
                        said.push(format!("«{}» {} ریال{confirmed}", now.name_seen, c.cost));
                        Some(c.cost)
                    }
                },
                Ok(None) => {
                    said.push(format!("«{}» بی‌قیمت", now.name_seen));
                    None
                }
                Err(e) if fatal(&e) => return Err(e),
                Err(e) => {
                    let ids: Vec<i64> = candidates
                        .iter()
                        .filter(|c| c.product == product)
                        .map(|c| c.item_id)
                        .collect();
                    held(ctx, &ids, &e)?;
                    candidates.retain(|c| c.product != product);
                    continue 'products;
                }
            };
            weighed.push(OtherLink {
                key: now.name_key.clone(),
                version: now.link_version,
                cost,
            });
        }
        out.notes.insert(
            product.clone(),
            format!(
                "پیوندهای دیگر همین قطعه که در این اجرا نبودند: {}",
                said.join(" | ")
            ),
        );
        out.links.insert(product, weighed);
    }
    Ok(out)
}

/// Price → write → reprice → report over every item that is PLANNED with a product.
pub async fn commit(ctx: &JobCtx, svc: &Services) -> Result<()> {
    let (_, _, store) = svc.settings.require_landau()?;
    let store = store.to_string();
    let job = ctx
        .db
        .job(ctx.job_id)?
        .ok_or(BridgeError::JobNotFound(ctx.job_id))?;
    let mode = job
        .mode
        .as_deref()
        .and_then(ImportMode::parse)
        .unwrap_or(ImportMode::Zero);
    ctx.db.set_job_status(ctx.job_id, "RUNNING", "PRICE")?;

    // deferred review items are left out of this run; a refused stored link keeps its own kind,
    // so the report counts it as held (plan 130 R-h)
    for item in ctx
        .db
        .job_items(ctx.job_id)?
        .iter()
        .filter(|i| i.status == "REVIEW")
    {
        let identity = item
            .error_kind
            .as_deref()
            .is_some_and(|k| IDENTITY_KINDS.contains(&k));
        if identity {
            ctx.item(item.id, &[("status", t("SKIPPED"))])?;
        } else {
            ctx.item(
                item.id,
                &[
                    ("status", t("SKIPPED")),
                    ("error_kind", t(kinds::UNMATCHED)),
                    (
                        "error_msg",
                        t("بدون تصمیم؛ در اجرای بعدی دوباره پرسیده می‌شود."),
                    ),
                ],
            )?;
        }
    }

    ctx.phase("PRICE")?;
    let items: Vec<_> = ctx
        .db
        .job_items(ctx.job_id)?
        .into_iter()
        .filter(|i| i.status == "PLANNED" && i.product_id.is_some())
        .collect();
    let total = items.len() as u32;
    let mut vat = VatMismatches::default();
    let mut registry = None;
    for (n, item) in items.iter().enumerate() {
        ctx.check_cancel()?;
        match quote_for(ctx, svc, &item.name, &item.code, &mut vat).await {
            Ok(Some(c)) if c.renamed => {
                // PI-26: the code found another name — judged against the product before its cost
                // may stand for this link; PI-28: unless the seller confirmed that very rename
                if registry.is_none() {
                    registry = Some(job_registry(ctx, svc).await?);
                }
                let product = item.product_id.as_deref().unwrap_or("");
                let link = item_link(ctx, item)?;
                let judged = match registry.as_deref() {
                    Some(r) => judge_rename(&ctx.db, r, link.as_ref(), product, &c)?,
                    None => Rename::Holds {
                        reasons: vec![Reason::DefinitionNotExact],
                        asks_seller: false,
                    },
                };
                ctx.db
                    .add_item_note(item.id, &format!("نام در بارزپخش عوض شد: «{}»", c.name))?;
                match judged {
                    Rename::Holds {
                        reasons,
                        asks_seller,
                    } => {
                        ctx.db.add_item_note(item.id, RENAME_HELD_NOTE)?;
                        if asks_seller {
                            ctx.db.add_item_note(item.id, RENAME_ASK_NOTE)?;
                        }
                        held(ctx, &[item.id], &held_error(&reasons))?;
                    }
                    rename => {
                        if rename == Rename::Confirmed {
                            ctx.db.add_item_note(item.id, RENAME_CONFIRMED_NOTE)?;
                        }
                        if !item.code.is_empty() && c.code != item.code {
                            ctx.db.add_item_note(
                                item.id,
                                &format!("کد بارزپخش عوض شد: {} ← {}", item.code, c.code),
                            )?;
                        }
                        if !c.available {
                            ctx.db.add_item_note(item.id, "ناموجود در بارزپخش")?;
                        }
                        ctx.item(
                            item.id,
                            &[("cost", i(c.cost)), ("available", i(c.available as i64))],
                        )?
                    }
                }
            }
            Ok(Some(c)) => {
                // found under the invoice's name: a rename waiting for the seller is gone (PI-28)
                forget_waiting_rename(&ctx.db, item_link(ctx, item)?.as_ref(), &c)?;
                if !item.code.is_empty() && c.code != item.code {
                    ctx.db.add_item_note(
                        item.id,
                        &format!("کد بارزپخش عوض شد: {} ← {}", item.code, c.code),
                    )?;
                }
                if !c.available {
                    ctx.db.add_item_note(item.id, "ناموجود در بارزپخش")?;
                }
                ctx.item(
                    item.id,
                    &[("cost", i(c.cost)), ("available", i(c.available as i64))],
                )?
            }
            Ok(None) => {
                let e = BridgeError::NoCurrentPrice {
                    code: if item.code.is_empty() {
                        item.name.clone()
                    } else {
                        item.code.clone()
                    },
                };
                ctx.error("PRICE", &e, Some(json!({"item_id": item.id})));
                ctx.item(
                    item.id,
                    &[
                        ("status", t("SKIPPED")),
                        ("error_kind", t(e.kind())),
                        ("error_msg", t(&e.message_fa())),
                    ],
                )?;
            }
            Err(e) if fatal(&e) => return Err(e),
            Err(e) => {
                ctx.error("PRICE", &e, Some(json!({"item_id": item.id})));
                ctx.item(
                    item.id,
                    &[
                        ("status", t("FAILED")),
                        ("error_kind", t(e.kind())),
                        ("error_msg", t(&e.message_fa())),
                    ],
                )?;
            }
        }
        ctx.progress("PRICE", n as u32 + 1, total, "قیمت روز بارزپخش");
    }

    let all = ctx.db.job_items(ctx.job_id)?;
    // Products a link of this job still waits on — under review, unpriced, failed at its quote, or
    // planned without a link — so no other link writes there in its place (R-e; slice 2's review).
    let mut unresolved: HashSet<String> = HashSet::new();
    for item in all
        .iter()
        .filter(|i| !(i.status == "PLANNED" && i.cost.is_some()))
    {
        unresolved.extend(item.link_product.iter().cloned());
        unresolved.extend(item.product_id.iter().cloned());
    }
    let mut candidates = Vec::new();
    for item in all
        .iter()
        .filter(|i| i.status == "PLANNED" && i.cost.is_some())
    {
        let (Some(product), Some(key), Some(version)) =
            (&item.product_id, &item.link_key, item.link_version)
        else {
            held(ctx, &[item.id], &BridgeError::LinkChanged)?;
            unresolved.extend(item.product_id.iter().cloned());
            continue;
        };
        candidates.push(ImportCandidate {
            item_id: item.id,
            product: product.clone(),
            authenticity: item
                .authenticity
                .as_deref()
                .and_then(Authenticity::parse)
                .unwrap_or(svc.settings.default_authenticity),
            cost: item.cost.unwrap_or(0),
            qty: item.qty,
            link_key: key.clone(),
            link_version: version,
        });
    }
    let others = other_links(ctx, svc, &mut candidates, &all, &mut vat).await?;
    vat.warn(ctx, "PRICE", svc.settings.vat_percent);

    ctx.phase("WRITE")?;
    let mut shelf = svc.landau.shelf(&store).await?;
    let plan = plan_import(
        &ctx.db,
        SUPPLIER_ID,
        ctx.job_id,
        &candidates,
        &unresolved,
        &others.links,
    )?;
    let writer = ShelfWriter::new(&svc.landau, &ctx.db, SUPPLIER_ID);
    let total = plan.len() as u32;
    let mut written: Vec<(i64, String)> = Vec::new();
    for (n, planned) in plan.iter().enumerate() {
        ctx.check_cancel()?;
        let w = match planned {
            Planned::Held { items, error } => {
                let product = candidates
                    .iter()
                    .find(|c| c.item_id == items[0])
                    .map(|c| c.product.as_str());
                let collision = matches!(error, BridgeError::LinkConflict { .. });
                if let Some(note) = product
                    .and_then(|p| others.notes.get(p))
                    .filter(|_| collision)
                {
                    for id in items {
                        ctx.db.add_item_note(*id, note)?;
                    }
                }
                held(ctx, items, error)?;
                continue;
            }
            Planned::Write(w) => w,
        };
        let group: Vec<&JobItemRow> = w
            .items()
            .iter()
            .filter_map(|id| all.iter().find(|i| i.id == *id))
            .collect();
        // an unavailable quote on any link of the row keeps the unavailable-only-raises hold, as in
        // REFRESH
        let available = group.iter().all(|i| i.available.unwrap_or(true))
            && !others.unavailable.contains(w.product());
        let authenticity = match w.target() {
            super::proof::Target::Row { authenticity, .. } => *authenticity,
            super::proof::Target::Stock { .. } => svc.settings.default_authenticity,
        };
        // The invoice lines this write transfers: the writer marks them in the same SQLite
        // transaction that closes the write's intent, and only once the write is known to have
        // landed — a crash or a lost reply leaves both to the next job's reconciliation (PI-7).
        let lines: Vec<(i64, String)> = if mode == ImportMode::Quantity {
            group
                .iter()
                .flat_map(|item| {
                    item.invoice_ids
                        .iter()
                        .map(|invoice_id| (*invoice_id, item.code.clone()))
                })
                .collect()
        } else {
            Vec::new()
        };
        let term = group
            .iter()
            .find_map(|i| i.product_name.as_deref())
            .unwrap_or("");
        let req = ImportWrite {
            store: &store,
            mode,
            available,
            min_margin_percent: svc.settings.min_margin_percent,
            term,
            lines: &lines,
        };
        match writer.import(w, &req, &mut shelf).await {
            Ok(done) => {
                let qty: i64 = group.iter().map(|i| i.qty).sum();
                if done.held {
                    ctx.db
                        .add_item_note(w.lead(), "ناموجود؛ قیمت پایین‌تر نادیده گرفته شد")?;
                }
                if done.replanned {
                    ctx.db.add_item_note(
                        w.lead(),
                        "ردیف قفسه پس از خواندن تغییر کرده بود؛ با مقدار تازهٔ آن نوشته شد.",
                    )?;
                }
                if done.restored {
                    ctx.info(
                        "WRITE",
                        "ردیف حذف‌شده بازیابی شد.",
                        Some(json!({"item_id": w.lead(), "stock_id": done.stock_id})),
                    );
                }
                if mode == ImportMode::Quantity && done.shortfall > 0 {
                    let short = done.shortfall;
                    ctx.warn(
                        "WRITE",
                        kinds::NEGATIVE_QUANTITY,
                        &format!(
                            "«{}»: مرجوعی‌ها {short} عدد بیشتر از موجودی قفسه بود؛ موجودی صفر شد.",
                            group[0].name
                        ),
                        Some(json!({"item_id": w.lead(), "stock_id": done.stock_id, "shortfall": short, "qty": qty, "qty_before": done.qty_before})),
                    );
                }
                for item in &group {
                    // the transfer guard was written with the landing, before the item is stamped
                    // WRITTEN: an orphaned guard only loses this item's report status, never
                    // inventory
                    let lead = item.id == w.lead();
                    let mut patch = vec![
                        ("stock_id", t(&done.stock_id)),
                        ("cost", i(done.cost)),
                        ("authenticity", t(authenticity.as_str())),
                        ("status", t("WRITTEN")),
                    ];
                    if lead {
                        patch.extend([
                            ("action", t(done.verb)),
                            ("qty_before", opt_i(done.qty_before)),
                            ("qty_after", opt_i(done.qty_after)),
                            ("price_before", opt_i(done.price_before)),
                            ("cost_before", opt_i(done.cost_before)),
                            ("price_after", opt_i(done.price_after)),
                        ]);
                    } else {
                        ctx.db.add_item_note(
                            item.id,
                            &format!(
                                "با «{}» یکجا روی همان ردیف نوشته شد (همان قیمت خرید).",
                                group[0].name
                            ),
                        )?;
                    }
                    ctx.item(item.id, &patch)?;
                    written.push((item.id, done.stock_id.clone()));
                }
            }
            Err(e) if fatal(&e) => return Err(e),
            Err(e) => held(ctx, w.items(), &e)?,
        }
        ctx.progress("WRITE", n as u32 + 1, total, "نوشتن روی قفسه لاندا");
    }

    reprice_items(ctx, svc, &written).await?;
    report(ctx, svc, &written)?;

    let all = ctx.db.job_items(ctx.job_id)?;
    let count = |status: &str| all.iter().filter(|i| i.status == status).count();
    let summary = json!({
        "items": all.len(),
        "done": count("DONE"),
        "attached": all.iter().filter(|i| i.action.as_deref() == Some("ATTACH")).count(),
        "patched": all.iter().filter(|i| i.action.as_deref() == Some("PATCH")).count(),
        // held rows are counted apart (plan 130 R-h), never twice
        "skipped": all.iter().filter(|i| i.status == "SKIPPED" && !i.error_kind.as_deref().is_some_and(|k| IDENTITY_KINDS.contains(&k))).count(),
        "held": all.iter().filter(|i| i.error_kind.as_deref().is_some_and(|k| IDENTITY_KINDS.contains(&k))).count(),
        "failed": count("FAILED"),
        "invoice_ids": job.summary.as_ref().and_then(|s| s.get("invoice_ids")).cloned().unwrap_or(json!([])),
    });
    ctx.db.set_job_summary(ctx.job_id, &summary)?;
    ctx.info("REPORT", &summary.to_string(), None);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::landau::writer::merge_row;
    use crate::landau::ShelfRow;

    fn row(id: &str, inventory: i64) -> ShelfRow {
        ShelfRow {
            id: id.into(),
            product: "p".into(),
            product_name: None,
            brand: None,
            store: None,
            inventory,
            price: None,
            cost: None,
            condition: Some("NEW".into()),
            authenticity: Some("ORIGINAL".into()),
            price_source: None,
            updated_at: None,
        }
    }

    #[test]
    fn the_ledger_is_bounded_only_for_a_quantity_run_that_asks_for_it() {
        let params = |mode, fetch_since_only| ImportParams {
            mode,
            since: Some("1405/01/01".into()),
            fetch_since_only,
            fetch_limit: None,
        };
        assert_eq!(
            params(ImportMode::Quantity, true).ledger_from(),
            Some("1405/01/01")
        );
        assert_eq!(params(ImportMode::Quantity, false).ledger_from(), None);
        assert_eq!(params(ImportMode::Zero, true).ledger_from(), None);
    }

    /// Serves a three-invoice ledger: `/f/once` fails on its first fetch only, `/f/never` on
    /// every fetch, `/f/ok` never. Records every invoice URL asked for.
    #[derive(Default)]
    struct FlakySupplier {
        asked: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl crate::supplier::Supplier for FlakySupplier {
        fn id(&self) -> &'static str {
            "flaky"
        }
        async fn login(&self) -> Result<()> {
            Ok(())
        }
        async fn ledger(
            &self,
            _from: Option<&str>,
            _progress: Progress,
        ) -> Result<Vec<crate::domain::InvoiceHeader>> {
            Ok(["once", "never", "ok"]
                .iter()
                .enumerate()
                .map(|(n, name)| crate::domain::InvoiceHeader {
                    number: format!("{}", 100 + n),
                    kind: crate::domain::InvoiceKind::Sale,
                    date_jalali: "1405/01/01".into(),
                    date_iso: "2026-03-21".into(),
                    paid: true,
                    url: format!("/f/{name}"),
                })
                .collect())
        }
        async fn invoice_lines(&self, url: &str) -> Result<Vec<crate::domain::InvoiceLine>> {
            let nth = {
                let mut asked = self.asked.lock().unwrap();
                asked.push(url.to_string());
                asked.iter().filter(|u| *u == url).count()
            };
            let parse = || BridgeError::SupplierParse {
                page: "factor".into(),
                field: "lines table".into(),
            };
            match (url, nth) {
                ("/f/once", 1) | ("/f/never", _) => Err(parse()),
                _ => Ok(vec![crate::domain::InvoiceLine {
                    line_no: 1,
                    code: "3137006".into(),
                    name: "لنت جلو".into(),
                    qty: 1,
                    unit: "دست".into(),
                    unit_price: 1,
                }]),
            }
        }
        async fn price_of(&self, _code: &str) -> Result<Option<crate::domain::PriceQuote>> {
            Err(BridgeError::Other("offline".into()))
        }
        async fn quotes_by_name(&self, _name: &str) -> Result<Vec<crate::domain::PriceQuote>> {
            Err(BridgeError::Other("offline".into()))
        }
    }

    fn flaky_services() -> Services {
        services(Box::new(FlakySupplier::default()))
    }

    fn services(supplier: Box<dyn crate::supplier::Supplier>) -> Services {
        Services {
            landau: crate::landau::LandauClient::new("http://127.0.0.1:1", "apt-test").unwrap(),
            supplier,
            snapshot: Default::default(),
            settings: crate::settings::Settings {
                landau_url: "http://127.0.0.1:1".into(),
                landau_token: Some("apt-test".into()),
                store: Some("store1".into()),
                store_name: None,
                supplier_user: None,
                supplier_password: None,
                default_authenticity: Authenticity::Original,
                threshold: 85,
                vat_percent: 10.0,
                min_margin_percent: 0.0,
                overwrite_manual: false,
                fetch_since_only: true,
                auto_update: true,
            },
        }
    }

    /// The 2026-10-07 report: a page the site failed to serve once was reported as a layout change
    /// and left out of the run. Every item-level failure gets one more pass at the end of FETCH;
    /// only what fails twice reaches the report.
    fn job_ctx() -> JobCtx {
        let db = crate::db::Db::in_memory().unwrap();
        let job_id = db
            .create_job("IMPORT", Some("ZERO"), None, "store1", false)
            .unwrap();
        JobCtx {
            db,
            app: None,
            job_id,
            cancel: Default::default(),
        }
    }

    #[tokio::test]
    async fn fetch_retries_failed_invoices_once_before_reporting_them() {
        let ctx = job_ctx();
        let svc = flaky_services();
        fetch(&ctx, &svc, None, None).await.unwrap();

        let left: Vec<String> = ctx
            .db
            .unfetched_invoice_ids(SUPPLIER_ID, None)
            .unwrap()
            .into_iter()
            .map(|id| ctx.db.invoice_url(id).unwrap())
            .collect();
        assert_eq!(left, vec!["/f/never".to_string()]);

        let errors = ctx.db.list_errors(10).unwrap();
        assert_eq!(
            errors.len(),
            1,
            "only the invoice that failed twice is reported"
        );
        assert!(
            errors[0].message.contains("/f/never"),
            "{}",
            errors[0].message
        );
    }

    /// Serves `/f/<name>` invoices `parallel` at a time, each after `delay`, counting how many ran
    /// together. `once…` fails on its first fetch, `never…` on every one, `fatal` stops the job;
    /// `cancel_on` raises the job's cancel flag the first time that invoice is asked for.
    struct ParallelSupplier {
        names: Vec<String>,
        parallel: usize,
        delay: std::time::Duration,
        running: std::sync::atomic::AtomicUsize,
        max_running: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        asked: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        cancel_on: Option<(String, std::sync::Arc<std::sync::atomic::AtomicBool>)>,
    }

    impl ParallelSupplier {
        fn new(names: &[&str], parallel: usize) -> Self {
            ParallelSupplier {
                names: names.iter().map(|n| n.to_string()).collect(),
                parallel,
                delay: std::time::Duration::from_millis(20),
                running: Default::default(),
                max_running: Default::default(),
                asked: Default::default(),
                cancel_on: None,
            }
        }
    }

    #[async_trait::async_trait]
    impl crate::supplier::Supplier for ParallelSupplier {
        fn id(&self) -> &'static str {
            "parallel"
        }
        async fn login(&self) -> Result<()> {
            Ok(())
        }
        async fn ledger(
            &self,
            _from: Option<&str>,
            _progress: Progress,
        ) -> Result<Vec<crate::domain::InvoiceHeader>> {
            Ok(self
                .names
                .iter()
                .enumerate()
                .map(|(n, name)| crate::domain::InvoiceHeader {
                    number: format!("{}", 1000 + n),
                    kind: crate::domain::InvoiceKind::Sale,
                    date_jalali: "1405/01/01".into(),
                    date_iso: "2026-03-21".into(),
                    paid: true,
                    url: format!("/f/{name}"),
                })
                .collect())
        }
        async fn invoice_lines(&self, url: &str) -> Result<Vec<crate::domain::InvoiceLine>> {
            use std::sync::atomic::Ordering::SeqCst;
            let nth = {
                let mut asked = self.asked.lock().unwrap();
                asked.push(url.to_string());
                asked.iter().filter(|u| *u == url).count()
            };
            if let Some((on, flag)) = &self.cancel_on {
                if url == on {
                    flag.store(true, SeqCst);
                }
            }
            let now = self.running.fetch_add(1, SeqCst) + 1;
            self.max_running.fetch_max(now, SeqCst);
            tokio::time::sleep(self.delay).await;
            self.running.fetch_sub(1, SeqCst);
            let name = url.trim_start_matches("/f/");
            if name == "fatal" {
                return Err(BridgeError::SupplierUnreachable("connection reset".into()));
            }
            if name.starts_with("never") || (name.starts_with("once") && nth == 1) {
                return Err(BridgeError::SupplierFaulted { path: url.into() });
            }
            Ok(vec![crate::domain::InvoiceLine {
                line_no: 1,
                code: "3137006".into(),
                name: "لنت جلو".into(),
                qty: 1,
                unit: "دست".into(),
                unit_price: 1,
            }])
        }
        async fn price_of(&self, _code: &str) -> Result<Option<crate::domain::PriceQuote>> {
            Err(BridgeError::Other("offline".into()))
        }
        async fn quotes_by_name(&self, _name: &str) -> Result<Vec<crate::domain::PriceQuote>> {
            Err(BridgeError::Other("offline".into()))
        }
        fn parallelism(&self) -> usize {
            self.parallel
        }
    }

    fn unfetched(ctx: &JobCtx) -> Vec<String> {
        ctx.db
            .unfetched_invoice_ids(SUPPLIER_ID, None)
            .unwrap()
            .into_iter()
            .map(|id| ctx.db.invoice_url(id).unwrap())
            .collect()
    }

    /// The parallel fetch keeps LND-Q01's contract: per-invoice failures stay per invoice, get one
    /// more pass, and only what fails twice is reported — now with several invoices in flight.
    #[tokio::test]
    async fn fetch_runs_as_many_as_the_supplier_allows_and_keeps_the_second_pass() {
        let mut names: Vec<String> = (0..17).map(|n| format!("ok{n}")).collect();
        names.extend(["once1", "once2", "never1"].map(String::from));
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let supplier = ParallelSupplier::new(&names, 4);
        let (max_running, asked) = (supplier.max_running.clone(), supplier.asked.clone());
        let ctx = job_ctx();
        let started = std::time::Instant::now();
        fetch(&ctx, &services(Box::new(supplier)), None, None)
            .await
            .unwrap();
        let took = started.elapsed();

        assert_eq!(
            max_running.load(std::sync::atomic::Ordering::SeqCst),
            4,
            "never more than the supplier's parallelism, and all of it used"
        );
        assert_eq!(unfetched(&ctx), vec!["/f/never1".to_string()]);
        let errors = ctx.db.list_errors(10).unwrap();
        assert_eq!(
            errors.len(),
            1,
            "only the invoice that failed twice is reported"
        );
        assert!(errors[0].message.contains("/f/never1"));
        assert_eq!(
            asked.lock().unwrap().len(),
            20 + 3,
            "the three failures asked once more"
        );
        // 23 fetches of 20 ms, four at a time: ~0.12 s; one at a time would take 0.46 s.
        assert!(took < std::time::Duration::from_millis(350), "{took:?}");
    }

    /// As the one-at-a-time loop let its current invoice land, a cancel lets every invoice in
    /// flight land — and starts nothing new.
    #[tokio::test]
    async fn a_cancel_lets_what_is_in_flight_land_and_starts_nothing_new() {
        let names: Vec<String> = (0..10).map(|n| format!("ok{n}")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let ctx = job_ctx();
        let mut supplier = ParallelSupplier::new(&names, 3);
        supplier.cancel_on = Some(("/f/ok1".into(), ctx.cancel.clone()));
        let asked = supplier.asked.clone();
        let result = fetch(&ctx, &services(Box::new(supplier)), None, None).await;
        assert_eq!(result, Err(BridgeError::JobCancelled));
        assert_eq!(
            asked.lock().unwrap().len(),
            3,
            "only the first three were started"
        );
        assert_eq!(unfetched(&ctx).len(), 10 - 3, "and all three were stored");
    }

    #[tokio::test]
    async fn a_fatal_failure_still_stops_the_fetch() {
        let ctx = job_ctx();
        let supplier = ParallelSupplier::new(&["ok0", "fatal", "ok1", "ok2", "ok3", "ok4"], 2);
        let result = fetch(&ctx, &services(Box::new(supplier)), None, None).await;
        assert!(
            matches!(result, Err(BridgeError::SupplierUnreachable(_))),
            "{result:?}"
        );
        assert!(
            ctx.db.list_errors(10).unwrap().is_empty(),
            "a stop, not a report"
        );
    }

    #[test]
    fn merge_row_replaces_by_id_or_appends_and_keeps_the_rest() {
        let mut shelf = vec![row("a", 1), row("b", 2)];
        merge_row(&mut shelf, row("b", 7));
        merge_row(&mut shelf, row("c", 3));
        assert_eq!(shelf.len(), 3);
        assert_eq!(shelf[0].inventory, 1, "untouched sibling keeps its figure");
        assert_eq!(shelf[1].inventory, 7, "same id replaced in place");
        assert_eq!(shelf[2].id, "c");
    }

    /// Task 3, R-b: a registry Landau cannot serve whole stops MATCH; no link is judged, every
    /// stored link stays `UNVERIFIED`, and no item is planned from one.
    #[tokio::test]
    async fn a_registry_out_of_reach_stops_matching_and_leaves_links_unverified() {
        let ctx = job_ctx();
        ctx.db
            .set_match(SUPPLIER_ID, "قطعه", "C1", Some(("p1", "x")), "USER", None)
            .unwrap();
        ctx.db.add_job_item(ctx.job_id, "C1", "قطعه", 1).unwrap();
        let e = match_phase(&ctx, &flaky_services()).await.unwrap_err();
        assert!(matches!(e, BridgeError::LandauUnreachable(_)), "{e:?}");
        assert!(fatal(&e), "the job stops");
        let link = ctx.db.match_for_name(SUPPLIER_ID, "قطعه").unwrap().unwrap();
        assert_eq!(link.state, "UNVERIFIED");
        let item = &ctx.db.job_items(ctx.job_id).unwrap()[0];
        assert_eq!(
            (item.product_id.as_deref(), item.link_key.as_deref()),
            (None, None)
        );
    }

    /// R-d: a hurried pick against a conflict writes nothing until its exact reasons are
    /// confirmed; the confirmed pick keeps what was confirmed with its link.
    #[test]
    fn a_conflicting_pick_is_refused_until_its_reasons_are_confirmed() {
        use crate::matching::evidence::tests::{bearing_5e12, cand, RIGHT_BEARING};
        let ctx = job_ctx();
        ctx.db
            .set_job_status(ctx.job_id, "REVIEW", "REVIEW")
            .unwrap();
        let (code, name) = RIGHT_BEARING;
        let id = ctx.db.add_job_item(ctx.job_id, code, name, 3).unwrap();
        ctx.db
            .update_job_item(id, &[("status", t("REVIEW"))])
            .unwrap();
        let wrong = cand(
            "6aec987cd00e9931421a5e00",
            "بلبرینگ چرخ جلو قدیم پراید",
            Some("بلبرینگ 69349"),
            Some("PSN عظام"),
        );
        let registry =
            crate::matching::registry::Registry::from_products(vec![bearing_5e12(), wrong.clone()]);
        let item = ctx.db.job_item(id).unwrap().unwrap();
        let judged = crate::inspect::judge_pick(&registry, name, code, &wrong.id, 85).unwrap();
        let crate::matching::gate::Verdict::ConfirmThenWrite(shown) = judged.verdict.clone() else {
            panic!("expected a confirm: {:?}", judged.verdict)
        };

        let e = pick(&ctx.db, &item, &judged, None, None).unwrap_err();
        assert!(matches!(e, BridgeError::ConfirmRequired { .. }), "{e:?}");
        let e = pick(&ctx.db, &item, &judged, Some(&[Reason::Tied]), None).unwrap_err();
        assert!(matches!(e, BridgeError::ConfirmRequired { .. }), "{e:?}");
        assert_eq!(ctx.db.job_item(id).unwrap().unwrap().status, "REVIEW");
        assert!(
            ctx.db.match_for_name(SUPPLIER_ID, name).unwrap().is_none(),
            "no link written"
        );

        pick(&ctx.db, &item, &judged, Some(&shown), None).unwrap();
        assert_eq!(ctx.db.job_item(id).unwrap().unwrap().status, "PLANNED");
        let link = ctx.db.match_for_name(SUPPLIER_ID, name).unwrap().unwrap();
        let kept: Vec<Reason> =
            serde_json::from_str(link.confirmed_json.as_deref().unwrap()).unwrap();
        assert_eq!(kept, shown);

        // a judgement made for another item never decides this one — in review or in Matches
        let other = "بلبرينگ 44649 PSN عظام";
        let elsewhere = crate::inspect::judge_pick(&registry, other, "X1", &wrong.id, 85).unwrap();
        ctx.db
            .update_job_item(id, &[("status", t("REVIEW"))])
            .unwrap();
        let e = pick(&ctx.db, &item, &elsewhere, Some(&shown), None).unwrap_err();
        assert!(matches!(e, BridgeError::Other(_)), "{e:?}");
        let e = matches_pick(&ctx.db, other, "X1", &judged, Some(&shown)).unwrap_err();
        assert!(matches!(e, BridgeError::Other(_)), "{e:?}");

        // the Matches screen's pick: the same refusal, then the definition and what was confirmed
        let judged = crate::inspect::judge_pick(&registry, other, "X1", &wrong.id, 85).unwrap();
        let crate::matching::gate::Verdict::ConfirmThenWrite(shown) = judged.verdict.clone() else {
            panic!("expected a confirm: {:?}", judged.verdict)
        };
        let e = matches_pick(&ctx.db, other, "X1", &judged, None).unwrap_err();
        assert!(matches!(e, BridgeError::ConfirmRequired { .. }), "{e:?}");
        assert!(ctx.db.match_for_name(SUPPLIER_ID, other).unwrap().is_none());
        matches_pick(&ctx.db, other, "X1", &judged, Some(&shown)).unwrap();
        let link = ctx.db.match_for_name(SUPPLIER_ID, other).unwrap().unwrap();
        assert_eq!(
            (link.how.as_str(), link.state.as_str(), link.validated_job),
            ("USER", "UNVERIFIED", None),
            "proven by no job yet"
        );
        assert_eq!(
            (link.product_brand.as_deref(), link.product_alias.as_deref()),
            (Some("PSN عظام"), Some("بلبرینگ 69349")),
            "the definition recorded, so a re-import rebuilds it"
        );
        assert!(link.confirmed_json.is_some());

        // a consistent pick records no confirmation, and clears an earlier one
        let right =
            crate::inspect::judge_pick(&registry, name, code, &bearing_5e12().id, 85).unwrap();
        ctx.db
            .update_job_item(id, &[("status", t("REVIEW"))])
            .unwrap();
        pick(&ctx.db, &item, &right, None, None).unwrap();
        let link = ctx.db.match_for_name(SUPPLIER_ID, name).unwrap().unwrap();
        assert_eq!(link.confirmed_json, None);

        // slice 4's review: a pick that waited while the job moved on lands nowhere
        ctx.db
            .update_job_item(id, &[("status", t("REVIEW"))])
            .unwrap();
        ctx.db
            .set_job_status(ctx.job_id, "RUNNING", "PRICE")
            .unwrap();
        let e = pick(&ctx.db, &item, &right, None, None).unwrap_err();
        assert!(matches!(e, BridgeError::Other(_)), "{e:?}");
        let link = ctx.db.match_for_name(SUPPLIER_ID, name).unwrap().unwrap();
        assert_eq!(link.confirmed_json, None);
    }

    /// The seller's pick in review is a proven link for this job, at the version it now holds,
    /// carrying the definition the job's snapshot holds (R-a, R-b).
    #[test]
    fn a_review_pick_is_proven_for_its_job_with_the_definition_shown() {
        let ctx = job_ctx();
        ctx.db
            .set_job_status(ctx.job_id, "REVIEW", "REVIEW")
            .unwrap();
        let id = ctx.db.add_job_item(ctx.job_id, "C1", "قطعه", 1).unwrap();
        ctx.db
            .update_job_item(id, &[("status", t("REVIEW"))])
            .unwrap();
        let registry =
            crate::matching::registry::Registry::from_products(vec![crate::landau::Candidate {
                id: "p1".into(),
                name: "x".into(),
                alias: Some("a | b".into()),
                brand: Some("عظام".into()),
                score: 0.0,
            }]);
        let item = ctx.db.job_item(id).unwrap().unwrap();
        let judged =
            crate::inspect::judge_pick(&registry, &item.name, &item.code, "p1", 85).unwrap();
        pick(&ctx.db, &item, &judged, None, None).unwrap();
        let item = ctx.db.job_item(id).unwrap().unwrap();
        assert_eq!(item.status, "PLANNED");
        let link = ctx.db.match_for_name(SUPPLIER_ID, "قطعه").unwrap().unwrap();
        assert!(link.writable_for(ctx.job_id, item.link_version.unwrap(), "p1"));
        assert_eq!(item.link_key.as_deref(), Some(link.name_key.as_str()));
        assert_eq!(
            (link.product_brand.as_deref(), link.product_alias.as_deref()),
            (Some("عظام"), Some("a | b"))
        );
        let last = ctx
            .db
            .match_events(SUPPLIER_ID, &link.name_key)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(
            (last.actor.as_str(), last.evidence.as_deref()),
            ("SELLER", Some("shown"))
        );
    }
}
