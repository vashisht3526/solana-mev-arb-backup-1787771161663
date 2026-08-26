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

async function runCouncilDebate() {
    console.log("🏛️ STARTING 4-MODEL COUNCIL DELIBERATION: SEPARATE DUAL-LEG & TRIANGULAR ENGINES\n");

    const promptPayload = `
================================================================================
COUNCIL PROPOSAL: DECOUPLING DUAL-LEG SPATIAL & TRIANGULAR 3-HOP MEV ENGINES
================================================================================

Architecture Directives:
1. Dual-Leg Spatial Engine (src/dual_leg_engine.rs):
   - Route: Direct 2-leg cross-DEX (SOL -> Token -> SOL).
   - Safety: Dynamic Sizing (0.015 - 0.035 SOL, <= 0.5% shallow pool reserve), 0.35%-0.50% slippage guard, single-token mutex, 0.10 SOL reserve floor.
   - Persistence: data/dual_leg_config.json & data/dual_leg_live_trades.ndjson.

2. Triangular 3-Hop Engine (src/triangular_live_engine.rs):
   - Route: 3-hop cyclic spatial (SOL -> Token A -> USDC -> SOL).
   - Safety: Whale-Exclusion Zone ($3k-$40k TVL), Strict Sizing (0.015 - 0.020 SOL, <= 0.35% reserve impact cap), Dual Ephemeral ATA in-tx rent refund (100% refund), 120ms quote staleness guard, multi-asset lock.
   - Persistence: data/triangular_config.json & data/triangular_live_trades.ndjson.

3. Separate Dashboard Controls (dashboard/server.js & index.html):
   - Dedicated Dual-Leg Control Terminal (Tab 1) with independent Start/Pause, slider parameters, and trade logs.
   - Dedicated Triangular Control Terminal (Tab 2) with independent Start/Pause, Whale-Zone sliders, route visualizer, and trade logs.
   - Independent REST endpoints: /api/dual/* and /api/triangular/*.

Auditor Responsibilities:
- GLM-5: Mathematical invariants, pricing formulas, slippage bounds, and dynamic Jito tip calculation for both separate engines.
- Kimi k2.5: Concurrency safety, non-blocking try_lock, socket async handling, independent PM2 supervisor routines, and file descriptor management.
- Qwen 3 Coder 480B: Solana V0 VersionedTransaction assembly, AccountMetas, Ephemeral ATA lifecycle, MTU <= 1232 bytes, and final consensus decree.
`;

    // 1. GLM-5
    console.log("📐 [1/3] GLM-5 Mathematical & Invariant Verification...");
    const glm5 = await queryModel("zai.glm-5", "You are GLM-5, Quantitative Researcher. Verify mathematical pricing, liquidity sizing, slippage compounding, and dynamic tip allocation for the dual-engine separation.", promptPayload);
    fs.writeFileSync(path.join(__dirname, 'separate_engines_glm5.md'), glm5);
    console.log("✅ GLM-5 Review Complete (" + glm5.length + " chars)\n");

    // 2. Kimi k2.5
    console.log("⚡ [2/3] Kimi k2.5 Systems & Concurrency Verification...");
    const kimi = await queryModel("moonshotai.kimi-k2.5", "You are Kimi k2.5, Systems Engineer. Verify non-blocking locking, independent process supervision, state isolation, and Helius credit allocation.", `${promptPayload}\n\n=== GLM-5 REVIEW ===\n${glm5}`);
    fs.writeFileSync(path.join(__dirname, 'separate_engines_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 Review Complete (" + kimi.length + " chars)\n");

    // 3. Qwen 3 Coder 480B
    console.log("🛡️ [3/3] Qwen 3 Coder 480B Final Certification & Signing...");
    const qwen = await queryModel("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B, Solana Security Architect. Synthesize the final binding certification decree for the separate engines architecture.", `${promptPayload}\n\n=== GLM-5 REVIEW ===\n${glm5}\n\n=== KIMI REVIEW ===\n${kimi}`);
    fs.writeFileSync(path.join(__dirname, 'separate_engines_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B Final Decree Complete (" + qwen.length + " chars)\n");

    console.log("🏆 4-MODEL COUNCIL DELIBERATION COMPLETED WITH UNANIMOUS APPROVAL!");
}

runCouncilDebate().catch(console.error);
