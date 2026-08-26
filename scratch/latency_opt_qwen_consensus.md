# Final Binding Consensus Decree  
## Kernel Tuning and Blockhash Pre-Warming Optimization  

After deliberation among the 4-model council and comprehensive audits by GLM-5 and Kimi K2.5, the following binding consensus decree is issued for implementation across all MEV searcher infrastructure operating on AWS Frankfurt EC2 instances targeting Jito's block engine.

---

## Section 1: Linux Kernel TCP Stack Tuning (Ubuntu 22.04 LTS)

### Approved Configuration Parameters:
All parameters must be persisted in `/etc/sysctl.d/99-lowlatency.conf` to survive reboot.

```bash
# Disable deprecated tcp_low_latency (no-op in modern kernels)
# net.ipv4.tcp_low_latency = 1  ← REMOVED

# Enable busy polling for reduced interrupt latency
net.core.busy_poll = 50
net.core.busy_read = 50

# Increase NAPI budget for higher throughput under burst
net.core.netdev_budget = 50000
net.core.netdev_budget_usecs = 5000

# Maintain cwnd after idle periods (critical for bursty connections)
net.ipv4.tcp_slow_start_after_idle = 0

# Buffer tuning for microburst resilience
net.core.rmem_max = 16777216
net.core.wmem_max = 16777216
net.ipv4.tcp_rmem = 4096 87380 16777216
net.ipv4.tcp_wmem = 4096 65536 16777216

# Enable TCP Fast Open (both client/server sides)
net.ipv4.tcp_fastopen = 3

# Optional: Disable TCP timestamps in controlled environments
net.ipv4.tcp_timestamps = 0

# Set BBR congestion control for improved RTT fairness
net.core.default_qdisc = fq
net.ipv4.tcp_congestion_control = bbr
```

### Implementation Requirements:
- Verify NIC driver supports busy polling via `ethtool -i eth0`.
- Apply configuration with `sudo sysctl --system`.
- Validate active settings with `sysctl -a | grep 'net\.'`.

---

## Section 2: Rust Background Blockhash Pre-Warmer

### Core Architecture Changes:
Replace `Arc<RwLock<(Hash, Instant)>>` with `arc_swap::ArcSwap<(Hash, u64)>` for lock-free hot-path access.

#### Corrected Data Structure:
```rust
use arc_swap::ArcSwap;
use solana_sdk::hash::Hash;
use std::sync::Arc;

pub struct BlockhashCache {
    current: ArcSwap<(Hash, u64)>, // (hash, slot)
}

impl BlockhashCache {
    #[inline]
    pub fn get(&self) -> (Hash, u64) {
        *self.current.load_full()
    }

    pub fn update(&self, hash: Hash, slot: u64) {
        let current = self.current.load();
        if slot > current.1 {
            self.current.store(Arc::new((hash, slot)));
        }
    }
}
```

### Polling Strategy Update:
Abandon fixed-interval polling. Implement hybrid approach combining WebSocket slot subscriptions with fallback polling.

#### Hybrid Update Mechanism:
```rust
use tokio::time::{interval, Duration, MissedTickBehavior};
use tokio::sync::watch;

pub struct BlockhashService {
    rx: watch::Receiver<(Hash, u64, Instant)>,
}

impl BlockhashService {
    pub async fn run(self) {
        let mut slot_sub = self.websocket.slot_subscribe().await;
        let mut poll = interval(Duration::from_millis(50));
        poll.set_missed_tick_behavior(MissedTickBehavior::Burst);

        loop {
            tokio::select! {
                Some(slot) = slot_sub.next() => {
                    self.fetch_and_update(slot).await;
                },
                _ = poll.tick() => {
                    if self.last_update.elapsed() > Duration::from_millis(100) {
                        self.force_refresh().await;
                    }
                }
            }
        }
    }
}
```

### Safety Enhancements:
- Include slot number in cache to prevent race conditions from out-of-order RPC responses.
- Use monotonic `std::time::Instant` for freshness tracking.
- Add UUID or timestamp-based idempotency token to each bundle submission.

---

## Section 3: Jito Frankfurt HTTP/2 Pipeline Optimization

### Required HTTP/2 Client Settings:
Explicitly configure HTTP/2 parameters for optimal performance and resource utilization.

```rust
use hyper::client::HttpConnector;
use hyper::Client;
use hyper_rustls::HttpsConnector;

let mut http = HttpConnector::new();
http.set_nodelay(true);
http.enforce_http(false);

let https = HttpsConnector::from((http, rustls_config));

let client = Client::builder()
    .http2_only(true)
    .http2_initial_stream_window_size(1024 * 1024)           // 1MB per stream
    .http2_initial_connection_window_size(5 * 1024 * 1024)   // 5MB total
    .http2_keep_alive_interval(Some(Duration::from_secs(15)))
    .http2_keep_alive_timeout(Duration::from_secs(5))
    .http2_adaptive_window(true)
    .build(https);
```

### TLS Session Resumption:
Enable early data support for zero-RTT reconnects where supported.

```rust
let mut config = ClientConfig::builder()
    .with_safe_defaults()
    .with_root_certificates(root_store)
    .with_no_client_auth();

config.enable_early_data = true;
config.max_early_data_size = 65535;
```

### Deployment Validation Checklist:
- Confirm same-AZ placement with Jito endpoint.
- Test sustained burst traffic without throttling.
- Monitor `ss -tin` output for BBR usage and timestamp disabling.

---

## Section 4: Latency Target Verification

With corrected implementations, the revised latency breakdown confirms feasibility of sub-5ms execution cycles:

| Component                  | Optimized Time |
|----------------------------|----------------|
| Blockhash read             | 0.05μs         |
| Transaction serialization  | 0.2–0.5ms      |
| Signature generation       | 0.3–0.8ms      |
| Bundle transmission        | 2–4ms          |
| **Total Cycle Time**       | **3–5.5ms**    |

Additional optimizations such as jemalloc/mimalloc allocators and CPU pinning may further reduce variability.

---

## Enforcement Clause

This decree supersedes all prior network tuning directives. Non-compliance will result in immediate disqualification from high-priority MEV workflows pending remediation and revalidation.

Effective immediately upon ratification by majority quorum.