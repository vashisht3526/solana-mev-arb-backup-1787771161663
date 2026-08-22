# Council Report: Architecture for 'rust-mev-arb-live' Production Engine

**Classification:** CRITICAL INFRASTRUCTURE
**Author:** GLM-5, Senior Solana MEV Quantitative Risk Officer
**Subject:** Safety Invariants, Architecture, and Circuit Breakers for Live Mainnet Trading

================================================================================

## I. EXECUTIVE SUMMARY

The transition from shadow mode to live mainnet execution requires a paradigm shift from "performance optimization" to "capital preservation and fault tolerance." We cannot guarantee 100% bug-free code, but we can architect a system that is *safe-by-default*.

This proposal outlines a "Fail-Static" architecture. If the engine encounters unknown states, simulation failures, or metric breaches, it defaults to a complete halt rather than a best-effort trade.

================================================================================

## II. KEYPAIR & WALLET ISOLATION (Security Layer)

To prevent credential leakage and ensure operational security, the engine will operate under the principle of **Least Privilege & Ephemeral Exposure**.

### A. Key Ingestion Mechanism
The engine will **not** accept plaintext private keys via command-line arguments (process list exposure).
1.  **Source:** A dedicated, read-once JSON keypair file (`trading-key.json`) with strict `0o400` file permissions, or a base58 encoded string injected via a secure Environment Variable (`TRADING_PRIV_KEY`).
2.  **Memory Safety:**
    *   The keypair struct will be wrapped in a `Zeroize` wrapper.
    *   Upon instantiation of the `LiveExecutor` struct, the key is loaded into memory.
    *   **Invariant:** The keypair is *never* logged. Any `Debug` or `Display` implementation for the executor struct must mask the key field as `[REDACTED]`.
    *   **Drop Mechanism:** On graceful shutdown, the memory holding the key is explicitly zeroed out.

### B. Wallet Isolation
*   **Invariant:** The engine initializes by deriving the Public Key from the private key. It refuses to trade if the derived address does not match a whitelisted `AUTHORIZED_TRADER_PUBKEY` configured in the environment. This prevents a compromised key file from acting on an unintended wallet.

================================================================================

## II. PRE-FLIGHT SAFETY SHIELDS (Execution Layer)

Before any bundle is signed and dispatched to Jito, it must pass the "Gatekeeper" checks.

### A. The Simulation Mandate
**Invariant:** No transaction shall be signed without a successful `simulateTransaction` RPC call.
1.  **Process:**
    *   Construct the atomic bundle (Setup -> Swap A -> Swap B -> Cleanup).
    *   Set Jito Tip to 0 for simulation purposes (to isolate logic errors from tip mechanics).
    *   Call `simulateTransaction` with commitment level `Processed`.
2.  **Verification:**
    *   Check `meta.err` is `null`.
    *   **Balance Check:** Parse the simulation result. Calculate `post_balances - pre_balances`.
    *   **Profit Assertion:** `require!(net_lamport_change > 0)`. Note: Since we simulate with 0 tip, a positive net change indicates gross profit > gas fees.

### B. Atomic Revert Guarantee
The transaction structure must be strictly atomic.
*   **Structure:** `ComputeBudget` -> `Setup (ATA creation if needed)` -> `Swap A` -> `Swap B` -> `Close WSOL ATA` -> `Jito Tip`.
*   **Logic:** If `Swap B` fails, the entire transaction reverts. The `Close WSOL ATA` instruction ensures we reclaim rent if the swap succeeds but the logic fails later (though in an atomic bundle, the rent is reclaimed on revert anyway).
*   **Safety:** We never send a "partial" transaction.

### C. Jito Tip Rotation
To prevent single-point-of-failure or tip-account blacklisting by accident:
*   **Implementation:** Maintain a static array of the 8 official Jito Tip Accounts.
*   **Strategy:** Rotate the tip account selection using a Round-Robin index incremented on every transaction attempt, or random selection per bundle to prevent pattern analysis by adversaries.

================================================================================

## IV. HARD PROGRAMMATIC CIRCUIT BREAKERS (Risk Management Layer)

These are non-negotiable, hard-coded guards that run in the main event loop *before* any opportunity analysis begins.

### A. Minimum Reserve Floor (Gas Tank)
*   **Logic:** `if (current_sol_balance <= 0.15 SOL) { return Err(EngineHalt::InsufficientGasReserve); }`
*   **Rationale:** Ensures the wallet always has enough SOL for priority fees and rent exemption for ATAs, preventing the engine from "bricking" itself unable to close positions.

### B. Max Drawdown Stop-Loss (The "Panic Switch")
*   **Logic:** Store `initial_balance` at engine start. Calculate `current_drawdown = initial_balance - current_balance`.
*   **Trigger:** `if (current_drawdown >= 0.05 SOL) { trigger_emergency_shutdown(); }`
*   **Action:** Sets a global atomic boolean `TRADING_ENABLED = false`. Sends an immediate alert to the operator. Requires manual restart to reset the `initial_balance` baseline.

### C. Consecutive Failure Cooldown
*   **Logic:** Maintain a counter `consecutive_failures`.
*   **Trigger:** `if (consecutive_failures >= 3) { sleep(Duration::from_secs(300)); reset_counter(); }`
*   **Reset:** On a successful landing, reset counter to 0.
*   **Rationale:** Prevents the engine from spiraling during network congestion or RPC node failure, saving gas fees during unfavorable conditions.

### D. Position Sizing Envelope
*   **Logic:** `trade_size = min(0.08 SOL, active_balance * 0.20)`.
*   **Hard Cap:** Even if the wallet grows to 10 SOL, the initial live phase caps the trade at 0.08 SOL to limit blast radius.
*   **Invariant:** The input amount for the swap instruction is clamped to this value mathematically before signing.

================================================================================

## V. EPHEMERAL ATA & GAS MANAGEMENT

### A. Lifecycle Management
For SOL -> Token -> SOL arb loops:
1.  **Check:** Does the wallet have the Token ATA? If not, include `create_associated_token_account` in the bundle.
2.  **WSOL Handling:** The engine uses a temporary WSOL account.
    *   Fund WSOL account (sync native).
    *   Swap.
    *   **Crucial:** The final instruction in the bundle must be `close_account` for the WSOL ATA, returning rent and remaining WSOL to the main wallet.

### B. Compute Budget Strategy
*   **Request:** 180,000 Compute Units (Conservative buffer for 2 DEX swaps + overhead).
*   **Priority Fee:** Dynamic calculation based on recent blockhash fee market, capped at a maximum (e.g., 10,000 micro-lamports per CU) to prevent overspending during spam attacks.
*   **Invariant:** `PriorityFee <= Max_Fee_Cap`.

================================================================================

## VI. ERROR HANDLING & TELEMETRY

### A. Real-Time Telemetry
Logs must be structured (JSON) for parsing by external monitoring tools (Grafana/Datadog).
*   **Fields:** `timestamp`, `level`, `event_type`, `signature`, `slot`, `profit_lamports`, `error_msg`.
*   **Sensitive Data:** Strict filter on logs. No private keys, no seed phrases.

### B. Process Isolation (PM2)
*   **Configuration:** `rust-mev-arb-live` runs as a standalone PM2 process `ecosystem.config.js`.
*   **Watchdog:** PM2 restarts the process if it crashes (exit code != 0).
*   **Crash Loop Detection:** If the process restarts 5 times in 60 seconds, PM2 should stop attempting to allow investigation (prevents draining funds via restart loops).

### C. Signature Verification
*   Before logging a "Success", the engine must query the RPC `getSignatureStatuses` to confirm the transaction landed in a confirmed block. "Dropped" bundles must be flagged immediately.

================================================================================

## VII. ARCHITECTURAL DIAGRAM (Logical Flow)

```text
[START LOOP]
      |
      v
[CHECK CIRCUIT BREAKERS] --(Breach)--> [HALT & ALERT]
      |
      v
[FETCH WALLET BALANCE] --(< 0.15 SOL)--> [HALT]
      |
      v
[SCAN OPPORTUNITY]
      |
      v
[BUILD BUNDLE] --(Size > Cap)--> [CLAMP SIZE]
      |
      v
[SIMULATE TX] --(Fail)--> [LOG & CONTINUE]
      |
      v
[SIGN TX] -> [ROTATE TIP ACCOUNT] -> [SEND JITO BUNDLE]
      |
      v
[CONFIRM TX] --(Timeout/Fail)--> [INCREMENT FAIL COUNTER]
      |                           |
      |                           v
      |                     [COOLDOWN CHECK]
      |
      v
[LOG PnL] -> [RESET FAIL COUNTER] -> [LOOP]
```

================================================================================

## VIII. FINAL SAFETY CHECKLIST

1.  [ ] **Key Security:** Is the keypair zeroized on drop?
2.  [ ] **Simulation:** Is simulation mandatory for *every* trade?
3.  [ ] **Atomicity:** Does the transaction revert cleanly on failure?
4.  [ ] **Circuit Breakers:** Are the drawdown and reserve floors hard-coded constants?
5.  [ ] **Position Sizing:** Is the trade size clamped to the lower of the cap or percentage?

**Recommendation:** Proceed with implementation of `rust-mev-arb-live` adhering strictly to these invariants. Start with a funded wallet of exactly 0.38 SOL to test the reserve floor logic immediately.

*End of Report.*