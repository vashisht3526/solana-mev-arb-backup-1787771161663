$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

Write-Host "==============================================================================="
Write-Host "🚀 SYNCING 100% PURE RUST REFACTORED CODEBASE TO FRANKFURT EC2"
Write-Host "==============================================================================="

# 1. Read local files
$cargoToml = Get-Content -Raw "Cargo.toml"
$mathRs = Get-Content -Raw "src\math.rs"
$ammRs = Get-Content -Raw "src\amm.rs"
$jitoBuilderRs = Get-Content -Raw "src\jito_builder.rs"
$wsFeedRs = Get-Content -Raw "src\websocket_feed.rs"
$scannerRs = Get-Content -Raw "src\scanner.rs"
$liveEngineRs = Get-Content -Raw "src\live_engine.rs"
$typesRs = Get-Content -Raw "src\types.rs"
$mainRs = Get-Content -Raw "src\main.rs"
$reclaimRentRs = Get-Content -Raw "src\bin\reclaim_rent.rs"
$sellAllRs = Get-Content -Raw "src\bin\sell_all.rs"
$dashboardServerJs = Get-Content -Raw "dashboard\server.js"

# 2. Construct Remote Sync Script
$syncScript = @"
mkdir -p /home/ubuntu/solana-mev-arb/src/bin
mkdir -p /home/ubuntu/solana-mev-arb/data
mkdir -p /home/ubuntu/solana-mev-arb/dashboard

# Clean legacy node executor from remote
rm -f /home/ubuntu/solana-mev-arb/dashboard/live_executor.js
rm -f /home/ubuntu/solana-mev-arb/dashboard/test_live_swap.js

cat << 'EOF_CARGO' > /home/ubuntu/solana-mev-arb/Cargo.toml
$cargoToml
EOF_CARGO

cat << 'EOF_MATH' > /home/ubuntu/solana-mev-arb/src/math.rs
$mathRs
EOF_MATH

cat << 'EOF_AMM' > /home/ubuntu/solana-mev-arb/src/amm.rs
$ammRs
EOF_AMM

cat << 'EOF_JITO' > /home/ubuntu/solana-mev-arb/src/jito_builder.rs
$jitoBuilderRs
EOF_JITO

cat << 'EOF_WS' > /home/ubuntu/solana-mev-arb/src/websocket_feed.rs
$wsFeedRs
EOF_WS

cat << 'EOF_SCANNER' > /home/ubuntu/solana-mev-arb/src/scanner.rs
$scannerRs
EOF_SCANNER

cat << 'EOF_LIVE' > /home/ubuntu/solana-mev-arb/src/live_engine.rs
$liveEngineRs
EOF_LIVE

cat << 'EOF_TYPES' > /home/ubuntu/solana-mev-arb/src/types.rs
$typesRs
EOF_TYPES

cat << 'EOF_MAIN' > /home/ubuntu/solana-mev-arb/src/main.rs
$mainRs
EOF_MAIN

cat << 'EOF_RECLAIM' > /home/ubuntu/solana-mev-arb/src/bin/reclaim_rent.rs
$reclaimRentRs
EOF_RECLAIM

cat << 'EOF_SELL' > /home/ubuntu/solana-mev-arb/src/bin/sell_all.rs
$sellAllRs
EOF_SELL

cat << 'EOF_DASH' > /home/ubuntu/solana-mev-arb/dashboard/server.js
$dashboardServerJs
EOF_DASH

echo "📦 Codebase synchronized! Compiling release binary on Frankfurt Ubuntu..."
cd /home/ubuntu/solana-mev-arb
~/.cargo/bin/cargo build --release

echo "🧪 Running unit tests..."
~/.cargo/bin/cargo test --release

echo "🚀 Restarting PM2 Live Services..."
pm2 restart rust-mev-arb-live || pm2 start ./target/release/solana-mev-arb --name rust-mev-arb-live -- --live
pm2 restart solana-mev-dashboard || pm2 start dashboard/server.js --name solana-mev-dashboard
pm2 save

echo "📊 PM2 Process Status:"
pm2 status
"@

$syncScript | ssh -i $SSH_KEY -o StrictHostKeyChecking=no -o ConnectTimeout=30 $REMOTE_HOST "bash -s"
