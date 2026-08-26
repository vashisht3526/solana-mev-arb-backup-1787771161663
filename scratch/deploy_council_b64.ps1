$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

$typesB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\types.rs"))
$ammB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\amm.rs"))
$jitoB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\jito_builder.rs"))
$liveB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\live_engine.rs"))
$scannerB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\scanner.rs"))

$cmd = @"
echo '$typesB64' | base64 -d > /home/ubuntu/solana-mev-arb/src/types.rs
echo '$ammB64' | base64 -d > /home/ubuntu/solana-mev-arb/src/amm.rs
echo '$jitoB64' | base64 -d > /home/ubuntu/solana-mev-arb/src/jito_builder.rs
echo '$liveB64' | base64 -d > /home/ubuntu/solana-mev-arb/src/live_engine.rs
echo '$scannerB64' | base64 -d > /home/ubuntu/solana-mev-arb/src/scanner.rs
cd /home/ubuntu/solana-mev-arb
~/.cargo/bin/cargo build --release
pm2 restart all
pm2 save
"@

$cmd | ssh -i $SSH_KEY $REMOTE_HOST "bash -s"
