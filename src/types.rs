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
    pub const MAX_ACCEPTABLE_FEE_BPS: u16 = 0;

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

/// Dynamic Compute Budget Configuration (Upgraded to 135k CUs for DLMM bin crossing)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputeBudgetConfig {
    pub cu_limit: u32,
    pub cu_price_micro_lamports: u64,
}

impl ComputeBudgetConfig {
    pub const MEV_STANDARD: Self = Self {
        cu_limit: 135_000,
        cu_price_micro_lamports: 5_000,
    };

    pub const CONSERVATIVE_CPMM: Self = Self {
        cu_limit: 85_000,
        cu_price_micro_lamports: 5_000,
    };
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
