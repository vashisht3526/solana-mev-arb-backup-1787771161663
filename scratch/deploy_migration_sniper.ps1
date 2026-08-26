$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

$cargoB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\Cargo.toml"))
$typesB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\types.rs"))
$cpmmB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\raydium_cpmm.rs"))
$sniperB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\migration_sniper.rs"))
$binSniperB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\bin\migration_sniper.rs"))
$triangularB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\triangular_math.rs"))
$scannerB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\scanner.rs"))
$mainB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\src\main.rs"))
$serverB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\dashboard\server.js"))
$indexB64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("$PSScriptRoot\..\dashboard\public\index.html"))

$cmd = @"
mkdir -p /home/ubuntu/solana-mev-arb/src/bin
echo '$cargoB64' | base64 -d > /home/ubuntu/solana-mev-arb/Cargo.toml
echo '$typesB64' | base64 -d > /home/ubuntu/solana-mev-arb/src/types.rs
echo '$cpmmB64' | base64 -d > /home/ubuntu/solana-mev-arb/src/raydium_cpmm.rs
echo '$sniperB64' | base64 -d > /home/ubuntu/solana-mev-arb/src/migration_sniper.rs
echo '$binSniperB64' | base64 -d > /home/ubuntu/solana-mev-arb/src/bin/migration_sniper.rs
echo '$triangularB64' | base64 -d > /home/ubuntu/solana-mev-arb/src/triangular_math.rs
echo '$scannerB64' | base64 -d > /home/ubuntu/solana-mev-arb/src/scanner.rs
echo '$mainB64' | base64 -d > /home/ubuntu/solana-mev-arb/src/main.rs
echo '$serverB64' | base64 -d > /home/ubuntu/solana-mev-arb/dashboard/server.js
echo '$indexB64' | base64 -d > /home/ubuntu/solana-mev-arb/dashboard/public/index.html
cd /home/ubuntu/solana-mev-arb
~/.cargo/bin/cargo build --release
pm2 start ./target/release/migration_sniper --name rust-migration-sniper || pm2 restart rust-migration-sniper
pm2 start "./target/release/solana-mev-arb -- --triangular" --name rust-mev-arb-triangular || pm2 restart rust-mev-arb-triangular
pm2 restart all
pm2 save
"@

$cmd | ssh -i $SSH_KEY $REMOTE_HOST "bash -s"
