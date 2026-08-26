const https = require('https');
const fs = require('fs');

const HELIUS_KEY = '5125a754-c4e8-467a-af07-fd3ccf3d1143';
const RPC_URL = `https://mainnet.helius-rpc.com/?api-key=${HELIUS_KEY}`;

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
                    if (json.error) reject(new Error(JSON.stringify(json.error)));
                    else resolve(json.result);
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

async function deepInspect() {
    const TARGET = 'MRiYA4oN3158fCV8evhuCofrDzbHyYvYnGZUDJvoCsa';
    const sigs = await rpcCall("getSignaturesForAddress", [TARGET, { limit: 50 }]);
    console.log(`Analyzing ${sigs.length} transactions for ${TARGET}...`);

    let winningTrades = [];
    let losingTrades = [];

    for (const s of sigs) {
        const tx = await rpcCall("getTransaction", [
            s.signature,
            { encoding: "jsonParsed", maxSupportedTransactionVersion: 0 }
        ]);

        if (!tx || !tx.meta) continue;

        const accountKeys = tx.transaction.message.accountKeys.map(k => typeof k === 'string' ? k : k.pubkey);
        const walletIdx = accountKeys.indexOf(TARGET);

        let solDelta = 0;
        if (walletIdx !== -1 && tx.meta.preBalances && tx.meta.postBalances) {
            const pre = tx.meta.preBalances[walletIdx];
            const post = tx.meta.postBalances[walletIdx];
            solDelta = (post - pre) / 1e9;
        }

        // Parse token balance deltas
        const tokenDeltas = [];
        const preToken = tx.meta.preTokenBalances || [];
        const postToken = tx.meta.postTokenBalances || [];

        // Check inner instructions to see DEX hops
        const hops = [];
        if (tx.meta.innerInstructions) {
            for (const inner of tx.meta.innerInstructions) {
                for (const ix of inner.instructions) {
                    const pId = ix.programId || ix.program;
                    if (pId === "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA") hops.push("Pump.fun");
                    else if (pId === "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo") hops.push("Meteora DLMM");
                    else if (pId === "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc") hops.push("Orca Whirlpool");
                    else if (pId === "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK") hops.push("Raydium CPMM");
                    else if (pId === "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8") hops.push("Raydium AMM V4");
                }
            }
        }

        const logMessages = tx.meta.logMessages || [];
        const computeUnitsConsumed = tx.meta.computeUnitsConsumed || 0;

        const tradeObj = {
            signature: s.signature,
            slot: tx.slot,
            blockTime: tx.blockTime ? new Date(tx.blockTime * 1000).toISOString() : "N/A",
            solDelta,
            err: s.err,
            hops: [...new Set(hops)],
            cuConsumed: computeUnitsConsumed,
            logsSummary: logMessages.slice(0, 8)
        };

        if (!s.err && solDelta > 0.0001) {
            winningTrades.push(tradeObj);
        } else {
            losingTrades.push(tradeObj);
        }
    }

    console.log(`\n🏆 FOUND ${winningTrades.length} WINNING TRADES & ${losingTrades.length} OTHER/FAILED TRADES.`);
    fs.writeFileSync('scratch/winning_trades_analysis.json', JSON.stringify({ winningTrades, sampleLosing: losingTrades.slice(0, 10) }, null, 2));
}

deepInspect().catch(console.error);
