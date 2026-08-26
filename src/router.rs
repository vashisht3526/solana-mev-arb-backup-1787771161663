#![allow(dead_code)]
use crate::types::{ArbitrageOpportunity, ComputeBudgetConfig, EphemeralAtaLifecycle};
use serde::{Deserialize, Serialize};

pub const ATOMIC_ROUTER_PROGRAM_ID: &str = "AN225ksocfZpbPVEbwffiANbtUdYbySM3WJ6dbsYAqij";
pub const ORCA_WHIRLPOOL_PROGRAM_ID: &str = "DF1ow4tspfHX9JwWJsAb9epbkA8hmpSEAtxXy1V27QBH";
pub const METEORA_DLMM_PROGRAM_ID: &str = "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo";
pub const RAYDIUM_CPMM_PROGRAM_ID: &str = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C";
pub const RAYDIUM_CLMM_PROGRAM_ID: &str = "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc";
pub const PUMP_FUN_PROGRAM_ID: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";
pub const SPL_ASSOCIATED_TOKEN_PROGRAM_ID: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
pub const SPL_TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomicSwapInstruction {
    pub program_id: String,
    pub accounts: Vec<String>,
    pub data_hex: String,
    pub leg: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomicBundlePlan {
    pub compute_budget: ComputeBudgetConfig,
    pub ephemeral_ata: EphemeralAtaLifecycle,
    pub instructions: Vec<String>,
    pub expected_profit_sol: f64,
    pub jito_tip_lamports: u64,
}

pub struct AtomicRouterEngine;

impl AtomicRouterEngine {
    /// Selects optimal Compute Budget dynamically based on pool types and Ephemeral ATA overhead
    pub fn select_compute_budget(dex_a: &str, dex_b: &str) -> ComputeBudgetConfig {
        let is_complex = dex_a.contains("METEORA")
            || dex_b.contains("METEORA")
            || dex_a.contains("ORCA")
            || dex_b.contains("ORCA")
            || dex_a.contains("CLMM")
            || dex_b.contains("CLMM");

        if is_complex {
            ComputeBudgetConfig::EPHEMERAL_COMPLEX
        } else {
            ComputeBudgetConfig::EPHEMERAL_STANDARD
        }
    }

    /// Assembles an optimized Atomic Bundle Plan for an identified circular arbitrage opportunity
    pub fn build_atomic_bundle(
        opp: &ArbitrageOpportunity,
        wallet_pubkey: &str,
    ) -> AtomicBundlePlan {
        let compute_budget = Self::select_compute_budget(&opp.target_dex, "PUMPSWAP");
        let ephemeral_ata = EphemeralAtaLifecycle::new(&opp.mint, wallet_pubkey);

        let dynamic_tip_lamports = ((opp.net_profit_sol * 1_000_000_000.0) * 0.25)
            .max(100_000.0)
            .min(10_000_000.0) as u64;

        AtomicBundlePlan {
            compute_budget,
            ephemeral_ata,
            instructions: vec![
                format!("SET_CU_LIMIT_{}", compute_budget.cu_limit),
                format!("SET_CU_PRICE_{}", compute_budget.cu_price_micro_lamports),
                format!("CREATE_EPHEMERAL_ATA_{}", opp.mint),
                format!("SWAP_LEG1_{}_IN_OUT", opp.target_dex),
                format!("SWAP_LEG2_PUMPSWAP_IN_OUT"),
                format!("JITO_TIP_TRANSFER_{}", dynamic_tip_lamports),
                format!("CLOSE_EPHEMERAL_ATA_{}_RECLAIM_RENT", opp.mint),
            ],
            expected_profit_sol: opp.net_profit_sol,
            jito_tip_lamports: dynamic_tip_lamports,
        }
    }
}
