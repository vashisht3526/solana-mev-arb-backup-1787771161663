use std::sync::Arc;
use tokio::signal;
use log::info;

mod amm;
mod types;
mod websocket_feed;
mod math;
mod paper_engine;
mod scanner;
mod jito;
mod jito_builder;
mod router;
mod live_engine;
mod dual_leg_engine;
mod triangular_live_engine;
mod raydium_cpmm;
mod migration_sniper;
mod triangular_math;
mod helius_stream;
mod pool_decoder;
mod mriy_watch;
pub mod swaps_adapter;

use dual_leg_engine::DualLegLiveEngine;
use live_engine::LiveTradingEngine;
use migration_sniper::RaydiumMigrationSniper;
use paper_engine::PaperTradingEngine;
use scanner::ArbitrageScanner;
use triangular_live_engine::TriangularLiveEngine;

fn resolve_keypair_path() -> &'static str {
    if std::path::Path::new("wallet/keypair.json").exists() {
        "wallet/keypair.json"
    } else if std::path::Path::new("/home/ubuntu/solana-mev-arb/wallet/keypair.json").exists() {
        "/home/ubuntu/solana-mev-arb/wallet/keypair.json"
    } else if std::path::Path::new("/home/ubuntu/.config/solana/mev_live.json").exists() {
        "/home/ubuntu/.config/solana/mev_live.json"
    } else {
        "data/mev_live.json"
    }
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();
    let env_mode = std::env::var("MEV_MODE").unwrap_or_default();

    let is_dual_live = env_mode == "dual_live" || args.iter().any(|arg| arg == "--dual-live" || arg == "-dl" || arg == "--live" || arg == "-l" || arg == "--dual");
    let is_triangular_live = env_mode == "triangular_live" || args.iter().any(|arg| arg == "--triangular-live" || arg == "-tl" || arg == "--triangular" || arg == "-t");
    let is_micro = env_mode == "micro" || args.iter().any(|arg| arg == "--micro" || arg == "--35usd" || arg == "-m");
    let is_1sol = env_mode == "1sol" || args.iter().any(|arg| arg == "--1sol" || arg == "-1");

    let keypair_path = resolve_keypair_path();

    let (initial_virtual_sol, paper_log_file, max_position_sol, min_net_profit_sol, min_spread_pct, banner_label, dual_engine, tri_engine) = if is_dual_live && !is_triangular_live {
        let dual = Arc::new(DualLegLiveEngine::new(
            keypair_path,
            0.3981,
            "data/dual_leg_live_trades.ndjson",
            "data/dual_leg_config.json"
        ));
        let conf = dual.load_config();
        (
            0.3981,
            "data/paper_trades_dual_shadow.ndjson",
            conf.max_position_sol, // 0.099 SOL Concentrated Position
            conf.min_net_profit_sol,
            conf.min_spread_pct,
            "🛡️ DUAL-LEG CONCENTRATED ENGINE (Armed with Real Keypair: AA1ihbeh... | Meteora DLMM & Orca)",
            Some(dual),
            None
        )
    } else if is_triangular_live {
        let tri = Arc::new(TriangularLiveEngine::new(
            keypair_path,
            0.3981,
            "data/triangular_live_trades.ndjson",
            "data/triangular_config.json"
        ));
        let conf = tri.load_config();
        (
            0.3981,
            "data/paper_trades_triangular.ndjson",
            conf.max_position_sol, // 0.099 SOL Concentrated Position
            conf.min_net_profit_sol,
            conf.min_spread_pct,
            "🔺 TRIANGULAR CONCENTRATED ENGINE (Armed with Real Keypair: AA1ihbeh... | $10k-$60k Goldilocks Zone)",
            None,
            Some(tri)
        )
    } else if is_micro {
        (
            0.38,
            "data/paper_trades_micro.ndjson",
            0.12,
            0.00050,
            0.80,
            "⚡ $35 USD (0.38 SOL) MICRO-PILOT SHADOW ENGINE (Pos: 0.12 SOL | Spread: +0.80%)",
            None,
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
            None,
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
            None,
            None
        )
    };

    println!("=======================================================================");
    println!("  ⚡ SOLANA MEV ENGINE (Decoupled Dual & Triangular Execution) ⚡");
    println!("  🔹 Mode: {}", banner_label);
    println!("  🔹 Verified by Council: GLM-5 + Kimi k2.5 + DeepSeek V3.2 + Qwen 3 Coder 480B");
    println!("  🔹 Stream: Direct Zero-Cost Stream (PumpPortal WS + Raydium Delta Auditor)");
    println!("  🔹 Invariant: Atomic Dual-Leg Ephemeral Bundle + 100% Shadow Migration Sniper");
    println!("=======================================================================");

    info!("💰 Starting Balance: {:.4} SOL", initial_virtual_sol);
    info!("📁 Log Output: {}", paper_log_file);
    info!("🛡️ Max Position Size per Arbitrage: {:.2} SOL", max_position_sol);
    info!("🎯 Minimum Net Profit Floor: {:.5} SOL", min_net_profit_sol);
    info!("📊 Minimum Gross Spread Trigger: +{:.2}%", min_spread_pct);

    let paper_engine = Arc::new(PaperTradingEngine::new(
        initial_virtual_sol,
        paper_log_file,
    ));

    let mut scanner = ArbitrageScanner::new(
        Arc::clone(&paper_engine),
        None,
        max_position_sol,
        min_net_profit_sol,
        min_spread_pct,
    );

    if let Some(dual) = dual_engine {
        scanner.set_dual_leg_engine(dual);
    }
    if let Some(tri) = tri_engine {
        scanner.set_triangular_engine(tri);
    }

    let migration_sniper = Arc::new(RaydiumMigrationSniper::new(
        "https://mainnet.helius-rpc.com/?api-key=9b358df5-981f-4a3f-8f25-5ff9af04653f",
        "data/migration_shadow_trades.ndjson",
    ));
    scanner.set_migration_sniper(Arc::clone(&migration_sniper));

    let scanner_arc = Arc::new(scanner);

    let instance_name = if is_dual_live {
        // V42c: competitor correlation runs in exactly ONE process to bound
        // getTransaction credit spend and keep data/mriy_correlation.jsonl single-writer.
        crate::mriy_watch::ensure_mriy_watcher();
        "dual_live"
    } else if is_triangular_live {
        "triangular_live"
    } else if is_micro {
        "micro"
    } else if is_1sol {
        "1sol"
    } else {
        "10sol"
    };

    let runner_scanner = Arc::clone(&scanner_arc);
    let scanner_handle = tokio::spawn(async move {
        runner_scanner.run(instance_name).await;
    });

    info!("🚀 Initializing Decoupled MEV Event Loop for [{}]...", instance_name);

    match signal::ctrl_c().await {
        Ok(()) => {
            println!("\n🛑 Graceful shutdown initiated. Flushed all trades.");
        }
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
        }
    }

    scanner_handle.abort();
}
