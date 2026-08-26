#![allow(dead_code)]
//! V42c: LIVE COMPETITOR CORRELATION — MRiY whale wallet watcher.
//!
//! Question this answers with DATA instead of assumptions: when we see a spread and
//! Jupiter calls it phantom, was it actually REAL and someone (MRiY) filled it before
//! us? And how big is their latency lead?
//!
//! Mechanics:
//!  * getSignaturesForAddress poll (20s) on MRiY's wallet buffers every tx they
//!    emit, stamped with CHAIN blockTime. (signatureSubscribe was tried first but
//!    pubkey-mode notifications carry no signature and result.value is the err
//!    object — every event was silently dropped.)
//!  * Our engines record every real attempt (rejected OR dispatched) via record_attempt.
//!  * A correlator matches their txs against our attempts inside a ±12s window and
//!    selectively getTransaction()s ONLY those sigs (credit-bounded, deduped forever).
//!  * Every resolved attempt appends a verdict line to data/mriy_correlation.jsonl:
//!      COMPETITOR_FILLED_SAME_MINT (their lead ms + SOL delta) or GHOST_NO_COMPETITOR.
//!
//! Runs ONLY in the dual_live process (see main.rs wiring) to avoid duplicate fetches.

use log::{error, info, warn};
use std::collections::{HashSet, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

pub const MRIY_WALLET: &str = "MRiYA4oN3158fCV8evhuCofrDzbHyYvYnGZUDJvoCsa";
const MATCH_WINDOW_MS: i64 = 12_000;
const MAX_TX_FETCHES_PER_CYCLE: usize = 16;
/// V42c fix: pubkey-based signatureSubscribe notifications do NOT carry the
/// signature (only result.value.err), so the WS approach silently saw nothing.
/// A 20s getSignaturesForAddress poll is robust, and because entries are stamped
/// with the CHAIN blockTime, window-matching is exact regardless of poll latency.
const POLL_INTERVAL_MS: u64 = 20_000;

#[derive(Debug, Clone)]
pub struct OurAttempt {
    pub ts_ms: i64,
    pub mint: String,
    pub symbol: String,
    pub est_spread_pct: f64,
    pub dispatched: bool,
    pub event_fresh: bool,
    pub resolved: bool,
}

static ATTEMPTS: OnceLock<Mutex<Vec<OurAttempt>>> = OnceLock::new();
static SIG_BUFFER: OnceLock<Mutex<VecDeque<(String, u64, i64)>>> = OnceLock::new();
static FETCHED_SIGS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn attempts() -> &'static Mutex<Vec<OurAttempt>> {
    ATTEMPTS.get_or_init(|| Mutex::new(Vec::new()))
}
fn sig_buffer() -> &'static Mutex<VecDeque<(String, u64, i64)>> {
    SIG_BUFFER.get_or_init(|| Mutex::new(VecDeque::with_capacity(2048)))
}
fn fetched_sigs() -> &'static Mutex<HashSet<String>> {
    FETCHED_SIGS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Engines call this on EVERY real attempt — rejected at gates AND dispatched.
pub fn record_attempt(mint: &str, symbol: &str, est_spread_pct: f64, dispatched: bool, event_fresh: bool) {
    let a = OurAttempt {
        ts_ms: chrono::Utc::now().timestamp_millis(),
        mint: mint.to_string(),
        symbol: symbol.to_string(),
        est_spread_pct,
        dispatched,
        event_fresh,
        resolved: false,
    };
    if let Ok(mut v) = attempts().lock() {
        v.push(a);
        if v.len() > 400 {
            let excess = v.len() - 400;
            v.drain(0..excess);
        }
    }
}

pub fn ensure_mriy_watcher() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    tokio::spawn(run_sig_poller());
    tokio::spawn(run_correlator());
    info!(
        "🕵️ [MRIY WATCH] Competitor correlation online (poll mode, {}s) — wallet {}… | window ±{}ms",
        POLL_INTERVAL_MS / 1000,
        &MRIY_WALLET[..8],
        MATCH_WINDOW_MS
    );
}

/// Poll MRiY's recent signatures and buffer NEW ones stamped with CHAIN blockTime.
async fn run_sig_poller() {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(6_000))
        .build()
        .expect("mriy poller client");
    let url = format!(
        "https://mainnet.helius-rpc.com/?api-key={}",
        crate::helius_stream::HELIUS_API_KEYS[0]
    );
    let req_body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "getSignaturesForAddress",
        "params": [MRIY_WALLET, {"limit": 25}]
    });

    let mut seen: HashSet<String> = HashSet::new();
    let mut first_pass = true;
    loop {
        match client.post(&url).json(&req_body).send().await {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(body) => {
                    let now_s = chrono::Utc::now().timestamp();
                    if let Some(sigs) = body["result"].as_array() {
                        for s in sigs.iter() {
                            let Some(sig) = s["signature"].as_str() else { continue };
                            if !seen.insert(sig.to_string()) {
                                continue;
                            }
                            if first_pass {
                                continue; // seed history without flooding the correlator
                            }
                            let slot = s["slot"].as_u64().unwrap_or(0);
                            // Chain truth beats local clock — immune to poll latency.
                            let bt_ms = s["blockTime"].as_i64().unwrap_or(now_s) * 1000;
                            let failed = !s["err"].is_null();
                            if let Ok(mut buf) = sig_buffer().lock() {
                                buf.push_back((sig.to_string(), slot, bt_ms));
                                while buf.len() > 2000 {
                                    buf.pop_front();
                                }
                            }
                            info!(
                                "🕵️ [MRIY TX] slot {} sig {}…{}",
                                slot,
                                &sig[..16.min(sig.len())],
                                if failed { " (FAILED probe)" } else { "" }
                            );
                        }
                    } else if let Some(e) = body["error"].as_str() {
                        warn!("🕵️ [MRIY WATCH] poll RPC error: {}", e);
                    }
                    first_pass = false;
                }
                Err(e) => warn!("🕵️ [MRIY WATCH] poll JSON fail: {}", e),
            },
            Err(e) => warn!("🕵️ [MRIY WATCH] poll net fail: {}", e),
        }
        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}

struct MriyTxInfo {
    mints: HashSet<String>,
    wallet_sol_delta_lamports: i64,
    block_time_s: i64,
}

async fn fetch_tx_info(client: &reqwest::Client, sig: &str) -> Option<MriyTxInfo> {
    let url = format!(
        "https://mainnet.helius-rpc.com/?api-key={}",
        crate::helius_stream::HELIUS_API_KEYS[3]
    );
    let req = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "getTransaction",
        "params": [sig, {"encoding": "jsonParsed", "maxSupportedTransactionVersion": 0, "commitment": "confirmed"}]
    });
    let resp = client.post(&url).json(&req).send().await.ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;
    let r = body.get("result")?;
    if r.is_null() {
        return None;
    }

    let mut mints: HashSet<String> = HashSet::new();
    for side in ["preTokenBalances", "postTokenBalances"] {
        if let Some(arr) = r["meta"][side].as_array() {
            for tb in arr {
                if let Some(m) = tb["mint"].as_str() {
                    mints.insert(m.to_string());
                }
            }
        }
    }

    // Wallet SOL delta: locate MRiY among account keys, diff pre/post lamports.
    let mut wallet_idx: Option<usize> = None;
    if let Some(keys) = r["transaction"]["message"]["accountKeys"].as_array() {
        for (i, k) in keys.iter().enumerate() {
            let pk = k["pubkey"].as_str().or_else(|| k.as_str());
            if pk == Some(MRIY_WALLET) {
                wallet_idx = Some(i);
                break;
            }
        }
    }
    let wallet_sol_delta = match wallet_idx {
        Some(i) => {
            let pre = r["meta"]["preBalances"][i].as_i64().unwrap_or(0);
            let post = r["meta"]["postBalances"][i].as_i64().unwrap_or(0);
            post - pre
        }
        None => 0,
    };

    Some(MriyTxInfo {
        mints,
        wallet_sol_delta_lamports: wallet_sol_delta,
        block_time_s: r["blockTime"].as_i64().unwrap_or(0),
    })
}

fn append_correlation_line(line: serde_json::Value) {
    let _ = std::fs::create_dir_all("data");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("data/mriy_correlation.jsonl")
    {
        use std::io::Write;
        let _ = writeln!(f, "{}", line);
    }
}

/// Periodic reconciliation: match buffered MRiY sigs against our unresolved attempts.
async fn run_correlator() {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(4_000))
        .build()
        .expect("correlator client");
    let mut cycle_count: u64 = 0;

    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        cycle_count += 1;

        // Drain fresh sigs into a local snapshot. Third tuple element is CHAIN
        // blockTime ms (not local seen time) so ±12s window math stays exact.
        let mut recent: VecDeque<(String, u64, i64)> = VecDeque::new();
        if let Ok(buf) = sig_buffer().lock() {
            let cutoff = chrono::Utc::now().timestamp_millis() - 20 * 60_000;
            for item in buf.iter().rev() {
                if item.2 >= cutoff && recent.len() < 600 {
                    recent.push_front(item.clone());
                }
            }
        }

        // Snapshot unresolved attempts newer than 5 minutes.
        let open_attempts: Vec<OurAttempt> = match attempts().lock() {
            Ok(v) => v
                .iter()
                .filter(|a| !a.resolved && chrono::Utc::now().timestamp_millis() - a.ts_ms < 300_000)
                .cloned()
                .collect(),
            Err(_) => continue,
        };

        // Selectively fetch only sigs inside some attempt's window.
        let mut to_fetch: Vec<(String, i64)> = Vec::new();
        {
            let fetched = match fetched_sigs().lock() {
                Ok(f) => f,
                Err(_) => continue,
            };
            for (sig, _slot, bt_ms) in recent.iter() {
                if fetched.contains(sig) || to_fetch.iter().any(|(s, _)| s == sig) {
                    continue;
                }
                if open_attempts
                    .iter()
                    .any(|a| (a.ts_ms - bt_ms).abs() <= MATCH_WINDOW_MS)
                {
                    to_fetch.push((sig.clone(), *bt_ms));
                }
            }
        }
        to_fetch.truncate(MAX_TX_FETCHES_PER_CYCLE);

        for (sig, bt_ms) in to_fetch {
            let Some(info) = fetch_tx_info(&client, &sig).await else {
                // Unconfirmed/expired — leave unfetched so we retry once more later.
                if let Ok(mut f) = fetched_sigs().lock() {
                    f.insert(sig.clone());
                }
                continue;
            };
            if let Ok(mut f) = fetched_sigs().lock() {
                f.insert(sig.clone());
            }

            for a in open_attempts.iter() {
                if (a.ts_ms - bt_ms).abs() > MATCH_WINDOW_MS {
                    continue;
                }
                if info.mints.contains(&a.mint) {
                    append_correlation_line(serde_json::json!({
                        "verdict": "COMPETITOR_FILLED_SAME_MINT",
                        "our_ts_ms": a.ts_ms,
                        "our_symbol": a.symbol,
                        "our_mint": a.mint,
                        "our_est_spread_pct": a.est_spread_pct,
                        "our_dispatched": a.dispatched,
                        "our_event_fresh": a.event_fresh,
                        "mriy_sig": sig,
                        "mriy_block_time_s": info.block_time_s,
                        "delta_ms_our_attempt_vs_their_tx": a.ts_ms - info.block_time_s * 1000,
                        "mriy_wallet_sol_delta_lamports": info.wallet_sol_delta_lamports,
                        "mriy_blocktime_ms": bt_ms,
                    }));
                    info!(
                        "🥊 [MRIY CORRELATION] {} — MRiY filled SAME MINT within window | our est spread {:.2}% | their lead vs our attempt: {}ms | wallet Δ{} lamports",
                        a.symbol, a.est_spread_pct, a.ts_ms - info.block_time_s * 1000, info.wallet_sol_delta_lamports
                    );
                    if let Ok(mut v) = attempts().lock() {
                        if let Some(x) = v.iter_mut().find(|x| x.ts_ms == a.ts_ms && x.mint == a.mint) {
                            x.resolved = true;
                        }
                    }
                }
            }
        }

        // Resolve stale un-matched attempts as GHOST evidence (every ~4 cycles).
        if cycle_count % 4 == 0 {
            let cutoff = chrono::Utc::now().timestamp_millis() - MATCH_WINDOW_MS * 3;
            let stale: Vec<OurAttempt> = match attempts().lock() {
                Ok(v) => v
                    .iter()
                    .filter(|a| !a.resolved && !a.dispatched && a.ts_ms < cutoff)
                    .cloned()
                    .collect(),
                Err(_) => Vec::new(),
            };
            for a in stale {
                append_correlation_line(serde_json::json!({
                    "verdict": "GHOST_NO_COMPETITOR_IN_WINDOW",
                    "our_ts_ms": a.ts_ms,
                    "our_symbol": a.symbol,
                    "our_mint": a.mint,
                    "our_est_spread_pct": a.est_spread_pct,
                    "our_event_fresh": a.event_fresh,
                }));
                info!(
                    "👻 [MRIY CORRELATION] {} — no competitor fill within window | est spread {:.2}% ⇒ GHOST evidence",
                    a.symbol, a.est_spread_pct
                );
                if let Ok(mut v) = attempts().lock() {
                    if let Some(x) = v.iter_mut().find(|x| x.ts_ms == a.ts_ms && x.mint == a.mint) {
                        x.resolved = true;
                    }
                }
            }
        }
    }
}
