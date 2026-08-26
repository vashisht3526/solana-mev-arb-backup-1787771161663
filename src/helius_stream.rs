#![allow(dead_code)]
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Sender;
use tokio::time::sleep;
use tokio_tungstenite::{
    client_async_tls_with_config,
    tungstenite::{handshake::client::generate_key, http::Request, Message},
};
use url::Url;

pub const HELIUS_API_KEYS: [&str; 4] = [
    "5125a754-c4e8-467a-af07-fd3ccf3d1143", // Key #1: dual_live dedicated stream
    "63d3a2f2-1c89-4f09-99dc-83cec3bb5a1b", // Key #2: triangular_live dedicated stream
    "70587221-78e1-46ff-8eb3-a2ae35e6a2b1", // Key #3: micro/shadow dedicated stream
    "9b358df5-981f-4a3f-8f25-5ff9af04653f", // Key #4: Live Blockhash & Jito Settlement (RPC only)
];

// DEX Protocol Program IDs
pub const RAYDIUM_CPMM_PROGRAM: &str = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C";
pub const RAYDIUM_AMM_V4_PROGRAM: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";
pub const METEORA_DLMM_PROGRAM: &str = "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo";
pub const ORCA_WHIRLPOOL_PROGRAM: &str = "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc";

/// V42 REFLEX freshness window: an arb dispatched within this many milliseconds of a
/// LaserStream pool event counts as race-hot and may fire in marginal/negative-quote
/// territory (revert cost ≈ gas only). Stale poll-driven triggers keep strict gating.
pub const EVENT_FRESH_WINDOW_MS: i64 = 2_500;

// V42 diagnostics: per-process frame-path counters so a dead trigger feed can be
// localized instantly (frames arriving vs parsing vs pubkey extraction vs channel).
static FRAMES_TOTAL: AtomicU64 = AtomicU64::new(0);
static FRAMES_UNPARSED: AtomicU64 = AtomicU64::new(0);
static SUBSCRIBE_ERRORS: AtomicU64 = AtomicU64::new(0);
static NOTIFS_TOTAL: AtomicU64 = AtomicU64::new(0);
static NOTIFS_UNROUTED: AtomicU64 = AtomicU64::new(0);
static FIRST_RAW_NOTIFICATION: OnceLock<std::sync::Mutex<bool>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeliusAccountUpdate {
    pub pubkey: String,
    pub slot: u64,
    pub lamports: u64,
    pub owner: String,
    pub data: String,
    pub timestamp_ms: i64,
}

// ---------------------------------------------------------------------------
// V42 GLOBAL REFLEX REGISTRY
// ---------------------------------------------------------------------------
// Account-scoped subscriptions replace the old whole-program programSubscribe
// firehose. Every tracked pool address goes into TRIGGER_POOLS; each instance
// worker mirrors that set onto its own WebSocket via accountSubscribe
// (commitment: processed). This kills the trigger asymmetry (one DEX program
// per engine) and cuts stream volume ~1000x — credits now scale with OUR
// tracked universe, not global DEX activity.

fn trigger_pools() -> &'static RwLock<BTreeSet<String>> {
    static POOLS: OnceLock<RwLock<BTreeSet<String>>> = OnceLock::new();
    POOLS.get_or_init(|| RwLock::new(BTreeSet::new()))
}

fn mint_event_ms() -> &'static DashMap<String, i64> {
    static MAP: OnceLock<DashMap<String, i64>> = OnceLock::new();
    MAP.get_or_init(DashMap::new)
}

/// Register a pool address for LaserStream trigger subscriptions (call on admission).
pub fn register_trigger_pool(pool_pubkey: &str) {
    if pool_pubkey.is_empty() {
        return;
    }
    let inserted = trigger_pools()
        .write()
        .map(|mut set| set.insert(pool_pubkey.to_string()))
        .unwrap_or(false);
    if !inserted {
        return;
    }
    info!(
        "🎯 [REFLEX SUBS] Registered pool {} | total subscribed: {}",
        &pool_pubkey[..8.min(pool_pubkey.len())],
        registered_pool_count()
    );
}

/// Unregister pools dropped by lifecycle expiry so we stop paying attention to them.
pub fn unregister_trigger_pool(pool_pubkey: &str) {
    if let Ok(mut set) = trigger_pools().write() {
        set.remove(pool_pubkey);
    }
}

pub fn registered_pool_count() -> usize {
    trigger_pools().read().map(|s| s.len()).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// V42b DATA ACCOUNTS — pool VAULT token accounts subscribed for live reserve
// amounts (u64 @ offset 64 of any SPL token account). Same worker, same socket;
// distinguished from trigger pools only at the consumer. Subscribing a vault
// costs nothing extra per event beyond the notification itself and gives us
// chain-truth prices on every swap.
// ---------------------------------------------------------------------------

fn data_accounts() -> &'static RwLock<BTreeSet<String>> {
    static ACCOUNTS: OnceLock<RwLock<BTreeSet<String>>> = OnceLock::new();
    ACCOUNTS.get_or_init(|| RwLock::new(BTreeSet::new()))
}

/// Register a vault/data account for subscription (no evaluation triggers).
pub fn register_data_account(addr: &str) {
    if addr.is_empty() {
        return;
    }
    let _ = data_accounts()
        .write()
        .map(|mut set| set.insert(addr.to_string()));
}

pub fn unregister_data_account(addr: &str) {
    if let Ok(mut set) = data_accounts().write() {
        set.remove(addr);
    }
}

pub fn registered_data_count() -> usize {
    data_accounts().read().map(|s| s.len()).unwrap_or(0)
}

fn desired_accounts_union() -> Vec<String> {
    let mut all: BTreeSet<String> = BTreeSet::new();
    if let Ok(s) = trigger_pools().read() {
        all.extend(s.iter().cloned());
    }
    if let Ok(s) = data_accounts().read() {
        all.extend(s.iter().cloned());
    }
    all.into_iter().collect()
}

/// Stamp the moment a mint's pool moved on-chain (called by the hot consumer).
pub fn stamp_mint_event(mint: &str) {
    mint_event_ms().insert(mint.to_string(), chrono::Utc::now().timestamp_millis());
}

/// True when this mint saw a LaserStream pool event inside EVENT_FRESH_WINDOW_MS.
/// Engines read this to decide between reflex (fire-first) and strict dispatch.
pub fn is_event_fresh(mint: &str) -> bool {
    match mint_event_ms().get(mint) {
        Some(t) => (chrono::Utc::now().timestamp_millis() - *t) < EVENT_FRESH_WINDOW_MS,
        None => false,
    }
}

pub fn fresh_mint_count() -> usize {
    let now = chrono::Utc::now().timestamp_millis();
    mint_event_ms()
        .iter()
        .filter(|kv| now - *kv.value() < EVENT_FRESH_WINDOW_MS)
        .count()
}

pub struct HeliusStreamManager {
    key_rotation_idx: Arc<AtomicU64>,
}

impl HeliusStreamManager {
    pub fn new() -> Self {
        Self {
            key_rotation_idx: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn get_next_endpoint(&self) -> String {
        let idx = self.key_rotation_idx.fetch_add(1, Ordering::Relaxed) % HELIUS_API_KEYS.len() as u64;
        let key = HELIUS_API_KEYS[idx as usize];
        format!("wss://mainnet.helius-rpc.com/?api-key={}", key)
    }

    /// V42: account-scoped LaserStream worker. One connection per instance, ALL DEX
    /// programs covered (asymmetry fixed), commitment=processed, dynamic subscribe /
    /// unsubscribe synced against the shared TRIGGER_POOLS registry every 3 seconds.
    pub async fn run_laser_stream_worker(
        key_idx: usize,
        account_sender: Sender<HeliusAccountUpdate>,
        instance_name: String,
    ) {
        let api_key = HELIUS_API_KEYS[key_idx % HELIUS_API_KEYS.len()];
        let url_str = format!("wss://mainnet.helius-rpc.com/?api-key={}", api_key);
        let parsed_url = Url::parse(&url_str).expect("Valid Helius WS URL");

        let mut active_subs: HashMap<String, u64> = HashMap::new(); // pool pubkey -> sub id
        let mut pending_ids: HashMap<u64, String> = HashMap::new(); // rpc id -> pool pubkey
        let mut next_rpc_id: u64 = 100;
        let mut msgs_since_report: u64 = 0;
        let mut last_credit_report = chrono::Utc::now().timestamp_millis();
        // Reverse of active_subs: notification routing key. accountSubscribe payloads
        // carry ONLY params.subscription — the account struct comes bare in
        // result.value with NO pubkey field (that wrapper shape is programSubscribe's).
        let mut sub_to_pool: HashMap<u64, String> = HashMap::new();

        loop {
            info!(
                "⚡ [HELIUS-REFLEX #{}] Connecting '{}' to Frankfurt peer (account-scoped, processed)...",
                key_idx + 1,
                instance_name
            );

            let host = parsed_url.host_str().unwrap_or("mainnet.helius-rpc.com");
            let port = parsed_url.port_or_known_default().unwrap_or(443);
            let addr = format!("{}:{}", host, port);

            match TcpStream::connect(&addr).await {
                Ok(tcp_stream) => {
                    let _ = tcp_stream.set_nodelay(true);

                    let request = Request::builder()
                        .uri(url_str.as_str())
                        .header("Host", host)
                        .header("Connection", "Upgrade")
                        .header("Upgrade", "websocket")
                        .header("Sec-WebSocket-Version", "13")
                        .header("Sec-WebSocket-Key", generate_key())
                        .body(())
                        .unwrap();

                    match client_async_tls_with_config(request, tcp_stream, None, None).await {
                        Ok((ws_stream, _)) => {
                            info!("✅ [HELIUS-REFLEX #{}] Connected!", key_idx + 1);
                            let (mut ws_sink, mut ws_source) = ws_stream.split();

                            // Fresh connect: drop stale sub ids; the 3s sync loop resubscribes.
                            active_subs.clear();
                            sub_to_pool.clear();
                            pending_ids.clear();

                            let mut ping_interval =
                                tokio::time::interval(Duration::from_secs(5));
                            let mut sync_interval = tokio::time::interval(Duration::from_secs(3));
                            sync_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

                            loop {
                                tokio::select! {
                                    _ = ping_interval.tick() => {
                                        if let Err(e) = ws_sink.send(Message::Ping(vec![0xAA])).await {
                                            warn!("⚠️ Ping failed on Reflex worker #{}: {}", key_idx + 1, e);
                                            break;
                                        }
                                    }

                                    _ = sync_interval.tick() => {
                                        // Diff desired subscription set vs active, then reconcile.
                                        // V42b: union of trigger pools + vault data accounts.
                                        let desired: Vec<String> = desired_accounts_union();

                                        // 1. Subscribe to new pools
                                        for pool in &desired {
                                            if active_subs.contains_key(pool) || pending_ids.values().any(|v| v == pool) {
                                                continue;
                                            }
                                            let rid = next_rpc_id;
                                            next_rpc_id += 1;
                                            let req = serde_json::json!({
                                                "jsonrpc": "2.0",
                                                "id": rid,
                                                "method": "accountSubscribe",
                                                "params": [
                                                    pool,
                                                    { "encoding": "base64", "commitment": "processed" }
                                                ]
                                            });
                                            if ws_sink.send(Message::Text(req.to_string())).await.is_err() {
                                                break;
                                            }
                                            pending_ids.insert(rid, pool.clone());
                                        }

                                        // 2. Unsubscribe pools that left the registry
                                        let desired_set: std::collections::HashSet<&String> = desired.iter().collect();
                                        let stale: Vec<(String, u64)> = active_subs
                                            .iter()
                                            .filter(|(k, _)| !desired_set.contains(*k))
                                            .map(|(k, v)| (k.clone(), *v))
                                            .collect();
                                        for (pool, sub_id) in stale {
                                            let rid = next_rpc_id;
                                            next_rpc_id += 1;
                                            let req = serde_json::json!({
                                                "jsonrpc": "2.0",
                                                "id": rid,
                                                "method": "accountUnsubscribe",
                                                "params": [sub_id]
                                            });
                                            let _ = ws_sink.send(Message::Text(req.to_string())).await;
                                            active_subs.remove(&pool);
                                            sub_to_pool.remove(&sub_id);
                                        }

                                        // 3. Credit-burn telemetry (per-minute msg rate proxy)
                                        let now_ms = chrono::Utc::now().timestamp_millis();
                                        if now_ms - last_credit_report >= 60_000 {
                                            let rate = msgs_since_report as f64 / ((now_ms - last_credit_report) as f64 / 1000.0);
                                            info!("📊 [REFLEX CREDITS] '{}': {:.1} pool-events/s | subs {} | frames:{} notifs:{} unparsed:{} unrouted:{} sub-rejects:{}",
                                                instance_name, rate, active_subs.len(),
                                                FRAMES_TOTAL.load(Ordering::Relaxed),
                                                NOTIFS_TOTAL.load(Ordering::Relaxed),
                                                FRAMES_UNPARSED.load(Ordering::Relaxed),
                                                NOTIFS_UNROUTED.load(Ordering::Relaxed),
                                                SUBSCRIBE_ERRORS.load(Ordering::Relaxed));
                                            msgs_since_report = 0;
                                            last_credit_report = now_ms;
                                        }
                                    }

                                    msg_opt = ws_source.next() => {
                                        match msg_opt {
                                            Some(Ok(Message::Binary(bytes))) => {
                                                Self::process_raw_bytes(&bytes, &account_sender, &mut active_subs, &mut sub_to_pool, &mut pending_ids, &mut msgs_since_report);
                                            }
                                            Some(Ok(Message::Text(text))) => {
                                                Self::process_raw_bytes(text.as_bytes(), &account_sender, &mut active_subs, &mut sub_to_pool, &mut pending_ids, &mut msgs_since_report);
                                            }
                                            Some(Ok(Message::Pong(_))) => {}
                                            Some(Ok(Message::Ping(p))) => {
                                                let _ = ws_sink.send(Message::Pong(p)).await;
                                            }
                                            Some(Err(e)) => {
                                                warn!("⚠️ [HELIUS-REFLEX #{}] Connection dropped: {}. Reconnecting...", key_idx + 1, e);
                                                break;
                                            }
                                            None => {
                                                warn!("⚠️ [HELIUS-REFLEX #{}] Stream closed. Reconnecting...", key_idx + 1);
                                                break;
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("⚠️ Handshake failed for Helius-Reflex #{}: {}. Retrying in 2s...", key_idx + 1, e);
                        }
                    }
                }
                Err(e) => {
                    error!("⚠️ TCP connect failed for Helius-Reflex #{}: {}. Retrying in 2s...", key_idx + 1, e);
                }
            }
            sleep(Duration::from_secs(2)).await;
        }
    }

    #[inline(always)]
    fn process_raw_bytes(
        bytes: &[u8],
        sender: &Sender<HeliusAccountUpdate>,
        active_subs: &mut HashMap<String, u64>,
        sub_to_pool: &mut HashMap<u64, String>,
        pending_ids: &mut HashMap<u64, String>,
        msgs_since_report: &mut u64,
    ) {
        FRAMES_TOTAL.fetch_add(1, Ordering::Relaxed);

        let Ok(val) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            FRAMES_UNPARSED.fetch_add(1, Ordering::Relaxed);
            return;
        };

        // Subscription confirmations: {"jsonrpc":"2.0","result":<sub_id>,"id":<rpc_id>}
        if !val.get("params").is_some() {
            if let (Some(rid), Some(result)) = (
                val.get("id").and_then(|i| i.as_u64()),
                val.get("result"),
            ) {
                if let Some(pool) = pending_ids.remove(&rid) {
                    if let Some(sub_id) = result.as_u64() {
                        sub_to_pool.insert(sub_id, pool.clone());
                        active_subs.insert(pool, sub_id);
                    } else {
                        // Helius rejected this subscribe (plan limit / bad param / rate cap)
                        SUBSCRIBE_ERRORS.fetch_add(1, Ordering::Relaxed);
                        warn!(
                            "🚫 [HELIUS-REFLEX] Subscribe REJECTED for {}…: {}",
                            &pool[..12.min(pool.len())],
                            serde_json::to_string(&val).unwrap_or_default()
                        );
                    }
                }
                // accountUnsubscribe acks carry our rpc ids too — harmless fallthrough
            }
            return;
        }

        // Notification: {"method":"accountNotification","params":{"subscription":N,
        //               "result":{"context":{"slot"},"value":{"pubkey","account"}}}}
        NOTIFS_TOTAL.fetch_add(1, Ordering::Relaxed);
        if FIRST_RAW_NOTIFICATION
            .get_or_init(|| std::sync::Mutex::new(false))
            .lock()
            .map(|mut seen| {
                if *seen {
                    false
                } else {
                    *seen = true;
                    true
                }
            })
            .unwrap_or(false)
        {
            info!(
                "🔬 [HELIUS-REFLEX] First raw notification: {}",
                &String::from_utf8_lossy(bytes)[..400.min(bytes.len())]
            );
        }
        if let Some(params) = val.get("params") {
            // Route by params.subscription → pool. The value object IS the bare
            // account struct (lamports/data/owner) — there is no pubkey inside.
            let sub_id = params.get("subscription").and_then(|s| s.as_u64()).unwrap_or(0);
            let pool = match sub_to_pool.get(&sub_id) {
                Some(p) => p.clone(),
                None => {
                    NOTIFS_UNROUTED.fetch_add(1, Ordering::Relaxed);
                    return;
                }
            };
            if let Some(result) = params.get("result") {
                let slot = result
                    .get("context")
                    .and_then(|c| c.get("slot"))
                    .and_then(|s| s.as_u64())
                    .unwrap_or(0);
                let acct = result.get("value");
                let lamports = acct
                    .and_then(|a| a.get("lamports"))
                    .and_then(|l| l.as_u64())
                    .unwrap_or(0);
                let owner = acct
                    .and_then(|a| a.get("owner"))
                    .and_then(|o| o.as_str())
                    .unwrap_or("")
                    .to_string();
                let data = acct
                    .and_then(|a| a.get("data"))
                    .and_then(|d| d.get(0))
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();

                *msgs_since_report += 1;
                let update = HeliusAccountUpdate {
                    pubkey: pool,
                    slot,
                    lamports,
                    owner,
                    data,
                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                };
                let _ = sender.try_send(update);
            }
        }
    }

    /// Launches the instance's dedicated reflex worker (one key per instance,
    /// ALL tracked pools regardless of DEX — trigger asymmetry eliminated).
    pub fn launch_dedicated_instance_worker(account_sender: Sender<HeliusAccountUpdate>, instance_name: &str) {
        let key_idx = match instance_name {
            "dual_live" => 0,
            "triangular_live" => 1,
            "micro" => 2,
            _ => 3,
        };

        let sender_clone = account_sender;
        let name_owned = instance_name.to_string();
        tokio::spawn(async move {
            Self::run_laser_stream_worker(key_idx, sender_clone, name_owned).await;
        });
        info!(
            "🌟 [HELIUS REFLEX] Launched account-scoped processed-commitment worker for '{}' on Key #{}!",
            instance_name,
            key_idx + 1
        );
    }
}
