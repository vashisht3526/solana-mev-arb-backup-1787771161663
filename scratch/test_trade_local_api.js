const https = require('https');

const postData = JSON.stringify({
    publicKey: "AA1ihbehbPrMW4L9jWtdxJfkH76Up5F2azbeepQ1wixs",
    action: "buy",
    mint: "DtR4D9FtVoTX2569gaL837ZgrB6wNjj6tkmnX9Rdk9B2",
    denominatedInSol: "true",
    amount: 0.002,
    slippage: 10,
    priorityFee: 0.0001,
    pool: "auto"
});

console.log("Sending payload:", postData);

const req = https.request('https://pumpportal.fun/api/trade-local', {
    method: 'POST',
    headers: {
        'Content-Type': 'application/json',
        'Content-Length': Buffer.byteLength(postData)
    }
}, (res) => {
    console.log("Status Code:", res.statusCode);
    const chunks = [];
    res.on('data', c => chunks.push(c));
    res.on('end', () => {
        const body = Buffer.concat(chunks);
        console.log("Body Length:", body.length);
        if (body.length < 200) {
            console.log("Response text:", body.toString());
        } else {
            console.log("Success! Binary VersionedTransaction received (bytes:", body.length, ")");
        }
    });
});
req.on('error', console.error);
req.write(postData);
req.end();
