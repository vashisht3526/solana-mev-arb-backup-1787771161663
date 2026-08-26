const https = require('https');

async function testJupiter() {
    const inputMint = "So11111111111111111111111111111111111111112"; // SOL
    const outputMint = "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm"; // $WIF
    const amountLamports = 2000000; // 0.002 SOL

    console.log("🔍 [1/2] Fetching Jupiter Swap Quote for 0.002 SOL -> $WIF...");
    const quoteUrl = `https://quote-api.jup.ag/v6/quote?inputMint=${inputMint}&outputMint=${outputMint}&amount=${amountLamports}&slippageBps=50`;

    const quoteRes = await new Promise((resolve, reject) => {
        https.get(quoteUrl, { headers: { 'User-Agent': 'Mozilla/5.0' } }, (res) => {
            let d = '';
            res.on('data', c => d += c);
            res.on('end', () => resolve(JSON.parse(d)));
        }).on('error', reject);
    });

    console.log(`✅ Quote received! In: ${quoteRes.inAmount / 1e9} SOL -> Out: ${quoteRes.outAmount} tokens | Route: ${quoteRes.routePlan?.map(r => r.swapInfo?.label).join(' -> ')}`);

    console.log("🔍 [2/2] Compiling Versioned Transaction via Jupiter Swap API...");
    const swapPostData = JSON.stringify({
        quoteResponse: quoteRes,
        userPublicKey: "AA1ihbehbPrMW4L9jWtdxJfkH76Up5F2azbeepQ1wixs",
        wrapAndUnwrapSol: true,
        dynamicComputeUnitLimit: true,
        prioritizationFeeLamports: 100000
    });

    const swapRes = await new Promise((resolve, reject) => {
        const req = https.request('https://quote-api.jup.ag/v6/swap', {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Content-Length': Buffer.byteLength(swapPostData)
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
    } else {
        console.error("❌ Swap compilation error:", swapRes);
    }
}

testJupiter().catch(console.error);
