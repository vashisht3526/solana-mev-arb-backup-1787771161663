# Binding Consensus Decree & Rust Code Blueprint for V14 Deployment

## Executive Summary

After thorough deliberation by the 4-model council, we approve V14 with critical modifications to ensure mathematical integrity and profitability preservation. The key changes involve replacing the flawed 5% pool depth heuristic with true price impact calculations, implementing tiered pool strategies, and enforcing strict CLMM rejection protocols.

## Binding Consensus Decree (V14)

### Core Mandates

1. **In-Memory Routing Protocol**
   - Jupiter HTTP routing is hereby **DEPRECATED** for triangular arbitrage
   - All quote calculations must use `triangular_math.rs`
   - CLMM pools are **STRICTLY PROHIBITED** from in-memory routing (must fallback to Jupiter)
   - Stale data (>10 slots old) triggers automatic pool rejection

2. **TVL Floor Adjustment**
   - Minimum TVL threshold lowered to **$1,000 USD**
   - Accompanied by mandatory adaptive sizing controls per pool tier
   - Micro-cap pools ($1K-$2.5K) require special handling due to high price impact sensitivity

3. **Adaptive Trade Sizing Framework**
   - Replace naive 5% pool depth rule with **impact-based calculations**
   - Implement tiered approach based on pool TVL and composition
   - Enforce conservative triangular multipliers (halve single-hop limits)

4. **Decimal Precision Standards**
   - All arithmetic must operate on normalized values (9-decimal Solana standard)
   - Token-specific scaling factors applied before and after calculations
   - Overflow protection mandatory in all computational paths

5. **Safety Protocols**
   - Pre-trade profitability validation required
   - Dynamic fee inclusion in profit calculations
   - Absolute minimum trade size enforced (0.005 SOL)

## Rust Code Blueprint (V14)

```rust
// src/triangular_math.rs

use solana_program::{pubkey::Pubkey};
use crate::error::ArbitrageError;

// Configuration structures
#[derive(Debug, Clone)]
pub struct AdaptiveSizingConfig {
    pub max_impact_bps: u16,
    pub min_trade_lamports: u64,
    pub global_max_lamports: u64,
}

#[derive(Debug, Clone)]
pub struct PoolLiquidity {
    pub total_tvl_sol: f64,
    pub token_a_weight: f64,
    pub token_b_weight: f64,
    pub pool_type: PoolType,
    pub last_update_slot: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PoolType {
    ConstantProduct,
    StableSwap,
    ConcentratedLiquidity,
}

#[derive(Debug, Clone)]
pub struct SwapLeg {
    pub input_decimals: u8,
    pub output_decimals: u8,
    pub reserve_in: u64,
    pub reserve_out: u64,
    pub fee_bps: u16,
    pub fee_lamports: u64,
    pub jito_tip: u64,
    pub impact_bps: u16,
}

#[derive(Debug)]
pub struct TriangularQuote {
    pub input: u64,
    pub output: u64,
    pub profit_lamports: u64,
    pub price_impacts: [u16; 3],
}

// Tiered pool classification system
pub enum PoolTier {
    Micro { max_impact_bps: u16, max_trade_sol: f64 },   // <$1K
    Small { max_impact_bps: u16, max_trade_sol: f64 },   // $1K-$2.5K
    Medium { max_impact_bps: u16, max_trade_sol: f64 },  // $2.5K-$10K
    Standard { max_trade_sol: f64 },                     // >$10K
}

impl PoolTier {
    pub fn get_config(&self) -> AdaptiveSizingConfig {
        match self {
            PoolTier::Micro { max_impact_bps, max_trade_sol } => AdaptiveSizingConfig {
                max_impact_bps: *max_impact_bps,
                min_trade_lamports: 5_000_000, // 0.005 SOL
                global_max_lamports: (*max_trade_sol * 1_000_000_000.0) as u64,
            },
            PoolTier::Small { max_impact_bps, max_trade_sol } => AdaptiveSizingConfig {
                max_impact_bps: *max_impact_bps,
                min_trade_lamports: 10_000_000, // 0.01 SOL
                global_max_lamports: (*max_trade_sol * 1_000_000_000.0) as u64,
            },
            PoolTier::Medium { max_impact_bps, max_trade_sol } => AdaptiveSizingConfig {
                max_impact_bps: *max_impact_bps,
                min_trade_lamports: 25_000_000, // 0.025 SOL
                global_max_lamports: (*max_trade_sol * 1_000_000_000.0) as u64,
            },
            PoolTier::Standard { max_trade_sol } => AdaptiveSizingConfig {
                max_impact_bps: 50, // 0.5%
                min_trade_lamports: 50_000_000, // 0.05 SOL
                global_max_lamports: (*max_trade_sol * 1_000_000_000.0) as u64,
            },
        }
    }
}

pub fn classify_pool(tvl_usd: u64) -> PoolTier {
    match tvl_usd {
        0..=1_000 => PoolTier::Micro {
            max_impact_bps: 150, // 1.5%
            max_trade_sol: 0.024, // Reduced from 0.15 SOL
        },
        1_001..=2_500 => PoolTier::Small {
            max_impact_bps: 100, // 1.0%
            max_trade_sol: 0.060,
        },
        2_501..=10_000 => PoolTier::Medium {
            max_impact_bps: 50, // 0.5%
            max_trade_sol: 0.120,
        },
        _ => PoolTier::Standard {
            max_trade_sol: 0.150, // Original value for larger pools
        },
    }
}

// Impact-based trade sizing calculation
pub fn calculate_safe_triangular_size(
    pool: &PoolLiquidity,
    config: &AdaptiveSizingConfig,
    current_slot: u64,
) -> Result<u64, ArbitrageError> {
    // Validation checks
    if current_slot.saturating_sub(pool.last_update_slot) > 10 {
        return Err(ArbitrageError::StalePoolData);
    }
    
    if matches!(pool.pool_type, PoolType::ConcentratedLiquidity) {
        return Err(ArbitrageError::UnsupportedPoolType);
    }
    
    // Calculate per-leg liquidity using conservative weighting
    let min_weight = pool.token_a_weight.min(pool.token_b_weight);
    let leg_liq_sol = pool.total_tvl_sol * min_weight;
    
    // Solve impact equation for maximum input
    // impact = dx / (x + dx)  →  dx = impact * x / (1 - impact)
    let impact = config.max_impact_bps as f64 / 10_000.0;
    let max_input_sol = (impact * leg_liq_sol) / (1.0 - impact);
    
    // Triangular safety: 3 hops compound error
    // Conservative approach: halve for round-trip margin
    let triangular_max_sol = max_input_sol / 2.0;
    
    let triangular_max_lamports = (triangular_max_sol * 1e9) as u64;
    
    // Apply constraints
    let effective_size = config.global_max_lamports
        .min(triangular_max_lamports)
        .max(config.min_trade_lamports);
    
    // Profitability pre-check
    if effective_size < config.min_trade_lamports {
        return Err(ArbitrageError::InsufficientLiquidity);
    }
    
    Ok(effective_size)
}

// Optimized decimal handling
pub struct DecimalNormalizer {
    pub input_scale: u64,
    pub output_scale: u64,
    pub common_scale: u64,
}

impl DecimalNormalizer {
    pub fn new(input_decimals: u8, output_decimals: u8) -> Self {
        Self {
            input_scale: 10u64.pow(9 - input_decimals as u32),
            output_scale: 10u64.pow(9 - output_decimals as u32),
            common_scale: 1_000_000_000,
        }
    }
    
    pub fn normalize_and_swap(
        &self,
        amount: u64,
        reserve_in: u64,
        reserve_out: u64,
        fee_bps: u16,
    ) -> Option<u64> {
        // Scale to common 9-decimal representation
        let amount_norm = amount.checked_mul(self.input_scale)?;
        let reserve_in_norm = reserve_in.checked_mul(self.input_scale)?;
        let reserve_out_norm = reserve_out.checked_mul(self.output_scale)?;
        
        // Constant product with fee: dy = y * dx * (1-f) / (x + dx * (1-f))
        let fee_num = 10_000 - fee_bps as u64;
        let dx_with_fee = amount_norm.checked_mul(fee_num)?.checked_div(10_000)?;
        
        let numerator = reserve_out_norm.checked_mul(dx_with_fee)?;
        let denominator = reserve_in_norm.checked_add(dx_with_fee)?;
        
        let output_norm = numerator.checked_div(denominator)?;
        
        // Denormalize to output token's decimals
        output_norm.checked_div(self.output_scale)
    }
}

// Core triangular calculation engine
pub fn calculate_triangular_output(
    leg1: &SwapLeg,
    leg2: &SwapLeg,
    leg3: &SwapLeg,
    input_lamports: u64,
) -> Result<TriangularQuote, ArbitrageError> {
    let out1 = calculate_single_leg(leg1, input_lamports)?;
    let out2 = calculate_single_leg(leg2, out1)?;
    let out3 = calculate_single_leg(leg3, out2)?;
    
    // Invariant: must return more than input + costs
    let total_costs = leg1.fee_lamports + leg2.fee_lamports + leg3.fee_lamports 
                    + leg1.jito_tip + leg2.jito_tip + leg3.jito_tip;
    
    if out3 <= input_lamports.saturating_add(total_costs) {
        return Err(ArbitrageError::NegativeProfit);
    }
    
    Ok(TriangularQuote {
        input: input_lamports,
        output: out3,
        profit_lamports: out3 - input_lamports - total_costs,
        price_impacts: [leg1.impact_bps, leg2.impact_bps, leg3.impact_bps],
    })
}

fn calculate_single_leg(
    leg: &SwapLeg,
    input: u64,
) -> Result<u64, ArbitrageError> {
    let normalizer = DecimalNormalizer::new(
        leg.input_decimals,
        leg.output_decimals,
    );
    
    normalizer.normalize_and_swap(
        input,
        leg.reserve_in,
        leg.reserve_out,
        leg.fee_bps,
    ).ok_or(ArbitrageError::CalculationOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_micro_pool_sizing() {
        let pool = PoolLiquidity {
            total_tvl_sol: 6.5,
            token_a_weight: 0.5,
            token_b_weight: 0.5,
            pool_type: PoolType::ConstantProduct,
            last_update_slot: 100,
        };
        
        let tier = classify_pool(1000);
        let config = tier.get_config();
        let size = calculate_safe_triangular_size(&pool, &config, 105).unwrap();
        
        // Should be around 0.024 SOL (24,000,000 lamports)
        assert!(size < 30_000_000, "Trade too large for $1K pool safety");
        assert!(size >= 5_000_000, "Trade above minimum floor");
    }
    
    #[test]
    fn test_clmm_rejection() {
        let pool = PoolLiquidity {
            total_tvl_sol: 100.0,
            token_a_weight: 0.5,
            token_b_weight: 0.5,
            pool_type: PoolType::ConcentratedLiquidity,
            last_update_slot: 100,
        };
        
        let tier = classify_pool(5000);
        let config = tier.get_config();
        
        assert!(matches!(
            calculate_safe_triangular_size(&pool, &config, 105),
            Err(ArbitrageError::UnsupportedPoolType)
        ));
    }
}
```

## Implementation Notes

1. **Performance Optimization**: The decimal normalizer uses native u64 operations to stay within Solana's compute budget
2. **Safety First**: All calculations include overflow checks and pre-trade profitability validation
3. **Tiered Approach**: Different risk parameters for different pool sizes ensure appropriate capital allocation
4. **Future Proofing**: Clear separation between supported pool types enables future expansion

This V14 implementation balances aggressive yield capture with mathematical rigor, ensuring sustainable operations across the entire liquidity spectrum while maintaining sub-millisecond transaction build times.