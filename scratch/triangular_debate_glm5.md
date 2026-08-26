# GLM-5: Rigorous Mathematical Evaluation of Triangular MEV Architecture

## Executive Summary
The proposed architecture demonstrates sophisticated understanding of Solana's execution environment, but contains **critical pricing errors in the whale-exclusion zone sizing** and **incomplete slippage modeling** that will result in unprofitable trades. Below is the rigorous mathematical evaluation.

---

## 1. Triangular AMM Pricing: Mathematical Framework

### 1.1 General 3-Hop Output Function
For the path $SOL \xrightarrow{A} Token_A \xrightarrow{B} USDC \xrightarrow{C} SOL$, the gross output is defined by the composite product:

$$
Y_{out} = \Delta_{SOL} \cdot \prod_{i=1}^{3} \left( \frac{R_{out}^{(i)}}{R_{in}^{(i)} + \Delta_i} \right) \cdot \gamma_i
$$

Where:
- $\Delta_{SOL}$: Input amount in lamports
- $R_{in}^{(i)}, R_{out}^{(i)}$: Reserves of input/output tokens for pool $i$
- $\gamma_i = 1 - f_i$: Post-fee multiplier (typically 0.997 for 0.3% fee)
- $\Delta_i$: Intermediate input amount (function of previous swap output)

### 1.2 Recursive Reserve Update (In-Memory State)
To achieve sub-microsecond evaluation, the recursive intermediate amounts must be calculated as:

$$
\Delta_1 = \Delta_{SOL}
$$
$$
\Delta_2 = \Delta_1 \cdot \frac{R_{out}^{(1)}}{R_{in}^{(1)} + \Delta_1} \cdot \gamma_1
$$
$$
\Delta_3 = \Delta_2 \cdot \frac{R_{out}^{(2)}}{R_{in}^{(2)} + \Delta_2} \cdot \gamma_2
$$

**Implementation Requirement**: `triangular_math.rs` must cache $R_{in}, R_{out}$ as `u128` to prevent overflow during intermediate multiplication before the division.

---

## 2. Optimal Position Sizing in Whale-Exclusion Zone

### 2.1 Critical Pricing Error Identification
**The proposed sizing of 0.015 - 0.020 SOL is mathematically mispriced by a factor of 1000.**

- Proposed: 15,000,000 - 20,000,000 lamports
- Claimed USD Value: ~$1.50 - $2.00
- **Actual USD Value (at ~$150/SOL)**: **$2,250 - $3,000**

**Verdict**: The label "Whale-Exclusion Zone" contradicts the actual capital at risk. A $2,500 position size is **not** in the "dust" category; it is a significant retail trade. True whale-exclusion sizing (sub-$10) would be < 70,000 lamports.

### 2.2 Optimal Sizing Formula (Kelly-Criterion Adjusted for AMM Depth)
Given the constraint of Max Price Impact $\phi_{max} = 0.15\%$ on the shallowest pool, the optimal input $\Delta^*$ is bounded by:

$$
\Delta^* \leq \min_{i \in \{1,2,3\}} \left[ R_{in}^{(i)} \cdot \left( \frac{\phi_{max}}{1 - \phi_{max}} \right) \right]
$$

**Proof for Constant Product ($x \cdot y = k$):**
Price impact is defined as the relative change in marginal price:
$$
\phi = 1 - \frac{P_{new}}{P_{old}} = 1 - \frac{R_{out}/(R_{in} + \Delta)}{R_{out}/R_{in}} = \frac{\Delta}{R_{in} + \Delta}
$$
Solving for $\Delta$:
$$
\Delta = R_{in} \cdot \frac{\phi}{1 - \phi}
$$
For $\phi = 0.0015$ (0.15%):
$$
\Delta \approx R_{in} \cdot 0.001502
$$

### 2.3 Numerical Example & Constraint Verification
Assume the shallowest pool in the triangle has TVL of \$500 (Pool 1: SOL/Token A).
- If SOL is 50% of TVL: $R_{SOL} \approx \$250 \approx 1.67$ SOL.
- Max Input $\Delta^* \approx 1.67 \cdot 0.001502 \approx 0.0025$ SOL (**2,500,000 lamports**).

**Conclusion**: The proposed 0.015 SOL input violates the 0.15% price impact constraint on a \$500 TVL pool by a factor of **6x**. The trade will suffer significant slippage, eroding the arbitrage spread.

**Corrected Sizing for Strict Adherence**:
To strictly satisfy $\phi < 0.15\%$ on a \$500 TVL pool:
$$
\Delta_{max} \approx 0.0025 \text{ SOL}
$$
This reduces the profit surface area significantly. The architecture must choose between:
1. **Higher TVL pools** ($> \$3,000$) to support 0.015 SOL sizing.
2. **Accept higher price impact** (e.g., 1%) on low TVL pools.

---

## 3. Price Impact Bounds & Slippage Model

### 3.1 Composite Price Impact
The total price impact is not additive but multiplicative across hops:
$$
\Phi_{total} = 1 - \prod_{i=1}^{3} (1 - \phi_i)
$$
For small $\phi_i$:
$$
\Phi_{total} \approx \sum_{i=1}^{3} \phi_i
$$

**Risk**: If each hop incurs 0.15% impact, total slippage $\approx 0.45\%$. The arbitrage opportunity must exceed this composite drag plus fees.

### 3.2 Slippage Bounds Implementation
In `triangular_math.rs`, enforce a hard bound:
```rust
fn validate_price_impact(delta: u64, reserve_in: u64, max_bps: u64) -> bool {
    // Impact in basis points
    let impact_bps = (delta as u128 * 10000) / (reserve_in as u128 + delta as u128);
    impact_bps <= max_bps
}
```

### 3.3 DLMM Specifics (Meteora)
For the Meteora DLMM leg, the pricing is not constant-product but follows the active bin logic:
$$
P_{bin} = (1 + \Delta s)^b
$$
Where $\Delta s$ is the bin step and $b$ is the bin index.
**Requirement**: The in-memory math must track the **Bin Liquidity Distribution** ($L_{bin}$) rather than just $R_{in}/R_{out}$. If $\Delta$ exceeds $L_{active\_bin}$, the price jumps discretely to the next bin, causing a step-function increase in price impact.

---

## 4. Profitability Invariant Analysis

### 4.1 Profit Floor Equation
The proposed invariant:
$$
Y_{out} \geq \Delta_{SOL} + \Gamma_{Jito} + \Gamma_{Gas} + \Pi_{min}
$$

Where $\Gamma_{Jito}$ is the dynamic tip (35% margin share).

### 4.2 Break-Even Spread Requirement
Let $S_{gross}$ be the gross arbitrage spread (difference between effective route price and oracle price).
$$
\Pi_{net} = \Delta_{SOL} \cdot S_{gross} - \Delta_{SOL} \cdot \Phi_{total} - \Gamma_{total}
$$
For $\Pi_{net} > 0$:
$$
S_{gross} > \Phi_{total} + \frac{\Gamma_{total}}{\Delta_{SOL}}
$$

**Numerical Analysis**:
- $\Delta_{SOL} = 0.015$ SOL ($2,250 notional).
- $\Gamma_{total} \approx 0.0001$ SOL (Gas + Rent exempt ephemeral ATA overhead).
- $\Phi_{total} \approx 0.45\%$ (assuming 0.15% per hop).
- Required Gross Spread: $S_{gross} > 0.45\% + \frac{0.0001}{0.015} \approx 0.45\% + 0.67\% \approx 1.12\%$.

**Verdict**: The architecture requires finding triangular arbitrage opportunities with **> 1.12% gross spread** to be profitable at 0.015 SOL sizing. This is feasible in volatile meme markets but requires tight latency.

---

## 5. Implementation Requirements & Mathematical Fixes

### 5.1 Corrected Sizing Logic
The "Whale-Exclusion Zone" logic must be re-parameterized:
1. **Dynamic TVL-Based Cap**: Calculate $\Delta_{max}$ dynamically based on the shallowest pool's reserves using the formula in Section 2.2.
2. **Hard Cap**: Enforce a hard cap of 0.005 SOL (5M lamports) for pools with TVL < \$1,000 to prevent adverse selection.

### 5.2 Ephemeral ATA Gas Economics
The rent-exempt reclaim is mathematically sound:
$$
\text{Net Gas} = \text{Base Fees} + \text{ATA Creation} - \text{Rent Reclaim}
$$
Since Rent Reclaim $\approx$ ATA Creation Cost, the marginal cost is near-zero (only compute units for Close Account instruction). This is a **valid optimization**.

### 5.3 Jito Tip Optimization
The 35% margin share is aggressive. The optimal tip $\Gamma^*$ to maximize inclusion probability while minimizing cost is:
$$
\Gamma^* = \arg\max_{\Gamma} \left[ P_{inclusion}(\Gamma) \cdot \Pi_{net}(\Gamma) \right]
$$
Given the competitive environment, a dynamic tip based on the simulated bundle profit is superior to a fixed 35% share.

---

## Final Verdict

| Component | Mathematical Soundness | Notes |
|-----------|----------------------|-------|
| Triangular Pricing | **Valid** | Requires u128 overflow protection |
| Whale-Exclusion Sizing | **INVALID** | Mispriced by 1000x; violates impact constraints |
| Price Impact Model | **Valid** | Must include DLMM bin-step logic |
| Profitability Invariant | **Valid** | Requires >1% gross spread discovery |
| Ephemeral ATA | **Valid** | Correct rent-recovery math |

**Recommendation**: Correct the sizing formula to be dynamic relative to pool depth. The architecture is viable only if the sizing constraint is relaxed or the TVL filter is raised to >$5,000 per pool.