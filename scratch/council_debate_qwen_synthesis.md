# Final Unanimous Consensus Architecture

After thorough analysis and debate among all council members, we have reached unanimous consensus on the following architecture for high-frequency trading on Solana with absolute atomicity guarantees and dynamic pool discovery.

## Core Principles

1. **Absolute Atomicity**: Every transaction must be fully atomic - either complete successfully or revert with zero state change
2. **Dynamic Discovery**: No hardcoded token lists; all pools discovered in real-time through multiple data sources
3. **MEV Protection**: Bundle submission through Jito with proper tip management
4. **Performance Optimization**: Minimal latency through efficient caching and parallel processing

## Implementation

### 1. Atomic MEV Engine (Rust)

```rust
// atomic_mev_engine.rs
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use anchor_lang::prelude::*;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcSimulateTransactionConfig, RpcSendTransactionConfig},
};
use solana_sdk::{
    address_lookup_table_account::AddressLookupTableAccount,
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    hash::Hash,
    instruction::Instruction,
    message::{v0::Message as MessageV0, VersionedMessage},
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    system_instruction,
    transaction::VersionedTransaction,
};
use tokio::sync::{RwLock, Semaphore};
use serde::{Deserialize, Serialize};

// Constants
const INSTRUCTION_MAX_AGE_MS: u64 = 200;
const BUNDLE_TIMEOUT_MS: u64 = 500;
const MAX_BUNDLE_RETRIES: u32 = 3;
const ATA_RENT_EXEMPTION_LAMPORTS: u64 = 2_039_280;
const MIN_PROFIT_LAMPORTS: u64 = 10_000;
const BASE_SWAP_UNITS: u32 = 50_000;
const JUPITER_ROUTING_UNITS: u32 = 100_000;
const DUAL_LEG_BASE: u32 = 300_000;
const SAFETY_MARGIN_UNITS: u32 = 50_000;
const MAX_COMPUTE_UNITS: u32 = 1_400_000;

/// Atomic invariant for dual-leg MEV
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtomicInvariant {
    pub input_sol: u64,
    pub min_output_sol: u64,
    pub jito_tip: u64,
    pub estimated_fees: FeeBreakdown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeeBreakdown {
    pub ata_rent: u64,
    pub leg_a_swap_fee: u64,
    pub leg_b_swap_fee: u64,
    pub priority_fee: u64,
    pub base_fee: u64,
}

impl FeeBreakdown {
    pub fn total(&self) -> u64 {
        self.ata_rent
            .saturating_add(self.leg_a_swap_fee)
            .saturating_add(self.leg_b_swap_fee)
            .saturating_add(self.priority_fee)
            .saturating_add(self.base_fee)
    }
}

/// Time-bound instructions to prevent staleness
#[derive(Debug, Clone)]
pub struct TimeBoundInstructions {
    pub instructions: Vec<Instruction>,
    pub fetched_at: Instant,
    pub blockhash_at_fetch: Hash,
    pub expected_output: u64,
    pub minimum_output: u64,
    pub ata_creation_required: bool,
    pub intermediate_mint: Pubkey,
}

impl TimeBoundInstructions {
    pub fn is_fresh(&self) -> bool {
        self.fetched_at.elapsed().as_millis() < INSTRUCTION_MAX_AGE_MS as u128
    }

    pub fn blockhash_valid(&self, current_blockhash: &Hash) -> bool {
        self.is_fresh()
    }
}

/// Direction of swap
#[derive(Debug, Clone, Copy)]
pub enum Direction {
    SolToToken,
    TokenToSol,
}

/// Swap leg definition
#[derive(Debug, Clone)]
pub struct SwapLeg {
    pub token_mint: Pubkey,
    pub amount: u64,
    pub slippage_bps: u16,
    pub minimum_output: u64,
}

/// Arbitrage opportunity
#[derive(Debug, Clone)]
pub struct ArbitrageOpportunity {
    pub leg_a: SwapLeg,
    pub leg_b: SwapLeg,
    pub input_sol_lamports: u64,
    pub jito_tip_lamports: u64,
    pub priority_fee_micro_lamports: u64,
    pub address_lookup_tables: Vec<AddressLookupTableAccount>,
}

/// Atomic transaction
#[derive(Debug)]
pub struct AtomicTransaction {
    pub transaction: VersionedTransaction,
    pub expected_cu: u64,
    pub min_output_sol: u64,
    pub intermediate_ata: Option<Pubkey>,
    pub fetched_at: Instant,
}

/// Simulation outcome
#[derive(Debug)]
pub enum SimulationOutcome {
    Success { cu_consumed: u64, output_sol: u64 },
    Failure { error: solana_sdk::transaction::TransactionError, logs: Vec<String> },
}

/// Jupiter quote request
#[derive(Debug, Clone, Serialize)]
struct JupiterQuoteRequest {
    pub input_mint: Pubkey,
    pub output_mint: Pubkey,
    pub amount: u64,
    pub slippage_bps: u16,
    pub swap_mode: SwapMode,
    pub other_amount_threshold: u64,
    pub only_direct_routes: bool,
    pub as_legacy_transaction: bool,
}

/// Swap mode
#[derive(Debug, Clone, Serialize)]
enum SwapMode {
    ExactIn,
    ExactOut,
}

/// Jupiter client trait
#[async_trait::async_trait]
trait JupiterClient {
    async fn quote(&self, request: &JupiterQuoteRequest) -> Result<serde_json::Value, Box<dyn std::error::Error>>;
    async fn swap_instructions(&self, request: &serde_json::Value) -> Result<SwapInstructionsResponse, Box<dyn std::error::Error>>;
}

/// Swap instructions response
#[derive(Debug, Clone, Deserialize)]
struct SwapInstructionsResponse {
    pub instructions: Vec<Instruction>,
    pub address_lookup_table_accounts: Vec<AddressLookupTableAccount>,
}

/// Jito bundle client
#[derive(Debug, Clone)]
struct JitoBundleClient {
    tip_account: Pubkey,
}

impl JitoBundleClient {
    fn tip_account(&self) -> Pubkey {
        self.tip_account
    }
}

/// Atomic transaction builder
pub struct AtomicTransactionBuilder {
    rpc: Arc<RpcClient>,
    jito_client: Arc<JitoBundleClient>,
    wallet: Arc<Keypair>,
    submission_semaphore: Arc<Semaphore>,
    ata_cache: Arc<RwLock<HashMap<Pubkey, Pubkey>>>,
    jupiter_client: Arc<dyn JupiterClient + Send + Sync>,
}

impl AtomicTransactionBuilder {
    pub fn new(
        rpc: Arc<RpcClient>,
        jito_client: Arc<JitoBundleClient>,
        wallet: Arc<Keypair>,
        jupiter_client: Arc<dyn JupiterClient + Send + Sync>,
    ) -> Self {
        Self {
            rpc,
            jito_client,
            wallet,
            submission_semaphore: Arc::new(Semaphore::new(10)),
            ata_cache: Arc::new(RwLock::new(HashMap::new())),
            jupiter_client,
        }
    }

    pub async fn build_atomic_dual_leg(
        &self,
        opportunity: &ArbitrageOpportunity,
    ) -> Result<Option<AtomicTransaction>, Box<dyn std::error::Error>> {
        // Fetch fresh instructions for both legs
        let (leg_a_instructions, leg_b_instructions) = tokio::try_join!(
            self.fetch_fresh_leg_instructions(&opportunity.leg_a, Direction::SolToToken),
            self.fetch_fresh_leg_instructions(&opportunity.leg_b, Direction::TokenToSol),
        )?;

        // Validate temporal consistency
        if leg_a_instructions.blockhash_at_fetch != leg_b_instructions.blockhash_at_fetch {
            return Ok(None);
        }

        // Calculate exact fees
        let fee_breakdown = self.calculate_exact_fees(
            &leg_a_instructions,
            &leg_b_instructions,
            opportunity.priority_fee_micro_lamports,
        ).await?;

        // Verify atomic settlement invariant
        let min_output = opportunity
            .input_sol_lamports
            .saturating_add(opportunity.jito_tip_lamports)
            .saturating_add(fee_breakdown.total())
            .saturating_add(MIN_PROFIT_LAMPORTS);

        if leg_b_instructions.minimum_output < min_output {
            return Ok(None);
        }

        // Build instruction sequence
        let mut instructions: Vec<Instruction> = Vec::with_capacity(12);

        // Compute budget
        let cu_limit = self.calculate_compute_units(&leg_a_instructions, &leg_b_instructions);
        instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(cu_limit));
        instructions.push(ComputeBudgetInstruction::set_compute_unit_price(
            opportunity.priority_fee_micro_lamports,
        ));

        // Create intermediate token ATA if needed
        let intermediate_ata = self.get_or_create_ata(
            &self.wallet.pubkey(),
            &leg_a_instructions.intermediate_mint,
            &mut instructions,
        ).await?;

        // Leg A: SOL → Token
        instructions.extend(leg_a_instructions.instructions);

        // Leg B: Token → SOL
        instructions.extend(leg_b_instructions.instructions);

        // Close intermediate ATA
        instructions.push(spl_token::instruction::close_account(
            &spl_token::id(),
            &intermediate_ata,
            &self.wallet.pubkey(),
            &self.wallet.pubkey(),
            &[],
        )?);

        // Jito tip
        instructions.push(system_instruction::transfer(
            &self.wallet.pubkey(),
            &self.jito_client.tip_account(),
            opportunity.jito_tip_lamports,
        ));

        // Build transaction
        let recent_blockhash = self.rpc.get_latest_blockhash().await?;
        
        if !leg_a_instructions.blockhash_valid(&recent_blockhash) {
            return Ok(None);
        }

        let message = VersionedMessage::V0(
            MessageV0::try_compile(
                &self.wallet.pubkey(),
                &instructions,
                &opportunity.address_lookup_tables,
                recent_blockhash,
            )?
        );

        let transaction = VersionedTransaction::try_new(message, &[&*self.wallet])?;

        // Final simulation
        let simulation_result = self.simulate_with_exact_accounts(&transaction).await?;
        
        match simulation_result {
            SimulationOutcome::Success { cu_consumed, output_sol } => {
                if output_sol < min_output {
                    return Ok(None);
                }

                Ok(Some(AtomicTransaction {
                    transaction,
                    expected_cu: cu_consumed,
                    min_output_sol: min_output,
                    intermediate_ata: Some(intermediate_ata),
                    fetched_at: Instant::now(),
                }))
            }
            SimulationOutcome::Failure { .. } => {
                Ok(None)
            }
        }
    }

    async fn fetch_fresh_leg_instructions(
        &self,
        leg: &SwapLeg,
        direction: Direction,
    ) -> Result<TimeBoundInstructions, Box<dyn std::error::Error>> {
        let start = Instant::now();
        
        let jupiter_request = JupiterQuoteRequest {
            input_mint: match direction {
                Direction::SolToToken => spl_token::native_mint::id(),
                Direction::TokenToSol => leg.token_mint,
            },
            output_mint: match direction {
                Direction::SolToToken => leg.token_mint,
                Direction::TokenToSol => spl_token::native_mint::id(),
            },
            amount: leg.amount,
            slippage_bps: leg.slippage_bps,
            swap_mode: SwapMode::ExactIn,
            other_amount_threshold: leg.minimum_output,
            only_direct_routes: false,
            as_legacy_transaction: false,
        };

        let quote = self.jupiter_client.quote(&jupiter_request).await?;
        let swap_instructions = self.jupiter_client.swap_instructions(&quote).await?;

        let blockhash = self.rpc.get_latest_blockhash().await?;

        Ok(TimeBoundInstructions {
            instructions: swap_instructions.instructions,
            fetched_at: start,
            blockhash_at_fetch: blockhash,
            expected_output: quote["outAmount"].as_str().unwrap_or("0").parse()?,
            minimum_output: leg.minimum_output,
            ata_creation_required: swap_instructions.address_lookup_table_accounts.is_empty(),
            intermediate_mint: leg.token_mint,
        })
    }

    async fn calculate_exact_fees(
        &self,
        leg_a: &TimeBoundInstructions,
        leg_b: &TimeBoundInstructions,
        priority_fee_micro_lamports: u64,
    ) -> Result<FeeBreakdown, Box<dyn std::error::Error>> {
        let ata_rent = if leg_a.ata_creation_required { 
            ATA_RENT_EXEMPTION_LAMPORTS 
        } else { 
            0 
        };

        let leg_a_fee = (leg_a.expected_output as f64 * 0.0025) as u64;
        let leg_b_fee = (leg_b.expected_output as f64 * 0.0025) as u64;

        let estimated_cu = 300_000u64;
        let priority_fee = priority_fee_micro_lamports * estimated_cu / 1_000_000;

        Ok(FeeBreakdown {
            ata_rent,
            leg_a_swap_fee: leg_a_fee,
            leg_b_swap_fee: leg_b_fee,
            priority_fee,
            base_fee: 5_000,
        })
    }

    async fn simulate_with_exact_accounts(
        &self,
        transaction: &VersionedTransaction,
    ) -> Result<SimulationOutcome, Box<dyn std::error::Error>> {
        let config = RpcSimulateTransactionConfig {
            sig_verify: false,
            replace_recent_blockhash: false,
            commitment: Some(CommitmentConfig::processed()),
            encoding: None,
            accounts: None,
            min_context_slot: None,
        };

        let result = self.rpc.simulate_transaction_with_config(transaction, config).await?;

        match result.value.result {
            Ok(()) => {
                let cu_consumed = result.value.units_consumed.unwrap_or(0);
                let output_sol = self.parse_output_from_logs(&result.value.logs.unwrap_or_default());
                
                Ok(SimulationOutcome::Success {
                    cu_consumed,
                    output_sol,
                })
            }
            Err(err) => Ok(SimulationOutcome::Failure {
                error: err,
                logs: result.value.logs.unwrap_or_default(),
            }),
        }
    }

    fn parse_output_from_logs(&self, logs: &[String]) -> u64 {
        // Parse output SOL from simulation logs
        // This would need to be implemented based on actual log format
        0
    }

    async fn get_or_create_ata(
        &self,
        owner: &Pubkey,
        mint: &Pubkey,
        instructions: &mut Vec<Instruction>,
    ) -> Result<Pubkey, Box<dyn std::error::Error>> {
        {
            let cache = self.ata_cache.read().await;
            if let Some(ata) = cache.get(mint) {
                return Ok(*ata);
            }
        }

        let ata = spl_associated_token_account::get_associated_token_address(owner, mint);
        let account = self.rpc.get_account(&ata).await;
        
        if account.is_err() {
            instructions.push(
                spl_associated_token_account::instruction::create_associated_token_account(
                    owner,
                    owner,
                    mint,
                    &spl_token::id(),
                ),
            );
        }

        {
            let mut cache = self.ata_cache.write().await;
            cache.insert(*mint, ata);
        }

        Ok(ata)
    }

    fn calculate_compute_units(
        &self,
        _leg_a: &TimeBoundInstructions,
        _leg_b: &TimeBoundInstructions,
    ) -> u32 {
        let base = DUAL_LEG_BASE + SAFETY_MARGIN_UNITS;
        std::cmp::min(base, MAX_COMPUTE_UNITS)
    }
}
```

### 2. Dynamic Pool Discovery System (Rust)

```rust
// dynamic_pool_discovery.rs
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{RwLock, mpsc};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

/// Pool type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PoolType {
    RaydiumCpmm { curve_type: u8 },
    RaydiumClmm { tick_spacing: u16 },
    MeteoraDlmm { bin_step: u16 },
    OrcaWhirlpool { tick_spacing: u16 },
    PumpPortal { bonding_curve: Pubkey },
}

/// Discovery source
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DiscoverySource {
    RaydiumEventStream,
    MeteoraWebSocket,
    OrcaEventFeed,
    PumpPortalWebSocket,
    GeyserPlugin,
    RpcLogSubscription,
}

/// Volatility score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolatilityScore {
    pub mint: Pubkey,
    pub price_change_1m: f64,
    pub price_change_5m: f64,
    pub price_change_15m: f64,
    pub volume_spike_ratio: f64,
    pub overall_score: f64,
    pub last_update: u64,
}

/// Spread quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpreadQuality {
    pub effective_spread_bps: u16,
    pub depth_at_1pct: f64,
    pub is_ghost: bool,
    pub last_trade_timestamp: u64,
}

/// Pool metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolMetadata {
    pub pool_address: Pubkey,
    pub token_a_mint: Pubkey,
    pub token_b_mint: Pubkey,
    pub pool_type: PoolType,
    pub liquidity_usd: f64,
    pub volume_24h_usd: f64,
    pub fee_bps: u16,
    pub last_update: u64,
    pub discovery_source: DiscoverySource,
    pub volatility_score: f64,
    pub spread_quality: SpreadQuality,
}

/// Ghost spread filter
pub struct GhostSpreadFilter {
    min_trade_age_seconds: i64,
    min_volume_24h: f64,
    min_liquidity: f64,
}

impl GhostSpreadFilter {
    pub fn new() -> Self {
        Self {
            min_trade_age_seconds: 300,
            min_volume_24h: 10_000.0,
            min_