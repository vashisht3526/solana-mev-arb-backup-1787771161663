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
                    temperature: 0.15,
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
                    timeout: 150000
                }, (res) => {
                    let data = '';
                    res.on('data', (chunk) => data += chunk);
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
            await sleep(2500 * attempt);
        }
    }
}

async function consultThreeModelsOnCode() {
    console.log("⚔️ CONSULTING GLM-5, KIMI K2.5, AND QWEN 3 CODER 480B ON EXACT CODE IMPLEMENTATIONS\n");

    const codeContext = `
================================================================================
EXACT RUST CODE IMPLEMENTATION CONSULTATION FOR 4 CONSENSUS FEATURES
================================================================================

We are implementing the unanimous consensus recommendations into our production Rust engine:
1. Enhanced Risk Shields in src/scanner.rs & src/types.rs:
   - pool_liquidity_usd >= 5,000.0 (skips ultra-shallow pools with extreme price impact)
   - freeze_authority_present filter (rejects honeypot blacklist tokens)
   - mint_authority_present filter (rejects inflation-attack tokens)
   - transfer_fee_tax filter (rejects tokens with buy/sell fees)
2. Slot-Boundary & Jitter Synchronization in src/scanner.rs:
   - Check time difference / slot jitter between Pool A and Pool B (reject opportunities if data skew > 1,500ms)
3. 135,000 Compute Unit Profile in src/router.rs & src/jito_builder.rs:
   - Upgrade hardcoded 110,000 CU to 135,000 CU limit to ensure multi-bin Meteora DLMM crossings never fail.
   - Maintain 5,000 micro-lamports CU price.
4. Clean 2-Leg Spatial Execution:
   - Confirm adherence to optimal Newton-Raphson 2-leg spatial arbitrage.

Please provide your formal code review and exact Rust implementation recommendations for these 4 components.
`;

    console.log("🤖 [1/3] Consulting GLM-5...");
    const glm5 = await queryModelWithRetry("zai.glm-5", "You are GLM-5, Senior Rust Systems Engineer. Review and validate the exact Rust implementation details for Solana MEV consensus features.", codeContext);
    fs.writeFileSync(path.join(__dirname, 'code_review_glm5.md'), glm5);
    console.log("✅ GLM-5 review complete.\n");

    console.log("🤖 [2/3] Consulting Kimi k2.5...");
    const kimi = await queryModelWithRetry("moonshotai.kimi-k2.5", "You are Kimi k2.5, Rust MEV & Distributed Systems Auditor. Audit and refine the Rust implementation details.", `${codeContext}\n\n=== GLM-5 SUGGESTIONS ===\n${glm5}`);
    fs.writeFileSync(path.join(__dirname, 'code_review_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 review complete.\n");

    console.log("🤖 [3/3] Consulting Qwen 3 Coder 480B...");
    const qwen = await queryModelWithRetry("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B, Chief Architect. Synthesize the final, verified Rust implementation that all 3 models approve.", `${codeContext}\n\n=== GLM-5 REVIEW ===\n${glm5}\n\n=== KIMI K2.5 REVIEW ===\n${kimi}`);
    fs.writeFileSync(path.join(__dirname, 'code_review_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B consensus review complete.\n");
}

consultThreeModelsOnCode().catch(console.error);
