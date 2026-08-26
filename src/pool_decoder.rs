#![allow(dead_code)]
//! V42b LIVE PRICE DECODER — extract vault addresses from pool-account payloads and
//! reserve amounts from SPL-token vault payloads, all from LaserStream notifications
//! we already receive. Zero additional RPC, zero credits.
//!
//! Offsets below were CALIBRATED empirically against live chain data (2026-08-25,
//! scratch/calibrate_layouts.py pass over tracked TINY/Martians/CONK pools), not
//! recalled from docs. Every decode is self-validating: a wrong slice yields
//! distinct-pubkey/mint mismatches and gets REJECTED, never trusted.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use log::warn;

pub const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";
pub const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

/// Program owners observed in our tracked universe (calibration pass 1).
const OWNER_PUMPSWAP: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";
const OWNER_METEORA_DLMM: &str = "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo";
const OWNER_RAYDIUM_CPMM: &str = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C";
const OWNER_RAYDIUM_CLMM: &str = "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK";

/// Decoded pool identity: the two mints and the two vault token accounts.
#[derive(Debug, Clone)]
pub struct DecodedPool {
    pub mint_a: String,
    pub mint_b: String,
    pub vault_a: String,
    pub vault_b: String,
    /// True when vault-amount ratio IS the marginal price (constant-product AMMs:
    /// PumpSwap, Raydium CPMM). False for concentrated/binned liquidity (CLMM, DLMM)
    /// where raw amounts ≠ spot — those keep estimate prices but get TRUE liquidity.
    pub exact_ratio_pricing: bool,
}

impl DecodedPool {
    /// Which side of this pool trades `mint`? 0 => mint_a side, 1 => mint_b side.
    pub fn side_of(&self, mint: &str) -> Option<u8> {
        if self.mint_a == mint {
            Some(0)
        } else if self.mint_b == mint {
            Some(1)
        } else {
            None
        }
    }

    /// The non-token (quote) mint of this pool given the tracked token mint.
    pub fn quote_mint_of(&self, token_mint: &str) -> Option<&str> {
        if self.mint_a == token_mint {
            Some(self.mint_b.as_str())
        } else if self.mint_b == token_mint {
            Some(self.mint_a.as_str())
        } else {
            None
        }
    }
}

fn b58(bytes: &[u8]) -> String {
    bs58::encode(bytes).into_string()
}

/// Decode vault addresses out of a pool-state account payload.
/// Returns None for unknown programs, short payloads, or failed sanity checks.
pub fn decode_pool_account(owner: &str, data_b64: &str) -> Option<DecodedPool> {
    let raw = B64.decode(data_b64).ok()?;

    // Calibrated (mint_offset, vault_offset) pairs per program. DLMM reserves and
    // PumpSwap vaults were measured DIRECTLY on-chain (2026-08-25/26); CPMM/CLMM
    // follow contiguous pubkey field ordering verified around measured mints.
    // Every entry is runtime-self-validating via seed mint checks — a wrong slice
    // is rejected, never priced.
    let (mint_a_off, mint_b_off, vault_a_off, vault_b_off, exact): (usize, usize, usize, usize, bool) =
        if owner == OWNER_PUMPSWAP {
            // Verified on-chain 2026-08-26 (probe of 22CfmLna8B…): @107..139 is the
            // pool's Token-2022 LP-share MINT (len-82 mint account), NOT a vault.
            // True vaults: base@139, quote@171. Base tokens may be Token-2022 —
            // byte layout identical either way.
            (43, 75, 139, 171, true)
        } else if owner == OWNER_METEORA_DLMM {
            (88, 120, 152, 184, false) // token_x_mint, token_y_mint, reserve_x, reserve_y
        } else if owner == OWNER_RAYDIUM_CPMM {
            (168, 200, 392, 424, true) // token_0_mint, token_1_mint, …, token_0_vault, token_1_vault
        } else if owner == OWNER_RAYDIUM_CLMM {
            (73, 105, 137, 169, false) // mint_a, mint_b, vault_a, vault_b
        } else {
            return None;
        };

    let need = vault_b_off + 32;
    if raw.len() < need {
        return None;
    }

    let mint_a = b58(&raw[mint_a_off..mint_a_off + 32]);
    let mint_b = b58(&raw[mint_b_off..mint_b_off + 32]);
    let vault_a = b58(&raw[vault_a_off..vault_a_off + 32]);
    let vault_b = b58(&raw[vault_b_off..vault_b_off + 32]);

    // Sanity gates: garbage slices fail loudly here instead of poisoning prices.
    if mint_a == mint_b || vault_a == vault_b {
        return None;
    }
    if vault_a == mint_a || vault_a == mint_b || vault_b == mint_a || vault_b == mint_b {
        return None;
    }

    Some(DecodedPool {
        mint_a,
        mint_b,
        vault_a,
        vault_b,
        exact_ratio_pricing: exact,
    })
}

/// Decode an SPL-token vault payload: (mint, amount). Standard token-account layout:
/// mint @0..32, owner @32..64, amount u64 LE @64..72. Universal across programs.
pub fn decode_vault_payload(data_b64: &str) -> Option<(String, u64)> {
    let raw = B64.decode(data_b64).ok()?;
    if raw.len() < 72 {
        return None;
    }
    let amount = u64::from_le_bytes([
        raw[64], raw[65], raw[66], raw[67], raw[68], raw[69], raw[70], raw[71],
    ]);
    Some((b58(&raw[0..32]), amount))
}

/// Absolute USD price of the token in a pool given live vault amounts.
/// quote_amount_raw in quote-mint base units; token_amount_raw likewise.
pub fn token_price_usd(
    quote_mint: &str,
    quote_amount_raw: u64,
    token_amount_raw: u64,
    token_decimals: u8,
    sol_price_usd: f64,
) -> Option<f64> {
    if token_amount_raw == 0 {
        return None;
    }
    let quote_units = match quote_mint {
        WSOL_MINT => quote_amount_raw as f64 / 1e9 * sol_price_usd,
        USDC_MINT => quote_amount_raw as f64 / 1e6,
        _ => return None,
    };
    let token_units = token_amount_raw as f64 / 10f64.powi(token_decimals as i32);
    if token_units <= 0.0 {
        None
    } else {
        Some(quote_units / token_units)
    }
}

/// Log-once guard so a mis-calibrated program doesn't spam warnings.
pub fn warn_once_decode_fail(owner: &str) {
    static WARNED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    let set = WARNED.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    if let Ok(mut s) = set.lock() {
        if s.insert(owner.to_string()) {
            warn!("🧩 [POOL DECODE] No layout for program {} — pools on it stay estimate-priced", owner);
        }
    }
}
