 I'll cross-examine GLM-5's proposal against critical edge cases and provide the hardened implementation. GLM-5 has significant gaps that will cause production failures.

---

## 🔴 CRITICAL DEFECTS IN GLM-5 PROPOSAL

| Defect | Severity | Impact |
|--------|----------|--------|
| **Blockhash expiry race** | 🔴 CRITICAL | Transactions fail silently after 90-120 slots |
| **Static Jito tip account** | 🟡 HIGH | Account rotation failure, bundle drops |
| **Missing `getBundleStatuses` polling** | 🔴 CRITICAL | No confirmation tracking, false success signals |
| **No slippage enforcement in ATA close** | 🟡 HIGH | Failed closes lock rent |
| **Wrong `close_account_ix` construction** | 🔴 CRITICAL | Instruction data malformed, tx fails |
| **No compute budget instructions** | 🟡 HIGH | CU limit exceeded, bundle rejection |
| **Missing signature verification** | 🟡 HIGH | Invalid txs sent to Jito |

---

## CORRECTED IMPLEMENTATION

### 1. `src/jito_builder.rs` — Hardened with Real Cryptography

```rust
//! Jito Bundle Construction with Real Solana Cryptography
//! Addresses: Blockhash expiry, dynamic tip rotation, proper ATA lifecycle, CU management

use anyhow::{Context, Result, bail};
use log::{info, warn, debug};
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    hash::Hash,
    instruction::{AccountMeta, Instruction},
    message::{VersionedMessage, v0::Message as MessageV0},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::VersionedTransaction,
};
use spl_associated_token_account::{
    get_associated_token_address,
    instruction::create_associated_token_account_idempotent,
};
use spl_token::instruction::close_account;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};

// Jito tip accounts — rotated for resilience against account-specific issues
const JITO_TIP_ACCOUNTS: [&str; 8] = [
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
    "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
];

/// Compute unit limits for different operation profiles
pub enum ComputeProfile {
    StandardSwap,      // ~65k CU
    ComplexArb,        // ~140k CU
    MaxSafe,           // ~1.4M CU (approaching block limit)
}

impl ComputeProfile {
    pub fn cu_limit(&self) -> u32 {
        match self {
            Self::StandardSwap => 65_000,
            Self::ComplexArb => 140_000,
            Self::MaxSafe => 1_400_000,
        }
    }
    
    /// Priority fee in micro-lamports per CU
    pub fn priority_fee(&self) -> u64 {
        match self {
            Self::StandardSwap => 10_000,    // 0.00001 SOL per CU
            Self::ComplexArb => 25_000,
            Self::MaxSafe => 50_000,
        }
    }
}

pub struct JitoBuilder {
    pub payer: Keypair,
    pub payer_pubkey: Pubkey,
    tip_rotation_index: AtomicUsize,
}

/// Bundle construction result with metadata for tracking
pub struct BundlePackage {
    pub versioned_tx: VersionedTransaction,
    pub base64_encoded: String,
    pub ephemeral_ata: Pubkey,
    pub tip_account: Pubkey,
    pub tip_lamports: u64,
    pub blockhash: Hash,
    pub blockhash_slot: u64,  // For expiry tracking
}

impl JitoBuilder {
    /// Load keypair with strict permission validation
    pub fn load_from_path(path: &str) -> Result<Self> {
        let expanded = shellexpand::tilde(path);
        let path = Path::new(expanded.as_ref());
        
        // Security: Verify file permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(path)
                .context("Keypair file not accessible")?;
            let mode = metadata.permissions().mode();
            let perm = mode & 0o777;
            if perm != 0o400 {
                bail!(
                    "INSECURE_KEYPAIR_PERMISSIONS: Expected 0400, got {:04o}. \
                     Run: chmod 400 {}", perm, path.display()
                );
            }
        }

        let json_data = fs::read_to_string(path)
            .context("Failed to read keypair file")?;
        
        // Handle both JSON array and base58-encoded secret key formats
        let keypair = if json_data.trim().starts_with('[') {
            // Standard Solana CLI format: [12, 34, 56, ...]
            let bytes: Vec<u8> = serde_json::from_str(&json_data)
                .context("Invalid JSON byte array in keypair file")?;
            Keypair::from_bytes(&bytes)
                .context("Invalid keypair bytes")?
        } else {
            // Alternative: base58-encoded secret key
            let bytes = bs58::decode(json_data.trim())
                .into_vec()
                .context("Invalid base58 encoding")?;
            Keypair::from_bytes(&bytes)
                .context("Invalid keypair from base58")?
        };

        let pubkey = keypair.pubkey();
        info!("🔐 [CRYPTO] Loaded signing keypair: {}", pubkey);

        Ok(Self {
            payer: keypair,
            payer_pubkey: pubkey,
            tip_rotation_index: AtomicUsize::new(0),
        })
    }

    /// Rotate through Jito tip accounts for resilience
    fn select_tip_account(&self) -> Pubkey {
        let idx = self.tip_rotation_index.fetch_add(1, Ordering::Relaxed) % JITO_TIP_ACCOUNTS.len();
        Pubkey::from_str(JITO_TIP_ACCOUNTS[idx])
            .expect("Static tip account pubkey invalid")
    }

    /// Derive ATA with validation
    pub fn derive_ata(&self, mint: &Pubkey) -> Pubkey {
        get_associated_token_address(&self.payer_pubkey, mint)
    }

    /// Build atomic bundle with proper instruction ordering and CU management
    /// 
    /// Instruction order (critical for execution):
    /// 1. ComputeBudget::set_compute_unit_limit
    /// 2. ComputeBudget::set_compute_unit_price  
    /// 3. SystemProgram::create ATA (idempotent)
    /// 4. DEX swap instruction(s)
    /// 5. TokenProgram::close ATA (reclaim rent)
    /// 6. SystemProgram::transfer (Jito tip) — MUST BE LAST for bundle scoring
    pub fn build_atomic_bundle(
        &self,
        mint: &Pubkey,
        recent_blockhash: Hash,
        blockhash_slot: u64,
        swap_instructions: Vec<Instruction>,
        jito_tip_lamports: u64,
        compute_profile: ComputeProfile,
    ) -> Result<BundlePackage> {
        
        let ephemeral_ata = self.derive_ata(mint);
        let tip_account = self.select_tip_account();

        // Validate: ephemeral ATA must not be the mint itself (common config error)
        if ephemeral_ata == *mint {
            bail!("CONFIG_ERROR: ATA derivation produced mint address — check owner/mint ordering");
        }

        // === CONSTRUCT INSTRUCTIONS ===
        let mut instructions: Vec<Instruction> = vec![];

        // 1. Compute budget (MUST be first)
        instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(
            compute_profile.cu_limit()
        ));
        instructions.push(ComputeBudgetInstruction::set_compute_unit_price(
            compute_profile.priority_fee()
        ));

        // 2. Create ATA (idempotent — safe to call if exists)
        instructions.push(create_associated_token_account_idempotent(
            &self.payer_pubkey,  // funding
            &self.payer_pubkey,  // owner
            mint,
            &spl_token::id(),
        ));

        // 3. Swap instructions from router
        instructions.extend(swap_instructions);

        // 4. Close ATA — uses proper spl_token instruction, NOT manual construction
        // Requires: ATA is owned by payer, has zero balance or explicit authority
        let close_ix = close_account(
            &spl_token::id(),
            &ephemeral_ata,           // account to close
            &self.payer_pubkey,       // destination for rent lamports
            &self.payer_pubkey,       // authority (must sign)
            &[],                      // additional signers (none for single-sig)
        ).context("Failed to construct close_account instruction")?;
        
        instructions.push(close_ix);

        // 5. Jito tip — LAST position for bundle scoring
        instructions.push(system_instruction::transfer(
            &self.payer_pubkey,
            &tip_account,
            jito_tip_lamports,
        ));

        // === BUILD VERSIONED TRANSACTION ===
        // Use MessageV0 for address table lookup support (future-proofing)
        let message = MessageV0::try_compile(
            &self.payer_pubkey,
            &instructions,
            &[], // address table lookups (empty for now)
            recent_blockhash,
        ).context("Failed to compile MessageV0")?;

        let versioned_message = VersionedMessage::V0(message);
        
        // Sign with payer only (single-sig for atomic bundle)
        let signatures = vec![self.payer.sign_message(&versioned_message.serialize())];
        
        let versioned_tx = VersionedTransaction {
            signatures,
            message: versioned_message,
        };

        // Verify signature before encoding
        versioned_tx.verify()
            .context("Transaction signature verification failed")?;

        // Serialize to base64 for Jito API
        let serialized = bincode::serialize(&versioned_tx)
            .context("Failed to serialize VersionedTransaction")?;
        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &serialized
        );

        info!(
            "✅ [BUNDLE] Built: ATA={}, Tip={}lamports, CU={}, BlockhashSlot={}",
            ephemeral_ata, jito_tip_lamports, compute_profile.cu_limit(), blockhash_slot
        );

        Ok(BundlePackage {
            versioned_tx,
            base64_encoded: encoded,
            ephemeral_ata,
            tip_account,
            tip_lamports: jito_tip_lamports,
            blockhash,
            blockhash_slot,
        })
    }

    /// Re-sign with new blockhash if original expires (bundle retry)
    pub fn resign_with_blockhash(
        &self,
        package: &BundlePackage,
        new_blockhash: Hash,
        new_slot: u64,
    ) -> Result<BundlePackage> {
        // Rebuild message with new blockhash
        let mut instructions = vec![];
        
        // Re-extract instructions from original (simplified — in production, preserve original)
        // This is a stub for the pattern; full implementation requires instruction preservation
        
        // For now, return error indicating need for full rebuild
        bail!("BLOCKHASH_EXPIRED: Bundle {} slots old, requires full reconstruction", 
              new_slot.saturating_sub(package.blockhash_slot))
    }
}
```

---

### 2. `src/jito_submitter.rs` — Async Bundle Submission with `getBundleStatuses` Polling

```rust
//! Jito Bundle Submission and Confirmation Tracking
//! Implements: submit → poll → confirm/retry with exponential backoff

use anyhow::{Context, Result, bail};
use log::{info, warn, error, debug};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, sleep, timeout};

// Jito API endpoints
const JITO_BUNDLE_SUBMIT: &str = "https://frankfurt.mainnet.block-engine.jito.wtf/api/v1/bundles";
const JITO_BUNDLE_STATUS: &str = "https://frankfurt.mainnet.block-engine.jito.wtf/api/v1/bundles/status";

/// Bundle lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleState {
    PendingSubmit,      // Awaiting initial submission
    Submitted,          // HTTP 200 from submit, awaiting status
    PendingConfirmation, // In getBundleStatuses polling loop
    Confirmed,          // Landed on-chain
    Failed,             // Explicit failure (simulation, drop, etc)
    Expired,            // Blockhash expired before confirmation
    Unknown,            // Status query returned no info
}

/// Tracked bundle with metadata
#[derive(Debug, Clone)]
pub struct TrackedBundle {
    pub uuid: String,                    // Our internal tracking ID
    pub jito_uuid: Option<String>,       // Jito's returned UUID (if any)
    pub state: BundleState,
    pub submitted_at: Instant,
    pub last_status_check: Option<Instant>,
    pub confirmation_slot: Option<u64>,
    pub transactions: Vec<String>,       // Base58 signatures
    pub error: Option<String>,
    pub retry_count: u32,
}

/// Jito API response types
#[derive(Debug, Deserialize)]
struct BundleSubmitResponse {
    #[serde(default)]
    result: Option<String>,  // Jito's bundle UUID
    #[serde(default)]
    error: Option<BundleError>,
}

#[derive(Debug, Deserialize)]
struct BundleError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct BundleStatusResponse {
    #[serde(default)]
    result: Option<BundleStatusResult>,
    #[serde(default)]
    error: Option<BundleError>,
}

#[derive(Debug, Deserialize)]
struct BundleStatusResult {
    bundles: Vec<BundleStatus>,
    #[serde(default)]
    context: Option<StatusContext>,
}

#[derive(Debug, Deserialize)]
struct BundleStatus {
    bundle_id: String,
    transactions: Vec<String>,
    slot: u64,
    #[serde(default)]
    confirmation_status: Option<String>,
    #[serde(rename = "err")]
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct StatusContext {
    slot: u64,
}

/// Jito submission engine with background polling worker
pub struct JitoSubmitter {
    http_client: reqwest::Client,
    tracked_bundles: Arc<RwLock<HashMap<String, TrackedBundle>>>,
    shutdown_tx: mpsc::Sender<()>,
    _polling_worker: tokio::task::JoinHandle<()>,
}

impl JitoSubmitter {
    pub async fn new() -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(10)
            .build()
            .context("Failed to build HTTP client")?;

        let tracked = Arc::new(RwLock::new(HashMap::new()));
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        
        // Spawn background polling worker
        let tracked_clone = tracked.clone();
        let client_clone = http_client.clone();
        
        let polling_worker = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_millis(500)); // 500ms poll interval
            
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        Self::poll_pending_bundles(&tracked_clone, &client_clone).await;
                    }
                    _ = shutdown_rx.recv() => {
                        info!("🛑 [JITO_POLL] Shutdown signal received, exiting worker");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            http_client,
            tracked_bundles: tracked,
            shutdown_tx,
            _polling_worker: polling_worker,
        })
    }

    /// Submit bundle to Jito with immediate tracking
    pub async fn submit_bundle(
        &self,
        uuid: String,
        base64_txs: Vec<String>,
    ) -> Result<TrackedBundle> {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendBundle",
            "params": [base64_txs]
        });

        let start = Instant::now();
        let response = self.http_client
            .post(JITO_BUNDLE_SUBMIT)
            .json(&payload)
            .send()
            .await
            .context("HTTP request to Jito submit failed")?;

        let status = response.status();
        let body: BundleSubmitResponse = response
            .json()
            .await
            .context("Failed to parse Jito submit response")?;

        if let Some(err) = body.error {
            bail!("JITO_SUBMIT_ERROR [{}]: {}", err.code, err.message);
        }

        let jito_uuid = body.result;
        let elapsed_ms = start.elapsed().as_millis();

        info!(
            "📤 [JITO_SUBMIT] UUID={} JitoUUID={:?} Status={} Latency={}ms",
            uuid, jito_uuid, status, elapsed_ms
        );

        let tracked = TrackedBundle {
            uuid: uuid.clone(),
            jito_uuid: jito_uuid.clone(),
            state: BundleState::Submitted,
            submitted_at: Instant::now(),
            last_status_check: None,
            confirmation_slot: None,
            transactions: vec![], // populated from status response
            error: None,
            retry_count: 0,
        };

        self.tracked_bundles.write().await.insert(uuid, tracked.clone());
        Ok(tracked)
    }

    /// Background polling: query getBundleStatuses for all pending bundles
    async fn poll_pending_bundles(
        tracked: &Arc<RwLock<HashMap<String, TrackedBundle>>>,
        client: &reqwest::Client,
    ) {
        // Collect bundle IDs needing status check
        let to_check: Vec<(String,