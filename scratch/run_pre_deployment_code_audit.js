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

async function runPreDeployCodeAudit() {
    console.log("===============================================================================");
    console.log("🏛️ INDEPENDENT 3-MODEL PRE-DEPLOYMENT CODE VALIDATION");
    console.log("   Auditors: GLM-5 | Kimi k2.5 | Qwen 3 Coder 480B");
    console.log("===============================================================================\n");

    const liveExecutorCode = fs.readFileSync(path.join(__dirname, '..', 'dashboard', 'live_executor.js'), 'utf-8');
    const serverCode = fs.readFileSync(path.join(__dirname, '..', 'dashboard', 'server.js'), 'utf-8');

    const dossier = `
================================================================================
FINAL CODE VALIDATION DOSSIER: REAL ON-CHAIN JUPITER + JITO + LOCAL SIGNER
================================================================================

1. DEPLOYED FILE: dashboard/live_executor.js
\`\`\`javascript
${liveExecutorCode}
\`\`\`

2. INTEGRATION FILE: dashboard/server.js
\`\`\`javascript
${serverCode}
\`\`\`

================================================================================
AUDIT CRITERIA:
================================================================================
1. Zero-Simulation Guarantee: Confirm that this code executes REAL on-chain swaps with the user's private key and records authentic Solscan transaction signatures.
2. Capital Risk Protection: Confirm that the 0.10 SOL reserve floor and 0.05 SOL position limit are strictly enforced before any quote or transaction is compiled.
3. Dual Broadcast Reliability: Confirm that Jito Frankfurt block engine + Solana RPC preflight broadcasting operates safely without draining unallocated SOL.
4. Issue your final PRE-DEPLOYMENT CERTIFICATION (APPROVED / REJECTED).
`;

    // 1. GLM-5
    console.log("🤖 [1/3] Querying GLM-5...");
    const glm5 = await queryModelWithRetry("zai.glm-5", "You are GLM-5, Principal Solana Security Auditor. Validate the finalized code before live deployment.", dossier);
    fs.writeFileSync(path.join(__dirname, 'pre_deploy_glm5.md'), glm5);
    console.log("✅ GLM-5 Validation delivered.\n");

    // 2. Kimi k2.5
    console.log("🤖 [2/3] Querying Kimi k2.5...");
    const kimi = await queryModelWithRetry("moonshotai.kimi-k2.5", "You are Kimi k2.5, Chief HFT Protocol Engineer. Validate the finalized execution code before live deployment.", `${dossier}\n\n=== GLM-5 AUDIT ===\n${glm5}`);
    fs.writeFileSync(path.join(__dirname, 'pre_deploy_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 Validation delivered.\n");

    // 3. Qwen 3 Coder 480B
    console.log("🤖 [3/3] Querying Qwen 3 Coder 480B...");
    const qwen = await queryModelWithRetry("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the unanimous council consensus and issue the final Pre-Deployment Certificate.", `${dossier}\n\n=== GLM-5 AUDIT ===\n${glm5}\n\n=== KIMI AUDIT ===\n${kimi}`);
    fs.writeFileSync(path.join(__dirname, 'pre_deploy_qwen_certificate.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B Final Certificate synthesized.\n");

    console.log("🏆 ALL 3 MODELS HAVE CONCLUDED THE PRE-DEPLOYMENT CODE AUDIT!");
}

runPreDeployCodeAudit().catch(console.error);
