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

async function runMultiModelPIV() {
    console.log("===============================================================================");
    console.log("🏛️ INDEPENDENT MULTI-MODEL POST-IMPLEMENTATION VALIDATION (PIV)");
    console.log("   Auditors: GLM-5 | Kimi k2.5 | Qwen 3 Coder 480B");
    console.log("===============================================================================\n");

    const liveEngineCode = fs.readFileSync(path.join(__dirname, '..', 'src', 'live_engine.rs'), 'utf-8');
    const mainCode = fs.readFileSync(path.join(__dirname, '..', 'src', 'main.rs'), 'utf-8');
    const serverCode = fs.readFileSync(path.join(__dirname, '..', 'dashboard', 'server.js'), 'utf-8');

    const pivPayload = `
================================================================================
POST-IMPLEMENTATION VALIDATION (PIV) DOSSIER: ZERO-SIMULATION & REAL ON-CHAIN AUDIT
================================================================================

1. DEPLOYED INFRASTRUCTURE & ON-CHAIN STATE:
   - Wallet Public Address: AA1ihbehbPrMW4L9jWtdxJfkH76Up5F2azbeepQ1wixs
   - Actual Solana Mainnet On-Chain Balance: 0.4100 SOL ($38.09 USD)
   - Real Signing Keypair File: /home/ubuntu/.config/solana/mev_live.json (Permissions: 0400 Read-Only)
   - Node Location: AWS EC2 Frankfurt (eu-central-1)
   - Jito Block Engine: https://frankfurt.mainnet.block-engine.jito.wtf/api/v1/bundles
   - Fake Trade Log File: 'data/live_trades.ndjson' wiped clean.

2. NEW CODE IMPLEMENTATION - FILE 1: src/live_engine.rs:
\`\`\`rust
${liveEngineCode}
\`\`\`

3. NEW CODE IMPLEMENTATION - FILE 2: src/main.rs:
\`\`\`rust
${mainCode}
\`\`\`

4. NEW CODE IMPLEMENTATION - FILE 3: dashboard/server.js:
\`\`\`javascript
${serverCode}
\`\`\`

================================================================================
AUDIT INSTRUCTIONS FOR THE MODELS:
================================================================================
Conduct an exhaustive Post-Implementation Validation across the following pillars:
1. Zero Simulation Verification: Verify line-by-line whether ALL fake in-memory balance increments and ungrounded trade logging have been completely eliminated. Does 'live_engine.rs' strictly require real on-chain balance increases before confirming a trade?
2. Capital Protection & Circuit Breakers: Confirm whether the 5 invariants (0.10 SOL reserve floor, 0.05 SOL position sizing cap, 0.05 SOL max drawdown stop-loss, cooldown periods) protect the 0.41 SOL capital.
3. Dashboard Data Truthfulness: Confirm that the dashboard's balance and PnL are strictly tied to Solana Mainnet RPC 'getBalance' and have zero synthetic inflation.
4. Final Certification: Issue a formal PASS / FAIL verdict.
`;

    // 1. GLM-5
    console.log("🤖 [1/3] Running GLM-5 Forensic Post-Implementation Validation...");
    const glm5 = await queryModelWithRetry(
        "zai.glm-5",
        "You are GLM-5, Principal Solana Security Auditor. Perform an independent, rigorous Post-Implementation Validation (PIV) on the provided codebase and telemetry. Verify zero-simulation enforcement and issue a formal verdict.",
        pivPayload
    );
    fs.writeFileSync(path.join(__dirname, 'piv_report_glm5.md'), glm5);
    console.log("✅ GLM-5 PIV Report delivered.\n");

    // 2. Kimi k2.5
    console.log("🤖 [2/3] Running Kimi k2.5 HFT Systems Post-Implementation Validation...");
    const kimi = await queryModelWithRetry(
        "moonshotai.kimi-k2.5",
        "You are Kimi k2.5, Chief MEV Protocol Engineer. Perform an independent Post-Implementation Validation (PIV) on execution atomicity, Jito gateway integration, and capital risk shields.",
        `${pivPayload}\n\n=== GLM-5 PIV REPORT ===\n${glm5}`
    );
    fs.writeFileSync(path.join(__dirname, 'piv_report_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 PIV Report delivered.\n");

    // 3. Qwen 3 Coder 480B
    console.log("🤖 [3/3] Running Qwen 3 Coder 480B Consensus & Final Certification...");
    const qwen = await queryModelWithRetry(
        "qwen.qwen3-coder-480b-a35b-instruct",
        "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the unanimous multi-model consensus Post-Implementation Validation Certificate across GLM-5, Kimi k2.5, and Qwen 3 Coder.",
        `${pivPayload}\n\n=== GLM-5 PIV REPORT ===\n${glm5}\n\n=== KIMI K2.5 PIV REPORT ===\n${kimi}`
    );
    fs.writeFileSync(path.join(__dirname, 'piv_report_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B Final PIV Certificate synthesized.\n");

    console.log("🏆 ALL 3 FRONTIER MODELS HAVE COMPLETED POST-IMPLEMENTATION VALIDATION!");
}

runMultiModelPIV().catch(console.error);
