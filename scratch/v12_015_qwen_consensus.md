# Binding Consensus Decree & Rust Blueprint for 0.15 SOL V12 Deployment

Based on the comprehensive 4-model council deliberation and forensic audits from GLM-5 and Kimi K2.5, here is the synthesized final binding consensus decree and Rust implementation blueprint.

## Final Binding Consensus Decree

```rust
// ══════════════════════════════════════════════════════════════════════════════
// BINDING CONSENSUS DECREE FOR 0.15 SOL V12 DEPLOYMENT
// ══════════════════════════════════════════════════════════════════════════════

// ┌────────────────────────────────────────────────────────────────────────────┐
// │                              POSITION SIZING                               │
// └────────────────────────────────────────────────────────────────────────────┘

/// Maximum allowable position size (reduced from original proposal)
pub const MAX_POSITION_LAMPORTS: u64 = 145_000_000; // 0.145 SOL

/// Default position size used when no specific amount is specified
pub const DEFAULT_POSITION_LAMPORTS: u64 = 140_000_000; // 0.140 SOL

/// Buffer to account for potential slippage in LST swaps
pub const POSITION_BUFFER_LAMPORTS: u64 = 5_000_000; // 0.005 SOL

// ┌────────────────────────────────────────────────────────────────────────────┐
// │                             RESERVE MANAGEMENT                             │
// └────────────────────────────────────────────────────────────────────────────┘

/// Absolute minimum reserve that must remain in wallet at all times
pub const RESERVE_FLOOR_LAMPORTS: u64 = 200_000_000; // 0.20 SOL (raised from 0.15 SOL)

/// Operational buffer for fees and unexpected costs
pub const OPERATIONAL_BUFFER_LAMPORTS: u64 = 10_000_000; // 0.01 SOL

/// Worst-case fee estimation for complex transactions
pub const MAX_FEE_ESTIMATE_LAMPORTS: u64 = 15_000_000; // 0.015 SOL

// ┌────────────────────────────────────────────────────────────────────────────┐
// │                            PROFIT THRESHOLDS                               │
// └────────────────────────────────────────────────────────────────────────────┘

/// Minimum profit threshold for dual-leg trades (includes expected tip cost)
pub const MIN_DUAL_LEG_PROFIT: u64 = 55_000; // 0.000055 SOL

/// Minimum profit threshold for cross-base trades (higher due to LST risks)
pub const MIN_CROSS_BASE_PROFIT: u64 = 70_000; // 0.000070 SOL

// ┌────────────────────────────────────────────────────────────────────────────┐
// │                           JITO SHOTGUN CONFIG                              │
// └────────────────────────────────────────────────────────────────────────────┘

/// Base tip for secondary bundle submission
pub const BASE_JITO_TIP: u64 = 12_000; // 0.000012 SOL

/// Priority tip for primary bundle submission
pub const PRIORITY_JITO_TIP: u64 = 20_000; // 0.000020 SOL

/// Maximum concurrent tip exposure (only one bundle should land)
pub const MAX_CONCURRENT_TIP_EXPOSURE: u64 = 20_000; // Not 32,000 - loser canceled

// ┌────────────────────────────────────────────────────────────────────────────┐
// │                          LST ROUTING PARAMETERS                            │
// └────────────────────────────────────────────────────────────────────────────┘

/// JitoSOL mint address
pub const JITOSOL_MINT: &str = "J1tGGRPywGUhyvS5S2nscg9dE7XCc2EwKmLwcyc5KFRu";

/// jupSOL mint address
pub const JUPSOL_MINT: &str = "jupSoLaHXQiZZTSfEWMTRRfpnygnGy5StkeQUAh3st4";

/// Maximum allowed slippage in basis points for LST swaps
pub const MAX_LST_SLIPPAGE_BPS: u16 = 50; // 0.5%

/// Threshold for LST depeg detection (fallback to USDC if exceeded)
pub const LST_DEPEG_THRESHOLD_BPS: u16 = 30; // 0.3%

// ┌────────────────────────────────────────────────────────────────────────────┐
// │                              SAFETY LIMITS                                 │
// └────────────────────────────────────────────────────────────────────────────┘

/// Timeout period for flight locks in slots (~60 seconds)
pub const FLIGHT_TIMEOUT_SLOTS: u64 = 120;

// ┌────────────────────────────────────────────────────────────────────────────┐
// │                         STATE MACHINE DEFINITIONS                          │
// └────────────────────────────────────────────────────────────────────────────┘

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum FlightState {
    Idle = 0,
    Validating = 1,
    Bundling = 2,
    InFlight = 3,
    Recovering = 4,
}

impl From<u8> for FlightState {
    fn from(value: u8) -> Self {
        match value {
            0 => FlightState::Idle,
            1 => FlightState::Validating,
            2 => FlightState::Bundling,
            3 => FlightState::InFlight,
            4 => FlightState::Recovering,
            _ => FlightState::Idle,
        }
    }
}

// ┌──────────────────────────────────────────────────────��─────────────────────┐
// │                              ERROR DEFINITIONS                             │
// └────────────────────────────────────────────────────────────────────────────┘

#[derive(Debug)]
pub enum SafetyError {
    InsufficientCapital { available: u64, required_minimum: u64 },
    LockAcquisitionFailed(FlightError),
    PositionAlreadyInFlight,
    InsufficientReserveBuffer,
}

#[derive(Debug)]
pub enum FlightError {
    StateConflict(FlightState),
    InsufficientBuffer,
}
```

## Rust Implementation Blueprint

```rust
// ══════════════════════════════════════════════════════════════════════════════
// RUST BLUEPRINT FOR 0.15 SOL V12 DEPLOYMENT
// ══════════════════════════════════════════════════════════════════════════════

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use solana_program::{
    pubkey::Pubkey,
    system_instruction,
    sysvar::clock::Clock,
};
use solana_sdk::{
    transaction::VersionedTransaction,
    signature::Keypair,
};

// ┌────────────────────────────────────────────────────────────────────────────┐
// │                        FLIGHT LOCK IMPLEMENTATION                          │
// └────────────────────────────────────────────────────────────────────────────┘

pub struct FlightLock {
    state: AtomicU8,
    position_lamports: AtomicU64,
    entry_slot: AtomicU64,
}

impl FlightLock {
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(FlightState::Idle as u8),
            position_lamports: AtomicU64::new(0),
            entry_slot: AtomicU64::new(0),
        }
    }

    /// Attempt to acquire the flight lock with validation
    pub fn try_acquire(
        &self,
        requested_size: u64,
        wallet_balance: u64,
        current_slot: u64,
    ) -> Result<FlightGuard, SafetyError> {
        // First check for timeout recovery
        if self.check_timeout(current_slot) {
            log::warn!("Recovered from timeout at slot {}", current_slot);
        }

        // Compare-and-swap transition: Idle -> Validating
        match self.state.compare_exchange(
            FlightState::Idle as u8,
            FlightState::Validating as u8,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => {
                // Validate under lock to prevent race conditions
                let reserve_required = RESERVE_FLOOR_LAMPORTS
                    .saturating_add(OPERATIONAL_BUFFER_LAMPORTS)
                    .saturating_add(MAX_FEE_ESTIMATE_LAMPORTS);
                
                let available = wallet_balance.saturating_sub(reserve_required);
                
                if requested_size > available {
                    self.state.store(FlightState::Idle as u8, Ordering::SeqCst);
                    return Err(SafetyError::InsufficientCapital {
                        available: wallet_balance,
                        required_minimum: reserve_required.saturating_add(requested_size),
                    });
                }
                
                // Clamp position size to maximum allowed
                let clamped_size = requested_size.min(MAX_POSITION_LAMPORTS);
                
                // Commit position size atomically
                self.position_lamports.store(clamped_size, Ordering::SeqCst);
                self.entry_slot.store(current_slot, Ordering::SeqCst);
                
                // Transition to Bundling
                self.state.store(FlightState::Bundling as u8, Ordering::SeqCst);
                
                Ok(FlightGuard { lock: self })
            }
            Err(current) => Err(SafetyError::LockAcquisitionFailed(
                FlightError::StateConflict(FlightState::from(current))
            )),
        }
    }
    
    /// Check for timeout condition and force reset if needed
    pub fn check_timeout(&self, current_slot: u64) -> bool {
        let entry = self.entry_slot.load(Ordering::SeqCst);
        let state = self.state.load(Ordering::SeqCst);
        
        if state != FlightState::Idle as u8 && 
           current_slot.saturating_sub(entry) > FLIGHT_TIMEOUT_SLOTS {
            self.force_reset();
            true
        } else {
            false
        }
    }
    
    /// Force reset of the flight lock (emergency recovery)
    fn force_reset(&self) {
        self.state.store(FlightState::Idle as u8, Ordering::SeqCst);
        self.position_lamports.store(0, Ordering::SeqCst);
        self.entry_slot.store(0, Ordering::SeqCst);
        log::error!("Flight lock forcibly reset due to timeout");
    }
    
    /// Get current position size
    pub fn position_size(&self) -> u64 {
        self.position_lamports.load(Ordering::Relaxed)
    }
}

// ┌────────────────────────────────────────────────────────────────────────────┐
// │                           FLIGHT GUARD RAII                                │
// └────────────────────────────────────────────────────────────────────────────┘

/// RAII guard ensuring proper cleanup of flight lock
pub struct FlightGuard<'a> {
    lock: &'a FlightLock,
}

impl Drop for FlightGuard<'_> {
    fn drop(&mut self) {
        self.lock.state.store(FlightState::Idle as u8, Ordering::SeqCst);
        self.lock.position_lamports.store(0, Ordering::SeqCst);
    }
}

// ┌────────────────────────────────────────────────────────────────────────────┐
// │                      POSITION VALIDATION FUNCTION                          │
// └────────────────────────────────────────────────────────────────────────────┘

/// Validate position safety before execution
pub fn validate_position_safety(
    wallet_balance: u64,
    position_size: u64,
    flight_lock: &FlightLock,
    current_slot: u64,
) -> Result<FlightGuard, SafetyError> {
    // Timeout check first
    if flight_lock.check_timeout(current_slot) {
        log::warn!("Recovered from timeout at slot {}", current_slot);
    }
    
    // Hard reserve check
    let minimum_operational = RESERVE_FLOOR_LAMPORTS
        .saturating_add(OPERATIONAL_BUFFER_LAMPORTS)
        .saturating_add(MAX_FEE_ESTIMATE_LAMPORTS);
    
    let maximum_position = wallet_balance.saturating_sub(minimum_operational);
    let clamped_position = position_size.min(MAX_POSITION_LAMPORTS).min(maximum_position);
    
    if clamped_position < DEFAULT_POSITION_LAMPORTS {
        return Err(SafetyError::InsufficientCapital {
            available: wallet_balance,
            required_minimum: minimum_operational.saturating_add(DEFAULT_POSITION_LAMPORTS),
        });
    }
    
    // Acquire lock with validated size
    flight_lock.try_acquire(clamped_position, wallet_balance, current_slot)
}

// ┌────────────────────────────────────────────────────────────────────────────┐
// │                     JITO SHOTGUN BUNDLING STRUCTURE                        │
// └────────────────────────────────────────────────────────────────────────────┘

pub struct BundleTier {
    pub tip_lamports: u64,
    pub engine_url: &'static str,
    pub priority: u8,
}

pub struct ShotgunConfig {
    pub tier1: BundleTier,
    pub tier2: BundleTier,
    pub timeout_ms: u64,
}

pub const SHOTGUN_TIERS: [BundleTier; 2] = [
    BundleTier {
        tip_lamports: PRIORITY_JITO_TIP,
        engine_url: "https://frankfurt.mainnet.block-engine.jito.wtf",
        priority: 0,
    },
    BundleTier {
        tip_lamports: BASE_JITO_TIP,
        engine_url: "https://amsterdam.mainnet.block-engine.jito.wtf",
        priority: 1,
    },
];

// ┌────────────────────────────────────────────────────────────────────────────┐
// │                        LST ROUTING UTILITIES                               │
// └────────────────────────────────────────────────────────────────────────────┘

/// Check if LSTs are within acceptable depeg thresholds
pub fn check_lst_depeg(jitosol_price: f64, jupsol_price: f64) -> bool {
    let jitosol_depeg_bps = ((1.0 - jitosol_price).abs() * 10000.0) as u16;
    let jupsol_depeg_bps = ((1.0 - jupsol_price).abs() * 10000.0) as u16;
    
    jitosol_depeg_bps <= LST_DEPEG_THRESHOLD_BPS && 
    jupsol_depeg_bps <= LST_DEPEG_THRESHOLD_BPS
}

/// Build a bundle with appropriate tip
pub fn build_bundle(
    transactions: &[VersionedTransaction], 
    tip_lamports: u64
) -> Result<Vec<VersionedTransaction>, Box<dyn std::error::Error>> {
    let mut bundle_transactions = transactions.to_vec();
    
    // Add tip instruction to last transaction
    if let Some(last_tx) = bundle_transactions.last_mut() {
        // In practice, you would add a tip transfer to the Jito tip account
        // This is a simplified representation
    }
    
    Ok(bundle_transactions)
}

// ┌────────────────────────────────────────────────────────────────────────────┐
// │                           WALLET BALANCE CHECK                             │
// └────────────────────────────────────────────────────────────────────────────┘

/// Calculate available trading capital after reserves
pub fn calculate_available_capital(wallet_balance: u64) -> u64 {
    let required_reserve = RESERVE_FLOOR_LAMPORTS
        .saturating_add(OPERATIONAL_BUFFER_LAMPORTS)
        .saturating_add(MAX_FEE_ESTIMATE_LAMPORTS);
    
    wallet_balance.saturating_sub(required_reserve)
}

// ┌────────────────────────────────────────────────────────────────────────────┐
// │                          PROFIT CALCULATION                                │
// └────────────────────────────────────────────────────────────────────────────┘

/// Determine if a trade meets minimum profit requirements
pub fn meets_profit_threshold(
    gross_profit: u64,
    is_cross_base: bool,
    tip_paid: u64,
) -> bool {
    let min_profit = if is_cross_base {
        MIN_CROSS_BASE_PROFIT
    } else {
        MIN_DUAL_LEG_PROFIT
    };
    
    gross_profit.saturating_sub(tip_paid) >= min_profit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_validation_safe() {
        let flight_lock = FlightLock::new();
        let wallet_balance = 398_100_000; // 0.3981 SOL
        let position_size = 140_000_000; // 0.14 SOL
        let current_slot = 1000;
        
        let result = validate_position_safety(
            wallet_balance,
            position_size,
            &flight_lock,
            current_slot
        );
        
        assert!(result.is_ok());
        assert_eq!(flight_lock.position_size(), position_size);
    }

    #[test]
    fn test_position_validation_insufficient_funds() {
        let flight_lock = FlightLock::new();
        let wallet_balance = 300_000_000; // 0.3 SOL
        let position_size = 145_000_000; // 0.145 SOL (too large)
        let current_slot = 1000;
        
        let result = validate_position_safety(
            wallet_balance,
            position_size,
            &flight_lock,
            current_slot
        );
        
        assert!(matches!(result, Err(SafetyError::InsufficientCapital { .. })));
    }

    #[test]
    fn test_flight_lock_timeout_recovery() {
        let flight_lock = FlightLock::new();
        let wallet_balance = 398_100_000;
        let position_size = 140_000_000;
        let entry_slot = 1000;
        
        // Manually set up a locked state
        flight_lock.state.store(FlightState::Bundling as u8, Ordering::SeqCst);
        flight_lock.position_lamports.store(position_size, Ordering::SeqCst);
        flight_lock.entry_slot.store(entry_slot, Ordering::SeqCst);
        
        // Check timeout with current slot far ahead
        let timeout_detected = flight_lock.check_timeout(entry_slot + FLIGHT_TIMEOUT_SLOTS + 1);
        
        assert!(timeout_detected);
        assert_eq!(flight_lock.state.load