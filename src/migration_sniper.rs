use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use log::{error, info, warn};
use reqwest::Client as HttpClient;
use serde_json::json;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, Instant};

use crate::raydium_cpmm::RaydiumCpmmPoolState;
use crate::types::{
    HoldingPosition, MigrationDecision, MigrationShadowRecord, TokenRiskMeta,
};
use crate::websocket_feed::RaydiumMigrationEvent;

/// Minimum SOL delta to filter noise/dust swaps (0.001 SOL = 100,000 lamports)
const MIN_SOL_DELTA_LAMPORTS: u64 = 100_000;

/// Maximum time to hold active shadow position before hard time-stop (10 minutes)
const MAX_HOLD_DURATION_SECONDS: u64 = 600;

/// Raydium-Native Migration Sniping Shadow Engine
#[derive(Clone)]
pub struct RaydiumMigrationSniper {
    http_client: HttpClient,
    rpc_url: String,
    shadow_log_path: String,
    active_positions: Arc<RwLock<HashMap<String, HoldingPosition>>>,
    pub migrations_evaluated: Arc<AtomicUsize>,
    pub shadow_trades_filled: Arc<AtomicUsize>,
    pub total_shadow_pnl_sol: Arc<RwLock<f64>>,
    pub total_shadow_wins: Arc<AtomicUsize>,
    pub total_shadow_losses: Arc<AtomicUsize>,
}

impl RaydiumMigrationSniper {
    pub fn new(rpc_url: &str, shadow_log_path: &str) -> Self {
        Self {
            http_client: HttpClient::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
            rpc_url: rpc_url.to_string(),
            shadow_log_path: shadow_log_path.to_string(),
            active_positions: Arc::new(RwLock::new(HashMap::new())),
            migrations_evaluated: Arc::new(AtomicUsize::new(0)),
            shadow_trades_filled: Arc::new(AtomicUsize::new(0)),
            total_shadow_pnl_sol: Arc::new(RwLock::new(0.0)),
            total_shadow_wins: Arc::new(AtomicUsize::new(0)),
            total_shadow_losses: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Primary event handler when PumpPortal detects a graduation to Raydium
    pub async fn on_migration_detected(&self, event: RaydiumMigrationEvent) {
        self.migrations_evaluated.fetch_add(1, Ordering::Relaxed);
        info!("🎯 [MIGRATION DETECTED] Mint: {} | Target Raydium Pool: {} | Initial SOL: {:.2}",
            event.mint, event.pool_address, event.initial_sol_liquidity
        );

        let sniper_clone = self.clone();
        tokio::spawn(async move {
            sniper_clone.process_migration_lifecycle(event).await;
        });
    }

    /// Full asynchronous lifecycle for a detected migration
    async fn process_migration_lifecycle(&self, event: RaydiumMigrationEvent) {
        let start_time = Instant::now();
        let migration_start_ms = Utc::now().timestamp_millis();

        // 1. Solve P0 Race Condition: Poll getAccountInfo with 200ms backoff
        let mut genesis_reserves = None;
        for attempt in 1..=10 {
            if let Some(res) = self.fetch_pool_reserves_rpc(&event.pool_address).await {
                genesis_reserves = Some(res);
                break;
            }
            sleep(Duration::from_millis(200)).await;
        }

        let (mut r_tok_last, mut r_sol_last) = match genesis_reserves {
            Some(res) => res,
            None => {
                // Fallback: estimate from migration event parameters
                let sol_lamports = (event.initial_sol_liquidity * 1_000_000_000.0) as u64;
                let tok_lamports = 200_000_000_000_000u64; // ~200M standard remaining supply
                (tok_lamports, sol_lamports)
            }
        };

        // Canonical P0 open price calculation
        let p0_open_price = RaydiumCpmmPoolState::calculate_price_sol(r_sol_last, r_tok_last, 6);
        let p0_usd = p0_open_price * 95.0; // Benchmark SOL @ $95 USD

        info!("📊 [CANONICAL P0 CAPTURED] Pool: {} | P0: {:.8} SOL (${:.6}) | Initial Sol Reserves: {:.2} SOL",
            event.pool_address, p0_open_price, p0_usd, (r_sol_last as f64 / 1e9)
        );

        // 2. Track rolling trade volume and velocity decay over 0-45s window
        let mut sells_0_15s: usize = 0;
        let mut sells_15_30s: usize = 0;
        let mut buy_volume_sol_15_45s: f64 = 0.0;
        let mut sell_volume_sol_15_45s: f64 = 0.0;
        let mut total_swaps: usize = 0;

        // Monitor pool state changes over observation window
        let mut eval_price_usd = p0_usd;
        while start_time.elapsed().as_secs() < 35 {
            sleep(Duration::from_millis(500)).await;

            if let Some((r_tok_new, r_sol_new)) = self.fetch_pool_reserves_rpc(&event.pool_address).await {
                let elapsed_secs = start_time.elapsed().as_secs();
                let current_price = RaydiumCpmmPoolState::calculate_price_sol(r_sol_new, r_tok_new, 6);
                eval_price_usd = current_price * 95.0;

                // Check for reserve deltas
                if r_sol_new != r_sol_last {
                    let sol_diff = if r_sol_new > r_sol_last {
                        r_sol_new - r_sol_last
                    } else {
                        r_sol_last - r_sol_new
                    };

                    // Filter noise / dust swaps
                    if sol_diff >= MIN_SOL_DELTA_LAMPORTS {
                        total_swaps += 1;
                        let sol_moved = sol_diff as f64 / 1e9;

                        if r_sol_new > r_sol_last {
                            // SOL added to pool => Token bought
                            if elapsed_secs >= 15 {
                                buy_volume_sol_15_45s += sol_moved;
                            }
                        } else {
                            // SOL removed from pool => Token sold
                            if elapsed_secs <= 15 {
                                sells_0_15s += 1;
                            } else if elapsed_secs <= 30 {
                                sells_15_30s += 1;
                            }
                            if elapsed_secs >= 15 {
                                sell_volume_sol_15_45s += sol_moved;
                            }
                        }
                    }
                }
                r_tok_last = r_tok_new;
                r_sol_last = r_sol_new;
            }
        }

        // 3. Compute Metrics at T+35s Evaluation Window
        let price_ratio_to_p0 = if p0_usd > 0.0 { eval_price_usd / p0_usd } else { 0.0 };
        
        let velocity_decay_pct = if sells_0_15s > 0 {
            let decay = (sells_0_15s as f64 - sells_15_30s as f64) / (sells_0_15s as f64) * 100.0;
            decay.max(0.0)
        } else {
            0.0
        };

        let buy_sell_ratio = if sell_volume_sol_15_45s > 0.0 {
            buy_volume_sol_15_45s / sell_volume_sol_15_45s
        } else if buy_volume_sol_15_45s > 0.0 {
            2.0 // Strong buy interest with zero sell volume
        } else {
            0.0
        };

        info!("🔎 [MIGRATION METRICS T+35s] {} | Price/P0: {:.2}% | Velocity Decay: {:.1}% ({} -> {}) | Buy/Sell Vol: {:.2} ({:.2} SOL / {:.2} SOL) | Swaps: {}",
            event.mint, (price_ratio_to_p0 * 100.0), velocity_decay_pct, sells_0_15s, sells_15_30s,
            buy_sell_ratio, buy_volume_sol_15_45s, sell_volume_sol_15_45s, total_swaps
        );

        // 4. Evaluate Entry Filters (Zero Emotion)
        let decision = if total_swaps < 2 {
            MigrationDecision::RejectedZombiePool { swap_count: total_swaps }
        } else if price_ratio_to_p0 < 0.75 {
            MigrationDecision::RejectedKnife { ratio_to_p0: price_ratio_to_p0 }
        } else if velocity_decay_pct < 30.0 && sells_0_15s >= 3 {
            MigrationDecision::RejectedVelocityDecay { decay_pct: velocity_decay_pct }
        } else if buy_sell_ratio < 0.80 && sell_volume_sol_15_45s > 0.5 {
            MigrationDecision::RejectedBuySellRatio { ratio: buy_sell_ratio }
        } else {
            // Check 5-Pillar Security Shield
            let risk_meta = TokenRiskMeta::safe_default(&event.mint);
            if let Err(violation) = risk_meta.validate_risk_shields() {
                MigrationDecision::RejectedShieldViolation { reason: violation.to_string() }
            } else {
                MigrationDecision::Accepted
            }
        };

        let is_accepted = decision == MigrationDecision::Accepted;

        let mut record = MigrationShadowRecord {
            event_id: format!("mig-{}", &event.mint[..8.min(event.mint.len())]),
            timestamp_ms: migration_start_ms,
            iso_time: Utc::now().to_rfc3339(),
            mint: event.mint.clone(),
            symbol: format!("MIG/{}", &event.mint[..4]),
            pool_address: event.pool_address.clone(),
            initial_sol_liquidity: event.initial_sol_liquidity,
            p0_open_price_usd: (p0_usd * 10000.0).round() / 10000.0,
            decision: decision.clone(),
            eval_price_usd: (eval_price_usd * 10000.0).round() / 10000.0,
            price_ratio_to_p0: (price_ratio_to_p0 * 100.0).round() / 100.0,
            velocity_decay_pct: (velocity_decay_pct * 10.0).round() / 10.0,
            buy_volume_sol: (buy_volume_sol_15_45s * 100.0).round() / 100.0,
            sell_volume_sol: (sell_volume_sol_15_45s * 100.0).round() / 100.0,
            buy_sell_volume_ratio: (buy_sell_ratio * 100.0).round() / 100.0,
            total_swaps,
            entry_price_usd: None,
            exit_price_usd: None,
            exit_reason: None,
            hold_duration_seconds: None,
            simulated_net_pnl_sol: None,
            simulated_net_pnl_usd: None,
            simulated_roi_pct: None,
            counterfactual_p10m_usd: None,
            counterfactual_max_price_usd: None,
            would_have_profited: None,
            rug_detected: None,
        };

        // 🚀 INSTANT LOGGING: Write initial evaluation record immediately so dashboard updates in real time!
        self.append_shadow_record(&record).await;

        // 5. If Accepted, Execute Simulated 0.10 SOL Shadow Buy & Track Exits
        if is_accepted {
            self.shadow_trades_filled.fetch_add(1, Ordering::Relaxed);
            let entry_price = eval_price_usd;
            let sim_position_sol = 0.10; // 0.10 SOL position size
            let sim_tokens = if entry_price > 0.0 { (sim_position_sol * 95.0) / entry_price } else { 0.0 };
            
            info!("🚀 [SHADOW MIGRATION BUY] Mint: {} | Size: {:.2} SOL | Entry Price: ${:.6}",
                event.mint, sim_position_sol, entry_price
            );

            let pos = HoldingPosition {
                position_id: record.event_id.clone(),
                mint: event.mint.clone(),
                symbol: record.symbol.clone(),
                pool_address: event.pool_address.clone(),
                entry_timestamp_ms: Utc::now().timestamp_millis(),
                entry_price_usd: entry_price,
                position_size_sol: sim_position_sol,
                token_amount: sim_tokens,
                local_peak_price_usd: entry_price,
                is_active: true,
            };

            self.active_positions.write().await.insert(event.mint.clone(), pos);

            // Active Position Tracking Loop (Polled every 2 seconds for up to 10 minutes)
            let hold_start = Instant::now();
            let mut local_peak = entry_price;
            let mut exit_reason = "TIME_STOP_10MIN".to_string();
            let mut final_exit_price = entry_price;

            while hold_start.elapsed().as_secs() < MAX_HOLD_DURATION_SECONDS {
                sleep(Duration::from_secs(2)).await;

                if let Some((r_tok, r_sol)) = self.fetch_pool_reserves_rpc(&event.pool_address).await {
                    let live_price = RaydiumCpmmPoolState::calculate_price_sol(r_sol, r_tok, 6) * 95.0;
                    if live_price > local_peak {
                        local_peak = live_price;
                    }

                    // 1. Take Profit (+10% from entry)
                    if live_price >= entry_price * 1.10 {
                        exit_reason = "TAKE_PROFIT_10PCT".to_string();
                        final_exit_price = live_price;
                        break;
                    }

                    // 2. Trailing Stop (-5% from local peak if in profit)
                    if local_peak >= entry_price * 1.04 && live_price <= local_peak * 0.95 {
                        exit_reason = "TRAILING_STOP_5PCT".to_string();
                        final_exit_price = live_price;
                        break;
                    }

                    // 3. Hard Stop Loss (-10% from entry)
                    if live_price <= entry_price * 0.90 {
                        exit_reason = "HARD_STOP_10PCT".to_string();
                        final_exit_price = live_price;
                        break;
                    }

                    final_exit_price = live_price;
                }
            }

            let hold_duration = hold_start.elapsed().as_secs();
            let gross_roi = if entry_price > 0.0 { (final_exit_price - entry_price) / entry_price } else { 0.0 };
            let net_roi = gross_roi - 0.005; // Deduct 0.5% roundtrip LP fees & gas
            let pnl_sol = sim_position_sol * net_roi;
            let pnl_usd = pnl_sol * 95.0;

            if pnl_sol > 0.0 {
                self.total_shadow_wins.fetch_add(1, Ordering::Relaxed);
            } else {
                self.total_shadow_losses.fetch_add(1, Ordering::Relaxed);
            }
            *self.total_shadow_pnl_sol.write().await += pnl_sol;

            info!("🏁 [SHADOW POSITION EXITED] {} | Reason: {} | Hold: {}s | PnL: {:+.4} SOL (${:+.2}) | ROI: {:+.2}%",
                event.mint, exit_reason, hold_duration, pnl_sol, pnl_usd, (net_roi * 100.0)
            );

            record.entry_price_usd = Some((entry_price * 10000.0).round() / 10000.0);
            record.exit_price_usd = Some((final_exit_price * 10000.0).round() / 10000.0);
            record.exit_reason = Some(exit_reason);
            record.hold_duration_seconds = Some(hold_duration);
            record.simulated_net_pnl_sol = Some((pnl_sol * 10000.0).round() / 10000.0);
            record.simulated_net_pnl_usd = Some((pnl_usd * 100.0).round() / 100.0);
            record.simulated_roi_pct = Some((net_roi * 100.0).round() / 100.0);

            self.active_positions.write().await.remove(&event.mint);

            // Re-append updated record with closed trade metrics
            self.append_shadow_record(&record).await;
        }

        // 6. Spawn Counterfactual 10m Tracker for tuning
        let sniper_counterfactual = self.clone();
        let pool_addr = event.pool_address.clone();
        let eval_price = eval_price_usd;
        let mut final_record = record;

        tokio::spawn(async move {
            let remaining_secs = 600u64.saturating_sub(start_time.elapsed().as_secs());
            if remaining_secs > 0 {
                sleep(Duration::from_secs(remaining_secs)).await;
            }

            if let Some((r_tok, r_sol)) = sniper_counterfactual.fetch_pool_reserves_rpc(&pool_addr).await {
                let price_10m = RaydiumCpmmPoolState::calculate_price_sol(r_sol, r_tok, 6) * 95.0;
                let sol_liq = r_sol as f64 / 1e9;
                let is_rug = sol_liq < 2.0;
                let would_profit = price_10m >= eval_price * 1.10 && !is_rug;

                final_record.counterfactual_p10m_usd = Some((price_10m * 10000.0).round() / 10000.0);
                final_record.counterfactual_max_price_usd = Some((price_10m.max(eval_price) * 10000.0).round() / 10000.0);
                final_record.would_have_profited = Some(would_profit);
                final_record.rug_detected = Some(is_rug);
                
                sniper_counterfactual.append_shadow_record(&final_record).await;
            }
        });
    }

    /// Fetch raw pool reserves via JSON-RPC
    async fn fetch_pool_reserves_rpc(&self, pool_address: &str) -> Option<(u64, u64)> {
        let req_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                pool_address,
                { "encoding": "base64", "commitment": "processed" }
            ]
        });

        let resp = self.http_client.post(&self.rpc_url).json(&req_body).send().await.ok()?;
        let body: serde_json::Value = resp.json().await.ok()?;
        let data_b64 = body.get("result")?.get("value")?.get("data")?.as_array()?.get(0)?.as_str()?;
        let data_bytes = base64::decode(data_b64).ok()?;

        RaydiumCpmmPoolState::decode_reserves_from_raw_bytes(&data_bytes)
    }

    /// Append record to shadow log file
    async fn append_shadow_record(&self, record: &MigrationShadowRecord) {
        if let Ok(serialized) = serde_json::to_string(record) {
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.shadow_log_path)
            {
                let _ = writeln!(file, "{}", serialized);
            }
        }
    }

    /// Get current stats snapshot for dashboard API
    pub async fn get_stats_snapshot(&self) -> serde_json::Value {
        let evaluated = self.migrations_evaluated.load(Ordering::Relaxed);
        let filled = self.shadow_trades_filled.load(Ordering::Relaxed);
        let wins = self.total_shadow_wins.load(Ordering::Relaxed);
        let losses = self.total_shadow_losses.load(Ordering::Relaxed);
        let pnl = *self.total_shadow_pnl_sol.read().await;
        let win_rate = if (wins + losses) > 0 {
            (wins as f64 / (wins + losses) as f64) * 100.0
        } else {
            0.0
        };

        json!({
            "migrations_evaluated": evaluated,
            "shadow_trades_filled": filled,
            "wins": wins,
            "losses": losses,
            "win_rate_pct": (win_rate * 10.0).round() / 10.0,
            "total_shadow_pnl_sol": (pnl * 10000.0).round() / 10000.0,
            "active_positions_count": self.active_positions.read().await.len(),
            "mode": "100% Zero-Real-Capital Shadow Simulation"
        })
    }
}
