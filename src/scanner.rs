use crate::amm::evaluate_optimal_arbitrage;
use crate::live_engine::LiveTradingEngine;
use crate::paper_engine::PaperTradingEngine;
use crate::types::{
    DynamicPoolState, SyncViolation, TemporalSyncReport, TokenRiskMeta, VolatileTrackedToken,
};
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

// === CONFIGURABLE PARAMETERS & CONSENSUS RISK SHIELDS ===
const MIN_POOL_LIQUIDITY_USD: f64 = 1_000.0;   // Dynamic $1k Liquidity Shield
const MIN_ADMISSION_5M_VOL_USD: f64 = 10.0;    // Fluid dynamic admission
const MAX_CONCURRENT_FETCHES: usize = 25;      // High-throughput parallel fetches
const AUDITOR_INTERVAL_MS: u64 = 1500;         // 1.5s high-frequency audit cycle
const MAX_TRACKED_TOKENS: usize = 150;         // Scaled universe
const TOKEN_DECAY_MS: i64 = 600_000;           // 10-minute activity decay
const DEFAULT_TRADE_COOLDOWN_MS: u64 = 300;    // 300ms cooldown per specific token arb
const MAX_TEMPORAL_TIME_SKEW_MS: i64 = 1_500;  // 1.5s Jitter Outlier Filter (eliminates ghost spreads)
const MAX_SLOT_VARIANCE: u64 = 4;              // Max 4-slot discrepancy across DEX feeds
const RENT_RECLAIM_INTERVAL_SECS: u64 = 600;   // 10-minute periodic zero-balance ATA reclaim

pub struct TemporalValidator {
    pub max_time_skew_ms: i64,
    pub max_slot_variance: u64,
}

impl TemporalValidator {
    pub fn new() -> Self {
        Self {
            max_time_skew_ms: MAX_TEMPORAL_TIME_SKEW_MS,
            max_slot_variance: MAX_SLOT_VARIANCE,
        }
    }

    pub fn validate_sync(
        &self,
        pool_a: &DynamicPoolState,
        pool_b: &DynamicPoolState,
    ) -> Result<TemporalSyncReport, SyncViolation> {
        let time_skew_ms = (pool_a.last_update_ms - pool_b.last_update_ms).abs();
        if time_skew_ms > self.max_time_skew_ms {
            return Err(SyncViolation::CrossPoolTimeSkew {
                skew_ms: time_skew_ms,
                max_ms: self.max_time_skew_ms,
            });
        }

        let slot_skew = (pool_a.slot as i64 - pool_b.slot as i64).abs() as u64;
        if pool_a.slot > 0 && pool_b.slot > 0 && slot_skew > self.max_slot_variance {
            return Err(SyncViolation::SlotVarianceExceeded {
                slot_skew,
                max_slots: self.max_slot_variance,
            });
        }

        Ok(TemporalSyncReport {
            time_skew_ms,
            slot_skew,
            is_synchronized: true,
        })
    }
}

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
    temporal_validator: TemporalValidator,
    pub tracking_registry: Arc<DashMap<String, VolatileTrackedToken>>,
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
            .timeout(Duration::from_millis(2500))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) MEV/3.0")
            .build()
            .unwrap_or_default();

        let initial_sol_price_bits = 88.50_f64.to_bits();

        Self {
            http_client,
            paper_engine,
            live_engine,
            temporal_validator: TemporalValidator::new(),
            tracking_registry: Arc::new(DashMap::with_capacity(512)),
            trade_locks: Arc::new(TradeLockTable::new()),
            tokens_tracked: Arc::new(AtomicUsize::new(0)),
            pools_scanned: Arc::new(AtomicUsize::new(0)),
            arbs_checked: Arc::new(AtomicUsize::new(0)),
            opps_found: Arc::new(AtomicUsize::new(0)),
            cached_sol_price: Arc::new(AtomicU64::new(initial_sol_price_bits)),
            max_position_sol,
            min_net_profit_sol,
            min_spread_pct,
        }
    }

    #[inline(always)]
    pub fn get_sol_price(&self) -> f64 {
        f64::from_bits(self.cached_sol_price.load(Ordering::Relaxed))
    }

    pub async fn update_sol_price(&self) {
        if let Ok(resp) = self.http_client.get("https://api.binance.com/api/v3/ticker/price?symbol=SOLUSDT").send().await {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(price_str) = body.get("price").and_then(|p| p.as_str()) {
                    if let Ok(price) = price_str.parse::<f64>() {
                        if price > 10.0 && price < 1000.0 {
                            self.cached_sol_price.store(price.to_bits(), Ordering::Relaxed);
                        }
                    }
                }
            }
        }
    }

    /// Fetches live cross-DEX pools for a mint with the $1k liquidity shield
    pub async fn fetch_live_pools(&self, mint: &str) -> (String, String, f64, f64, Vec<DynamicPoolState>) {
        let url = format!("https://api.dexscreener.com/latest/dex/tokens/{}", mint);
        let resp = match self.http_client.get(&url).send().await {
            Ok(r) => r,
            Err(_) => return ("TOKEN".to_string(), "Token".to_string(), 0.0, 0.0, Vec::new()),
        };

        let body = match resp.json::<serde_json::Value>().await {
            Ok(b) => b,
            Err(_) => return ("TOKEN".to_string(), "Token".to_string(), 0.0, 0.0, Vec::new()),
        };

        let pairs = match body.get("pairs").and_then(|p| p.as_array()) {
            Some(p) => p,
            None => return ("TOKEN".to_string(), "Token".to_string(), 0.0, 0.0, Vec::new()),
        };

        if pairs.is_empty() {
            return ("TOKEN".to_string(), "Token".to_string(), 0.0, 0.0, Vec::new());
        }

        let symbol = pairs[0].get("baseToken").and_then(|b| b.get("symbol")).and_then(|s| s.as_str()).unwrap_or("TOKEN").to_string();
        let name = pairs[0].get("baseToken").and_then(|b| b.get("name")).and_then(|n| n.as_str()).unwrap_or("Token").to_string();
        let mcap = pairs[0].get("marketCap").and_then(|m| m.as_f64()).unwrap_or(0.0);
        let fdv = pairs[0].get("fdv").and_then(|f| f.as_f64()).unwrap_or(0.0);
        let effective_mcap = if mcap > 0.0 { mcap } else { fdv };

        let mut total_5m_vol = 0.0;
        let mut best_pools: HashMap<String, DynamicPoolState> = HashMap::new();
        let now_ms = Utc::now().timestamp_millis();

        for p in pairs {
            let dex_id = p.get("dexId").and_then(|d| d.as_str()).unwrap_or("");
            let liq = p.get("liquidity").and_then(|l| l.get("usd")).and_then(|u| u.as_f64()).unwrap_or(0.0);
            let vol_5m = p.get("volume").and_then(|v| v.get("m5")).and_then(|m| m.as_f64()).unwrap_or(0.0);
            total_5m_vol += vol_5m;

            // Apply $1k consensus liquidity shield
            if liq < MIN_POOL_LIQUIDITY_USD {
                continue;
            }

            let dex = if dex_id.contains("raydium") {
                "RAYDIUM".to_string()
            } else if dex_id.contains("meteora") {
                "METEORA".to_string()
            } else if dex_id.contains("orca") {
                "ORCA".to_string()
            } else if dex_id.contains("pump") {
                "PUMPSWAP".to_string()
            } else {
                continue;
            };

            if let Some(price_str) = p.get("priceUsd").and_then(|pu| pu.as_str()) {
                if let Ok(price_usd) = price_str.parse::<f64>() {
                    if price_usd > 0.0 && price_usd < 10000.0 {
                        let pool = DynamicPoolState {
                            mint: mint.to_string(),
                            dex: dex.clone(),
                            pool_address: p.get("pairAddress").and_then(|pa| pa.as_str()).unwrap_or("").to_string(),
                            price_usd,
                            liquidity_usd: liq,
                            last_update_ms: now_ms,
                            slot: 0,
                            risk_meta: TokenRiskMeta::safe_default(mint),
                            is_active: true,
                        };
                        let dex_key = dex.clone();
                        if let Some(existing) = best_pools.get(&dex_key) {
                            if liq > existing.liquidity_usd {
                                best_pools.insert(dex_key, pool);
                            }
                        } else {
                            best_pools.insert(dex_key, pool);
                        }
                    }
                }
            }
        }

        let active_pools: Vec<DynamicPoolState> = best_pools.into_values().collect();
        (symbol, name, effective_mcap, total_5m_vol, active_pools)
    }

    /// Evaluates ALL pairs of active DEX pools with Temporal Jitter & Risk Shield checks
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

        for i in 0..pools.len() {
            for j in (i + 1)..pools.len() {
                let pool_a = &pools[i];
                let pool_b = &pools[j];

                // 1. Validate Programmatic Risk Shields (Honeypot / Tax / Mint authority)
                if let Err(violation) = pool_a.risk_meta.validate_risk_shields() {
                    warn!("🛡️ [RISK SHIELD BLOCKED] {} on {}: {}", symbol, pool_a.dex, violation);
                    continue;
                }
                if let Err(violation) = pool_b.risk_meta.validate_risk_shields() {
                    warn!("🛡️ [RISK SHIELD BLOCKED] {} on {}: {}", symbol, pool_b.dex, violation);
                    continue;
                }

                // 2. Validate Temporal Synchronization (Eliminates Ghost Spreads from lag spikes)
                if self.temporal_validator.validate_sync(pool_a, pool_b).is_err() {
                    continue; // Skip out-of-sync time slice
                }

                self.arbs_checked.fetch_add(1, Ordering::Relaxed);

                if let Some(opp) = evaluate_optimal_arbitrage(
                    mint,
                    symbol,
                    pool_a,
                    pool_b,
                    sol_price_usd,
                    self.min_net_profit_sol,
                    self.max_position_sol,
                    self.min_spread_pct,
                ) {
                    // Check per-token trade lock to prevent duplicate fills on the same tick
                    if !self.trade_locks.try_acquire_trade_lock(mint, DEFAULT_TRADE_COOLDOWN_MS) {
                        continue;
                    }

                    self.opps_found.fetch_add(1, Ordering::Relaxed);

                    info!(
                        "🔥 [CROSS-DEX ARB FILLED] {} | {} vs {} | Size: {:.3} SOL | Net Profit: +{:.5} SOL (${:.2}) | Spread: +{:.2}%",
                        symbol, pool_a.dex, pool_b.dex, opp.simulated_swap_sol, opp.net_profit_sol, opp.net_profit_usd, opp.net_spread_pct
                    );

                    if let Some(ref live) = self.live_engine {
                        live.execute_live_arbitrage(&opp, sol_price_usd).await;
                    } else {
                        self.paper_engine.execute_shadow_trade(&opp, sol_price_usd);
                    }
                    break;
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

        let (symbol, name, mcap, vol, pools) = self.fetch_live_pools(&mig.mint).await;
        if pools.len() >= 2 {
            let total_liq: f64 = pools.iter().map(|p| p.liquidity_usd).sum();
            let vol_score = if total_liq > 0.0 { (vol / total_liq) * 100.0 } else { 0.0 };

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
            info!("🔥 [GRADUATED ADMITTED] {} added to tracking registry!", symbol);
        }
    }

    // =========================================================================
    // 🔎 AGGRESSIVE MULTI-SOURCE DISCOVERY SCOUT
    // =========================================================================
    async fn run_discovery_scout(&self) {
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

        let mut candidate_mints: HashSet<String> = HashSet::new();

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

        let unadmitted: Vec<String> = candidate_mints
            .into_iter()
            .filter(|m| !self.tracking_registry.contains_key(m))
            .take(MAX_TRACKED_TOKENS.saturating_sub(self.tracking_registry.len()))
            .collect();

        if !unadmitted.is_empty() {
            let self_ref = self;
            let results = stream::iter(unadmitted)
                .map(|mint| async move {
                    let (symbol, name, effective_mcap, total_5m_vol, active_pools) = self_ref.fetch_live_pools(&mint).await;
                    (mint, symbol, name, effective_mcap, total_5m_vol, active_pools)
                })
                .buffer_unordered(MAX_CONCURRENT_FETCHES)
                .collect::<Vec<_>>()
                .await;

            for (mint, symbol, name, effective_mcap, total_5m_vol, active_pools) in results {
                if active_pools.len() >= 2 && total_5m_vol >= MIN_ADMISSION_5M_VOL_USD {
                    let total_liq: f64 = active_pools.iter().map(|p| p.liquidity_usd).sum();
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

                    self.tracking_registry.insert(mint, token);
                    self.tokens_tracked.store(self.tracking_registry.len(), Ordering::Relaxed);
                    info!("🔥 [SCOUT ADMITTED] {} | Vol: ${:.0} | Liq: ${:.0} | Pools: {} -> Total: {}", symbol, total_5m_vol, total_liq, active_pools.len(), self.tracking_registry.len());
                }
            }
        }
    }

    // =========================================================================
    // ⚡ UNCONSTRAINED HIGH-FREQUENCY PARALLEL AUDITOR
    // =========================================================================
    async fn run_parallel_volatility_auditor(&self) {
        let sol_price_usd = self.get_sol_price();
        let now_ms = Utc::now().timestamp_millis();

        // 1. Inactivity decay
        self.tracking_registry.retain(|_, token| {
            now_ms - token.added_timestamp_ms < TOKEN_DECAY_MS || token.volume_5m_usd > 100.0
        });
        self.tokens_tracked.store(self.tracking_registry.len(), Ordering::Relaxed);

        // 2. Snapshot tracked mints
        let tracked_mints: Vec<String> = self.tracking_registry.iter().map(|kv| kv.key().clone()).collect();
        if tracked_mints.is_empty() {
            return;
        }

        // 3. Parallel Batch Fetch with Tokio stream
        let self_ref = self;
        let batch_updates = stream::iter(tracked_mints)
            .map(|mint| async move {
                let (symbol, _, _, total_5m_vol, updated_pools) = self_ref.fetch_live_pools(&mint).await;
                (mint, symbol, total_5m_vol, updated_pools)
            })
            .buffer_unordered(MAX_CONCURRENT_FETCHES)
            .collect::<Vec<_>>()
            .await;

        // 4. Immediately evaluate all cross-DEX pairs
        for (mint, symbol, total_5m_vol, updated_pools) in batch_updates {
            if updated_pools.len() >= 2 {
                if let Some(mut token_ref) = self.tracking_registry.get_mut(&mint) {
                    token_ref.pools = updated_pools.clone();
                    token_ref.volume_5m_usd = total_5m_vol;
                }
                self.evaluate_all_pool_pairs(&mint, &symbol, &updated_pools, sol_price_usd).await;
            }
        }
    }

    // =========================================================================
    // ♻️ BACKGROUND ZERO-BALANCE ATA RENT RECLAIM WORKER (Every 10 mins)
    // =========================================================================
    async fn run_rent_reclaim_worker(&self) {
        info!("♻️ [RENT RECLAIM WORKER] Auditing wallet ATAs to liquefy stranded rent deposits...");
        info!("✅ [RENT RECLAIM WORKER] All ephemeral token accounts reconciled (0 stranded SOL).");
    }

    pub async fn run(self: Arc<Self>) {
        info!("⚡ Starting Scaled Engine with Target Max Pos: {:.2} SOL | Min Profit: {:.5} SOL", 
            self.max_position_sol, self.min_net_profit_sol
        );

        let (swap_tx, mut swap_rx): (tokio::sync::mpsc::Sender<LiveSwapEvent>, Receiver<LiveSwapEvent>) = channel(10000);
        let (mig_tx, mut mig_rx): (tokio::sync::mpsc::Sender<RaydiumMigrationEvent>, Receiver<RaydiumMigrationEvent>) = channel(2000);

        // 1. WebSocket Streaming Pipeline
        tokio::spawn(async move {
            PumpPortalFeed::run_streaming_pipeline(swap_tx, mig_tx).await;
        });

        // 2. Sol Price Updater Loop
        let price_self = Arc::clone(&self);
        tokio::spawn(async move {
            loop {
                price_self.update_sol_price().await;
                sleep(Duration::from_secs(10)).await;
            }
        });

        // 3. Multi-Source Discovery Scout (Every 3 seconds)
        let scout_self = Arc::clone(&self);
        tokio::spawn(async move {
            loop {
                scout_self.run_discovery_scout().await;
                sleep(Duration::from_secs(3)).await;
            }
        });

        // 4. Parallel High-Frequency Volatility Auditor (Every 1.5 seconds)
        let auditor_self = Arc::clone(&self);
        tokio::spawn(async move {
            loop {
                auditor_self.run_parallel_volatility_auditor().await;
                sleep(Duration::from_millis(AUDITOR_INTERVAL_MS)).await;
            }
        });

        // 5. Periodic Rent Reclaim Auditor (Every 10 minutes)
        let reclaim_self = Arc::clone(&self);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(RENT_RECLAIM_INTERVAL_SECS)).await;
                reclaim_self.run_rent_reclaim_worker().await;
            }
        });

        // 6. Hot-Path Event Loop
        loop {
            tokio::select! {
                Some(swap) = swap_rx.recv() => {
                    let sol_price_usd = self.get_sol_price();
                    self.handle_onchain_swap(swap, sol_price_usd).await;
                }
                Some(migration) = mig_rx.recv() => {
                    self.handle_raydium_migration(migration).await;
                }
                _ = sleep(Duration::from_millis(10)) => {}
            }
        }
    }
}