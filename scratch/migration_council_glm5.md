# ⚠️ PRINCIPAL AUDIT REPORT: MIGRATION SNIPING ENGINE v1.0

**Auditor:** GLM-5, Principal Solana Security & Smart Contract Auditor
**Classification:** CRITICAL ARCHITECTURE DECISION
**Status:** AUDIT COMPLETE — CONDITIONAL APPROVAL

---

## 1. ARCHITECTURE VERDICT: KEEP (DUAL-ENGINE HYBRID)

**Decision:** You **MUST** retain the current Atomic Arbitrage Engine. Do not replace it.

### Rationale

The proposed Migration Sniping Engine and the existing Atomic Arbitrage Engine operate on fundamentally different mathematical invariants. Replacing the atomic system with this directional system would be a strategic error equivalent to dismantling a vault because you bought a lottery ticket.

**A. Mathematical Incompatibility**
*   **Atomic Arbitrage:** Operates on a **Zero-Loss Invariant**. The transaction either succeeds with a guaranteed profit $P > 0$, or it reverts. The risk surface is strictly execution failure (slippage, RPC latency). It is deterministic.
*   **Migration Sniping:** Operates on a **Probabilistic Expectancy**. $E[V] = (Win\% \times AvgWin) - (Loss\% \times AvgLoss) - Fees$. This introduces market risk (price action, liquidity evaporation, malicious contract logic).
*   **Conflict:** You cannot "replace" a risk-free mechanism with a risky one without drastically altering the bot's risk profile.

**B. Capital Efficiency & Opportunity Cost**
*   **Atomic Engine:** Capital is only locked for the duration of a single block (~400ms). High turnover.
*   **Sniper Engine:** Capital is locked for 30s - 180s (holding time).
*   **Operational Risk:** If you replace the engine, your 0.40 SOL bankroll sits idle during holds, missing atomic opportunities that could fire in parallel. A Hybrid system allows the Atomic Engine to hunt for risk-free arb while the Sniper Engine manages the directional bet.

**C. The Hybrid Architecture Recommendation**
Implement a **Compartmentalized Dual-Engine**:
1.  **Reserve Bucket:** 0.15 SOL (Immutable Reserve).
2.  **Atomic Bucket:** 0.15 SOL (Dedicated to circular arb).
3.  **Sniper Bucket:** 0.10 SOL (Dedicated to migration sniping).
*   **Logic:** The Atomic Engine runs freely. The Sniper Engine only activates when specific migration criteria are met. If the Sniper is in a position, the Atomic Engine continues to operate using its isolated 0.15 SOL bucket.

---

## 2. HONEYPOT & RUG SHIELD AUDIT

The "Pre-flight Sell Simulation" is the most critical component. On Solana, you cannot simulate a state transition without executing it. Therefore, we must use a **Split-Transaction Simulation Strategy**.

### The "Ghost Sell" Protocol (Rust Implementation)

You cannot sell a token you do not own. Therefore, the simulation must happen logically *before* the buy, but technically involves checking the state of the pool *as if* you owned the tokens.

**The Correct Approach: Instruction Inspection & Quote Simulation**

1.  **Instruction Inspection (The "5-Pillar Shield"):**
    Before buying, fetch the token's AccountData.
    *   **Mint Authority:** Must be `None` (Revoked).
    *   **Freeze Authority:** Must be `None` (Revoked).
    *   **Transfer Fee Configuration:** Check extension data. Must be absent or 0%.
    *   **Delegate:** Check for hidden `approve` instructions in the pool initialization logs or current state.

2.  **The "Quote Sell" Simulation (The Honeypot Trap):**
    Most honeypots block the *transfer* instruction specifically for the `Transfer` function while allowing `TransferChecked`, or they blacklist specific programs.
    
    **Implementation:**
    Use the Raydium SDK/CPMM Program to build a **mock sell instruction**.
    *   Do not sign it.
    *   Use `solana_rpc_client::rpc_client::RpcClient::simulate_transaction_with_config`.
    *   **Crucial:** Set `SigVerifyConfig` to skip signature verification (since you don't have the tokens yet).
    *   **The Check:** Does the simulation return `InsufficientFunds` (Expected, because you have 0 tokens) or `InstructionError` (Honeypot/Blocked)?
    *   **Advanced Check:** If the simulation returns `InsufficientFunds`, the transfer logic is likely valid. If it returns a custom error or succeeds (impossible), investigate further.
    
    **Alternative (More Robust):**
    Inspect the liquidity pool's **withdrawal logic**. If the pool allows removing liquidity (LP tokens), you can often bypass transfer restrictions. However, for simple sniping, checking if the `transfer` instruction contains malicious logic (e.g., `if amount > 0 then revert`) via instruction parsing is safer.

**Verdict:** The "5-Pillar Shield" is your primary defense. Simulation is unreliable for tokens you don't hold. Rely on **Static Analysis** of the mint account and **Pool Authority Checks**.

---

## 3. RUST IMPLEMENTATION DETAILS

### A. State Machine: `GraduationSniper`

```rust
pub enum SniperState {
    Idle,
    Scanning, // Monitoring Pump.fun API / Geyser for migration tx
    PreflightCheck { token_mint: Pubkey, pool_id: Pubkey },
    EntryQueued { signal: SignalData },
    InPosition { entry_price: f64, entry_time: Instant, amount: u64 },
    ExitSignal { reason: ExitReason },
    CoolingDown,
}

pub struct SignalData {
    pub token_mint: Pubkey,
    pub pool_id: Pubkey,
    pub liquidity_sol: f64,
    pub price_deviation_bps: u16, // e.g., 1000 for 10%
    pub sell_velocity: u8,
}
```

### B. In-Memory Position Tracking
Since we are holding for 30s+, we cannot rely on RPC `getAccountInfo` for every loop (too slow).
*   **Local State:** Maintain a `Arc<Mutex<Option<Position>>>` in the bot's memory.
*   **Subscription:** Subscribe to the Pool's `AccountData` via Geyser/Websocket.
*   **Update:** On every slot update, recalculate `current_price` based on the pool's SOL/Token reserves.

### C. Raydium CPMM Swap Instruction Layout
Raydium CPMM (Concentrated Product Market Maker) requires specific instructions.
*   **Instruction:** `Swap`
*   **Accounts:**
    1.  `[SIGNER]` Payer/Trader
    2.  `[WRITE]` Token Account A (User's Token)
    3.  `[WRITE]` Token Account B (User's SOL/WSOL)
    4.  `[WRITE]` Pool Token Account A
    5.  `[WRITE]` Pool Token Account B
    6.  `[]` Pool State Account
    7.  `[]` AMM Config
    8.  `[]` Observation State (optional, usually needed)
    9.  `[]` Token Program
    10. `[]` Vault (optional)

**Note:** You must handle **WSOL** (Wrapped SOL).
*   **Buy Flow:** Create WSOL Account -> Sync (Lamports -> WSOL) -> Swap -> Close WSOL Account.
*   **Sell Flow:** Create WSOL Account -> Swap -> Sync (WSOL -> Lamports) -> Close.
*   *Optimization:* Keep a permanent WSOL ATA (Associated Token Account) funded with minimal rent to save compute units on creation.

### D. ATA Lifecycle
*   **Check:** `get_associated_token_address`.
*   **Creation:** If ATA missing, add `create_associated_token_account` instruction to the buy transaction.
*   **Cleanup:** Do not close the ATA immediately. Keep it open in case the position re-enters. Close only on bot shutdown or if token balance is 0 for > 1 hour.

---

## 4. RISK & EXPECTANCY REVIEW

### Mathematical Model
*   **Bankroll ($B$):** 0.4046 SOL.
*   **Trade Size ($S$):** 0.10 SOL.
*   **Risk per Trade ($R$):** 10% Stop Loss = 0.01 SOL.
*   **Take Profit ($T$):** ~7-10% (Target 97% of fair price from 90% entry).
*   **Fees ($F$):** ~0.0025 SOL (Solana + Raydium fees).

### Expectancy Calculation
$$E = (Win\% \times (T - F)) - (Loss\% \times (R + F))$$

**Scenario A: Optimistic (Early Snipe Success)**
*   Win Rate: 60%
*   Avg Win: 0.007 SOL (7% move)
*   Avg Loss: 0.01 SOL (Stop loss hit)
*   $E = (0.6 \times 0.007) - (0.4 \times 0.0125) = 0.0042 - 0.005 = -0.0008$ SOL.

**Result:** **Negative Expectancy.**

**Scenario B: Realistic (High Volatility)**
*   Win Rate: 40%
*   $E = (0.4 \times 0.007) - (0.6 \times 0.0125) = 0.0028 - 0.0075 = -0.0047$ SOL.

### Critical Flaw Identified
The strategy assumes a "dip" recovery. In Pump.fun migrations, the "dump" is often permanent (Rug Pull). The probability of price recovering to 97% of fair price after a migration dump is low. Most migrations trend to zero or stabilize at -50%.

**Risk of Ruin (RoR):**
With 0.40 SOL and 0.10 SOL trades, you have ~4 "bullets".
If you hit 3 consecutive stop losses (-0.03 SOL total), your bankroll drops to 0.37 SOL.
The strategy is **High Variance**.

### Recommendations for Viability
1.  **Reduce Trade Size:** 0.05 SOL (2% of bankroll risk).
2.  **Tighter Entry:** Only enter if Price Depression > 15% (Entry at 85% fair price). This increases the spread for recovery.
3.  **Time Stop is Critical:** The 3-minute limit is good. Do not extend it.
4.  **Add "Dev Dump" Detection:** If Dev wallet sells > 50% of their holdings in the first 5s, **ABORT**.

### Final Summary
The architecture is sound (Hybrid), but the **Expectancy is currently negative** based on standard Stop Loss/Take Profit ratios. The edge relies on the assumption that "Bonding Curve Fair Price" is a magnet for price action, which is often false in crypto markets (price often overshoots or collapses).

**Adjustment:** Increase the entry threshold to 85% price depression to widen the profit margin, or reduce position size to 0.05 SOL to survive the inevitable variance.