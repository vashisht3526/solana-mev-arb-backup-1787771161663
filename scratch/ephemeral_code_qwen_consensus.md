Looking at both implementations, I'll synthesize the final consensus code that addresses all requirements while fixing the identified issues.

## Final Consensus Implementation

### `src/types.rs`
```rust
use solana_sdk::{pubkey::Pubkey, clock::Slot};
use std::sync::atomic::{AtomicU64, Ordering};

/// Compute Budget configurations for MEV operations with ephemeral ATAs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComputeBudgetConfig {
    /// Standard CPMM: 125,000 CUs, 1,000 micro-lamports priority.
    EphemeralStandard,
    /// DLMM/CLMM + ATA lifecycle: 180,000 CUs, 5,000 micro-lamports priority.
    EphemeralComplex,
}

impl ComputeBudgetConfig {
    pub const fn units(&self) -> u32 {
        match self {
            Self::EphemeralStandard => 125_000,
            Self::EphemeralComplex => 180_000,
        }
    }

    pub const fn micro_lamports(&self) -> u64 {
        match self {
            Self::EphemeralStandard => 1_000,
            Self::EphemeralComplex => 5_000,
        }
    }
}

/// Tracks ephemeral ATA state across transaction lifecycle.
/// Thread-safe for cross-task monitoring.
#[derive(Debug)]
pub struct EphemeralAtaLifecycle {
    pub ata_address: Pubkey,
    pub target_mint: Pubkey,
    pub token_program: Pubkey, // Token-2022 support
    pub creation_slot: Slot,
    pub creation_timestamp: AtomicU64, // Unix timestamp for grace period
    pub is_closed: std::sync::atomic::AtomicBool,
    pub expected_rent: u64,
    pub pending_close: std::sync::atomic::AtomicBool, // Prevent double-close attempts
}

impl EphemeralAtaLifecycle {
    pub fn new(
        ata_address: Pubkey,
        target_mint: Pubkey,
        token_program: Pubkey,
        creation_slot: Slot,
    ) -> Self {
        Self {
            ata_address,
            target_mint,
            token_program,
            creation_slot,
            creation_timestamp: AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            ),
            is_closed: std::sync::atomic::AtomicBool::new(false),
            expected_rent: 2_039_280, // Rent-exempt minimum for token account (approx)
            pending_close: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Grace period before background reclamation (seconds).
    pub const RECLAIM_GRACE_SECS: u64 = 300; // 5 minutes

    pub fn can_reclaim(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let created = self.creation_timestamp.load(Ordering::Relaxed);
        
        now.saturating_sub(created) >= Self::RECLAIM_GRACE_SECS
            && !self.is_closed.load(Ordering::Relaxed)
            && !self.pending_close.load(Ordering::Relaxed)
    }

    pub fn mark_pending_close(&self) -> bool {
        self.pending_close
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}

/// Pool type classification for compute budget selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PoolType {
    CPMM,  // Constant Product (Raydium AMM, Orca Whirlpool standard)
    DLMM,  // Meteora Dynamic Liquidity Market Maker
    CLMM,  // Concentrated Liquidity (Orca Whirlpool concentrated, Raydium CLMM)
}

impl PoolType {
    pub fn requires_ephemeral_ata(&self) -> bool {
        // All types may use ephemeral ATAs for intermediate tokens
        true
    }

    pub fn compute_intensity(&self) -> u32 {
        match self {
            Self::CPMM => 80_000,    // Base swap cost
            Self::CLMM => 120_000,   // Tick math overhead
            Self::DLMM => 140_000,   // Bin math + dynamic rebalancing
        }
    }
}

/// Errors for lifecycle management.
#[derive(Debug, thiserror::Error)]
pub enum LifecycleError {
    #[error("ATA already closed or closing")]
    AlreadyClosed,
    #[error("Grace period not elapsed")]
    GracePeriodActive,
    #[error("Invalid token program")]
    InvalidTokenProgram,
}
```

### `src/router.rs`
```rust
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    compute_budget::ComputeBudgetInstruction,
};
use spl_associated_token_account::{
    get_associated_token_address_with_program_id,
    instruction::{
        create_associated_token_account_idempotent_with_program_id,
    },
};
use crate::types::{ComputeBudgetConfig, EphemeralAtaLifecycle, PoolType};

/// Errors from atomic bundle construction.
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("Invalid pool type for operation: {0:?}")]
    InvalidPoolType(PoolType),
    #[error("Swap instruction construction failed: {0}")]
    SwapConstructionFailed(String),
    #[error("Profit check program unavailable")]
    ProfitCheckUnavailable,
    #[error("Token program mismatch: expected {expected}, got {actual}")]
    TokenProgramMismatch { expected: Pubkey, actual: Pubkey },
    #[error("ATA derivation error: {0}")]
    AtaDerivationError(String),
}

/// Atomic instruction bundle for MEV arbitrage with ephemeral ATA.
pub struct AtomicBundle {
    pub instructions: Vec<Instruction>,
    pub lifecycle: EphemeralAtaLifecycle,
    pub ephemeral_ata: Pubkey,
    pub estimated_cus: u32,
}

pub struct AtomicRouterEngine {
    pub wallet: Pubkey,
    pub profit_check_program: Pubkey,
    /// Token program ID to use (spl_token or spl_token_2022)
    pub token_program: Pubkey,
}

impl AtomicRouterEngine {
    pub fn new(
        wallet: Pubkey,
        profit_check_program: Pubkey,
        token_program: Pubkey,
    ) -> Self {
        Self {
            wallet,
            profit_check_program,
            token_program,
        }
    }

    /// Select compute budget based on pool complexity and ATA overhead.
    /// 
    /// Formula: pool_compute_intensity + ATA_CREATE_CU + ATA_CLOSE_CU + SAFETY_MARGIN
    /// - ATA Create: ~10,000 CUs
    /// - ATA Close: ~6,000 CUs  
    /// - Safety margin: 20%
    pub fn select_compute_budget(&self, pool_type: PoolType) -> ComputeBudgetConfig {
        let base = pool_type.compute_intensity();
        let ata_overhead = 16_000; // Create + Close
        let total = base + ata_overhead;
        
        // Scale to predefined buckets for deterministic fee estimation
        if total > 150_000 {
            ComputeBudgetConfig::EphemeralComplex
        } else {
            ComputeBudgetConfig::EphemeralStandard
        }
    }

    /// Build atomic arbitrage bundle with ephemeral ATA lifecycle.
    /// 
    /// Execution order guarantees:
    /// 1. Compute budget set first (required by Solana runtime)
    /// 2. ATA created idempotently (succeeds if exists)
    /// 3. Swap legs execute atomically
    /// 4. ATA closed to reclaim rent
    /// 5. Profit verification finalizes
    pub fn build_atomic_bundle(
        &self,
        target_mint: &Pubkey,
        input_sol_lamports: u64,
        min_profit_lamports: u64,
        swap_leg_1: Instruction,
        swap_leg_2: Instruction,
        pool_type: PoolType,
        current_slot: u64,
    ) -> Result<AtomicBundle, RouterError> {
        
        // Validate token program compatibility
        self.validate_token_program(&swap_leg_1)?;
        self.validate_token_program(&swap_leg_2)?;

        let budget = self.select_compute_budget(pool_type);
        
        // Derive ephemeral ATA with correct token program
        let ephemeral_ata = get_associated_token_address_with_program_id(
            &self.wallet,
            target_mint,
            &self.token_program,
        );

        // Initialize lifecycle tracking
        let lifecycle = EphemeralAtaLifecycle::new(
            ephemeral_ata,
            *target_mint,
            self.token_program,
            current_slot,
        );

        // Build instruction sequence
        let mut ixs = Vec::with_capacity(7);

        // Ix 0: Set compute unit limit (MUST be first)
        ixs.push(ComputeBudgetInstruction::set_compute_unit_limit(budget.units()));
        
        // Ix 1: Set compute unit price (MUST be second, or first if no limit set)
        ixs.push(ComputeBudgetInstruction::set_compute_unit_price(
            budget.micro_lamports()
        ));

        // Ix 2: Create ATA idempotent (handles Token-2022)
        ixs.push(create_associated_token_account_idempotent_with_program_id(
            &self.wallet,                    // payer
            &self.wallet,                    // owner
            target_mint,                     // mint
            &self.token_program,             // token program
        ));

        // Ix 3-4: Swap legs (caller-provided, validated for ATA usage)
        ixs.push(swap_leg_1);
        ixs.push(swap_leg_2);

        // Ix 5: Close ATA, refund rent to wallet
        // Using raw spl_token close to avoid ATA program overhead
        ixs.push(spl_token::instruction::close_account(
            &self.token_program,
            &ephemeral_ata,
            &self.wallet,                    // rent refund destination
            &self.wallet,                    // owner
            &[],                             // no additional signers
        ).map_err(|e| RouterError::SwapConstructionFailed(e.to_string()))?);

        // Ix 6: Atomic profit check
        ixs.push(self.build_profit_check_ix(
            input_sol_lamports,
            min_profit_lamports,
        )?);

        Ok(AtomicBundle {
            instructions: ixs,
            lifecycle,
            ephemeral_ata,
            estimated_cus: budget.units(),
        })
    }

    /// Validate that swap instructions reference compatible token programs.
    fn validate_token_program(&self, ix: &Instruction) -> Result<(), RouterError> {
        // Check if instruction accounts reference our token program
        let has_token_ref = ix.accounts.iter().any(|meta| {
            meta.pubkey == self.token_program || 
            meta.pubkey == spl_token::id() ||
            meta.pubkey == spl_token_2022::id()
        });
        
        if !has_token_ref {
            // Non-token instruction, skip validation
            return Ok(());
        }

        // Verify program ID matches expected
        if ix.program_id != self.token_program && 
           (ix.program_id == spl_token::id() || ix.program_id == spl_token_2022::id()) {
            return Err(RouterError::TokenProgramMismatch {
                expected: self.token_program,
                actual: ix.program_id,
            });
        }
        
        Ok(())
    }

    /// Build profit verification instruction.
    /// 
    /// Invokes on-chain program that verifies:
    /// - Post-execution wallet balance >= pre-execution balance + min_profit
    /// - Transaction succeeded atomically
    fn build_profit_check_ix(
        &self,
        input_lamports: u64,
        min_profit_lamports: u64,
    ) -> Result<Instruction, RouterError> {
        use solana_sdk::instruction::AccountMeta;
        
        let min_out = input_lamports
            .checked_add(min_profit_lamports)
            .ok_or(RouterError::SwapConstructionFailed(
                "Profit overflow".to_string()
            ))?;

        // Serialize: [discriminator: u8 = 0, min_out: u64]
        let mut data = vec![0u8];
        data.extend_from_slice(&min_out.to_le_bytes());

        Ok(Instruction::new_with_bytes(
            self.profit_check_program,
            &data,
            vec![
                AccountMeta::new(self.wallet, false), // wallet to check
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::id(), false),
            ],
        ))
    }
}

/// Re-export for convenience
pub use crate::types::PoolType;
```

### `src/scanner.rs`
```rust
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use solana_client::{
    rpc_client::RpcClient,
    rpc_config::RpcAccountInfoConfig,
    rpc_filter::{RpcFilterType, Memcmp, MemcmpEncodedBytes},
};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signer::Signer,
    transaction::Transaction,
};
use tokio::sync::mpsc;
use crate::types::{EphemeralAtaLifecycle, LifecycleError};

/// Background rent reclamation engine with safety guarantees.
pub struct RentReclaimEngine {
    rpc: Arc<RpcClient>,
    /// Thread-safe lifecycle registry
    lifecycles: Arc<RwLock<HashMap<Pubkey, Arc<EphemeralAtaLifecycle>>>>,
    /// Channel for reclamation results
    result_tx: mpsc::Sender<ReclaimResult>,
    /// Wallet for signing reclamation transactions
    wallet: Arc<dyn Signer + Send + Sync>,
    /// Minimum slot distance before reclamation (prevent race with pending txs)
    min_slot_distance: u64,
}

#[derive(Debug, Clone)]
pub struct ReclaimResult {
    pub ata: Pubkey,
    pub success: bool,
    pub lamports_reclaimed: u64,
    pub error: Option<String>,
}

/// Configuration for scanner behavior.
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    pub audit_interval_secs: u64,
    pub min_slot_distance: u64,
    pub max_concurrent_reclaims: usize,
    pub rent_exempt_threshold: u64, // Minimum balance to consider "empty"
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            audit_interval_secs: 600, // 10 minutes
            min_slot_distance: 32,     // ~2-3 slots per second, 32 = ~10-15s
            max_concurrent_reclaims: 4,
            rent_exempt_threshold: 2_039_280, // Token account rent exempt
        }
    }
}

impl RentReclaimEngine {
    pub fn new(
        rpc_url: String,
        wallet: Arc<dyn Signer + Send + Sync>,
        config: ScannerConfig,
    ) -> (Self, mpsc::Receiver<ReclaimResult>) {
        let (result_tx, result_rx) = mpsc::channel(1024);
        
        let engine = Self {
            rpc: Arc::new(RpcClient::new_with_commitment(
                rpc_url,
                CommitmentConfig::confirmed(),
            )),
            lifecycles: Arc::new(RwLock::new(HashMap::new())),
            result_tx,
            wallet,
            min_slot_distance: config.min_slot_distance,
        };
        
        (engine, result_rx)
    }

    /// Register a new ephemeral ATA for monitoring.
    pub fn track_lifecycle(&self, lifecycle: EphemeralAtaLifecycle) {
        let mut guard = self.lifecycles.write().unwrap();
        guard.insert(
            lifecycle.ata_address,
            Arc::new(lifecycle),
        );
    }

    /// Spawn the background auditor task.
    pub fn spawn_auditor(
        self: Arc<Self>,
        config: ScannerConfig,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                Duration::from_secs(config.audit_interval_secs)
            );
            
            loop {
                interval.tick().await;
                
                let current_slot = match self.rpc.get_slot().await {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!("[Reclaim] Failed to get current slot: {}", e);
                        continue;
                    }
                };
                
                if let Err(e) = self.run_audit_cycle(current_slot, &config).await {
                    log::error!("[Reclaim] Audit cycle error: {}", e);
                }
            }
        })
    }

    async fn run_audit_cycle(
        &self,
        current_slot: u64,
        config: &ScannerConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        
        let candidates = {
            let guard = self.lifecycles.read().unwrap();
            guard
                .iter()
                .filter_map(|(addr, lifecycle)| {
                    // Slot-based safety check
                    let slot_age = current_slot.saturating_sub(lifecycle.creation_slot);
                    let time_eligible = lifecycle.can_reclaim();
                    let slot_eligible = slot_age >= self.min_slot_distance;
                    
                    if time_eligible && slot_eligible && !lifecycle.is_closed.load(std::sync::atomic::Ordering::Relaxed) {
                        Some((*addr, Arc::clone(lifecycle)))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };

        log::info!(
            "[Reclaim] Audit: {} candidates from {} tracked",
            candidates.len(),
            self.lifecycles.read().unwrap().len()
        );

        // Process with concurrency limit
        let semaphore = Arc::new(tokio::sync::Semaphore::new(config.max_concurrent_reclaims));
        
        let mut handles = Vec::new();
        for (addr, lifecycle) in candidates {
            let permit = semaphore.clone().acquire_owned().await?;
            let rpc = Arc::clone(&self.rpc);
            let wallet = Arc::clone(&self.wallet);
            let result_tx = self.result_tx.clone();
            let lifecycles = Arc::clone(&self.lifecycles);
            
            let handle = tokio::spawn(async move {
                let _permit = permit; // Hold until complete
                
                if !lifecycle.mark_pending_close() {
                    return; // Another task got it
                }
                
                let result = Self::attempt_reclaim(
                    rpc,
                    wallet,
                    addr,
                    lifecycle,
                    config,
                ).await;
                
                // Update lifecycle state
                if result.success {
                    lifecycle.is_closed.store(true, std::