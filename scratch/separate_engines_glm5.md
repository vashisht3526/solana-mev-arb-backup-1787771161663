**AUDIT REPORT: GLM-5 Quantitative Analysis**
**Subject:** Mathematical Invariants & Pricing Verification for Decoupled MEV Engines

================================================================================
### I. EXECUTIVE SUMMARY
================================================================================

I have verified the proposed safety parameters for both the Dual-Leg Spatial and Triangular 3-Hop engines. The decoupling strategy is mathematically sound, provided the sizing heuristics are strictly bound to pool liquidity depth.

**Critical Finding:** The slippage compounding in the Triangular engine requires a tighter aggregate slippage guard than the proposed 0.35% reserve impact cap to ensure profitability after Jito tips. I recommend a dynamic tip allocation model derived from the Gross Extractable Value (GEV) rather than static bounds.

================================================================================
### II. DUAL-LEG SPATIAL ENGINE (Direct 2-Leg) VERIFICATION
================================================================================

**1. Liquidity Sizing & Depth Analysis**
*   **Proposed Constraints:** 0.015 - 0.035 SOL sizing, $\le 0.5\%$ shallow pool reserve.
*   **Mathematical Check:**
    Let $R_{shallow}$ be the reserve of the shallower pool.
    The trade size $S_{trade}$ must satisfy:
    $$S_{trade} \le 0.005 \times R_{shallow}$$
    
    For typical SOL/USDC pools (e.g., Raydium/Orca), liquidity depth often exceeds 50,000 SOL. A 0.035 SOL trade represents $\approx 0.00007\%$ of depth.
    **Verdict:** **PASS**. The 0.5% reserve impact cap is a safe conservative bound. The 0.035 SOL ceiling is negligible for deep liquidity but acts as a necessary guardrail for illiquid "shitcoin" pairs where $R_{shallow}$ might be $< 100$ SOL.

**2. Slippage Bounds**
*   **Proposed Guard:** 0.35% - 0.50%.
*   **Pricing Formula:**
    Expected Output $O_{exp}$ vs. Min Output $O_{min}$.
    $$O_{min} = O_{exp} \times (1 - \epsilon_{slip})$$
    where $\epsilon_{slip} \in [0.0035, 0.0050]$.
*   **Analysis:** A 2-leg route involves two AMM interactions.
    Effective Price Impact $\approx \text{Impact}_1 + \text{Impact}_2$.
    Given the sizing constraint ($\le 0.5\%$ reserve), the price impact is naturally capped below 0.5%.
    **Verdict:** **PASS**. The slippage guard is aligned with the sizing constraint.

**3. Dynamic Tip Allocation (Jito)**
*   **Formula:**
    $$Tip_{jito} = \max(0.0001 \text{ SOL}, \beta \times (\text{GrossProfit}_{sim}))$$
    where $\beta \approx 0.3$ (aggressive) to $0.5$ (conservative).
*   **Recommendation:** For the Dual-Leg engine, set $\beta = 0.4$. The transaction size is small (2 swaps), fitting easily in a single packet without compute budget overflow.

================================================================================
### III. TRIANGULAR 3-HOP ENGINE (Cyclic Spatial) VERIFICATION
================================================================================

**1. Whale-Exclusion Zone & Sizing**
*   **Proposed Constraints:** 0.015 - 0.020 SOL, TVL $3k-$40k.
*   **Mathematical Check:**
    Targeting low-liquidity pools ($3k-$40k TVL) implies $R_{shallow}$ is significantly lower than major pairs.
    If TVL $\approx \$10,000$ (approx 60 SOL), and we trade 0.020 SOL:
    $$\frac{S_{trade}}{R_{shallow}} \approx \frac{0.020}{30} \approx 0.0006 \implies 0.06\% \text{ Reserve Impact}$$
    **Verdict:** **PASS**. The sizing is strictly conservative for the target liquidity profile. The 0.35% reserve impact cap is redundant but safe; the actual impact will likely be $<0.1\%$ given the 0.020 SOL max size.

**2. Slippage Compounding (Critical Audit)**
*   **Route:** SOL $\to$ Token A $\to$ USDC $\to$ SOL.
*   **Compounding Effect:**
    Total Slippage $\Psi_{total} \approx \psi_1 + \psi_2 + \psi_3$.
    While individual pool impacts are small, the *effective* slippage includes:
    1. AMM Fees (typically 0.25% - 0.30% per hop).
    2. Price Impact.
    3. Staleness risk (120ms guard).
    
    **Calculation:**
    Fee Drag alone = $3 \times 0.25\% = 0.75\%$.
    The proposed reserve impact cap is 0.35%.
    **Conflict:** The reserve impact cap (0.35%) is *lower* than the cumulative AMM fees (0.75%). This implies the engine will filter out valid opportunities if "reserve impact" is confused with "total cost".
    
    **Correction Required:** The constraint `<= 0.35% reserve impact cap` applies to *price impact*, not total fees. Ensure the logic distinguishes:
    $$Cost_{total} = \sum (\text{Fee}_i) + \sum (\text{Impact}_i)$$
    The engine should require:
    $$\text{Profit}_{gross} > \text{Cost}_{total} + \text{Tip}_{jito}$$
    
    **Verdict:** **CONDITIONAL PASS**. Clarify that the 0.35% cap applies to *Price Impact* only, not AMM fees. The engine must calculate profitability *post-fees*.

**3. Dynamic Tip Allocation (Jito)**
*   **Complexity:** 3-hop transactions consume significantly more Compute Units (CU). A standard CU budget (200k) may be exceeded, requiring prioritization of compute budget instructions.
*   **Staleness Guard:** 120ms.
    At block times of ~400ms, 120ms allows for roughly 1-2 landing attempts.
    **Formula:**
    $$Tip_{jito} = \max(0.0005 \text{ SOL}, \gamma \times (\text{Profit}_{sim} - \text{Fees}_{amm}))$$
    Use $\gamma = 0.5$ to outbid generic arbitrage bots.
    
    **Recommendation:** Implement a "Tip Floor" of 0.0005 SOL for Triangular routes. The complexity of 3-hops increases the risk of simulation mismatch; a higher tip floor ensures inclusion in the leader's block during competitive windows.

================================================================================
### IV. FINAL DECOUPLING CONSENSUS
================================================================================

**Mathematical Invariants Confirmed:**
1.  **Sizing:** Both engines utilize conservative sizing relative to their target liquidity profiles.
2.  **Slippage:** Dual-Leg bounds are safe. Triangular requires strict separation of "Fee Drag" vs. "Price Impact" in the profit calculation logic.
3.  **Liquidity:** The Whale-Zone logic ($3k-$40k) effectively filters out high-capital competitors, creating a niche extraction window.

**GLM-5 Decree:**
Proceed with the separation. The mathematical models hold under the condition that the Triangular engine's profit filter explicitly accounts for the additive nature of 3-hop AMM fees before checking the 0.35% impact reserve constraint.

**Signed:**
GLM-5, Quantitative Researcher
Council Auditor