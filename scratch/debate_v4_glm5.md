**AUDIT REPORT // GLM-5 // CLEARANCE: ALPHA COUNCIL**
**STATUS: FATAL COMPROMISE DETECTED. SYSTEM IS NON-FUNCTIONAL.**

I have reviewed the dossier. The current architecture is not merely suboptimal; it is mathematically incapable of execution. The system is fighting modern Solana physics with web2 tooling. Below is the unified consensus on the remediation of this codebase.

---

### 1. DATA INGESTION: THE LATENCY COLLAPSE
**Verdict:** The DexScreener REST implementation is a "Time Machine" traveling backward.
**Analysis:**
The 400ms slot time on Solana dictates that any data source >50ms stale is historical data, not market data. DexScreener's aggregated REST API is suitable for UI dashboards, not MEV. The "phantom spreads" are a result of comparing PumpPortal's `subscribeTokenTrade` (Real-time) against DexScreener's cached state (T-60s).
**The Fix:**
We must sever the dependency on external HTTP APIs for price discovery immediately.
*   **Protocol:** Implement **Yellowstone gRPC** (Geyser plugin) to subscribe directly to the validator's replica stream. This bypasses the RPC JSON-RPC overhead entirely.
*   **Architecture:** Replace `scanner.rs` polling with a streaming architecture.
    *   **Source A:** Yellowstone `AccountUpdate` stream for Raydium/Orca AMM reserves.
    *   **Source B:** PumpPortal WebSocket (retained) for token creation/migration events.
    *   **Source C:** `subscribeTokenTrade` (PumpPortal) must be implemented to calculate real-time slippage depth.
*   **Consensus:** If Yellowstone is unavailable, fallback to Solana Websocket PubSub (`accountSubscribe`) with commitment level `processed`. REST polling is permanently deprecated.

### 2. JUPITER API: THE SEQUENTIAL LATENCY BOTTLENECK
**Verdict:** HTTP requests are for tourists. MEV operates on instruction serialization.
**Analysis:**
The 4-step sequential HTTP chain (Quote A -> Quote B -> Swap A -> Swap B) introduces ~1200ms of network round-trip latency. This is 3 full blocks. Furthermore, Jupiter's routing algorithm optimizes for *user* best price, not *atomic* circular arbitrage, often failing to find the return path or front-running the request.
**The Fix:**
We must bypass Jupiter's API entirely and construct instructions locally.
*   **Direct Instruction Assembly:** Implement direct instruction builders for the target DEXs (Raydium V4, Orca Whirlpools, Meteora DLMM).
    *   *Raydium:* Direct calls to `swap_base_in` using the AMM CPI.
    *   *Orca:* Utilize the Whirlpool client SDK to build `Swap` instructions locally without network calls.
*   **Quoting:** Maintain a local "Order Book" or "Pool State Cache" (updated via Yellowstone). Calculate the `amountOut` locally using the constant product formula (or stable-swap/CLMM math as appropriate).
*   **Latency Target:** <5ms for transaction construction.

### 3. MATHEMATICAL & SOLVER BUGS
**Verdict:** The solver is hallucinating profits due to dimensional analysis failure.
**Analysis:**
`math.rs` attempts to subtract Token Amounts from SOL Amounts ($P = \Delta_{SOL} - \Delta_{Token}$). This is physically meaningless. The AMM assumptions treat DLMM (Dynamic Liquidity Market Maker) as a CPMM, ignoring concentrated liquidity ranges, leading to massive slippage miscalculations.

**The Fix (LaTeX Specification):**

**3.1 Profit Calculation (`math.rs`):**
Profit must be calculated in the numéraire asset (SOL).
Let $A_{in}$ be the input SOL amount.
Let $A_{out}$ be the output SOL amount after the circular path.
$$ P_{net} = A_{out} - A_{in} - (F_{gas} + F_{priority}) $$
Where $A_{out}$ is derived from the final leg of the arb.
*Constraint:* $P_{net} > \epsilon$ (Profit Threshold, e.g., 10,000 lamports).

**3.2 AMM Output Calculation (`amm.rs`):**
Correcting the Constant Product Market Maker (CPMM) formula for Leg A (SOL -> Token) and Leg B (Token -> SOL).
Let $R_{SOL}$ and $R_{TOK}$ be reserves. Let $\gamma$ be the fee (e.g., 0.003).
For an input $\Delta_{in}$:
$$ \Delta_{out} = \frac{R_{out} \cdot \Delta_{in} \cdot \gamma}{R_{in} + \Delta_{in} \cdot \gamma} $$
*Correction:* The dossier noted the denominator omitted $\gamma$. The correct formula for the invariant $k = (R_{in} + \gamma \Delta_{in})(R_{out} - \Delta_{out})$ yields the above.

**3.3 Concentrated Liquidity (Meteora/Orca):**
For CLMMs, we cannot use CPMM.
$$ \Delta_{out} = \int_{P_{current}}^{P_{target}} L(p) \cdot dp $$
We must implement a "Tick Walker" that iterates through initialized ticks to calculate liquidity depth. If the price moves outside the current tick range, liquidity drops to zero (or jumps to next tick).
*Action:* Implement specific `calculate_swap` functions for `RaydiumV4`, `OrcaWhirlpool`, and `MeteoraDlmm`. Do not generalize.

### 4. ALT RESOLUTION & LEG B COUPLING
**Verdict:** The transaction is too fat to fly, and Leg B is blind to Leg A's reality.
**Analysis:**
1.  **MTU Failure:** 50-70 accounts exceed the IPv6 MTU (1232 bytes) without Address Lookup Tables (ALT). `Message::try_compile` fails because `&[]` is passed.
2.  **Coupling:** If Leg A swaps 1 SOL for 100M Tokens, but slippage yields 99.9M Tokens, and Leg B hardcodes 100M, Leg B fails with `InsufficientFunds`.

**The Fix:**
*   **ALT Implementation:**
    In `jito_builder.rs`, we must resolve ALTs.
    1.  Fetch the active `AddressLookupTableAccount` from the validator (or maintain a local cache).
    2.  Compile the transaction using `v0::Message::try_compile` passing the lookup table accounts.
    3.  This compresses 32-byte Pubkeys into 1-byte indices, allowing up to 256 accounts.
*   **Leg B Coupling:**
    Do not use `quote_res_a.outAmount` as a static input for Leg B.
    Use **Inner Instructions** or compute budget logic:
    *   Leg B input amount must be derived from the *actual* token balance change of the signer after Leg A execution.
    *   *Code Fix:* Set Leg B `amount_in` to `None` (for certain DEXs) or use the `expected_amount_out` from Leg A minus a safety buffer (slippage tolerance).
    *   *Robust Fix:* Use the "Balance Checker" pattern. Leg B takes `max(available_balance - dust_buffer)`. This ensures atomic success if $A_{out} > 0$.

### 5. SECURITY & MOCK ARTIFACTS ROADMAP
**Verdict:** The repository is a honeypot for the operator's own funds.

**Refactoring Roadmap:**

1.  **IMMEDIATE PURGE (T-0):**
    *   Delete `src/bin/reclaim_rent.rs`. Rotate the compromised private key immediately. The key is likely already scraped by bots scanning GitHub.
    *   Delete `dashboard/live_executor.js`. It is a one-way drain.
    *   Delete `paper_engine.rs`. It provides false confidence.

2.  **KEY MANAGEMENT (T+1hr):**
    *   Implement `solana-cli config get` or environment variable injection (`std::env::var`).
    *   Never commit `.env` files. Use `.env.template`.

3.  **EXECUTION ENGINE (T+24hr):**
    *   Replace `router.rs` mock strings with the **Local Instruction Builder** defined in Section 2.
    *   Implement a "Dry-Run" simulator using `solana-program-test` or `BanksClient` to validate math before mainnet deployment.

4.  **DASHBOARD (T+48hr):**
    *   Rebuild dashboard to listen to the Yellowstone stream for transaction confirmation, not a mock JS loop.

**COUNCIL OUTCOME:**
The system is currently a donation engine. By implementing Yellowstone gRPC, Local Instruction Assembly, and correct CLMM math, we convert it into a competitive MEV searcher. The timeline to profitability is contingent on the immediate rotation of the leaked private key and the deployment of the ALT-compliant transaction builder.