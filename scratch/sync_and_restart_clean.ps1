$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

$mainRs = Get-Content -Raw "src\main.rs"
$scannerRs = Get-Content -Raw "src\scanner.rs"
$jitoBuilderRs = Get-Content -Raw "src\jito_builder.rs"
$liveEngineRs = Get-Content -Raw "src\live_engine.rs"
$serverJs = Get-Content -Raw "dashboard\server.js"

$script = @"
cat << 'EOF' > /home/ubuntu/solana-mev-arb/src/main.rs
$mainRs
EOF

cat << 'EOF' > /home/ubuntu/solana-mev-arb/src/scanner.rs
$scannerRs
EOF

cat << 'EOF' > /home/ubuntu/solana-mev-arb/src/jito_builder.rs
$jitoBuilderRs
EOF

cat << 'EOF' > /home/ubuntu/solana-mev-arb/src/live_engine.rs
$liveEngineRs
EOF

cat << 'EOF' > /home/ubuntu/solana-mev-arb/dashboard/server.js
$serverJs
EOF

# Remove legacy live_executor.js if present
rm -f /home/ubuntu/solana-mev-arb/dashboard/live_executor.js

cd /home/ubuntu/solana-mev-arb
~/.cargo/bin/cargo build --release

# Restart all 5 PM2 processes smoothly
pm2 restart all || pm2 start ecosystem.config.js
pm2 save
pm2 list
"@

$script | ssh -i $SSH_KEY -o ServerAliveInterval=15 -o ConnectTimeout=30 $REMOTE_HOST "bash -s"
