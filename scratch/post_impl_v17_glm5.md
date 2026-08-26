================================================================================
AUDIT REPORT V17: DEDICATED HELIUS RPC MIGRATION & EXECUTION PIPELINE
================================================================================
Auditor: GLM-5
Status: **CONDITIONAL PASS** (Ready for Frankfurt EC2 Deployment with Minor Config Adjustments)
================================================================================

### 1. RPC MIGRATION & RELIABILITY AUDIT

**Observations:**
*   **Endpoint Migration:** The migration from the public `api.mainnet-beta.solana.com` to a dedicated Helius endpoint is correctly implemented across the target modules (`dual_leg_engine`, `triangular_live_engine`, `jito_builder`, `live_engine`, `main`).
*   **Rate Limit Mitigation:** The transition successfully eliminates the primary bottleneck of HTTP 429 errors during high-frequency polling loops. This is critical for the `triangular_live_engine` where concurrent quote requests previously caused stalls.
*   **Latency Profile:** The expectation of sub-15ms response latency is realistic for a dedicated Helius plan within the AWS `eu-central-1` (Frankfurt) region, assuming the Helius geographic endpoint is optimized for EU West.

**Critical Finding - Security Risk:**
*   **Hardcoded API Key:** The URL `https://mainnet.helius-rpc.com/?api-key=9b358df5-981f-4a3f-8f25-5ff9af04653f` contains a live API key directly in the source code.
    *   *Risk:* If this repository is public or compromised, the key can be abused, leading to quota exhaustion or service termination.
    *   *Remediation:* Move the API key to an environment variable (e.g., `HELIUS_RPC_URL`) or a secure secrets manager. Do not commit keys to version control.

### 2. EXECUTION PIPELINE & LATENCY OPTIMIZATION

**Blockhash Pre-warmer:**
*   **Mechanism:** The 200ms background refresh cycle is aggressive but appropriate for a dedicated RPC.
*   **Impact:** This ensures the transaction builder (`jito_builder.rs`) always has a "fresh" blockhash, reducing the likelihood of "blockhash not found" errors during bundle submission.
*   **Latency Gain:** By decoupling blockhash fetching from the critical transaction signing path, you save approximately 50-150ms per trade execution cycle.

**Zero-Delay Dispatch:**
*   The removal of artificial sleep timers in the dispatch loop allows the engine to react immediately to on-chain opportunities.
*   *Warning:* Ensure the dedicated RPC plan has sufficient "Compute Unit" (CU) bandwidth. A tight loop without rate limiting can still hit provider-side CU caps even if HTTP 429s are gone.

### 3. CAPITAL PROTECTION & SIZING LOGIC

**Parameters Reviewed:**
*   **Reserve Floor Guard (0.150 SOL):** This is a robust safety buffer. It prevents the bot from draining the wallet entirely during a failed arbitrage leg or gas spike.
*   **Adaptive Sizing (5% Headroom):** The clamp range (0.050 - 0.150 SOL) is conservative.
    *   *Analysis:* This ensures the bot doesn't over-leverage into shallow liquidity pools. However, for high-volume pairs, 0.150 SOL might be too small to capture significant alpha.
    *   *Recommendation:* Monitor success rates. If hit-rate is high but total profit is low, consider scaling the upper clamp to 0.500 SOL once the engine proves stable in Frankfurt.

**Minimum Profit Floor (30,000 lamports):**
*   *Analysis:* 30,000 lamports (~$0.005 at current SOL prices) is extremely tight.
*   *Risk:* A profit floor of 0.00003 SOL barely covers the transaction fees (0.000005 SOL base) and potential Jito tips.
*   *Recommendation:* Increase the minimum profit floor to **100,000 - 150,000 lamports (0.0001 - 0.00015 SOL)**. The current floor risks "dust trades" that generate negative PnL after accounting for full gas costs and slippage.

### 4. FRANKFURT EC2 DEPLOYMENT READINESS

**Infrastructure Checklist:**
1.  **Network Proximity:** Ensure the Helius RPC endpoint selected is the EU/Global endpoint. If you are using the US endpoint from Frankfurt, you will add ~80ms of round-trip latency, negating the benefits of the dedicated RPC.
2.  **Clock Sync:** Verify `chronyd` is running on the EC2 instance. Zero-delay execution relies on accurate timestamps for blockhash validity.
3.  **Resource Limits:** Check `ulimit -n` on the EC2 instance. The new aggressive polling rate will open more TCP connections. Ensure the limit is raised (e.g., 65535) to avoid "Too many open files" errors.

### 5. FINAL VERDICT

**Summary:**
The migration logic is sound and the architectural shift to a dedicated RPC with pre-warming is the correct move for high-frequency arbitrage.

**Action Items before Release:**
1.  **CRITICAL:** Externalize the API Key immediately.
2.  **HIGH:** Increase `Minimum Profit Floor` to >100,000 lamports to ensure net-positive PnL.
3.  **MEDIUM:** Verify Helius endpoint geographic routing for Frankfurt.

**Approval:** Cleared for deployment to Frankfurt EC2 staging environment upon remediation of the API Key exposure.