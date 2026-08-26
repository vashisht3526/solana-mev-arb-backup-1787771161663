$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

$cargoToml = Get-Content -Raw "Cargo.toml"
$reclaimCode = Get-Content -Raw "src\bin\reclaim_rent.rs"

$script = @"
cat << 'EOF' > /home/ubuntu/solana-mev-arb/Cargo.toml
$cargoToml
EOF

cat << 'EOF' > /home/ubuntu/solana-mev-arb/src/bin/reclaim_rent.rs
$reclaimCode
EOF

cd /home/ubuntu/solana-mev-arb
~/.cargo/bin/cargo run --bin reclaim_rent
"@

$script | ssh -i $SSH_KEY -o ServerAliveInterval=15 -o ConnectTimeout=30 $REMOTE_HOST "bash -s"
