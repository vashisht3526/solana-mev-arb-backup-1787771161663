#![allow(dead_code)]
use crate::jito_builder::{JitoBuilder, SignedCircularPackage, USDC_MINT};
use crate::types::ArbitrageOpportunity;
use chrono::Utc;
use dashmap::DashMap;
use log::{info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_sdk::hash::Hash;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

pub const JITO_BLOCK_ENGINE_FRANKFURT: &str = "https://frankfurt.mainnet.block-engine.jito.wtf/api/v1/bundles";
pub const SOLANA_MAINNET_RPC: &str = "https://mainnet.helius-rpc.com/?api-key=9b358df5-981f-4a3f-8f25-5ff9af04653f";

// === TRIANGULAR SPECIFIC SAFETY PROTOCOL CONSTANTS (COUNCIL CERTIFIED) ===
pub const MIN_RESERVE_FLOOR_LAMPORTS: u64 = 150_000_000;       // 0.15 SOL untouchable floor
pub const MAX_TRIANGULAR_POSITION_LAMPORTS: u64 = 300_000_000;  // 0.300 SOL Concentrated Cap
pub const DEFAULT_TRIANGULAR_POSITION_LAMPORTS: u64 = 50_000_000; // 0.050 SOL Default Baseline Position
pub const MIN_TRIANGULAR_PROFIT_LAMPORTS: u64 = 20_000;         // 0.000020 SOL net profit floor
pub const DEFAULT_TRIANGULAR_JITO_TIP: u64 = 20_000;            // 20k lamports baseline tip
pub const MAX_TRIANGULAR_PRICE_IMPACT_BPS: u64 = 35;            // Max 0.35% price impact cap
pub const WHALE_EXCLUSION_MIN_TVL: f64 = 5_000.0;               // $5k Micro-Alpha Floor
pub const WHALE_EXCLUSION_MAX_TVL: f64 = 2_500_000.0;           // $2.5M TVL Cap

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriangularConfig {
    pub wallet_pubkey: String,
    pub wallet_balance_sol: f64,
    pub is_engine_running: bool,
    pub min_pool_tvl_usd: f64,
    pub max_pool_tvl_usd: f64,
    pub max_position_sol: f64,
    pub min_spread_pct: f64,
    pub min_net_profit_sol: f64,
    pub max_compound_slippage_pct: f64,
    pub jito_tip_pct: f64,
    pub reserve_floor_sol: f64,
    pub intermediate_bridge: String,
    pub status_label: String,
}

impl Default for TriangularConfig {
    fn default() -> Self {
        Self {
            wallet_pubkey: "AA1ihbehbPrMW4L9jWtdxJfkH76Up5F2azbeepQ1wixs".to_string(),
            wallet_balance_sol: 0.3981,
            is_engine_running: true,
            min_pool_tvl_usd: 3_000.0,
            max_pool_tvl_usd: 40_000.0,
            max_position_sol: 0.020,
            min_spread_pct: 0.80,
            min_net_profit_sol: 0.00010,
            max_compound_slippage_pct: 0.40,
            jito_tip_pct: 35.0,
            reserve_floor_sol: 0.10,
            intermediate_bridge: "USDC".to_string(),
            status_label: "TRIANGULAR ACTIVE (SOL ➔ Token ➔ USDC ➔ SOL)".to_string(),
        }
    }
}

pub struct TriangularLiveEngine {
    pub jito_builder: JitoBuilder,
    pub http_client: Client,
    pub wallet_pubkey: String,
    pub verified_onchain_lamports: Arc<AtomicU64>,
    pub live_trades_log_path: String,
    pub config_path: String,
    pub consecutive_reverts: Arc<AtomicU64>,
    pub asset_locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
    pub penalty_cooldowns: Arc<DashMap<String, i64>>,
    pub cached_blockhash: Arc<tokio::sync::RwLock<Option<(Hash, std::time::Instant)>>>,
}

impl TriangularLiveEngine {
    pub fn new(keypair_path: &str, initial_sol: f64, log_path: &str, config_path: &str) -> Self {
        let jito_builder = JitoBuilder::load_from_path(keypair_path)
            .expect("Failed to initialize JitoBuilder for Triangular engine");
        let wallet_pubkey = jito_builder.payer_pubkey.clone();

        let http_client = Client::builder()
            .timeout(Duration::from_millis(5000))
            .pool_max_idle_per_host(16)
            .user_agent("Mozilla/5.0 (TriangularMev; Linux x86_64)")
            .build()
            .unwrap_or_default();

        let initial_lamports = (initial_sol * 1_000_000_000.0) as u64;

        info!("🔺 [TRIANGULAR LIVE ENGINE INITIALIZED] Armed Keypair: {}", wallet_pubkey);
        info!("💰 Triangular Bankroll: {} lamports ({:.4} SOL) | Sizing: 0.015-0.020 SOL | Whale Moat: $3k-$40k TVL",
            initial_lamports, initial_sol
        );

        let cached_blockhash = Arc::new(tokio::sync::RwLock::new(None));
        let bh_clone = cached_blockhash.clone();
        let http_clone = http_client.clone();

        // ⚡ 200ms Background Blockhash Pre-Warmer (Council Certified)
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(200));
            loop {
                interval.tick().await;
                let post_data = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "getLatestBlockhash",
                    "params": [{"commitment": "processed"}]
                });
                if let Ok(resp) = http_clone.post(SOLANA_MAINNET_RPC).json(&post_data).send().await {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if let Some(bh_str) = json.get("result").and_then(|r| r.get("value")).and_then(|v| v.get("blockhash")).and_then(|b| b.as_str()) {
                            if let Ok(hash) = Hash::from_str(bh_str) {
                                let mut w = bh_clone.write().await;
                                *w = Some((hash, std::time::Instant::now()));
                            }
                        }
                    }
                }
            }
        });

        Self {
            jito_builder,
            http_client,
            wallet_pubkey,
            verified_onchain_lamports: Arc::new(AtomicU64::new(initial_lamports)),
            live_trades_log_path: log_path.to_string(),
            config_path: config_path.to_string(),
            consecutive_reverts: Arc::new(AtomicU64::new(0)),
            asset_locks: Arc::new(DashMap::new()),
            penalty_cooldowns: Arc::new(DashMap::new()),
            cached_blockhash,
        }
    }

    #[inline(always)]
    pub fn is_token_in_cooldown(&self, mint: &str) -> bool {
        if let Some(expiry) = self.penalty_cooldowns.get(mint) {
            let now = Utc::now().timestamp_millis();
            if now < *expiry {
                return true;
            }
        }
        false
    }

    #[inline(always)]
    pub fn set_token_cooldown(&self, mint: &str, cooldown_ms: i64) {
        let now = Utc::now().timestamp_millis();
        self.penalty_cooldowns.insert(mint.to_string(), now + cooldown_ms);
    }

    pub fn load_config(&self) -> TriangularConfig {
        if let Ok(content) = fs::read_to_string(&self.config_path) {
            if let Ok(cfg) = serde_json::from_str::<TriangularConfig>(&content) {
                return cfg;
            }
        }
        TriangularConfig::default()
    }

    pub async fn fetch_real_onchain_lamports(&self) -> u64 {
        let rpc_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getBalance",
            "params": [
                self.wallet_pubkey,
                { "commitment": "confirmed" }
            ]
        });

        if let Ok(resp) = self.http_client.post(SOLANA_MAINNET_RPC).json(&rpc_payload).send().await {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(lamports) = body.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_u64()) {
                    self.verified_onchain_lamports.store(lamports, Ordering::SeqCst);
                    return lamports;
                }
            }
        }
        self.verified_onchain_lamports.load(Ordering::SeqCst)
    }

    pub async fn fetch_recent_blockhash(&self) -> Option<Hash> {
        // Fast-path: Instant 0ms read from background pre-warmed cache (< 1500ms old)
        {
            let r = self.cached_blockhash.read().await;
            if let Some((hash, instant)) = *r {
                if instant.elapsed() < Duration::from_millis(1500) {
                    return Some(hash);
                }
            }
        }

        let rpc_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestBlockhash",
            "params": [
                { "commitment": "processed" }
            ]
        });

        if let Ok(resp) = self.http_client.post(SOLANA_MAINNET_RPC).json(&rpc_payload).send().await {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(bh_str) = body.get("result").and_then(|r| r.get("value")).and_then(|v| v.get("blockhash")).and_then(|b| b.as_str()) {
                    if let Ok(hash) = Hash::from_str(bh_str) {
                        let mut w = self.cached_blockhash.write().await;
                        *w = Some((hash, std::time::Instant::now()));
                        return Some(hash);
                    }
                }
            }
        }
        None
    }

    pub async fn simulate_transaction_preflight(&self, signed_tx_base64: &str) -> bool {
        let rpc_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "simulateTransaction",
            "params": [
                signed_tx_base64,
                { "encoding": "base64", "sigVerify": false, "commitment": "confirmed" }
            ]
        });

        if let Ok(resp) = self.http_client.post(SOLANA_MAINNET_RPC).json(&rpc_payload).send().await {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(val) = body.get("result").and_then(|r| r.get("value")) {
                    let err = val.get("err");
                    if err.is_none() || err.unwrap().is_null() {
                        info!("🧪 [TRIANGULAR SIMULATION PASSED] 3-Hop route valid on-chain");
                        return true;
                    } else {
                        warn!("🛑 [TRIANGULAR SIMULATION FAILED] Revert detected: {:?}", err);
                        return false;
                    }
                }
            }
        }
        false
    }

    pub async fn verify_transaction_level_settlement(&self, tx_signature_base58: &str, max_attempts: u32) -> Option<(u64, u64)> {
        let rpc_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTransaction",
            "params": [
                tx_signature_base58,
                { "encoding": "jsonParsed", "maxSupportedTransactionVersion": 0 }
            ]
        });

        for attempt in 1..=max_attempts {
            sleep(Duration::from_millis(1500)).await;
            if let Ok(resp) = self.http_client.post(SOLANA_MAINNET_RPC).json(&rpc_payload).send().await {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if let Some(result) = body.get("result") {
                        if !result.is_null() {
                            let meta = result.get("meta");
                            let pre_balances = meta.and_then(|m| m.get("preBalances")).and_then(|b| b.as_array());
                            let post_balances = meta.and_then(|m| m.get("postBalances")).and_then(|b| b.as_array());

                            let mut wallet_idx: Option<usize> = None;
                            if let Some(account_keys) = result.get("transaction")
                                .and_then(|t| t.get("message"))
                                .and_then(|m| m.get("accountKeys"))
                                .and_then(|k| k.as_array())
                            {
                                for (i, key_val) in account_keys.iter().enumerate() {
                                    let key_str = if let Some(s) = key_val.as_str() {
                                        s
                                    } else if let Some(s) = key_val.get("pubkey").and_then(|p| p.as_str()) {
                                        s
                                    } else {
                                        ""
                                    };
                                    if key_str == self.wallet_pubkey {
                                        wallet_idx = Some(i);
                                        break;
                                    }
                                }
                            }

                            if let Some(idx) = wallet_idx {
                                if let (Some(pre_b), Some(post_b)) = (pre_balances, post_balances) {
                                    if let (Some(pre_val), Some(post_val)) = (
                                        pre_b.get(idx).and_then(|v| v.as_u64()),
                                        post_b.get(idx).and_then(|v| v.as_u64()),
                                    ) {
                                        info!("🎯 [TRIANGULAR SETTLEMENT CONFIRMED] Wallet Index: {} | Pre: {} -> Post: {} lamports (Attempt {})",
                                            idx, pre_val, post_val, attempt
                                        );
                                        return Some((pre_val, post_val));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Executes live 3-hop triangular arbitrage: SOL -> Token A -> USDC -> SOL
    pub async fn execute_triangular_arbitrage(
        &self,
        token_a_mint: &str,
        symbol: &str,
        dex_hop1: &str,
        dex_hop2: &str,
        dex_hop3: &str,
        pool_tvl_usd: f64,
        sol_price_usd: f64,
    ) {
        let config = self.load_config();
        if !config.is_engine_running {
            return;
        }

        // V42 KILL SWITCH: instant operator halt, ahead of any quote spend.
        if crate::jito::is_trading_killed() {
            return;
        }

        if self.is_token_in_cooldown(token_a_mint) {
            return;
        }

        // 1. Whale-Exclusion TVL Filter ($3k - $40k TVL)
        if pool_tvl_usd < config.min_pool_tvl_usd || pool_tvl_usd > config.max_pool_tvl_usd {
            return;
        }

        // 2. Non-blocking multi-asset lock
        let asset_mutex = self.asset_locks
            .entry(token_a_mint.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .value()
            .clone();

        let _asset_guard = match asset_mutex.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                warn!("⏳ [TRIANGULAR IN FLIGHT] {} trade already in progress. Skipping.", symbol);
                return;
            }
        };

        let pre_lamports = self.fetch_real_onchain_lamports().await;

        // 3. Reserve Floor Guard & Available Headroom Check
        let reserve_floor_lamports = (config.reserve_floor_sol * 1_000_000_000.0) as u64;
        let max_safe_headroom = pre_lamports.saturating_sub(reserve_floor_lamports);
        if max_safe_headroom < 25_000_000 {
            warn!("🛡️ [TRIANGULAR RESERVE FLOOR] Available headroom {} lamports < 0.025 SOL floor", max_safe_headroom);
            return;
        }

        // Fix 2: Exclude Pump.fun bonding curves from triangular routes
        if dex_hop1.to_lowercase().contains("pump") || dex_hop2.to_lowercase().contains("pump") || dex_hop3.to_lowercase().contains("pump") {
            return;
        }

        // V41: Concentrated Adaptive Sizing with Pool Depth Guard (0.5% of per-side
        // reserves ≈ ~1% price impact per leg; Jupiter confirmed-profit gate arbitrates)
        let pool_reserves_sol = (pool_tvl_usd / 2.0) / sol_price_usd.max(10.0);
        let max_depth_cap_lamports = ((pool_reserves_sol * 0.005) * 1_000_000_000.0) as u64;
        let optimal_swap_lamports = ((pool_reserves_sol * 0.02) * 1_000_000_000.0) as u64;
        let config_max_lamports = (config.max_position_sol * 1_000_000_000.0) as u64;

        let effective_size_lamports = optimal_swap_lamports
            .min(max_depth_cap_lamports)
            .min(config_max_lamports)
            .min(max_safe_headroom)
            .min(MAX_TRIANGULAR_POSITION_LAMPORTS)
            .clamp(20_000_000, 300_000_000);

        let recent_blockhash = match self.fetch_recent_blockhash().await {
            Some(bh) => bh,
            None => return,
        };

        let expected_profit_lamports = (config.min_net_profit_sol * 1_000_000_000.0) as u64;
        // V41: Escalated Jito Tip estimate (60% share, [25k, 1.5M] — mirrors builder policy)
        let tip_lamports = ((expected_profit_lamports as f64 * (config.jito_tip_pct / 100.0)) as u64)
            .clamp(25_000, 1_500_000);
        let slippage_bps = (config.max_compound_slippage_pct * 100.0) as u32;

        info!("🔺 [LIVE TRIANGULAR MEV] SOL -> {} -> USDC -> SOL | Size: {} lamports | Route: {} -> {} -> {} | Tip: {} lamports",
            symbol, effective_size_lamports, dex_hop1, dex_hop2, dex_hop3, tip_lamports
        );

        let triangular_pkg = match self.jito_builder.build_atomic_triangular_arbitrage_tx(
            token_a_mint,
            USDC_MINT,
            dex_hop1,
            dex_hop2,
            dex_hop3,
            effective_size_lamports,
            slippage_bps.clamp(20, 80),
            tip_lamports,
            recent_blockhash,
            crate::helius_stream::is_event_fresh(token_a_mint),
        ).await {
            Ok(pkg) => pkg,
            Err(e) => {
                warn!("⚠️ [TRIANGULAR COMPILATION REJECT] {}: {}", symbol, e);
                // V42c: feed competitor correlation — this attempt was gate-rejected.
                // (Tri path has no est-spread in scope; mint+timing carry the signal.)
                crate::mriy_watch::record_attempt(token_a_mint, symbol, 0.0, false, crate::helius_stream::is_event_fresh(token_a_mint));
                self.set_token_cooldown(token_a_mint, 15_000);
                return;
            }
        };

        info!("⚡ [ZERO-DELAY DIRECT DISPATCH] Dispatching in-memory triangular bundle to Jito Frankfurt: {} | Sig: {} | Net Profit: +{} lamports",
            symbol, triangular_pkg.tx_signature_base58, triangular_pkg.net_profit_lamports
        );
        // V42c: dispatched attempt also feeds correlation.
        crate::mriy_watch::record_attempt(token_a_mint, symbol, 0.0, true, crate::helius_stream::is_event_fresh(token_a_mint));

        info!("⚡ [ZERO-DELAY DIRECT DISPATCH] Dispatching in-memory triangular bundle to Jito Frankfurt: {} | Sig: {} | Net Profit: +{} lamports",
            symbol, triangular_pkg.tx_signature_base58, triangular_pkg.net_profit_lamports
        );

        let opp = ArbitrageOpportunity {
            timestamp_ms: Utc::now().timestamp_millis(),
            iso_time: Utc::now().to_rfc3339(),
            mint: token_a_mint.to_string(),
            symbol: format!("TRI/{}", symbol),
            name: format!("Triangular 3-Hop {}", symbol),
            buy_dex: dex_hop1.to_string(),
            sell_dex: format!("{}->{}", dex_hop2, dex_hop3),
            target_dex: dex_hop1.to_string(),
            pair_address: "".to_string(),
            pump_price_usd: 0.0,
            raydium_price_usd: 0.0,
            gross_spread_pct: 2.5,
            net_spread_pct: 2.0,
            pool_liquidity_usd: pool_tvl_usd,
            simulated_swap_sol: effective_size_lamports as f64 / 1e9,
            gross_profit_usd: (triangular_pkg.net_profit_lamports as f64 / 1e9) * sol_price_usd,
            jito_tip_usd: (triangular_pkg.tip_lamports as f64 / 1e9) * sol_price_usd,
            net_profit_usd: (triangular_pkg.net_profit_lamports as f64 / 1e9) * sol_price_usd,
            net_profit_sol: triangular_pkg.net_profit_lamports as f64 / 1e9,
        };

        self.submit_bundle_to_jito(&triangular_pkg, &opp).await;
    }

    async fn submit_bundle_to_jito(&self, pkg: &SignedCircularPackage, opp: &ArbitrageOpportunity) {
        let jito_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendBundle",
            "params": [[pkg.signed_tx_base58]]
        });

        // V42: race Frankfurt + Amsterdam + Dublin block engines concurrently.
        let bundle_submitted = match crate::jito::submit_bundle_multiregion(&self.http_client, &jito_payload).await {
            Ok(uuid) => {
                info!("📡 [TRIANGULAR JITO DISPATCHED] Jito UUID: {} | multi-region (FRA/AMS/DUB)", uuid);
                crate::jito::spawn_bundle_outcome_tracker(
                    self.http_client.clone(),
                    uuid,
                    format!("TRI/{}", opp.symbol),
                    pkg.tip_lamports,
                );
                true
            }
            Err(e) => {
                warn!("⚠️ [TRIANGULAR JITO REJECT] {}", e);
                false
            }
        };

        let current_bal = self.verified_onchain_lamports.load(Ordering::Relaxed);

        if !bundle_submitted {
            self.append_live_trade_log(pkg, opp, "JITO_REJECTED", current_bal, current_bal, 0.0);
            return;
        }

        self.append_live_trade_log(pkg, opp, "DISPATCHED_TO_JITO", current_bal, current_bal, 0.0);

        if let Some((exact_pre, exact_post)) = self.verify_transaction_level_settlement(&pkg.tx_signature_base58, 6).await {
            if exact_post > exact_pre {
                let actual_profit_lamports = exact_post - exact_pre;
                self.consecutive_reverts.store(0, Ordering::SeqCst);
                self.verified_onchain_lamports.store(exact_post, Ordering::SeqCst);

                info!("🏆 [TRIANGULAR ARBITRAGE WON & SETTLED] Mint: {} | Net PnL: +{} lamports (+{:.5} SOL) | Wallet: {:.4} SOL",
                    opp.symbol, actual_profit_lamports, actual_profit_lamports as f64 / 1e9, exact_post as f64 / 1e9
                );

                self.append_live_trade_log(pkg, opp, "LANDED_CONFIRMED", exact_pre, exact_post, actual_profit_lamports as f64 / 1e9);
            } else {
                self.append_live_trade_log(pkg, opp, "DROPPED_BY_VALIDATOR", exact_pre, exact_post, 0.0);
            }
        } else {
            self.append_live_trade_log(pkg, opp, "DROPPED_BY_VALIDATOR", current_bal, current_bal, 0.0);
        }
    }

    fn append_live_trade_log(
        &self,
        pkg: &SignedCircularPackage,
        opp: &ArbitrageOpportunity,
        status: &str,
        pre_lamports: u64,
        post_lamports: u64,
        net_profit_sol: f64,
    ) {
        let now = Utc::now();
        let log_entry = serde_json::json!({
            "trade_id": format!("tri_{}_{}", now.timestamp_millis(), &opp.mint[..4.min(opp.mint.len())]),
            "timestamp_ms": now.timestamp_millis(),
            "iso_time": now.to_rfc3339(),
            "engine_type": "TRIANGULAR_3HOP",
            "mint": opp.mint,
            "symbol": opp.symbol,
            "position_size_sol": pkg.input_lamports as f64 / 1e9,
            "expected_net_profit_sol": net_profit_sol,
            "jito_tip_sol": pkg.tip_lamports as f64 / 1e9,
            "live_status": status,
            "bundle_signature": pkg.tx_signature_base58,
            "pre_balance_sol": pre_lamports as f64 / 1e9,
            "post_balance_sol": post_lamports as f64 / 1e9,
        });

        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&self.live_trades_log_path) {
            let _ = writeln!(file, "{}", log_entry.to_string());
        }
    }
}
