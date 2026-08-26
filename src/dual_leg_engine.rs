#![allow(dead_code)]
use crate::jito_builder::{JitoBuilder, SignedCircularPackage};
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

// === DUAL-LEG SPECIFIC SAFETY PROTOCOL CONSTANTS ===
pub const MIN_RESERVE_FLOOR_LAMPORTS: u64 = 150_000_000;    // 0.15 SOL untouchable floor
pub const MAX_DUAL_POSITION_LAMPORTS: u64 = 300_000_000;    // 0.300 SOL Max Cap
pub const DEFAULT_DUAL_POSITION_LAMPORTS: u64 = 50_000_000; // 0.050 SOL Default Baseline Position
pub const MIN_DUAL_PROFIT_LAMPORTS: u64 = 20_000;           // 0.000020 SOL net profit threshold
pub const DEFAULT_DUAL_JITO_TIP: u64 = 20_000;              // 20k lamports baseline tip
pub const MAX_DUAL_PRICE_IMPACT_BPS: u64 = 50;              // Max 0.50% pool reserve impact

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DualLegConfig {
    pub wallet_pubkey: String,
    pub wallet_balance_sol: f64,
    pub is_engine_running: bool,
    pub max_position_sol: f64,
    pub min_spread_pct: f64,
    pub min_net_profit_sol: f64,
    pub max_slippage_pct: f64,
    pub jito_tip_pct: f64,
    pub reserve_floor_sol: f64,
    pub status_label: String,
}

impl Default for DualLegConfig {
    fn default() -> Self {
        Self {
            wallet_pubkey: "AA1ihbehbPrMW4L9jWtdxJfkH76Up5F2azbeepQ1wixs".to_string(),
            wallet_balance_sol: 0.3981,
            is_engine_running: true,
            max_position_sol: 0.035,
            min_spread_pct: 0.50,
            min_net_profit_sol: 0.00010,
            max_slippage_pct: 0.50,
            jito_tip_pct: 35.0,
            reserve_floor_sol: 0.10,
            status_label: "DUAL-LEG ACTIVE (SOL ➔ Token ➔ SOL)".to_string(),
        }
    }
}

pub struct DualLegLiveEngine {
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

impl DualLegLiveEngine {
    pub fn new(keypair_path: &str, initial_sol: f64, log_path: &str, config_path: &str) -> Self {
        let jito_builder = JitoBuilder::load_from_path(keypair_path)
            .expect("Failed to initialize JitoBuilder for Dual-Leg engine");
        let wallet_pubkey = jito_builder.payer_pubkey.clone();

        let http_client = Client::builder()
            .timeout(Duration::from_millis(5000))
            .pool_max_idle_per_host(16)
            .user_agent("Mozilla/5.0 (DualLegMev; Linux x86_64)")
            .build()
            .unwrap_or_default();

        let initial_lamports = (initial_sol * 1_000_000_000.0) as u64;

        info!("🛡️ [DUAL-LEG LIVE ENGINE INITIALIZED] Armed Keypair: {}", wallet_pubkey);
        info!("💰 Dual-Leg Bankroll: {} lamports ({:.4} SOL) | Floor: 0.10 SOL | Max Size: 0.035 SOL",
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
                            if let Ok(bh) = Hash::from_str(bh_str) {
                                *bh_clone.write().await = Some((bh, std::time::Instant::now()));
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

    pub fn load_config(&self) -> DualLegConfig {
        if let Ok(content) = fs::read_to_string(&self.config_path) {
            if let Ok(cfg) = serde_json::from_str::<DualLegConfig>(&content) {
                return cfg;
            }
        }
        DualLegConfig::default()
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
                        info!("🧪 [DUAL-LEG SIMULATION PASSED] Valid on-chain");
                        return true;
                    } else {
                        warn!("🛑 [DUAL-LEG SIMULATION FAILED] Revert detected: {:?}", err);
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
                                        info!("🎯 [DUAL-LEG SETTLEMENT CONFIRMED] Wallet Index: {} | Pre: {} -> Post: {} lamports (Attempt {})",
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

    /// Executes live 2-leg spatial arbitrage: SOL -> Token -> SOL
    pub async fn execute_dual_leg_arbitrage(&self, opp: &ArbitrageOpportunity, sol_price_usd: f64) {
        let config = self.load_config();
        if !config.is_engine_running {
            return;
        }

        // V42 KILL SWITCH: instant operator halt, ahead of any quote spend.
        if crate::jito::is_trading_killed() {
            return;
        }

        if self.is_token_in_cooldown(&opp.mint) {
            return;
        }

        // Non-blocking concurrency lock on token mint
        let asset_mutex = self.asset_locks
            .entry(opp.mint.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .value()
            .clone();

        let _asset_guard = match asset_mutex.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                warn!("⏳ [DUAL-LEG IN FLIGHT] {} trade already in progress. Skipping.", opp.symbol);
                return;
            }
        };

        let pre_lamports = self.fetch_real_onchain_lamports().await;

        // 1. Reserve Floor Guard & Available Headroom Check
        let reserve_floor_lamports = (config.reserve_floor_sol * 1_000_000_000.0) as u64;
        let max_safe_headroom = pre_lamports.saturating_sub(reserve_floor_lamports);
        if max_safe_headroom < 25_000_000 {
            warn!("🛡️ [DUAL-LEG RESERVE FLOOR] Available headroom {} lamports < 0.025 SOL floor", max_safe_headroom);
            return;
        }

        // Fix 2: Exclude Pump.fun bonding curves from dual-leg execution
        if opp.buy_dex.to_lowercase().contains("pump") || opp.sell_dex.to_lowercase().contains("pump") {
            return;
        }

        // V41: Dynamic sizing based on active pool depth. Depth cap = 0.5% of per-side
        // reserves (~1% price impact per leg on constant-product pools); the Jupiter
        // confirmed-profit gate remains the final arbiter of whether the size survives.
        let pool_reserves_sol = (opp.pool_liquidity_usd / 2.0) / sol_price_usd.max(10.0);
        let max_depth_cap_lamports = ((pool_reserves_sol * 0.005) * 1_000_000_000.0) as u64;
        let optimal_swap_lamports = (opp.simulated_swap_sol * 1_000_000_000.0) as u64;
        let config_max_lamports = (config.max_position_sol * 1_000_000_000.0) as u64;

        let effective_size_lamports = optimal_swap_lamports
            .min(max_depth_cap_lamports)
            .min(config_max_lamports)
            .min(max_safe_headroom)
            .min(MAX_DUAL_POSITION_LAMPORTS)
            .clamp(20_000_000, 300_000_000);

        // 2. In-RAM Exact AMM Math Pre-Filter (GLM-5.2 Lead Architecture Directive)
        let expected_profit_lamports = (opp.net_profit_sol * 1_000_000_000.0) as u64;
        let min_profit_lamports = (config.min_net_profit_sol * 1_000_000_000.0) as u64;
        if expected_profit_lamports < min_profit_lamports.max(MIN_DUAL_PROFIT_LAMPORTS) {
            return;
        }

        // Drop unviable shallow-depth opportunities in RAM (<1µs) before entering network I/O
        if opp.pool_liquidity_usd < 1500.0 {
            return;
        }

        // 3. V41: Escalated Jito Tip estimate (60% share, 25k floor, 1.5M ceiling —
        //    matches the builder's confirmed-profit tip so pre-build logs don't understate bids)
        let tip_lamports = ((expected_profit_lamports as f64 * (config.jito_tip_pct / 100.0)) as u64)
            .clamp(25_000, 1_500_000);

        // 4. Fetch Recent Blockhash
        let recent_blockhash = match self.fetch_recent_blockhash().await {
            Some(bh) => bh,
            None => return,
        };

        let buy_dex = &opp.buy_dex;
        let sell_dex = if !opp.sell_dex.is_empty() { &opp.sell_dex } else { &opp.target_dex };

        info!("🚀 [DUAL-LEG ATOMIC MEV] SOL -> {} -> SOL | Size: {} lamports | Buy: {} | Sell: {} | Tip: {} lamports",
            opp.symbol, effective_size_lamports, buy_dex, sell_dex, tip_lamports
        );

        // ========== JUPITER PRE-VALIDATION (V31) — DISABLED ==========
        // V33: Bypassed. On-chain atomic slippage (otherAmountThreshold) in build_atomic
        // already guarantees 100% revert if price moves against us. This 50ms HTTP
        // round-trip was redundant — the same Jupiter quotes are fetched again inside
        // build_atomic_circular_arbitrage_tx to compile swap instructions.
        // Set to true to re-enable if needed.
        const ENABLE_PREFLIGHT_QUOTE_CHECK: bool = false;
        if ENABLE_PREFLIGHT_QUOTE_CHECK {
            let sol_mint = "So11111111111111111111111111111111111111112";
            let pre_quote_a = self.jito_builder.fetch_jupiter_quote(
                sol_mint, &opp.mint, effective_size_lamports, 25, Some(buy_dex)
            ).await;
            let intermediate_tokens: u64 = pre_quote_a.get("otherAmountThreshold")
                .or_else(|| pre_quote_a.get("outAmount"))
                .and_then(|v| v.as_str())
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);

            if intermediate_tokens == 0 {
                warn!("🚫 [JUPITER PRE-CHECK FAIL] {} | Leg A returned 0 tokens — skipping", opp.symbol);
                self.set_token_cooldown(&opp.mint, 10_000);
                return;
            }

            let pre_quote_b = self.jito_builder.fetch_jupiter_quote(
                &opp.mint, sol_mint, intermediate_tokens, 25, Some(sell_dex)
            ).await;
            let final_sol_out: u64 = pre_quote_b.get("outAmount")
                .or_else(|| pre_quote_b.get("otherAmountThreshold"))
                .and_then(|v| v.as_str())
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);

            let min_profitable = effective_size_lamports + 34_000; // input + gas floor
            if final_sol_out <= min_profitable {
                warn!("🚫 [JUPITER PRE-CHECK REJECT] {} | Jupiter: {}→{} lamports (need >{}) | Scanner spread: +{:.2}% is PHANTOM",
                    opp.symbol, effective_size_lamports, final_sol_out, min_profitable, opp.gross_spread_pct
                );
                self.set_token_cooldown(&opp.mint, 8_000);
                return;
            }

            let jupiter_profit = final_sol_out.saturating_sub(effective_size_lamports);
            info!("✅ [JUPITER PRE-CHECK PASS] {} | Jupiter confirms +{} lamports profit | Proceeding to atomic build",
                opp.symbol, jupiter_profit
            );
        }
        // ========== END JUPITER PRE-VALIDATION ==========

        let slippage_bps = (config.max_slippage_pct * 100.0) as u32;
        // V42 REFLEX: LaserStream event-fresh triggers unlock marginal/negative-quote
        // dispatch (race zone, revert ≈ gas). Stale poll triggers stay strict.
        let allow_marginal_dispatch = crate::helius_stream::is_event_fresh(&opp.mint);
        let circular_pkg = match self.jito_builder.build_atomic_circular_arbitrage_tx(
            &opp.mint,
            buy_dex,
            sell_dex,
            effective_size_lamports,
            slippage_bps.clamp(20, 100),
            tip_lamports,
            recent_blockhash,
            allow_marginal_dispatch,
        ).await {
            Ok(pkg) => pkg,
            Err(e) => {
                warn!("⚠️ [DUAL-LEG COMPILATION REJECT] {}: {}", opp.symbol, e);
                // V42c: feed competitor correlation — this attempt was gate-rejected.
                crate::mriy_watch::record_attempt(&opp.mint, &opp.symbol, opp.net_spread_pct, false, allow_marginal_dispatch);
                self.set_token_cooldown(&opp.mint, 15_000);
                return;
            }
        };

        info!("⚡ [ZERO-DELAY DIRECT DISPATCH] Dispatching in-memory atomic bundle to Jito Frankfurt: {} | Sig: {} | Net Profit: +{} lamports",
            opp.symbol, circular_pkg.tx_signature_base58, circular_pkg.net_profit_lamports
        );
        // V42c: dispatched attempt also feeds correlation (auction-loss analysis).
        crate::mriy_watch::record_attempt(&opp.mint, &opp.symbol, opp.net_spread_pct, true, allow_marginal_dispatch);

        self.submit_bundle_to_jito(&circular_pkg, opp).await;
    }

    async fn submit_bundle_to_jito(&self, pkg: &SignedCircularPackage, opp: &ArbitrageOpportunity) {
        let jito_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendBundle",
            "params": [[pkg.signed_tx_base58]]
        });

        // V42: race Frankfurt + Amsterdam + Dublin block engines concurrently —
        // whichever region's leader slot comes first can include us.
        let bundle_submitted = match crate::jito::submit_bundle_multiregion(&self.http_client, &jito_payload).await {
            Ok(uuid) => {
                info!("📡 [DUAL-LEG JITO DISPATCHED] Jito UUID: {} | multi-region (FRA/AMS/DUB)", uuid);
                crate::jito::spawn_bundle_outcome_tracker(
                    self.http_client.clone(),
                    uuid,
                    format!("DUAL/{}", opp.symbol),
                    pkg.tip_lamports,
                );
                true
            }
            Err(e) => {
                warn!("⚠️ [DUAL-LEG JITO REJECT] {}", e);
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

                info!("🏆 [DUAL-LEG ARBITRAGE WON & SETTLED] Mint: {} | Net PnL: +{} lamports (+{:.5} SOL) | Wallet: {:.4} SOL",
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
            "trade_id": format!("dual_{}_{}", now.timestamp_millis(), &opp.mint[..4.min(opp.mint.len())]),
            "timestamp_ms": now.timestamp_millis(),
            "iso_time": now.to_rfc3339(),
            "engine_type": "DUAL_LEG_SPATIAL",
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
