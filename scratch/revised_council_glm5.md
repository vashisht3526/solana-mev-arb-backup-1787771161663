## ⚡ GLM-5 SECURITY AUDIT: REVISED RAYDIUM-NATIVE MIGRATION MODEL

**Status:** <span style="color: #ff4444">**CRITICAL FLAW RESOLVED - NEW SYSTEMIC RISKS IDENTIFIED**</span>

The diagnosis of the "Virtual Reserve Flaw" is mathematically precise and correct. The previous model was structurally broken. However, the revised proposal introduces **Liquidity Execution Risk** and **Latency Drift** that must be addressed.

---

### 1. MATHEMATICAL & MARKET STRUCTURE VERDICT

#### ✅ **The Virtual Reserve Fix (Validated)**
You have correctly identified that Pump.fun's bonding curve utilizes a synthetic k-value that does not translate to Raydium's CPMM (Constant Product Market Maker).
*   **Pump.fun:** Price is a function of `virtual_sol_reserves` (including phantom buffer) and `virtual_token_reserves`.
*   **Raydium:** Price is strictly `real_sol_reserves / real_token_reserves`.
*   **Conclusion:** Comparing Pump.fun's "virtual fair price" to Raydium's "real open price" was indeed a phantom discount signal. Discarding the bonding curve anchor in favor of **Raydium Pool Open Price (P0)** is the only mathematically sound approach.

#### ⚠️ **Risk: The "P0" Anchor Volatility**
While P0 is the correct anchor, it is highly unstable in the first 15 seconds.
*   **Mechanism:** Migration triggers a race between snipers (buying) and paper-handed holders (selling).
*   **The Trap:** If the initial dump is severe (e.g., price drops to 60% P0 instantly), your rule "Price > 80% of P0" prevents entry. This is good risk management.
*   **The Edge Case:** If snipers pump the price to 150% P0 in seconds, and you enter at 80% during a retrace, you are buying a distribution bag.
*   **Recommendation:** Add a **Volatile Range Filter**: Only enter if `Current_Price` is within `0.80 * P0` to `1.10 * P0`. Avoid chasing sniper pumps above P0.

#### ✅ **Sell Velocity Decay (Validated)**
*   **Math:** Measuring the delta in sell counts (or volume) between windows `T[0-10]` and `T[10-30]` is a robust proxy for panic exhaustion.
*   **Logic:** A >60% drop in sell velocity indicates the "migration dumpers" have exhausted their supply. This is a valid entry signal.

#### ⚠️ **Risk: Buy/Sell Volume Ratio**
*   **The Flaw:** A ratio > 1.2 is dangerous if the absolute volume is low (e.g., 2 SOL buy / 1 SOL sell).
*   **Fix:** Add a **Minimum Volume Threshold**: `Total_Buy_Volume_30s > 5 SOL` AND `Ratio > 1.2`. This ensures the demand is significant, not just noise.

---

### 2. TIMING & EXIT STRATEGY AUDIT

#### **Time Stop: 10 Minutes (600s)**
*   **Verdict:** **APPROVED.**
*   **Rationale:** Meme token volatility follows a power law. The "recovery or death" phase typically resolves within 2-8 minutes. Extending from 3m to 10m captures the recovery tail without exposing capital to the long-term bleed phase.

#### **Trailing Stop: -5% from Local High**
*   **Verdict:** **TOO TIGHT.**
*   **Rationale:** Meme assets have standard noise volatility of 5-10%. A 5% trailing stop will be triggered by normal market microstructure noise (random walk), exiting a winning position prematurely.
*   **Correction:** Set **Trailing Stop to -8% or -10%**. This allows the position to "breathe" while protecting the upside.

#### **Hard Stop Loss: -10%**
*   **Verdict:** **APPROVED.**
*   **Rationale:** Hard limit to preserve capital.

#### **Zero-Retry Toxic Tag**
*   **Verdict:** **CRITICAL SECURITY FEATURE.**
*   **Rationale:** Re-attempting sells on failed transactions is the primary cause of "fee burn" in honeypots. Permanent tagging prevents capital leakage.

---

### 3. RUST TECHNICAL IMPLEMENTATION (RAYDIUM CPMM)

To implement this natively in Rust, you must monitor the Raydium AMM program directly.

#### **A. Data Structures**

```rust
#[derive(Debug, Clone)]
struct PoolState {
    pub pool_id: Pubkey,
    pub base_mint: Pubkey, // SOL
    pub quote_mint: Pubkey, // Token
    pub sol_reserves: u64,
    pub token_reserves: u64,
    pub open_price_p0: f64, // Captured at T=0
    pub creation_slot: u64,
}

#[derive(Debug, Default)]
struct VelocityTracker {
    sells_t0_t10: u64, // Volume in slots 0-10
    sells_t10_t30: u64, // Volume in slots 10-30
    buys_t0_t30: u64,
    window_start_slot: u64,
}
```

#### **B. Extracting P0 & Reserves**
Raydium CPMM pools are standard Solana Accounts. You need to deserialize the `AccountInfo` using the Raydium SDK definitions (or `spl-token` account layouts for simpler liquidity pools).

**Logic Flow:**
1.  **Detect Migration:** Listen for logs mentioning `RaydiumInitialize2` or similar instruction signature.
2.  **Capture P0:**
    *   Fetch the Pool Account immediately.
    *   Calculate `P0 = sol_reserves / token_reserves` (adjusting for decimals).
    *   Store `P0` in local memory mapped to `pool_id`.
3.  **Real-time Updates:** Subscribe to the pool account via `geyserPlugin` or `rpc_pubsub` for slot-by-slot updates.

#### **C. Computing Rolling Velocity**
You cannot rely on block timestamps (unreliable). Use **Solana Slots**.

```rust
impl VelocityTracker {
    fn update(&mut self, current_slot: u64, is_sell: bool, amount: u64) {
        let slot_delta = current_slot - self.window_start_slot;
        
        // Reset window if > 30s (approx 150 slots assuming 400ms block time, adjust for current network)
        if slot_delta > 150 { 
            self.sells_t0_t10 = 0; // Shift windows or reset
            // ... logic to shift windows ...
        }
        
        if is_sell {
            // Add to appropriate window based on slot offset
        }
    }
    
    fn check_entry_signal(&self) -> bool {
        let decay = self.sells_t0_t10.saturating_sub(self.sells_t10_t30);
        let decay_pct = decay as f64 / self.sells_t0_t10.max(1) as f64;
        
        decay_pct > 0.60 // > 60% decay
    }
}
```

#### **D. Managing Trailing Stops**
Maintain a state machine for the active position.

```rust
struct PositionManager {
    entry_price: f64,
    local_high: f64,
    entry_slot: u64,
}

impl PositionManager {
    fn evaluate_exit(&mut self, current_price: f64, current_slot: u64) -> ExitSignal {
        // Update High
        if current_price > self.local_high {
            self.local_high = current_price;
        }

        // 1. Take Profit
        if current_price >= self.entry_price * 1.10 {
            return ExitSignal::TakeProfit;
        }

        // 2. Trailing Stop (Revised to 8%)
        if current_price <= self.local_high * 0.92 {
            return ExitSignal::TrailingStop;
        }

        // 3. Hard Stop
        if current_price <= self.entry_price * 0.90 {
            return ExitSignal::HardStop;
        }

        // 4. Time Stop
        if (current_slot - self.entry_slot) > 1500 { // ~10 mins
            return ExitSignal::TimeStop;
        }

        ExitSignal::Hold
    }
}
```

---

### 4. FINAL SYNTHESIZED IMPLEMENTATION PLAN

#### **Phase 1: Detection & Initialization (T=0)**
1.  **Listen:** Stream logs for Raydium Pool Creation signatures.
2.  **Filter:** Check for `Pool Liquidity > 20 SOL`.
3.  **Initialize:**
    *   Fetch Pool State.
    *   Calculate `P0`.
    *   Initialize `VelocityTracker`.
    *   Initialize `PositionManager` (Empty).

#### **Phase 2: The Wait (T=0 to T=15s)**
1.  **Action:** Monitor swaps. Populate velocity windows.
2.  **Logic:** Do not execute trades. Let the "Migration Dump" and "Sniper Pump" settle.

#### **Phase 3: Entry Logic (T > 15s)**
Execute Buy ONLY if ALL conditions met:
1.  `Current_Price >= 0.80 * P0` AND `Current_Price <= 1.10 * P0` (Avoiding extremes).
2.  `Sell_Velocity_Decay > 60%`.
3.  `Buy_Sell_Ratio > 1.2` AND `Buy_Volume_Absolute > 5 SOL`.
4.  `Dev_Hold < 3%` (Requires pre-fetching token metadata).
5.  **Ghost Simulation:** Simulate a swap of 1 token unit. If simulation passes -> **EXECUTE BUY**.

#### **Phase 4: Position Management**
1.  **Update:** On every slot update, recalculate `current_price` from reserves.
2.  **Trailing:** Update `local_high`.
3.  **Exit:** Trigger sell transaction immediately upon `ExitSignal`.

#### **Phase 5: Toxicity Handling**
1.  If sell simulation fails OR transaction reverts:
    *   **Log:** "Toxic Asset Detected".
    *   **Tag:** Add `pool_id` to local blacklist.
    *   **Action:** Do not retry. Move to next opportunity.

**Final Security Note:** This model relies heavily on the accuracy of the `P0` capture. Ensure your RPC node is high priority to prevent stale block data from skewing your entry price. Use `skipPreflight: false` for entry transactions to catch errors early, but `skipPreflight: true` for exits (desperation mode) unless the Toxic Tag logic is triggered.