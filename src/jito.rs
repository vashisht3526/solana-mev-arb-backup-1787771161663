#![allow(dead_code)]
use std::sync::OnceLock;

/// Jito Block Engine Endpoints for direct validator leader co-location
pub const JITO_BLOCK_ENGINE_FRANKFURT: &str = "https://frankfurt.mainnet.block-engine.jito.wtf/api/v1/bundles";
pub const JITO_BLOCK_ENGINE_AMSTERDAM: &str = "https://amsterdam.mainnet.block-engine.jito.wtf/api/v1/bundles";
pub const JITO_BLOCK_ENGINE_DUBLIN: &str = "https://dublin.mainnet.block-engine.jito.wtf/api/v1/bundles";
pub const JITO_BLOCK_ENGINE_TOKYO: &str = "https://tokyo.mainnet.block-engine.jito.wtf/api/v1/bundles";
pub const JITO_BLOCK_ENGINE_NY: &str = "https://ny.mainnet.block-engine.jito.wtf/api/v1/bundles";

/// V43: regions we race simultaneously from Frankfurt EC2 (<20ms RTT each).
pub const MULTIREGION_ENDPOINTS: &[&str] = &[
    JITO_BLOCK_ENGINE_FRANKFURT,
    JITO_BLOCK_ENGINE_AMSTERDAM,
    JITO_BLOCK_ENGINE_DUBLIN,
];

/// Live tip floor endpoint (public, no key, no credits)
const TIP_FLOOR_URL: &str = "https://bundles.jito.wtf/api/v1/bundles/tip_floor";

/// Public Jito Validator Tip Accounts
pub const JITO_TIP_ACCOUNTS: &[&str] = &[
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
    "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
];

pub struct JitoBundleOptimizer;

impl JitoBundleOptimizer {
    /// Calculate optimal validator tip lamports based on expected gross profit
    /// Bids between 40% and 75% of profit depending on competition margin
    pub fn calculate_dynamic_tip_lamports(gross_profit_sol: f64) -> u64 {
        if gross_profit_sol <= 0.0 {
            return 10_000; // Minimum 0.00001 SOL base tip
        }

        let profit_lamports = (gross_profit_sol * 1e9) as u64;
        let tip_ratio = if gross_profit_sol > 0.05 {
            0.65 // Bid 65% on large arbs to guarantee inclusion
        } else {
            0.50 // Bid 50% on standard micro-arbs
        };

        let calculated_tip = (profit_lamports as f64 * tip_ratio) as u64;
        calculated_tip.max(10_000).min(profit_lamports.saturating_sub(10_000))
    }

    /// Select optimal Jito tip account
    pub fn get_tip_account(index_hint: usize) -> &'static str {
        JITO_TIP_ACCOUNTS[index_hint % JITO_TIP_ACCOUNTS.len()]
    }
}

// ---------------------------------------------------------------------------
// V42: MULTI-REGION SUBMISSION + OUTCOME TELEMETRY + LIVE TIP FLOOR
// ---------------------------------------------------------------------------

/// Race the same bundle across Frankfurt/Amsterdam/Dublin block engines concurrently.
/// Returns Ok(bundle_uuid) when at least one region accepted it.
pub async fn submit_bundle_multiregion(
    client: &reqwest::Client,
    payload: &serde_json::Value,
) -> Result<String, String> {
    let posts = MULTIREGION_ENDPOINTS.iter().map(|ep| {
        let fut = client.post(*ep).json(payload).send();
        let ep_str = (*ep)
            .split('.')
            .next()
            .unwrap_or("region")
            .to_string();
        async move { (ep_str, fut.await) }
    });

    let results = futures_util::future::join_all(posts).await;
    let mut first_uuid: Option<String> = None;
    for (region, res) in results {
        match res {
            Ok(resp) => {
                let status = resp.status();
                match resp.json::<serde_json::Value>().await {
                    Ok(body) => {
                        if let Some(uuid) = body.get("result").and_then(|r| r.as_str()) {
                            log::info!("📡 [MULTI-REGION] {} accepted bundle {}", region.to_uppercase(), uuid);
                            if first_uuid.is_none() {
                                first_uuid = Some(uuid.to_string());
                            }
                        } else if first_uuid.is_none() {
                            log::warn!("⚠️ [MULTI-REGION] {} reject HTTP {}: {:?}", region.to_uppercase(), status, body);
                        }
                    }
                    Err(e) => {
                        log::warn!("⚠️ [MULTI-REGION] {} unreadable response: {}", region.to_uppercase(), e);
                    }
                }
            }
            Err(e) => {
                log::warn!("⚠️ [MULTI-REGION] {} network error: {}", region.to_uppercase(), e);
            }
        }
    }

    first_uuid.ok_or_else(|| "All regions rejected or unreachable".to_string())
}

/// V42 outcome telemetry: poll getInflightBundleStatuses until the bundle resolves,
/// append every dispatch + resolution to data/bundle_outcomes.jsonl. This builds the
/// on-chain execution dataset (landed / auction-lost / missed-leader) that replaces
/// assumption about what the gates should be.
pub fn spawn_bundle_outcome_tracker(
    client: reqwest::Client,
    bundle_uuid: String,
    symbol: String,
    tip_lamports: u64,
) {
    tokio::spawn(async move {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getInflightBundleStatuses",
            "params": [[bundle_uuid]]
        });

        let mut last_status = String::from("Pending");
        for delay_ms in [2_500u64, 4_000, 6_000] {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            let body_opt = client
                .post(JITO_BLOCK_ENGINE_FRANKFURT)
                .json(&payload)
                .send()
                .await
                .ok();
            let parsed = if let Some(resp) = body_opt {
                resp.json::<serde_json::Value>().await.ok()
            } else {
                None
            };
            if let Some(body) = parsed {
                if let Some(st) = body["result"]["value"][0]["status"].as_str() {
                    last_status = st.to_string();
                    match st {
                        "Landed" => break,      // settled — no need to keep polling
                        "Failed" => break,      // lost the auction definitively
                        "Invalid" => break,     // construction problem
                        _ => {}                 // Pending → keep waiting
                    }
                }
            }
        }

        let outcome_line = serde_json::json!({
            "ts": chrono::Utc::now().to_rfc3339(),
            "bundle_uuid": bundle_uuid,
            "symbol": symbol,
            "tip_lamports": tip_lamports,
            "final_status": last_status,
        });
        let _ = std::fs::create_dir_all("data");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("data/bundle_outcomes.jsonl")
        {
            use std::io::Write;
            let _ = writeln!(f, "{}", outcome_line);
        }

        match last_status.as_str() {
            "Landed" => log::info!("🏆 [BUNDLE TRACKER] {} LANDED | uuid {} | tip {}", symbol, bundle_uuid, tip_lamports),
            "Failed" => log::info!("💨 [BUNDLE TRACKER] {} auction LOST | uuid {} | tip {}", symbol, bundle_uuid, tip_lamports),
            "Invalid" => log::warn!("❌ [BUNDLE TRACKER] {} INVALID bundle | uuid {}", symbol, bundle_uuid),
            _ => log::info!("⏳ [BUNDLE TRACKER] {} expired unresolved (likely missed leader window) | uuid {}", symbol, bundle_uuid),
        }
    });
}

#[derive(Debug, Clone, Copy)]
pub struct TipFloorLamports {
    pub p50: u64,
    pub p75: u64,
    pub p95: u64,
    pub p99: u64,
}

static TIP_FLOOR: OnceLock<std::sync::RwLock<Option<TipFloorLamports>>> = OnceLock::new();

fn tip_floor_cell() -> &'static std::sync::RwLock<Option<TipFloorLamports>> {
    TIP_FLOOR.get_or_init(|| std::sync::RwLock::new(None))
}

/// Current live p50 landed tip in lamports (0 when no data yet).
pub fn current_tip_floor_p50() -> u64 {
    tip_floor_cell()
        .read()
        .ok()
        .and_then(|g| *g)
        .map(|t| t.p50)
        .unwrap_or(0)
}

pub fn current_tip_floor() -> Option<TipFloorLamports> {
    tip_floor_cell().read().ok().and_then(|g| *g)
}

fn parse_tip_floor_json(body: &[serde_json::Value]) -> Option<TipFloorLamports> {
    // Endpoint returns fractions of SOL (e.g. 7.232e-6) under *_50th_percentile keys.
    let entry = body.first()?;
    let sol_to_lamports = |key: &str| -> Option<u64> {
        Some((entry.get(key)?.as_f64()? * 1_000_000_000.0) as u64)
    };
    Some(TipFloorLamports {
        p50: sol_to_lamports("landed_tips_50th_percentile")?,
        p75: sol_to_lamports("landed_tips_75th_percentile")?,
        p95: sol_to_lamports("landed_tips_95th_percentile")?,
        p99: sol_to_lamports("landed_tips_99th_percentile")?,
    })
}

/// Background fetcher: refresh the live Jito tip floor every 60s. Public endpoint,
/// zero Helius credits. Idempotent — call from every engine process; only one loop runs.
pub fn ensure_tip_floor_fetcher() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(3_000))
            .build()
            .expect("tip floor client");
        let mut last_ok = true;
        loop {
            let mut outcome: Result<(), String> = Ok(());
            match client.get(TIP_FLOOR_URL).send().await {
                Ok(resp) => match resp.json::<Vec<serde_json::Value>>().await {
                    Ok(body) => match parse_tip_floor_json(&body) {
                        Some(floor) => {
                            log::info!(
                                "💹 [TIP FLOOR] live landed tips p50:{} p75:{} p95:{} p99:{} lamports",
                                floor.p50, floor.p75, floor.p95, floor.p99
                            );
                            if let Ok(mut cell) = tip_floor_cell().write() {
                                *cell = Some(floor);
                            }
                        }
                        None => {
                            outcome = Err(format!("unparseable body: {}", serde_json::to_string(&body).unwrap_or_default()));
                        }
                    },
                    Err(e) => outcome = Err(format!("json decode: {}", e)),
                },
                Err(e) => outcome = Err(format!("network: {}", e)),
            }
            if let Err(detail) = outcome {
                if last_ok {
                    // State transition only — don't spam one line per failed minute.
                    log::warn!("⚠️ [TIP FLOOR] feed FAILING (will retry each 60s): {}", &detail[..300.min(detail.len())]);
                }
                last_ok = false;
            } else {
                if !last_ok {
                    log::info!("💹 [TIP FLOOR] feed RECOVERED");
                }
                last_ok = true;
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

// ---------------------------------------------------------------------------
// V42: PROCESS-WIDE KILL SWITCH — sub-second trading halt
//
// The config-file pause (`is_engine_running`) only takes effect between full
// engine cycles and can race with dashboard rewrites. This switch is an atomic
// bool mirrored from the presence of `data/KILL_SWITCH`, polled every 250ms,
// checked INSIDE the builders before a bundle is signed AND at the top of every
// execution path. Flip it from anywhere: dashboard button, curl, or
//   touch data/KILL_SWITCH   # halt everything within 250ms
//   rm    data/KILL_SWITCH   # resume
// ---------------------------------------------------------------------------
static KILL_SWITCH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub const KILL_SWITCH_FILE: &str = "data/KILL_SWITCH";

#[inline(always)]
pub fn is_trading_killed() -> bool {
    KILL_SWITCH.load(std::sync::atomic::Ordering::SeqCst)
}

#[inline(always)]
fn set_kill_state(state: bool) {
    KILL_SWITCH.store(state, std::sync::atomic::Ordering::SeqCst);
}

pub fn ensure_killswitch_watcher() {
    static WATCHER_STARTED: OnceLock<()> = OnceLock::new();
    if WATCHER_STARTED.set(()).is_err() {
        return;
    }
    // Seed from disk so a restart with a stale kill file stays killed.
    let initial = std::path::Path::new(KILL_SWITCH_FILE).exists();
    set_kill_state(initial);
    if initial {
        log::error!("⛔ [KILL SWITCH] STARTUP HALT: {} exists — all live dispatch blocked until removed", KILL_SWITCH_FILE);
    }
    tokio::spawn(async move {
        let mut last = initial;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            let now = std::path::Path::new(KILL_SWITCH_FILE).exists();
            if now != last {
                last = now;
                set_kill_state(now);
                if now {
                    log::error!("⛔ [KILL SWITCH] ENGAGED — live dispatch HALTED ({} created)", KILL_SWITCH_FILE);
                } else {
                    log::info!("✅ [KILL SWITCH] RELEASED — live dispatch resumed ({} removed)", KILL_SWITCH_FILE);
                }
            }
        }
    });
}
