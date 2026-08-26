const https = require('https');

async function testJupiterSwapCompilation() {
    const inputMint = "So11111111111111111111111111111111111111112"; // SOL
    const outputMint = "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm"; // $WIF
    const amountLamports = 2000000; // 0.002 SOL

    console.log("🔍 [1/2] Getting Quote from api.jup.ag...");
    const quoteUrl = `https://api.jup.ag/swap/v1/quote?inputMint=${inputMint}&outputMint=${outputMint}&amount=${amountLamports}&slippageBps=50`;

    const quoteRes = await new Promise((resolve, reject) => {
        https.get(quoteUrl, { headers: { 'User-Agent': 'Mozilla/5.0' } }, (res) => {
            let d = '';
            res.on('data', c => d += c);
            res.on('end', () => resolve(JSON.parse(d)));
        }).on('error', reject);
    });

    console.log(`✅ Quote received! outAmount: ${quoteRes.outAmount}`);

    console.log("🔍 [2/2] Compiling Versioned Transaction via https://api.jup.ag/swap/v1/swap...");
    const swapPostData = JSON.stringify({
        quoteResponse: quoteRes,
        userPublicKey: "AA1ihbehbPrMW4L9jWtdxJfkH76Up5F2azbeepQ1wixs",
        wrapAndUnwrapSol: true,
        dynamicComputeUnitLimit: true,
        prioritizationFeeLamports: 100000
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
            res.on('end', () => resolve(JSON.parse(d)));
        });
        req.on('error', reject);
        req.write(swapPostData);
        req.end();
    });

    if (swapRes.swapTransaction) {
        console.log(`🏆 SUCCESS! Real compiled Solana VersionedTransaction Base64 received (length: ${swapRes.swapTransaction.length})!`);
        console.log(`   Transaction base64 preview: ${swapRes.swapTransaction.substring(0, 80)}...`);
    } else {
        console.error("❌ Swap compilation error:", swapRes);
    }
}

testJupiterSwapCompilation().catch(console.error);
