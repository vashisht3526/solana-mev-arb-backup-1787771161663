================================================================================
POST-IMPLEMENTATION AUDIT REPORT V15: NATIVE RAYDIUM CPMM & IN-MEMORY ARBITRAGE
================================================================================

**Auditor:** GLM-5
**Status:** CONDITIONAL APPROVAL FOR FRANKFURT EC2 DEPLOYMENT
**Severity Matrix:** 2 CRITICAL | 1 HIGH | 2 MEDIUM

---

### I. EXECUTIVE SUMMARY

The implementation of the native Raydium CPMM instruction builder and the in-memory triangular fast-path represents a significant architectural maturation, reducing dependency on the Jupiter aggregator for specific DEX pairs. However, the audit reveals **critical mathematical risks** in the CPMM integration and **logic gaps** in the fast-path fallback mechanism.

The system is **NOT READY** for live capital deployment until the CPMM slippage calculation is corrected and the fallback logic is hardened against infinite recursion.

---

### II. CRITICAL FINDINGS

#### 1. [CRITICAL] Mathematical Integrity: Raydium CPMM Slippage Calculation
**Location:** `create_direct_raydium_cpmm_swap_ix` (implied logic)

The audit identifies a high probability of incorrect slippage application in the native instruction builder. Standard Constant Product ($x \cdot y = k$) formulas differ significantly from Raydium's CPMM implementation regarding fee handling.

**The Vulnerability:**
Developers often apply slippage tolerance (e.g., 0.5%) to the *Input Amount* rather than the *Minimum Output Amount*. In CPMMs, fees are often deducted from the input *before* the swap calculation, or the constant $k$ is adjusted.

**Mathematical Check:**
Incorrect Implementation:
$$ \text{min\_out} = \text{expected\_out} \times (1 - \text{slippage}) $$
$$ \text{where } \text{expected\_out} = \frac{\text{Input} \times \text{Output\_Reserve}}{\text{Input\_Reserve} + \text{Input}} $$
*(This fails to account for the swap fee deducted from Input, leading to transaction failure or value loss.)*

Correct Implementation (Raydium CPMM Standard):
1. Calculate effective input: $\text{Input}_{eff} = \text{Input} \times (1 - \text{fee}_{swap})$
2. Calculate raw output: $\text{Output}_{raw} = \frac{\text{Input}_{eff} \times \text{Output\_Reserve}}{\text{Input\_Reserve} + \text{Input}_{eff}}$
3. Apply slippage protection: $\text{Min\_Out} = \text{Output}_{raw} \times (1 - \text{slippage}_{tol})$

**Action Required:** Verify the instruction builder uses the post-fee input amount for the $y$-reserve adjustment and applies the slippage tolerance strictly to the final `min_out` parameter passed to the Anchor discriminator.

#### 2. [CRITICAL] Control Flow: Recursive Fallback Risk in Triangular Fast-Path
**Location:** `build_atomic_triangular_arbitrage_tx`

The implementation of the fast-path fallback to `build_direct_in_memory_dual_leg_tx` introduces a potential infinite loop or stack overflow if not strictly gated.

**Scenario:**
1. `triangular_arb` calls Jupiter for 3-hop quote.
2. Jupiter fails (timeout/error).
3. Fallback triggers `build_direct_in_memory_dual_leg_tx`.
4. If `build_direct_in_memory_dual_leg_tx` fails to find a route (e.g., liquidity too low), does it error out, or does it bubble up a "Retry" signal that triggers the parent function again?

**Code Requirement:**
The fallback logic must be terminal. It must return a distinct error type (e.g., `ArbitrageError::FallbackLiquidityInsufficient`) rather than a generic retry error.

```rust
// REQUIRED PATTERN
match jupiter_quote {
    Ok(quote) => execute_triangular(quote),
    Err(_) => {
        log::warn!("Jupiter failed, engaging native 2-leg fast-path");
        build_direct_in_memory_dual_leg_tx(params).map_err(|e| {
            // Ensure this error terminates the loop
            ArbitrageError::NativeFallbackFailed(e)
        })
    }
}
```

---

### III. HIGH SEVERITY FINDINGS

#### 3. [HIGH] Discriminator Hardcoding vs. Dynamic Resolution
**Location:** `create_direct_raydium_cpmm_swap_ix`

The Anchor discriminator `[143, 190, 74, 135, 197, 61, 69, 203]` is hardcoded.
While this matches the Raydium CPMM program ID `CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C`, Anchor discriminators are method-specific (Sighash).

**Risk:** If Raydium updates their program or if the instruction being called is not the standard `swap` (e.g., `swap_base_input` vs `swap_base_output`), the hardcoded bytes will cause the transaction to silently fail with a "Instruction Error: Invalid Account Data" or similar, consuming compute units without execution.

**Action:** Ensure the discriminator corresponds exactly to the `swap_base_input` (or equivalent) function signature used in the Raydium SDK source. Add a comment block referencing the specific SDK version/commit hash used to derive the hash.

---

### IV. MEDIUM SEVERITY FINDINGS

#### 4. [MEDIUM] Capital Protection: TVL Floor Logic in Micro-Cap Pools
**Location:** Capital Protection Parameters

The TVL floor is set at $1,000 USD ("micro-alpha zone").
**Risk:** Pools with $1,000 TVL are extremely susceptible to:
1. **Rug Pulls:** Liquidity can be withdrawn instantly.
2. **Price Impact:** Even 0.05 SOL (~$7) may move the price >5% in a $1k pool, triggering the slippage protection and failing the trade.

**Recommendation:** The `0.150 SOL` position sizing cap is appropriate, but the entry logic must check `pool_reserves > (position_size * 20)`. If the pool cannot handle 20x the position size without exceeding the slippage threshold, the trade should be aborted regardless of the $1,000 TVL floor.

#### 5. [MEDIUM] In-Memory Routing: Stale Quote Risk
**Location:** `build_direct_in_memory_dual_leg_tx`

In-memory assembly relies on locally cached reserve data (RPC `getMultipleAccounts`).
**Latency Gap:** The Frankfurt EC2 instance has ~150-200ms latency to Solana validators.
**Scenario:**
1. Bot fetches reserves.
2. 200ms latency.
3. Bot submits TX.
4. In that 200ms, a competitor arbitrages the pool.
5. Bot's TX fails due to slippage (Capital Protected) but wastes the blockspace fee (Rent exempt lamports lost).

**Mitigation:** Implement a "Staleness Check" timestamp. If the reserve data is older than 400ms, force a refresh or skip the opportunity.

---

### V. CAPITAL PROTECTION VERIFICATION

**Parameters:**
- TVL Floor: $1,000 (Active)
- Position Sizing: 0.050 - 0.150 SOL (Adaptive)
- Untouchable Reserve: 0.150 SOL

**Audit Result:** PASS (with caveats)
The adaptive 5% pool reserve headroom clamp is correctly implemented to prevent draining the pool and triggering `InvalidAccountData` errors.
**Warning:** Ensure the "Untouchable reserve floor" (0.150 SOL) is checked *before* the transaction is built, not just after. If the bot attempts to build a TX with 0.160 SOL total balance, the fee deduction might push it below the rent-exempt threshold of the executing account.

---

### VI. DEPLOYMENT CHECKLIST (FRANKFURT EC2)

Before releasing to Frankfurt production:

1.  [ ] **Unit Test:** Verify Raydium CPMM math against a known mainnet transaction (TX signature validation).
2.  [ ] **Integration Test:** Force Jupiter to fail and verify the native 2-leg fallback executes without error.
3.  [ ] **Stress Test:** Simulate 50 concurrent requests to ensure the in-memory cache doesn't race condition.
4.  [ ] **Discriminator Check:** Confirm `[143, 190, 74, 135, 197, 61, 69, 203]` matches the deployed Raydium CPMM binary.
5.  [ ] **Capital Safety:** Verify the bot aborts if wallet balance < 0.300 SOL (Reserve + Max Position).

**Final Verdict:** Fix Critical #1 (Math) and Critical #2 (Recursion) before deployment. The architecture is sound, but the edge cases in native assembly are where capital is lost.