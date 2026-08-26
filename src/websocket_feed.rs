use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const PUMPPORTAL_WS_URL: &str = "wss://pumpportal.fun/api/data";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveSwapEvent {
    pub mint: String,
    pub tx_type: String,       // "buy" or "sell"
    pub sol_amount: f64,
    pub token_amount: f64,
    pub v_sol_in_bonding_curve: f64,
    pub v_tokens_in_bonding_curve: f64,
    pub market_cap_sol: f64,
    pub signature: String,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaydiumMigrationEvent {
    pub mint: String,
    pub pool_address: String,
    pub initial_sol_liquidity: f64,
    pub signature: String,
    pub timestamp_ms: i64,
}

pub struct PumpPortalFeed;

impl PumpPortalFeed {
    pub async fn run_streaming_pipeline(
        swap_sender: Sender<LiveSwapEvent>,
        migration_sender: Sender<RaydiumMigrationEvent>,
    ) {
        loop {
            info!("🔌 Connecting to PumpPortal Live On-Chain WebSocket Feed: {}", PUMPPORTAL_WS_URL);

            match connect_async(PUMPPORTAL_WS_URL).await {
                Ok((mut ws_stream, _)) => {
                    info!("⚡ Connected to PumpPortal Real-Time Stream! Subscribing to trade & liquidity events...");

                    // 1. Subscribe to Raydium migrations
                    let raydium_sub = serde_json::json!({
                        "method": "subscribeRaydiumLiquidity"
                    });
                    if let Err(e) = ws_stream.send(Message::Text(raydium_sub.to_string())).await {
                        error!("Failed to subscribe to Raydium migrations: {}", e);
                    }

                    // 2. Subscribe to all new token creations
                    let new_token_sub = serde_json::json!({
                        "method": "subscribeNewToken"
                    });
                    if let Err(e) = ws_stream.send(Message::Text(new_token_sub.to_string())).await {
                        error!("Failed to subscribe to new tokens: {}", e);
                    }

                    // 3. Subscribe to all token trades (Real-time live swap stream)
                    let token_trades_sub = serde_json::json!({
                        "method": "subscribeTokenTrade",
                        "keys": []
                    });
                    if let Err(e) = ws_stream.send(Message::Text(token_trades_sub.to_string())).await {
                        error!("Failed to subscribe to live token trades: {}", e);
                    }

                    while let Some(msg) = ws_stream.next().await {
                        match msg {
                            Ok(Message::Text(text)) => {
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                                    let now_ms = chrono::Utc::now().timestamp_millis();

                                    // Handle Raydium Migration
                                    if let Some(pool_addr) = val.get("poolAddress").and_then(|p| p.as_str()) {
                                        let mint = val.get("mint").and_then(|m| m.as_str()).unwrap_or("").to_string();
                                        let initial_sol = val.get("solAmount").and_then(|s| s.as_f64()).unwrap_or(0.0);
                                        let sig = val.get("signature").and_then(|s| s.as_str()).unwrap_or("").to_string();

                                        if !mint.is_empty() {
                                            let migration = RaydiumMigrationEvent {
                                                mint,
                                                pool_address: pool_addr.to_string(),
                                                initial_sol_liquidity: initial_sol,
                                                signature: sig,
                                                timestamp_ms: now_ms,
                                            };
                                            let _ = migration_sender.try_send(migration);
                                        }
                                    }
                                    // Handle Live Swap
                                    else if let Some(tx_type) = val.get("txType").and_then(|t| t.as_str()) {
                                        let mint = val.get("mint").and_then(|m| m.as_str()).unwrap_or("").to_string();
                                        let sol_amount = val.get("solAmount").and_then(|s| s.as_f64()).unwrap_or(0.0);
                                        let token_amount = val.get("tokenAmount").and_then(|t| t.as_f64()).unwrap_or(0.0);
                                        let v_sol = val.get("vSolInBondingCurve").and_then(|s| s.as_f64()).unwrap_or(0.0);
                                        let v_tokens = val.get("vTokensInBondingCurve").and_then(|t| t.as_f64()).unwrap_or(0.0);
                                        let mcap_sol = val.get("marketCapSol").and_then(|m| m.as_f64()).unwrap_or(0.0);
                                        let sig = val.get("signature").and_then(|s| s.as_str()).unwrap_or("").to_string();

                                        if !mint.is_empty() && v_tokens > 0.0 {
                                            let swap = LiveSwapEvent {
                                                mint,
                                                tx_type: tx_type.to_string(),
                                                sol_amount,
                                                token_amount,
                                                v_sol_in_bonding_curve: v_sol,
                                                v_tokens_in_bonding_curve: v_tokens,
                                                market_cap_sol: mcap_sol,
                                                signature: sig,
                                                timestamp_ms: now_ms,
                                            };
                                            let _ = swap_sender.try_send(swap);
                                        }
                                    }
                                }
                            }
                            Ok(Message::Ping(data)) => {
                                let _ = ws_stream.send(Message::Pong(data)).await;
                            }
                            Err(e) => {
                                warn!("WebSocket stream error: {}. Reconnecting...", e);
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to connect to PumpPortal WS: {}. Retrying in 3s...", e);
                    sleep(Duration::from_secs(3)).await;
                }
            }
        }
    }
}
