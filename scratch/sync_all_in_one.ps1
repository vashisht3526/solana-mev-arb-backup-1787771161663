$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

$typesContent = Get-Content -Raw "src\types.rs"
$scannerContent = Get-Content -Raw "src\scanner.rs"
$routerContent = Get-Content -Raw "src\router.rs"

$remoteScript = @"
cat << 'EOF' > /home/ubuntu/solana-mev-arb/src/types.rs
$typesContent
EOF

cat << 'EOF' > /home/ubuntu/solana-mev-arb/src/scanner.rs
$scannerContent
EOF

cat << 'EOF' > /home/ubuntu/solana-mev-arb/src/router.rs
$routerContent
EOF

source ~/.cargo/env
cd /home/ubuntu/solana-mev-arb
cargo build --release
pm2 restart all
sleep 3
pm2 status
"@

$remoteScript | ssh -i $SSH_KEY -o ConnectTimeout=25 $REMOTE_HOST "bash -s"
