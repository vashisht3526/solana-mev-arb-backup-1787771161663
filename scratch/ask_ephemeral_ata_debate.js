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

async function runAtaStrategyDebate() {
    console.log("⚔️ STARTING COUNCIL DEBATE: EPHEMERAL IN-TRANSACTION ATAs & RENT RECOVERY\n");

    const ataContext = `
================================================================================
EVALUATION OF EPHEMERAL ATAs & AUTO-RECLAIM FOR SMALL-CAPITAL SOLANA MEV
================================================================================

THE PROBLEM:
Previously, the council noted that Associated Token Account (ATA) rent (0.00204 SOL per token) creates an existential drag on small capital ($35 - $100) if ATAs stay open.

THE PROPOSED SOLUTION:
1. SAME-TRANSACTION EPHEMERAL ATAs (Atomic Create-Swap-Close):
   - Ix 1: create_associated_token_account_idempotent(payer=wallet, owner=wallet, mint=target_token)
   - Ix 2: swap_leg_1(SOL -> Token into ATA)
   - Ix 3: swap_leg_2(Token from ATA -> SOL)
   - Ix 4: close_account(account=ATA, destination=wallet, owner=wallet)
   - Ix 5: assertion require!(final_sol >= initial_sol + min_profit)

2. BACKGROUND RENT-RECLAIM WORKER (Cron Loop):
   - Every 10 mins, queries get_token_accounts_by_owner for accounts with amount == 0.
   - Submits close_account batch to refund all rent back to the wallet.

QUESTIONS FOR THE COUNCIL:
1. Does the Same-Transaction Ephemeral ATA pattern completely eliminate the 0.0408 SOL ATA rent capital lockup, returning 0.00204 SOL in the exact same block?
2. What is the exact Compute Unit (CU) overhead of create_idempotent (~24k CUs) + close_account (~3k CUs), and does our upgraded 135,000 CU budget accommodate this cleanly?
3. With Ephemeral ATAs active + 135k CUs + 4-pillar risk shields, does this eliminate the #1 structural objection to small-capital ($35 - $100) live execution?
`;

    // 1. GLM-5
    console.log("🤖 [1/3] Querying GLM-5 on Ephemeral ATAs...");
    const glm5 = await queryModelWithRetry("zai.glm-5", "You are GLM-5, Solana Smart Contract & MEV Systems Auditor. Analyze Ephemeral ATAs and compute unit dynamics.", ataContext);
    fs.writeFileSync(path.join(__dirname, 'ata_debate_glm5.md'), glm5);
    console.log("✅ GLM-5 analysis complete.\n");

    // 2. Kimi k2.5
    console.log("🤖 [2/3] Querying Kimi k2.5 on CU Limits & Rent Lifecycle...");
    const kimi = await queryModelWithRetry("moonshotai.kimi-k2.5", "You are Kimi k2.5, HFT Solana Systems Auditor. Cross-examine GLM-5's findings on Ephemeral ATAs and gas consumption.", `${ataContext}\n\n=== GLM-5 ANALYSIS ===\n${glm5}`);
    fs.writeFileSync(path.join(__dirname, 'ata_debate_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 analysis complete.\n");

    // 3. Qwen 3 Coder 480B
    console.log("🤖 [3/3] Querying Qwen 3 Coder 480B for Consensus...");
    const qwen = await queryModelWithRetry("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the final unanimous verdict on Ephemeral ATAs.", `${ataContext}\n\n=== GLM-5 ANALYSIS ===\n${glm5}\n\n=== KIMI K2.5 ANALYSIS ===\n${kimi}`);
    fs.writeFileSync(path.join(__dirname, 'ata_debate_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B consensus complete.\n");
}

runAtaStrategyDebate().catch(console.error);
