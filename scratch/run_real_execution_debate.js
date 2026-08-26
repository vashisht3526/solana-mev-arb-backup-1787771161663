const fs = require('fs');
const path = require('path');
const https = require('https');

const MANTLE_API_URL = 'https://bedrock-mantle.us-east-1.api.aws/v1/chat/completions';
const MANTLE_BEARER_TOKEN = 'ABSKTWFudGxlQXBpS2V5LW95em9rNWttLWF0LTQwNDY4ODE5NTY3NzpEc0kwS3hxaGdrMm9hOHV1akM2eUlmOWdLRGJOV1Q0ZnpSckFzajl1NTB1OURBSXdCeGFiT0VpRGNGOD0=';

function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

async function queryModelWithRetry(modelName, systemPrompt, userPrompt, retries = 3) {
    for (let attempt = 1; attempt <= retries; attempt++) {
        try {
            return await new Promise((resolve, reject) => {
                const postData = JSON.stringify({
                    model: modelName,
                    messages: [
                        { role: "system", content: systemPrompt },
                        { role: "user", content: userPrompt }
                    ],
                    temperature: 0.1,
                    max_tokens: 4096
                });

                const url = new URL(MANTLE_API_URL);
                const req = https.request({
                    hostname: url.hostname,
                    port: 443,
                    path: url.pathname,
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'Authorization': `Bearer ${MANTLE_BEARER_TOKEN}`,
                        'Content-Length': Buffer.byteLength(postData)
                    },
                    timeout: 180000
                }, (res) => {
                    let data = '';
                    res.on('data', c => data += c);
                    res.on('end', () => {
                        try {
                            const json = JSON.parse(data);
                            if (json.choices && json.choices.length > 0) {
                                resolve(json.choices[0].message.content);
                            } else {
                                reject(new Error("API returned error or empty response: " + data));
                            }
                        } catch (e) {
                            reject(new Error("JSON parse error: " + data.substring(0, 100)));
                        }
                    });
                });

                req.on('error', reject);
                req.on('timeout', () => {
                    req.destroy();
                    reject(new Error("Request timed out"));
                });
                req.write(postData);
                req.end();
            });
        } catch (err) {
            console.warn(`⚠️ [Attempt ${attempt}/${retries}] Failed for ${modelName}: ${err.message}`);
            if (attempt === retries) throw err;
            await sleep(3000 * attempt);
        }
    }
}

async function runRealExecutionDebate() {
    console.log("===============================================================================");
    console.log("⚔️ COUNCIL DEBATE: HOW TO EXECUTE REAL ON-CHAIN SWAPS ON SOLANA MAINNET");
    console.log("   Auditors: GLM-5 | Kimi k2.5 | Qwen 3 Coder 480B");
    console.log("===============================================================================\n");

    const prompt = `
================================================================================
CRITICAL SOLANA ON-CHAIN MEV EXECUTION PROBLEM
================================================================================

The user has:
- Armed keypair: AA1ihbehbPrMW4L9jWtdxJfkH76Up5F2azbeepQ1wixs
- Capital: 0.41 SOL ($38.09 USD) on Solana Mainnet
- Target: Execute real arbitrage swaps on Raydium / PumpSwap / Meteora using 0.05 SOL per trade with Jito tips.

CURRENT STATUS:
In 'src/jito_builder.rs', the bot generates a dummy placeholder string:
'tx_bundle_v0_bundle_...'
When Jito receives this string, it rejects it because it is not a signed binary Solana transaction (VersionedTransaction).
Because the transaction is rejected by Jito, no on-chain swap executes, and the wallet balance remains unchanged at 0.41 SOL.

QUESTION FOR THE COUNCIL:
What is the fastest, safest, and most reliable production-grade method in Rust / Node.js to construct, sign, and broadcast REAL executable Solana swap transactions on mainnet?
Option A: Use PumpPortal Lightning Local Trade API ('https://pumpportal.fun/api/trade-local') / Jupiter Ultra-Fast Swap API to get exact compiled transaction bytes for Raydium/Pump/Meteora, sign with the ed25519 Keypair, attach Jito tip, and broadcast.
Option B: Manually serialize low-level binary instruction layouts for Raydium AMM v4, Raydium CPMM, Pump.fun bonding curve, and Meteora DLMM in Rust.

Provide the exact technical recommendation and Rust implementation code.
`;

    // 1. GLM-5
    console.log("🤖 [1/3] Querying GLM-5...");
    const glm5 = await queryModelWithRetry(
        "zai.glm-5",
        "You are GLM-5, Principal Solana Security Auditor.",
        prompt
    );
    fs.writeFileSync(path.join(__dirname, 'real_swap_glm5.md'), glm5);
    console.log("✅ GLM-5 delivered.\n");

    // 2. Kimi k2.5
    console.log("🤖 [2/3] Querying Kimi k2.5...");
    const kimi = await queryModelWithRetry(
        "moonshotai.kimi-k2.5",
        "You are Kimi k2.5, Chief HFT Protocol Engineer.",
        `${prompt}\n\n=== GLM-5 RECOMMENDATION ===\n${glm5}`
    );
    fs.writeFileSync(path.join(__dirname, 'real_swap_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 delivered.\n");

    // 3. Qwen 3 Coder 480B
    console.log("🤖 [3/3] Querying Qwen 3 Coder 480B...");
    const qwen = await queryModelWithRetry(
        "qwen.qwen3-coder-480b-a35b-instruct",
        "You are Qwen 3 Coder 480B, Chief Systems Architect.",
        `${prompt}\n\n=== GLM-5 RECOMMENDATION ===\n${glm5}\n\n=== KIMI K2.5 RECOMMENDATION ===\n${kimi}`
    );
    fs.writeFileSync(path.join(__dirname, 'real_swap_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B synthesized.\n");

    console.log("🏆 ALL 3 MODELS HAVE CONCLUDED THE REAL ON-CHAIN SWAP DEBATE!");
}

runRealExecutionDebate().catch(console.error);
