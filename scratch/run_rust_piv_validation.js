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

async function runRustPIV() {
    console.log("===============================================================================");
    console.log("🏛️ POST-IMPLEMENTATION VALIDATION: 100% NATIVE RUST LIVE MEV ENGINE");
    console.log("   Auditors: GLM-5 | Kimi k2.5 | Qwen 3 Coder 480B");
    console.log("===============================================================================\n");

    const jitoBuilderRust = fs.readFileSync(path.join(__dirname, '..', 'src', 'jito_builder.rs'), 'utf-8');
    const liveEngineRust = fs.readFileSync(path.join(__dirname, '..', 'src', 'live_engine.rs'), 'utf-8');

    const dossier = `
================================================================================
POST-IMPLEMENTATION VALIDATION DOSSIER: 100% NATIVE RUST LIVE TRADING ENGINE
================================================================================

1. NATIVE RUST JITO & SWAP BUILDER: src/jito_builder.rs
\`\`\`rust
${jitoBuilderRust}
\`\`\`

2. NATIVE RUST LIVE ENGINE: src/live_engine.rs
\`\`\`rust
${liveEngineRust}
\`\`\`

================================================================================
VALIDATION CRITERIA:
================================================================================
1. Pure Rust Architecture: Confirm that the live execution engine is 100% native Rust, running in sub-millisecond compiled release binary on Frankfurt EC2 without Node.js runtime dependency for trade execution.
2. Zero Simulation Invariant: Confirm that trades are only logged if the physical on-chain balance fetched from Solana RPC actually increases.
3. 5-Pillar Capital Protection: Confirm strict 0.10 SOL reserve floor, 0.05 SOL position cap, and atomic mutex execution.
4. Issue your final POST-IMPLEMENTATION VALIDATION CERTIFICATE (APPROVED / REJECTED).
`;

    // 1. GLM-5
    console.log("🤖 [1/3] Querying GLM-5 for Rust PIV...");
    const glm5 = await queryModelWithRetry("zai.glm-5", "You are GLM-5, Principal Solana Security Auditor. Perform Post-Implementation Validation on the native Rust MEV codebase.", dossier);
    fs.writeFileSync(path.join(__dirname, 'rust_piv_glm5.md'), glm5);
    console.log("✅ GLM-5 PIV delivered.\n");

    // 2. Kimi k2.5
    console.log("🤖 [2/3] Querying Kimi k2.5 for Rust PIV...");
    const kimi = await queryModelWithRetry("moonshotai.kimi-k2.5", "You are Kimi k2.5, Chief HFT Protocol Engineer. Perform Post-Implementation Validation on the native Rust MEV codebase.", `${dossier}\n\n=== GLM-5 AUDIT ===\n${glm5}`);
    fs.writeFileSync(path.join(__dirname, 'rust_piv_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 PIV delivered.\n");

    // 3. Qwen 3 Coder 480B
    console.log("🤖 [3/3] Querying Qwen 3 Coder 480B for Final PIV Synthesis...");
    const qwen = await queryModelWithRetry("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the unanimous council consensus and issue the final Post-Implementation Validation Certificate.", `${dossier}\n\n=== GLM-5 AUDIT ===\n${glm5}\n\n=== KIMI AUDIT ===\n${kimi}`);
    fs.writeFileSync(path.join(__dirname, 'rust_piv_qwen_certificate.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B Final PIV Certificate synthesized.\n");

    console.log("🏆 ALL 3 MODELS HAVE CONCLUDED THE POST-IMPLEMENTATION VALIDATION!");
}

runRustPIV().catch(console.error);
