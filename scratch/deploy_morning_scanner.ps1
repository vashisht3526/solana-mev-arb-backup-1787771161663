$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

$scannerBytes = [System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\scanner.rs")
$b64 = [Convert]::ToBase64String($scannerBytes)

$cmd = @"
echo '$b64' | base64 -d > /home/ubuntu/solana-mev-arb/src/scanner.rs
cd /home/ubuntu/solana-mev-arb
~/.cargo/bin/cargo build --release
pm2 restart all
pm2 save
"@

ssh -i $SSH_KEY $REMOTE_HOST $cmd
