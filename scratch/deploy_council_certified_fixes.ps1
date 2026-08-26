$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

scp -i $SSH_KEY src\jito_builder.rs "${REMOTE_HOST}:/home/ubuntu/solana-mev-arb/src/jito_builder.rs"
scp -i $SSH_KEY src\live_engine.rs "${REMOTE_HOST}:/home/ubuntu/solana-mev-arb/src/live_engine.rs"
scp -i $SSH_KEY src\scanner.rs "${REMOTE_HOST}:/home/ubuntu/solana-mev-arb/src/scanner.rs"

ssh -i $SSH_KEY $REMOTE_HOST "cd /home/ubuntu/solana-mev-arb && ~/.cargo/bin/cargo build --release && pm2 restart all && pm2 save"
