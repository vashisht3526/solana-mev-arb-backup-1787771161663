$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

$cargoToml = Get-Content -Raw "Cargo.toml"
$sellAll = Get-Content -Raw "src\bin\sell_all.rs"

$script = @"
cat << 'EOF' > /home/ubuntu/solana-mev-arb/Cargo.toml
$cargoToml
EOF

mkdir -p /home/ubuntu/solana-mev-arb/src/bin
cat << 'EOF' > /home/ubuntu/solana-mev-arb/src/bin/sell_all.rs
$sellAll
EOF

cd /home/ubuntu/solana-mev-arb
~/.cargo/bin/cargo run --bin sell_all
"@

$script | ssh -i $SSH_KEY -o ConnectTimeout=20 $REMOTE_HOST "bash -s"
