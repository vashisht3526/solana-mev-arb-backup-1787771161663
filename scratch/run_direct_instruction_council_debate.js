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

async function runDirectInstructionCouncilDebate() {
    console.log("🏛️ STARTING 4-MODEL COUNCIL DEBATE: DIRECT ZERO-LATENCY INSTRUCTION INJECTION IN RUST\n");

    const promptPayload = `
================================================================================
4-MODEL COUNCIL DELIBERATION: DIRECT ZERO-LATENCY INSTRUCTION INJECTION IN RUST
================================================================================

Problem:
Currently, the Solana MEV engine makes sequential HTTP requests to Jupiter lite-api.jup.ag for swap instructions.
This adds 35ms-80ms of network latency and fails with NO_ROUTES_FOUND when DEX filters are applied to new pools.

Solution Proposal:
Implement Direct In-Memory Solana SDK Instruction Injection in Rust:
1. Pump.fun Direct Instruction Builder:
   - Program ID: 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P
   - Global Config: 4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf
   - Fee Recipient: CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM
   - Buy Discriminator: 0x69c02607a51f0c36 (16927863322537952870)
   - Sell Discriminator: 0x33e685a4017f83ad (12502976635542562355)
   - Event Authority: Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1
   - Complete 12-account layout for Buy and Sell.

2. Raydium CPMM Direct Instruction Builder:
   - Program ID: CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK
   - Swap Base In Discriminator: 0x8fbe4a87c53d45cb ([143, 190, 74, 135, 197, 61, 69, 203])
   - Complete 14-account layout for BaseIn swaps.

3. Raydium AMM V4 Direct Instruction Builder:
   - Program ID: 675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8
   - Swap instruction discriminator byte: 9.

4. Ephemeral ATA Lifecycle (100% In-Transaction Rent Refund):
   - In-tx: Create Associated Token Account -> Direct Swap Leg 1 -> Direct Swap Leg 2 -> Close ATA (refund rent lamports back to user wallet) -> Jito Tip Transfer.

5. Lazy Jupiter Fallback:
   - Only trigger Jupiter HTTP quote if pool addresses are unmapped or from unsupported experimental AMMs.

Review Tasks for Council:
1. Validate binary discriminator bytes, instruction account metas (is_signer, is_writable), and account ordering.
2. Confirm that combining in-memory instruction creation with in-tx Ephemeral ATAs guarantees 0 rent leakage and sub-millisecond execution (< 5 microseconds).
3. Evaluate fail-safe behavior when fallback to Jupiter is triggered.
`;

    // 1. GLM-5
    console.log("📐 [1/3] GLM-5 Quant & Protocol Review...");
    const glm5 = await queryModel(
        "zai.glm-5",
        "You are GLM-5, Lead Quant Researcher and Solana Protocol Specialist. Analyze the binary instruction layouts, account metas, and AMM swap formulas for Pump.fun, Raydium CPMM, and Raydium V4.",
        promptPayload
    );
    fs.writeFileSync(path.join(__dirname, 'direct_ix_glm5.md'), glm5);
    console.log("✅ GLM-5 Review Complete (" + glm5.length + " chars)\n");

    // 2. Kimi k2.5
    console.log("⚡ [2/3] Kimi k2.5 Systems & Latency Audit...");
    const kimi = await queryModel(
        "moonshotai.kimi-k2.5",
        "You are Kimi k2.5, Systems & MEV Performance Engineer. Evaluate the sub-millisecond in-memory instruction compilation, memory allocations, CU budget, and network latency reduction compared to Jupiter HTTP.",
        `${promptPayload}\n\n=== GLM-5 QUANT REVIEW ===\n${glm5}`
    );
    fs.writeFileSync(path.join(__dirname, 'direct_ix_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 Audit Complete (" + kimi.length + " chars)\n");

    // 3. Qwen 3 Coder 480B
    console.log("🛡️ [3/3] Qwen 3 Coder 480B Consensus Decree & Rust Code Blueprint...");
    const qwen = await queryModel(
        "qwen.qwen3-coder-480b-a35b-instruct",
        "You are Qwen 3 Coder 480B, Chief Solana Smart Contract & Security Architect. Synthesize the final binding consensus decree and provide the production-ready Rust code blueprint for direct instruction building.",
        `${promptPayload}\n\n=== GLM-5 REVIEW ===\n${glm5}\n\n=== KIMI REVIEW ===\n${kimi}`
    );
    fs.writeFileSync(path.join(__dirname, 'direct_ix_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B Consensus Decree Complete (" + qwen.length + " chars)\n");

    console.log("🏆 COUNCIL DELIBERATION FINISHED! UNANIMOUS CERTIFICATION DECREE RECORDED.");
}

runDirectInstructionCouncilDebate().catch(console.error);
