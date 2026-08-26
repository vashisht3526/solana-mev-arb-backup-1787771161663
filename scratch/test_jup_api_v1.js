const https = require('https');

const inputMint = "So11111111111111111111111111111111111111112"; // SOL
const outputMint = "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm"; // $WIF
const amountLamports = 2000000; // 0.002 SOL

const quoteUrl = `https://api.jup.ag/swap/v1/quote?inputMint=${inputMint}&outputMint=${outputMint}&amount=${amountLamports}&slippageBps=50`;

https.get(quoteUrl, { headers: { 'User-Agent': 'Mozilla/5.0' } }, (res) => {
    let d = '';
    res.on('data', c => d += c);
    res.on('end', () => {
        console.log("Status:", res.statusCode);
        console.log("Body:", d.substring(0, 300));
    });
}).on('error', console.error);
