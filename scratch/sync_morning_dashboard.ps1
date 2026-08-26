$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

$serverJs = Get-Content -Raw "dashboard\server.js"

$script = @"
cat << 'EOF' > /home/ubuntu/solana-mev-arb/dashboard/server.js
$serverJs
EOF

cd /home/ubuntu/solana-mev-arb
pm2 restart mev-dashboard
"@

$script | ssh -i $SSH_KEY -o ServerAliveInterval=15 -o ConnectTimeout=30 $REMOTE_HOST "bash -s"
