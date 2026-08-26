use std::sync::Arc;
use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::signal;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

// Include module files directly for standalone binary compilation
#[path = "../types.rs"]
mod types;
#[path = "../raydium_cpmm.rs"]
mod raydium_cpmm;
#[path = "../websocket_feed.rs"]
mod websocket_feed;
#[path = "../migration_sniper.rs"]
mod migration_sniper;

use migration_sniper::RaydiumMigrationSniper;
use websocket_feed::RaydiumMigrationEvent;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("=======================================================================");
    println!("  🎯 RAYDIUM MIGRATION SNIPER (100% Zero-Real-Capital Shadow Engine) 🎯");
    println!("  🔹 Mode: 100% Shadow / Paper-Trading Simulation ($0.00 Real Funds)");
    println!("  🔹 Verified by Council: GLM-5 + Kimi k2.5 + DeepSeek V3.2 + Qwen 3 Coder 480B");
    println!("  🔹 Strategy: P0 Genesis Capture + Panic Velocity Decay + Demand Confirmation");
    println!("  🔹 Output Log: data/migration_shadow_trades.ndjson");
    println!("=======================================================================");

    let sniper = Arc::new(RaydiumMigrationSniper::new(
        "https://api.mainnet-beta.solana.com",
        "data/migration_shadow_trades.ndjson",
    ));

    let sniper_clone = Arc::clone(&sniper);
    let ws_handle = tokio::spawn(async move {
        run_standalone_pumpportal_stream(sniper_clone).await;
    });

    tokio::select! {
        _ = signal::ctrl_c() => {
            println!("\n🛑 Graceful shutdown initiated. Migration Sniper stopped.");
        }
        _ = ws_handle => {
            println!("Migration Sniper stream exited.");
        }
    }
}

async fn run_standalone_pumpportal_stream(sniper: Arc<RaydiumMigrationSniper>) {
    let ws_url = "wss://pumpportal.fun/api/data";
    info!("📡 [MIGRATION SNIPER WS] Connecting to PumpPortal Real-Time Stream: {}", ws_url);

    loop {
        match connect_async(ws_url).await {
            Ok((ws_stream, _)) => {
                info!("✅ [MIGRATION SNIPER WS] Connected to PumpPortal! Subscribing to raydium migrations...");
                let (mut write, mut read) = ws_stream.split();

                let sub_payload_1 = serde_json::json!({
                    "method": "subscribeRaydiumLiquidity"
                });
                let sub_payload_2 = serde_json::json!({
                    "method": "subscribeRaydiumMigration"
                });

                let _ = write.send(Message::Text(sub_payload_1.to_string())).await;
                let _ = write.send(Message::Text(sub_payload_2.to_string())).await;

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&text) {
                                if let Some(mint) = json_val.get("mint").and_then(|m| m.as_str()) {
                                    let pool_address = json_val.get("poolAddress")
                                        .or_else(|| json_val.get("raydiumPool"))
                                        .or_else(|| json_val.get("pairAddress"))
                                        .and_then(|p| p.as_str())
                                        .unwrap_or(mint)
                                        .to_string();

                                    let sol_liq = json_val.get("solAmount")
                                        .or_else(|| json_val.get("initialSolLiquidity"))
                                        .and_then(|s| s.as_f64())
                                        .unwrap_or(69.0);

                                    let sig = json_val.get("signature")
                                        .or_else(|| json_val.get("txHash"))
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("sim_sig")
                                        .to_string();

                                    let event = RaydiumMigrationEvent {
                                        mint: mint.to_string(),
                                        pool_address,
                                        initial_sol_liquidity: sol_liq,
                                        signature: sig,
                                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                                    };

                                    let sniper_task = Arc::clone(&sniper);
                                    tokio::spawn(async move {
                                        sniper_task.on_migration_detected(event).await;
                                    });
                                }
                            }
                        }
                        Ok(Message::Ping(_)) => {
                            let _ = write.send(Message::Pong(vec![])).await;
                        }
                        Ok(Message::Close(_)) => {
                            warn!("⚠️ [MIGRATION SNIPER WS] Connection closed by remote host. Reconnecting...");
                            break;
                        }
                        Err(e) => {
                            warn!("⚠️ [MIGRATION SNIPER WS] Read error: {}. Reconnecting...", e);
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                warn!("⚠️ [MIGRATION SNIPER WS] Connection error: {}. Retrying in 3s...", e);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }
}
