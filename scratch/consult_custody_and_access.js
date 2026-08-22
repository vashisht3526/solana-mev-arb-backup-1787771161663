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
            await sleep(2500 * attempt);
        }
    }
}

async function runCustodyConsultation() {
    console.log("🏛️ CONSULTING COUNCIL ON USER NON-CUSTODIAL WALLET ACCESS & CONTROL\n");

    const promptPayload = `
USER CONCERN:
"there is no way i am gonna put my money into, when i have no access of that wallet give me another way consult again"

CONTEXT:
The user wants 100% full custodial control of the trading wallet from their own Phantom app at all times, with the ability to see the balance, view transactions, and withdraw funds back to their main wallet whenever they choose.

PROPOSED NON-CUSTODIAL SOLUTIONS:

1. DUAL-ACCESS IMPORT (Import Server Keypair into Phantom):
   - Provide the Base58 private key of the server-generated keypair directly to the user.
   - The user imports it into Phantom via "Add/Connect Wallet" -> "Import Private Key".
   - The user now has 100% full custody of the exact same wallet inside their Phantom app: they can see the live balance, view every swap transaction in real time, and click "Send/Withdraw" to empty the wallet at any second.

2. ONE-CLICK SWEEP / EMERGENCY DRAIN BUTTON ON DASHBOARD:
   - User configures their personal cold wallet address as "Designated Recovery Address".
   - A single button on the dashboard ("⚡ Emergency Sweep to My Personal Wallet") instantly transfers 100% of the SOL balance back to the user's primary wallet.

3. DEDICATED EXTENSION BURNER DERIVATION:
   - User creates an isolated burner account in Phantom/Solflare and connects it.

Please evaluate these options and provide a clear, empathetic, and ultra-secure roadmap that guarantees the user has 100% custody and control over their funds.
`;

    // 1. GLM-5
    console.log("🤖 [1/3] Consulting GLM-5...");
    const glm5 = await queryModelWithRetry("zai.glm-5", "You are GLM-5, Senior MEV Risk Officer. Provide a non-custodial wallet access framework for the user.", promptPayload);
    fs.writeFileSync(path.join(__dirname, 'custody_audit_glm5.md'), glm5);
    console.log("✅ GLM-5 recommendations delivered.\n");

    // 2. Kimi k2.5
    console.log("🤖 [2/3] Consulting Kimi k2.5...");
    const kimi = await queryModelWithRetry("moonshotai.kimi-k2.5", "You are Kimi k2.5, Chief Systems Engineer. Provide the exact non-custodial setup steps and security guarantees.", `${promptPayload}\n\n=== GLM-5 AUDIT ===\n${glm5}`);
    fs.writeFileSync(path.join(__dirname, 'custody_audit_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 recommendations delivered.\n");

    // 3. Qwen 3 Coder 480B
    console.log("🤖 [3/3] Consulting Qwen 3 Coder 480B for Consensus...");
    const qwen = await queryModelWithRetry("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the final non-custodial wallet architecture giving the user 100% control in Phantom.", `${promptPayload}\n\n=== GLM-5 AUDIT ===\n${glm5}\n\n=== KIMI K2.5 AUDIT ===\n${kimi}`);
    fs.writeFileSync(path.join(__dirname, 'custody_audit_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B consensus synthesized.\n");
}

runCustodyConsultation().catch(console.error);
