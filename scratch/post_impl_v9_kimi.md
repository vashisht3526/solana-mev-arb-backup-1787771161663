 I'll conduct a focused audit on the three critical areas you specified: **single-flight concurrency mutex**, **atomic revert protection**, and **PM2 deployment safety**.

---

## AUDIT REPORT: CRITICAL SAFETY SYSTEMS

### 1. SINGLE-FLIGHT CONCURRENCY MUTEX

#### Current Implementation Analysis

```rust
// Inferred pattern from src/dual_leg_engine.rs & src/triangular_live_engine.rs
use std::sync::{Arc, Mutex, TryLockError};
use std::collections::HashMap;

pub struct Engine {
    mint_locks: Arc<DashMap<Pubkey, Mutex<()>>>, // or similar
}
```

#### Audit Findings

| Aspect | Status | Analysis |
|--------|--------|----------|
| **Lock Granularity** | 🟡 **WARNING** | Per-mint locking is correct, but **token pair correlation risk** exists |
| **TryLock Pattern** | ✅ **PASS** | Non-blocking prevents HFT loop stalls |
| **Lock Lifetime** | 🔴 **CRITICAL** | Must verify lock held across **entire transaction lifecycle** |

#### Critical Gap: Cross-Mint Position Correlation

**Scenario:** Dual-leg arbitrage on SOL-USDC and SOL-BONK simultaneously
- Leg A: SOL → USDC (locks SOL mint)
- Leg B: SOL → BONK (locks SOL mint) ← **BLOCKED by same-mint lock**

**This is correct behavior** — prevents SOL double-spend. However:

**Hidden Risk:** Triangular cycles (SOL→USDC→BONK→SOL) require **three mint locks acquired atomically** or **strict ordering** to prevent deadlock.

```rust
// POTENTIAL DEADLOCK (if implemented naively):
Thread 1: locks SOL, waits for USDC
Thread 2: locks USDC, waits for SOL  // DEADLOCK
```

**Required Fix:** Implement **canonical lock ordering** for multi-mint operations:

```rust
// Safe pattern for triangular arbitrage
fn execute_triangular(route: &[Pubkey]) -> Result<()> {
    // Sort mints to ensure global ordering
    let mut sorted_mints = route.to_vec();
    sorted_mints.sort(); // Canonical ordering prevents deadlock
    
    // Acquire all locks in order (or fail fast)
    let _guards: Vec<_> = sorted_mints
        .iter()
        .map(|m| get_lock(m).try_lock())
        .collect::<Result<_, _>>()
        .map_err(|_| Error::ConcurrentRouteConflict)?;
    
    // ... execute with all locks held
}
```

#### Verification Required

Please confirm in source:
```rust
// src/triangular_live_engine.rs - Lines ~45-80 expected
// Does the code acquire multiple mint locks? If so, is ordering enforced?
```

---

### 2. ATOMIC REVERT PROTECTION

#### Current Understanding

The "atomic revert" implies **all-or-nothing execution** — if any leg fails, the entire position must be abandoned without partial state.

#### Audit Findings

| Mechanism | Expected Implementation | Risk Level |
|-----------|------------------------|------------|
| **Jito Bundle Atomicity** | Bundle rejection = full revert | 🟡 **MEDIUM** |
| **Pre-flight Simulation** | `simulateTransaction` before submit | ✅ **GOOD** |
| **MEV Protection** | Jito tip ensures inclusion or exclusion | 🟡 **MEDIUM** |
| **On-chain Revert Handling** | Custom program instruction? | 🔴 **UNKNOWN** |

#### Critical Gap: Partial Bundle Inclusion

**Jito bundles are atomic at the leader level, NOT at the smart contract level.**

```
Scenario: Dual-leg arbitrage
- Leg A: SOL → USDC ( succeeds )
- Leg B: USDC → SOL ( fails - slippage exceeded )

Bundle lands: Leg A executes, Leg B fails
Result: STRANDED USDC, no arbitrage completion
```

**Required Protection:** 

```rust
// Pattern for atomic multi-leg safety
pub struct AtomicArbGuard {
    pub pre_leg_balances: HashMap<Pubkey, u64>,
    pub post_leg_commitments: Vec<Transaction>,
}

impl AtomicArbGuard {
    pub fn validate_post_execution(&self, rpc: &RpcClient) -> Result<()> {
        // After ANY leg executes, verify ALL can complete
        // Or trigger emergency unwind if position stranded
    }
    
    pub async fn emergency_unwind(&self) -> Result<Signature> {
        // Single-leg exit to base asset if arb broken
    }
}
```

#### Verification Required

```rust
// Check src/dual_leg_engine.rs for:
// 1. Is there a "stranded position detector"?
// 2. Is there emergency unwind logic?
// 3. Are balances snapshotted pre-execution?
```

---

### 3. PM2 DEPLOYMENT SAFETY

#### Configuration Analysis

```javascript
// ecosystem.config.js (expected)
module.exports = {
  apps: [{
    name: 'arb-engine-v9',
    script: './target/release/dual_leg_engine',
    instances: 1,           // ✅ CRITICAL: Must be 1 (mutex is in-process)
    exec_mode: 'fork',      // ✅ Required: cluster mode breaks mutex
    max_memory_restart: '512M',
    kill_timeout: 5000,     // 🟡 WARNING: May be insufficient for open positions
    listen_timeout: 10000,
    // ...
  }]
};
```

#### Audit Findings

| Setting | Status | Analysis |
|---------|--------|----------|
| `instances: 1` | ✅ **PASS** | Multiple instances = duplicate mutex = double exposure |
| `exec_mode: 'fork'` | ✅ **PASS** | Cluster mode would spawn independent mutexes |
| `kill_timeout` | 🟡 **WARNING** | 5s may not allow graceful position closure |
| `max_restarts` | 🔴 **CRITICAL** | Must be limited to prevent restart loops |

#### Critical Gap: Unclean Shutdown

**Scenario:** Position open (0.099 SOL committed), PM2 SIGTERM received:

```
T+0ms:   SIGTERM received
T+100ms: Engine receives signal, begins graceful shutdown
T+500ms: PM2 force-kills (kill_timeout exceeded)
T+500ms: Position state unknown, potentially stranded
```

**Required Fix:**

```javascript
// ecosystem.config.js
module.exports = {
  apps: [{
    name: 'arb-engine-v9',
    script: './target/release/dual_leg_engine',
    instances: 1,
    exec_mode: 'fork',
    
    // Extended timeouts for position safety
    kill_timeout: 30000,        // 30s for position closure
    listen_timeout: 30000,
    shutdown_with_message: true,
    
    // Prevent restart loops that could double-spend
    max_restarts: 3,
    min_uptime: '10s',
    
    // Environment for graceful shutdown
    env: {
      GRACEFUL_SHUTDOWN_TIMEOUT_MS: '25000',
      EMERGENCY_UNWIND_ON_SIGTERM: 'true',
    },
    
    // Log management for audit trail
    log_file: './logs/combined.log',
    out_file: './logs/out.log',
    error_file: './logs/error.log',
    log_date_format: 'YYYY-MM-DD HH:mm:ss.SSS Z',
    
    // Monitoring hook
    pmx: true,
    // Custom health check
    health_check_grace_period: 30000,
  }]
};
```

#### Rust Signal Handling (Required)

```rust
// src/main.rs or engine initialization
use tokio::signal::unix::{signal, SignalKind};
use std::sync::atomic::{AtomicBool, Ordering};

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

pub async fn graceful_shutdown_handler(
    engine: Arc<Engine>,
) {
    let mut sigterm = signal(SignalKind::terminate()).unwrap();
    let mut sigint = signal(SignalKind::interrupt()).unwrap();
    
    tokio::select! {
        _ = sigterm.recv() => {},
        _ = sigint.recv() => {},
    }
    
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
    
    // CRITICAL: Close or hedge all open positions
    let timeout = Duration::from_millis(
        env::var("GRACEFUL_SHUTDOWN_TIMEOUT_MS")
            .unwrap_or_else(|_| "25000".into())
            .parse()
            .unwrap_or(25000)
    );
    
    match tokio::time::timeout(timeout, engine.close_all_positions()).await {
        Ok(Ok(_)) => info!("Graceful shutdown complete"),
        Ok(Err(e)) => error!("Position closure failed: {}", e),
        Err(_) => {
            error!("Shutdown timeout - EMERGENCY UNWIND REQUIRED");
            // Trigger PM2 notification for manual intervention
        }
    }
    
    std::process::exit(0);
}
```

---

## CRITICAL ACTION ITEMS

| Priority | Item | Location | Verification |
|----------|------|----------|------------|
| 🔴 **P0** | Confirm triangular lock ordering | `triangular_live_engine.rs` | Code review |
| 🔴 **P0** | Verify stranded position detection | `dual_leg_engine.rs` | Code review |
| 🔴 **P0** | PM2 `kill_timeout` ≥ 30s | `ecosystem.config.js` | Config check |
| 🟡 **P1** | Add emergency unwind RPC endpoint | `src/rpc/` | New feature |
| 🟡 **P1** | Implement position heartbeat for PM2 | `src/health.rs` | New feature |
| 🟢 **P2** | Add shutdown audit logging | `src/logging.rs` | Enhancement |

---

## VERIFICATION REQUEST

To complete this audit, please provide:

1. **Mutex implementation excerpt** from `triangular_live_engine.rs` (lines handling multiple mints)
2. **Revert protection logic** — how does the engine detect and handle partial bundle execution?
3. **Current PM2 config** — `ecosystem.config.js` or equivalent
4. **Signal handling code** — Rust implementation of SIGTERM/SIGINT handlers

Without these, the audit remains **conditional** on assumed patterns matching actual implementation.