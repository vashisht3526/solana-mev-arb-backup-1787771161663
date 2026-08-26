$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

$jitoBuilderBytes = [System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\jito_builder.rs")
$b64Jito = [Convert]::ToBase64String($jitoBuilderBytes)

$liveEngineBytes = [System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\live_engine.rs")
$b64Live = [Convert]::ToBase64String($liveEngineBytes)

$cmd = @"
echo '$b64Jito' | base64 -d > /home/ubuntu/solana-mev-arb/src/jito_builder.rs
echo '$b64Live' | base64 -d > /home/ubuntu/solana-mev-arb/src/live_engine.rs
cd /home/ubuntu/solana-mev-arb
~/.cargo/bin/cargo build --release
pm2 restart all
pm2 save
"@

ssh -i $SSH_KEY $REMOTE_HOST $cmd
