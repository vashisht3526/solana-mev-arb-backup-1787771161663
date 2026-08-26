$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

Write-Host "🚀 [OPTION A] Packaging and Syncing all latest local files to Frankfurt EC2..."

# Read file contents
$cargoToml = Get-Content -Raw "Cargo.toml"
$mainRs = Get-Content -Raw "src\main.rs"
$scannerRs = Get-Content -Raw "src\scanner.rs"
$dualLegEngine = Get-Content -Raw "src\dual_leg_engine.rs"
$triangularLiveEngine = Get-Content -Raw "src\triangular_live_engine.rs"
$liveEngine = Get-Content -Raw "src\live_engine.rs"
$jitoBuilder = Get-Content -Raw "src\jito_builder.rs"
$jitoRs = Get-Content -Raw "src\jito.rs"
$routerRs = Get-Content -Raw "src\router.rs"
$ammRs = Get-Content -Raw "src\amm.rs"
$mathRs = Get-Content -Raw "src\math.rs"
$paperEngine = Get-Content -Raw "src\paper_engine.rs"
$typesRs = Get-Content -Raw "src\types.rs"
$wsFeed = Get-Content -Raw "src\websocket_feed.rs"
$raydiumCpmm = Get-Content -Raw "src\raydium_cpmm.rs"
$migrationSniper = Get-Content -Raw "src\migration_sniper.rs"
$triMath = Get-Content -Raw "src\triangular_math.rs"
$heliusStream = Get-Content -Raw "src\helius_stream.rs"

$serverJs = Get-Content -Raw "dashboard\server.js"
$indexHtml = Get-Content -Raw "dashboard\public\index.html"
$ecoConfig = Get-Content -Raw "ecosystem.config.js"
$dualConfig = Get-Content -Raw "data\dual_leg_config.json"
$triConfig = Get-Content -Raw "data\triangular_config.json"

$remoteScript = @"
mkdir -p /home/ubuntu/solana-mev-arb/src
mkdir -p /home/ubuntu/solana-mev-arb/dashboard/public
mkdir -p /home/ubuntu/solana-mev-arb/data

cat << 'EOF_CARGO' > /home/ubuntu/solana-mev-arb/Cargo.toml
$cargoToml
EOF_CARGO

cat << 'EOF_MAIN' > /home/ubuntu/solana-mev-arb/src/main.rs
$mainRs
EOF_MAIN

cat << 'EOF_SCAN' > /home/ubuntu/solana-mev-arb/src/scanner.rs
$scannerRs
EOF_SCAN

cat << 'EOF_DUAL' > /home/ubuntu/solana-mev-arb/src/dual_leg_engine.rs
$dualLegEngine
EOF_DUAL

cat << 'EOF_TRI' > /home/ubuntu/solana-mev-arb/src/triangular_live_engine.rs
$triangularLiveEngine
EOF_TRI

cat << 'EOF_LIVE' > /home/ubuntu/solana-mev-arb/src/live_engine.rs
$liveEngine
EOF_LIVE

cat << 'EOF_JITOB' > /home/ubuntu/solana-mev-arb/src/jito_builder.rs
$jitoBuilder
EOF_JITOB

cat << 'EOF_JITO' > /home/ubuntu/solana-mev-arb/src/jito.rs
$jitoRs
EOF_JITO

cat << 'EOF_ROUTER' > /home/ubuntu/solana-mev-arb/src/router.rs
$routerRs
EOF_ROUTER

cat << 'EOF_AMM' > /home/ubuntu/solana-mev-arb/src/amm.rs
$ammRs
EOF_AMM

cat << 'EOF_MATH' > /home/ubuntu/solana-mev-arb/src/math.rs
$mathRs
EOF_MATH

cat << 'EOF_PAPER' > /home/ubuntu/solana-mev-arb/src/paper_engine.rs
$paperEngine
EOF_PAPER

cat << 'EOF_TYPES' > /home/ubuntu/solana-mev-arb/src/types.rs
$typesRs
EOF_TYPES

cat << 'EOF_WS' > /home/ubuntu/solana-mev-arb/src/websocket_feed.rs
$wsFeed
EOF_WS

cat << 'EOF_CPMM' > /home/ubuntu/solana-mev-arb/src/raydium_cpmm.rs
$raydiumCpmm
EOF_CPMM

cat << 'EOF_MIG' > /home/ubuntu/solana-mev-arb/src/migration_sniper.rs
$migrationSniper
EOF_MIG

cat << 'EOF_TRIMATH' > /home/ubuntu/solana-mev-arb/src/triangular_math.rs
$triMath
EOF_TRIMATH

cat << 'EOF_HELIUS' > /home/ubuntu/solana-mev-arb/src/helius_stream.rs
$heliusStream
EOF_HELIUS

cat << 'EOF_SRV' > /home/ubuntu/solana-mev-arb/dashboard/server.js
$serverJs
EOF_SRV

cat << 'EOF_HTML' > /home/ubuntu/solana-mev-arb/dashboard/public/index.html
$indexHtml
EOF_HTML

cat << 'EOF_ECO' > /home/ubuntu/solana-mev-arb/ecosystem.config.js
$ecoConfig
EOF_ECO

cat << 'EOF_DCONF' > /home/ubuntu/solana-mev-arb/data/dual_leg_config.json
$dualConfig
EOF_DCONF

cat << 'EOF_TCONF' > /home/ubuntu/solana-mev-arb/data/triangular_config.json
$triConfig
EOF_TCONF

echo "📦 Files written. Building release binary on Frankfurt EC2..."
cd /home/ubuntu/solana-mev-arb
~/.cargo/bin/cargo build --release

echo "⚡ Restarting PM2 process manager with ecosystem.config.js..."
pm2 delete all || true
pm2 start ecosystem.config.js
pm2 save
pm2 status
"@

$remoteScript | ssh -i $SSH_KEY -o StrictHostKeyChecking=no -o ConnectTimeout=30 $REMOTE_HOST "bash -s"
