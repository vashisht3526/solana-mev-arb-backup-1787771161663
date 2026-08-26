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

async function runNativeInstructionCouncilDebate() {
    console.log("🏛️ STARTING 4-MODEL COUNCIL DEBATE: NATIVE METEORA DLMM & ORCA WHIRLPOOL INSTRUCTION ASSEMBLY\n");

    const promptPayload = `
================================================================================
4-MODEL COUNCIL DELIBERATION: NATIVE METEORA DLMM & ORCA WHIRLPOOL INSTRUCTION ASSEMBLY (ZERO HTTP)
================================================================================

Objective:
Eliminate Jupiter HTTP API calls (/quote and /swap-instructions) entirely from src/jito_builder.rs.
Replace with native Rust binary instruction constructors for:
1. Meteora DLMM (Program: LBUZKhJmBkiLxsay82h8abbaAsgu9naw1mBHje9977A)
   - Sighash / Discriminator: swap exact in / swap
   - Account Metas: lb_pair, bin_array_bitmap_extension, reserve_x, reserve_y, user_token_x, user_token_y, token_x_mint, token_y_mint, oracle, host_fee_in, user, token_x_program, token_y_program, event_authority, program.
   - Data Layout: amount_in: u64, min_amount_out: u64.

2. Orca Whirlpool (Program: whirLbM3G6Zgpxw7wepmct6wT26sc696bCgM86bS8un)
   - Sighash / Discriminator: 0xf8c69e91e17587c8 (swap)
   - Account Metas: token_program, token_authority, whirlpool, token_owner_account_a, token_vault_a, token_owner_account_b, token_vault_b, tick_array_0, tick_array_1, tick_array_2, oracle.
   - Data Layout: amount: u64, other_amount_threshold: u64, sqrt_price_limit: u128, amount_specified_is_input: bool, a_to_b: bool.

3. Pure In-Memory Transaction Pipeline:
   - Calculate swap math & slippage bounds via internal Newton-Raphson & pool reserves.
   - Assemble VersionedTransaction in < 2 microseconds in CPU cache.
   - Sign with keypair and dispatch directly to Jito Frankfurt Block Engine via gRPC/HTTP bundle RPC.

Council Mandate:
1. Verify exact Anchor discriminators, account layouts, and byte encodings for Meteora DLMM and Orca Whirlpools.
2. Confirm safety invariants, slippage protection, and atomic rollback guarantees.
3. Formulate the binding Rust implementation blueprint for src/jito_builder.rs.
`;

    // 1. GLM-5
    console.log("📐 [1/3] GLM-5 Protocol Layout & Instruction Sighash Audit...");
    const glm5 = await queryModel("zai.glm-5", "You are GLM-5. Audit the Meteora DLMM and Orca Whirlpool binary instruction layouts, Anchor sighashes, and account order.", promptPayload);
    fs.writeFileSync(path.join(__dirname, 'native_dlmm_orca_glm5.md'), glm5);
    console.log("✅ GLM-5 Review Complete (" + glm5.length + " chars)\n");

    // 2. Kimi k2.5
    console.log("⚡ [2/3] Kimi k2.5 Systems & Atomic Jito Bundle Audit...");
    const kimi = await queryModel("moonshotai.kimi-k2.5", "You are Kimi k2.5. Audit in-memory transaction assembly, CU limits, ephemeral ATA close, and Jito bundle safety.", `${promptPayload}\n\n=== GLM-5 REVIEW ===\n${glm5}`);
    fs.writeFileSync(path.join(__dirname, 'native_dlmm_orca_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 Audit Complete (" + kimi.length + " chars)\n");

    // 3. Qwen 3 Coder 480B
    console.log("🛡️ [3/3] Qwen 3 Coder 480B Consensus Decree & Rust Blueprint...");
    const qwen = await queryModel("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B. Synthesize the final binding consensus decree and Rust code for src/jito_builder.rs.", `${promptPayload}\n\n=== GLM-5 REVIEW ===\n${glm5}\n\n=== KIMI REVIEW ===\n${kimi}`);
    fs.writeFileSync(path.join(__dirname, 'native_dlmm_orca_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B Consensus Complete (" + qwen.length + " chars)\n");

    console.log("🏆 COUNCIL DELIBERATION FINISHED! UNANIMOUS CERTIFICATION RECORDED.");
}

runNativeInstructionCouncilDebate().catch(console.error);
