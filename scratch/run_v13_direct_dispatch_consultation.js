const fs = require('fs');
const path = require('path');
const https = require('https');

const MANTLE_API_URL = 'https://bedrock-mantle.us-east-1.api.aws/v1/chat/completions';
const MANTLE_BEARER_TOKEN = 'ABSKTWFudGxlQXBpS2V5LW95em9rNWttLWF0LTQwNDY4ODE5NTY3NzpEc0kwS3hxaGdrMm9hOHV1akM2eUlmOWdLRGJOV1Q0ZnpSckFzajl1NTB1OURBSXdCeGFiT0VpRGNGOD0=';

function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

async function queryModel(modelName, systemPrompt, userPrompt, retries = 3) {
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
                    res.on('data', (chunk) => data += chunk);
                    res.on('end', () => {
                        try {
                            const json = JSON.parse(data);
                            if (json.choices && json.choices.length > 0) {
                                resolve(json.choices[0].message.content);
                            } else {
                                reject(new Error("API error: " + data));
                            }
                        } catch (e) {
                            reject(e);
                        }
                    });
                });

                req.on('error', reject);
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

async function runV13Consultation() {
    console.log("🏛️ STARTING 4-MODEL COUNCIL CONSULTATION: V13 ZERO-DELAY DIRECT DISPATCH VS RPC PREFLIGHT\n");

    const promptPayload = `
================================================================================
4-MODEL COUNCIL CONSULTATION: SHOULD WE BYPASS RPC PRE-FLIGHT SIMULATION?
================================================================================

Current Operational Situation:
1. The Problem:
   - In our Rust MEV engine, after assembling binary instructions in < 2 microseconds, we currently call \`simulate_transaction_preflight\` via HTTP RPC before sending to Jito.
   - This HTTP roundtrip adds 40 - 80 ms latency.
   - On Solana meme coin pairs (Pump.fun, Meteora DLMM), price spreads are fleeting (10 - 20 ms).
   - Because RPC nodes lag behind validator tip by 1-2 slots, the simulation evaluates on stale state and frequently aborts viable trades locally, keeping the engine idle.

2. Competitive On-Chain Evidence:
   - On-chain forensics of high-profit MEV bot wallet MRiYA4oN3158fCV8evhuCofrDzbHyYvYnGZUDJvoCsa (Balance: 77.75 SOL) proves it executes 1-3 transactions/sec on the exact same pairs (Pump.fun ↔ Meteora DLMM) with ZERO RPC simulation delay.
   - It relies purely on on-chain slippage/invariant assertions (\`min_amount_out\` or custom contract error 'Custom: 0').

3. Safety Guarantees if We Bypass RPC Simulation:
   - A. Jito Bundle Invariant: Jito bundles are atomic. If the on-chain trade reverts or fails the minimum return check, the bundle drops and JITO TIPS ARE NOT DEDUCTED ($0.000000 tip loss).
   - B. Base Solana Gas: Even if an on-chain transaction reverts, Solana base transaction fee is ~0.000010 SOL ($0.001), while our trade size is 0.15 SOL with a 0.15 SOL reserve floor and 0.2481 SOL free cushion.
   - C. Exact In-Memory Slippage Guard: The compiled binary instruction contains the exact \`min_amount_out\` required to guarantee net profit.

Council Core Questions:
1. Should we eliminate the \`simulate_transaction_preflight\` RPC blocking check in dual_leg_engine.rs and triangular_live_engine.rs to achieve Zero-Delay Direct Dispatch (< 5ms total latency)? YES or NO?
2. What are the exact mathematical and engineering risks, and what guards must be in place?
3. Synthesize the definitive Council Recommendation Decree.
`;

    // 1. GLM-5
    console.log("📐 [1/3] GLM-5 Latency vs Simulation Audit...");
    const glm5 = await queryModel("zai.glm-5", "You are GLM-5. Audit the RPC pre-flight simulation bottleneck vs direct Jito bundle dispatch.", promptPayload);
    fs.writeFileSync(path.join(__dirname, 'v13_direct_glm5.md'), glm5);
    console.log("✅ GLM-5 Review Complete (" + glm5.length + " chars)\n");

    // 2. Kimi k2.5
    console.log("⚡ [2/3] Kimi k2.5 Jito Atomicity & Gas Risk Audit...");
    const kimi = await queryModel("moonshotai.kimi-k2.5", "You are Kimi k2.5. Audit Jito bundle atomicity, tip safety, and on-chain revert economics.", `${promptPayload}\n\n=== GLM-5 REVIEW ===\n${glm5}`);
    fs.writeFileSync(path.join(__dirname, 'v13_direct_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 Audit Complete (" + kimi.length + " chars)\n");

    // 3. Qwen 3 Coder 480B
    console.log("🛡️ [3/3] Qwen 3 Coder 480B Recommendation Decree...");
    const qwen = await queryModel("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B. Synthesize the binding Council Recommendation Decree on whether to bypass RPC simulation.", `${promptPayload}\n\n=== GLM-5 REVIEW ===\n${glm5}\n\n=== KIMI REVIEW ===\n${kimi}`);
    fs.writeFileSync(path.join(__dirname, 'v13_direct_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B Recommendation Complete (" + qwen.length + " chars)\n");

    console.log("🏆 V13 COUNCIL CONSULTATION FINISHED! VERDICT RECORDED.");
}

runV13Consultation().catch(console.error);
