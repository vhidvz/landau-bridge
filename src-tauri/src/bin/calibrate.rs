//! Measures auto-accept precision/coverage per threshold against the SKU → registry-name map the
//! registry was imported from, and writes docs/matching-threshold.md. Feature `live`.
//!
//! Env: LND_API_URL, LND_APT_TOKEN (required); PARTS_MAP (default: the landau-parts export);
//! SAMPLE (default 2000); SEED (default 42); OUT (default docs/matching-threshold.md).

use std::collections::BTreeMap;
use std::io::Write;

use landau_bridge_lib::errors::BridgeError;
use landau_bridge_lib::landau::{LandauClient, RegistryPage};
use landau_bridge_lib::matching::normalize::{key_text, search_term};
use landau_bridge_lib::matching::{candidates, score};

/// The live registry occasionally answers a bare request with a transient backend error (seen:
/// `LandauRejected { status: 500, body: "ERR invalid expire time in 'psetex' command" }`, roughly
/// once per few hundred requests — a rate-limiter edge case, not a query problem). `LandauClient`
/// itself does not retry, so a 2 000-sample run would otherwise abort on the first blip; retry
/// transport errors and 5xx/429 up to twice with a short backoff, same policy shape as the
/// supplier client's (spec §3), scaled down since this is a one-off calibration run, not the app.
async fn search_with_retry(
    landau: &LandauClient,
    q: &str,
    limit: usize,
    skip: usize,
) -> landau_bridge_lib::errors::Result<RegistryPage> {
    let delays = [
        std::time::Duration::from_millis(500),
        std::time::Duration::from_millis(1500),
    ];
    for delay in delays {
        match landau.registry_search(q, limit, skip).await {
            Ok(page) => return Ok(page),
            Err(BridgeError::LandauUnreachable(_)) => tokio::time::sleep(delay).await,
            Err(BridgeError::LandauRejected { status, .. }) if status >= 500 || status == 429 => {
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
    landau.registry_search(q, limit, skip).await
}

#[derive(Debug)]
struct Trial {
    code: String,
    raw_name: String,
    expected: String,
    top_name: Option<String>,
    similarity: u8,
    correct: bool,
}

/// Deterministic sample without an RNG crate: a linear congruential walk over the row index.
fn sample_indices(len: usize, n: usize, seed: u64) -> Vec<usize> {
    let mut out = Vec::with_capacity(n.min(len));
    let mut x = seed;
    let mut seen = std::collections::HashSet::new();
    while out.len() < n.min(len) {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let i = (x >> 33) as usize % len;
        if seen.insert(i) {
            out.push(i);
        }
    }
    out
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("LND_API_URL")?;
    let token = std::env::var("LND_APT_TOKEN")?;
    let map = std::env::var("PARTS_MAP")
        .unwrap_or_else(|_| "/home/vahid/WorkSpace/landau-parts/output/parts_map.csv".into());
    let sample: usize = std::env::var("SAMPLE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000);
    let seed: u64 = std::env::var("SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);
    let out = std::env::var("OUT").unwrap_or_else(|_| "../docs/matching-threshold.md".into());

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(&map)?;
    let headers = reader.headers()?.clone();
    let col = |name: &str| {
        headers
            .iter()
            .position(|h| h.trim_start_matches('\u{feff}') == name)
            .unwrap_or_else(|| panic!("column {name}"))
    };
    let (c_code, c_raw, c_state, c_product) = (
        col("code"),
        col("raw_name"),
        col("state"),
        col("product_name"),
    );
    let rows: Vec<(String, String, String)> = reader
        .records()
        .filter_map(|r| r.ok())
        .filter(|r| r.get(c_state) == Some("done") && !r.get(c_product).unwrap_or("").is_empty())
        .map(|r| {
            (
                r[c_code].to_string(),
                r[c_raw].to_string(),
                r[c_product].to_string(),
            )
        })
        .collect();
    eprintln!("{} usable rows in {map}", rows.len());

    let landau = LandauClient::new(&url, &token)?;
    let mut trials = Vec::new();
    let mut empty_streak = 0;
    for (i, idx) in sample_indices(rows.len(), sample, seed)
        .into_iter()
        .enumerate()
    {
        let (code, raw, expected) = &rows[idx];
        let term = search_term(raw, code);
        let search = |q: String, limit: usize, skip: usize| {
            let landau = &landau;
            async move { search_with_retry(landau, &q, limit, skip).await }
        };
        let hits = candidates(&search, &term).await?;
        empty_streak = if hits.is_empty() { empty_streak + 1 } else { 0 };
        if i < 20 && empty_streak == 20 {
            panic!("the first 20 searches answered nothing — is the registry imported? (docs/RUNBOOK.md → Registry)");
        }
        let scored = score(&term, &hits);
        let top = scored.first();
        let top_name = top.map(|t| t.candidate.name.clone());
        let correct = top_name
            .as_deref()
            .map(|n| key_text(n) == key_text(expected))
            .unwrap_or(false);
        trials.push(Trial {
            code: code.clone(),
            raw_name: raw.clone(),
            expected: expected.clone(),
            top_name,
            similarity: top.map(|t| t.similarity).unwrap_or(0),
            correct,
        });
        if (i + 1) % 100 == 0 {
            eprintln!("{}/{sample}", i + 1);
        }
    }

    // precision/coverage per threshold
    let mut table: BTreeMap<u8, (usize, usize)> = BTreeMap::new(); // t -> (accepted, correct)
    for t in (50..=100).step_by(5) {
        let accepted: Vec<&Trial> = trials.iter().filter(|x| x.similarity >= t).collect();
        let correct = accepted.iter().filter(|x| x.correct).count();
        table.insert(t, (accepted.len(), correct));
    }
    let chosen = table
        .iter()
        .filter(|(_, (a, c))| *a > 0 && (*c as f64) / (*a as f64) >= 0.98)
        .map(|(t, _)| *t)
        .min()
        .unwrap_or(100);
    let top1_correct = trials.iter().filter(|x| x.correct).count();

    let mut f = std::fs::File::create(&out)?;
    writeln!(f, "# Matching threshold — calibration record\n")?;
    writeln!(
        f,
        "Generated by `cargo run --bin calibrate --features live` on {} against `{map}`.",
        chrono::Utc::now().format("%Y-%m-%d")
    )?;
    writeln!(f, "Sample: {} SKUs (seed {seed}) of {} usable rows; candidates = the seller's registry picker (`GET /career/stocks/registry?term=`, substring over name/alias/brand) queried with the whole normalised name, its bigrams and its tokens (single tokens matching > 300 rows skipped); similarity = token-set Jaccard of the normalised invoice name against each candidate's name, name+brand, and aliases (with and without the brand).\n", trials.len(), rows.len())?;
    writeln!(f, "Top-1 hit names the expected registry part for **{top1_correct} / {}** samples regardless of threshold.\n", trials.len())?;
    writeln!(
        f,
        "| threshold | auto-accepted | correct | precision | coverage |"
    )?;
    writeln!(f, "| --- | --- | --- | --- | --- |")?;
    for (t, (a, c)) in &table {
        let p = if *a == 0 {
            0.0
        } else {
            *c as f64 / *a as f64 * 100.0
        };
        writeln!(
            f,
            "| {t} | {a} | {c} | {p:.1}% | {:.1}% |",
            *a as f64 / trials.len() as f64 * 100.0
        )?;
    }
    writeln!(f, "\n## Decision\n")?;
    writeln!(f, "**Threshold = {chosen}** — the lowest threshold whose auto-accept precision is ≥ 98 % (a wrong auto-match puts a seller's stock under the wrong part with no one looking; a missed match only costs one click in review). Set as `DEFAULT_THRESHOLD` in `src-tauri/src/settings.rs`; adjustable per seller in تنظیمات.\n")?;
    writeln!(f, "## Misses at the chosen threshold (first 30)\n")?;
    writeln!(
        f,
        "| code | invoice name | expected | top hit | similarity |"
    )?;
    writeln!(f, "| --- | --- | --- | --- | --- |")?;
    for x in trials
        .iter()
        .filter(|x| x.similarity >= chosen && !x.correct)
        .take(30)
    {
        writeln!(
            f,
            "| {} | {} | {} | {} | {} |",
            x.code,
            x.raw_name,
            x.expected,
            x.top_name.clone().unwrap_or_default(),
            x.similarity
        )?;
    }
    println!("threshold={chosen} written to {out}");
    Ok(())
}
