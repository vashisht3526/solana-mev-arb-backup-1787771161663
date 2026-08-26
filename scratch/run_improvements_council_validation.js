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

async function runCouncilValidation() {
    console.log("===============================================================================");
    console.log("🏛️ AI COUNCIL VALIDATION: PERFORMANCE UPGRADES & MATHEMATICAL INVARIANTS");
    console.log("   Council: GLM-5 | Kimi k2.5 | Qwen 3 Coder 480B");
    console.log("===============================================================================\n");

    const jitoBuilderRust = fs.readFileSync(path.join(__dirname, '..', 'src', 'jito_builder.rs'), 'utf-8');
    const liveEngineRust = fs.readFileSync(path.join(__dirname, '..', 'src', 'live_engine.rs'), 'utf-8');

    const validationPrompt = `
You are evaluating the mathematical integrity and execution safety of the following production Solana MEV Arbitrage codebase and proposed optimizations:

### CODEBASE UNDER REVIEW:
1. \`src/jito_builder.rs\`:
\`\`\`rust
${jitoBuilderRust}
\`\`\`

2. \`src/live_engine.rs\`:
\`\`\`rust
${liveEngineRust}
\`\`\`

### PROPOSED PERFORMANCE & EXECUTION UPGRADES:
1. **Multi-Gateway Fallback (Eliminating HTTP 429 Drops)**:
   - Primary: \`https://lite-api.jup.ag/swap/v1\` (Low-latency Frankfurt eu-central-1 endpoint with zero rate limits).
   - Secondary: \`https://quote-api.jup.ag/v6\` & \`https://api.jup.ag/swap/v1\` fallback.
2. **Adaptive Dynamic Position Sizing for Micro Pools**:
   - Instead of static 0.05 SOL, size dynamically: \`swap_size = min(max_position_sol, pool_reserves_sol * 0.02, 0.035 SOL)\`.
   - Prevents excessive price impact on Leg B (Token -> SOL) that turns positive spreads negative on thin liquidity pools.
3. **Parallel Quote Compilation**:
   - Concurrently prepare Leg A and Leg B quotes and instructions via non-blocking async calls.
4. **Strict Integer Lamport Mathematical Invariant**:
   - \`final_sol_lamports >= input_lamports + tip_lamports + network_fees + min_profit_lamports\`.
   - Pre-flight RPC simulation + on-chain single-tx rollback guarantee.

### REQUIRED EVALUATION:
1. Verify whether the mathematical invariants in Rust ensure 100% protection against negative PnL and directional token exposure.
2. Critique the proposed upgrades and identify any edge cases or failure modes.
3. Deliver your formal VALIDATION CERTIFICATE (APPROVED / REJECTED).
`;

    // 1. GLM-5
    console.log("🤖 [1/3] Querying GLM-5 for Mathematical & Security Audit...");
    const glm5 = await queryModelWithRetry(
        "zai.glm-5",
        "You are GLM-5, Principal Solana Security Auditor. Evaluate mathematical invariants and execution safety.",
        validationPrompt
    );
    fs.writeFileSync(path.join(__dirname, 'improvements_validation_glm5.md'), glm5);
    console.log("✅ GLM-5 Audit Delivered.\n");

    // 2. Kimi k2.5
    console.log("🤖 [2/3] Querying Kimi k2.5 for HFT Latency & Invariant Verification...");
    const kimi = await queryModelWithRetry(
        "moonshotai.kimi-k2.5",
        "You are Kimi k2.5, Chief HFT Protocol Engineer. Evaluate mathematical invariants, slippage, and latency optimization.",
        `${validationPrompt}\n\n=== GLM-5 AUDIT REPORT ===\n${glm5}`
    );
    fs.writeFileSync(path.join(__dirname, 'improvements_validation_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 Audit Delivered.\n");

    // 3. Qwen 3 Coder 480B
    console.log("🤖 [3/3] Querying Qwen 3 Coder 480B for Final Unanimous Synthesis & Certification...");
    const qwen = await queryModelWithRetry(
        "qwen.qwen3-coder-480b-a35b-instruct",
        "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the unanimous council consensus and issue the final Validation Certificate.",
        `${validationPrompt}\n\n=== GLM-5 AUDIT REPORT ===\n${glm5}\n\n=== KIMI K2.5 AUDIT REPORT ===\n${kimi}`
    );
    fs.writeFileSync(path.join(__dirname, 'improvements_validation_qwen_certificate.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B Final Certificate Synthesized.\n");

    console.log("🏆 ALL 3 MODELS HAVE CONCLUDED THE VALIDATION REVIEW!");
}

runCouncilValidation().catch(err => {
    console.error("❌ Council Validation failed:", err);
    process.exit(1);
});
