//! Pure Rust Utility to Close Empty SPL Token Accounts and Reclaim Rent SOL (Secure Key Loading)
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::message::Message;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::keypair::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::transaction::Transaction;
use std::fs;
use std::path::Path;
use std::str::FromStr;

pub const SOLANA_MAINNET_RPC: &str = "https://api.mainnet-beta.solana.com";

fn load_secure_keypair() -> Result<Keypair, Box<dyn std::error::Error>> {
    let keypair_path = if let Ok(p) = std::env::var("SOLANA_KEYPAIR_PATH") {
        p
    } else if Path::new("/home/ubuntu/.config/solana/mev_live.json").exists() {
        "/home/ubuntu/.config/solana/mev_live.json".to_string()
    } else if Path::new("data/mev_live.json").exists() {
        "data/mev_live.json".to_string()
    } else {
        return Err("No keypair found in standard locations. Please set SOLANA_KEYPAIR_PATH.".into());
    };

    let content = fs::read_to_string(&keypair_path)?;
    let trimmed = content.trim();

    let secret_key_bytes: Vec<u8> = if trimmed.starts_with('[') {
        serde_json::from_str(trimmed)?
    } else {
        bs58::decode(trimmed).into_vec()?
    };

    let keypair = Keypair::from_bytes(&secret_key_bytes)?;
    Ok(keypair)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧹 [RENT RECLAIM] Initializing Secure Rust Rent Reclaimer...");

    let keypair = load_secure_keypair()?;
    let payer_pubkey = keypair.pubkey();
    println!("🔑 Armed Wallet: {}", payer_pubkey);

    let client = reqwest::Client::new();

    // 1. Fetch initial balance
    let rpc_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getBalance",
        "params": [payer_pubkey.to_string()]
    });
    let res: serde_json::Value = client.post(SOLANA_MAINNET_RPC).json(&rpc_payload).send().await?.json().await?;
    let initial_lamports = res["result"]["value"].as_u64().unwrap_or(0);
    println!("💰 Pre-Reclaim Balance: {:.6} SOL", initial_lamports as f64 / 1e9);

    // 2. Fetch Recent Blockhash
    let bh_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLatestBlockhash",
        "params": [{ "commitment": "confirmed" }]
    });
    let bh_res: serde_json::Value = client.post(SOLANA_MAINNET_RPC).json(&bh_payload).send().await?.json().await?;
    let bh_str = bh_res["result"]["value"]["blockhash"].as_str().unwrap();
    let recent_blockhash = solana_sdk::hash::Hash::from_str(bh_str)?;

    let spl_token_prog = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")?;
    
    // Fetch all token accounts owned by this wallet
    let token_accs_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getTokenAccountsByOwner",
        "params": [
            payer_pubkey.to_string(),
            { "programId": spl_token_prog.to_string() },
            { "encoding": "jsonParsed" }
        ]
    });

    let token_res: serde_json::Value = client.post(SOLANA_MAINNET_RPC).json(&token_accs_payload).send().await?.json().await?;
    let mut empty_accounts_to_close = Vec::new();

    if let Some(accounts) = token_res["result"]["value"].as_array() {
        for acc in accounts {
            let pubkey_str = acc["pubkey"].as_str().unwrap_or("");
            let token_amount = acc["account"]["data"]["parsed"]["info"]["tokenAmount"]["uiAmount"].as_f64().unwrap_or(0.0);
            if token_amount == 0.0 && !pubkey_str.is_empty() {
                if let Ok(pk) = Pubkey::from_str(pubkey_str) {
                    empty_accounts_to_close.push(pk);
                }
            }
        }
    }

    if empty_accounts_to_close.is_empty() {
        println!("✅ [CLEAN] No zero-balance Associated Token Accounts found. All rent is already liquid!");
        return Ok(());
    }

    println!("🔍 Found {} empty token accounts to close and reclaim rent...", empty_accounts_to_close.len());

    let mut instructions = Vec::new();
    for acc in empty_accounts_to_close {
        instructions.push(Instruction {
            program_id: spl_token_prog,
            accounts: vec![
                AccountMeta::new(acc, false),              // account to close
                AccountMeta::new(payer_pubkey, false),     // destination for rent refund
                AccountMeta::new_readonly(payer_pubkey, true), // owner authority signer
            ],
            data: vec![9], // CloseAccount opcode
        });
    }

    let message = Message::new(&instructions, Some(&payer_pubkey));
    let mut tx = Transaction::new_unsigned(message);
    tx.sign(&[&keypair], recent_blockhash);

    let tx_bytes = bincode::serialize(&tx)?;
    let tx_base64 = BASE64_STANDARD.encode(&tx_bytes);

    let send_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sendTransaction",
        "params": [
            tx_base64,
            { "encoding": "base64", "skipPreflight": false, "preflightCommitment": "confirmed" }
        ]
    });

    let send_res: serde_json::Value = client.post(SOLANA_MAINNET_RPC).json(&send_payload).send().await?.json().await?;
    println!("📡 Broadcast Response: {:?}", send_res);

    if let Some(sig) = send_res.get("result").and_then(|r| r.as_str()) {
        println!("✅ [SUCCESS] Reclaim Tx Signature: https://solscan.io/tx/{}", sig);
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        let res_final: serde_json::Value = client.post(SOLANA_MAINNET_RPC).json(&rpc_payload).send().await?.json().await?;
        let final_lamports = res_final["result"]["value"].as_u64().unwrap_or(0);
        println!("🏆 [RENT RECOVERED] New Liquid SOL Balance: {:.6} SOL (+{:.6} SOL refunded)",
            final_lamports as f64 / 1e9,
            (final_lamports.saturating_sub(initial_lamports)) as f64 / 1e9
        );
    }

    Ok(())
}
