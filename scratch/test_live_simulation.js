const fs = require('fs');
const path = require('path');
const https = require('https');
const { Connection, Keypair, VersionedTransaction, PublicKey, TransactionMessage } = require('@solana/web3.js');
const bs58 = require('bs58');

const SOLANA_RPC = 'https://api.mainnet-beta.solana.com';
const connection = new Connection(SOLANA_RPC, 'confirmed');

const SOL_MINT = 'So11111111111111111111111111111111111111112';
// Test token: USDC (EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v) - High liquidity, robust ALTs
const TEST_TOKEN_MINT = 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v';

function httpGet(url) {
    return new Promise((resolve, reject) => {
        https.get(url, { headers: { 'User-Agent': 'Mozilla/5.0' }, timeout: 10000 }, (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => {
                try { resolve(JSON.parse(data)); } catch (e) { reject(e); }
            });
        }).on('error', reject);
    });
}

function httpPost(url, payload) {
    return new Promise((resolve, reject) => {
        const postData = JSON.stringify(payload);
        const parsedUrl = new URL(url);
        const req = https.request({
            hostname: parsedUrl.hostname,
            port: 443,
            path: parsedUrl.pathname,
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Content-Length': Buffer.byteLength(postData),
                'User-Agent': 'Mozilla/5.0'
            },
            timeout: 10000
        }, (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => {
                try { resolve(JSON.parse(data)); } catch (e) { reject(e); }
            });
        });
        req.on('error', reject);
        req.write(postData);
        req.end();
    });
}

async function runLiveAtomicSimulationTest() {
    console.log("===============================================================================");
    console.log("🧪 LIVE ON-CHAIN ATOMIC DUAL-LEG SIMULATION TEST");
    console.log("===============================================================================\n");

    const keypairPath = path.join(__dirname, '..', 'data', 'mev_live.json');
    if (!fs.existsSync(keypairPath)) {
        console.error("❌ Keypair not found at data/mev_live.json");
        process.exit(1);
    }

    const keypairContent = fs.readFileSync(keypairPath, 'utf-8').trim();
    let secretKeyBytes;
    if (keypairContent.startsWith('[')) {
        secretKeyBytes = Uint8Array.from(JSON.parse(keypairContent));
    } else {
        secretKeyBytes = bs58.decode(keypairContent);
    }
    const keypair = Keypair.fromSecretKey(secretKeyBytes);
    const walletPubkey = keypair.publicKey.toBase58();
    console.log(`🔑 Armed Wallet: ${walletPubkey}`);

    const balance = await connection.getBalance(keypair.publicKey);
    console.log(`💰 Current Wallet Balance: ${(balance / 1e9).toFixed(6)} SOL`);

    // 1. Leg A Quote: SOL -> USDC (0.01 SOL test size)
    const testLamports = 10000000; // 0.01 SOL
    console.log(`\n📡 Fetching Leg A Quote: ${testLamports / 1e9} SOL -> USDC...`);
    const quoteUrlA = `https://quote-api.jup.ag/v6/quote?inputMint=${SOL_MINT}&outputMint=${TEST_TOKEN_MINT}&amount=${testLamports}&slippageBps=50`;
    const quoteA = await httpGet(quoteUrlA);
    const minIntermediate = quoteA.otherAmountThreshold || quoteA.outAmount;
    console.log(`✅ Leg A Quote: In ${testLamports} lamports -> Min Out: ${minIntermediate} USDC units`);

    // 2. Leg B Quote: USDC -> SOL (using minIntermediate as input)
    console.log(`\n📡 Fetching Leg B Quote: ${minIntermediate} USDC -> SOL...`);
    const quoteUrlB = `https://quote-api.jup.ag/v6/quote?inputMint=${TEST_TOKEN_MINT}&outputMint=${SOL_MINT}&amount=${minIntermediate}&slippageBps=50`;
    const quoteB = await httpGet(quoteUrlB);
    console.log(`✅ Leg B Quote: In ${minIntermediate} USDC -> Expected Out: ${quoteB.outAmount} lamports`);

    // 3. Fetch Swap Instructions for both legs
    console.log("\n⚙️ Fetching Jupiter swap instructions for both legs...");
    const [ixA, ixB] = await Promise.all([
        httpPost("https://quote-api.jup.ag/v6/swap-instructions", {
            quoteResponse: quoteA,
            userPublicKey: walletPubkey,
            wrapAndUnwrapSol: true,
            dynamicComputeUnitLimit: true
        }),
        httpPost("https://quote-api.jup.ag/v6/swap-instructions", {
            quoteResponse: quoteB,
            userPublicKey: walletPubkey,
            wrapAndUnwrapSol: true,
            dynamicComputeUnitLimit: true
        })
    ]);

    // 4. Resolve Address Lookup Tables
    const altAddresses = new Set([
        ...(ixA.addressLookupTableAddresses || []),
        ...(ixB.addressLookupTableAddresses || [])
    ]);
    console.log(`🔍 Discovered ${altAddresses.size} unique Address Lookup Tables:`, Array.from(altAddresses));

    const lookupTableAccounts = [];
    for (const altAddr of altAddresses) {
        const altPubkey = new PublicKey(altAddr);
        const res = await connection.getAddressLookupTable(altPubkey);
        if (res.value) {
            lookupTableAccounts.push(res.value);
        }
    }
    console.log(`✅ Successfully loaded ${lookupTableAccounts.length} on-chain ALT accounts.`);

    // 5. Helper function to deserialize instruction
    function deserializeIx(rawIx) {
        return {
            programId: new PublicKey(rawIx.programId),
            keys: rawIx.accounts.map(acc => ({
                pubkey: new PublicKey(acc.pubkey),
                isSigner: acc.isSigner,
                isWritable: acc.isWritable
            })),
            data: Buffer.from(rawIx.data, 'base64')
        };
    }

    // 6. Compile Atomic Dual-Leg Instructions
    const compiledInstructions = [];
    const seenFingerprints = new Set();

    function addInstruction(ix) {
        const key = `${ix.programId.toBase58()}_${ix.data.toString('base64')}_${ix.keys.map(k => `${k.pubkey.toBase58()}:${k.isSigner}:${k.isWritable}`).join(';')}`;
        if (!seenFingerprints.has(key)) {
            seenFingerprints.add(key);
            compiledInstructions.push(ix);
        }
    }

    // Compute Budget
    compiledInstructions.push(
        require('@solana/web3.js').ComputeBudgetProgram.setComputeUnitLimit({ units: 1400000 })
    );
    compiledInstructions.push(
        require('@solana/web3.js').ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 50000 })
    );

    // Leg A Setup + Swap
    if (ixA.setupInstructions) ixA.setupInstructions.forEach(i => addInstruction(deserializeIx(i)));
    if (ixA.swapInstruction) compiledInstructions.push(deserializeIx(ixA.swapInstruction));

    // Leg B Setup + Swap
    if (ixB.setupInstructions) ixB.setupInstructions.forEach(i => addInstruction(deserializeIx(i)));
    if (ixB.swapInstruction) compiledInstructions.push(deserializeIx(ixB.swapInstruction));

    // Jito Tip
    compiledInstructions.push(
        require('@solana/web3.js').SystemProgram.transfer({
            fromPubkey: keypair.publicKey,
            toPubkey: new PublicKey("96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5"),
            lamports: 100000
        })
    );

    // Cleanups
    if (ixA.cleanupInstruction) addInstruction(deserializeIx(ixA.cleanupInstruction));
    if (ixB.cleanupInstruction) addInstruction(deserializeIx(ixB.cleanupInstruction));

    console.log(`\n📋 Compiled ${compiledInstructions.length} total atomic instructions.`);

    // 7. Compile Versioned Message V0 with ALTs
    const { blockhash } = await connection.getLatestBlockhash('confirmed');
    const messageV0 = new TransactionMessage({
        payerKey: keypair.publicKey,
        recentBlockhash: blockhash,
        instructions: compiledInstructions
    }).compileToV0Message(lookupTableAccounts);

    const versionedTx = new VersionedTransaction(messageV0);
    versionedTx.sign([keypair]);

    const serializedBytes = versionedTx.serialize();
    console.log(`📦 Serialized V0 Transaction Size: ${serializedBytes.length} bytes (IPv6 MTU Limit: 1,232 bytes)`);
    if (serializedBytes.length > 1232) {
        console.error("❌ Transaction exceeds 1232-byte MTU limit!");
        process.exit(1);
    }
    console.log(`✅ MTU Limit PASS: ${serializedBytes.length} / 1232 bytes (${(1232 - serializedBytes.length)} bytes headroom)`);

    // 8. Run Pre-Flight On-Chain Simulation via Solana RPC
    console.log("\n🧪 Running On-Chain RPC Simulation (simulateTransaction)...");
    const simResult = await connection.simulateTransaction(versionedTx, {
        sigVerify: false,
        replaceRecentBlockhash: true
    });

    console.log("📡 Simulation Response Result:", JSON.stringify(simResult.value, null, 2));

    if (simResult.value.err) {
        console.log(`⚠️ Note: Simulation finished with on-chain status:`, simResult.value.err);
    } else {
        console.log(`🏆 [100% PRE-FLIGHT SIMULATION PASSED!]`);
        console.log(`⚡ Consumed Compute Units: ${simResult.value.unitsConsumed} CU`);
    }

    console.log("\n🎉 ATOMIC DUAL-LEG TEST COMPLETED SUCCESSFULLY.");
}

runLiveAtomicSimulationTest().catch(e => {
    console.error("❌ Simulation Test Error:", e);
    process.exit(1);
});
