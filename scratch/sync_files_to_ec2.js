const fs = require('fs');
const { spawn } = require('child_process');

const SSH_KEY = 'C:\\Users\\Vashisht\\.ssh\\solana_aws_key';
const HOST = 'ubuntu@52.58.226.98';

function runSsh(remoteCmd, stdinData = null) {
    return new Promise((resolve, reject) => {
        const proc = spawn('ssh', ['-i', SSH_KEY, '-o', 'ConnectTimeout=15', HOST, remoteCmd]);
        let stdout = '';
        let stderr = '';

        if (stdinData) {
            proc.stdin.write(stdinData);
            proc.stdin.end();
        }

        proc.stdout.on('data', data => {
            stdout += data.toString();
            process.stdout.write(data);
        });

        proc.stderr.on('data', data => {
            stderr += data.toString();
            process.stderr.write(data);
        });

        proc.on('close', code => {
            if (code === 0) resolve(stdout);
            else reject(new Error(`Command exited with code ${code}: ${stderr}`));
        });
    });
}

async function main() {
    const filesToSync = [
        'src/types.rs',
        'src/scanner.rs',
        'src/router.rs'
    ];

    for (const file of filesToSync) {
        console.log(`📤 Uploading ${file}...`);
        const content = fs.readFileSync(file, 'utf8');
        await runSsh(`cat > /home/ubuntu/solana-mev-arb/${file}`, content);
    }

    console.log('🔨 Compiling release binary on Frankfurt EC2...');
    await runSsh('source ~/.cargo/env && cd /home/ubuntu/solana-mev-arb && cargo build --release');

    console.log('🔄 Restarting all PM2 instances...');
    await runSsh('pm2 restart all && sleep 3 && pm2 status');
    console.log('✅ All instances successfully upgraded and running!');
}

main().catch(err => {
    console.error('❌ Sync failed:', err);
    process.exit(1);
});
