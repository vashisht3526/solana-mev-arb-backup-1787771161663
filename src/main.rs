mod amm;
mod jito;
mod jito_builder;
mod live_engine;
mod math;
mod paper_engine;
mod router;
mod scanner;
mod types;
mod websocket_feed;

use live_engine::LiveTradingEngine;
use log::info;
use paper_engine::PaperTradingEngine;
use scanner::ArbitrageScanner;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();
    let is_live = args.iter().any(|arg| arg == "--live" || arg == "-l");
    let is_micro = args.iter().any(|arg| arg == "--micro" || arg == "--35usd" || arg == "-m");
    let is_1sol = args.iter().any(|arg| arg == "--1sol" || arg == "-1");

    let (initial_virtual_sol, paper_log_file, max_position_sol, min_net_profit_sol, min_spread_pct, banner_label, engine_tag, live_engine) = if is_live {
        let wallet_pubkey = std::env::var("SOLANA_WALLET_PUBKEY").unwrap_or_else(|_| "LIVE_TRADING_WALLET_ACTIVE".to_string());
        let live = Arc::new(LiveTradingEngine::new(&wallet_pubkey, 0.38, "data/live_trades.ndjson"));
        (
            0.38,
            "data/live_trades.ndjson",
            0.08, // Strict 0.08 SOL cap for live pilot
            0.00030,
            0.50,
            "🛡️ LIVE MAINNET TRADING ENGINE (5-Pillar Circuit Breakers | Ephemeral ATAs | Zero Risk Leakage)",
            "LIVE-MAINNET-ENGINE",
            Some(live)
        )
    } else if is_micro {
        (
            0.38,
            "data/paper_trades_micro.ndjson",
            0.12,
            0.00050,
            0.80,
            "⚡ $35 USD (0.38 SOL) MICRO-PILOT SHADOW ENGINE (Pos: 0.12 SOL | Spread: +0.80%)",
            "MICRO-35USD-ENGINE",
            None
        )
    } else if is_1sol {
        (
            1.00,
            "data/paper_trades_1sol.ndjson",
            0.12,
            0.00008,
            0.40,
            "⚡ 1.0 SOL CONSERVATIVE RUNWAY SHADOW ENGINE (Max Pos: 0.12 SOL | Reserve: 0.15 SOL)",
            "1-SOL-ENGINE",
            None
        )
    } else {
        (
            10.00,
            "data/paper_trades_10sol.ndjson",
            0.50,
            0.00030,
            0.20,
            "⚡ 10.0 SOL AGGRESSIVE GROWTH SHADOW ENGINE (Max Pos: 0.50 SOL | Reserve: 1.50 SOL)",
            "10-SOL-ENGINE",
            None
        )
    };

    println!("=======================================================================");
    println!(" ⚡ SOLANA MEV ENGINE (Live Execution & Shadow Simulation System) ⚡");
    println!(" 🏷️ Mode: {}", banner_label);
    println!(" 🤖 Verified by Council: GLM-5 + Kimi k2.5 + Qwen 3 Coder 480B");
    println!(" 🚀 Stream: Direct Zero-Cost Stream (PumpPortal WS + Active Volatility Auditor)");
    println!(" 🛡️ Invariant: Atomic Ephemeral ATA Bundle (require! final >= in + profit)");
    println!("=======================================================================");

    info!("💰 Starting Balance: {:.4} SOL", initial_virtual_sol);
    info!("📄 Log Output: {}", paper_log_file);
    info!("⚡ Max Position Size per Arbitrage: {:.2} SOL", max_position_sol);
    info!("🎯 Minimum Net Profit Floor: {:.5} SOL", min_net_profit_sol);
    info!("🎯 Minimum Gross Spread Trigger: +{:.2}%", min_spread_pct);

    let paper_engine = Arc::new(PaperTradingEngine::new(initial_virtual_sol, paper_log_file));
    let scanner = Arc::new(ArbitrageScanner::new(
        Arc::clone(&paper_engine),
        live_engine,
        max_position_sol,
        min_net_profit_sol,
        min_spread_pct,
    ));

    let tokens_counter = Arc::clone(&scanner.tokens_tracked);
    let pools_counter = Arc::clone(&scanner.pools_scanned);
    let arbs_counter = Arc::clone(&scanner.arbs_checked);
    let opps_counter = Arc::clone(&scanner.opps_found);
    let reporter_engine = Arc::clone(&paper_engine);

    // Spawn periodic statistics reporter (every 10s)
    let tag = engine_tag.to_string();
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(10)).await;
            let (total, wins, fails, pnl, balance) = reporter_engine.get_summary();
            let win_rate = if total > 0 {
                (wins as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            let tracked = tokens_counter.load(Ordering::Relaxed);
            let scanned = pools_counter.load(Ordering::Relaxed);
            let evaluated = arbs_counter.load(Ordering::Relaxed);
            let opportunities = opps_counter.load(Ordering::Relaxed);

            println!(
                "📊 [{}] Tokens: {} | Pools: {} | Arbs: {} | Opps: {} | Trades: {} | Wins: {} | Fails: {} | Win Rate: {:.1}% | PnL: {:+.4} SOL | Balance: {:.4} SOL",
                tag,
                tracked,
                scanned,
                evaluated,
                opportunities,
                total,
                wins,
                fails,
                win_rate,
                pnl,
                balance
            );
        }
    });

    info!("🚀 Initializing Engine Event Loop...");
    scanner.run().await;
}
