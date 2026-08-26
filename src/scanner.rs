use crate::amm::evaluate_optimal_arbitrage;
use crate::dual_leg_engine::DualLegLiveEngine;
use crate::live_engine::LiveTradingEngine;
use crate::migration_sniper::RaydiumMigrationSniper;
use crate::paper_engine::PaperTradingEngine;
use crate::triangular_live_engine::TriangularLiveEngine;
use crate::types::{ArbitrageOpportunity, DynamicPoolState, TokenRiskMeta, VolatileTrackedToken};
use crate::websocket_feed::{LiveSwapEvent, PumpPortalFeed, RaydiumMigrationEvent};
use chrono::Utc;
use dashmap::DashMap;
use futures::stream::{self, StreamExt};
use log::{info, warn};
use reqwest::Client;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{channel, Receiver};
use tokio::time::sleep;

// === MORNING PROVEN CONFIGURATION ===
const MIN_POOL_LIQUIDITY_USD: f64 = 1000.0;   // $1,000 min pool liquidity
const MIN_ADMISSION_5M_VOL_USD: f64 = 20.0;    // Active volume threshold ($20 / 5min)
const MAX_CONCURRENT_FETCHES: usize = 30;     // Fast concurrent fetches
const AUDITOR_INTERVAL_MS: u64 = 1500;        // 1.5s high-frequency audit cycle
const MAX_TRACKED_TOKENS: usize = 300;        // Track up to 300 active tokens in RAM
const TOKEN_DECAY_MS: i64 = 600_000;          // 10-minute activity decay
const DEFAULT_TRADE_COOLDOWN_MS: u64 = 500;   // 500ms cooldown per specific token arb

pub struct TradeLockTable {
    inner: DashMap<String, AtomicU64>,
}

impl TradeLockTable {
    pub fn new() -> Self {
        Self {
            inner: DashMap::with_capacity(2048),
        }
    }

    #[inline(always)]
    pub fn try_acquire_trade_lock(&self, token_mint: &str, cooldown_ms: u64) -> bool {
        let now = Utc::now().timestamp_millis() as u64;
        let new_expiry = now.saturating_add(cooldown_ms);

        let atomic = self.inner
            .entry(token_mint.to_string())
            .or_insert_with(|| AtomicU64::new(0));

        let current_expiry = atomic.load(Ordering::Relaxed);
        if now <= current_expiry {
            return false;
        }

        atomic.compare_exchange_weak(current_expiry, new_expiry, Ordering::SeqCst, Ordering::Relaxed).is_ok()
    }
}

pub struct ArbitrageScanner {
    http_client: Client,
    paper_engine: Arc<PaperTradingEngine>,
    live_engine: Option<Arc<LiveTradingEngine>>,
    pub dual_leg_engine: Option<Arc<DualLegLiveEngine>>,
    pub triangular_engine: Option<Arc<TriangularLiveEngine>>,
    pub migration_sniper: Option<Arc<RaydiumMigrationSniper>>,
    pub tracking_registry: Arc<DashMap<String, VolatileTrackedToken>>,
    pub pool_to_mint: Arc<DashMap<String, String>>,
    /// V42 REFLEX: per-pool trigger dedupe (last slot seen, last dispatch ms) so a
    /// single pool update fires at most one probe per interval while still allowing
    /// rapid-fire retries on genuinely hot spreads.
    pub trigger_throttle: Arc<DashMap<String, (u64, i64)>>,
    /// V42b LIVE RESERVES: pools already vault-decoded (pool -> ()).
    pub pool_meta_registered: Arc<DashMap<String, ()>>,
    /// pool -> decoded (mint_a, mint_b, exact_ratio_pricing) from its payload.
    pub pool_mints: Arc<DashMap<String, (String, String, bool)>>,
    /// vault address -> (pool address, side 0|1) routing for reserve updates.
    pub vault_side: Arc<DashMap<String, (String, u8)>>,
    /// pool -> [(amount_raw, ts_ms); 2] — chain-truth reserves per side.
    pub live_reserves: Arc<DashMap<String, [(u64, i64); 2]>>,
    /// token/quote mint -> decimals (admission-time fetch; WSOL/USDC seeded).
    pub mint_decimals: Arc<DashMap<String, u8>>,
    /// per-pool [LIVE-PX] log throttle (pool -> last log ms).
    pub px_log_throttle: Arc<DashMap<String, i64>>,
    pub trade_locks: Arc<TradeLockTable>,
    pub tokens_tracked: Arc<AtomicUsize>,
    pub pools_scanned: Arc<AtomicUsize>,
    pub arbs_checked: Arc<AtomicUsize>,
    pub opps_found: Arc<AtomicUsize>,
    cached_sol_price: Arc<AtomicU64>,
    max_position_sol: f64,
    min_net_profit_sol: f64,
    min_spread_pct: f64,
}

impl ArbitrageScanner {
    pub fn new(
        paper_engine: Arc<PaperTradingEngine>,
        live_engine: Option<Arc<LiveTradingEngine>>,
        max_position_sol: f64,
        min_net_profit_sol: f64,
        min_spread_pct: f64,
    ) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_millis(3000))
            .pool_max_idle_per_host(32)
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .build()
            .expect("Failed to build HTTP client");

        let scanner = Self {
            http_client,
            paper_engine,
            live_engine,
            dual_leg_engine: None,
            triangular_engine: None,
            migration_sniper: None,
            tracking_registry: Arc::new(DashMap::new()),
            pool_to_mint: Arc::new(DashMap::new()),
            trigger_throttle: Arc::new(DashMap::new()),
            pool_meta_registered: Arc::new(DashMap::new()),
            pool_mints: Arc::new(DashMap::new()),
            vault_side: Arc::new(DashMap::new()),
            live_reserves: Arc::new(DashMap::new()),
            mint_decimals: Arc::new(DashMap::new()),
            px_log_throttle: Arc::new(DashMap::new()),
            trade_locks: Arc::new(TradeLockTable::new()),
            tokens_tracked: Arc::new(AtomicUsize::new(0)),
            pools_scanned: Arc::new(AtomicUsize::new(0)),
            arbs_checked: Arc::new(AtomicUsize::new(0)),
            opps_found: Arc::new(AtomicUsize::new(0)),
            cached_sol_price: Arc::new(AtomicU64::new((95.0 * 100.0) as u64)),
            max_position_sol,
            min_net_profit_sol,
            min_spread_pct,
        };
        // Known-quote decimals seeded; token decimals fetched at admission.
        scanner.mint_decimals.insert(crate::pool_decoder::WSOL_MINT.to_string(), 9);
        scanner.mint_decimals.insert(crate::pool_decoder::USDC_MINT.to_string(), 6);
        scanner
    }

    /// V42b: one-time vault registration for a pool whose LaserStream payload just
    /// arrived. Decodes mints + vault addresses (self-validating), wires the vaults
    /// into the reserve feed and subscribes them for future notifications.
    fn register_pool_from_payload(&self, pool: &str, owner: &str, data_b64: &str) {
        match crate::pool_decoder::decode_pool_account(owner, data_b64) {
            Some(decoded) => {
                self.pool_mints.insert(
                    pool.to_string(),
                    (
                        decoded.mint_a.clone(),
                        decoded.mint_b.clone(),
                        decoded.exact_ratio_pricing,
                    ),
                );
                self.live_reserves.insert(pool.to_string(), [(0u64, 0i64); 2]);
                for (side, vault) in [&decoded.vault_a, &decoded.vault_b].iter().enumerate() {
                    self.vault_side
                        .insert(vault.to_string(), (pool.to_string(), side as u8));
                    crate::helius_stream::register_data_account(vault);
                }
                info!(
                    "🧩 [LIVE RESERVES] {} wired | owner {}… | mintA {}… mintB {}… | vaults subscribed",
                    &pool[..10.min(pool.len())],
                    &owner[..6.min(owner.len())],
                    &decoded.mint_a[..8.min(decoded.mint_a.len())],
                    &decoded.mint_b[..8.min(decoded.mint_b.len())]
                );
            }
            None => {
                crate::pool_decoder::warn_once_decode_fail(owner);
            }
        }
        self.pool_meta_registered.insert(pool.to_string(), ());
    }

    pub fn set_dual_leg_engine(&mut self, dual: Arc<DualLegLiveEngine>) {
        self.dual_leg_engine = Some(dual);
    }

    pub fn set_triangular_engine(&mut self, tri: Arc<TriangularLiveEngine>) {
        self.triangular_engine = Some(tri);
    }

    pub fn set_migration_sniper(&mut self, sniper: Arc<RaydiumMigrationSniper>) {
        self.migration_sniper = Some(sniper);
    }

    #[inline(always)]
    pub fn get_sol_price(&self) -> f64 {
        self.cached_sol_price.load(Ordering::Relaxed) as f64 / 1000.0
    }

    pub async fn update_sol_price(&self) {
        if let Ok(resp) = self.http_client.get("https://api.binance.com/api/v3/ticker/price?symbol=SOLUSDT").send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(price_str) = json.get("price").and_then(|p| p.as_str()) {
                    if let Ok(price) = price_str.parse::<f64>() {
                        self.cached_sol_price.store((price * 1000.0) as u64, Ordering::Relaxed);
                    }
                }
            }
        }
    }

    /// Fetches all active DEX pools for a given mint from DexScreener
    pub async fn fetch_live_pools_batched(&self, mints: &[String]) -> HashMap<String, (String, String, f64, f64, Vec<DynamicPoolState>)> {
        let mut results: HashMap<String, (String, String, f64, f64, Vec<DynamicPoolState>)> = HashMap::new();
        if mints.is_empty() {
            return results;
        }

        let now_ms = Utc::now().timestamp_millis();
        let sol_price_usd = (self.cached_sol_price.load(Ordering::Relaxed) as f64 / 1000.0).max(50.0);

        for chunk in mints.chunks(30) {
            let joined_mints = chunk.join(",");
            let url = format!("https://api.dexscreener.com/latest/dex/tokens/{}", joined_mints);

            let resp = match self.http_client.get(&url).send().await {
                Ok(r) => r,
                Err(_) => continue,
            };

            let json = match resp.json::<serde_json::Value>().await {
                Ok(j) => j,
                Err(_) => continue,
            };

            let pairs = match json.get("pairs").and_then(|p| p.as_array()) {
                Some(p) => p,
                None => continue,
            };

            for p in pairs {
                let base_mint = match p.get("baseToken").and_then(|b| b.get("address")).and_then(|a| a.as_str()) {
                    Some(m) => m.to_string(),
                    None => continue,
                };

                let quote_symbol = p.get("quoteToken").and_then(|q| q.get("symbol")).and_then(|s| s.as_str()).unwrap_or("");
                let quote_address = p.get("quoteToken").and_then(|q| q.get("address")).and_then(|a| a.as_str()).unwrap_or("");
                let is_sol_quote = quote_symbol == "SOL" || quote_symbol == "WSOL" || quote_address == "So11111111111111111111111111111111111111112";
                if !is_sol_quote {
                    continue;
                }

                let quote_amt = p.get("liquidity").and_then(|l| l.get("quote")).and_then(|q| q.as_f64()).unwrap_or(0.0);
                if quote_amt < 10.0 {
                    continue;
                }

                let real_liq_usd = quote_amt * sol_price_usd * 2.0;

                let symbol = p.get("baseToken").and_then(|b| b.get("symbol")).and_then(|s| s.as_str()).unwrap_or("TOKEN").to_string();
                let name = p.get("baseToken").and_then(|b| b.get("name")).and_then(|n| n.as_str()).unwrap_or("Token").to_string();
                let mcap = p.get("marketCap").and_then(|m| m.as_f64()).unwrap_or(0.0);
                let fdv = p.get("fdv").and_then(|f| f.as_f64()).unwrap_or(0.0);
                let effective_mcap = if mcap > 0.0 { mcap } else { fdv };
                let vol_5m = p.get("volume").and_then(|v| v.get("m5")).and_then(|m| m.as_f64()).unwrap_or(0.0);

                let dex_id = p.get("dexId").and_then(|d| d.as_str()).unwrap_or("");
                let dex = if dex_id.contains("raydium") {
                    "RAYDIUM".to_string()
                } else if dex_id.contains("meteora") {
                    "METEORA".to_string()
                } else if dex_id.contains("orca") {
                    "ORCA".to_string()
                } else if dex_id.contains("pump") || dex_id.contains("moonshot") {
                    "PUMPSWAP".to_string()
                } else {
                    continue;
                };

                let price_usd = match p.get("priceUsd").and_then(|pu| pu.as_str()).and_then(|s| s.parse::<f64>().ok()) {
                    Some(pr) if pr > 0.0 && pr < 100000.0 => pr,
                    _ => continue,
                };

                let pool = DynamicPoolState {
                    mint: base_mint.clone(),
                    dex: dex.clone(),
                    pool_address: p.get("pairAddress").and_then(|pa| pa.as_str()).unwrap_or("").to_string(),
                    price_usd,
                    liquidity_usd: real_liq_usd,
                    last_update_ms: now_ms,
                    slot: 0,
                    risk_meta: TokenRiskMeta::safe_default(&base_mint),
                    is_active: true,
                };

                let entry = results.entry(base_mint.clone()).or_insert_with(|| {
                    (symbol.clone(), name.clone(), effective_mcap, 0.0, Vec::new())
                });

                entry.3 += vol_5m;
                if let Some(existing_idx) = entry.4.iter().position(|x| x.dex == pool.dex) {
                    if pool.liquidity_usd > entry.4[existing_idx].liquidity_usd {
                        entry.4[existing_idx] = pool;
                    }
                } else {
                    entry.4.push(pool);
                }
            }
        }

        results
    }

    pub async fn fetch_live_pools(&self, mint: &str) -> (String, String, f64, f64, Vec<DynamicPoolState>) {
        let batched = self.fetch_live_pools_batched(&[mint.to_string()]).await;
        batched.into_iter().next().map(|(_, v)| v).unwrap_or(("TOKEN".to_string(), "Token".to_string(), 0.0, 0.0, Vec::new()))
    }

    /// Evaluates ALL pairs of active DEX pools (Linear + 3-Hop Triangular)
    /// V42b: overlay chain-truth price/liquidity onto pool copies where live vault
    /// feeds are fresh. Constant-product pools get exact prices; concentrated pools
    /// (CLMM/DLMM) keep estimate prices but gain TRUE liquidity from vault totals.
    fn apply_live_reserves(&self, token_mint: &str, symbol: &str, pools: &mut [DynamicPoolState]) {
        const LIVE_TTL_MS: i64 = 15_000;
        const DRIFT_SANITY_X: f64 = 50.0; // decoded price >50x off estimate ⇒ bad decode, ignore
        let sol_price = self.get_sol_price();
        let now_ms = chrono::Utc::now().timestamp_millis();

        for p in pools.iter_mut() {
            let arr = match self.live_reserves.get(&p.pool_address) {
                Some(a) => *a.value(),
                None => continue,
            };
            let (amt_a, ts_a) = arr[0];
            let (amt_b, ts_b) = arr[1];
            if amt_a == 0 || amt_b == 0 || now_ms - ts_a > LIVE_TTL_MS || now_ms - ts_b > LIVE_TTL_MS
            {
                continue;
            }
            let (minta, mintb, exact) = match self.pool_mints.get(&p.pool_address) {
                Some(m) => m.value().clone(),
                None => continue,
            };

            // Identify token vs quote side from decoded mints.
            let (quote_mint, quote_amt, token_amt) = if minta == token_mint {
                (mintb.clone(), amt_b, amt_a)
            } else if mintb == token_mint {
                (minta.clone(), amt_a, amt_b)
            } else {
                continue;
            };
            let quote_usd = match quote_mint.as_str() {
                crate::pool_decoder::WSOL_MINT => quote_amt as f64 / 1e9 * sol_price,
                crate::pool_decoder::USDC_MINT => quote_amt as f64 / 1e6,
                _ => continue,
            };

            // TRUE total liquidity (valid for every flavor — vaults hold everything).
            let liq_usd = 2.0 * quote_usd;

            if exact {
                if let Some(dec) = self.mint_decimals.get(token_mint) {
                    if let Some(real_px) = crate::pool_decoder::token_price_usd(
                        &quote_mint,
                        quote_amt,
                        token_amt,
                        *dec,
                        sol_price,
                    ) {
                        let sane = p.price_usd <= 0.0
                            || (real_px / p.price_usd < DRIFT_SANITY_X
                                && p.price_usd / real_px < DRIFT_SANITY_X);
                        if sane && real_px > 0.0 {
                            let drift = if p.price_usd > 0.0 {
                                (real_px / p.price_usd - 1.0).abs()
                            } else {
                                0.0
                            };
                            if drift > 0.25 {
                                let mut th = self.px_log_throttle.entry(p.pool_address.clone()).or_insert(0);
                                if now_ms - *th > 10_000 {
                                    *th = now_ms;
                                    info!(
                                        "🎯 [LIVE-PX] {} {} real ${:.8} vs est ${:.8} ({:+.0}%) | liq ${:.0}",
                                        symbol, &p.dex, real_px, p.price_usd, drift * 100.0, liq_usd
                                    );
                                }
                            }
                            p.price_usd = real_px;
                        }
                    }
                }
            }
            p.liquidity_usd = liq_usd;
            p.last_update_ms = now_ms;
        }
    }

    pub async fn evaluate_all_pool_pairs(
        &self,
        mint: &str,
        symbol: &str,
        pools: &[DynamicPoolState],
        sol_price_usd: f64,
    ) {
        if pools.len() < 2 {
            return;
        }

        // V42b: overlay chain-truth reserves before any spread math.
        let mut corrected: Vec<DynamicPoolState> = pools.to_vec();
        self.apply_live_reserves(mint, symbol, &mut corrected);
        let pools: &[DynamicPoolState] = &corrected;

        // 1. Evaluate 3-Hop Triangular Spatial Arbitrage in the Whale-Exclusion Zone ($3k-$40k TVL)
        self.evaluate_triangular_arbitrage(mint, symbol, pools, sol_price_usd).await;

        // 2. Evaluate Linear 2-Leg Arbitrage — collect all, rank by spread, fire fattest first
        let mut ranked_opps: Vec<ArbitrageOpportunity> = Vec::new();
        for i in 0..pools.len() {
            for j in (i + 1)..pools.len() {
                self.arbs_checked.fetch_add(1, Ordering::Relaxed);
                if let Some(opp) = evaluate_optimal_arbitrage(
                    mint, symbol, &pools[i], &pools[j],
                    sol_price_usd, self.min_net_profit_sol, self.max_position_sol, self.min_spread_pct,
                ) {
                    ranked_opps.push(opp);
                }
            }
        }

        // Sort by net spread descending — execute fattest spread first (zero latency cost)
        ranked_opps.sort_by(|a, b| b.net_spread_pct.partial_cmp(&a.net_spread_pct).unwrap_or(std::cmp::Ordering::Equal));

        for opp in ranked_opps.iter().take(1) {
            if !self.trade_locks.try_acquire_trade_lock(mint, DEFAULT_TRADE_COOLDOWN_MS) {
                continue;
            }

            self.opps_found.fetch_add(1, Ordering::Relaxed);

            info!(
                "🔥 [CROSS-DEX ARB FILLED] {} | {} vs {} | Size: {:.3} SOL | Net Profit: +{:.5} SOL (${:.2}) | Spread: +{:.2}%",
                symbol, opp.buy_dex, opp.sell_dex, opp.simulated_swap_sol, opp.net_profit_sol, opp.net_profit_usd, opp.net_spread_pct
            );

            // Always execute shadow paper trade for simulation runway
            self.paper_engine.execute_shadow_trade(&opp, sol_price_usd);

            // If live engine is active and enabled, also attempt real atomic execution.
            // V40 GUARD: spawned + time-bounded so one hung engine await can never
            // park the scanner's auditor/consumer loops again (zombie-freeze fix).
            if let Some(ref live) = self.live_engine {
                let engine = Arc::clone(live);
                let opp_owned = opp.clone();
                tokio::spawn(async move {
                    if tokio::time::timeout(Duration::from_secs(60), engine.execute_live_arbitrage(&opp_owned, sol_price_usd)).await.is_err() {
                        warn!("⏱️ [ENGINE GUARD] Live execution exceeded 60s — aborted to keep scanner alive");
                    }
                });
            }
            if let Some(ref dual) = self.dual_leg_engine {
                let engine = Arc::clone(dual);
                let opp_owned = opp.clone();
                tokio::spawn(async move {
                    if tokio::time::timeout(Duration::from_secs(60), engine.execute_dual_leg_arbitrage(&opp_owned, sol_price_usd)).await.is_err() {
                        warn!("⏱️ [ENGINE GUARD] Dual-leg execution exceeded 60s — aborted to keep scanner alive");
                    }
                });
            }
        }
    }

    /// Evaluates 3-Hop Triangular Spatial Arbitrage: SOL -> Token -> USDC -> SOL
    pub async fn evaluate_triangular_arbitrage(
        &self,
        mint: &str,
        symbol: &str,
        pools: &[DynamicPoolState],
        sol_price_usd: f64,
    ) {
        use crate::triangular_math::{evaluate_triangular_route, calculate_dynamic_jito_tip, PoolReserveState};

        let target_size_sol = crate::types::TARGET_TRADE_SIZE_SOL; // 0.10 SOL ($10 USD target trade size)
        let sol_in_lamports = (target_size_sol * 1e9) as u64;

        // Find candidate pools in the Whale-Scalper Zone ($5k to $3M TVL)
        let whale_zone_pools: Vec<&DynamicPoolState> = pools.iter().filter(|p| {
            p.liquidity_usd >= 5000.0 && p.liquidity_usd <= 3_000_000.0
        }).collect();

        if whale_zone_pools.is_empty() {
            return;
        }

        // Compare Raydium vs Meteora/Orca across SOL <-> Token <-> USDC (Exclude Pump.fun 1% bonding curves)
        for pool1 in pools.iter() {
            if pool1.dex.to_lowercase().contains("pump") { continue; }
            for pool2 in pools.iter() {
                if pool2.dex.to_lowercase().contains("pump") || pool1.dex == pool2.dex {
                    continue;
                }

                // Leg 1: SOL -> Token (Pool 1)
                let r_sol_1 = ((pool1.liquidity_usd / 2.0) / sol_price_usd * 1e9) as u64;
                let r_tok_1 = if pool1.price_usd > 0.0 { ((pool1.liquidity_usd / 2.0) / pool1.price_usd * 1e9) as u64 } else { 0 };
                let leg1 = PoolReserveState::new(r_sol_1.max(1_000_000), r_tok_1.max(1_000_000), 25);

                // Leg 2: Token -> USDC (Pool 2)
                let r_tok_2 = if pool2.price_usd > 0.0 { ((pool2.liquidity_usd / 2.0) / pool2.price_usd * 1e9) as u64 } else { 0 };
                let r_usdc_2 = ((pool2.liquidity_usd / 2.0) * 1e6) as u64;
                let leg2 = PoolReserveState::new(r_tok_2.max(1_000_000), r_usdc_2.max(1_000_000), 30);

                // Leg 3: USDC -> SOL (Deep Benchmark Pool e.g. Orca Whirlpool 1 SOL = sol_price_usd USDC)
                let r_usdc_3 = 500_000 * 1_000_000; // Deep 500k USDC
                let r_sol_3 = ((500_000.0 / sol_price_usd) * 1e9) as u64;
                let leg3 = PoolReserveState::new(r_usdc_3, r_sol_3, 5); // 0.05% fee on deep pool

                if let Some((final_sol_out, gross_profit_lamports, spread_pct)) = evaluate_triangular_route(sol_in_lamports, &leg1, &leg2, &leg3) {
                    // V42 bounds: upper spread ceiling raised 20% -> 50% (MRiY calibration:
                    // genuinely executable ~14% spreads were being blanket-rejected). The
                    // atomic slippage thresholds remain the real phantom-spread arbiter.
                    if gross_profit_lamports > 30_000 && gross_profit_lamports <= 10_000_000 && spread_pct >= 1.40 && spread_pct <= 50.0 {
                        let dynamic_tip = calculate_dynamic_jito_tip(gross_profit_lamports as u64);
                        let net_profit_lamports = gross_profit_lamports.saturating_sub(dynamic_tip as i64).saturating_sub(50_000); // 50k gas
                        
                        if net_profit_lamports > 20_000 {
                            let net_profit_sol = net_profit_lamports as f64 / 1e9;
                            let net_profit_usd = net_profit_sol * sol_price_usd;

                            if self.trade_locks.try_acquire_trade_lock(mint, DEFAULT_TRADE_COOLDOWN_MS) {
                                self.opps_found.fetch_add(1, Ordering::Relaxed);
                                
                                info!("🔺 [TRIANGULAR SPATIAL ARB] {} | {} ➔ {} ➔ ORCA_USDC | Spread: +{:.2}% | Profit: +{:.5} SOL (${:.2}) | Tip: {:.5} SOL | TVL: ${:.0}",
                                    symbol, pool1.dex, pool2.dex, spread_pct, net_profit_sol, net_profit_usd, (dynamic_tip as f64 / 1e9), pool1.liquidity_usd
                                );

                                let opp = crate::types::ArbitrageOpportunity {
                                    timestamp_ms: Utc::now().timestamp_millis(),
                                    iso_time: Utc::now().to_rfc3339(),
                                    mint: mint.to_string(),
                                    symbol: format!("TRI/{}", symbol),
                                    name: format!("{} (Triangular 3-Hop)", symbol),
                                    buy_dex: pool1.dex.clone(),
                                    sell_dex: format!("{}->USDC", pool2.dex),
                                    target_dex: "ORCA_USDC".to_string(),
                                    pair_address: pool1.pool_address.clone(),
                                    pump_price_usd: pool1.price_usd,
                                    raydium_price_usd: pool2.price_usd,
                                    gross_spread_pct: spread_pct,
                                    net_spread_pct: spread_pct - 0.75, // Deduct 3 swap fees
                                    pool_liquidity_usd: pool1.liquidity_usd,
                                    simulated_swap_sol: target_size_sol,
                                    gross_profit_usd: net_profit_usd + (dynamic_tip as f64 / 1e9 * sol_price_usd),
                                    jito_tip_usd: (dynamic_tip as f64 / 1e9) * sol_price_usd,
                                    net_profit_usd,
                                    net_profit_sol,
                                };

                                self.paper_engine.execute_shadow_trade(&opp, sol_price_usd);

                                // V40 GUARD: detached + time-bounded execution (zombie-freeze fix)
                                if let Some(ref live) = self.live_engine {
                                    let engine = Arc::clone(live);
                                    let mint_owned = mint.to_string();
                                    let sym_owned = symbol.to_string();
                                    let dex1 = pool1.dex.clone();
                                    let dex2 = pool2.dex.clone();
                                    let liq = pool1.liquidity_usd;
                                    tokio::spawn(async move {
                                        if tokio::time::timeout(Duration::from_secs(60), engine.execute_triangular_arbitrage(&mint_owned, &sym_owned, &dex1, &dex2, "ORCA", liq, sol_price_usd)).await.is_err() {
                                            warn!("⏱️ [ENGINE GUARD] Live triangular execution exceeded 60s — aborted to keep scanner alive");
                                        }
                                    });
                                }
                                if let Some(ref tri) = self.triangular_engine {
                                    let engine = Arc::clone(tri);
                                    let mint_owned = mint.to_string();
                                    let sym_owned = symbol.to_string();
                                    let dex1 = pool1.dex.clone();
                                    let dex2 = pool2.dex.clone();
                                    let liq = pool1.liquidity_usd;
                                    tokio::spawn(async move {
                                        if tokio::time::timeout(Duration::from_secs(60), engine.execute_triangular_arbitrage(&mint_owned, &sym_owned, &dex1, &dex2, "ORCA", liq, sol_price_usd)).await.is_err() {
                                            warn!("⏱️ [ENGINE GUARD] Triangular execution exceeded 60s — aborted to keep scanner alive");
                                        }
                                    });
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    pub async fn handle_onchain_swap(&self, swap: LiveSwapEvent, sol_price_usd: f64) {
        self.pools_scanned.fetch_add(1, Ordering::Relaxed);

        let current_sol_price = if sol_price_usd > 10.0 { sol_price_usd } else { self.get_sol_price() };

        if let Some(mut token) = self.tracking_registry.get_mut(&swap.mint) {
            let price_in_sol = if swap.v_tokens_in_bonding_curve > 0.0 {
                swap.v_sol_in_bonding_curve / swap.v_tokens_in_bonding_curve
            } else if swap.token_amount > 0.0 {
                swap.sol_amount / swap.token_amount
            } else {
                0.0
            };

            let updated_pump_pool = DynamicPoolState {
                mint: swap.mint.clone(),
                dex: "PUMPSWAP".to_string(),
                pool_address: format!("pump_{}", &swap.mint),
                price_usd: price_in_sol * current_sol_price,
                liquidity_usd: swap.v_sol_in_bonding_curve * current_sol_price * 2.0,
                last_update_ms: Utc::now().timestamp_millis(),
                slot: 0,
                risk_meta: TokenRiskMeta::safe_default(&swap.mint),
                is_active: true,
            };

            let mut found = false;
            for p in token.pools.iter_mut() {
                if p.dex == "PUMPSWAP" {
                    *p = updated_pump_pool.clone();
                    found = true;
                    break;
                }
            }
            if !found {
                token.pools.push(updated_pump_pool);
            }

            let pools_snapshot = token.pools.clone();
            let symbol = token.symbol.clone();
            let mint = token.key().clone();
            drop(token);

            self.evaluate_all_pool_pairs(&mint, &symbol, &pools_snapshot, sol_price_usd).await;
        }
    }

    pub async fn handle_raydium_migration(&self, mig: RaydiumMigrationEvent) {
        info!("🎉 [MIGRATION DETECTED] Mint: {} -> Raydium Pool: {} (Initial SOL: {:.2})", 
            mig.mint, mig.pool_address, mig.initial_sol_liquidity
        );

        if let Some(sniper) = &self.migration_sniper {
            sniper.on_migration_detected(mig.clone()).await;
        }

        let (symbol, name, mcap, vol, pools) = self.fetch_live_pools(&mig.mint).await;
        if pools.len() >= 2 {
            let total_liq: f64 = pools.iter().map(|p| p.liquidity_usd).sum();
            
            // 🎯 Exclude pools outside the volatile micro-cap zone ($2.5k - $60k)
            if total_liq < crate::types::MIN_VOLATILE_LIQUIDITY_USD || total_liq > crate::types::MAX_VOLATILE_LIQUIDITY_USD {
                return;
            }

            let vol_score = if total_liq > 0.0 { (vol / total_liq) * 100.0 } else { 0.0 };

            let pools_clone = pools.clone();
            let token = VolatileTrackedToken {
                mint: mig.mint.clone(),
                symbol: symbol.clone(),
                name,
                market_cap_usd: mcap,
                volume_5m_usd: vol,
                total_liquidity_usd: total_liq,
                volatility_score: vol_score,
                pools,
                added_timestamp_ms: mig.timestamp_ms,
                last_spread_pct: 0.0,
                consecutive_flat_ticks: 0,
            };

            self.tracking_registry.insert(mig.mint.clone(), token);
            self.tokens_tracked.store(self.tracking_registry.len(), Ordering::Relaxed);
            // V42 REFLEX: subscribe the new pools for LaserStream triggers.
            for p in &pools_clone {
                if !p.pool_address.is_empty() {
                    self.pool_to_mint.insert(p.pool_address.clone(), mig.mint.clone());
                    crate::helius_stream::register_trigger_pool(&p.pool_address);
                }
            }
            info!("🔥 [VOLATILE GRADUATED ADMITTED] {} added to tracking registry!", symbol);

            // Fast-track immediate migration arbitrage evaluation
            self.evaluate_all_pool_pairs(&mig.mint, &symbol, &pools_clone, self.get_sol_price()).await;
        }
    }

    // =========================================================================
    // 🔎 MULTI-SOURCE DISCOVERY SCOUT (Raydium Official API + DexScreener Fallback)
    // =========================================================================
    async fn run_discovery_scout(&self) {
        let mut candidate_mints: HashSet<String> = HashSet::new();

        // 1. Raydium Official Pools API (Pages 1 to 5: Volume & Liquidity sorted)
        for page in 1..=5 {
            let r_ep = format!("https://api-v3.raydium.io/pools/info/list?poolType=all&poolSortField=volume24h&sortType=desc&pageSize=100&page={}", page);
            if let Ok(resp) = self.http_client.get(&r_ep).send().await {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if let Some(data_list) = body.get("data").and_then(|d| d.get("data")).and_then(|d| d.as_array()) {
                        for p in data_list {
                            let mint_a = p.get("mintA").and_then(|m| m.get("address")).and_then(|a| a.as_str()).unwrap_or("");
                            let mint_b = p.get("mintB").and_then(|m| m.get("address")).and_then(|a| a.as_str()).unwrap_or("");
                            let wsol = "So11111111111111111111111111111111111111112";

                            if !mint_a.is_empty() && mint_a != wsol {
                                candidate_mints.insert(mint_a.to_string());
                            }
                            if !mint_b.is_empty() && mint_b != wsol {
                                candidate_mints.insert(mint_b.to_string());
                            }
                        }
                    }
                }
            }
        }

        // 2. Meteora DLMM Active Pairs API
        if let Ok(resp) = self.http_client.get("https://dlmm-api.meteora.ag/pair/all_by_groups").send().await {
            if let Ok(groups) = resp.json::<Vec<serde_json::Value>>().await {
                for g in groups.iter().take(50) {
                    if let Some(pairs) = g.get("pairs").and_then(|p| p.as_array()) {
                        for p in pairs {
                            let mint_x = p.get("mint_x").and_then(|m| m.as_str()).unwrap_or("");
                            let mint_y = p.get("mint_y").and_then(|m| m.as_str()).unwrap_or("");
                            let wsol = "So11111111111111111111111111111111111111112";
                            if !mint_x.is_empty() && mint_x != wsol {
                                candidate_mints.insert(mint_x.to_string());
                            }
                            if !mint_y.is_empty() && mint_y != wsol {
                                candidate_mints.insert(mint_y.to_string());
                            }
                        }
                    }
                }
            }
        }

        // 3. Jupiter Verified & Community Token Registry (expanded to 1000)
        if let Ok(resp) = self.http_client.get("https://tokens.jup.ag/tokens?tags=verified,community").send().await {
            if let Ok(tokens) = resp.json::<Vec<serde_json::Value>>().await {
                for t in tokens.iter().take(1000) {
                    if let Some(addr) = t.get("address").and_then(|a| a.as_str()) {
                        if addr != "So11111111111111111111111111111111111111112" {
                            candidate_mints.insert(addr.to_string());
                        }
                    }
                }
            }
        }

        // 4. DexScreener Boosts & Search Endpoints
        let discovery_endpoints = [
            "https://api.dexscreener.com/token-boosts/top/v1",
            "https://api.dexscreener.com/token-boosts/latest/v1",
            "https://api.dexscreener.com/token-profiles/latest/v1",
            "https://api.dexscreener.com/latest/dex/search?q=solana",
            "https://api.dexscreener.com/latest/dex/search?q=raydium",
            "https://api.dexscreener.com/latest/dex/search?q=meteora",
            "https://api.dexscreener.com/latest/dex/search?q=pump",
            "https://api.dexscreener.com/latest/dex/search?q=orca",
        ];

        for ep in discovery_endpoints {
            if let Ok(resp) = self.http_client.get(ep).send().await {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if let Some(items) = body.as_array() {
                        for item in items {
                            let chain = item.get("chainId").and_then(|c| c.as_str()).unwrap_or("");
                            if chain == "solana" {
                                if let Some(token_addr) = item.get("tokenAddress").and_then(|a| a.as_str()) {
                                    candidate_mints.insert(token_addr.to_string());
                                }
                            }
                        }
                    } else if let Some(pairs) = body.get("pairs").and_then(|p| p.as_array()) {
                        for p in pairs {
                            let chain = p.get("chainId").and_then(|c| c.as_str()).unwrap_or("");
                            if chain == "solana" {
                                if let Some(base_addr) = p.get("baseToken").and_then(|b| b.get("address")).and_then(|a| a.as_str()) {
                                    candidate_mints.insert(base_addr.to_string());
                                }
                            }
                        }
                    }
                }
            }
            sleep(Duration::from_millis(50)).await;
        }

        // 5. GeckoTerminal Solana Trending Pools (free, no API key)
        let gecko_endpoints = [
            "https://api.geckoterminal.com/api/v2/networks/solana/trending_pools?page=1",
            "https://api.geckoterminal.com/api/v2/networks/solana/trending_pools?page=2",
            "https://api.geckoterminal.com/api/v2/networks/solana/trending_pools?page=3",
        ];
        for ep in gecko_endpoints {
            if let Ok(resp) = self.http_client.get(ep)
                .header("Accept", "application/json")
                .timeout(Duration::from_millis(2000))
                .send().await
            {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                        for pool in data {
                            // Extract base token address from relationships
                            if let Some(base_addr) = pool.get("relationships")
                                .and_then(|r| r.get("base_token"))
                                .and_then(|bt| bt.get("data"))
                                .and_then(|d| d.get("id"))
                                .and_then(|id| id.as_str())
                            {
                                // GeckoTerminal IDs are "solana_<address>"
                                let addr = base_addr.strip_prefix("solana_").unwrap_or(base_addr);
                                if addr != "So11111111111111111111111111111111111111112" && addr.len() > 30 {
                                    candidate_mints.insert(addr.to_string());
                                }
                            }
                        }
                    }
                }
            }
            sleep(Duration::from_millis(50)).await;
        }

        // 6. Raydium Liquidity-Sorted Pages (catches deep pools missed by volume sort)
        for page in 1..=3 {
            let r_liq = format!("https://api-v3.raydium.io/pools/info/list?poolType=all&poolSortField=liquidity&sortType=desc&pageSize=100&page={}", page);
            if let Ok(resp) = self.http_client.get(&r_liq).timeout(Duration::from_millis(2000)).send().await {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if let Some(data_list) = body.get("data").and_then(|d| d.get("data")).and_then(|d| d.as_array()) {
                        for p in data_list {
                            let mint_a = p.get("mintA").and_then(|m| m.get("address")).and_then(|a| a.as_str()).unwrap_or("");
                            let mint_b = p.get("mintB").and_then(|m| m.get("address")).and_then(|a| a.as_str()).unwrap_or("");
                            let wsol = "So11111111111111111111111111111111111111112";
                            if !mint_a.is_empty() && mint_a != wsol { candidate_mints.insert(mint_a.to_string()); }
                            if !mint_b.is_empty() && mint_b != wsol { candidate_mints.insert(mint_b.to_string()); }
                        }
                    }
                }
            }
        }

        let unadmitted: Vec<String> = candidate_mints
            .into_iter()
            .filter(|m| !self.tracking_registry.contains_key(m))
            .take(MAX_TRACKED_TOKENS.saturating_sub(self.tracking_registry.len()))
            .collect();

        if !unadmitted.is_empty() {
            let batched_map = self.fetch_live_pools_batched(&unadmitted).await;

            for (mint, (symbol, name, effective_mcap, total_5m_vol, active_pools)) in batched_map {
                if active_pools.len() >= 2 && total_5m_vol >= MIN_ADMISSION_5M_VOL_USD {
                    let total_liq: f64 = active_pools.iter().map(|p| p.liquidity_usd).sum();
                    
                    // 🎯 VOLATILE MICRO-CAP MOAT: Only admit tokens in $1,000 <= Liquidity <= $60,000
                    if total_liq < crate::types::MIN_VOLATILE_LIQUIDITY_USD || total_liq > crate::types::MAX_VOLATILE_LIQUIDITY_USD {
                        continue;
                    }

                    let vol_score = if total_liq > 0.0 { (total_5m_vol / total_liq) * 100.0 } else { 0.0 };

                    let token = VolatileTrackedToken {
                        mint: mint.clone(),
                        symbol: symbol.clone(),
                        name,
                        market_cap_usd: effective_mcap,
                        volume_5m_usd: total_5m_vol,
                        total_liquidity_usd: total_liq,
                        volatility_score: vol_score,
                        pools: active_pools.clone(),
                        added_timestamp_ms: Utc::now().timestamp_millis(),
                        last_spread_pct: 0.0,
                        consecutive_flat_ticks: 0,
                    };

                    for p in &active_pools {
                        if !p.pool_address.is_empty() {
                            self.pool_to_mint.insert(p.pool_address.clone(), mint.clone());
                            // V42 REFLEX: subscribe this pool for LaserStream triggers.
                            crate::helius_stream::register_trigger_pool(&p.pool_address);
                        }
                    }

                    self.tracking_registry.insert(mint, token);
                    self.tokens_tracked.store(self.tracking_registry.len(), Ordering::Relaxed);
                    info!("🔥 [VOLATILE SCOUT ADMITTED] {} | Vol: ${:.0} | Liq: ${:.0} | Pools: {} -> Total Tracked: {}", symbol, total_5m_vol, total_liq, active_pools.len(), self.tracking_registry.len());
                }
            }
        }
    }

    // =========================================================================
    // ⚡ UNCONSTRAINED HIGH-FREQUENCY PARALLEL AUDITOR (Runs every 1.5s)
    // =========================================================================
    async fn run_parallel_volatility_auditor(&self) {
        let sol_price_usd = self.get_sol_price();
        let now_ms = Utc::now().timestamp_millis();

        // 1. Stable 10-Minute Lifecycle Policy ($1.0k - $60k TVL)
        let pre_retain_pools: HashSet<String> = self
            .tracking_registry
            .iter()
            .flat_map(|kv| kv.value().pools.iter().map(|p| p.pool_address.clone()).collect::<Vec<_>>())
            .collect();
        self.tracking_registry.retain(|_, token| {
            let total_liq: f64 = token.pools.iter().map(|p| p.liquidity_usd).sum();
            let is_in_whale_moat = total_liq >= crate::types::MIN_VOLATILE_LIQUIDITY_USD && total_liq <= crate::types::MAX_VOLATILE_LIQUIDITY_USD;
            let is_not_expired = (now_ms - token.added_timestamp_ms) < TOKEN_DECAY_MS;
            let has_multi_pools = token.pools.len() >= 2;

            is_in_whale_moat && is_not_expired && has_multi_pools
        });
        self.tokens_tracked.store(self.tracking_registry.len(), Ordering::Relaxed);

        // V42 REFLEX: evicted tokens release their LaserStream subscriptions + reverse
        // lookups (also fixes the old unbounded pool_to_mint leak).
        let retained_pools: HashSet<String> = self
            .tracking_registry
            .iter()
            .flat_map(|kv| kv.value().pools.iter().map(|p| p.pool_address.clone()).collect::<Vec<_>>())
            .collect();
        for pool in pre_retain_pools.difference(&retained_pools) {
            crate::helius_stream::unregister_trigger_pool(pool);
            self.pool_to_mint.remove(pool);
            self.trigger_throttle.remove(pool);
            // V42b: release vault subscriptions + reserve caches for evicted pools.
            let dead_vaults: Vec<String> = self
                .vault_side
                .iter()
                .filter(|kv| kv.value().0 == *pool)
                .map(|kv| kv.key().clone())
                .collect();
            for v in dead_vaults {
                crate::helius_stream::unregister_data_account(&v);
                self.vault_side.remove(&v);
            }
            self.live_reserves.remove(pool);
            self.pool_mints.remove(pool);
            self.pool_meta_registered.remove(pool);
            self.px_log_throttle.remove(pool);
        }

        // 2. Snapshot tracked mints
        let tracked_mints: Vec<String> = self.tracking_registry.iter().map(|kv| kv.key().clone()).collect();
        if tracked_mints.is_empty() {
            self.dump_tracked_tokens_to_disk();
            return;
        }

        // 3. Parallel Batch Fetch using 30-mint chunked requests
        let batched_map = self.fetch_live_pools_batched(&tracked_mints).await;

        // 4. Merge pool updates per-DEX and evaluate all cross-DEX pairs
        for (mint, (symbol, _, _, total_5m_vol, updated_pools)) in batched_map {
            if let Some(mut token_ref) = self.tracking_registry.get_mut(&mint) {
                for new_p in updated_pools {
                    if let Some(existing_p) = token_ref.pools.iter_mut().find(|p| p.dex == new_p.dex) {
                        *existing_p = new_p;
                    } else {
                        token_ref.pools.push(new_p);
                    }
                }
                if total_5m_vol > 0.0 {
                    token_ref.volume_5m_usd = total_5m_vol;
                }

                if token_ref.pools.len() >= 2 {
                    let pools_clone = token_ref.pools.clone();
                    self.evaluate_all_pool_pairs(&mint, &symbol, &pools_clone, sol_price_usd).await;
                }
            }
        }

        // 5. Export real-time tracked tokens & multi-DEX spreads to disk for dashboard consumption
        self.dump_tracked_tokens_to_disk();
    }

    /// Dumps current in-memory tracked tokens, pool matrix, and calculated spreads to data/tracked_tokens.json
    pub fn dump_tracked_tokens_to_disk(&self) {
        let mut summaries: Vec<serde_json::Value> = Vec::new();
        let now_ms = Utc::now().timestamp_millis();

        for kv in self.tracking_registry.iter() {
            let token = kv.value();
            if token.pools.is_empty() {
                continue;
            }

            let mut best_spread = 0.0;
            let mut best_buy_dex = String::new();
            let mut best_sell_dex = String::new();
            let mut best_buy_price = 0.0;
            let mut best_sell_price = 0.0;

            for i in 0..token.pools.len() {
                for j in 0..token.pools.len() {
                    if i == j { continue; }
                    let p_buy = &token.pools[i];
                    let p_sell = &token.pools[j];
                    if p_buy.price_usd > 0.0 && p_sell.price_usd > p_buy.price_usd {
                        let spread = ((p_sell.price_usd - p_buy.price_usd) / p_buy.price_usd) * 100.0;
                        if spread > best_spread {
                            best_spread = spread;
                            best_buy_dex = p_buy.dex.clone();
                            best_sell_dex = p_sell.dex.clone();
                            best_buy_price = p_buy.price_usd;
                            best_sell_price = p_sell.price_usd;
                        }
                    }
                }
            }

            let pool_list: Vec<serde_json::Value> = token.pools.iter().map(|p| {
                serde_json::json!({
                    "dex": p.dex,
                    "price_usd": p.price_usd,
                    "liquidity_usd": p.liquidity_usd,
                    "pool_address": p.pool_address,
                    "last_update_ms": p.last_update_ms
                })
            }).collect();

            let total_liq: f64 = token.pools.iter().map(|p| p.liquidity_usd).sum();

            summaries.push(serde_json::json!({
                "mint": token.mint,
                "symbol": token.symbol,
                "name": token.name,
                "market_cap_usd": token.market_cap_usd,
                "volume_5m_usd": token.volume_5m_usd,
                "total_liquidity_usd": total_liq,
                "pool_count": token.pools.len(),
                "best_spread_pct": (best_spread * 100.0).round() / 100.0,
                "buy_dex": best_buy_dex,
                "sell_dex": best_sell_dex,
                "buy_price_usd": best_buy_price,
                "sell_price_usd": best_sell_price,
                "pools": pool_list,
                "last_update_ms": now_ms
            }));
        }

        // Sort by spread descending — highest spread at top
        summaries.sort_by(|a, b| {
            let sa = a.get("best_spread_pct").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let sb = b.get("best_spread_pct").and_then(|v| v.as_f64()).unwrap_or(0.0);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });

        if let Ok(json_str) = serde_json::to_string(&summaries) {
            let _ = std::fs::create_dir_all("data");
            let _ = std::fs::write("data/tracked_tokens.tmp", &json_str);
            let _ = std::fs::rename("data/tracked_tokens.tmp", "data/tracked_tokens.json");
        }
    }

    pub async fn run(self: Arc<Self>, instance_name: &str) {
        info!("⚡ Starting Scaled Engine [{}] with Target Max Pos: {:.2} SOL | Min Profit: {:.5} SOL", 
            instance_name, self.max_position_sol, self.min_net_profit_sol
        );

        let (swap_tx, mut swap_rx): (tokio::sync::mpsc::Sender<LiveSwapEvent>, Receiver<LiveSwapEvent>) = channel(10000);
        let (mig_tx, mut mig_rx): (tokio::sync::mpsc::Sender<RaydiumMigrationEvent>, Receiver<RaydiumMigrationEvent>) = channel(2000);
        let (helius_tx, mut helius_rx): (tokio::sync::mpsc::Sender<crate::helius_stream::HeliusAccountUpdate>, Receiver<crate::helius_stream::HeliusAccountUpdate>) = channel(50000);

        // 1. Launch Dedicated Credit-Optimized Helius EU LaserStream Worker (1 assigned key per instance)
        crate::helius_stream::HeliusStreamManager::launch_dedicated_instance_worker(helius_tx, instance_name);

        // 1.5 V42 process-wide services: sub-second kill switch watcher + live tip floor feed.
        crate::jito::ensure_killswitch_watcher();
        crate::jito::ensure_tip_floor_fetcher();
        info!("🧰 [V42 REFLEX] Kill-switch watcher + tip-floor feed online | trigger window {}ms", crate::helius_stream::EVENT_FRESH_WINDOW_MS);

        // 1.6 V42b health heartbeat: make the live-reserve feed state observable —
        // if LIVE-PX corrections ever go quiet again, this line says why.
        let hb_self = Arc::clone(&self);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(60)).await;
                let wired = hb_self.pool_mints.len();
                let mut both_sides = 0usize;
                let mut one_side = 0usize;
                for entry in hb_self.live_reserves.iter() {
                    let a_populated = entry.value()[0].1 > 0;
                    let b_populated = entry.value()[1].1 > 0;
                    match (a_populated, b_populated) {
                        (true, true) => both_sides += 1,
                        (true, false) | (false, true) => one_side += 1,
                        _ => {}
                    }
                }
                let dec_known = hb_self.mint_decimals.len();
                let data_subs = crate::helius_stream::registered_data_count();
                info!(
                    "🧮 [LIVE FEED HEALTH] wired:{} both-sides-populated:{} one-side:{} decimals-known:{} data-subs:{}",
                    wired, both_sides, one_side, dec_known, data_subs
                );
            }
        });

        // 2. PumpPortal Fallback Streaming Pipeline
        tokio::spawn(async move {
            PumpPortalFeed::run_streaming_pipeline(swap_tx, mig_tx).await;
        });

        // 3. Sol Price Updater Loop (Every 10 seconds)
        let price_self = Arc::clone(&self);
        tokio::spawn(async move {
            loop {
                price_self.update_sol_price().await;
                sleep(Duration::from_secs(10)).await;
            }
        });

        // 4. Multi-Source Discovery Scout (Every 3 seconds)
        let scout_self = Arc::clone(&self);
        tokio::spawn(async move {
            loop {
                scout_self.run_discovery_scout().await;
                sleep(Duration::from_secs(3)).await;
            }
        });

        // 5. Parallel High-Frequency Volatility Auditor (Every 1.5 seconds)
        let auditor_self = Arc::clone(&self);
        tokio::spawn(async move {
            loop {
                auditor_self.run_parallel_volatility_auditor().await;
                sleep(Duration::from_millis(AUDITOR_INTERVAL_MS)).await;
            }
        });

        // 6. Hot-Path Dedicated Consumers
        let self_swap = Arc::clone(&self);
        let swap_handle = tokio::spawn(async move {
            while let Some(swap) = swap_rx.recv().await {
                let sol_price_usd = self_swap.get_sol_price();
                self_swap.handle_onchain_swap(swap, sol_price_usd).await;
            }
        });

        let self_mig = Arc::clone(&self);
        let mig_handle = tokio::spawn(async move {
            while let Some(migration) = mig_rx.recv().await {
                self_mig.handle_raydium_migration(migration).await;
            }
        });

        let self_helius = Arc::clone(&self);
        let helius_handle = tokio::spawn(async move {
            while let Some(acc_update) = helius_rx.recv().await {
                self_helius.pools_scanned.fetch_add(1, Ordering::Relaxed);

                // V42 KILL SWITCH: halt evaluation instantly on operator command.
                if crate::jito::is_trading_killed() {
                    continue;
                }

                // V42b: VAULT updates — chain-truth reserve amounts (u64@64). Routed by
                // vault_side; validated against the pool's decoded mints before trusting.
                if let Some(binding) = self_helius.vault_side.get(&acc_update.pubkey) {
                    let (pool, side) = binding.value().clone();
                    drop(binding);
                    if let Some((vault_mint, amount)) = crate::pool_decoder::decode_vault_payload(&acc_update.data) {
                        let mint_ok = self_helius
                            .pool_mints
                            .get(&pool)
                            .map(|m| {
                                let expected = if side == 0 { &m.value().0 } else { &m.value().1 };
                                *expected == vault_mint
                            })
                            .unwrap_or(false);
                        if mint_ok {
                            let now_ms = chrono::Utc::now().timestamp_millis();
                            if let Some(mut arr) = self_helius.live_reserves.get_mut(&pool) {
                                arr[side as usize] = (amount, now_ms);
                            }
                        }
                    }
                    continue;
                }

                // V42b: POOL-STATE update — one-time vault decode + subscription wiring.
                let mut freshly_wired = false;
                if self_helius.pool_to_mint.contains_key(&acc_update.pubkey)
                    && !self_helius.pool_meta_registered.contains_key(&acc_update.pubkey)
                {
                    self_helius.register_pool_from_payload(
                        &acc_update.pubkey,
                        &acc_update.owner,
                        &acc_update.data,
                    );
                    freshly_wired = self_helius.pool_mints.contains_key(&acc_update.pubkey);

                    // V42b fix: cold-start SEED — pull both vault amounts immediately
                    // (previously live_reserves stayed [0,0] until each side saw a swap,
                    // so apply_live_reserves never corrected anything), plus any unknown
                    // mint decimals, all in ONE batched RPC per newly wired pool.
                    if freshly_wired {
                        let mints_pair = self_helius
                            .pool_mints
                            .get(&acc_update.pubkey)
                            .map(|m| (m.value().0.clone(), m.value().1.clone()));
                        if let Some((mint_a, mint_b)) = mints_pair {
                            if let Some(decoded) = crate::pool_decoder::decode_pool_account(
                                &acc_update.owner,
                                &acc_update.data,
                            ) {
                                let unknown_mints: Vec<String> = [mint_a.clone(), mint_b.clone()]
                                    .into_iter()
                                    .filter(|mk| !self_helius.mint_decimals.contains_key(mk))
                                    .collect();
                                let client = self_helius.http_client.clone();
                                let dec_map = self_helius.mint_decimals.clone();
                                let res_map = self_helius.live_reserves.clone();
                                let pool_pk = acc_update.pubkey.clone();
                                let owner_short = acc_update.owner[..6.min(acc_update.owner.len())].to_string();
                                let vaults = (decoded.vault_a.clone(), decoded.vault_b.clone());
                                let expected = (mint_a, mint_b);
                                tokio::spawn(async move {
                                    use base64::Engine as _;
                                    // Key layout: unknown mints first, then vaultA, vaultB.
                                    let mut keys = unknown_mints.clone();
                                    keys.push(vaults.0);
                                    keys.push(vaults.1);
                                    // Rotate REST load across ALL keys by pool hash —
                                    // key#3 alone 429s under startup wiring bursts.
                                    let key_idx =
                                        pool_pk.bytes().map(|b| b as usize).sum::<usize>()
                                            % crate::helius_stream::HELIUS_API_KEYS.len();
                                    let url = format!(
                                        "https://mainnet.helius-rpc.com/?api-key={}",
                                        crate::helius_stream::HELIUS_API_KEYS[key_idx]
                                    );
                                    let req = serde_json::json!({
                                        "jsonrpc": "2.0", "id": 1,
                                        "method": "getMultipleAccounts",
                                        "params": [keys, {"encoding": "base64"}]
                                    });
                                    let short = &pool_pk[..10.min(pool_pk.len())];
                                    // Startup wires ~30 pools at once → key#3 rate-limits.
                                    // One spaced retry absorbs the burst without a semaphore.
                                    let mut body_result: Result<
                                        serde_json::Value,
                                        String,
                                    > = Err(String::new());
                                    for attempt in 0..2u8 {
                                        let r = client.post(&url).json(&req).send().await;
                                        match r {
                                            Err(e) => body_result = Err(format!("RPC {}", e)),
                                            Ok(resp) => match resp.json::<serde_json::Value>().await {
                                                Ok(b) => {
                                                    body_result = Ok(b);
                                                    break;
                                                }
                                                Err(e) => body_result = Err(format!("JSON {}", e)),
                                            },
                                        }
                                        if attempt == 0 {
                                            tokio::time::sleep(Duration::from_millis(2_500)).await;
                                        }
                                    }
                                    let body = match body_result {
                                        Ok(b) => b,
                                        Err(e) => {
                                            warn!("🌱 [LIVE-PX SEED] {} fail after retry: {}", short, e);
                                            return;
                                        }
                                    };
                                    let now_ms = chrono::Utc::now().timestamp_millis();
                                    let mut seeded_vaults = 0usize;
                                    let mut got_decimals = 0usize;
                                    if let Some(arr) = body["result"]["value"].as_array() {
                                        for (i, v) in arr.iter().enumerate() {
                                            let Some(data_b64) = v["data"][0].as_str() else {
                                                continue;
                                            };
                                            if i < unknown_mints.len() {
                                                // SPL Mint raw layout: decimals u8 @ byte 44.
                                                if let Ok(raw) = base64::engine::general_purpose::STANDARD
                                                    .decode(data_b64)
                                                {
                                                    if raw.len() >= 82 {
                                                        dec_map.insert(unknown_mints[i].clone(), raw[44]);
                                                        got_decimals += 1;
                                                    }
                                                }
                                            } else {
                                                let side = (i - unknown_mints.len()) as u8;
                                                let want = if side == 0 { &expected.0 } else { &expected.1 };
                                                match crate::pool_decoder::decode_vault_payload(data_b64) {
                                                    Some((vault_mint, amount)) => {
                                                        if &vault_mint == want {
                                                            if let Some(mut e) = res_map.get_mut(&pool_pk) {
                                                                e[side as usize] = (amount, now_ms);
                                                            }
                                                            seeded_vaults += 1;
                                                        } else {
                                                            warn!(
                                                                "🌱 [LIVE-PX SEED] {} vault{} MISMATCH owner:{}… want {}… got {}… amt {}",
                                                                short,
                                                                side,
                                                                owner_short,
                                                                &want[..8.min(want.len())],
                                                                &vault_mint[..8.min(vault_mint.len())],
                                                                amount
                                                            );
                                                        }
                                                    }
                                                    None => {
                                                        warn!("🌱 [LIVE-PX SEED] {} vault{} payload undecodable", short, side);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    info!(
                                        "🌱 [LIVE-PX SEED] {} vaults-seeded:{} decimals:{} {}",
                                        short,
                                        seeded_vaults,
                                        got_decimals,
                                        if seeded_vaults == 2 { "BOTH SIDES LIVE" } else { "PARTIAL" }
                                    );
                                });
                            }
                        }
                    }
                }

                // V42 REFLEX hot path: account-scoped subscriptions mean every event
                // IS a tracked pool. Reverse-lookup the mint; unknown pools are noise.
                let target_mint = match self_helius.pool_to_mint.get(&acc_update.pubkey) {
                    Some(m) => m.value().clone(),
                    None => continue,
                };

                // Slot dedupe + per-pool dispatch throttle: one probe per pool per
                // interval max (1.2s), but a NEW slot always re-arms — hot spreads can
                // be retried rapidly instead of waiting out long cooldowns.
                const TRIGGER_MIN_INTERVAL_MS: i64 = 1_200;
                {
                    let mut entry = self_helius
                        .trigger_throttle
                        .entry(acc_update.pubkey.clone())
                        .or_insert((0u64, 0i64));
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    if acc_update.slot <= entry.0 || now_ms - entry.1 < TRIGGER_MIN_INTERVAL_MS {
                        continue;
                    }
                    entry.0 = acc_update.slot;
                    entry.1 = now_ms;
                }

                // Stamp freshness — engines read this to unlock reflex marginal dispatch.
                crate::helius_stream::stamp_mint_event(&target_mint);

                if let Some(token_ref) = self_helius.tracking_registry.get(&target_mint) {
                    let pools = token_ref.pools.clone();
                    let symbol = token_ref.symbol.clone();
                    drop(token_ref);
                    self_helius.evaluate_all_pool_pairs(&target_mint, &symbol, &pools, self_helius.get_sol_price()).await;
                }
            }
        });

        let _ = tokio::join!(swap_handle, mig_handle, helius_handle);
    }
}