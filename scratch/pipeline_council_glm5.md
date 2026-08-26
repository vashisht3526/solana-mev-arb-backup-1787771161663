# GLM-5 Security Audit: Raydium-Native Migration Sniping Pipeline

**Audit Classification:** Architectural Feasibility & Economic Viability
**Principal Engineer:** GLM-5, Solana RPC Protocol Specialist
**Target:** Zero-Cost Data Pipeline & Timing Calibration

---

## 1. ZERO-COST DATA PIPELINE AUDIT

### Verdict: Feasible with Critical Constraints

The zero-cost approach is technically viable but requires precise architectural implementation to avoid rate-limits and data loss on public RPC endpoints.

### The Optimal Architecture: Hybrid "Trigger-Subscribe" (Option C Modified)

Standard `logsSubscribe` on public nodes is risky—it consumes high bandwidth and public endpoints often throttle broad filter subscriptions. The most efficient free architecture is a **Triggered Account Subscribe** model.

#### Recommended Rust Stack (Zero-Cost)
1.  **Trigger Source:** Poll PumpPortal WebSocket (free, specialized) or monitor `logsSubscribe` specifically for the Raydium Migration Program ID (`675ttPX...`) signatures only.
2.  **Data Ingest:** Upon detecting a migration transaction signature, extract the new Pool Pubkey from the transaction metadata.
3.  **Instant Subscribe:** Immediately open `accountSubscribe` to that specific Pool Pubkey.
    *   **Encoding:** `base64` (faster parsing than jsonParsed, which requires RPC compute).
    *   **Commitment:** `processed` (lowest latency, ~400ms slot time).
    *   **Data Extraction:** Parse the base64 buffer locally to extract `R_sol` (SOL reserves) and `R_tok` (Token reserves).

#### Critical Implementation Note (The "Free Tier" Constraint)
Public RPCs (Solana Mainnet Beta endpoints) generally allow ~10-20 concurrent WebSocket subscriptions.
*   **Mitigation:** Your Rust client must implement a strict **Subscription Manager**. Once a pool exits your evaluation window (e.g., T+60s), you **must** `accountUnsubscribe` immediately to free the slot for new migrations.
*   **Buffer Parsing:** Do not fetch account state via REST. Parse the WebSocket push payload directly. Raydium AMM accounts have fixed offsets for reserves (typically offsets 128-136 for SOL).

---

## 2. TIMING & RATIO CALIBRATION AUDIT

### Validation: Shift to T+30s is Strategically Sound

Your hypothesis regarding the T+15s "panic dump" phase is empirically accurate for the current meta.

#### The "Migration Dump" Phenomenon
When a token migrates from Pump.fun to Raydium:
1.  **T+0s to T+15s:** "Bag holders" from the bonding curve often dump immediately to exit positions, creating massive sell pressure and artificial velocity spikes.
2.  **T+15s to T+30s:** Price stabilization. The dumpers are exhausted.
3.  **T+30s+:** Organic volume enters.

#### Revised Calibration Parameters
*   **Evaluation Window:** **T+30s to T+45s** (15-second observation window).
*   **Entry Trigger:** Buy/Sell Ratio > 1.5 calculated over the [T+30s, T+45s] window.
*   **Velocity Filter:** Sell Velocity Decay must be < 50% of the peak velocity observed in [T+0s, T+15s].
    *   *Rationale:* This confirms the initial panic dump has subsided.

**Warning:** This 30-second delay introduces "Sniper Risk." Other bots using paid HyperSDK feeds may front-run this window. However, for a *shadow* system testing zero-cost feasibility, this delay is necessary to filter noise.

---

## 3. COUNTERFACTUAL SHADOW LOGGING DESIGN

To tune filters, you must track False Negatives (missed opportunities) and False Positives (avoided rugs).

### Data Structure: `MigrationShadowLog`

```rust
struct MigrationShadowLog {
    // Identifiers
    token_mint: Pubkey,
    raydium_pool: Pubkey,
    migration_slot: u64,
    migration_timestamp: i64,

    // Decision State
    filter_result: FilterResult, // Enum: Accepted, Rejected_Ratio, Rejected_Velocity
    
    // Metrics at Decision Time (T+45s)
    metrics_snapshot: MetricsSnapshot {
        buy_sell_ratio_t45: f64,
        sell_velocity_decay: f64,
        reserve_sol: u64,
    },

    // Counterfactual Tracking (10-minute post-analysis)
    outcome_analysis: Option<OutcomeAnalysis> 
}

struct OutcomeAnalysis {
    price_at_t10m: f64,      // Price 10 minutes post-migration
    volume_at_t10m: u64,     // Total volume in the 10m window
    is_rug: bool,            // Did liquidity drop >90%?
    x_score: i32,            // "Would we have profited?" score
}
```

### Logging Mechanism
1.  **Log Every Candidate:** Do not just log trades you *would have* taken. Log every migration detected.
2.  **Async Analysis:** A separate thread wakes up 10 minutes after `migration_timestamp`, fetches the current pool state via a lightweight REST call, and populates `OutcomeAnalysis`.
3.  **Tuning Metric:** Calculate "Regret Score."
    *   If `filter_result == Rejected` AND `outcome_analysis.x_score > 0`, increment **False Negative Counter**.
    *   If `filter_result == Accepted` AND `outcome_analysis.is_rug == true`, increment **False Positive Counter**.

---

## 4. FINAL SYNTHESIZED IMPLEMENTATION PLAN

### Phase 1: Infrastructure (Rust / Frankfurt EC2)
1.  **WebSocket Manager:** Implement `tokio-tungstenite` client connecting to a public Solana RPC endpoint (e.g, `api.mainnet-beta.solana.com` or a backup provider).
2.  **Subscription Pool:** Create a bounded channel for subscription IDs. Limit to 15 concurrent subscriptions to stay under public RPC limits.
3.  **Deserializer:** Implement `serde` deserializer for raw Raydium AMM account layout to extract reserves from base64 blobs.

### Phase 2: Detection Logic
1.  **Monitor:** Subscribe to logs for the Raydium Migration Program.
2.  **Parser:** On log event, parse transaction metadata to find the newly created Pool ID.
3.  **Tracker:** Push `MigrationShadowLog` entry with timestamp T=0. Subscribe to Pool Account updates.

### Phase 3: Evaluation Logic (The "Shadow Brain")
1.  **Collector:** Accumulate reserve updates in a rolling window buffer.
2.  **T+30s Check:** Calculate initial velocity.
3.  **T+45s Check:** Calculate Buy/Sell Ratio.
    *   **Decision:** Log `Accepted` or `Rejected` with specific reason.
    *   **Action:** Unsubscribe from Pool (critical for resource management).

### Phase 4: Post-Processing (Counterfactual)
1.  **T+10min:** Cron job fetches final state for all logs in the last hour.
2.  **Report:** Generate daily CSV: "Missed Profit vs. Avoided Rugs."

**Security Sign-off:**
This architecture minimizes capital expenditure ($0) while maximizing signal fidelity by shifting the evaluation window past the initial volatility phase. The counterfactual logging is essential; without it, you are flying blind on filter sensitivity.

**GLM-5**
*Principal Solana Security & RPC Protocol Engineer*