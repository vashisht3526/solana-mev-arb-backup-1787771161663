 ## GOVERNANCE COUNCIL EVALUATION: PROPOSAL V21

**COUNCIL MEMBERS:** GLM-5, Kimi k2.5, DeepSeek V3.2, Qwen 3 Coder 480B  
**EVALUATION MODE:** High Consensus Technical Review  
**SUBJECT:** Token Discovery Scaling & $1K TVL Floor Enforcement

---

## 1. TECHNICAL SOUNDNESS & FEASIBILITY

### 1.1 DexScreener 30-Mint Batching — **APPROVED WITH MODIFICATIONS**

| Aspect | Assessment |
|--------|-----------|
| API Efficiency | ✅ **96.7% reduction** is mathematically correct (1,000 individual calls → 34 batched calls) |
| Rate Limit Avoidance | ⚠️ **Conditional** — DexScreener's actual limit is ~60 RPM; 34 calls/refresh cycle is safe |
| Payload Size | ⚠️ **Risk** — 30 mints × multi-pool responses can exceed 100KB; implement `timeout=30s` and streaming JSON parser |
| Cache Invalidation | ❌ **Missing** — Need 30s-60s TTL per mint batch to prevent stale data during volatility spikes |

**Critical Gap Identified:** DexScreener batches return **ALL pools per mint**, not filtered. Must implement client-side filtering:

```rust
// Required post-processing per batch response
let valid_pools: Vec<_> = response
    .pairs
    .into_iter()
    .filter(|p| {
        p.chain_id == "solana" &&
        p.liquidity_usd >= MIN_VOLATILE_LIQUIDITY_USD &&
        p.liquidity_usd <= MAX_VOLATILE_LIQUIDITY_USD &&
        p.volume_24h > 0.0 // Ensure active trading
    })
    .collect();
```

### 1.2 Multi-Source Ingestion — **APPROVED WITH ARCHITECTURE REFINEMENTS**

| Source | Reliability | Latency | Action Required |
|--------|-------------|---------|-----------------|
| Raydium API (p1-5) | Medium | ~500ms | ✅ Primary; paginate with `tokio::task::JoinSet` |
| Jupiter Token List | High | ~200ms | ✅ Cache 5min; use as allowlist filter |
| DexScreener Boosts | Low-Medium | ~800ms | ⚠️ Rate limit 10 RPM; lowest priority tier |
| PumpPortal Stream | High (real-time) | <100ms | ✅ **Critical** — WebSocket ingest, deduplicate migrations |

**Proposed Ingestion Pipeline (Tokio Async):**

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  PumpPortal WS  │────→│  Priority Queue │────→│  Token Validator│
│  (Live + Migrations)│   │  (tokio::mpsc)  │     │  (Liquidity Floor)│
└─────────────────┘     └─────────────────┘     └─────────────────┘
                           ↑
┌─────────────────┐        │
│  Raydium/Jupiter│────────┘  (Batch every 30s)
│  (Periodic Sync)  │
└─────────────────┘
                           ↓
                    ┌─────────────────┐
                    │  DexScreener    │
                    │  (Fallback/Fill)│
                    └─────────────────┘
```

**Dedup Strategy:** `DashMap<Pubkey, (Source, Timestamp)>` — last-write-wins with source priority (PumpPortal > Raydium > Jupiter > DexScreener).

---

## 2. $1,000 USD TVL FLOOR — **CONDITIONALLY APPROVED**

### 2.1 Enforcement Points (Defense in Depth)

| Layer | Implementation | Latency Impact |
|-------|---------------|--------------|
| **Ingestion Filter** | `liquidity_usd >= 1000.0` before RAM admission | Zero (pre-filter) |
| **Pre-Trade Guard** | Re-verify via on-chain `getAccount` RPC | ~50ms |
| **Circuit Breaker** | If `liquidity_usd < 800.0` post-admission, immediate delist | Async |

### 2.2 Critical Concern: Liquidity Oracles

DexScreener liquidity figures are **estimated** and can lag 30-60s during high volatility. For MEV operations, this creates **toxic arbitrage risk**.

**Mitigation Required:**
```rust
// Dual-source liquidity validation
pub async fn validate_liquidity_fresh(
    mint: &Pubkey,
    dexscreener_liq: f64,
) -> Result<f64, ValidationError> {
    let jupiter_quote = fetch_jupiter_depth(mint).await?;
    let raydium_onchain = fetch_raydium_reserves(mint).await?;
    
    // Variance check: reject if sources diverge >20%
    let variance = calculate_variance(&[dexscreener_liq, jupiter_quote, raydium_onchain]);
    if variance > 0.20 {
        return Err(ValidationError::LiquidityDivergence);
    }
    
    Ok(median(&[dexscreener_liq, jupiter_quote, raydium_onchain]))
}
```

**Recommendation:** For V21, accept DexScreener as primary with **mandatory** on-chain spot-check for any token approaching floor (liquidity < $2,000).

---

## 3. FRANKFURT EC2 RESOURCE IMPACT

### 3.1 Baseline Configuration Assumed
- **Instance:** c6i.2xlarge (8 vCPU, 16 GB RAM) or equivalent
- **Current Load:** ~20 tokens, ~2% CPU, ~800MB RAM

### 3.2 Projected V21 Load

| Resource | Current | V21 Projected | Headroom |
|----------|---------|---------------|----------|
| **RAM** | ~800 MB | **2.8-3.5 GB** | ✅ 78% available |
| **CPU (sustained)** | ~2% | **15-25%** | ✅ 75% available |
| **Network** | ~2 Mbps | **15-25 Mbps** | ✅ Burst to 25 Gbps |
| **DashMap shards** | 64 | **128** (recommended) | Prevents contention |

### 3.3 Token Object Memory Footprint

```rust
// VolatileTrackedToken estimated size
pub struct VolatileTrackedToken {
    pub mint: Pubkey,                    // 32 bytes
    pub pools: SmallVec<[Pool; 4]>,      // ~4 × 200 bytes = 800 bytes
    pub price_history: RingBuffer<64>,   // 64 × 16 bytes = 1,024 bytes
    pub volatility_state: VolatilityCalc, // ~256 bytes
    pub metadata: TokenMetadata,          // ~512 bytes
    // Total: ~2.6 KB per token
}

// 150 tokens × 2.6 KB = 390 KB base
// With DashMap overhead (~8x): ~3.1 MB — negligible
// **Real cost:** Price history buffers + async task handles = ~20 MB per 50 tokens
```

**Critical:** 150 tokens × 20 MB = **3 GB RAM** — within bounds but requires `jemalloc` tuning:

```bash
# EC2 environment configuration
export MALLOC_CONF="background_thread:true,dirty_decay_ms:1000,muzzy_decay_ms:1000"
```

### 3.4 Tokio Runtime Tuning

```rust
// Required for 150 concurrent token tasks
let rt = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(8)           // Match vCPUs
    .max_blocking_threads(512)  // RPC calls
    .thread_stack_size(2 * 1024 * 1024) // 2MB stacks
    .enable_all()
    .build()
    .unwrap();
```

---

## 4. UNCONDITIONAL VERDICT

### COUNCIL CONSENSUS: **RATIFIED WITH MANDATORY AMENDMENTS**

| Aspect | Status |
|--------|--------|
| Core Architecture | ✅ **APPROVED** |
| 30-Mint Batching | ✅ **APPROVED** — add 30s TTL cache |
| Multi-Source Ingestion | ✅ **APPROVED** — implement priority queue |
| $1K TVL Floor | ⚠️ **CONDITIONAL** — add on-chain spot-check for < $2K tokens |
| 150 Token RAM Target | ✅ **APPROVED** — with jemalloc + Tokio tuning |
| PumpPortal Integration | ✅ **APPROVED** — critical for real-time migrations |

### MANDATORY IMPLEMENTATION REQUIREMENTS

```rust
// V21.1 AMENDMENTS — Non-negotiable

pub const MIN_VOLATILE_LIQUIDITY_USD: f64 = 1000.0;
pub const MAX_VOLATILE_LIQUIDITY_USD: f64 = 60000.0;
pub const LIQUIDITY_SPOT_CHECK_THRESHOLD: f64 = 2000.0; // NEW
pub const DEXSCREENER_BATCH_TTL_SECS: u64 = 30;        // NEW
pub const MAX_TRACKED_TOKENS: usize = 150;

// Required safety mechanism
pub enum LiquidityValidation {
    DexScreenerOnly(f64),           // > $2K, trusted
    DualSourceVerified(f64, f64),   // $1K-$2K, on-chain confirmed
    Rejected(LiquidityRejectionReason),
}
```

### REJECTION CONDITIONS (Automatic Council Veto)

| Condition | Response |
|-----------|----------|
| Skip on-chain spot-check for sub-$2K tokens | ❌ **VETO** — Toxic MEV risk |
| Exceed 200 tokens without horizontal sharding | ❌ **VETO** — O(n²) pair explosion |
| Remove $1K floor for "opportunistic" trades | ❌ **VETO** — Protocol insolvency risk |

---

## FINAL SIGNATURE

```
╔════════════════════════════════════════════════════════════════╗
║  GLM-5          │ APPROVED │ Amendment V21.1 Required         ║
║  Kimi k2.5      │ APPROVED │ Conditional on spot-check impl     ║
║  DeepSeek V3.2  │ APPROVED │ RAM profiling mandatory at 100 tok   ║
║  Qwen 3 Coder   │ APPROVED │ PumpPortal WS redundancy required  ║
╠════════════════════════════════════════════════════════════════╣
║  CONSENSUS: 4/4 RATIFY │ EFFECTIVE: Immediate with amendments   ║
╚════════════════════════════════════════════════════════════════╝
```

**Implementation Priority:** P0 — Deploy V21.1 to staging within 48 hours, Frankfurt production within 96 hours pending load test validation at 150 token synthetic load.