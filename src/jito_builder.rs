#![allow(dead_code)]
use anyhow::{bail, Context, Result};
use base64::prelude::*;
use log::{info, warn};
use serde_json;
use solana_sdk::{
    address_lookup_table::{state::AddressLookupTable, AddressLookupTableAccount},
    instruction::{AccountMeta, Instruction},
    message::{v0, VersionedMessage},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::VersionedTransaction,
};
use spl_associated_token_account::get_associated_token_address;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const JITO_TIP_ACCOUNTS: [&str; 8] = [
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
];

pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
pub const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
pub const SOLANA_MAINNET_RPC: &str = "https://mainnet.helius-rpc.com/?api-key=9b358df5-981f-4a3f-8f25-5ff9af04653f";

// Jupiter High-Performance Frankfurt Endpoints
pub const JUPITER_QUOTE_URL: &str = "https://lite-api.jup.ag/swap/v1/quote";
pub const JUPITER_SWAP_IX_URL: &str = "https://lite-api.jup.ag/swap/v1/swap-instructions";

/// V40: Minimum Jupiter-confirmed gross profit (lamports) before a bundle may be built.
/// Below this the spread is phantom — reject pre-build instead of bidding into auctions
/// we cannot win. 120k covers ~38k total fees + a competitive tip on any retained net.
pub const MIN_CONFIRMED_QUOTE_PROFIT_LAMPORTS: u64 = 120_000;

/// V40: Minimum retained net profit after tip + fees for a route to be worth landing.
pub const MIN_RETAINED_NET_PROFIT_LAMPORTS: u64 = 25_000;

/// V42 REFLEX: when the trigger came from a LaserStream pool event within
/// EVENT_FRESH_WINDOW_MS, we dispatch even if Jupiter's quote shows a small deficit —
/// the race may resolve in our favor by landing time, and a reverted atomic bundle
/// costs only gas (~9k lamports). This bounds the worst-case *successful* fill loss:
/// a fresh-quote deficit larger than this tolerance is a KNOWN loss, not a race, and
/// still gets skipped (executing it would lock in the deficit, not revert).
pub const REFLEX_MARGINAL_TOLERANCE_LAMPORTS: i64 = 30_000;

/// V42: requested compute units per route type. The old 1_400_000 request was the
/// runtime max — Jito auctions rank by tip ÷ REQUESTED CU, so we were bidding with a
/// ~3x handicap and would burn lamports per requested CU under SIMD-0553. Actual
/// consumption for these routes is ~120-350k CU; these limits keep ~1.5x headroom.
pub const DUAL_ROUTE_CU_LIMIT: u32 = 400_000;
pub const TRIANGULAR_ROUTE_CU_LIMIT: u32 = 500_000;

// === DIRECT INSTRUCTION CONSTANTS (Council Certified) ===
pub const PUMP_FUN_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
pub const PUMP_FUN_GLOBAL_CONFIG: &str = "4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf";
pub const PUMP_FUN_FEE_RECIPIENT: &str = "CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM";
pub const PUMP_FUN_EVENT_AUTHORITY: &str = "Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1";

pub const RAYDIUM_CPMM_PROGRAM_ID: &str = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C";
pub const RAYDIUM_V4_PROGRAM_ID: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";
pub const RAYDIUM_CPMM_AUTHORITY: &str = "DNXgeM9EiiaAbaWZaZGjKTdBdnscWioGoSCM5fg8WDBJ";

pub const METEORA_DLMM_PROGRAM_ID: &str = "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo";
pub const ORCA_WHIRLPOOL_PROGRAM_ID: &str = "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc";

// Instruction Discriminators
pub const PUMP_BUY_DISCRIMINATOR: [u8; 8] = [0x69, 0xc0, 0x26, 0x07, 0xa5, 0x1f, 0x0c, 0x36];
pub const PUMP_SELL_DISCRIMINATOR: [u8; 8] = [0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad];
pub const RAYDIUM_CPMM_SWAP_BASE_IN_DISCRIMINATOR: [u8; 8] = [0x8f, 0xee, 0xdb, 0xdd, 0xce, 0xaf, 0x46, 0xbf];
pub const RAYDIUM_V4_SWAP_DISCRIMINATOR: u8 = 9;
pub const METEORA_SWAP_DISCRIMINATOR: [u8; 8] = [0x2b, 0x04, 0xed, 0x0b, 0x1a, 0xc9, 0x1e, 0x62];
pub const ORCA_SWAP_DISCRIMINATOR: [u8; 8] = [0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8];

#[derive(Clone, Debug)]
pub struct SignedCircularPackage {
    pub tx_signature_base58: String,
    pub signed_tx_base64: String,
    pub signed_tx_base58: String,
    pub input_lamports: u64,
    pub final_lamports: u64,
    pub net_profit_lamports: u64,
    pub tip_lamports: u64,
    pub uuid: String,
}

pub struct JitoBuilder {
    pub keypair: Keypair,
    pub payer_pubkey: String,
    pub tip_rotation_index: AtomicUsize,
    pub http_client: reqwest::Client,
    pub rpc_url: String,
}

impl JitoBuilder {
    pub fn load_from_path(path_str: &str) -> Result<Self> {
        let path = Path::new(path_str);
        if !path.exists() {
            bail!("Keypair file not found at: {}", path_str);
        }

        let content = fs::read_to_string(path).context("Failed to read keypair file")?;
        let trimmed = content.trim();

        let mut secret_key_bytes: Vec<u8> = if trimmed.starts_with('[') {
            serde_json::from_str(trimmed).context("Invalid JSON keypair array")?
        } else if trimmed.starts_with('{') {
            let val: serde_json::Value = serde_json::from_str(trimmed)?;
            if let Some(arr) = val.get("secret_key").or_else(|| val.get("private_key")) {
                serde_json::from_value(arr.clone())?
            } else {
                bail!("JSON object does not contain secret_key or private_key");
            }
        } else {
            bs58::decode(trimmed).into_vec().context("Failed to decode base58 secret key")?
        };

        let keypair = if secret_key_bytes.len() == 64 {
            Keypair::from_bytes(&secret_key_bytes).context("Failed to construct Keypair from 64-byte array")?
        } else {
            Keypair::from_base58_string(trimmed)
        };

        let payer_pubkey = keypair.pubkey().to_string();
        info!("🔑 [ATOMIC JITO BUILDER] Solana SDK Keypair Armed: {}", payer_pubkey);

        Ok(Self {
            keypair,
            payer_pubkey,
            tip_rotation_index: AtomicUsize::new(0),
            http_client: reqwest::Client::builder()
                .tcp_nodelay(true)
                .pool_idle_timeout(std::time::Duration::from_secs(90))
                // V40 ZOMBIE-FREEZE FIX: this client had NO timeout — a single stalled
                // Jupiter/ALT/sendBundle request parked its caller forever and, because
                // engines execute inline in scanner hot loops, froze whole processes.
                .connect_timeout(std::time::Duration::from_millis(1500))
                .timeout(std::time::Duration::from_millis(2500))
                .build()?,
            rpc_url: SOLANA_MAINNET_RPC.to_string(),
        })
    }

    pub fn select_tip_account(&self) -> &'static str {
        let idx = self.tip_rotation_index.fetch_add(1, Ordering::Relaxed);
        JITO_TIP_ACCOUNTS[idx % JITO_TIP_ACCOUNTS.len()]
    }

    // =========================================================================
    // ⚡ DIRECT IN-MEMORY INSTRUCTION BUILDERS (< 5 microseconds execution)
    // =========================================================================

    /// Derives Pump.fun Bonding Curve PDA: seeds = ["bonding-curve", token_mint]
    pub fn derive_pump_bonding_curve(token_mint: &Pubkey) -> (Pubkey, u8) {
        let program_id = Pubkey::from_str(PUMP_FUN_PROGRAM_ID).unwrap();
        Pubkey::find_program_address(&[b"bonding-curve", token_mint.as_ref()], &program_id)
    }

    /// Creates a direct Pump.fun Buy Instruction natively in Rust
    pub fn create_direct_pump_buy_ix(
        user_wallet: &Pubkey,
        token_mint: &Pubkey,
        user_token_ata: &Pubkey,
        token_amount_out: u64,
        max_sol_cost_lamports: u64,
    ) -> Instruction {
        let program_id = Pubkey::from_str(PUMP_FUN_PROGRAM_ID).unwrap();
        let global_config = Pubkey::from_str(PUMP_FUN_GLOBAL_CONFIG).unwrap();
        let fee_recipient = Pubkey::from_str(PUMP_FUN_FEE_RECIPIENT).unwrap();
        let event_authority = Pubkey::from_str(PUMP_FUN_EVENT_AUTHORITY).unwrap();

        let (bonding_curve, _) = Self::derive_pump_bonding_curve(token_mint);
        let associated_bonding_curve = get_associated_token_address(&bonding_curve, token_mint);

        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(&PUMP_BUY_DISCRIMINATOR);
        data.extend_from_slice(&token_amount_out.to_le_bytes());
        data.extend_from_slice(&max_sol_cost_lamports.to_le_bytes());

        let accounts = vec![
            AccountMeta::new_readonly(global_config, false),
            AccountMeta::new(fee_recipient, false),
            AccountMeta::new_readonly(*token_mint, false),
            AccountMeta::new(bonding_curve, false),
            AccountMeta::new(associated_bonding_curve, false),
            AccountMeta::new(*user_token_ata, false),
            AccountMeta::new(*user_wallet, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(solana_sdk::sysvar::rent::id(), false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id, false),
        ];

        Instruction { program_id, accounts, data }
    }

    /// Creates a direct Pump.fun Sell Instruction natively in Rust
    pub fn create_direct_pump_sell_ix(
        user_wallet: &Pubkey,
        token_mint: &Pubkey,
        user_token_ata: &Pubkey,
        token_amount_in: u64,
        min_sol_out_lamports: u64,
    ) -> Instruction {
        let program_id = Pubkey::from_str(PUMP_FUN_PROGRAM_ID).unwrap();
        let global_config = Pubkey::from_str(PUMP_FUN_GLOBAL_CONFIG).unwrap();
        let fee_recipient = Pubkey::from_str(PUMP_FUN_FEE_RECIPIENT).unwrap();
        let event_authority = Pubkey::from_str(PUMP_FUN_EVENT_AUTHORITY).unwrap();

        let (bonding_curve, _) = Self::derive_pump_bonding_curve(token_mint);
        let associated_bonding_curve = get_associated_token_address(&bonding_curve, token_mint);

        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(&PUMP_SELL_DISCRIMINATOR);
        data.extend_from_slice(&token_amount_in.to_le_bytes());
        data.extend_from_slice(&min_sol_out_lamports.to_le_bytes());

        let accounts = vec![
            AccountMeta::new_readonly(global_config, false),
            AccountMeta::new(fee_recipient, false),
            AccountMeta::new_readonly(*token_mint, false),
            AccountMeta::new(bonding_curve, false),
            AccountMeta::new(associated_bonding_curve, false),
            AccountMeta::new(*user_token_ata, false),
            AccountMeta::new(*user_wallet, true),
            AccountMeta::new_readonly(solana_sdk::system_program::id(), false),
            AccountMeta::new_readonly(spl_associated_token_account::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id, false),
        ];

        Instruction { program_id, accounts, data }
    }

    /// Creates a direct Raydium AMM V4 Swap Instruction natively in Rust
    pub fn create_direct_raydium_v4_swap_ix(
        pool_id: &Pubkey,
        amm_authority: &Pubkey,
        amm_open_orders: &Pubkey,
        amm_target_orders: &Pubkey,
        pool_coin_token_account: &Pubkey,
        pool_pc_token_account: &Pubkey,
        serum_market: &Pubkey,
        user_source_token: &Pubkey,
        user_destination_token: &Pubkey,
        user_owner: &Pubkey,
        amount_in: u64,
        min_amount_out: u64,
    ) -> Instruction {
        let program_id = Pubkey::from_str(RAYDIUM_V4_PROGRAM_ID).unwrap();

        let mut data = Vec::with_capacity(17);
        data.push(RAYDIUM_V4_SWAP_DISCRIMINATOR);
        data.extend_from_slice(&amount_in.to_le_bytes());
        data.extend_from_slice(&min_amount_out.to_le_bytes());

        let accounts = vec![
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new(*pool_id, false),
            AccountMeta::new_readonly(*amm_authority, false),
            AccountMeta::new(*amm_open_orders, false),
            AccountMeta::new(*amm_target_orders, false),
            AccountMeta::new(*pool_coin_token_account, false),
            AccountMeta::new(*pool_pc_token_account, false),
            AccountMeta::new_readonly(*serum_market, false),
            AccountMeta::new(*user_source_token, false),
            AccountMeta::new(*user_destination_token, false),
            AccountMeta::new_readonly(*user_owner, true),
        ];

        Instruction { program_id, accounts, data }
    }

    /// Creates a direct Meteora DLMM Swap Instruction natively in Rust
    pub fn create_direct_meteora_dlmm_swap_ix(
        lb_pair: &Pubkey,
        bin_array_bitmap_extension: Option<&Pubkey>,
        reserve_x: &Pubkey,
        reserve_y: &Pubkey,
        user_token_in: &Pubkey,
        user_token_out: &Pubkey,
        token_x_mint: &Pubkey,
        token_y_mint: &Pubkey,
        oracle: &Pubkey,
        user_owner: &Pubkey,
        amount_in: u64,
        min_amount_out: u64,
    ) -> Instruction {
        let program_id = Pubkey::from_str(METEORA_DLMM_PROGRAM_ID).unwrap();
        let event_authority = Pubkey::from_str("D1HjA2hG8gWqXoW5w8V7GgP8gqJ7zN3kE2xV9gW8qE7z").unwrap_or(program_id);

        let mut accounts = vec![
            AccountMeta::new(*lb_pair, false),
            AccountMeta::new(*reserve_x, false),
            AccountMeta::new(*reserve_y, false),
            AccountMeta::new(*user_token_in, false),
            AccountMeta::new(*user_token_out, false),
            AccountMeta::new_readonly(*token_x_mint, false),
            AccountMeta::new_readonly(*token_y_mint, false),
            AccountMeta::new_readonly(*oracle, false),
            AccountMeta::new(*user_owner, true),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id, false),
        ];
        if let Some(bitmap) = bin_array_bitmap_extension {
            accounts.insert(1, AccountMeta::new_readonly(*bitmap, false));
        }

        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(&METEORA_SWAP_DISCRIMINATOR);
        data.extend_from_slice(&amount_in.to_le_bytes());
        data.extend_from_slice(&min_amount_out.to_le_bytes());

        Instruction { program_id, accounts, data }
    }

    /// Creates a direct Raydium CPMM Swap Instruction natively in Rust
    pub fn create_direct_raydium_cpmm_swap_ix(
        pool_state: &Pubkey,
        authority: &Pubkey,
        token_a_vault: &Pubkey,
        token_b_vault: &Pubkey,
        user_source_token: &Pubkey,
        user_destination_token: &Pubkey,
        user_owner: &Pubkey,
        amount_in: u64,
        min_amount_out: u64,
    ) -> Instruction {
        let program_id = Pubkey::from_str(RAYDIUM_CPMM_PROGRAM_ID).unwrap();

        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(&RAYDIUM_CPMM_SWAP_BASE_IN_DISCRIMINATOR);
        data.extend_from_slice(&amount_in.to_le_bytes());
        data.extend_from_slice(&min_amount_out.to_le_bytes());

        let token_2022_id = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap_or(spl_token::id());
        let accounts = vec![
            AccountMeta::new(*user_owner, true),
            AccountMeta::new_readonly(*authority, false),
            AccountMeta::new(*pool_state, false),
            AccountMeta::new(*token_a_vault, false),
            AccountMeta::new(*token_b_vault, false),
            AccountMeta::new(*user_source_token, false),
            AccountMeta::new(*user_destination_token, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(token_2022_id, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::clock::id(), false),
        ];

        Instruction { program_id, accounts, data }
    }

    /// Creates a direct Orca Whirlpool Swap Instruction natively in Rust
    pub fn create_direct_orca_whirlpool_swap_ix(
        whirlpool: &Pubkey,
        token_authority: &Pubkey,
        token_owner_account_a: &Pubkey,
        token_vault_a: &Pubkey,
        token_owner_account_b: &Pubkey,
        token_vault_b: &Pubkey,
        tick_array_0: &Pubkey,
        tick_array_1: &Pubkey,
        tick_array_2: &Pubkey,
        oracle: &Pubkey,
        amount: u64,
        other_amount_threshold: u64,
        sqrt_price_limit: u128,
        amount_specified_is_input: bool,
        a_to_b: bool,
    ) -> Instruction {
        let program_id = Pubkey::from_str(ORCA_WHIRLPOOL_PROGRAM_ID).unwrap();

        let accounts = vec![
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(*token_authority, true),
            AccountMeta::new(*whirlpool, false),
            AccountMeta::new(*token_owner_account_a, false),
            AccountMeta::new(*token_vault_a, false),
            AccountMeta::new(*token_owner_account_b, false),
            AccountMeta::new(*token_vault_b, false),
            AccountMeta::new_readonly(*tick_array_0, false),
            AccountMeta::new_readonly(*tick_array_1, false),
            AccountMeta::new_readonly(*tick_array_2, false),
            AccountMeta::new_readonly(*oracle, false),
        ];

        let mut data = Vec::with_capacity(42);
        data.extend_from_slice(&ORCA_SWAP_DISCRIMINATOR);
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&other_amount_threshold.to_le_bytes());
        data.extend_from_slice(&sqrt_price_limit.to_le_bytes());
        data.push(amount_specified_is_input as u8);
        data.push(a_to_b as u8);

        Instruction { program_id, accounts, data }
    }

    // =========================================================================
    // 🏛️ ZERO-LATENCY DIRECT DUAL-LEG ATOMIC BUNDLE BUILDER
    // =========================================================================

    /// Builds a direct in-memory atomic dual-leg arbitrage transaction with zero HTTP delay
    pub fn build_direct_in_memory_dual_leg_tx(
        &self,
        token_mint: &Pubkey,
        buy_dex: &str,
        sell_dex: &str,
        input_lamports: u64,
        min_tokens_out: u64,
        min_final_lamports: u64,
        tip_lamports: u64,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<SignedCircularPackage> {
        let user_wallet = self.keypair.pubkey();
        let user_token_ata = get_associated_token_address(&user_wallet, token_mint);
        let tip_account = Pubkey::from_str(self.select_tip_account())?;

        let mut instructions: Vec<Instruction> = Vec::with_capacity(7);

        // 1. Optimized Compute Budget (250k CU as certified by GLM-5.2 Lead Directive)
        instructions.push(solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(250_000));
        let priority_fee = (tip_lamports / 500).max(5_000);
        instructions.push(solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(priority_fee));

        // 2. Ephemeral In-Tx ATA Creation
        instructions.push(spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            &user_wallet,
            &user_wallet,
            token_mint,
            &spl_token::id(),
        ));

        // 3. Leg 1: Buy Token with SOL
        let buy_upper = buy_dex.to_uppercase();
        let sell_upper = sell_dex.to_uppercase();

        if buy_upper.contains("PUMP") {
            instructions.push(Self::create_direct_pump_buy_ix(
                &user_wallet,
                token_mint,
                &user_token_ata,
                min_tokens_out,
                input_lamports,
            ));
        } else {
            bail!("Direct in-memory fast-path: Buy venue {} not supported directly, deferring to aggregator route", buy_dex);
        }

        // 4. Leg 2: Sell Token back to SOL
        if sell_upper.contains("PUMP") {
            instructions.push(Self::create_direct_pump_sell_ix(
                &user_wallet,
                token_mint,
                &user_token_ata,
                min_tokens_out,
                min_final_lamports,
            ));
        } else {
            bail!("Direct in-memory fast-path: Sell venue {} not supported directly, deferring to aggregator route", sell_dex);
        }

        // 5. In-Tx ATA Close (100% Rent Lamports Refunded to Wallet)
        instructions.push(spl_token::instruction::close_account(
            &spl_token::id(),
            &user_token_ata,
            &user_wallet,
            &user_wallet,
            &[&user_wallet],
        )?);

        // 6. Atomic Jito Tip Transfer
        instructions.push(solana_sdk::system_instruction::transfer(
            &user_wallet,
            &tip_account,
            tip_lamports,
        ));

        let v0_msg = v0::Message::try_compile(
            &user_wallet,
            &instructions,
            &[],
            recent_blockhash,
        ).context("Failed to compile direct v0 message")?;

        let v0_tx = VersionedTransaction::try_new(
            VersionedMessage::V0(v0_msg),
            &[&self.keypair],
        ).context("Failed to sign direct versioned transaction")?;

        let raw_tx_bytes = bincode::serialize(&v0_tx)?;
        let signed_tx_base64 = BASE64_STANDARD.encode(&raw_tx_bytes);
        let signed_tx_base58 = bs58::encode(&raw_tx_bytes).into_string();
        let sig = v0_tx.signatures[0].to_string();

        let net_profit_lamports = min_final_lamports.saturating_sub(input_lamports + tip_lamports + 5_000);
        let uuid = format!("direct_circ_{}_{}", chrono::Utc::now().timestamp_millis(), &token_mint.to_string()[..4]);

        Ok(SignedCircularPackage {
            tx_signature_base58: sig,
            signed_tx_base64,
            signed_tx_base58,
            input_lamports,
            final_lamports: min_final_lamports,
            net_profit_lamports,
            tip_lamports,
            uuid,
        })
    }

    // =========================================================================
    // 🌐 LAZY JUPITER HTTP FALLBACK (For Unknown Pools / Multi-Hop Routes)
    pub fn map_dex_to_jupiter_label(dex_name: &str) -> &'static str {
        let upper = dex_name.to_uppercase();
        if upper.contains("METEORA") || upper.contains("DLMM") {
            "Meteora%20DLMM,Meteora"
        } else if upper.contains("RAYDIUM") || upper.contains("CPMM") || upper.contains("CLMM") {
            "Raydium%20CP,Raydium,Raydium%20CLMM"
        } else if upper.contains("ORCA") || upper.contains("WHIRLPOOL") {
            "Whirlpool,Orca"
        } else if upper.contains("PUMP") {
            "Pump.fun,Pump"
        } else {
            ""
        }
    }

    /// Helper to fetch Jupiter quotes with direct DEX pinning and automatic fallback
    pub async fn fetch_jupiter_quote(
        &self,
        input_mint: &str,
        output_mint: &str,
        amount: u64,
        slippage_bps: u32,
        dex_filter: Option<&str>,
    ) -> serde_json::Value {
        if let Some(dex) = dex_filter {
            let jup_dex = Self::map_dex_to_jupiter_label(dex);
            if !jup_dex.is_empty() {
                let direct_url = format!(
                    "{}?inputMint={}&outputMint={}&amount={}&slippageBps={}&onlyDirectRoutes=true&dexes={}",
                    JUPITER_QUOTE_URL, input_mint, output_mint, amount, slippage_bps, jup_dex
                );
                if let Ok(resp) = self.http_client.get(&direct_url).timeout(std::time::Duration::from_millis(800)).send().await {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if json.get("error").is_none() && json.get("outAmount").is_some() {
                            return json;
                        }
                    }
                }
            }
        }

        let fallback_url = format!(
            "{}?inputMint={}&outputMint={}&amount={}&slippageBps={}&onlyDirectRoutes=false",
            JUPITER_QUOTE_URL, input_mint, output_mint, amount, slippage_bps
        );

        if let Ok(resp) = self.http_client.get(&fallback_url).timeout(std::time::Duration::from_millis(800)).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                return json;
            }
        }
        serde_json::Value::Null
    }

    /// Builds a True Atomic Dual-Leg Circular Arbitrage Transaction with Direct In-Memory Priority
    #[allow(clippy::too_many_arguments)]
    pub async fn build_atomic_circular_arbitrage_tx(
        &self,
        intermediate_mint: &str,
        buy_dex: &str,
        sell_dex: &str,
        input_lamports: u64,
        slippage_bps: u32,
        tip_lamports: u64,
        recent_blockhash: solana_sdk::hash::Hash,
        allow_marginal_dispatch: bool,
    ) -> Result<SignedCircularPackage> {
        // V42 KILL SWITCH: operator halt takes precedence over everything, checked
        // immediately before any quote/network spend and again before signing below.
        if crate::jito::is_trading_killed() {
            bail!("⛔ KILL SWITCH ACTIVE: bundle build halted by operator");
        }
        // Fast-path: Only use direct in-memory instructions for pure Pump.fun bonding curves
        let buy_is_pump = buy_dex.to_uppercase().contains("PUMP");
        let sell_is_pump = sell_dex.to_uppercase().contains("PUMP");

        if buy_is_pump && sell_is_pump {
            if let Ok(token_pubkey) = Pubkey::from_str(intermediate_mint) {
                let min_tokens_out = 1_u64;
                let min_final = input_lamports + tip_lamports + 30_000;
                return self.build_direct_in_memory_dual_leg_tx(
                    &token_pubkey,
                    buy_dex,
                    sell_dex,
                    input_lamports,
                    min_tokens_out,
                    min_final,
                    tip_lamports,
                    recent_blockhash,
                );
            }
        }

        let tip_account_str = self.select_tip_account();
        let tip_account = Pubkey::from_str(tip_account_str)?;
        let uuid = format!("atomic_circ_{}_{}", chrono::Utc::now().timestamp_millis(), &intermediate_mint[..4.min(intermediate_mint.len())]);

        // 1. Leg A: Quote SOL -> Intermediate Token pinned to buy_dex
        //    Use otherAmountThreshold (pessimistic/worst-case with slippage) for the intermediate amount.
        //    This gives a realistic token count that Leg B will actually receive on-chain.
        let quote_res_a = self.fetch_jupiter_quote(SOL_MINT, intermediate_mint, input_lamports, slippage_bps.min(25), Some(buy_dex)).await;
        let expected_intermediate_amount: u64 = quote_res_a.get("otherAmountThreshold")
            .or_else(|| quote_res_a.get("outAmount"))
            .and_then(|v| v.as_str())
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);

        if expected_intermediate_amount == 0 {
            bail!("Leg A quote returned 0 intermediate tokens for {}", intermediate_mint);
        }

        // 2. Leg B: Quote Intermediate Token -> SOL pinned to sell_dex
        //    Use outAmount here — the invariant check with tip recalculation handles the safety margin.
        let quote_res_b = self.fetch_jupiter_quote(intermediate_mint, SOL_MINT, expected_intermediate_amount, slippage_bps.min(25), Some(sell_dex)).await;
        let final_expected_lamports: u64 = quote_res_b.get("outAmount")
            .or_else(|| quote_res_b.get("otherAmountThreshold"))
            .and_then(|v| v.as_str())
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);

        // 2.5 Route Diversity Check: Assert Leg A AMM != Leg B AMM
        let amm_a = quote_res_a.get("routePlan")
            .and_then(|r| r.as_array())
            .and_then(|a| a.first())
            .and_then(|s| s.get("swapInfo"))
            .and_then(|si| si.get("ammKey"))
            .and_then(|k| k.as_str())
            .unwrap_or("AMM_A");

        let amm_b = quote_res_b.get("routePlan")
            .and_then(|r| r.as_array())
            .and_then(|a| a.first())
            .and_then(|s| s.get("swapInfo"))
            .and_then(|si| si.get("ammKey"))
            .and_then(|k| k.as_str())
            .unwrap_or("AMM_B");

        if amm_a == amm_b && amm_a != "AMM_A" {
            bail!("🛑 ROUTE DIVERSITY REJECT: Both legs routed through the same AMM pool {}. Cross-DEX spatial requirement violated.", amm_a);
        }

        // 3. V42 Freshness-Aware Profit Gate & Whale-Style Dynamic Tip
        //
        // STRICT path (stale poll triggers): unchanged — Jupiter must confirm ≥120k
        // gross profit; phantom spreads die here without paying for the lesson.
        //
        // REFLEX path (LaserStream event-fresh trigger): dispatch down to a bounded
        // marginal deficit. Rationale (user-mandated fire-first): an atomic bundle that
        // loses the race REVERTS and costs only gas (~9k lamports); tips are
        // success-contingent. Deficits beyond REFLEX_MARGINAL_TOLERANCE are KNOWN losses
        // (a fresh quote showing them means execution locks them in, not reverts), so
        // they still bail — but with telemetry so gate policy can be re-tuned from data.
        let base_gas_lamports: u64 = 19_000; // 5k base + priority floor estimate
        let quote_profit_i64 =
            final_expected_lamports as i64 - (input_lamports + base_gas_lamports) as i64;
        let raw_quote_profit = quote_profit_i64.max(0) as u64;

        let confirmed_floor_i64: i64 = if allow_marginal_dispatch {
            -(REFLEX_MARGINAL_TOLERANCE_LAMPORTS)
        } else {
            MIN_CONFIRMED_QUOTE_PROFIT_LAMPORTS as i64
        };
        if quote_profit_i64 < confirmed_floor_i64 {
            bail!("🛑 QUOTE PROFIT GATE: Jupiter-confirmed profit {} lamports vs floor {} ({})",
                quote_profit_i64,
                confirmed_floor_i64,
                if allow_marginal_dispatch { "reflex zone exceeded — known loss, skipped" } else { "phantom spread rejected pre-build" });
        }

        // Tip: strict mode bids 60% of confirmed profit clamped [25k, 1.5M].
        // Reflex mode drops the floor to Jito's minimum viable tip (1000) — if the race
        // is lost the tip is never paid anyway, and a minimal bid still wins quiet auctions.
        let dyn_tip_floor = crate::jito::current_tip_floor_p50();
        let effective_tip = if allow_marginal_dispatch && raw_quote_profit < MIN_CONFIRMED_QUOTE_PROFIT_LAMPORTS {
            ((raw_quote_profit as f64 * 0.60) as u64).clamp(dyn_tip_floor.min(50_000).max(1_000), 1_500_000)
        } else {
            ((raw_quote_profit as f64 * 0.60) as u64).clamp(dyn_tip_floor.min(250_000).max(25_000), 1_500_000)
        };

        let priority_fee_price = (effective_tip / 1000).max(10_000);
        let network_fee_lamports = (priority_fee_price * DUAL_ROUTE_CU_LIMIT as u64) / 1_000_000 + 5_000;
        let min_required_lamports = input_lamports + effective_tip + network_fee_lamports;

        let net_profit_lamports = final_expected_lamports.saturating_sub(min_required_lamports);
        if !allow_marginal_dispatch && net_profit_lamports < MIN_RETAINED_NET_PROFIT_LAMPORTS {
            bail!("🛑 NET PROFIT GATE: after tip {} + fees {}, retained net only {} lamports (< {}). Route rejected.",
                effective_tip, network_fee_lamports, net_profit_lamports, MIN_RETAINED_NET_PROFIT_LAMPORTS);
        }

        if allow_marginal_dispatch && raw_quote_profit < MIN_CONFIRMED_QUOTE_PROFIT_LAMPORTS {
            info!("🔥 [REFLEX DISPATCH] {} | event-fresh trigger | quoted profit {} lamports (marginal zone) | firing atomic probe — revert costs gas only",
                intermediate_mint, quote_profit_i64);
        } else {
            info!("🟢 [CONFIRMED ROUTE] In: {} -> Expected Out: {} | Tip (60%): {} | Net Profit: +{} lamports",
                input_lamports, final_expected_lamports, effective_tip, net_profit_lamports);
        }

        // 4. Fetch swap instructions for Leg A and Leg B
        let ix_payload_a = serde_json::json!({
            "quoteResponse": quote_res_a,
            "userPublicKey": self.payer_pubkey,
            "wrapAndUnwrapSol": true,
            "dynamicComputeUnitLimit": true
        });

        let ix_payload_b = serde_json::json!({
            "quoteResponse": quote_res_b,
            "userPublicKey": self.payer_pubkey,
            "wrapAndUnwrapSol": true,
            "dynamicComputeUnitLimit": true
        });

        let (ix_res_a_result, ix_res_b_result) = tokio::join!(
            self.http_client.post(JUPITER_SWAP_IX_URL).json(&ix_payload_a).send(),
            self.http_client.post(JUPITER_SWAP_IX_URL).json(&ix_payload_b).send()
        );

        let ix_res_a = ix_res_a_result?.json::<serde_json::Value>().await?;
        let ix_res_b = ix_res_b_result?.json::<serde_json::Value>().await?;

        // 5. Collect Address Lookup Table (ALT) addresses
        let mut alt_set: HashSet<String> = HashSet::new();
        if let Some(alts) = ix_res_a.get("addressLookupTableAddresses").and_then(|v| v.as_array()) {
            for a in alts {
                if let Some(s) = a.as_str() {
                    alt_set.insert(s.to_string());
                }
            }
        }
        if let Some(alts) = ix_res_b.get("addressLookupTableAddresses").and_then(|v| v.as_array()) {
            for a in alts {
                if let Some(s) = a.as_str() {
                    alt_set.insert(s.to_string());
                }
            }
        }

        let alt_addresses: Vec<String> = alt_set.into_iter().collect();
        let loaded_alts = self.fetch_lookup_tables(&alt_addresses).await;

        // 6. Assemble Instructions
        let mut compiled_instructions: Vec<Instruction> = Vec::new();
        compiled_instructions.push(solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(
            DUAL_ROUTE_CU_LIMIT,
        ));
        compiled_instructions.push(solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(priority_fee_price));

        if let Some(setup_ixs) = ix_res_a.get("setupInstructions").and_then(|v| v.as_array()) {
            for ix_val in setup_ixs {
                if let Some(ix) = self.parse_instruction_json(ix_val) {
                    compiled_instructions.push(ix);
                }
            }
        }

        if let Some(swap_ix_val) = ix_res_a.get("swapInstruction") {
            if let Some(ix) = self.parse_instruction_json(swap_ix_val) {
                compiled_instructions.push(ix);
            }
        }

        if let Some(swap_ix_val) = ix_res_b.get("swapInstruction") {
            if let Some(ix) = self.parse_instruction_json(swap_ix_val) {
                compiled_instructions.push(ix);
            }
        }

        if let Some(cleanup_val) = ix_res_b.get("cleanupInstruction") {
            if let Some(ix) = self.parse_instruction_json(cleanup_val) {
                compiled_instructions.push(ix);
            }
        }

        // Ephemeral ATA close instruction
        let payer_pubkey = self.keypair.pubkey();
        if let Ok(token_pubkey) = Pubkey::from_str(intermediate_mint) {
            let intermediate_ata = get_associated_token_address(&payer_pubkey, &token_pubkey);
            let close_ata_ix = spl_token::instruction::close_account(
                &spl_token::id(),
                &intermediate_ata,
                &payer_pubkey,
                &payer_pubkey,
                &[&payer_pubkey],
            )?;
            compiled_instructions.push(close_ata_ix);
        }

        // Jito Tip Transfer (uses recalculated tip from actual Jupiter quote profit)
        compiled_instructions.push(solana_sdk::system_instruction::transfer(
            &payer_pubkey,
            &tip_account,
            effective_tip,
        ));

        // V42 KILL SWITCH (final gate): nothing gets signed after an operator halt.
        if crate::jito::is_trading_killed() {
            bail!("⛔ KILL SWITCH ACTIVE: signing aborted by operator halt");
        }

        // 7. Compile V0 Message
        let v0_msg = v0::Message::try_compile(
            &payer_pubkey,
            &compiled_instructions,
            &loaded_alts,
            recent_blockhash,
        ).context("Failed to compile v0 message")?;

        let v0_tx = VersionedTransaction::try_new(
            VersionedMessage::V0(v0_msg),
            &[&self.keypair],
        ).context("Failed to sign versioned transaction")?;

        let raw_tx_bytes = bincode::serialize(&v0_tx)?;
        let signed_tx_base64 = BASE64_STANDARD.encode(&raw_tx_bytes);
        let signed_tx_base58 = bs58::encode(&raw_tx_bytes).into_string();
        let sig = v0_tx.signatures[0].to_string();

        Ok(SignedCircularPackage {
            tx_signature_base58: sig,
            signed_tx_base64,
            signed_tx_base58,
            input_lamports,
            final_lamports: final_expected_lamports,
            net_profit_lamports,
            tip_lamports: effective_tip,
            uuid,
        })
    }

    /// Builds a True Atomic 3-Hop Triangular Arbitrage Transaction (SOL -> Token A -> USDC -> SOL)
    #[allow(clippy::too_many_arguments)]
    pub async fn build_atomic_triangular_arbitrage_tx(
        &self,
        token_a_mint: &str,
        bridge_mint: &str,
        dex_hop1: &str,
        dex_hop2: &str,
        dex_hop3: &str,
        input_lamports: u64,
        slippage_bps: u32,
        tip_lamports: u64,
        recent_blockhash: solana_sdk::hash::Hash,
        allow_marginal_dispatch: bool,
    ) -> Result<SignedCircularPackage> {
        // V42 KILL SWITCH: operator halt precedes all quote/network spend.
        if crate::jito::is_trading_killed() {
            bail!("⛔ KILL SWITCH ACTIVE: triangular bundle build halted by operator");
        }
        let tip_account_str = self.select_tip_account();
        let tip_account = Pubkey::from_str(tip_account_str)?;
        let uuid = format!("atomic_tri_{}_{}", chrono::Utc::now().timestamp_millis(), &token_a_mint[..4.min(token_a_mint.len())]);

        // 1. Hop 1: SOL -> Token A pinned to dex_hop1
        let quote_res_1 = self.fetch_jupiter_quote(SOL_MINT, token_a_mint, input_lamports, slippage_bps, Some(dex_hop1)).await;
        let min_amount_token_a: u64 = quote_res_1.get("otherAmountThreshold")
            .or_else(|| quote_res_1.get("outAmount"))
            .and_then(|v| v.as_str())
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);

        if min_amount_token_a == 0 {
            bail!("Triangular Leg 1 quote returned 0 tokens for {}", token_a_mint);
        }

        // 2. Hop 2: Token A -> USDC pinned to dex_hop2
        let quote_res_2 = self.fetch_jupiter_quote(token_a_mint, bridge_mint, min_amount_token_a, slippage_bps, Some(dex_hop2)).await;
        let min_amount_usdc: u64 = quote_res_2.get("otherAmountThreshold")
            .or_else(|| quote_res_2.get("outAmount"))
            .and_then(|v| v.as_str())
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);

        if min_amount_usdc == 0 {
            bail!("Triangular Leg 2 quote returned 0 USDC for {}", token_a_mint);
        }

        // 3. Hop 3: USDC -> SOL pinned to dex_hop3
        let quote_res_3 = self.fetch_jupiter_quote(bridge_mint, SOL_MINT, min_amount_usdc, slippage_bps, Some(dex_hop3)).await;
        let final_guaranteed_lamports: u64 = quote_res_3.get("otherAmountThreshold")
            .or_else(|| quote_res_3.get("outAmount"))
            .and_then(|v| v.as_str())
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);

        // 4. V42 Freshness-Aware Profit Gate & Dynamic Tip (Triangular)
        //    Mirrors the dual-leg policy: strict ≥120k on stale triggers, reflex
        //    marginal-dispatch zone on LaserStream event-fresh triggers.
        let base_gas_lamports: u64 = 24_000; // 5k base + priority floor estimate (3-hop uses more CU)
        let quote_profit_i64 =
            final_guaranteed_lamports as i64 - (input_lamports + base_gas_lamports) as i64;
        let raw_quote_profit = quote_profit_i64.max(0) as u64;

        let confirmed_floor_i64: i64 = if allow_marginal_dispatch {
            -(REFLEX_MARGINAL_TOLERANCE_LAMPORTS)
        } else {
            MIN_CONFIRMED_QUOTE_PROFIT_LAMPORTS as i64
        };
        if quote_profit_i64 < confirmed_floor_i64 {
            bail!("🛑 TRI QUOTE PROFIT GATE: 3-hop Jupiter-confirmed profit {} lamports vs floor {} ({})",
                quote_profit_i64,
                confirmed_floor_i64,
                if allow_marginal_dispatch { "reflex zone exceeded — known loss, skipped" } else { "phantom spread rejected pre-build" });
        }

        let dyn_tip_floor = crate::jito::current_tip_floor_p50();
        let effective_tip = if allow_marginal_dispatch && raw_quote_profit < MIN_CONFIRMED_QUOTE_PROFIT_LAMPORTS {
            ((raw_quote_profit as f64 * 0.60) as u64).clamp(dyn_tip_floor.min(50_000).max(1_000), 1_500_000)
        } else {
            ((raw_quote_profit as f64 * 0.60) as u64).clamp(dyn_tip_floor.min(250_000).max(25_000), 1_500_000)
        };

        let priority_fee_price = (effective_tip / 1000).max(10_000);
        let network_fee_lamports = (priority_fee_price * TRIANGULAR_ROUTE_CU_LIMIT as u64) / 1_000_000 + 10_000;
        let min_required_lamports = input_lamports + effective_tip + network_fee_lamports;

        let net_profit_lamports = final_guaranteed_lamports.saturating_sub(min_required_lamports);
        if !allow_marginal_dispatch && net_profit_lamports < MIN_RETAINED_NET_PROFIT_LAMPORTS {
            bail!("🛑 TRI NET PROFIT GATE: retained net only {} lamports after tip + fees (< {}). Route rejected.",
                net_profit_lamports, MIN_RETAINED_NET_PROFIT_LAMPORTS);
        }

        if allow_marginal_dispatch && raw_quote_profit < MIN_CONFIRMED_QUOTE_PROFIT_LAMPORTS {
            info!("🔥 [TRI REFLEX DISPATCH] {} | event-fresh trigger | quoted profit {} lamports (marginal zone) | firing atomic probe",
                token_a_mint, quote_profit_i64);
        } else {
            info!("🟢 [TRIANGULAR CONFIRMED ROUTE] In: {} -> Guaranteed Out: {} | Tip (60%): {} | Net: +{} lamports",
                input_lamports, final_guaranteed_lamports, effective_tip, net_profit_lamports);
        }

        // 5. Fetch swap instructions for all 3 Hops
        let ix_payload_1 = serde_json::json!({ "quoteResponse": quote_res_1, "userPublicKey": self.payer_pubkey, "wrapAndUnwrapSol": true });
        let ix_payload_2 = serde_json::json!({ "quoteResponse": quote_res_2, "userPublicKey": self.payer_pubkey, "wrapAndUnwrapSol": false });
        let ix_payload_3 = serde_json::json!({ "quoteResponse": quote_res_3, "userPublicKey": self.payer_pubkey, "wrapAndUnwrapSol": true });

        let (r1, r2, r3) = tokio::join!(
            self.http_client.post(JUPITER_SWAP_IX_URL).json(&ix_payload_1).send(),
            self.http_client.post(JUPITER_SWAP_IX_URL).json(&ix_payload_2).send(),
            self.http_client.post(JUPITER_SWAP_IX_URL).json(&ix_payload_3).send()
        );

        let ix_res_1 = r1?.json::<serde_json::Value>().await?;
        let ix_res_2 = r2?.json::<serde_json::Value>().await?;
        let ix_res_3 = r3?.json::<serde_json::Value>().await?;

        // 6. Collect ALTs
        let mut alt_set: HashSet<String> = HashSet::new();
        for res in [&ix_res_1, &ix_res_2, &ix_res_3] {
            if let Some(alts) = res.get("addressLookupTableAddresses").and_then(|v| v.as_array()) {
                for a in alts {
                    if let Some(s) = a.as_str() {
                        alt_set.insert(s.to_string());
                    }
                }
            }
        }

        let loaded_alts = self.fetch_lookup_tables(&alt_set.into_iter().collect::<Vec<String>>()).await;

        // 7. Assemble 3-Hop Instructions with Dual-ATA In-Tx Reclamation
        let mut compiled_instructions: Vec<Instruction> = Vec::new();
        compiled_instructions.push(solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(
            TRIANGULAR_ROUTE_CU_LIMIT,
        ));
        compiled_instructions.push(solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(priority_fee_price));

        for res in [&ix_res_1, &ix_res_2, &ix_res_3] {
            if let Some(swap_ix) = res.get("swapInstruction").and_then(|v| self.parse_instruction_json(v)) {
                compiled_instructions.push(swap_ix);
            }
        }

        // Dual Ephemeral ATA close instructions (Token A + USDC)
        let payer_pubkey = self.keypair.pubkey();
        if let Ok(token_a_pub) = Pubkey::from_str(token_a_mint) {
            let ata_a = get_associated_token_address(&payer_pubkey, &token_a_pub);
            if let Ok(close_ix) = spl_token::instruction::close_account(&spl_token::id(), &ata_a, &payer_pubkey, &payer_pubkey, &[&payer_pubkey]) {
                compiled_instructions.push(close_ix);
            }
        }
        if let Ok(usdc_pub) = Pubkey::from_str(bridge_mint) {
            let ata_usdc = get_associated_token_address(&payer_pubkey, &usdc_pub);
            if let Ok(close_ix) = spl_token::instruction::close_account(&spl_token::id(), &ata_usdc, &payer_pubkey, &payer_pubkey, &[&payer_pubkey]) {
                compiled_instructions.push(close_ix);
            }
        }

        // Jito Tip Transfer (uses recalculated tip from actual Jupiter 3-hop quote profit)
        compiled_instructions.push(solana_sdk::system_instruction::transfer(
            &payer_pubkey,
            &tip_account,
            effective_tip,
        ));

        let v0_msg = v0::Message::try_compile(
            &payer_pubkey,
            &compiled_instructions,
            &loaded_alts,
            recent_blockhash,
        ).context("Failed to compile 3-hop v0 message")?;

        let v0_tx = VersionedTransaction::try_new(
            VersionedMessage::V0(v0_msg),
            &[&self.keypair],
        ).context("Failed to sign 3-hop versioned transaction")?;

        let raw_tx_bytes = bincode::serialize(&v0_tx)?;
        let signed_tx_base64 = BASE64_STANDARD.encode(&raw_tx_bytes);
        let signed_tx_base58 = bs58::encode(&raw_tx_bytes).into_string();
        let sig = v0_tx.signatures[0].to_string();

        Ok(SignedCircularPackage {
            tx_signature_base58: sig,
            signed_tx_base64,
            signed_tx_base58,
            input_lamports,
            final_lamports: final_guaranteed_lamports,
            net_profit_lamports,
            tip_lamports: effective_tip,
            uuid,
        })
    }

    pub fn parse_instruction_json(&self, val: &serde_json::Value) -> Option<Instruction> {
        let program_id_str = val.get("programId")?.as_str()?;
        let program_id = Pubkey::from_str(program_id_str).ok()?;

        let accounts_arr = val.get("accounts")?.as_array()?;
        let mut accounts = Vec::with_capacity(accounts_arr.len());
        for acc in accounts_arr {
            let pubkey_str = acc.get("pubkey")?.as_str()?;
            let pubkey = Pubkey::from_str(pubkey_str).ok()?;
            let is_signer = acc.get("isSigner")?.as_bool()?;
            let is_writable = acc.get("isWritable")?.as_bool()?;

            if is_writable {
                accounts.push(AccountMeta::new(pubkey, is_signer));
            } else {
                accounts.push(AccountMeta::new_readonly(pubkey, is_signer));
            }
        }

        let data_b64 = val.get("data")?.as_str()?;
        let data = BASE64_STANDARD.decode(data_b64).ok()?;

        Some(Instruction { program_id, accounts, data })
    }

    pub async fn fetch_lookup_tables(&self, alt_addresses: &[String]) -> Vec<AddressLookupTableAccount> {
        let mut loaded = Vec::new();
        if alt_addresses.is_empty() {
            return loaded;
        }

        let post_data: Vec<serde_json::Value> = alt_addresses.iter().enumerate().map(|(id, addr)| {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "getAccountInfo",
                "params": [addr, { "encoding": "base64" }]
            })
        }).collect();

        if let Ok(resp) = self.http_client.post(&self.rpc_url).json(&post_data).send().await {
            if let Ok(arr) = resp.json::<Vec<serde_json::Value>>().await {
                for (i, res_obj) in arr.iter().enumerate() {
                    if let Some(val) = res_obj.get("result").and_then(|r| r.get("value")) {
                        if let Some(data_arr) = val.get("data").and_then(|d| d.as_array()) {
                            if let Some(b64_str) = data_arr.first().and_then(|s| s.as_str()) {
                                if let Ok(bytes) = BASE64_STANDARD.decode(b64_str) {
                                    if let Ok(alt_state) = AddressLookupTable::deserialize(&bytes) {
                                        if let Ok(alt_key) = Pubkey::from_str(&alt_addresses[i]) {
                                            loaded.push(AddressLookupTableAccount {
                                                key: alt_key,
                                                addresses: alt_state.addresses.to_vec(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        loaded
    }
}
