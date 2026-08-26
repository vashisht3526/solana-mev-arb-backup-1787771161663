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

async function runRound3() {
    console.log("===============================================================================");
    console.log("🤖 RUNNING QWEN 3 CODER 480B (CHIEF SYSTEMS ARCHITECT) SYNTHESIS");
    console.log("===============================================================================\n");

    const glm5 = fs.readFileSync(path.join(__dirname, 'grand_debate_round1_glm5.md'), 'utf-8');
    const kimi = fs.readFileSync(path.join(__dirname, 'grand_debate_round2_kimi.md'), 'utf-8');

    const prompt = `
You are Qwen 3 Coder 480B, Chief Systems Architect.
Synthesize the unanimous consensus code for the Solana MEV Arbitrage system across GLM-5's instruction architecture and Kimi k2.5's Jito status polling and blockhash pipeline.

KEY REQUIREMENTS:
1. Target Wallet: AA1ihbehbPrMW4L9jWtdxJfkH76Up5F2azbeepQ1wixs (0.41 SOL capital)
2. Signing Keypair: /home/ubuntu/.config/solana/mev_live.json
3. Ephemeral ATA PDA formula.
4. Jito sendBundle + getBundleStatuses polling loop with timeout.
5. Strict on-chain balance confirmation before logging.

Provide the complete, clean Rust implementation for:
- src/live_engine.rs
- src/jito_builder.rs
- Cargo.toml (dependencies needed)

=== ROUND 1: GLM-5 ARCHITECTURE ===
${glm5.substring(0, 7000)}

=== ROUND 2: KIMI K2.5 CRITIQUE & POLLING ===
${kimi.substring(0, 7000)}
`;

    const qwen = await queryModelWithRetry(
        "qwen.qwen3-coder-480b-a35b-instruct",
        "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the final, complete, production-ready Rust code for live on-chain Jito MEV execution.",
        prompt
    );

    fs.writeFileSync(path.join(__dirname, 'grand_debate_round3_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B Consensus synthesized successfully!\n");
}

runRound3().catch(console.error);
