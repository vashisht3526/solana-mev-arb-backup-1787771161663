$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

$cargoToml = Get-Content -Raw "Cargo.toml"
$jitoBuilder = Get-Content -Raw "src\jito_builder.rs"
$liveEngine = Get-Content -Raw "src\live_engine.rs"

$script = @"
cat << 'EOF' > /home/ubuntu/solana-mev-arb/Cargo.toml
$cargoToml
EOF

cat << 'EOF' > /home/ubuntu/solana-mev-arb/src/jito_builder.rs
$jitoBuilder
EOF

cat << 'EOF' > /home/ubuntu/solana-mev-arb/src/live_engine.rs
$liveEngine
EOF

cd /home/ubuntu/solana-mev-arb
~/.cargo/bin/cargo build --release
"@

$script | ssh -i $SSH_KEY -o ConnectTimeout=20 $REMOTE_HOST "bash -s"
