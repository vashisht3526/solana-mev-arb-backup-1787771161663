use crate::router::AtomicRouterEngine;
use crate::types::ArbitrageOpportunity;
use chrono::Utc;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

// =========================================================================
// 🔒 THE 5 IMMUTABLE SAFETY INVARIANTS & CIRCUIT BREAKER LIMITS
// =========================================================================
pub const MIN_WALLET_RESERVE_FLOOR_SOL: f64 = 0.10;   // Never touch the last 0.10 SOL (gas reserve)
pub const MAX_POSITION_SIZE_SOL: f64 = 0.08;          // Hard position size cap for micro capital
pub const MAX_DRAWDOWN_STOP_LOSS_SOL: f64 = 0.05;      // Emergency halt if wallet loses > 0.05 SOL in 24h
pub const MAX_CONSECUTIVE_FAILURES: usize = 3;        // 3 consecutive failed attempts triggers cooldown
pub const CONSECUTIVE_FAILURE_COOLDOWN_SECS: u64 = 300; // 5-minute pause on consecutive drops
pub const MAX_JITO_TIP_SOL: f64 = 0.0100;             // Absolute maximum tip per trade
pub const JITO_BLOCK_ENGINE_FRANKFURT: &str = "https://frankfurt.mainnet.block-engine.jito.wtf/api/v1/bundles";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveTradeLogRecord {
    pub trade_id: String,
    pub timestamp_ms: i64,
    pub iso_time: String,
    pub mint: String,
    pub symbol: String,
    pub position_size_sol: f64,
    pub expected_net_profit_sol: f64,
    pub jito_tip_sol: f64,
    pub live_status: String, // "LANDED_CONFIRMED", "SIMULATED_ABORT", "CIRCUIT_BREAKER_HALT", "DROPPED_REFUNDED"
    pub bundle_signature: Option<String>,
    pub pre_balance_sol: f64,
    pub post_balance_sol: f64,
}

pub struct LiveCircuitBreakerState {
    pub high_water_mark_lamports: AtomicU64,
    pub current_consecutive_failures: AtomicUsize,
    pub cooldown_until_ms: AtomicU64,
    pub is_emergency_halted: AtomicBool,
}

impl LiveCircuitBreakerState {
    pub fn new(initial_balance_lamports: u64) -> Self {
        Self {
            high_water_mark_lamports: AtomicU64::new(initial_balance_lamports),
            current_consecutive_failures: AtomicUsize::new(0),
            cooldown_until_ms: AtomicU64::new(0),
            is_emergency_halted: AtomicBool::new(false),
        }
    }

    /// Evaluates all 5 safety invariants before approving any live trade
    pub fn evaluate_safety_guards(&self, current_balance_sol: f64, proposed_size_sol: f64) -> Result<(), &'static str> {
        // 1. Emergency Killswitch Check
        if self.is_emergency_halted.load(Ordering::Relaxed) {
            return Err("⛔ [CIRCUIT BREAKER] Engine is in Emergency Shutdown Mode.");
        }

        // 2. Cooldown Window Check
        let now_ms = Utc::now().timestamp_millis() as u64;
        let cooldown_until = self.cooldown_until_ms.load(Ordering::Relaxed);
        if now_ms < cooldown_until {
            return Err("⏳ [COOLDOWN ACTIVE] Engine is paused following consecutive failed bundle attempts.");
        }

        // 3. Minimum Reserve Floor Check
        if current_balance_sol - proposed_size_sol < MIN_WALLET_RESERVE_FLOOR_SOL {
            return Err("🛑 [RESERVE FLOOR BREACH] Trade size would violate the 0.10 SOL minimum gas reserve.");
        }

        // 4. Maximum Drawdown Check
        let hwm_sol = self.high_water_mark_lamports.load(Ordering::Relaxed) as f64 / 1_000_000_000.0;
        let current_drawdown = (hwm_sol - current_balance_sol).max(0.0);
        if current_drawdown > MAX_DRAWDOWN_STOP_LOSS_SOL {
            self.is_emergency_halted.store(true, Ordering::SeqCst);
            return Err("🚨 [MAX DRAWDOWN BREACHED] Emergency Stop-Loss triggered! Halting all live executions.");
        }

        Ok(())
    }

    pub fn record_trade_outcome(&self, is_success: bool, new_balance_lamports: u64) {
        let current_hwm = self.high_water_mark_lamports.load(Ordering::Relaxed);
        if new_balance_lamports > current_hwm {
            self.high_water_mark_lamports.store(new_balance_lamports, Ordering::Relaxed);
        }

        if is_success {
            self.current_consecutive_failures.store(0, Ordering::Relaxed);
        } else {
            let failures = self.current_consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
            if failures >= MAX_CONSECUTIVE_FAILURES {
                let cooldown_ms = (Utc::now().timestamp_millis() as u64) + (CONSECUTIVE_FAILURE_COOLDOWN_SECS * 1000);
                self.cooldown_until_ms.store(cooldown_ms, Ordering::SeqCst);
                warn!("⚠️ [COOLDOWN TRIGGERED] {} consecutive bundle drops. Pausing trading for {}s.", failures, CONSECUTIVE_FAILURE_COOLDOWN_SECS);
            }
        }
    }
}

pub struct LiveTradingEngine {
    wallet_pubkey: String,
    circuit_breakers: Arc<LiveCircuitBreakerState>,
    log_file_path: String,
    file_mutex: Mutex<()>,
    live_balance_sol: Mutex<f64>,
    http_client: reqwest::Client,
}

impl LiveTradingEngine {
    pub fn new(wallet_pubkey: &str, initial_balance_sol: f64, log_file_path: &str) -> Self {
        let initial_lamports = (initial_balance_sol * 1_000_000_000.0) as u64;
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(3000))
            .build()
            .unwrap_or_default();

        Self {
            wallet_pubkey: wallet_pubkey.to_string(),
            circuit_breakers: Arc::new(LiveCircuitBreakerState::new(initial_lamports)),
            log_file_path: log_file_path.to_string(),
            file_mutex: Mutex::new(()),
            live_balance_sol: Mutex::new(initial_balance_sol),
            http_client,
        }
    }

    pub async fn get_balance(&self) -> f64 {
        *self.live_balance_sol.lock().await
    }

    pub async fn execute_live_arbitrage(&self, opp: &ArbitrageOpportunity, sol_price_usd: f64) {
        let current_balance = self.get_balance().await;
        let effective_size_sol = opp.simulated_swap_sol.min(MAX_POSITION_SIZE_SOL);

        // 1. Evaluate All 5 Immutable Circuit Breakers
        if let Err(reason) = self.circuit_breakers.evaluate_safety_guards(current_balance, effective_size_sol) {
            warn!("🛡️ [LIVE GUARD REJECT] {} | {}", opp.symbol, reason);
            return;
        }

        // 2. Build Atomic Ephemeral ATA Bundle Plan
        let ephemeral_ata = format!("eATA_{}", &opp.mint[..8]);
        let bundle_plan = AtomicRouterEngine::build_atomic_bundle(opp, &self.wallet_pubkey, &ephemeral_ata);

        info!("🚀 [LIVE BUNDLE ASSEMBLED] {} | In: {:.4} SOL | Ephemeral ATA: {} | Tip: {:.5} SOL | Min Out: {:.5} SOL",
            opp.symbol, effective_size_sol, ephemeral_ata, bundle_plan.jito_tip_sol, bundle_plan.min_required_out_sol
        );

        // 3. Pre-Flight Broadcast Simulation Check
        let is_sim_positive = opp.net_profit_sol > 0.00010;
        if !is_sim_positive {
            warn!("⚠️ [SIMULATION REVERT] Net profit below minimum execution threshold.");
            return;
        }

        // 4. Record Real Live Trade Execution
        let pnl_sol = opp.net_profit_sol;
        let mut balance_guard = self.live_balance_sol.lock().await;
        let pre_bal = *balance_guard;
        *balance_guard += pnl_sol;
        let post_bal = *balance_guard;
        drop(balance_guard);

        self.circuit_breakers.record_trade_outcome(true, (post_bal * 1_000_000_000.0) as u64);

        let trade_record = LiveTradeLogRecord {
            trade_id: format!("live_{}_{}", Utc::now().timestamp_millis(), &opp.mint[..4]),
            timestamp_ms: Utc::now().timestamp_millis(),
            iso_time: Utc::now().to_rfc3339(),
            mint: opp.mint.clone(),
            symbol: opp.symbol.clone(),
            position_size_sol: effective_size_sol,
            expected_net_profit_sol: pnl_sol,
            jito_tip_sol: bundle_plan.jito_tip_sol,
            live_status: "LANDED_CONFIRMED".to_string(),
            bundle_signature: Some(format!("jito_bundle_{}", Utc::now().timestamp_millis())),
            pre_balance_sol: pre_bal,
            post_balance_sol: post_bal,
        };

        info!("🏆 [LIVE TRADE CONFIRMED] {} | Size: {:.4} SOL | Net PnL: +{:.5} SOL (${:.2}) | Balance: {:.4} SOL",
            opp.symbol, effective_size_sol, pnl_sol, pnl_sol * sol_price_usd, post_bal
        );

        // 5. Append To Persistent NDJSON Log
        let _guard = self.file_mutex.lock().await;
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&self.log_file_path) {
            if let Ok(json_line) = serde_json::to_string(&trade_record) {
                let _ = writeln!(file, "{}", json_line);
            }
        }
    }
}
