//! Pure Rust script to safely unwind token positions back to SOL
#![allow(dead_code)]
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use ed25519_dalek::Signer;
use std::fs;
use std::path::Path;

const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
const SOLANA_RPC: &str = "https://api.mainnet-beta.solana.com";

struct UnwindBot {
    pubkey_str: String,
    signing_key: ed25519_dalek::SigningKey,
    client: reqwest::Client,
}

impl UnwindBot {
    fn new(keypair_path: &str) -> Self {
        let content = fs::read_to_string(Path::new(keypair_path)).expect("Keypair read failed");
        let trimmed = content.trim();

        let secret_key_bytes: Vec<u8> = if trimmed.starts_with('[') {
            serde_json::from_str(trimmed).unwrap()
        } else {
            bs58::decode(trimmed).into_vec().unwrap()
        };

        let mut seed = [0u8; 32];
        seed.copy_from_slice(&secret_key_bytes[0..32]);
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
        let pubkey_str = bs58::encode(&secret_key_bytes[32..64]).into_string();

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap();

        Self { pubkey_str, signing_key, client }
    }

    async fn sell_token_to_sol(&self, mint: &str, amount: u64) -> anyhow::Result<()> {
        println!("\n🔄 [RUST UNWIND] Selling Mint: {} | Raw Amount: {}", mint, amount);

        let quote_url = format!(
            "https://api.jup.ag/swap/v1/quote?inputMint={}&outputMint={}&amount={}&slippageBps=300",
            mint, SOL_MINT, amount
        );

        let quote_res = self.client.get(&quote_url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let out_lamports = quote_res.get("outAmount")
            .and_then(|v| v.as_str())
            .unwrap_or("0");

        println!("✅ [QUOTE RECEIVED] Expected SOL out: {} lamports ({:.5} SOL)", 
            out_lamports, out_lamports.parse::<f64>().unwrap_or(0.0) / 1e9
        );

        let swap_payload = serde_json::json!({
            "quoteResponse": quote_res,
            "userPublicKey": self.pubkey_str,
            "wrapAndUnwrapSol": true,
            "dynamicComputeUnitLimit": true,
            "prioritizationFeeLamports": 100000
        });

        let swap_res = self.client.post("https://api.jup.ag/swap/v1/swap")
            .header("User-Agent", "Mozilla/5.0")
            .json(&swap_payload)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let unsigned_tx = swap_res.get("swapTransaction")
            .and_then(|v| v.as_str())
            .expect("No swapTransaction in Jupiter response");

        let mut tx_bytes = BASE64_STANDARD.decode(unsigned_tx)?;
        let msg_bytes = &tx_bytes[65..];
        let sig = self.signing_key.sign(msg_bytes);
        tx_bytes[1..65].copy_from_slice(&sig.to_bytes());

        let signed_b64 = BASE64_STANDARD.encode(&tx_bytes);

        // Broadcast to Solana RPC
        let rpc_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                signed_b64,
                { "encoding": "base64", "skipPreflight": false, "preflightCommitment": "confirmed" }
            ]
        });

        let rpc_res = self.client.post(SOLANA_RPC)
            .json(&rpc_payload)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        if let Some(sig_str) = rpc_res.get("result").and_then(|r| r.as_str()) {
            println!("🚀 [UNWIND TX BROADCAST SUCCESS] https://solscan.io/tx/{}", sig_str);
        } else {
            println!("❌ [RPC ERROR] {:?}", rpc_res);
        }

        Ok(())
    }

    async fn get_balance(&self) -> f64 {
        let rpc_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getBalance",
            "params": [&self.pubkey_str]
        });

        if let Ok(resp) = self.client.post(SOLANA_RPC).json(&rpc_payload).send().await {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(lamports) = body.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_f64()) {
                    return lamports / 1e9;
                }
            }
        }
        0.0
    }
}

#[tokio::main]
async fn main() {
    let keypair_path = if Path::new("/home/ubuntu/.config/solana/mev_live.json").exists() {
        "/home/ubuntu/.config/solana/mev_live.json"
    } else if Path::new("data/mev_live.json").exists() {
        "data/mev_live.json"
    } else {
        println!("⚠️ No keypair found in standard paths.");
        return;
    };

    let bot = UnwindBot::new(keypair_path);
    println!("💰 Initial SOL Balance: {:.6} SOL", bot.get_balance().await);

    // Unwind Ga5ErptAQ5oHwB7G195UEq9FeJ5UNDoV9SCRCpebFeG7
    let _ = bot.sell_token_to_sol("Ga5ErptAQ5oHwB7G195UEq9FeJ5UNDoV9SCRCpebFeG7", 21479632635350).await;
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    println!("🏆 Post-Unwind Liquid SOL Balance: {:.6} SOL", bot.get_balance().await);
}
