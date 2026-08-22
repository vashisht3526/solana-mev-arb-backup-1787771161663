$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

$serverContent = Get-Content -Raw "dashboard\server.js"
$htmlContent = Get-Content -Raw "dashboard\public\index.html"

$remoteScript = @"
cat << 'EOF' > /home/ubuntu/solana-mev-arb/dashboard/server.js
$serverContent
EOF

cat << 'EOF' > /home/ubuntu/solana-mev-arb/dashboard/public/index.html
$htmlContent
EOF

pm2 restart mev-dashboard
sleep 2
pm2 status
"@

$remoteScript | ssh -i $SSH_KEY -o ConnectTimeout=25 $REMOTE_HOST "bash -s"
