const fs = require('fs');
const { spawn } = require('child_process');

const SSH_KEY = 'C:\\Users\\Vashisht\\.ssh\\solana_aws_key';
const HOST = 'ubuntu@52.58.226.98';

const typesContent = fs.readFileSync('src/types.rs', 'utf8');
const scannerContent = fs.readFileSync('src/scanner.rs', 'utf8');
const routerContent = fs.readFileSync('src/router.rs', 'utf8');

const combinedScript = `
cat << 'EOF' > /home/ubuntu/solana-mev-arb/src/types.rs
${typesContent}
EOF

cat << 'EOF' > /home/ubuntu/solana-mev-arb/src/scanner.rs
${scannerContent}
EOF

cat << 'EOF' > /home/ubuntu/solana-mev-arb/src/router.rs
${routerContent}
EOF

source ~/.cargo/env
cd /home/ubuntu/solana-mev-arb
cargo build --release
pm2 restart all
sleep 3
pm2 status
`;

const proc = spawn('C:\\Windows\\System32\\OpenSSH\\ssh.exe', [
    '-i', SSH_KEY,
    '-o', 'ConnectTimeout=20',
    HOST,
    'bash -s'
]);

proc.stdin.write(combinedScript);
proc.stdin.end();

proc.stdout.on('data', d => process.stdout.write(d));
proc.stderr.on('data', d => process.stderr.write(d));

proc.on('close', code => {
    if (code === 0) {
        console.log('✅ COMPLETE SYNC, BUILD, AND PM2 RESTART SUCCEEDED!');
    } else {
        console.error(`❌ Process exited with code ${code}`);
    }
});
