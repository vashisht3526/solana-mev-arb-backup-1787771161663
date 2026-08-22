**COUNCIL AUDIT REPORT // CLASSIFICATION: RESTRICTED**
**AUDITOR:** GLM-5, Senior Solana MEV Risk Officer
**SUBJECT:** System Readiness & Wallet Security Protocol for `solana-mev-arb v0.1.0`
**DATE:** [Current Date]

================================================================================
### 1. SECURITY & MEV BEST PRACTICE EVALUATION (OPTION A vs. OPTION B)
================================================================================

**VERDICT: OPTION A (Server-Generated Burner Keypair) IS THE ONLY ACCEPTABLE METHOD.**

**Cryptographic & Operational Justification:**

*   **Attack Surface Reduction (Option A):** By generating the keypair directly on the EC2 instance (`solana-keygen new`), the private key exists *only* in the server's volatile memory and the local file system (encrypted at rest by AWS EBS defaults). The private key never transits the user's local machine, browser clipboard, or network interfaces. This eliminates local keylogger threats and clipboard hijacking vectors common in Windows/Mac environments.
*   **The "Hot Wallet" Doctrine:** In MEV operations, the execution wallet is a "Hot Wallet." It is programmatically signing transactions at high velocity. If the bot logic is compromised or a remote code execution (RCE) vulnerability is found in the Node.js Control Plane, the funds *will* be drained. Therefore, the wallet should be treated as "ephemeral capital."
*   **Catastrophic Risk of Option B:** Importing a Phantom Recovery Phrase (12-word seed) onto the server creates a **Single Point of Failure**.
    *   If the server is compromised, the attacker gains access not only to the MEV bot's capital but potentially to the user's *entire* Phantom portfolio (including cold storage assets).
    *   Phantom uses standard BIP39 derivation paths. Compromise of the seed phrase allows full reconstruction of all derived accounts.
*   **Operational Segregation:** Option A enforces strict asset segregation. The Phantom wallet acts as the "Vault/Treasury," and the Server Keypair acts as the "Trading Till." This is the industry standard for risk containment.

**MANDATORY IMPLEMENTATION PROTOCOL FOR OPTION A:**
1.  Execute generation command: `solana-keygen new --outfile ~/.config/solana/mev_live.json`
2.  **CRITICAL:** Write down the generated seed phrase *offline* on paper only if you wish to back up this specific burner wallet. Do not store it digitally on the server.
3.  Set permissions: `chmod 400 ~/.config/solana/mev_live.json`.
4.  Fund the public address from Phantom.
5.  Configure environment variable: `export SOLANA_PRIVATE_KEY=~/.config/solana/mev_live.json` (or equivalent for your Rust binary).

================================================================================
### 2. ARCHITECTURAL READINESS ASSESSMENT
================================================================================

**Status: CONDITIONALLY READY.**

The engine architecture demonstrates a sophisticated understanding of Solana MEV mechanics. The safeguards listed are robust, but I have flagged three critical "Go/No-Go" checks you must verify before arming the bot:

**A. Compute Budget & Fee Logic (PASS with Caveats):**
*   The 180,000 Compute Budget is correctly sized for complex routes (Raydium + Meteora DLMM).
*   **Risk:** Ensure your `priority_fee` calculation is dynamic. A static fee will result in blockouts during high congestion. Your "Jito 25% Tip Clamp" is a good safeguard, but verify the base fee is at least 10,000 microlamports per compute unit to land in blocks during turbulence.

**B. Ephemeral ATA Logic (EXCELLENT):**
*   The `CreateIdempotentATA -> Swap -> CloseAccount` flow is the gold standard for capital efficiency. It prevents "dust" accumulation of scam tokens and saves the 0.00203 SOL rent per trade.
*   **Verification:** Ensure your Rust binary handles the `CloseAccount` instruction failure gracefully. If the swap fails but the close instruction executes, you lose the rent; if the close fails, you leave dust. Both must be atomic.

**C. Temporal Jitter & Circuit Breakers (PASS):**
*   The 1,500ms Jitter Filter is essential for Pump.fun volatility. It prevents the bot from reacting to "flash" order book manipulations that disappear before execution.
*   **Circuit Breaker Check:** The 0.08 SOL Position Sizing Cap is **active**. Your total capital is 0.41 SOL. The bot will effectively split this into ~5 max-sized trades. This is safe.

**D. RPC & Connectivity (CRITICAL FAILURE POINT):**
*   You are running a WebSocket stream to PumpPortal. This is reliable for *detection*.
*   **WARNING:** You did not specify your *execution* RPC provider. Standard public RPCs (Solana Labs) will rate-limit your bot immediately upon high-frequency submission.
*   **REQUIREMENT:** You must have a dedicated RPC endpoint (Helius, QuickNode, or Triton) configured in your Rust binary for transaction submission. Do not rely on the public cluster API.

================================================================================
### 3. RISK & CAPITAL GUIDELINES (0.41 SOL BANKROLL)
================================================================================

With a starting capital of 0.41 SOL (~$38), you are in the "Micro-Capital" tier. The margin for error is thin.

**A. The "Gas War" Reserve:**
*   Your Circuit Breaker sets a "0.10 SOL Gas Reserve Floor."
*   **Reality:** 0.10 SOL is a safe buffer for rent-exempt accounts and failed transaction fees.
*   **Effective Trading Capital:** 0.41 SOL - 0.10 SOL Reserve = **0.31 SOL (~$29) Active Capital.**
*   **Implication:** Your bot will effectively be trading with ~75% of your stack. Do not manually lower the reserve floor.

**B. The First 24-48 Hours (Burn-In Phase):**
*   **Objective:** Survival, not Profit. The goal is to verify that the "Immutable Circuit Breakers" trigger correctly under live fire.
*   **Strategy:**
    1.  **Manual Override:** If possible, reduce the `Position Sizing Cap` from 0.08 SOL to **0.04 SOL** for the first 24 hours. This allows you to execute twice as many trades for data collection while risking less per round trip.
    2.  **Monitor Slippage:** Watch the `RequireProfit` logic like a hawk. On Pump.fun, slippage can exceed 10% in milliseconds. If your profit threshold is set to 0.5% but slippage averages 1%, you will bleed SOL.
    3.  **Jito Tip Analysis:** Monitor your Jito tips. If you are tipping 0.0005 SOL for a $0.50 profit trade, you are losing money after fees. Ensure the tip clamp scales with position size.

**C. Operational Security:**
*   **Port 3000 Exposure:** Your dashboard is on HTTP (Port 3000).
    *   **Risk:** Anyone with the IP can see your bot status.
    *   **Immediate Action:** Ensure `iptables` or AWS Security Groups restrict Port 3000 access to **ONLY** your personal IP address. Do not expose this to 0.0.0.0/0.

**FINAL DIRECTIVE:**
Proceed with **Option A**. Fund the wallet. Verify the RPC endpoint. Restrict port access. The engine is sound, but the market is ruthless. Good luck, Risk Officer.