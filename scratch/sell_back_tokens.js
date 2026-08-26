const { Connection, Keypair, VersionedTransaction } = require('@solana/web3.js');
const https = require('https');
const fs = require('fs');
const bs58 = require('bs58');

const RPC_ENDPOINT = 'https://api.mainnet-beta.solana.com';
const connection = new Connection(RPC_ENDPOINT, 'confirmed');

const KEYPAIR_PATH = '/home/ubuntu/.config/solana/mev_live.json';

function loadKeypair() {
    const targetPath = fs.existsSync(KEYPAIR_PATH) ? KEYPAIR_PATH : 'data/mev_live.json';
    const content = fs.readFileSync(targetPath, 'utf-8').trim();
    let secretKeyBytes;
    if (content.startsWith('[')) {
        secretKeyBytes = Uint8Array.from(JSON.parse(content));
    } else {
        secretKeyBytes = bs58.decode(content);
    }
    return Keypair.fromSecretKey(secretKeyBytes);
}

async function sellTokenBackToSol(tokenMint, tokenAmountRaw, decimals) {
    const keypair = loadKeypair();
    const pubkeyStr = keypair.publicKey.toBase58();
    const solMint = 'So11111111111111111111111111111111111111112';

    console.log(`🔄 [SELLING TOKEN BACK TO SOL] Mint: ${tokenMint} | Raw Amount: ${tokenAmountRaw}`);

    // 1. Get Quote
    const quoteUrl = `https://api.jup.ag/swap/v1/quote?inputMint=${tokenMint}&outputMint=${solMint}&amount=${tokenAmountRaw}&slippageBps=150`;
    const quoteRes = await new Promise((resolve, reject) => {
        https.get(quoteUrl, { headers: { 'User-Agent': 'Mozilla/5.0' } }, (res) => {
            let d = '';
            res.on('data', c => d += c);
            res.on('end', () => {
                try { resolve(JSON.parse(d)); } catch (e) { reject(e); }
            });
        }).on('error', reject);
    });

    if (!quoteRes || !quoteRes.outAmount) {
        console.error("Quote failed:", quoteRes);
        return;
    }

    console.log(`✅ [SELL QUOTE] In: ${tokenAmountRaw} tokens -> Out: ${(quoteRes.outAmount / 1e9).toFixed(5)} SOL`);

    // 2. Compile Swap
    const swapPostData = JSON.stringify({
        quoteResponse: quoteRes,
        userPublicKey: pubkeyStr,
        wrapAndUnwrapSol: true,
        dynamicComputeUnitLimit: true,
        prioritizationFeeLamports: 50000
    });

    const swapRes = await new Promise((resolve, reject) => {
        const req = https.request('https://api.jup.ag/swap/v1/swap', {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Content-Length': Buffer.byteLength(swapPostData),
                'User-Agent': 'Mozilla/5.0'
            }
        }, (res) => {
            let d = '';
            res.on('data', c => d += c);
            res.on('end', () => {
                try { resolve(JSON.parse(d)); } catch (e) { reject(e); }
            });
        });
        req.on('error', reject);
        req.write(swapPostData);
        req.end();
    });

    if (!swapRes.swapTransaction) {
        console.error("Swap compilation failed:", swapRes);
        return;
    }

    // 3. Deserialize & Sign
    const tx = VersionedTransaction.deserialize(Buffer.from(swapRes.swapTransaction, 'base64'));
    tx.sign([keypair]);

    // 4. Broadcast
    const sig = await connection.sendRawTransaction(tx.serialize(), {
        skipPreflight: false,
        preflightCommitment: 'confirmed'
    });

    console.log(`📤 [SELL BROADCAST SUCCESS] Solscan: https://solscan.io/tx/${sig}`);
    await connection.confirmTransaction(sig, 'confirmed');
    console.log(`🏆 [SELL CONFIRMED ON-CHAIN!] SOL returned to wallet!`);
}

async function main() {
    console.log("===============================================================================");
    console.log("🔄 UNWINDING TOKEN HOLDINGS & RETURNING 100% SOL TO WALLET");
    console.log("===============================================================================\n");

    // Token 1: 5483056652 lamports of CboMcTUYUcy9E6B3yGdFn6aEsGUnYV6yWeoeukw6pump
    try {
        await sellTokenBackToSol('CboMcTUYUcy9E6B3yGdFn6aEsGUnYV6yWeoeukw6pump', '5483056652', 6);
    } catch (e) {
        console.error("Token 1 sell error:", e.message);
    }

    // Token 2: 571041084 lamports of 6AJcP7wuLwmRYLBNbi825wgguaPsWzPBEHcHndpRpump
    try {
        await sellTokenBackToSol('6AJcP7wuLwmRYLBNbi825wgguaPsWzPBEHcHndpRpump', '571041084', 6);
    } catch (e) {
        console.error("Token 2 sell error:", e.message);
    }

    const keypair = loadKeypair();
    const finalBal = await connection.getBalance(keypair.publicKey);
    console.log(`\n💰 FINAL RECOVERED ON-CHAIN BALANCE: ${(finalBal / 1e9).toFixed(4)} SOL`);
}

main().catch(console.error);
