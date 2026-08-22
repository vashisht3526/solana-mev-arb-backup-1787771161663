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

async function consultEphemeralAtaCode() {
    console.log("🛡️ RUNNING PRE-IMPLEMENTATION CODE CONSULTATION ON EPHEMERAL ATAs & 180k CU PROFILES\n");

    const promptPayload = `
We are implementing the Ephemeral In-Transaction ATA Lifecycle & Background Rent-Reclaim Engine for the Solana MEV Arbitrage system written in Rust.

REQUIREMENTS TO IMPLEMENT:
1. 'src/types.rs':
   - Update ComputeBudgetConfig to add EPHEMERAL_COMPLEX (180,000 CUs, 5,000 micro-lamports) for DLMM/CLMM + Create/Close ATA, and EPHEMERAL_STANDARD (125,000 CUs) for CPMM.
   - Add EphemeralAtaLifecycle tracking struct.

2. 'src/router.rs':
   - Update 'AtomicRouterEngine::select_compute_budget' to scale to 180,000 CUs when dealing with concentrated liquidity / DLMM + Ephemeral ATA.
   - Structure 'AtomicRouterEngine::build_atomic_bundle' to assemble:
     * Ix 0: SetComputeUnitLimit(180_000)
     * Ix 1: SetComputeUnitPrice(5_000)
     * Ix 2: CreateAssociatedTokenAccountIdempotent(payer=wallet, owner=wallet, mint=target_mint)
     * Ix 3: SwapLeg1(SOL -> Token into ATA)
     * Ix 4: SwapLeg2(Token in ATA -> SOL to wallet)
     * Ix 5: CloseAccount(account=ATA, destination=wallet, owner=wallet) - rent refund
     * Ix 6: RequireProfit(min_out_sol = input_sol + min_net_profit)

3. 'src/scanner.rs':
   - Add a lightweight background Rent Reclaim auditor task (runs every 10 minutes) that logs and initiates zero-balance ATA closures.

Please review the proposed code changes, check for edge cases, and output the clean, robust Rust implementations.
`;

    // 1. GLM-5
    console.log("🤖 [1/3] Consulting GLM-5...");
    const glm5 = await queryModelWithRetry("zai.glm-5", "You are GLM-5, Senior Rust Systems Engineer. Provide your code recommendations and exact Rust structs for the Ephemeral ATA upgrade.", promptPayload);
    fs.writeFileSync(path.join(__dirname, 'ephemeral_code_glm5.md'), glm5);
    console.log("✅ GLM-5 code recommendation delivered.\n");

    // 2. Kimi k2.5
    console.log("🤖 [2/3] Consulting Kimi k2.5...");
    const kimi = await queryModelWithRetry("moonshotai.kimi-k2.5", "You are Kimi k2.5, HFT Solana Systems Engineer. Refine the Ephemeral ATA implementation and ensure zero compilation or runtime bugs.", `${promptPayload}\n\n=== GLM-5 CODE REVIEW ===\n${glm5}`);
    fs.writeFileSync(path.join(__dirname, 'ephemeral_code_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 code recommendation delivered.\n");

    // 3. Qwen 3 Coder 480B
    console.log("🤖 [3/3] Consulting Qwen 3 Coder 480B...");
    const qwen = await queryModelWithRetry("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the final consensus Rust code for types.rs, router.rs, and scanner.rs.", `${promptPayload}\n\n=== GLM-5 CODE REVIEW ===\n${glm5}\n\n=== KIMI K2.5 CODE REVIEW ===\n${kimi}`);
    fs.writeFileSync(path.join(__dirname, 'ephemeral_code_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B consensus code synthesized.\n");
}

consultEphemeralAtaCode().catch(console.error);
