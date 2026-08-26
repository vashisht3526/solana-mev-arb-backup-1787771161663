const https = require('https');
const fs = require('fs');
const path = require('path');

const TARGET_WALLET = 'MRiYA4oN3158fCV8evhuCofrDzbHyYvYnGZUDJvoCsa';
const HELIUS_KEY = '5125a754-c4e8-467a-af07-fd3ccf3d1143';
const RPC_URL = `https://mainnet.helius-rpc.com/?api-key=${HELIUS_KEY}`;

function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

async function rpcCall(method, params = []) {
    return new Promise((resolve, reject) => {
        const postData = JSON.stringify({
            jsonrpc: "2.0",
            id: 1,
            method,
            params
        });

        const url = new URL(RPC_URL);
        const req = https.request({
            hostname: url.hostname,
            port: 443,
            path: url.pathname + url.search,
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Content-Length': Buffer.byteLength(postData)
            },
            timeout: 30000
        }, (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => {
                try {
                    const json = JSON.parse(data);
                    if (json.error) {
                        reject(new Error(JSON.stringify(json.error)));
                    } else {
                        resolve(json.result);
                    }
                } catch (e) {
                    reject(e);
                }
            });
        });

        req.on('error', reject);
        req.write(postData);
        req.end();
    });
}

async function analyzeTargetWallet() {
    console.log(`🔍 [FORENSIC WALLET ANALYSIS] Target: ${TARGET_WALLET}`);

    // 1. Get Balance
    const balanceRes = await rpcCall("getBalance", [TARGET_WALLET]);
    const balanceSol = balanceRes ? (balanceRes.value / 1e9) : 0;
    console.log(`💰 Current Balance: ${balanceSol.toFixed(4)} SOL`);

    // 2. Get Signatures (last 50)
    console.log(`📜 Fetching recent transaction signatures...`);
    const signatures = await rpcCall("getSignaturesForAddress", [
        TARGET_WALLET,
        { limit: 40 }
    ]);

    if (!signatures || signatures.length === 0) {
        console.log("⚠️ No recent signatures found for this wallet.");
        return;
    }

    console.log(`✅ Found ${signatures.length} recent transactions. Analyzing transaction anatomy...`);

    const txDetails = [];
    const programFrequency = {};
    const tradeSizes = [];
    const netProfits = [];
    const tipAmounts = [];
    let successCount = 0;
    let failCount = 0;

    for (let i = 0; i < Math.min(signatures.length, 25); i++) {
        const sigInfo = signatures[i];
        if (sigInfo.err) {
            failCount++;
        } else {
            successCount++;
        }

        try {
            const tx = await rpcCall("getTransaction", [
                sigInfo.signature,
                { encoding: "jsonParsed", maxSupportedTransactionVersion: 0 }
            ]);

            if (tx && tx.meta) {
                const blockTime = tx.blockTime ? new Date(tx.blockTime * 1000).toISOString() : "N/A";
                const slot = tx.slot;
                const feeLamports = tx.meta.fee || 0;
                
                // Account keys
                const accountKeys = tx.transaction.message.accountKeys.map(k => typeof k === 'string' ? k : k.pubkey);
                
                // Balance Delta for target wallet
                const walletIdx = accountKeys.indexOf(TARGET_WALLET);
                let solDelta = 0;
                if (walletIdx !== -1 && tx.meta.preBalances && tx.meta.postBalances) {
                    const pre = tx.meta.preBalances[walletIdx];
                    const post = tx.meta.postBalances[walletIdx];
                    solDelta = (post - pre) / 1e9;
                }

                // Check instructions & programs
                const instructions = tx.transaction.message.instructions;
                const invokedPrograms = [];
                for (const ix of instructions) {
                    const pId = ix.programId || ix.program;
                    if (pId) {
                        invokedPrograms.push(pId);
                        programFrequency[pId] = (programFrequency[pId] || 0) + 1;
                    }
                }

                // Inner instructions
                if (tx.meta.innerInstructions) {
                    for (const inner of tx.meta.innerInstructions) {
                        for (const ix of inner.instructions) {
                            const pId = ix.programId || ix.program;
                            if (pId) {
                                programFrequency[pId] = (programFrequency[pId] || 0) + 1;
                            }
                        }
                    }
                }

                // Detect Jito tip
                const jitoTipAccounts = [
                    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
                    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
                    "Cw8CFyM9FkoMi7K7CrnqCGjZvVgyKZxo19C9jmG8RX5z",
                    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
                    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
                    "ADuUkR4vqLUMWXxW9gh6D6L8pWyssDxBpTxw9YKE1UHT",
                    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
                    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT"
                ];

                let tipPaidSol = 0;
                for (let aIdx = 0; aIdx < accountKeys.length; aIdx++) {
                    if (jitoTipAccounts.includes(accountKeys[aIdx])) {
                        if (tx.meta.preBalances && tx.meta.postBalances) {
                            const tipLamports = tx.meta.postBalances[aIdx] - tx.meta.preBalances[aIdx];
                            if (tipLamports > 0) {
                                tipPaidSol = tipLamports / 1e9;
                                tipAmounts.push(tipPaidSol);
                            }
                        }
                    }
                }

                // Token balances delta
                const preTokens = tx.meta.preTokenBalances || [];
                const postTokens = tx.meta.postTokenBalances || [];

                txDetails.push({
                    signature: sigInfo.signature,
                    slot,
                    blockTime,
                    solDelta,
                    tipPaidSol,
                    feeLamports,
                    err: sigInfo.err,
                    programs: [...new Set(invokedPrograms)]
                });

                if (solDelta !== 0) {
                    netProfits.push(solDelta);
                }
            }
            await sleep(150);
        } catch (err) {
            console.warn(`⚠️ Error parsing tx ${sigInfo.signature}: ${err.message}`);
        }
    }

    const report = {
        target_wallet: TARGET_WALLET,
        balance_sol: balanceSol,
        sample_size: txDetails.length,
        success_rate: (successCount / (successCount + failCount) * 100).toFixed(1) + "%",
        program_frequency: programFrequency,
        average_tip_sol: tipAmounts.length > 0 ? (tipAmounts.reduce((a, b) => a + b, 0) / tipAmounts.length).toFixed(6) : "0",
        transactions: txDetails
    };

    fs.writeFileSync(path.join(__dirname, 'wallet_deep_analysis.json'), JSON.stringify(report, null, 2));
    console.log(`\n🏆 Analysis Complete! Saved report to scratch/wallet_deep_analysis.json`);
}

analyzeTargetWallet().catch(console.error);
