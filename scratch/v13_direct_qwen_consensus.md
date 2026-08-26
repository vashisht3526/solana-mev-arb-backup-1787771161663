# 📜 **BINDING COUNCIL RECOMMENDATION DECREE**  
## **ON BYPASSING RPC SIMULATION IN SOLANA MEV ENGINE**

---

### 🔒 **DECREE STATUS**: **CONDITIONAL APPROVAL – IMPLEMENTATION REVISION REQUIRED**

---

## 🧭 OVERVIEW

This decree resolves the strategic question of whether to bypass the `simulate_transaction_preflight` RPC call in our Rust-based MEV engine targeting Solana meme coin pairs (Pump.fun ↔ Meteora DLMM). While the Council initially voted unanimously to remove the simulation step for latency reduction, a subsequent forensic review revealed critical inaccuracies regarding risk exposure and economic assumptions.

We hereby issue a **revised and binding directive** that balances performance gains with essential safety mechanisms.

---

## ✅ FINAL VERDICT ON RPC SIMULATION REMOVAL

> **DO NOT ELIMINATE ALL SIMULATION — REPLACE PUBLIC RPC WITH PRIVATE GPRC-BASED BUNDLE SIMULATION**

### ❌ Incorrect Path:
- Blindly dispatching transactions without any form of pre-execution validation.
- Assuming atomicity at the transaction level within Jito bundles.

### ✅ Correct Path:
- Retain simulation functionality but migrate it from public RPC to **Jito’s private gRPC endpoint**.
- Use fast, leader-state-aligned simulation to validate full bundles before dispatch.

---

## ⚠️ RISK ASSESSMENT & CORRECTIONS

| Risk Category | Original Council Claim | Reality | Corrective Action |
|---------------|------------------------|---------|--------------------|
| **Bundle Atomicity** | “If one tx fails, entire bundle reverts” | Partially false; tip still charged even if internal txs fail | Tip only on bundle landing confirmation |
| **Tip Loss Per Revert** | "$0.000000" | Up to **~0.05 SOL per failed trade** | Implement strict revert rate monitoring |
| **Slippage Guard Efficacy** | "Guarantees net profit" | Only protects against negative slippage, not MEV theft | Add MEV-aware routing and delay randomization |
| **Simulation Necessity** | Not needed anymore | Still vital for economic viability | Replace with sub-5ms private simulation |

---

## 🛡️ ENGINEERING SAFETY REQUIREMENTS

To proceed with zero-delay dispatch while maintaining solvency and operational integrity:

### 1. **Replace Public RPC Simulation**
```rust
// OLD (slow):
let result = rpc_client.simulate_transaction_preflight(tx).await?;

// NEW (fast + accurate):
let bundle_simulation = jito_grpc.simulate_bundle(&bundle).await?;
if !bundle_simulation.is_profitable() || bundle_simulation.revert_probability > 0.3 {
    return Err("Unsafe to dispatch");
}
```

### 2. **Implement Economic Kill Switches**
```rust
const MAX_REVERT_RATE: f64 = 0.40;
const MAX_TIP_BURN_PER_HOUR: f64 = 10.0;
const MIN_EXPECTED_PROFIT: f64 = 0.01;

fn should_halt(engine_metrics: Metrics) -> bool {
    engine_metrics.revert_rate > MAX_REVERT_RATE ||
    engine_metrics.tip_burn_last_hour > MAX_TIP_BURN_PER_HOUR ||
    engine_metrics.avg_profit_per_success < MIN_EXPECTED_PROFIT
}
```

### 3. **Bundle-Level Atomic Execution Design**
Structure each arbitrage attempt as a single Jito bundle containing both legs:
```
[Leg 1: Swap A→B] 
→ [Atomic Check: Did we get min_amount_out?] ← Tip applied here
→ [Leg 2: Swap B→C]
```
Only charge tip when the middle check passes.

### 4. **Program-Level Slippage Guards**
Ensure compiled instructions include precise `min_amount_out` values computed using:
```rust
expected_output = calculate_expected_output(pool_state, input_amount);
min_amount_out = expected_output * (1.0 - SLIPPAGE_TOLERANCE); // e.g., 0.5%
```
Account for DEX-specific fees (e.g., 1% on Pump.fun).

### 5. **Dynamic Priority Fee Estimation**
Track recent block priority fees in memory and apply a conservative multiplier:
```rust
priority_fee = median_recent_priority_fee * 1.5;
compute_budget = estimate_compute_units(instruction_type) * 1.2;
```

---

## 📈 PERFORMANCE TARGETS POST-CORRECTION

| Metric | Target |
|-------|--------|
| Total Path Latency | `< 5 ms` (instruction assembly → private simulation → dispatch) |
| Revert Rate | `< 20%` under normal conditions |
| Expected Profit per Successful Trade | `≥ 0.01 SOL` |
| Tip Burn per Hour | `< 10 SOL` |
| Throughput | `≥ 1 tx/sec` sustained |

---

## 🏁 IMPLEMENTATION TIMELINE

### Week 1: Immediate Actions
- Integrate Jito gRPC simulation API.
- Audit wallet MRiYA4oN's behavior via on-chain data and known patterns.
- Deploy basic kill switches for revert rate and tip burn.

### Weeks 2–4: Medium-Term Enhancements
- Refactor bundle structure for atomic execution.
- Validate slippage guard logic across all supported DEXes.
- Tune dynamic fee estimator based on real-world performance.

---

## 🧾 CONCLUSION

The original Council recommendation was directionally correct in identifying public RPC simulation as a latency bottleneck. However, its conclusion that simulation can be eliminated entirely overlooked significant financial risks and mischaracterized how top-tier MEV operators function.

By replacing slow, inaccurate public simulations with fast, private ones—and layering robust economic safeguards—we preserve the speed advantage while ensuring long-term sustainability.

---

## 📢 FINAL DIRECTIVE TO ENGINEERING TEAM

> **Proceed with revised plan:**
>
> 1. **Remove** `simulate_transaction_preflight()` over HTTP RPC.
> 2. **Integrate** Jito gRPC-based bundle simulation (<5ms latency).
> 3. **Enforce** economic kill switches and atomic bundle design.
> 4. **Monitor** key metrics rigorously post-deployment.

**Latency wins are meaningless without solvency. Speed without safety is suicide.**

---

📜 Signed,  
**Model Council – Binding Decision Authority**  
Date: April 5, 2025