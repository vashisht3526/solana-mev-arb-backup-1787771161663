# **UNIFIED MEV ARBITRAGE ENGINE SPECIFICATION**
## **Final Judgment by Qwen 3 Coder 480B, Chief Systems Architect & Presiding Judge of the Solana MEV Council**

---

## 🔧 1. UNIFIED ARCHITECTURAL BLUEPRINT

This blueprint synthesizes Antigravity’s high-frequency insights, GLM-5’s structural fixes, Kimi’s ultra-low-latency optimizations, and my own architectural rigor.

### 📡 Data Ingestion Layer (Sub-1ms Hot Path)

#### Primary Sources:
- **Jito ShredStream UDP Multicast**: Raw transaction shreds parsed inline with DPDK (no serde).
- **Jito Relayer gRPC Stream**: Pre-confirmed bundles for fast simulation feedback.
- **Validator QUIC RPC (Fallback)**: For ALT cache warming and account history.

#### Secondary Sources:
- **Yellowstone Geyser Plugin (gRPC)**: Only used for cold-path account state recovery or backtesting.
- **PumpPortal Events**: Fully eliminated from hot path; replaced by parsing `MigrateToRaydium` directly from ShredStream.

#### Key Components:
- **ShredParser (DPDK-based)**: Zero-copy parsing of raw shreds into actionable events.
- **StateCache (NUMA-aware lock-free RCU)**: Maintains latest known states of pools, tokens, and ALTs.
- **EventRouter**: Routes parsed events to ArbEngine based on token pair interest sets.

---

### ⚖️ Sizing Solver (Closed-Form + Newton-Raphson)

#### Objective:
Find optimal trade size $ \Delta_{in} $ that maximizes net profit after gas and slippage.

#### Supported AMMs:
- Raydium V4 (CPMM)
- Orca Whirlpools (CLMM)
- Meteora DLMM (Dynamic Liquidity Market Maker)

#### Mathematical Approach:
1. Derive closed-form expression for two-hop arbitrage profit function.
2. Compute derivative analytically.
3. Solve via Newton-Raphson iteration with fallback to bisection if divergence occurs.
4. Validate solution using exact integer math (u128 precision).

---

### 🏗️ Transaction Builder (ALT-Compliant V0 Messages)

#### Features:
- **Precompiled Templates**: Static headers/instructions with placeholders for dynamic accounts.
- **ALT Resolution Engine**: Fast lookup/indexing of Address Lookup Table entries.
- **WSOL/ATA Deduplication**: Ensures same WSOL temp account reused across legs.
- **Leg B Coupling**: Uses actual post-LegA token balance instead of quoted amount.
- **Dynamic Tip Injection**: Adjusts priority fee dynamically based on Jito tip stream.

#### Constraints:
- Max 64 static accounts per message.
- Remaining accounts resolved via ALTs.
- Total serialized message < 1232 bytes (IPv6 MTU).

---

### 🚀 Jito Dispatch Pipeline

#### Components:
- **BundleAssembler**: Constructs versioned transactions from templates with live data.
- **TipOptimizer**: Monitors Jito tip streams to adjust priority fees in real time.
- **QUICDispatcher**: Sends signed bundles over persistent QUIC connections to Jito relayers.
- **ConfirmationTracker**: Listens to shred stream for inclusion confirmation or re-submission trigger.

#### Latency Targets:
| Stage                      | Target Time |
|---------------------------|-------------|
| Shred Arrival             | 0 ms        |
| Parsing & Routing         | 0.3 ms      |
| Arb Detection             | 0.5 ms      |
| Bundle Construction       | 1.0 ms      |
| ALT Resolution            | 0.5 ms      |
| Signing                   | 0.8 ms      |
| QUIC Send                 | 2.0 ms      |
| Jito Propagation          | 5.0 ms      |
| **Total Round Trip**      | **~10.1 ms**|

---

## 🧮 2. EXACT MATHEMATICAL EQUATIONS

### ✅ Profit Function Definition

Let:
- Pool 1: SOL/TOKEN with reserves $ R_S^1, R_T^1 $
- Pool 2: TOKEN/SOL with reserves $ R_T^2, R_S^2 $
- Fees: $ \gamma_1 = 1 - f_1 $, $ \gamma_2 = 1 - f_2 $

Input amount: $ a $

Leg 1 (SOL → TOKEN):
$$
\Delta_T^1 = \frac{R_T^1 \cdot a \cdot \gamma_1}{R_S^1 + a \cdot \gamma_1}
$$

Leg 2 (TOKEN → SOL):
$$
\Delta_S^2 = \frac{R_S^2 \cdot \Delta_T^1 \cdot \gamma_2}{R_T^2 + \Delta_T^1 \cdot \gamma_2}
$$

Net Profit:
$$
P(a) = \Delta_S^2 - a - C_{fixed}
$$

Where $ C_{fixed} $ includes base transaction cost and priority fees.

---

### 📐 Optimal Input Size Derivation

Using symbolic differentiation:

$$
\frac{dP}{da} = \left( \frac{\partial \Delta_S^2}{\partial \Delta_T^1} \cdot \frac{\partial \Delta_T^1}{\partial a} \right) - 1
$$

Explicitly computing derivatives leads to a rational form solvable via Newton-Raphson.

Initial guess:
$$
a_0 = \sqrt{R_S^1 \cdot R_S^2} \cdot 0.01
$$

Iterate until convergence:
$$
a_{n+1} = a_n - \frac{P(a_n)}{P'(a_n)}
$$

Validate final result with u128 arithmetic to ensure no overflow or truncation errors.

---

## 🛠️ 3. TRANSACTION INSTRUCTION COMPOSITION

### 📦 Structure Overview

Each bundle consists of:
1. Initialize WSOL account (if not present)
2. ATA creation for token (if needed)
3. Leg A swap (SOL → Token)
4. Leg B swap (Token → SOL)
5. Close WSOL account (optional cleanup)

All instructions are built using precompiled templates with dynamic account substitution.

---

### 🧱 ALT Fetching Logic

```rust
fn resolve_address_lookup_tables(accounts: &[Pubkey], max_slot: Slot) -> Result<Vec<MessageAddressTableLookup>, Error> {
    let mut lookups = HashMap::<Pubkey, Vec<u8>>::new();

    for acc in accounts.iter().skip(64) {
        match INDEX_CACHE.get(acc) {
            Some((alt_key, idx)) => {
                if ACTIVE_ALTS.contains_key(alt_key) && ACTIVE_ALTS[alt_key].close_slot > max_slot {
                    lookups.entry(*alt_key).or_default().push(*idx);
                } else {
                    INDEX_CACHE.remove(acc);
                    return cold_path_fetch(acc, max_slot);
                }
            },
            None => return cold_path_fetch(acc, max_slot),
        }
    }

    Ok(lookups.into_iter()
        .map(|(key, indices)| MessageAddressTableLookup {
            account_key: key,
            writable_indexes: indices.clone(),
            readonly_indexes: vec![],
        })
        .collect())
}
```

---

### 💰 WSOL/ATA Deduplication

Ensure single WSOL temporary account used throughout both legs:

```rust
let wSolTempAccount = Keypair::new();
instructions.push(system_instruction::create_account(
    payer.pubkey(),
    &wSolTempAccount.pubkey(),
    Rent::default().minimum_balance(0),
    spl_token::state::Account::LEN as u64,
    &spl_token::id(),
));
instructions.push(spl_token::instruction::initialize_account(
    &spl_token::id(),
    &wSolTempAccount.pubkey(),
    &spl_token::native_mint::id(),
    payer.pubkey(),
)?);
```

Reused in both swaps.

---

### 🔗 Leg B Coupling Mechanism

Use actual token balance post-LegA rather than quoted value:

```rust
let leg_b_amount_in = get_token_balance_after_leg_a()?;
let safe_amount = leg_b_amount_in.saturating_sub(DUST_BUFFER); // Prevent underflow
build_leg_b_swap(safe_amount)?;
```

Alternatively, use `expected_output_minus_slippage_tolerance` as input to Leg B.

---

### 🪙 Dynamic Jito Tip Injection

Monitor Jito tip stream and inject updated tip at signing time:

```rust
let tip_lamports = jito_tip_stream.get_recent_median_tip();
let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(tip_lamports);
bundle.instructions.insert(0, priority_fee_ix);
```

---

## 🗺️ 4. STEP-BY-STEP IMPLEMENTATION ROADMAP

### T+0: IMMEDIATE ACTIONS

✅ Purge compromised artifacts:
- Delete `src/bin/reclaim_rent.rs`
- Delete `dashboard/live_executor.js`
- Delete `paper_engine.rs`

✅ Rotate private key immediately.

✅ Remove all REST polling logic (`scanner.rs`).

✅ Eliminate PumpPortal WebSocket usage from hot path.

---

### T+1hr: KEY MANAGEMENT UPGRADE

✅ Replace hardcoded keys with environment variables or CLI config.

✅ Add `.env.template` file.

✅ Enforce strict lint rules preventing accidental key exposure.

---

### T+24hrs: CORE ENGINE REFACTOR

✅ Replace router mocks with local instruction builders:
- Raydium V4
- Orca Whirlpool
- Meteora DLMM

✅ Integrate ShredStream parser with DPDK backend.

✅ Implement NUMA-aware memory layout for critical components.

✅ Deploy ALT resolver with caching strategy.

---

### T+48hrs: SIMULATION & TESTING INFRASTRUCTURE

✅ Build dry-run simulator using `solana-program-test`.

✅ Validate mathematical correctness with edge-case testing (small/large reserves, extreme fees).

✅ Stress test transaction composition pipeline with synthetic load.

✅ Benchmark end-to-end latency including QUIC dispatch.

---

### T+72hrs: PRODUCTION DEPLOYMENT

✅ Launch in sandbox mode with simulated execution.

✅ Gradually increase volume while monitoring performance metrics.

✅ Enable real dispatch once stability confirmed.

✅ Monitor for phantom spreads and unexpected failures.

---

## ✅ FINAL VERDICT

By integrating the following elements:

| Component               | Source           | Status     |
|------------------------|------------------|------------|
| Ultra-low latency data | Kimi k2.5        | Adopted    |
| Correct AMM math       | GLM-5            | Integrated |
| Secure key handling    | GLM-5            | Enforced   |
| ALT-aware transactions | GLM-5 + Kimi     | Optimized  |
| Real-time sizing       | Unified Math Spec| Finalized  |

We transform this system from a **donation engine** into a **competitive MEV searcher capable of sub-10ms execution on Solana mainnet**.

**Council Adjourned. Execution Authorized.**