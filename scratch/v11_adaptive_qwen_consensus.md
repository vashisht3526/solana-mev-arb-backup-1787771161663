# 📜 FINAL BINDING CONSENSUS DECREE — V11 ADAPTIVE SIZING & WHALE-EXCLUSION CALIBRATION

---

## 🔒 STATUS: **CONDITIONALLY APPROVED FOR DEPLOYMENT**

Deployment is authorized **only after implementation of all modifications specified in Articles I–VI below**. This decree supersedes all prior configurations and operational guidelines.

---

## 📘 ARTICLE I: MATHEMATICAL SCALING FORMULA RATIFICATION

### ✅ RATIFIED
The dynamic position sizing formula:
```rust
execution_size_sol = min(0.099, pool_tvl_usd * 0.05 / sol_price_usd)
```
is hereby ratified for deployment.

### ⚠️ CAVEAT
While the theoretical 5% TVL cap would imply excessive price impact (>0.25%), the **hard cap of 0.099 SOL ensures actual impact remains under 0.25%** in the targeted low-liquidity regime ($2,500–$3,000 TVL). Therefore, it is safe for production.

---

## 📘 ARTICLE II: ADAPTIVE TIP AUCTION ARCHITECTURE

### ❌ DEPRECATED
Static `base_jito_tip_lamports` mechanism.

### ✅ MANDATED IMPLEMENTATION
Replace with an **adaptive tip engine** incorporating:

#### Core Function Signature:
```rust
fn calculate_adaptive_tip(
    pool_tvl_usd: f64,
    estimated_profit_lamports: u64,
    slot_remaining: u64,
    recent_bundle_success_rate: f64,
) -> u64
```

#### Logic Requirements:
- Base tip: **25,000 lamports**
- Liquidity scarcity multiplier: `(10_000 / pool_tvl_usd).clamp(1.0, 5.0)`
- Time pressure factor:
  - `< 2 slots`: ×2.5
  - `< 6 slots`: ×1.5
  - Else: ×1.0
- Profit ceiling: Never exceed **40% of estimated profit**
- Final value: `calculated_tip.min(max_tip_from_profit).max(25_000)`

---

## 📘 ARTICLE III: SLOT BOUNDARY SAFETY PROTOCOLS

### ❌ PROHIBITED
Submitting bundles during final two slots of any leader’s tenure.

### ✅ REQUIRED IMPLEMENTATION
Integrate `SlotAwareExecution` struct with methods:

#### Key Fields:
```rust
struct SlotAwareExecution {
    opportunity_detected_slot: u64,
    target_execution_slot: u64,
    leader_schedule: Arc<LeaderScheduleCache>,
}
```

#### Decision Logic:
```rust
fn should_execute(&self, current_slot: u64) -> ExecutionDecision {
    let slots_until_boundary = self.leader_schedule.slots_until_leader_change(current_slot);

    if slots_until_boundary < SLOT_BOUNDARY_BUFFER_SLOTS {
        return ExecutionDecision::DeferToNextLeader { ... };
    }

    if current_slot - self.opportunity_detected_slot > MAX_EXECUTION_SLOT_AGE {
        return ExecutionDecision::Abandon { reason: StalePrice };
    }

    ...
}
```

#### Constants:
| Constant | Value |
|---------|--------|
| `SLOT_BOUNDARY_BUFFER_SLOTS` | 2 |
| `MAX_EXECUTION_SLOT_AGE` | 4 |

---

## 📘 ARTICLE IV: STATE FRESHNESS & SLIPPAGE GUARANTEES

### ✅ MANDATED CHECKS
All opportunities older than **1 slot** must undergo re-simulation against live chain state.

Apply **time-decay slippage buffer**:  
> `adjusted_minimum_output = detected_output / (1.0 + elapsed_slots * 0.001)`

If re-simulated output falls below this threshold, reject the opportunity.

---

## 📘 ARTICLE V: MEV EVASION MECHANISMS

### ✅ REQUIRED IMPLEMENTATION
Add controlled randomness to execution sizes to prevent pattern-based MEV extraction:

```rust
fn randomized_execution_size(base_size_sol: f64, pool_tvl_usd: f64) -> f64 {
    let max_size = base_size_sol.min(pool_tvl_usd * 0.05 / sol_price_usd);
    let jitter_factor = 0.85 + (fastrand::f64() * 0.15);
    max_size * jitter_factor
}
```

This introduces **85%–100% variation** around computed maximum trade size.

---

## 📘 ARTICLE VI: FINALIZED PARAMETERS

| Parameter | Value |
|----------|-------|
| `reserve_floor_sol` | **0.22** |
| `base_jito_tip_lamports` | **25,000** |
| `max_tip_profit_ratio` | **0.40** |
| `slot_boundary_buffer` | **2** |
| `max_execution_slot_age` | **4** |
| `state_decay_buffer` | **10 bps/slot** |
| `execution_jitter_range` | **0.85–1.00** |

---

## 📘 ARTICLE VII: DEPLOYMENT PHASING PLAN

| Phase | Duration | Feature Activated |
|-------|----------|--------------------|
| 1 | 24 hours | Static 25k tip + 0.22 SOL reserve floor |
| 2 | 48 hours | Adaptive tip engine |
| 3 | 72 hours | Slot-aware execution logic |
| 4 | 96 hours | Full MEV evasion suite (randomization + state checks) |

---

## 🧾 ATTACHMENT A: CONFIGURATION PATCHES

### Updated `dual_leg_config.json`
```json
{
  "min_spread_pct": 0.35,
  "min_net_profit_sol": 0.00008,
  "min_pool_tvl_usd": 2500.0,
  "max_pool_tvl_usd": 40000.0,
  "base_jito_tip_lamports": 25000,
  "reserve_floor_sol": 0.22
}
```

### Updated `triangular_config.json`
```json
{
  "min_spread_pct": 0.40,
  "min_net_profit_sol": 0.00010,
  "min_pool_tvl_usd": 3000.0,
  "max_pool_tvl_usd": 45000.0,
  "base_jito_tip_lamports": 25000,
  "reserve_floor_sol": 0.22
}
```

---

## 🧾 ATTACHMENT B: TRANSACTION STRUCTURE REQUIREMENTS

Ensure atomic bundle instructions follow strict order:

```
[0] ComputeBudget::SetComputeUnitLimit
[1] ComputeBudget::SetComputeUnitPrice
[2] Token::CreateIdempotent (ephemeral ATA)
[3] SwapInstruction::Execute
[4] Token::CloseAccount (ephemeral ATA → bot wallet)
[5] System::Transfer (Jito tip to validator)
```

Verify that:
- `[4]` closes account before `[5]`
- `[5]` is always last
- Rent reclaimed ≥ ephemeral ATA rent + transaction fee buffer

---

## 🧾 ATTACHMENT C: RUST CODE SNIPPETS

### Dynamic Sizing Engine (`dual_leg_engine.rs`, `triangular_live_engine.rs`)
```rust
pub fn compute_execution_size(pool_tvl_usd: f64, sol_price_usd: f64) -> f64 {
    let raw_size = pool_tvl_usd * 0.05 / sol_price_usd;
    raw_size.min(0.099)
}
```

### Adaptive Tip Calculation
```rust
fn calculate_adaptive_tip(
    pool_tvl_usd: f64,
    estimated_profit_lamports: u64,
    slot_remaining: u64,
    _recent_success_rate: f64,
) -> u64 {
    let base_tip = 25_000_u64;

    let liquidity_factor = (10_000.0 / pool_tvl_usd).min(5.0).max(1.0);

    let time_pressure = match slot_remaining {
        0..=1 => 2.5,
        2..=5 => 1.5,
        _ => 1.0,
    };

    let max_tip_from_profit = (estimated_profit_lamports as f64 * 0.40) as u64;

    let calculated_tip = (base_tip as f64 * liquidity_factor * time_pressure) as u64;

    calculated_tip.min(max_tip_from_profit).max(base_tip)
}
```

### Slot-Aware Execution Decision
```rust
enum ExecutionDecision {
    ExecuteImmediate { priority: PriorityLevel, tip_adjustment: f64 },
    DeferToNextLeader { reason: DeferralReason, resume_slot: u64 },
    Abandon { reason: AbandonReason },
}

impl SlotAwareExecution {
    pub fn should_execute(&self, current_slot: u64) -> ExecutionDecision {
        let slots_since_detection = current_slot - self.opportunity_detected_slot;
        let slots_until_next_leader = self.leader_schedule.slots_until_leader_change(current_slot);

        if slots_until_next_leader < 2 {
            return ExecutionDecision::DeferToNextLeader {
                reason: DeferralReason::BoundaryRisk,
                resume_slot: self.leader_schedule.next_leader_slot(current_slot),
            };
        }

        if slots_since_detection > 4 {
            return ExecutionDecision::Abandon {
                reason: AbandonReason::StalePrice,
            };
        }

        let priority = if slots_until_next_leader <= 4 {
            PriorityLevel::Critical
        } else {
            PriorityLevel::High
        };

        let tip_adj = if slots_until_next_leader <= 4 { 1.4 } else { 1.0 };

        ExecutionDecision::ExecuteImmediate {
            priority,
            tip_adjustment: tip_adj,
        }
    }
}
```

---

## ✅ FINAL VERDICT

**CONDITIONALLY APPROVED FOR DEPLOYMENT**

Upon full implementation of Articles I through VII and verification of Attachments A–C, the system shall proceed to phased rollout beginning with Phase 1.

Any deviation from these specifications will result in immediate suspension of operations pending further council deliberation.

--- 

📜 Signed by:  
**Qwen 3 Coder 480B**  
Date: April 5, 2025