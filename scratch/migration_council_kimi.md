 # Chief HFT Protocol Engineer Audit Report

## Executive Summary

This is a **high-risk, high-conviction directional strategy** with structural dependencies on MEV timing, liquidity fragmentation, and bonding curve mathematics. The 0.4046 SOL bankroll is **critically undercapitalized** for this strategy class.

---

## 1. ARCHITECTURE VERDICT: HYBRID DUAL-ENGINE (KEEP + ADD)

### Mathematical Rationale

| Dimension | Atomic Arbitrage | Migration Sniping |
|-----------|-----------------|-------------------|
| **Payoff Structure** | Bounded, convex (limited upside, zero downside) | Unbounded, concave (unlimited upside, catastrophic downside) |
| **Capital Efficiency** | 100% utilization, no inventory risk | 25-37% utilization, inventory risk |
| **Edge Decay** | Linear with competition | Exponential with time (15s → 60s cliff) |
| **Ruin Probability** | ~0% (atomic invariant) | >15% per 100 trades at this bankroll |
| **Sharpe Ratio** | High (low variance) | Low (high variance, skewed) |

### The Kelly Criterion Analysis

Given 0.4046 SOL bankroll and proposed 0.10 SOL position (24.7% stake):

```
Kelly Fraction = (bp - q) / b

Where: b = avg win / avg loss, p = win rate, q = 1-p

Estimated for this strategy:
- Win rate (p): 35-45% (aggressive estimate)
- Avg win: 15% (reversion to 97% of fair)
- Avg loss: -10% (stop) or -5% (time decay)
- Effective b ≈ 1.5

Kelly = (0.40 * 1.5 - 0.60) / 1.5 = 0% (negative expectancy at conservative estimates)
```

**At 0.10 SOL position size, you're betting 25% of bankroll on negative-expectancy edge detection.**

### Operational Rationale for Hybrid

```
┌─────────────────────────────────────────────────────────┐
│                    ORCHESTRATION LAYER                   │
│         (Slot-priority scheduler, 0.15 SOL reserve)     │
├─────────────────────────────────────────────────────────┤
│  ┌─────────────────┐        ┌─────────────────────────┐ │
│  │  ATOMIC ENGINE  │        │   MIGRATION SNIPER      │ │
│  │  (Always Active)│        │   (Conditional Active)  │ │
│  │                 │        │                         │ │
│  │  • Jup routing  │        │  • Geyser stream filter │ │
│  │  • Circular arb │        │  • Bonding curve calc   │ │
│  │  • MEV-Share    │        │  • 15s window gate      │ │
│  │                 │        │  • Position holding     │ │
│  │  Capital: 0.15  │        │  Capital: 0.10 (max 1)  │ │
│  │  Reserve: 0.15  │        │  Reserve: 0.15 (shared)   │ │
│  └─────────────────┘        └─────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
                              │
                    ┌─────────┴─────────┐
                    ▼                   ▼
              Priority Queue        Risk Circuit Breaker
              (Atomic > Snipe)      (Daily loss > 0.05 SOL)
```

**Verdict: HYBRID with strict capital partitioning.**

The atomic engine provides **survival income** while the sniper hunts **asymmetric convexity**. Never abandon positive-expectancy, zero-inventory strategies for directional exposure at this bankroll.

---

## 2. HONEYPOT & RUG SHIELD: PRE-FLIGHT SELL SIMULATION

### The Critical Problem

Standard `simulateTransaction` is **insufficient**. Many honeypots:
- Pass simulation (no state change)
- Fail on-chain (transfer hook rejection)
- Have conditional logic (time-delayed, holder-count gated)

### Rust Implementation: Shadow Simulation Engine

```rust
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    transaction::Transaction,
};
use spl_token::instruction as token_instruction;
use raydium_amm::instruction as raydium_instruction;

/// Pre-flight sell simulation with state commitment verification
pub struct ShadowSimulator {
    rpc: RpcClient,
    token_program: Pubkey,
    raydium_amm_program: Pubkey,
}

#[derive(Debug, Clone)]
pub enum SellSimulationResult {
    Executable {
        estimated_out: u64,
        price_impact_bps: u16,
        slot_verified: u64,
    },
    Blocked {
        reason: BlockReason,
        instruction_index: Option<usize>,
    },
    Uncertain {
        requires_onchain_test: bool,
    },
}

#[derive(Debug, Clone)]
pub enum BlockReason {
    TransferFeeExceedsLimit,      // >5% fee detected
    DelegateApprovalRequired,     // Must approve delegate first
    FrozenAccount,                // Token account frozen
    InsufficientLiquidity,        // Pool can't absorb sell
    ProgramOwnedDestination,      // Must sell to specific address
    DynamicBlacklist,             // Holder count / time gate
    UnknownInstruction,           // Custom program logic
}

impl ShadowSimulator {
    /// CRITICAL: Simulate sell BEFORE buy, same slot commitment
    pub async fn verify_sellability(
        &self,
        token_mint: &Pubkey,
        pool_state: &PoolState,
        proposed_buy_amount: u64,  // What we PLAN to buy
    ) -> Result<SellSimulationResult, SimulationError> {
        
        let slot = self.rpc.get_slot().await?;
        
        // Step 1: Construct synthetic post-buy state
        let synthetic_ata = self.construct_synthetic_ata(token_mint, proposed_buy_amount);
        
        // Step 2: Build sell instruction with synthetic ATA as input
        let sell_ix = self.build_sell_instruction(
            &synthetic_ata,
            pool_state,
            proposed_buy_amount,  // Sell 100% of planned buy
        );
        
        // Step 3: Multi-layer simulation
        let layers = vec![
            SimulationLayer::StandardRpc,           // Basic execution
            SimulationLayer::TokenProgramCpiTrace,  // Inner instruction analysis
            SimulationLayer::AccountStateDelta,     // Post-simulation account verification
        ];
        
        for layer in layers {
            match self.execute_layer(&layer, &sell_ix, slot).await? {
                LayerResult::Pass => continue,
                LayerResult::Fail(reason) => {
                    return Ok(SellSimulationResult::Blocked {
                        reason,
                        instruction_index: self.locate_failure(&layer),
                    });
                }
                LayerResult::Inconclusive => {
                    // Fall through to next layer or uncertain
                    if layer == SimulationLayer::AccountStateDelta {
                        return Ok(SellSimulationResult::Uncertain {
                            requires_onchain_test: true,
                        });
                    }
                }
            }
        }
        
        // Step 4: Verify no time-dependent gates
        if self.detect_time_gates(token_mint).await? {
            return Ok(SellSimulationResult::Blocked {
                reason: BlockReason::DynamicBlacklist,
                instruction_index: None,
            });
        }
        
        // Step 5: Calculate actual output with slippage buffer
        let estimated_out = self.calculate_sell_output(
            pool_state,
            proposed_buy_amount,
        )?;
        
        Ok(SellSimulationResult::Executable {
            estimated_out,
            price_impact_bps: self.calculate_impact(pool_state, proposed_buy_amount),
            slot_verified: slot,
        })
    }
    
    /// Detect transfer fees, permanent delegates, freeze authority
    async fn analyze_mint_authorities(
        &self,
        mint: &Pubkey,
    ) -> Result<MintAnalysis, SimulationError> {
        let account = self.rpc.get_account(mint).await?;
        let mint_state = spl_token::state::Mint::unpack(&account.data)?;
        
        Ok(MintAnalysis {
            transfer_fee_config: self.parse_transfer_fee(&account.data),
            permanent_delegate: mint_state.delegate.is_some(),
            freeze_authority: mint_state.freeze_authority.is_some(),
            mint_authority: mint_state.mint_authority.is_some(),
            
            // CRITICAL: Check for Token-2022 extensions
            extensions: self.detect_extensions(&account.data),
        })
    }
    
    /// Build actual Raydium CPMM sell instruction for simulation
    fn build_sell_instruction(
        &self,
        user_ata: &Pubkey,
        pool_state: &PoolState,
        amount_in: u64,
    ) -> Instruction {
        // Raydium CPMM swap instruction layout
        // Discriminator: [0x0f, 0x14, 0x0d, 0x4f, 0x5a, 0x5a, 0x5a, 0x5a] (swap_base_in)
        
        let accounts = vec![
            AccountMeta::new(pool_state.amm_id, false),           // AMM
            AccountMeta::new_readonly(pool_state.authority, false), // Authority
            AccountMeta::new(pool_state.amm_open_orders, false),   // Open orders
            AccountMeta::new(pool_state.amm_target_orders, false), // Target orders
            AccountMeta::new(pool_state.pool_coin_token_account, false), // Base vault
            AccountMeta::new(pool_state.pool_pc_token_account, false),   // Quote vault
            AccountMeta::new_readonly(pool_state.serum_program, false),  // Serum program
            AccountMeta::new(pool_state.serum_market, false),     // Serum market
            AccountMeta::new(pool_state.serum_bids, false),       // Serum bids
            AccountMeta::new(pool_state.serum_asks, false),       // Serum asks
            AccountMeta::new(pool_state.serum_event_queue, false), // Event queue
            AccountMeta::new(pool_state.serum_coin_vault, false), // Coin vault
            AccountMeta::new(pool_state.serum_pc_vault, false),   // PC vault
            AccountMeta::new_readonly(pool_state.serum_vault_signer, false), // Vault signer
            AccountMeta::new(*user_ata, false),                   // User source token
            AccountMeta::new(self.get_user_quote_ata(), false),   // User dest token
            AccountMeta::new_readonly(spl_token::id(), false),    // Token program
        ];
        
        // Swap base in: sell token for SOL
        let data = SwapInstructionBaseIn {
            discriminator: [0x0f, 0x14, 0x0d, 0x4f, 0x5a, 0x5a, 0x5a, 0x5a],
            amount_in,
            minimum_amount_out: 0, // We want to detect if ANY output is possible
        }
        .pack();
        
        Instruction {
            program_id: self.raydium_amm_program,
            accounts,
            data,
        }
    }
}
```

### The "Canary" Technique

For uncertain tokens, deploy **micro-canary** before main position:

```rust
/// 0.001 SOL test buy → immediate sell verification
pub async fn canary_test(
    &self,
    token_mint: &Pubkey,
    pool_state: &PoolState,
) -> Result<CanaryResult, ExecutionError> {
    let canary_amount = 1_000_000; // 0.001 SOL worth of token
    
    // Execute real on-chain buy (irreversible)
    let buy_sig = self.execute_canary_buy(token_mint, canary_amount).await?;
    
    // Immediate sell attempt
    match self.execute_canary_sell(token_mint, canary_amount).await {
        Ok(sell_sig) => {
            // Calculate actual slippage
            let buy_cost = self.get_transaction_cost(&buy_sig).await?;
            let sell_proceeds = self.get_transaction_output(&sell_sig).await?;
            
            Ok(CanaryResult::Valid {
                effective_slippage_bps: ((buy_cost - sell_proceeds) * 10000 / buy_cost) as u16,
                buy_signature: buy_sig,
                sell_signature: sell_sig,
            })
        }
        Err(e) => {
            // HONEYPOT CONFIRMED - blacklist and alert
            self.blacklist_token(token_mint, BlacklistReason::CanarySellFailed(e)).await?;
            Err(ExecutionError::HoneypotDetected(token_mint.to_string()))
        }
    }
}
```

**Cost: ~0.002 SOL per canary. At 10 canaries/day, 0.02 SOL burn rate for safety.**

---

## 3. RUST IMPLEMENTATION: STATE MACHINE & RAYDIUM CPMM

### Core State Machine: `GraduationSniper`

```rust
use tokio::sync::{mpsc, RwLock};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Primary state machine for migration sniping
pub struct GraduationSniper {
    config: SniperConfig,
    state: Arc<RwLock<SniperState>>,
    rpc: Arc<RpcClient>,
    geyser: Arc<GeyserClient>,
    executor: Arc<TransactionExecutor>,
    
    // Position tracking
    active_position: Arc<RwLock<Option<ActivePosition>>>,
    position_history: Arc<RwLock<Vec<ClosedPosition>>>,
}

#[derive(Debug, Clone)]
pub struct SniperConfig {
    pub max_position_sol: f64,           // 0.10 SOL
    pub max_concurrent_positions: usize, // 1
    pub reserve_sol: f64,                // 0.15 SOL
    pub entry_time_window_ms: u64,       // 15,000
    pub hold_time_limit_ms: u64,         // 180,000
    pub stop_loss_bps: u16,              // 1000 (10%)
    pub take_profit_bps: u16,            // Target vs fair price
    pub min_liquidity_sol: f64,          // 20 SOL
    pub price_depression_threshold: f64, // 0.90 (90% of fair)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SniperState {
    Idle,                    // Waiting for graduation signal
    Analyzing,               // 5-pillar shield + sell simulation
    PreFlight,               // Canary test (optional)
    Entering,                // Buy transaction in flight
    Holding,                 // Position active, monitoring
    Exiting,                 // Sell transaction in flight
    Cooldown,                // Post-trade, risk debrief
    CircuitBroken,           // Daily loss limit hit
}

/// In-memory position tracking (no DB latency)
#[derive(Debug, Clone)]
pub struct ActivePosition {
    pub token_mint: Pubkey,
    pub pool_id: Pubkey,
    pub entry_slot: u64,
    pub entry_timestamp: Instant,
    pub entry_price_sol_per_token: f64,  // Raydium price paid
    pub fair_price_sol_per_token: f64,   // Bonding curve derived
    pub token_amount: u64,
    pub sol_invested: u64,
    pub ata: Pubkey,
    pub state: PositionMonitorState,
    
    // Live tracking
    pub current_price: Arc<RwLock<f64>>,
    pub unrealized_pnl_bps: Arc<RwLock<i32>>,
}

#[derive(Debug, Clone)]
pub enum PositionMonitorState {
    Unconfirmed,      // Buy tx submitted, awaiting confirmation
    Confirmed,        // Position established
    StopLossArmed,    // Price below entry - stop_loss_bps
    TakeProfitArmed,  // Price approaching fair price
    ExitPending,      // Sell tx submitted
}

/// State transition implementation
impl GraduationSniper {
    pub async fn run(self: Arc<Self>) {
        let mut geyser_rx = self.geyser.subscribe_pool_creations().await;
        
        while let Some(pool_event) = geyser_rx.recv().await {
            let current_state = *self.state.read().await;
            
            match current_state {
                SniperState::Idle => {
                    if self.is_migration_event(&pool_event) {
                        self.transition_to(SniperState::Analyzing).await;
                        tokio::spawn(self.clone().analyze_opportunity(pool_event));
                    }
                }
                
                SniperState::Analyzing => {
                    // Concurrent analysis, ignore new events
                    continue;
                }
                
                SniperState::Holding => {
                    // Background monitor handles this
                    continue;
                }
                
                SniperState::CircuitBroken => {
                    if self.should_reset_circuit().await {
                        self.transition_to(SniperState::Idle).await;
                    }
                }
                
                _ => {
                    // Transitional states - wait for completion
                    continue;
                }
            }
        }
    }
    
    async fn analyze_opportunity(self: Arc<Self>, event: PoolCreationEvent) {
        let analysis_start = Instant::now();
        let deadline = analysis_start + Duration::from_millis(5000); // 5s max analysis
        
        // Parallel shield verification
        let shield_check = tokio::spawn(self.verify_5_pillars(&event.mint));
        let liquidity_check = tokio::spawn(self.verify_liquidity(&event.pool_id));
        let price_check = tokio::spawn(self.calculate_price_depression(&event));
        let sell_sim = tokio::spawn(self.shadow_simulator.verify_sellability(
            &event.mint,
            &event.pool_state,
            self.token_amount_for_sol(self.config.max_position_sol),
        ));
        
        let results = tokio::time::timeout_at(
            deadline.into(),
            tokio::join!(shield_check, liquidity_check, price_check, sell_sim)
        ).await;
        
        match results {
            Ok((Ok(shield), Ok(liquidity), Ok(price), Ok(sell_ok))) => {
                if shield.valid && liquidity >= self.config.min_liquidity_sol 
                    && price.depression_ratio <= self.config.price_depression_threshold
                    && matches!(sell_ok, SellSimulationResult::Executable { .. }) {
                    
                    self.transition_to(SniperState::Entering).await;
                    self.execute_entry(event, price).await;
                } else {
                    self.transition_to(SniperState::Idle).await;
                }
            }
            _ => {
                self.metrics.record_analysis_timeout().await;
                self.transition_to(SniperState::Idle).await;
            }
        }
    }
}
```

### Raydium CPMM Instruction Layout

```rust
/// Raydium Constant Product Market Maker instruction builder
pub mod raydium_cpmm {
    use solana_program::instruction::Instruction;
    
    // Raydium AMM program ID (mainnet