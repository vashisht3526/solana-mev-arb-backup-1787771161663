$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

$executorContent = Get-Content -Raw "dashboard\live_executor.js"
$serverContent = Get-Content -Raw "dashboard\server.js"
$pkgContent = Get-Content -Raw "dashboard\package.json"

$script = @"
cat << 'EOF' > /home/ubuntu/solana-mev-arb/dashboard/package.json
$pkgContent
EOF

cat << 'EOF' > /home/ubuntu/solana-mev-arb/dashboard/live_executor.js
$executorContent
EOF

cat << 'EOF' > /home/ubuntu/solana-mev-arb/dashboard/server.js
$serverContent
EOF

cd /home/ubuntu/solana-mev-arb/dashboard
npm install --no-audit --no-fund
pm2 restart mev-dashboard
sleep 2
pm2 status
"@

$script | ssh -i $SSH_KEY -o ConnectTimeout=20 $REMOTE_HOST "bash -s"
