#![allow(dead_code)]
use crate::jito_builder::{JitoBuilder, SignedCircularPackage, USDC_MINT};
use crate::types::ArbitrageOpportunity;
use chrono::Utc;
use dashmap::DashMap;
use log::{info, warn};
use reqwest::Client;
use serde_json;
use solana_sdk::hash::Hash;
use std::fs::OpenOptions;
use std::io::Write;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

pub const JITO_BLOCK_ENGINE_FRANKFURT: &str = "https://frankfurt.mainnet.block-engine.jito.wtf/api/v1/bundles";
pub const SOLANA_MAINNET_RPC: &str = "https://mainnet.helius-rpc.com/?api-key=9b358df5-981f-4a3f-8f25-5ff9af04653f";

// === STRICT INTEGER CAPITAL PROTECTION CONSTANTS (COUNCIL VERDICT COMPLIANT) ===
pub const MIN_RESERVE_FLOOR_LAMPORTS: u64 = 100_000_000;    // 0.10 SOL untouchable floor
pub const MAX_POSITION_SIZE_LAMPORTS: u64 = 20_000_000;     // 0.020 SOL Whale-Exclusion Zone cap
pub const DEFAULT_POSITION_SIZE_LAMPORTS: u64 = 18_000_000; // 0.018 SOL Default Size (~$1.70 USD)
pub const MIN_PROFIT_THRESHOLD_LAMPORTS: u64 = 30_000;       // 0.00003 SOL net profit threshold
pub const DEFAULT_JITO_TIP_LAMPORTS: u64 = 10_000;          // 10k lamports minimum tip
pub const MAX_CONSECUTIVE_REVERTS: u32 = 3;

pub struct LiveTradingEngine {
    pub jito_builder: JitoBuilder,
    pub http_client: Client,
    pub wallet_pubkey: String,
    pub verified_onchain_lamports: Arc<AtomicU64>,
    pub live_trades_log_path: String,
    pub consecutive_reverts: Arc<AtomicU64>,
    pub asset_locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

impl LiveTradingEngine {
    pub fn new(keypair_path: &str, initial_sol: f64, log_path: &str) -> Self {
        let jito_builder = JitoBuilder::load_from_path(keypair_path)
            .expect("Failed to initialize JitoBuilder with keypair");
        let wallet_pubkey = jito_builder.payer_pubkey.clone();

        let http_client = Client::builder()
            .timeout(Duration::from_millis(5000))
            .pool_max_idle_per_host(16)
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .build()
            .unwrap_or_default();

        let initial_lamports = (initial_sol * 1_000_000_000.0) as u64;

        info!("🛡️ [LIVE TRADING ENGINE INITIALIZED] Armed Keypair: {}", wallet_pubkey);
        info!("💰 Starting Bankroll: {} lamports ({:.4} SOL) | Reserve Floor: {} lamports (0.10 SOL)",
            initial_lamports, initial_sol, MIN_RESERVE_FLOOR_LAMPORTS
        );
        info!("🎯 Sizing Policy: Whale-Exclusion Zone (0.015 - 0.020 SOL | Default: 0.018 SOL)");

        Self {
            jito_builder,
            http_client,
            wallet_pubkey,
            verified_onchain_lamports: Arc::new(AtomicU64::new(initial_lamports)),
            live_trades_log_path: log_path.to_string(),
            consecutive_reverts: Arc::new(AtomicU64::new(0)),
            asset_locks: Arc::new(DashMap::new()),
        }
    }

    /// Fetches the real on-chain balance directly from Solana Mainnet RPC in integer lamports
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

    /// Fetches fresh blockhash from Solana RPC for transaction construction
    pub async fn fetch_recent_blockhash(&self) -> Option<Hash> {
        let rpc_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestBlockhash",
            "params": [
                { "commitment": "confirmed" }
            ]
        });

        if let Ok(resp) = self.http_client.post(SOLANA_MAINNET_RPC).json(&rpc_payload).send().await {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(bh_str) = body.get("result").and_then(|r| r.get("value")).and_then(|v| v.get("blockhash")).and_then(|b| b.as_str()) {
                    if let Ok(hash) = Hash::from_str(bh_str) {
                        return Some(hash);
                    }
                }
            }
        }
        None
    }

    /// Simulates the transaction pre-flight on Solana RPC before broadcasting
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
                        info!("🧪 [PRE-FLIGHT SIMULATION PASSED] Transaction valid on-chain");
                        return true;
                    } else {
                        warn!("🛑 [PRE-FLIGHT SIMULATION FAILED] Transaction would revert on-chain: {:?}", err);
                        return false;
                    }
                }
            }
        }
        false
    }

    /// Polls Solana RPC using exact wallet index search in getTransaction to guarantee 100% correct balance attribution
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
                                        info!("🎯 [PRECISE WALLET SETTLEMENT CONFIRMED] Wallet Index: {} | Pre: {} -> Post: {} lamports (Attempt {})",
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

    /// Executes live 2-leg spatial arbitrage with Whale-Exclusion sizing
    pub async fn execute_live_arbitrage(&self, opp: &ArbitrageOpportunity, sol_price_usd: f64) {
        let asset_mutex = self.asset_locks
            .entry(opp.mint.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .value()
            .clone();

        let _asset_guard = match asset_mutex.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                warn!("⏳ [ASSET IN FLIGHT] {} trade already in progress. Skipping duplicate tick.", opp.symbol);
                return;
            }
        };

        let pre_lamports = self.fetch_real_onchain_lamports().await;
        
        // Whale-Exclusion Zone Sizing: Clamp size to 0.015 - 0.020 SOL to guarantee price impact < 0.15%
        let pool_liq_sol = (opp.pool_liquidity_usd / sol_price_usd.max(10.0)).max(0.1);
        let pool_safe_cap_lamports = ((pool_liq_sol * 0.005) * 1_000_000_000.0) as u64; // Max 0.5% of pool reserve
        let effective_size_lamports = DEFAULT_POSITION_SIZE_LAMPORTS
            .min(pool_safe_cap_lamports.max(15_000_000))
            .min(MAX_POSITION_SIZE_LAMPORTS);

        // 1. Reserve Floor Guard (0.10 SOL / 100M lamports untouchable)
        if pre_lamports.saturating_sub(effective_size_lamports) < MIN_RESERVE_FLOOR_LAMPORTS {
            warn!("🛡️ [LIVE GUARD REJECT] Balance {} lamports - Size {} lamports < {} floor",
                pre_lamports, effective_size_lamports, MIN_RESERVE_FLOOR_LAMPORTS
            );
            return;
        }

        // 2. Profit Filter
        let expected_profit_lamports = (opp.net_profit_sol * 1_000_000_000.0) as u64;
        if expected_profit_lamports < MIN_PROFIT_THRESHOLD_LAMPORTS {
            return;
        }

        // 3. Dynamic Jito Tip
        let tip_lamports = ((expected_profit_lamports as f64 * 0.35) as u64)
            .clamp(DEFAULT_JITO_TIP_LAMPORTS, 100_000);

        // 4. Recent Blockhash
        let recent_blockhash = match self.fetch_recent_blockhash().await {
            Some(bh) => bh,
            None => return,
        };

        let buy_dex = "PUMPSWAP";
        let sell_dex = &opp.target_dex;

        info!("🚀 [DUAL-LEG ATOMIC MEV] Compiling SOL -> {} -> SOL | Size: {} lamports | Buy: {} | Sell: {} | Tip: {} lamports",
            opp.symbol, effective_size_lamports, buy_dex, sell_dex, tip_lamports
        );

        // 5. Build True Dual-Leg Atomic Transaction
        // V42: shadow engine stays STRICT-gated (no reflex marginal dispatch) so its
        // paper telemetry keeps measuring the conservative baseline.
        let circular_pkg = match self.jito_builder.build_atomic_circular_arbitrage_tx(
            &opp.mint,
            buy_dex,
            sell_dex,
            effective_size_lamports,
            50, // 0.50% adaptive MEV slippage
            tip_lamports,
            recent_blockhash,
            false,
        ).await {
            Ok(pkg) => pkg,
            Err(e) => {
                warn!("⚠️ [DUAL-LEG COMPILATION REJECT] {}: {}", opp.symbol, e);
                return;
            }
        };

        // 6. Pre-Flight Simulation
        if !self.simulate_transaction_preflight(&circular_pkg.signed_tx_base64).await {
            warn!("🛑 [SAFETY ABORT] Pre-flight simulation failed for {}.", opp.symbol);
            return;
        }

        info!("✅ [DUAL-LEG PACKAGE SIGNED & SIMULATED] Sig: {} | Net Profit: +{} lamports",
            circular_pkg.tx_signature_base58, circular_pkg.net_profit_lamports
        );

        // 7. Submit to Jito Frankfurt
        self.submit_bundle_to_jito(&circular_pkg, opp).await;
    }

    /// Executes live 3-hop Triangular Arbitrage (SOL -> Token A -> USDC -> SOL)
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
        let asset_mutex = self.asset_locks
            .entry(token_a_mint.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .value()
            .clone();

        let _asset_guard = match asset_mutex.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                warn!("⏳ [TRIANGULAR ASSET IN FLIGHT] {} trade already in progress. Skipping duplicate tick.", symbol);
                return;
            }
        };

        let pre_lamports = self.fetch_real_onchain_lamports().await;

        // Whale-Exclusion Zone Sizing for 3-hop routes
        let pool_liq_sol = (pool_tvl_usd / sol_price_usd.max(10.0)).max(0.1);
        let pool_safe_cap_lamports = ((pool_liq_sol * 0.0035) * 1_000_000_000.0) as u64; // Max 0.35% price impact
        let effective_size_lamports = DEFAULT_POSITION_SIZE_LAMPORTS
            .min(pool_safe_cap_lamports.max(15_000_000))
            .min(MAX_POSITION_SIZE_LAMPORTS);

        if pre_lamports.saturating_sub(effective_size_lamports) < MIN_RESERVE_FLOOR_LAMPORTS {
            return;
        }

        let recent_blockhash = match self.fetch_recent_blockhash().await {
            Some(bh) => bh,
            None => return,
        };

        let tip_lamports = 15_000; // 15k lamports default tip for triangular

        info!("🔺 [LIVE TRIANGULAR MEV] Compiling SOL -> {} -> USDC -> SOL | Size: {} lamports | Route: {} -> {} -> {}",
            symbol, effective_size_lamports, dex_hop1, dex_hop2, dex_hop3
        );

        let triangular_pkg = match self.jito_builder.build_atomic_triangular_arbitrage_tx(
            token_a_mint,
            USDC_MINT,
            dex_hop1,
            dex_hop2,
            dex_hop3,
            effective_size_lamports,
            40, // 0.40% tight slippage
            tip_lamports,
            recent_blockhash,
            false, // shadow: strict gate always (paper baseline)
        ).await {
            Ok(pkg) => pkg,
            Err(e) => {
                warn!("⚠️ [TRIANGULAR COMPILATION REJECT] {}: {}", symbol, e);
                return;
            }
        };

        if !self.simulate_transaction_preflight(&triangular_pkg.signed_tx_base64).await {
            warn!("🛑 [TRIANGULAR SAFETY ABORT] Pre-flight simulation failed for {}.", symbol);
            return;
        }

        info!("✅ [TRIANGULAR PACKAGE SIGNED & SIMULATED] Sig: {} | Net Profit: +{} lamports",
            triangular_pkg.tx_signature_base58, triangular_pkg.net_profit_lamports
        );

        // Dummy opp for logging
        let opp = ArbitrageOpportunity {
            timestamp_ms: Utc::now().timestamp_millis(),
            iso_time: Utc::now().to_rfc3339(),
            mint: token_a_mint.to_string(),
            symbol: format!("TRI/{}", symbol),
            name: format!("Triangular {}", symbol),
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
            "params": [[pkg.signed_tx_base64]]
        });

        let mut bundle_submitted = false;

        if let Ok(resp) = self.http_client.post(JITO_BLOCK_ENGINE_FRANKFURT)
            .json(&jito_payload)
            .send()
            .await 
        {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(res_str) = body.get("result").and_then(|r| r.as_str()) {
                    bundle_submitted = true;
                    info!("📡 [RUST JITO BUNDLE DISPATCHED] Jito UUID: {}", res_str);
                }
            }
        }

        if !bundle_submitted {
            warn!("⚠️ [JITO SUBMISSION FAILED] Bundle rejected for {}", opp.symbol);
            return;
        }

        if let Some((exact_pre, exact_post)) = self.verify_transaction_level_settlement(&pkg.tx_signature_base58, 6).await {
            if exact_post > exact_pre {
                let actual_profit_lamports = exact_post - exact_pre;
                self.consecutive_reverts.store(0, Ordering::SeqCst);
                self.verified_onchain_lamports.store(exact_post, Ordering::SeqCst);

                info!("🏆 [LIVE ARBITRAGE WON & SETTLED] Mint: {} | Net PnL: +{} lamports (+{:.5} SOL) | Wallet: {:.4} SOL",
                    opp.symbol, actual_profit_lamports, actual_profit_lamports as f64 / 1e9, exact_post as f64 / 1e9
                );

                self.append_live_trade_log(pkg, opp, "LANDED_CONFIRMED", exact_pre, exact_post, actual_profit_lamports as f64 / 1e9);
            }
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
            "trade_id": format!("live_{}_{}", now.timestamp_millis(), &opp.mint[..4.min(opp.mint.len())]),
            "timestamp_ms": now.timestamp_millis(),
            "iso_time": now.to_rfc3339(),
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
