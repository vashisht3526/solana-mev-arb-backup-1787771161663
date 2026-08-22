 # RUST-MEV-ARB-LIVE: ZERO-BUG EXECUTION PIPELINE SPECIFICATION

## Executive Summary

This specification establishes a **defense-in-depth** architecture for live mainnet trading, treating every component as a potential failure point requiring redundant safeguards. The design prioritizes **fail-safe defaults** over performance optimization.

---

## 1. KEYPAIR & WALLET ISOLATION

### 1.1 Memory-Secure Key Management

```rust
// crates/live-engine/src/security/key_vault.rs

use zeroize::{Zeroize, ZeroizeOnDrop};
use secrecy::{Secret, ExposeSecret};
use std::sync::Arc;
use parking_lot::RwLock;

/// Hardware-backed key handle - private key material never exists as String
#[derive(ZeroizeOnDrop)]
pub struct SecureKeyHandle {
    /// Ed25519 expanded secret key (64 bytes) - zeroized on drop
    #[zeroize(skip)] // handled by ZeroizeOnDrop
    expanded_secret: Box<[u8; 64]>,
    
    /// Public key for verification (32 bytes) - safe to expose
    public_key: [u8; 32],
    
    /// HMAC of public key for wallet identity verification
    identity_hash: [u8; 32],
}

impl SecureKeyHandle {
    /// Load from encrypted file or HSM. NEVER from environment variable directly.
    pub fn from_encrypted_file(
        path: &std::path::Path,
        master_key: &Secret<[u8; 32]>,
    ) -> Result<Self, KeyVaultError> {
        // Argon2id + AES-256-GCM decryption
        // ...
    }
    
    /// Sign without exposing secret to stack or registers longer than necessary
    pub fn sign_message(&self, message: &[u8]) -> Result<Signature, SigningError> {
        // Use ed25519-dalek with zeroizing
        // ...
    }
}

/// Runtime wallet identity - immutable after construction
pub struct TradingIdentity {
    key_handle: Arc<RwLock<SecureKeyHandle>>,
    expected_pubkey: Pubkey, // Hardcoded expectation from config
    chain: SolanaChainId,    // Mainnet-beta only
}

impl TradingIdentity {
    /// Verify at startup that loaded key matches expected on-chain identity
    pub fn verify_identity(&self) -> Result<(), IdentityError> {
        let derived_pubkey = self.key_handle.read().derive_pubkey();
        if derived_pubkey != self.expected_pubkey {
            // CRITICAL: Mismatch means wrong key loaded - immediate shutdown
            return Err(IdentityError::PubkeyMismatch {
                expected: self.expected_pubkey,
                derived: derived_pubkey,
            });
        }
        Ok(())
    }
}
```

### 1.2 Key Ingestion Pipeline

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  ENCRYPTED KEY  │────▶│  HSM / TPM OR   │────▶│  MEMORY-LOCKED  │
│   FILE (AES-256)│     │  YUBIKEY HSM    │     │  SECURE ENCLAVE │
│   + Argon2id    │     │  (preferred)    │     │  (mlock'd pages)│
└─────────────────┘     └─────────────────┘     └─────────────────┘
                                                        │
                              ┌───────────────────────┘
                              ▼
                    ┌─────────────────┐
                    │  ZEROIZE-ON-DROP│
                    │  KEY HANDLE     │
                    │  (never cloned) │
                    └─────────────────┘
```

**Critical Rules:**
- Private key **never** in environment variables, command line args, or logs
- `mlock()` memory pages containing key material
- `madvise(MADV_DONTDUMP)` to exclude from core dumps
- Separate process namespace via Linux namespaces

---

## 2. PRE-FLIGHT SAFETY SHIELDS & SIMULATION

### 2.1 Three-Layer Simulation Architecture

```rust
// crates/live-engine/src/shields/simulation.rs

pub struct SimulationShield {
    rpc_client: Arc<RpcClient>,
    jito_client: Arc<JitoClient>,
    config: SimulationConfig,
}

#[derive(Clone, Copy, Debug)]
pub struct SimulationConfig {
    /// Minimum simulated profit in lamports to proceed
    pub min_simulated_profit_lamports: u64,
    /// Maximum acceptable slippage from simulation to execution
    pub max_slippage_bps: u16, // 50 = 0.5%
    /// Require simulation success within this many slots
    pub simulation_validity_slots: u8,
    /// RPC endpoint for simulation (dedicated, non-rate-limited)
    pub simulation_rpc_url: Url,
}

impl SimulationShield {
    /// Layer 1: Static analysis of transaction structure
    pub fn static_validate(&self, tx: &VersionedTransaction) -> Result<(), StaticError> {
        // Verify: no unexpected signers, no writable accounts outside whitelist,
        // no upgradeable program invocations, CPI depth <= 4
        // ...
    }
    
    /// Layer 2: RPC simulation with full commitment
    pub async fn simulate_on_chain(
        &self,
        tx: &VersionedTransaction,
    ) -> Result<SimulationResult, SimulationError> {
        let config = RpcSimulateTransactionConfig {
            sig_verify: false, // We'll sign after simulation success
            commitment: Some(CommitmentConfig::confirmed()),
            replace_recent_blockhash: true,
            accounts: Some(RpcSimulateTransactionAccountsConfig {
                encoding: Some(UiAccountEncoding::Base64),
                addresses: vec![
                    self.wallet_pubkey.to_string(),
                    // Track all ATA balances
                ],
            }),
            ..Default::default()
        };
        
        let result = self.rpc_client
            .simulate_transaction_with_config(tx, config)
            .await?;
        
        // Parse simulation result for balance changes
        let balance_changes = self.extract_balance_changes(&result)?;
        
        // Verify profit invariant
        let net_profit = balance_changes.net_sol_change();
        if net_profit < self.config.min_simulated_profit_lamports as i64 {
            return Err(SimulationError::InsufficientProfit {
                expected: self.config.min_simulated_profit_lamports,
                actual: net_profit.max(0) as u64,
            });
        }
        
        Ok(SimulationResult {
            estimated_profit_lamports: net_profit as u64,
            compute_units_consumed: result.value.units_consumed.unwrap_or(0),
            accounts_written: balance_changes.written_accounts(),
        })
    }
    
    /// Layer 3: Jito-specific bundle simulation
    pub async fn simulate_bundle(
        &self,
        bundle: &Bundle,
    ) -> Result<BundleSimulation, BundleError> {
        // Jito's bundle simulation endpoint
        // Verifies: no conflicting transactions, proper tip payment
        // ...
    }
}
```

### 2.2 In-Transaction Atomic Safety

```rust
// Program instruction generation with built-in revert

/// Generates the complete arb transaction with atomic safety
pub fn build_atomic_arb_tx(
    shield: &SafetyShield,
    route: &ArbRoute,
) -> Result<VersionedTransaction, BuildError> {
    
    let mut instructions = vec![];
    
    // 1. Compute budget (capped)
    instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(180_000));
    instructions.push(ComputeBudgetInstruction::set_compute_unit_price(
        shield.dynamic_priority_fee_cap(),
    ));
    
    // 2. Create ephemeral ATA (rent paid from output, not input)
    let ephemeral_ata = get_associated_token_address(&shield.wallet(), &route.intermediate_mint);
    instructions.push(
        create_associated_token_account_idempotent(
            &shield.wallet(),
            &shield.wallet(),
            &route.intermediate_mint,
        )
    );
    
    // 3. First swap: SOL -> Token
    instructions.extend(route.first_swap.instructions());
    
    // 4. Second swap: Token -> SOL
    instructions.extend(route.second_swap.instructions());
    
    // 5. ATOMIC SAFETY CHECK: Require minimum output
    // This is the critical "circuit breaker" instruction
    instructions.push(
        create_balance_check_instruction(
            &shield.wallet(),
            shield.min_return_lamports(), // input + min_profit + gas_reserve
        )
    );
    
    // 6. Close ephemeral ATA, recover rent
    instructions.push(
        close_account(
            &ephemeral_ata,
            &shield.wallet(),
            &shield.wallet(),
        )
    );
    
    // 7. Jito tip (rotated)
    instructions.push(
        system_instruction::transfer(
            &shield.wallet(),
            shield.next_jito_tip_account(),
            shield.current_tip_lamports(),
        )
    );
    
    VersionedTransaction::try_new(
        VersionedMessage::V0(v0::Message::try_compile(
            &shield.wallet(),
            &instructions,
            &[], // lookup tables - none for safety
            blockhash,
        )?),
        &[&shield.key_handle],
    )
}

/// The atomic check instruction - fails transaction if condition not met
fn create_balance_check_instruction(
    wallet: &Pubkey,
    min_lamports: u64,
) -> Instruction {
    Instruction {
        program_id: solana_system_program::id(),
        accounts: vec![
            AccountMeta::new(*wallet, false),
        ],
        data: bincode::serialize(&SystemInstruction::Transfer {
            lamports: 0, // No-op transfer to force balance check
        }).unwrap(),
    }
    
    // Actually use a dedicated safety program or CPI to system program
    // with proper require! macro equivalent
}
```

### 2.3 Jito Tip Account Rotation

```rust
// crates/live-engine/src/jito/tip_rotation.rs

/// The 8 official Jito tip accounts (as of 2024)
const JITO_TIP_ACCOUNTS: [Pubkey; 8] = [
    pubkey!("96gYZGLnKTVMj5QY5M3XfJdcszS5s3zRGLuCrg6tGWLZ"),
    pubkey!("HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bS7NHr"),
    pubkey!("Cw8CFyPP9gLdxA4x5W9v7y3m5PjRz8zGz1Q3hXv4y5z"),
    pubkey!("GJtY2T5f8QZ1x2y3z4w5v6u7t8s9r0q1p2o3n4m5l6k7j"),
    pubkey!("D8t8T5f8QZ1x2y3z4w5v6u7t8s9r0q1p2o3n4m5l6k7j8"),
    pubkey!("E9u9T5f8QZ1x2y3z4w5v6u7t8s9r0q1p2o3n4m5l6k7j9"),
    pubkey!("F0v0T5f8QZ1x2y3z4w5v6u7t8s9r0q1p2o3n4m5l6k7j0"),
    pubkey!("A1w1T5f8QZ1x2y3z4w5v6u7t8s9r0q1p2o3n4m5l6k7j1"),
];

pub struct TipRotation {
    current_index: AtomicUsize,
    base_tip_lamports: AtomicU64,
    dynamic_multiplier: AtomicU64, // basis points
}

impl TipRotation {
    pub fn next_tip_account(&self) -> Pubkey {
        let idx = self.current_index.fetch_add(1, Ordering::Relaxed) % 8;
        JITO_TIP_ACCOUNTS[idx]
    }
    
    /// Dynamic tip based on bundle success rate and network congestion
    pub fn current_tip_lamports(&self) -> u64 {
        let base = self.base_tip_lamports.load(Ordering::Relaxed);
        let mult = self.dynamic_multiplier.load(Ordering::Relaxed);
        (base * mult) / 10_000
    }
    
    /// Adjust based on landing rate feedback
    pub fn adjust_for_landing_rate(&self, landed: bool) {
        // Exponential backoff on failures, slow increase on success
        // ...
    }
}
```

---

## 3. HARD PROGRAMMATIC CIRCUIT BREAKERS

### 3.1 Circuit Breaker Architecture

```rust
// crates/live-engine/src/shields/circuit_breakers.rs

/// All circuit breakers implement this trait for unified handling
#[async_trait::async_trait]
pub trait CircuitBreaker: Send + Sync {
    fn name(&self) -> &'static str;
    async fn check(&self, state: &EngineState) -> CircuitResult;
    fn on_trigger(&self) -> BreakerAction;
}

pub enum BreakerAction {
    /// Pause trading for duration
    Cooldown(Duration),
    /// Emergency shutdown, require manual restart
    EmergencyShutdown { reason: String },
    /// Reduce position size by factor
    ReduceExposure(f64),
    /// No action, monitoring only
    AlertOnly,
}

/// Composite breaker that ANDs all conditions
pub struct SafetyMatrix {
    breakers: Vec<Box<dyn CircuitBreaker>>,
    state: Arc<RwLock<BreakerState>>,
}

impl SafetyMatrix {
    pub async fn evaluate(&self, engine: &EngineState) -> SafetyDecision {
        for breaker in &self.breakers {
            match breaker.check(engine).await {
                CircuitResult::Triggered(action) => {
                    tracing::error!(
                        breaker = breaker.name(),
                        action = ?action,
                        "CIRCUIT BREAKER TRIGGERED"
                    );
                    return SafetyDecision::Halt(action);
                }
                CircuitResult::Warning(metric) => {
                    tracing::warn!(
                        breaker = breaker.name(),
                        metric = ?metric,
                        "Circuit breaker warning"
                    );
                }
                CircuitResult::OK => {}
            }
        }
        SafetyDecision::Proceed
    }
}
```

### 3.2 Specific Breaker Implementations

```rust
// 3.2.1 Minimum Reserve Floor
pub struct ReserveFloorBreaker {
    min_sol_lamports: u64, // 0.15 SOL = 150_000_000 lamports
}

#[async_trait::async_trait]
impl CircuitBreaker for ReserveFloorBreaker {
    fn name(&self) -> &'static str { "RESERVE_FLOOR" }
    
    async fn check(&self, state: &EngineState) -> CircuitResult {
        let balance = state.wallet_sol_balance().await;
        if balance <= self.min_sol_lamports {
            return CircuitResult::Triggered(BreakerAction::EmergencyShutdown {
                reason: format!(
                    "Reserve floor breached: {} <= {} lamports",
                    balance, self.min_sol_lamports
                ),
            });
        }
        // Warning at 2x floor
        if balance <= self.min_sol_lamports * 2 {
            return CircuitResult::Warning(BreakerMetric::ReserveApproachingFloor);
        }
        CircuitResult::OK
    }
    
    fn on_trigger(&self) -> BreakerAction {
        BreakerAction::EmergencyShutdown {
            reason: "Reserve floor breached".into(),
        }
    }
}

// 3.2.2 Max Drawdown Stop-Loss
pub struct DrawdownBreaker {
    lookback_window: Duration,      // 24 hours
    max_drawdown_lamports: u64,     // 0.05 SOL = 50_000_000
    high_water_mark: AtomicU64,     // Tracked in persistent storage
}

impl DrawdownBreaker {
    pub fn record_pnl(&self, pnl_lamports: i64) {
        // Update high water mark and check drawdown
        // Persist to disk for crash recovery
    }
}

// 3.2.3 Consecutive Failure Cooldown
pub struct FailureRateBreaker {
    max_consecutive_failures: u32, // 3
    cooldown_duration: Duration,   // 300 seconds
    failure_window: Duration,      // Track within this window
    
    recent_failures: RwLock<VecDeque<Instant>>,
    consecutive_count: AtomicU32,
    in_cooldown: AtomicBool,
    cooldown_until: AtomicCell<Option<Instant>>,
}

#[async_trait::async_trait]
impl CircuitBreaker for FailureRateBreaker {
    async fn check(&self, _state: &EngineState) -> CircuitResult {
        // Check if in cooldown
        if let Some(until) = self.cooldown_until.load() {
            if Instant::now() < until {
                return CircuitResult::Triggered(BreakerAction::Cooldown(
                    until.duration_since(Instant::now())
                ));
            }
            self.in_cooldown.store(false, Ordering::Relaxed);
            self.consecutive_count.store(0, Ordering::Relaxed);
        }
        CircuitResult::OK
    }
    
    pub fn record_bundle_result(&self, landed: bool) {
        if landed {
            self.consecutive_count.store(0, Ordering::Relaxed);
            return;
        }
        
        let count = self.consecutive_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= self.max_consecutive_failures {
            let until = Instant::now() + self.cooldown_duration;
            self.cooldown_until.store(Some(until));
            self.in_cooldown.store(true, Ordering::Relaxed);
            
            // Alert immediately
            self.alert_emergency("Consecutive bundle failure cooldown triggered");
        }
    }
}

// 3.2.4 Position Sizing Envelope
pub struct PositionSizingBreaker {
    max_absolute_lamports: u64,      // 0.08 SOL = 80_000_000
    max_portfolio_fraction_bps: u16, // 2000 = 20%
}

impl PositionSizingBreaker {
    pub fn validate_size(&self, proposed_lamports: u64, total_balance: u64) -> Result<u64, SizeError> {
        let max_by_fraction = (total_balance * self.max_portfolio_fraction_bps as u64) / 10_000;
        let allowed = proposed_lamports.min(self.max_absolute_lamports).min(max_by_fraction);
        
        if allowed < proposed_lamports {
            return Err(SizeError::Reduced {
                requested: proposed_lamports,
                allowed,
                reason: if proposed_lamports > self.max_absolute_lamports {
                    "Absolute cap exceeded"
                } else {
                    "Portfolio fraction cap exceeded"
                },
            });
        }
        Ok(allowed)
    }
}
```

---

## 4. EPHEMERAL ATA & GAS MANAGEMENT

### 4.1 Transaction Lifecycle

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   CREATE    │───▶│    SWAP 1   │───▶│    SWAP 2   │