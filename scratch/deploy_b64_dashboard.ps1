$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

$serverBytes = [System.IO.File]::ReadAllBytes("$PSScriptRoot\..\dashboard\server.js")
$b64 = [Convert]::ToBase64String($serverBytes)

$cmd = "echo '$b64' | base64 -d > /home/ubuntu/solana-mev-arb/dashboard/server.js && pm2 restart 5 && pm2 save"
ssh -i $SSH_KEY $REMOTE_HOST $cmd
