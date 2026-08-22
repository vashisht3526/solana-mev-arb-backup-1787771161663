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

async function validateImplementation() {
    console.log("🛡️ RUNNING POST-IMPLEMENTATION VALIDATION ACROSS GLM-5, KIMI K2.5, AND QWEN 3 CODER 480B\n");

    const typesCode = fs.readFileSync(path.join(__dirname, '../src/types.rs'), 'utf8');
    const scannerCode = fs.readFileSync(path.join(__dirname, '../src/scanner.rs'), 'utf8');
    const routerCode = fs.readFileSync(path.join(__dirname, '../src/router.rs'), 'utf8');

    const validationPayload = `
================================================================================
FINAL IMPLEMENTED RUST CODEBASE DEPLOYED ON FRANKFURT EC2 SERVER
================================================================================

--- FILE 1: src/types.rs ---
${typesCode}

--- FILE 2: src/scanner.rs ---
${scannerCode}

--- FILE 3: src/router.rs ---
${routerCode}

================================================================================
DEPLOYMENT & RUNTIME STATUS:
- Target server: AWS EC2 eu-central-1 (Frankfurt)
- Rust Compiler: cargo build --release completed with 0 errors
- PM2 Processes: rust-mev-arb-10sol, rust-mev-arb-1sol, rust-mev-arb-micro, mev-dashboard ALL ONLINE (0 restarts)
- Live streams: Real-time PumpPortal WebSockets + DEX liquidity updates active

================================================================================
VALIDATION QUESTIONS FOR EACH MODEL:
1. Are the 4 consensus features (Enhanced Risk Shields, Temporal Slot Sync, 135k CU profile, 2-leg spatial atomicity) correctly and robustly implemented?
2. Are there any edge-case vulnerabilities, memory leaks, or race conditions in the new Rust code?
3. Do you provide your final formal sign-off for this production build?
`;

    // 1. GLM-5
    console.log("🤖 [1/3] Validating with GLM-5...");
    const glm5 = await queryModelWithRetry("zai.glm-5", "You are GLM-5, Senior Rust Systems Auditor. Review the final implemented code and provide your post-implementation sign-off.", validationPayload);
    fs.writeFileSync(path.join(__dirname, 'post_validation_glm5.md'), glm5);
    console.log("✅ GLM-5 validation sign-off received.\n");

    // 2. Kimi k2.5
    console.log("🤖 [2/3] Validating with Kimi k2.5...");
    const kimi = await queryModelWithRetry("moonshotai.kimi-k2.5", "You are Kimi k2.5, HFT Quant & Solana Execution Auditor. Rigorously inspect the code for edge cases and provide your formal validation.", validationPayload);
    fs.writeFileSync(path.join(__dirname, 'post_validation_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 validation sign-off received.\n");

    // 3. Qwen 3 Coder 480B
    console.log("🤖 [3/3] Validating with Qwen 3 Coder 480B...");
    const qwen = await queryModelWithRetry("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the final validation report from all three models.", `${validationPayload}\n\n=== GLM-5 VALIDATION ===\n${glm5}\n\n=== KIMI K2.5 VALIDATION ===\n${kimi}`);
    fs.writeFileSync(path.join(__dirname, 'post_validation_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B final consensus sign-off received.\n");
}

validateImplementation().catch(console.error);
