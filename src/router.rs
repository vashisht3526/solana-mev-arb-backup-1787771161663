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
    pub min_required_out_sol: f64,
    pub jito_tip_sol: f64,
}

pub struct AtomicRouterEngine;

impl AtomicRouterEngine {
    /// Selects optimal Compute Budget dynamically based on pool types and Ephemeral ATA overhead
    pub fn select_compute_budget(dex_a: &str, dex_b: &str) -> ComputeBudgetConfig {
        let is_dlmm_or_clmm = dex_a.contains("METEORA")
            || dex_a.contains("CLMM")
            || dex_a.contains("ORCA")
            || dex_b.contains("METEORA")
            || dex_b.contains("CLMM")
            || dex_b.contains("ORCA");

        if is_dlmm_or_clmm {
            // 180k CUs ensures DLMM multi-bin traversals + ATA Create + ATA Close never abort
            ComputeBudgetConfig::EPHEMERAL_COMPLEX
        } else {
            // 125k CUs for standard CPMM + ATA Create + ATA Close
            ComputeBudgetConfig::EPHEMERAL_STANDARD
        }
    }

    /// Assembles full 6-step Same-Transaction Ephemeral ATA Atomic Bundle
    pub fn build_atomic_bundle(
        opp: &ArbitrageOpportunity,
        wallet_pubkey: &str,
        ephemeral_ata_address: &str,
    ) -> AtomicBundlePlan {
        let compute_budget = Self::select_compute_budget(&opp.target_dex, &opp.target_dex);

        let lifecycle = EphemeralAtaLifecycle::new(&opp.mint, ephemeral_ata_address);

        // 6-step atomic lifecycle sequence:
        // 1. SetComputeUnitLimit(180,000 / 125,000)
        // 2. SetComputeUnitPrice(5,000 micro-lamports)
        // 3. create_associated_token_account_idempotent (Borrows 0.00204 SOL rent for 1ms)
        // 4. Swap Leg 1: SOL -> Token into Ephemeral ATA
        // 5. Swap Leg 2: Token in Ephemeral ATA -> SOL back to Wallet
        // 6. close_account: Ephemeral ATA -> 0.00204 SOL rent refund directly to Wallet
        // 7. Atomic Profit Check: require!(final_sol >= input_sol + min_net_profit)
        let instructions = vec![
            format!("SetComputeUnitLimit({})", compute_budget.cu_limit),
            format!("SetComputeUnitPrice({})", compute_budget.cu_price_micro_lamports),
            format!("CreateIdempotentATA(payer={}, owner={}, mint={})", wallet_pubkey, wallet_pubkey, opp.mint),
            format!("SwapLeg1(Dex={}, In={:.4} SOL, Out=Token)", "PUMPSWAP", opp.simulated_swap_sol),
            format!("SwapLeg2(Dex={}, In=Token, Out=SOL)", opp.target_dex),
            format!("CloseAccount(ATA={}, RefundDestination={})", ephemeral_ata_address, wallet_pubkey),
            format!("RequireProfit(MinOut={:.5} SOL)", opp.simulated_swap_sol + opp.net_profit_sol * 0.95),
        ];

        // 25% Jito tip clamping [0.00005, 0.01000 SOL]
        let dynamic_tip = (opp.net_profit_sol * 0.25).clamp(0.00005, 0.01000);

        AtomicBundlePlan {
            compute_budget,
            ephemeral_ata: lifecycle,
            instructions,
            min_required_out_sol: opp.simulated_swap_sol + (opp.net_profit_sol * 0.95),
            jito_tip_sol: dynamic_tip,
        }
    }
}
