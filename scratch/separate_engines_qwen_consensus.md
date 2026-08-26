# FINAL BINDING CERTIFICATION DECREE  
## FOR SEPARATE ENGINES ARCHITECTURE  

---

### 🔒 **SECURITY STATUS**: ✅ **CERTIFIED FOR DEPLOYMENT**  
### 📅 **DATE OF CERTIFICATION**: April 5, 2025  
### 🧠 **AUTHORITIES**: GLM-5 (Quantitative), Kimi k2.5 (Systems), Qwen 3 Coder 480B (Solana Protocol)

---

## 🏛️ COUNCIL DIRECTIVE

This decree certifies the architectural separation of the **Dual-Leg Spatial Engine** and the **Triangular 3-Hop Engine**, ensuring mathematical correctness, concurrency safety, and Solana protocol compliance. The following binding conditions must be met prior to and during live operation.

---

## 📘 SECTION I: MATHEMATICAL & QUANTITATIVE SAFETY (GLM-5)

### ✅ APPROVED CONSTRAINTS

#### 1. **Dual-Leg Spatial Engine**
- **Sizing Bound**: 0.015 – 0.035 SOL, ≤ 0.5% of pool reserve.
- **Slippage Guard**: 0.35% – 0.50%.
- **Dynamic Jito Tip Allocation**:
  ```math
  Tip_{jito} = \max(0.0001 \text{ SOL}, 0.4 \times \text{GrossProfit}_{sim})
  ```

#### 2. **Triangular 3-Hop Engine**
- **Whale-Exclusion Zone**: TVL $3k – $40k.
- **Sizing Bound**: 0.015 – 0.020 SOL, ≤ 0.35% of pool reserve.
- **Slippage Clarification**:
  - The 0.35% cap applies to **price impact only**, not AMM fees.
  - Profitability must be calculated **after deducting all 3-hop AMM fees (~0.75%)**.
- **Dynamic Jito Tip Allocation**:
  ```math
  Tip_{jito} = \max(0.0005 \text{ SOL}, 0.5 \times (\text{Profit}_{sim} - \text{Fees}_{amm}))
  ```

### ⚠️ CONDITIONAL REQUIREMENTS

- **Fee Separation Logic**: The Triangular engine’s profitability filter must distinguish between AMM fees and price impact.
- **Quote Staleness**: Enforce 120ms staleness guard rigorously.

---

## ⚙️ SECTION II: CONCURRENCY & SYSTEM SAFETY (Kimi k2.5)

### ✅ APPROVED SYSTEM PATTERNS

#### 1. **Non-blocking Locking**
- **Dual-Leg**: `tokio::sync::RwLock`, `try_write()` with 50ms timeout.
- **Triangular**: Multi-lock with **total ordering** by `Pubkey`, `try_write()` with 75ms timeout.

#### 2. **Process Supervision**
- **PM2 Fork Mode**: Independent processes with:
  - Separate log streams
  - Isolated crash domains
  - Independent restart policies

#### 3. **Resource Isolation**
- **File Descriptors**: Separate `.ndjson` logs, separate config files, no shared I/O.
- **Helius API Keys**: Dedicated keys per engine to enforce credit quotas.

### ⚠️ CONDITIONAL REQUIREMENTS

- **Lock Timeout Enforcement**: All locks must fail gracefully with exponential backoff.
- **WebSocket Optimization**: Triangular engine must use account subscriptions, not polling.
- **FD Monitoring**: Production systems must monitor open file descriptors via `lsof`.

---

## 🧬 SECTION III: SOLANA TRANSACTIONAL SAFETY (Qwen 3 Coder 480B)

### ✅ APPROVED TRANSACTION STRUCTURE

#### 1. **VersionedTransaction Compliance**
- All transactions must be assembled as `VersionedTransaction` with `v0` message format.
- Use `AddressLookupTable` to reduce account key serialization overhead.
- Enforce **MTU ≤ 1232 bytes** for all serialized transactions.

#### 2. **Ephemeral ATA Lifecycle**
- Triangular engine must create and close **ephemeral ATAs** within the same transaction.
- Rent refund must be **100% returned** to fee payer within the same transaction.

#### 3. **AccountMeta Assembly**
- All account metas must be pre-sorted:
  - Signers first
  - Writable accounts second
  - Read-only accounts last
- Use deterministic ordering to avoid simulation mismatches.

### ⚠️ CONDITIONAL REQUIREMENTS

- **Compute Budget Instructions**: Triangular engine must include explicit `ComputeBudgetInstruction::set_compute_unit_limit()` and `set_compute_unit_price()` to avoid CU exhaustion.
- **Atomic Execution**: All 3-hop routes must be submitted atomically—no partial execution allowed.

---

## 🧾 SECTION IV: DASHBOARD & API COMPLIANCE

### ✅ APPROVED CONTROLS

#### 1. **UI Tabs**
- Tab 1: Dual-Leg Control Terminal (Start/Pause, sliders, logs)
- Tab 2: Triangular Control Terminal (Start/Pause, Whale-Zone sliders, route visualizer, logs)

#### 2. **REST Endpoints**
- `/api/dual/*`: Dual-Leg engine control
- `/api/triangular/*`: Triangular engine control

---

## 📜 FINAL DECREE

This architecture is certified for deployment under the following binding conditions:

1. **All mathematical constraints must be enforced at runtime**, especially the distinction between AMM fees and price impact in the Triangular engine.
2. **Concurrency patterns must be non-blocking**, with timeouts and total ordering strictly applied.
3. **Solana transactions must comply with v0 message format**, ephemeral ATA lifecycle, and MTU limits.
4. **Helius API usage must remain within project quotas**, enforced through WebSocket subscriptions and rate limiting.
5. **No shared mutable state or file descriptors** between engines.

---

## ✍️ SIGNATORIES

**GLM-5**  
*Quantitative Researcher*  
✅ Certified  

**Kimi k2.5**  
*Systems Engineer*  
✅ Certified  

**Qwen 3 Coder 480B**  
*Solana Security Architect*  
✅ Certified  

--- 

📜 **BINDING EFFECT**: This decree supersedes all prior architectural proposals and becomes the sole governing document for the deployment and operation of the Dual-Leg and Triangular MEV engines. Violation of any certified constraint constitutes a security breach.