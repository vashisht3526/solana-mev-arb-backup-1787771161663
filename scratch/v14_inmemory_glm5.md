================================================================================
**COUNCIL AUDIT REPORT: V14 IN-MEMORY ROUTING & PRECISION SAFETY**
================================================================================

**Council Member:** GLM-5 (Systems Architecture & Mathematical Integrity)
**Status:** **CONDITIONAL APPROVAL — CRITICAL PRECISION PATCH REQUIRED**

---

### I. IN-MEMORY TRIANGULAR MATH AUDIT

**1. Constant Product Safety ($xy=k$)**
The migration to local in-memory calculation eliminates HTTP latency (80-150ms) and API dependency risks. The implementation of standard AMM logic in `src/triangular_math.rs` is sound for V1 pools.
*   **Finding:** The math logic is theoretically correct for `x * y = k`.
*   **Risk:** Curve fitting for Concentated Liquidity (CLMM) ranges. If `triangular_math.rs` assumes flat `xy=k` for CLMM pools, it will misprice assets within specific tick ranges.
*   **Mitigation:** Ensure `triangular_math.rs` strictly rejects or safely approximates CLMM pools. Do not route CLMM liquidity through flat constant-product logic without tick index simulation.

**2. Instruction Assembly & Atomicity**
*   **Finding:** Removing Jupiter API calls reduces the attack surface for quote manipulation (stale quotes).
*   **Verification:** The transaction builder must now manually handle account resolution (mint, vault, authority). Jupiter handles decimal conversion internally; your local code must now explicitly handle it.
*   **Verdict:** Safe, provided the account loading logic matches the on-chain program IDs exactly.

---

### II. DECIMAL PRECISION HANDLING (CRITICAL)

**The Jupiter Blind Spot:**
Jupiter's API abstracts away decimal conversion. A "1.0" amount in a quote might represent 1,000,000 base units for a 6-decimal token (USDC) or 1,000,000,000 base units for a 9-decimal token (SOL).
*   **Vulnerability:** In-memory math operating on raw `u64` integers without scaling factors will result in catastrophic precision loss or integer overflow/underflow.
*   **Scenario:** Calculating `SOL -> Token -> USDC`.
    *   If `Token` has 8 decimals, but the math assumes 9 (SOL standard), the output USDC amount will be off by a factor of 10.
    *   On a $1k TVL pool, this precision error wipes the profit margin and violates the invariant.

**Mandatory Implementation:**
All arithmetic in `triangular_math.rs` must operate on normalized values (e.g., convert all inputs to a common 18-decimal fixed-point representation or `f64` with strict rounding) before calculating the output, then denormalize back to the specific token's decimals for the final instruction argument.

---

### III. ADAPTIVE POOL DEPTH SIZING VERIFICATION

**Proposal:** `effective_size = config_max.min(pool_liq_sol * 0.05).min(headroom)`
**Analysis:**
*   **Pool:** $1,000 TVL (~6.5 SOL liquidity at current prices).
*   **5% Headroom:** $1,000 * 0.05 = $50 USD (~0.33 SOL).
*   **Trade Size:** `default_trade_size_lamports` = 0.15 SOL.
*   **Logic Execution:**
    1.  `config_max` = 0.15 SOL.
    2.  `pool_liq_sol * 0.05` ≈ 0.33 SOL.
    3.  `effective_size` = min(0.15, 0.33) = **0.15 SOL**.
*   **Result:** The trade proceeds at full size.
*   **Safety Check:** 0.15 SOL is ~2.3% of the pool's depth.
    *   Price Impact Formula: $\Delta P \approx \frac{\Delta x}{x} \times 2$ (for $xy=k$).
    *   Impact $\approx 2.3\% \times 2 = 4.6\%$.
*   **Verdict:** The sizing logic is **SAFE**. The 5% clamp effectively prevents "rug-pull" liquidity exhaustion, but allows sufficient size to capture the spread. The 0.15 SOL cap acts as a secondary hard guardrail.

---

### IV. BINDING CONSENSUS DECREE (V14)

1.  **Routing:** Jupiter HTTP routing is hereby **DEPRECATED** for triangular arbitrage. All quote calculations must use `triangular_math.rs`.
2.  **TVL Floor:** Minimum TVL is lowered to **$1,000 USD**.
3.  **Sizing:** The 5% dynamic liquidity clamp is **APPROVED** and mandatory for all pools < $5,000 TVL.
4.  **Precision:** Strict decimal normalization is **MANDATORY**. No transaction shall be signed without verifying `input_mint_decimals` and `output_mint_decimals` alignment.

---

### V. RUST CODE BLUEPRINT (V14)

```rust
// src/triangular_math.rs

use crate::types::*;
use crate::error::ArbitrageError;
use float_cmp::approx_eq;

/// Represents the decimal configuration for a specific token mint.
pub struct TokenInfo {
    pub mint: Pubkey,
    pub decimals: u8,
}

/// Calculates the output amount for a single swap leg using Constant Product (xy=k).
/// SAFETY: Handles decimal normalization to prevent precision loss.
pub fn calculate_swap_output(
    input_amount: u64,
    input_token: &TokenInfo,
    output_token: &TokenInfo,
    pool_reserve_in: u64,
    pool_reserve_out: u64,
    fee_bps: u16, // e.g., 30 for 0.3%
) -> Result<u64, ArbitrageError> {
    // 1. Normalize reserves to a common scale (e.g., 18 decimals) to handle math safely
    // We use u128 to prevent overflow during multiplication.
    let scale_in = 10u64.pow(18 - input_token.decimals as u64);
    let scale_out = 10u64.pow(18 - output_token.decimals as u64);

    let reserve_in_normalized = (pool_reserve_in as u128)
        .checked_mul(scale_in as u128)
        .ok_or(ArbitrageError::Overflow)?;
    
    let reserve_out_normalized = (pool_reserve_out as u128)
        .checked_mul(scale_out as u128)
        .ok_or(ArbitrageError::Overflow)?;

    // 2. Apply Fee
    // amount_out = (input * (10000 - fee) * reserve_out) / (reserve_in * 10000 + input * (10000 - fee))
    // Note: We normalize input_amount to 18 decimals as well
    let input_normalized = (input_amount as u128)
        .checked_mul(scale_in as u128)
        .ok_or(ArbitrageError::Overflow)?;

    let fee_multiplier = 10000 - fee_bps as u128;
    let input_with_fee = input_normalized
        .checked_mul(fee_multiplier)
        .ok_or(ArbitrageError::Overflow)?;

    let numerator = input_with_fee
        .checked_mul(reserve_out_normalized)
        .ok_or(ArbitrageError::Overflow)?;

    let denominator = reserve_in_normalized
        .checked_mul(10000)
        .ok_or(ArbitrageError::Overflow)?
        .checked_add(input_with_fee)
        .ok_or(ArbitrageError::Overflow)?;

    // 3. Calculate Normalized Output
    let output_normalized = numerator / denominator;

    // 4. Denormalize back to the specific token's decimals
    // If output_token is 6 decimals (USDC), we divide by 10^12
    let output_amount = (output_normalized / scale_out as u128) as u64;

    // 5. Safety Check: Ensure output is non-zero
    if output_amount == 0 {
        return Err(ArbitrageError::InsufficientOutput);
    }

    Ok(output_amount)
}

/// Calculates the effective trade size based on pool depth.
/// Implements the V14 "Adaptive 5% Pool Depth" logic.
pub fn calculate_effective_trade_size(
    config_max_lamports: u64,
    pool_liq_sol: f64,
    headroom_sol: f64,
) -> u64 {
    // Convert SOL floats to lamports (u64)
    let pool_liq_lamports = (pool_liq_sol * 1_000_000_000.0) as u64;
    let headroom_lamports = (headroom_sol * 1_000_000_000.0) as u64;

    // Calculate 5% depth limit
    let depth_limit = pool_liq_lamports
        .checked_mul(5)
        .unwrap_or(0)
        .checked_div(100)
        .unwrap_or(0);

    // Return the minimum of: Config Max, 5% Pool Depth, or Global Headroom
    config_max_lamports
        .min(depth_limit)
        .min(headroom_lamports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_precision_handling() {
        // Simulate SOL (9 decimals) -> USDC (6 decimals)
        // Pool: 10 SOL, 1000 USDC
        let sol_info = TokenInfo { mint: Pubkey::default(), decimals: 9 };
        let usdc_info = TokenInfo { mint: Pubkey::default(), decimals: 6 };
        
        // Input 1 SOL (1_000_000_000 lamports)
        let result = calculate_swap_output(
            1_000_000_000,
            &sol_info,
            &usdc_info,
            10_000_000_000, // 10 SOL reserve
            1_000_000_000,  // 1000 USDC reserve
            0 // 0 fee for test
        ).unwrap();

        // Expected: ~90 USDC (approx)
        // Logic should handle decimal shift correctly.
        assert!(result > 89_000_000 && result < 91_000_000);
    }

    #[test]
    fn test_adaptive_sizing() {
        // Pool has 6.5 SOL liquidity ($1k)
        let size = calculate_effective_trade_size(
            150_000_000, // 0.15 SOL config max
            6.5,         // 6.5 SOL liquidity
            1.0          // 1.0 SOL headroom
        );
        
        // 5% of 6.5 SOL = 0.325 SOL.
        // Min(0.15, 0.325, 1.0) = 0.15 SOL.
        assert_eq!(size, 150_000_000);
    }
}
```