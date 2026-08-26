#![allow(dead_code)]
use serde::{Deserialize, Serialize};

/// Lightweight in-memory representation of a pool's reserve state for sub-microsecond math
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolReserveState {
    pub reserve_in: u64,
    pub reserve_out: u64,
    pub fee_bps: u64, // e.g. 25 for 0.25%, 30 for 0.30%
}

impl PoolReserveState {
    pub fn new(reserve_in: u64, reserve_out: u64, fee_bps: u64) -> Self {
        Self {
            reserve_in,
            reserve_out,
            fee_bps,
        }
    }

    /// Exact Constant Product Output Math using u128 intermediate precision to prevent overflow
    #[inline(always)]
    pub fn get_amount_out(&self, amount_in: u64) -> Option<u64> {
        if self.reserve_in == 0 || self.reserve_out == 0 || amount_in == 0 {
            return None;
        }

        let fee_multiplier = 10_000u128.saturating_sub(self.fee_bps as u128);
        let amount_in_with_fee = (amount_in as u128) * fee_multiplier;
        let numerator = amount_in_with_fee * (self.reserve_out as u128);
        let denominator = ((self.reserve_in as u128) * 10_000u128) + amount_in_with_fee;

        if denominator == 0 {
            return None;
        }

        Some((numerator / denominator) as u64)
    }

    /// Calculate price impact in basis points (1 bp = 0.01%)
    #[inline(always)]
    pub fn calculate_price_impact_bps(&self, amount_in: u64) -> u64 {
        if self.reserve_in == 0 {
            return 10_000;
        }
        let impact = ((amount_in as u128) * 10_000u128) / ((self.reserve_in as u128) + (amount_in as u128));
        impact as u64
    }

    /// Calculates safe input size capped by pool reserve to guarantee price impact <= max_impact_bps
    #[inline(always)]
    pub fn calculate_safe_input_size(&self, desired_size_lamports: u64, max_impact_bps: u64) -> u64 {
        if self.reserve_in == 0 {
            return 0;
        }
        // max_input = (reserve_in * max_impact_bps) / 10000
        let max_safe_input = ((self.reserve_in as u128 * max_impact_bps as u128) / 10_000u128) as u64;
        desired_size_lamports.min(max_safe_input)
    }
}

/// Represents a single hop in a 3-leg triangular route
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriangularHop {
    pub pool_address: String,
    pub dex_type: String,
    pub input_mint: String,
    pub output_mint: String,
    pub reserve_state: PoolReserveState,
}

/// Evaluates a 3-hop triangular route in memory (< 1 microsecond)
/// Returns (Final SOL Out, Gross Profit Lamports, Spread Pct, Safe Input Lamports)
pub fn evaluate_triangular_route_safe(
    target_sol_in_lamports: u64,
    leg1: &PoolReserveState, // SOL -> TokenA
    leg2: &PoolReserveState, // TokenA -> TokenB (or USDC)
    leg3: &PoolReserveState, // TokenB (or USDC) -> SOL
) -> Option<(u64, i64, f64, u64)> {
    // Dynamic liquidity safety clamp: Ensure Leg 1 price impact is <= 0.35% (35 bps)
    let safe_sol_in = leg1.calculate_safe_input_size(target_sol_in_lamports, 35);
    if safe_sol_in == 0 {
        return None;
    }

    let token_a_out = leg1.get_amount_out(safe_sol_in)?;
    let token_b_out = leg2.get_amount_out(token_a_out)?;
    let final_sol_out = leg3.get_amount_out(token_b_out)?;

    let gross_profit = (final_sol_out as i64) - (safe_sol_in as i64);
    let spread_pct = ((final_sol_out as f64 - safe_sol_in as f64) / (safe_sol_in as f64)) * 100.0;

    Some((final_sol_out, gross_profit, spread_pct, safe_sol_in))
}

/// Legacy evaluation wrapper for backward compatibility
pub fn evaluate_triangular_route(
    sol_in_lamports: u64,
    leg1: &PoolReserveState, // SOL -> TokenA
    leg2: &PoolReserveState, // TokenA -> TokenB (or USDC)
    leg3: &PoolReserveState, // TokenB (or USDC) -> SOL
) -> Option<(u64, i64, f64)> {
    let (sol_out, gross_profit, spread_pct, _) = evaluate_triangular_route_safe(sol_in_lamports, leg1, leg2, leg3)?;
    Some((sol_out, gross_profit, spread_pct))
}

/// Calculates dynamic Jito tip as a percentage of gross profit margin
/// Returns tip in lamports
pub fn calculate_dynamic_jito_tip(gross_profit_lamports: u64) -> u64 {
    const MIN_TIP_FLOOR: u64 = 10_000; // 0.000010 SOL minimum floor
    const MAX_TIP_CAP_SHARE: f64 = 0.50; // Max 50% of profit to tip
    const TARGET_PROFIT_SHARE: f64 = 0.35; // Target 35% margin share to win top-of-block placement

    if gross_profit_lamports <= MIN_TIP_FLOOR {
        return MIN_TIP_FLOOR;
    }

    let calculated_tip = (gross_profit_lamports as f64 * TARGET_PROFIT_SHARE) as u64;
    let max_tip = (gross_profit_lamports as f64 * MAX_TIP_CAP_SHARE) as u64;

    calculated_tip.clamp(MIN_TIP_FLOOR, max_tip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_pool_swap_math() {
        // Pool with 100 SOL and 1,000,000 Token, 0.25% fee (25 bps)
        let pool = PoolReserveState::new(100_000_000_000, 1_000_000_000_000, 25);
        let amount_in = 1_000_000_000; // 1 SOL
        let amount_out = pool.get_amount_out(amount_in).expect("should compute output");
        
        assert!(amount_out > 0);
        assert!(amount_out < 10_000_000_000); // Less than 10k tokens due to slippage and fee
    }

    #[test]
    fn test_safe_input_size_clamping() {
        // Small pool with 5 SOL reserve (5,000,000,000 lamports)
        let pool = PoolReserveState::new(5_000_000_000, 500_000_000_000, 25);
        
        // Desired size 0.05 SOL (50,000,000 lamports)
        // With 35 bps (0.35%) cap: (5,000,000,000 * 35) / 10000 = 17,500,000 lamports (0.0175 SOL)
        let safe_size = pool.calculate_safe_input_size(50_000_000, 35);
        assert_eq!(safe_size, 17_500_000);
        assert!(pool.calculate_price_impact_bps(safe_size) <= 35);
    }

    #[test]
    fn test_triangular_arbitrage_profitable_loop() {
        // Leg 1: SOL -> Token (Cheap Token on Raydium)
        let leg1 = PoolReserveState::new(50_000_000_000, 1_000_000_000_000, 25);
        
        // Leg 2: Token -> USDC (Expensive Token on Meteora)
        let leg2 = PoolReserveState::new(1_000_000_000_000, 110_000_000_000, 25); // 10% premium in USDC
        
        // Leg 3: USDC -> SOL (Deep USDC/SOL pair on Orca)
        let leg3 = PoolReserveState::new(100_000_000_000, 50_000_000_000, 25); // 1 SOL = 2 USDC benchmark

        let sol_in = 18_000_000; // 0.018 SOL
        let (sol_out, profit, spread) = evaluate_triangular_route(sol_in, &leg1, &leg2, &leg3)
            .expect("should evaluate triangular route");

        assert!(sol_out > sol_in, "should be profitable");
        assert!(profit > 0);
        assert!(spread > 0.0);
        
        let tip = calculate_dynamic_jito_tip(profit as u64);
        assert!(tip >= 10_000);
        assert!(tip < (profit as u64));
    }
}
