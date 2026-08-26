#![allow(dead_code)]
use crate::types::{ArbitrageOpportunity, PaperTradeRecord};
use chrono::Utc;
use log::{info, warn};
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Realistic failure rates based on real Solana MEV data:
///
/// - ~30% base failure rate (stale quotes, someone else fills first, tx dropped)
/// - Additional ~15% for tight spreads (<3%) where competition is fiercer  
/// - Additional ~10% for low-liquidity pools (<$10k) where fills are unreliable
///
/// Real Jito MEV bots on Solana see 40-65% win rates.
const BASE_FAILURE_RATE: f64 = 0.30;
const TIGHT_SPREAD_EXTRA_FAIL: f64 = 0.15; // Extra failure for spreads < 3%
const LOW_LIQ_EXTRA_FAIL: f64 = 0.10; // Extra failure for pools < $10k
const REVERT_GAS_COST_SOL: f64 = 0.000005; // Failed tx still costs base gas

pub struct PaperTradingEngine {
    virtual_wallet_balance_sol: Mutex<f64>,
    total_trades: AtomicUsize,
    winning_trades: AtomicUsize,
    failed_trades: AtomicUsize,
    cumulative_pnl_sol: Mutex<f64>,
    log_file_path: String,
}

impl PaperTradingEngine {
    pub fn new(initial_balance_sol: f64, log_file_path: &str) -> Self {
        if let Some(parent) = Path::new(log_file_path).parent() {
            let _ = create_dir_all(parent);
        }

        // Resume continuous balance progression from the last logged trade
        let mut starting_bal = initial_balance_sol;
        if let Ok(content) = std::fs::read_to_string(log_file_path) {
            for line in content.lines().rev() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                        if let Some(end_bal) = val.get("ending_wallet_balance_sol").and_then(|b| b.as_f64()) {
                            if end_bal > 0.0 {
                                starting_bal = end_bal;
                                break;
                            }
                        }
                    }
                }
            }
        }

        Self {
            virtual_wallet_balance_sol: Mutex::new(starting_bal),
            total_trades: AtomicUsize::new(0),
            winning_trades: AtomicUsize::new(0),
            failed_trades: AtomicUsize::new(0),
            cumulative_pnl_sol: Mutex::new(0.0),
            log_file_path: log_file_path.to_string(),
        }
    }

    /// Simulate realistic execution with failure modeling
    pub fn execute_shadow_trade(&self, opp: &ArbitrageOpportunity, sol_price_usd: f64) -> PaperTradeRecord {
        let mut balance = self.virtual_wallet_balance_sol.lock().unwrap();
        let starting_balance = *balance;

        let trade_id = format!("SHADOW-{:08x}", rand_u32());
        let now = Utc::now();

        // ---- REALISTIC FAILURE SIMULATION ----
        // 
        // In real Solana MEV, trades fail because:
        // 1. Someone else fills the arb before you (most common)
        // 2. Price moved between quote and execution (stale quote)
        // 3. Transaction dropped by validator
        // 4. Sandwich bot front-runs and inverts the spread
        // 5. Pool reserves changed mid-block
        //
        let mut failure_probability = BASE_FAILURE_RATE;

        // Tighter spreads = more competition = more failures
        if opp.gross_spread_pct < 3.0 {
            failure_probability += TIGHT_SPREAD_EXTRA_FAIL;
        }

        // Low liquidity pools = more unreliable fills
        if opp.pool_liquidity_usd < 10_000.0 {
            failure_probability += LOW_LIQ_EXTRA_FAIL;
        }

        // Deterministic-ish randomness from nanoseconds
        let roll = deterministic_random_f64();
        let trade_failed = roll < failure_probability;

        let (status, net_pnl_sol, net_pnl_usd, gas_fee_sol, jito_tip_sol) = if trade_failed {
            // FAILED TRADE - still costs base gas
            let gas = REVERT_GAS_COST_SOL;
            *balance -= gas;
            self.failed_trades.fetch_add(1, Ordering::SeqCst);

            let mut cum_pnl = self.cumulative_pnl_sol.lock().unwrap();
            *cum_pnl -= gas;

            let fail_reason = if roll < BASE_FAILURE_RATE * 0.5 {
                "FAILED_FRONT_RUN" // Someone else got there first
            } else if roll < BASE_FAILURE_RATE * 0.8 {
                "FAILED_STALE_QUOTE" // Price moved
            } else if roll < BASE_FAILURE_RATE {
                "FAILED_TX_DROPPED" // Validator dropped tx
            } else if roll < BASE_FAILURE_RATE + TIGHT_SPREAD_EXTRA_FAIL {
                "FAILED_COMPETITION" // Too many bots on this spread
            } else {
                "FAILED_THIN_FILL" // Pool couldn't fill at expected price
            };

            warn!(
                "[{}] {} | Spread: {:.2}% | Liq: ${:.0} | Lost gas: -{:.6} SOL",
                fail_reason, opp.symbol, opp.gross_spread_pct, opp.pool_liquidity_usd, gas
            );

            (fail_reason.to_string(), -gas, -gas * sol_price_usd, gas, 0.0)
        } else {
            // SUCCESSFUL FILL
            let gas = crate::amm::BASE_GAS_FEE_SOL + crate::amm::PRIORITY_FEE_SOL;
            let tip = opp.jito_tip_usd / sol_price_usd;
            let profit = opp.net_profit_sol;

            if profit > 0.0 {
                *balance += profit;
                self.winning_trades.fetch_add(1, Ordering::SeqCst);
                let mut cum_pnl = self.cumulative_pnl_sol.lock().unwrap();
                *cum_pnl += profit;

                info!(
                    "[FILLED] {} | Size: {} SOL | Spread: +{:.2}% | Slip: ~{:.2}% | Net PnL: {:+.4} SOL (${:+.2}) | Balance: {:.4} SOL",
                    opp.symbol,
                    opp.simulated_swap_sol,
                    opp.gross_spread_pct,
                    opp.gross_spread_pct - opp.net_spread_pct - 0.50, // Approximate slippage shown
                    profit,
                    profit * sol_price_usd,
                    *balance
                );

                ("FILLED".to_string(), profit, opp.net_profit_usd, gas, tip)
            } else {
                // Edge case: spread closed during execution
                *balance -= gas;
                self.failed_trades.fetch_add(1, Ordering::SeqCst);
                let mut cum_pnl = self.cumulative_pnl_sol.lock().unwrap();
                *cum_pnl -= gas;

                ("FAILED_SPREAD_CLOSED".to_string(), -gas, -gas * sol_price_usd, gas, 0.0)
            }
        };

        self.total_trades.fetch_add(1, Ordering::SeqCst);
        let ending_balance = *balance;

        let record = PaperTradeRecord {
            trade_id,
            timestamp_ms: now.timestamp_millis(),
            iso_time: now.to_rfc3339(),
            mint: opp.mint.clone(),
            symbol: opp.symbol.clone(),
            position_size_sol: opp.simulated_swap_sol,
            entry_pump_price_usd: opp.pump_price_usd,
            exit_raydium_price_usd: opp.raydium_price_usd,
            gross_spread_pct: opp.gross_spread_pct,
            net_spread_pct: opp.net_spread_pct,
            gas_fee_sol,
            jito_tip_sol,
            net_pnl_sol: (net_pnl_sol * 10000.0).round() / 10000.0,
            net_pnl_usd: (net_pnl_usd * 100.0).round() / 100.0,
            starting_wallet_balance_sol: (starting_balance * 10000.0).round() / 10000.0,
            ending_wallet_balance_sol: (ending_balance * 10000.0).round() / 10000.0,
            status,
        };

        // Write record to NDJSON log file
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&self.log_file_path) {
            if let Ok(json_str) = serde_json::to_string(&record) {
                let _ = writeln!(file, "{}", json_str);
            }
        }

        record
    }

    pub fn get_summary(&self) -> (usize, usize, usize, f64, f64) {
        let total = self.total_trades.load(Ordering::SeqCst);
        let wins = self.winning_trades.load(Ordering::SeqCst);
        let fails = self.failed_trades.load(Ordering::SeqCst);
        let pnl = *self.cumulative_pnl_sol.lock().unwrap();
        let balance = *self.virtual_wallet_balance_sol.lock().unwrap();
        (total, wins, fails, pnl, balance)
    }
}

/// Simple deterministic-ish random f64 in [0.0, 1.0) using nanosecond clock
fn deterministic_random_f64() -> f64 {
    let nanos = Utc::now().timestamp_subsec_nanos();
    // Mix bits for better distribution
    let mixed = nanos.wrapping_mul(2654435761); // Knuth multiplicative hash
    (mixed as f64) / (u32::MAX as f64)
}

fn rand_u32() -> u32 {
    let nanos = Utc::now().timestamp_subsec_nanos();
    nanos ^ 0x5bf03635
}
