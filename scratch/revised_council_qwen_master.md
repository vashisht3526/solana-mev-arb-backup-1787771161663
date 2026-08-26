# 🔧 Revised Master Implementation Plan  
## 📌 Pump.fun → Raydium Migration Bot – Structural Overhaul  

---

## ✅ I. MATHEMATICAL & MARKET STRUCTURE VERDICT  

### ❗ Flaw Identified:
The prior model used **synthetic virtual reserves** from the Pump.fun bonding curve as an anchor for pricing logic. These reserves include a **phantom 30 SOL buffer**, which inflates the theoretical fair price significantly above what actually migrates to Raydium (~69 SOL real reserves). This leads to a **permanent phantom depression signal**, causing systematic false-positive buys.

### 🧮 Corrective Approach:
We discard the bonding curve anchor entirely and adopt **Raydium-native dynamics** based on actual on-chain data:

#### Key Anchors:
| Metric | Description |
|-------|-------------|
| `P0` | Real Raydium Pool Open Price at pool creation (based on actual migrated reserves) |
| `SellVelocityDecay` | Rolling window decay of sell pressure over 10s intervals |
| `Buy/SellVolumeRatio` | Demand-side momentum indicator |

#### Why This Works:
- **P0 reflects true market value** post-migration.
- **SellVelocityDecay** detects panic exhaustion — key inflection point before reversal.
- **Buy/Sell Volume Ratio** confirms demand return after dumping phase.

✅ **Conclusion:** Mathematically sound; aligns with real liquidity structure.

---

## ⏱️ II. TIMING & EXIT STRATEGY EVALUATION  

### Entry Conditions (All Must Pass):

| Rule # | Condition | Rationale |
|--------|-----------|-----------|
| 1 | Time ≥ T+15s | Avoid initial bot dump chaos |
| 2 | Price > 0.8 × P0 | Prevent catching falling knives (<50% crash) |
| 3 | SellVelocityDecay > 60% | Panic subsiding – buyers stepping in |
| 4 | Buy/Sell Volume Ratio > 1.2 (last 30s) | Confirming renewed buyer interest |
| 5 | 5-Pillar Shield + Dev Hold <3%, Liquidity >20 SOL | Safety net against scams/honeypots |
| 6 | Pre-flight Ghost Sell Simulation Passes | Ensures transaction viability without gas waste |

### Exit Conditions:

| Type | Trigger | Action |
|------|---------|--------|
| Take Profit | Price ≥ 110% of Entry Price | Immediate exit |
| Trailing Stop | Local high drops by 5% | Protect gains dynamically |
| Hard Stop Loss | Price ≤ 90% of Entry Price | Cut losses decisively |
| Time Stop | Elapsed ≥ 600 seconds | Force exit if no movement |
| Toxic Tagging | Failed Sell Attempt | Permanently tag token as toxic, do not retry |

✅ **Conclusion:** Balanced risk management tailored to meme coin volatility cycles.

---

## 💻 III. RUST TECHNICAL IMPLEMENTATION  

### A. On-Chain Data Extraction from Raydium CPMM

Use Solana Program Library (`spl-token`, `raydium-cpmm`) to fetch live pool state.

```rust
use solana_client::rpc_client::RpcClient;
use raydium_cpmm_sdk::{get_pool_info, PoolState};

fn fetch_raydium_pool_price(client: &RpcClient, pool_address: Pubkey) -> f64 {
    let pool_state: PoolState = get_pool_info(client, pool_address).unwrap();
    
    // Calculate P0 using actual reserve values
    let token_amount = pool_state.token_amount;
    let sol_amount = pool_state.sol_amount;

    sol_amount as f64 / token_amount as f64
}
```

> Store this once upon migration confirmation as `P0`.

---

### B. Sell Velocity Decay Computation

Track sells per second in two consecutive 10-second windows:

```rust
struct SellTracker {
    pub timestamps: Vec<u64>, // Unix timestamps of sells
}

impl SellTracker {
    fn add_sell(&mut self, timestamp: u64) {
        self.timestamps.push(timestamp);
    }

    fn calculate_velocity_decay(&self, now: u64) -> Option<f64> {
        let recent_window_start = now - 10;
        let older_window_start = now - 20;

        let recent_count = self.timestamps.iter()
            .filter(|&&t| t >= recent_window_start && t < now)
            .count();

        let older_count = self.timestamps.iter()
            .filter(|&&t| t >= older_window_start && t < recent_window_start)
            .count();

        if older_count == 0 { return None; }

        Some((older_count - recent_count) as f64 / older_count as f64)
    }
}
```

> Threshold: `velocity_decay > 0.60`

---

### C. Buy/Sell Volume Ratio Calculation

Maintain rolling volume counters:

```rust
struct VolumeTracker {
    pub buy_volume: Vec<(u64, u64)>,   // (timestamp, amount)
    pub sell_volume: Vec<(u64, u64)>,
}

impl VolumeTracker {
    fn add_buy(&mut self, ts: u64, amt: u64) {
        self.buy_volume.push((ts, amt));
    }

    fn add_sell(&mut self, ts: u64, amt: u64) {
        self.sell_volume.push((ts, amt));
    }

    fn ratio_last_30s(&self, now: u64) -> f64 {
        let cutoff = now - 30;

        let total_buys: u64 = self.buy_volume.iter()
            .filter(|&&(t, _)| t >= cutoff)
            .map(|(_, v)| v)
            .sum();

        let total_sells: u64 = self.sell_volume.iter()
            .filter(|&&(t, _)| t >= cutoff)
            .map(|(_, v)| v)
            .sum();

        if total_sells == 0 { return f64::INFINITY; }

        total_buys as f64 / total_sells as f64
    }
}
```

> Threshold: `ratio > 1.2`

---

### D. Memory Management for Trailing Stops

Store entry price and track highest observed price since entry:

```rust
struct Position {
    entry_price: f64,
    peak_price: f64,
    entry_time: u64,
}

impl Position {
    fn update_peak(&mut self, current_price: f64) {
        if current_price > self.peak_price {
            self.peak_price = current_price;
        }
    }

    fn should_exit_trailing_stop(&self, current_price: f64) -> bool {
        ((self.peak_price - current_price) / self.peak_price) >= 0.05
    }

    fn should_exit_hard_stop(&self, current_price: f64) -> bool {
        ((self.entry_price - current_price) / self.entry_price) >= 0.10
    }

    fn should_exit_take_profit(&self, current_price: f64) -> bool {
        ((current_price - self.entry_price) / self.entry_price) >= 0.10
    }

    fn should_exit_timeout(&self, now: u64) -> bool {
        (now - self.entry_time) >= 600
    }
}
```

---

### E. Pre-flight Ghost Sell Simulation

Simulate a minimal sell order via CPI simulation or dry-run RPC call to detect revert conditions before committing funds.

```rust
async fn simulate_sell(
    client: &RpcClient,
    wallet: &Keypair,
    mint: Pubkey,
    amount: u64,
) -> Result<bool, Box<dyn std::error::Error>> {
    let tx = build_sell_transaction(wallet, mint, amount)?;
    match client.simulate_transaction(&tx).await {
        Ok(response) => Ok(!response.value.err.is_some()),
        Err(_) => Ok(false),
    }
}
```

If simulation fails → flag as toxic.

---

## 🛠 IV. FINAL SYNTHESIZED IMPLEMENTATION PLAN  

### 🎯 Objective:
Deploy a fully shadow-mode trading engine that evaluates newly migrated tokens using **Raydium-native signals only**, avoiding synthetic anchors like bonding curves.

---

### 🧩 Core Modules:

| Module | Responsibility |
|--------|----------------|
| `PoolWatcher` | Monitors new Raydium pools for eligible migrations |
| `SignalEngine` | Applies entry rules using P0, velocity decay, volume ratios |
| `PositionManager` | Tracks active positions, applies trailing/stop logic |
| `RiskGuard` | Enforces safety checks including dev holdings, liquidity thresholds |
| `GhostSimulator` | Dry-runs transactions to avoid honeypot traps |
| `ToxicTagger` | Flags failed sells permanently to prevent fee burn |

---

### 🔄 Workflow Logic:

1. **Migration Detection**
   - Watch for new Raydium AMM pool creations.
   - Extract `P0` from initial reserves.
   - Begin monitoring after T+15s.

2. **Signal Evaluation Loop**
   - Every 5 seconds:
     - Fetch latest price.
     - Update sell tracker & volume tracker.
     - Check all entry criteria.
     - If passed → simulate ghost sell → enter position.

3. **Active Position Monitoring**
   - Track price every 2 seconds.
   - Update peak/trailing stop logic.
   - Apply take-profit, stop-loss, timeout triggers.

4. **Exit Execution**
   - Execute sell when any rule fires.
   - Tag token as toxic if execution fails.

---

### 🧪 Testing Framework:

- Unit tests for each metric calculation.
- Integration tests simulating full trade lifecycle.
- Replay historical pump/fail scenarios.
- Shadow mode logging with zero fund exposure.

---

## 📈 Summary

This revised implementation plan corrects the flawed reliance on synthetic bonding curve prices and replaces it with **on-chain Raydium-native metrics** that reflect true market behavior. It introduces robust risk controls, dynamic exits, and intelligent pre-flight validation to ensure safe operation even during chaotic early-stage token activity.

By grounding strategy in **real liquidity and behavioral indicators**, we eliminate phantom depression bias and create a mathematically coherent framework for automated evaluation of migrated tokens.

--- 

## 🚀 GO-LIVE STATUS:  
🟢 **READY FOR DEVELOPMENT DEPLOYMENT IN SHADOW MODE**

Let’s proceed with building the core modules outlined above.