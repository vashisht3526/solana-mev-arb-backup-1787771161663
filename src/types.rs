#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::fmt;

/// Supported Decentralized Exchanges
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DexType {
    PumpSwap,
    RaydiumCpmm,
    RaydiumClmm,
    MeteoraDlmm,
    OrcaWhirlpool,
}

impl DexType {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        let s_lower = s.to_lowercase();
        if s_lower.contains("pump") {
            Some(DexType::PumpSwap)
        } else if s_lower.contains("raydium") || s_lower.contains("cpmm") {
            Some(DexType::RaydiumCpmm)
        } else if s_lower.contains("clmm") {
            Some(DexType::RaydiumClmm)
        } else if s_lower.contains("meteora") || s_lower.contains("dlmm") {
            Some(DexType::MeteoraDlmm)
        } else if s_lower.contains("orca") || s_lower.contains("whirlpool") {
            Some(DexType::OrcaWhirlpool)
        } else {
            None
        }
    }

    pub fn to_display(&self) -> &'static str {
        match self {
            DexType::PumpSwap => "PUMPSWAP",
            DexType::RaydiumCpmm => "RAYDIUM",
            DexType::RaydiumClmm => "RAYDIUM_CLMM",
            DexType::MeteoraDlmm => "METEORA",
            DexType::OrcaWhirlpool => "ORCA",
        }
    }
}

/// Authority state representation for honeypot and rug shield verification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorityState {
    Active,
    Revoked,
}

impl AuthorityState {
    pub fn is_revoked(&self) -> bool {
        matches!(self, AuthorityState::Revoked)
    }

    pub fn from_option<T>(opt: Option<T>) -> Self {
        match opt {
            None => AuthorityState::Revoked,
            Some(_) => AuthorityState::Active,
        }
    }
}

/// Token Risk Metadata evaluated before admitting pool into cross-DEX matrix
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRiskMeta {
    pub mint: String,
    pub decimals: u8,
    pub mint_authority: AuthorityState,
    pub freeze_authority: AuthorityState,
    pub close_authority: AuthorityState,
    pub transfer_fee_bps: u16,
    pub is_token_2022: bool,
    pub has_permanent_delegate: bool,
}

impl TokenRiskMeta {
    pub const MAX_ACCEPTABLE_FEE_BPS: u16 = 5; // 5 bps tolerance for standard tokens

    pub fn safe_default(mint: &str) -> Self {
        Self {
            mint: mint.to_string(),
            decimals: 9,
            mint_authority: AuthorityState::Revoked,
            freeze_authority: AuthorityState::Revoked,
            close_authority: AuthorityState::Revoked,
            transfer_fee_bps: 0,
            is_token_2022: false,
            has_permanent_delegate: false,
        }
    }

    pub fn validate_risk_shields(&self) -> Result<(), RiskShieldViolation> {
        if !self.mint_authority.is_revoked() {
            return Err(RiskShieldViolation::MintAuthorityActive);
        }
        if !self.freeze_authority.is_revoked() {
            return Err(RiskShieldViolation::FreezeAuthorityActive);
        }
        if !self.close_authority.is_revoked() {
            return Err(RiskShieldViolation::CloseAuthorityActive);
        }
        if self.transfer_fee_bps > Self::MAX_ACCEPTABLE_FEE_BPS {
            return Err(RiskShieldViolation::TransferFeePresent {
                bps: self.transfer_fee_bps,
            });
        }
        if self.has_permanent_delegate {
            return Err(RiskShieldViolation::PermanentDelegatePresent);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskShieldViolation {
    MintAuthorityActive,
    FreezeAuthorityActive,
    CloseAuthorityActive,
    TransferFeePresent { bps: u16 },
    PermanentDelegatePresent,
}

impl fmt::Display for RiskShieldViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RiskShieldViolation::MintAuthorityActive => write!(f, "mint authority active: inflation rug risk"),
            RiskShieldViolation::FreezeAuthorityActive => write!(f, "freeze authority active: honeypot risk"),
            RiskShieldViolation::CloseAuthorityActive => write!(f, "close authority active: supply destruction risk"),
            RiskShieldViolation::TransferFeePresent { bps } => write!(f, "transfer fee present: {} bps", bps),
            RiskShieldViolation::PermanentDelegatePresent => write!(f, "permanent delegate present: arbitrary transfer risk"),
        }
    }
}

impl std::error::Error for RiskShieldViolation {}

/// Dynamic Compute Budget Configuration (Upgraded to 180k CUs for Ephemeral ATA + DLMM)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputeBudgetConfig {
    pub cu_limit: u32,
    pub cu_price_micro_lamports: u64,
}

impl ComputeBudgetConfig {
    /// 180,000 CUs for Meteora DLMM / Concentrated AMMs + Ephemeral ATA Create/Close
    pub const EPHEMERAL_COMPLEX: Self = Self {
        cu_limit: 180_000,
        cu_price_micro_lamports: 5_000,
    };

    /// 125,000 CUs for standard CPMM (Raydium CPMM / PumpSwap) + Ephemeral ATA Create/Close
    pub const EPHEMERAL_STANDARD: Self = Self {
        cu_limit: 125_000,
        cu_price_micro_lamports: 5_000,
    };
}

/// Tracks ephemeral ATA state across in-transaction and background reclamation lifecycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EphemeralAtaLifecycle {
    pub mint: String,
    pub ata_address: String,
    pub rent_lamports: u64,
    pub creation_timestamp_ms: i64,
    pub is_closed: bool,
}

impl EphemeralAtaLifecycle {
    pub fn new(mint: &str, ata_address: &str) -> Self {
        Self {
            mint: mint.to_string(),
            ata_address: ata_address.to_string(),
            rent_lamports: 2_039_280, // Standard ATA rent exemption
            creation_timestamp_ms: chrono::Utc::now().timestamp_millis(),
            is_closed: false,
        }
    }
}

/// Temporal Sync report for validating cross-DEX slot skew
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalSyncReport {
    pub time_skew_ms: i64,
    pub slot_skew: u64,
    pub is_synchronized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncViolation {
    CrossPoolTimeSkew { skew_ms: i64, max_ms: i64 },
    SlotVarianceExceeded { slot_skew: u64, max_slots: u64 },
}

impl fmt::Display for SyncViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncViolation::CrossPoolTimeSkew { skew_ms, max_ms } => {
                write!(f, "cross-pool time skew exceeded: {}ms > {}ms", skew_ms, max_ms)
            }
            SyncViolation::SlotVarianceExceeded { slot_skew, max_slots } => {
                write!(f, "slot variance exceeded: {} slots > {} slots", slot_skew, max_slots)
            }
        }
    }
}

impl std::error::Error for SyncViolation {}

/// Dynamic in-memory pool state across DEXes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicPoolState {
    pub mint: String,
    pub dex: String, // "PUMPSWAP", "RAYDIUM", "METEORA", "ORCA"
    pub pool_address: String,
    pub price_usd: f64,
    pub liquidity_usd: f64,
    pub last_update_ms: i64,
    pub slot: u64,
    pub risk_meta: TokenRiskMeta,
    pub is_active: bool,
}

/// Token registered in the dynamic volatile tracking registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolatileTrackedToken {
    pub mint: String,
    pub symbol: String,
    pub name: String,
    pub market_cap_usd: f64,
    pub volume_5m_usd: f64,
    pub total_liquidity_usd: f64,
    pub volatility_score: f64, // (5m_vol / liq) * 100
    pub pools: Vec<DynamicPoolState>,
    pub added_timestamp_ms: i64,
    pub last_spread_pct: f64,
    pub consecutive_flat_ticks: u32,
}

/// Arbitrage opportunity candidate evaluated in RAM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageOpportunity {
    pub timestamp_ms: i64,
    pub iso_time: String,
    pub mint: String,
    pub symbol: String,
    pub name: String,
    pub buy_dex: String,
    pub sell_dex: String,
    pub target_dex: String,
    pub pair_address: String,
    pub pump_price_usd: f64,
    pub raydium_price_usd: f64,
    pub gross_spread_pct: f64,
    pub net_spread_pct: f64,
    pub pool_liquidity_usd: f64,
    pub simulated_swap_sol: f64,
    pub gross_profit_usd: f64,
    pub jito_tip_usd: f64,
    pub net_profit_usd: f64,
    pub net_profit_sol: f64,
}

/// Mainnet Shadow Paper-Trading execution record written to NDJSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperTradeRecord {
    pub trade_id: String,
    pub timestamp_ms: i64,
    pub iso_time: String,
    pub mint: String,
    pub symbol: String,
    pub position_size_sol: f64,
    pub entry_pump_price_usd: f64,
    pub exit_raydium_price_usd: f64,
    pub gross_spread_pct: f64,
    pub net_spread_pct: f64,
    pub gas_fee_sol: f64,
    pub jito_tip_sol: f64,
    pub net_pnl_sol: f64,
    pub net_pnl_usd: f64,
    pub starting_wallet_balance_sol: f64,
    pub ending_wallet_balance_sol: f64,
    pub status: String, // "FILLED", "FAILED_FRONT_RUN", "FAILED_STALE_QUOTE", etc.
}

/// Decision classification for Raydium-Native Migration Sniping
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MigrationDecision {
    Accepted,
    RejectedZombiePool { swap_count: usize },
    RejectedKnife { ratio_to_p0: f64 },
    RejectedVelocityDecay { decay_pct: f64 },
    RejectedBuySellRatio { ratio: f64 },
    RejectedShieldViolation { reason: String },
    RejectedGhostSell { reason: String },
}

/// Active Simulated Holding Position for Migration Sniping Engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldingPosition {
    pub position_id: String,
    pub mint: String,
    pub symbol: String,
    pub pool_address: String,
    pub entry_timestamp_ms: i64,
    pub entry_price_usd: f64,
    pub position_size_sol: f64,
    pub token_amount: f64,
    pub local_peak_price_usd: f64,
    pub is_active: bool,
}

/// Complete Shadow Execution & Counterfactual Record for Migration Sniping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationShadowRecord {
    pub event_id: String,
    pub timestamp_ms: i64,
    pub iso_time: String,
    pub mint: String,
    pub symbol: String,
    pub pool_address: String,
    pub initial_sol_liquidity: f64,
    pub p0_open_price_usd: f64,
    pub decision: MigrationDecision,
    pub eval_price_usd: f64,
    pub price_ratio_to_p0: f64,
    pub velocity_decay_pct: f64,
    pub buy_volume_sol: f64,
    pub sell_volume_sol: f64,
    pub buy_sell_volume_ratio: f64,
    pub total_swaps: usize,
    // Execution fields (if Accepted)
    pub entry_price_usd: Option<f64>,
    pub exit_price_usd: Option<f64>,
    pub exit_reason: Option<String>, // "TAKE_PROFIT_10PCT", "TRAILING_STOP_5PCT", "HARD_STOP_10PCT", "TIME_STOP_10MIN", "TOXIC_QUARANTINE"
    pub hold_duration_seconds: Option<u64>,
    pub simulated_net_pnl_sol: Option<f64>,
    pub simulated_net_pnl_usd: Option<f64>,
    pub simulated_roi_pct: Option<f64>,
    // Counterfactual 10m post-tracking (for tuning false negatives/positives)
    pub counterfactual_p10m_usd: Option<f64>,
    pub counterfactual_max_price_usd: Option<f64>,
    pub would_have_profited: Option<bool>,
    pub rug_detected: Option<bool>,
}

/// Volatile Micro-Cap Sweet Spot & Whale-Exclusion Zone ($5k - $2.5M TVL)
/// Captures high retail volatility with wide spreads, excluding large institutional bots (>50 SOL).
pub const MIN_VOLATILE_LIQUIDITY_USD: f64 = 5_000.0;
pub const MAX_VOLATILE_LIQUIDITY_USD: f64 = 2_500_000.0;
pub const WHALE_EXCLUSION_MIN_TVL: f64 = 5_000.0;
pub const WHALE_EXCLUSION_MAX_TVL: f64 = 2_500_000.0;
pub const TARGET_TRADE_SIZE_SOL: f64 = 0.15;            // 0.15 SOL target (dynamic sizing scales down for thin pools)

/// 3-Hop Triangular Spatial Arbitrage Opportunity Candidate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriangularRouteOpportunity {
    pub timestamp_ms: i64,
    pub iso_time: String,
    pub token_a_mint: String,
    pub token_a_symbol: String,
    pub token_b_mint: String, // Typically USDC or intermediate token
    pub token_b_symbol: String,
    pub leg1_dex: String,     // SOL -> TokenA (e.g. Raydium CPMM)
    pub leg2_dex: String,     // TokenA -> TokenB (e.g. Meteora DLMM)
    pub leg3_dex: String,     // TokenB -> SOL (e.g. Orca Whirlpool)
    pub leg1_pool: String,
    pub leg2_pool: String,
    pub leg3_pool: String,
    pub input_sol: f64,
    pub expected_sol_out: f64,
    pub gross_spread_pct: f64,
    pub net_profit_sol: f64,
    pub net_profit_usd: f64,
    pub dynamic_jito_tip_sol: f64,
    pub target_tvl_usd: f64,
    pub is_whale_exclusion_zone: bool,
}


