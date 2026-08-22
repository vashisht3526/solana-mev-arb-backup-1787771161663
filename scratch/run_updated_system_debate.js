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

async function runUpdatedSystemDebate() {
    console.log("⚔️ STARTING COUNCIL DEBATE ON THE FULLY UPGRADED SYSTEM & LIVE METRICS\n");

    const payload = `
================================================================================
UPDATED SYSTEM ARCHITECTURE & LIVE PRODUCTION METRICS (FRANKFURT EC2)
================================================================================

1. UPGRADED ARCHITECTURAL CAPABILITIES NOW ACTIVE:
- Ephemeral In-Transaction ATAs: CreateIdempotent + Swap + Swap + CloseAccount in 1 atomic transaction ($0.00 net storage rent locked).
- Dynamic 180,000 Compute Budget (EPHEMERAL_COMPLEX @ 5,000 micro-lamports) for DLMM multi-bins + ATA lifecycle.
- Programmatic Risk Shields: Freeze authority revoked, mint authority revoked, transfer fee <= 5 bps, pool liquidity >= $5,000 USD.
- Temporal Validator: 1,500ms time skew / 4-slot jitter filter eliminates ghost spreads.
- 2-Leg Spatial Router: strict require!(final_sol >= input_sol + min_net_profit) + 25% Jito tip clamping [0.00005, 0.01000 SOL].
- Rent Reclaim Worker: 10-min periodic sweep to prevent stranded lamports.

2. LATEST TELEMETRY FROM FRANKFURT SERVER:
- 10.0 SOL Growth Engine:
  * 69 Trades | 46 Wins | 23 Fails -> 66.7% Win Rate
  * Net PnL: +0.3943 SOL ($35.50 USD) | Balance: 10.3943 SOL
- 1.0 SOL Conservative Engine:
  * 56 Trades | 39 Wins | 17 Fails -> 69.6% Win Rate
  * Net PnL: +0.1015 SOL ($9.15 USD) | Balance: 1.1015 SOL (+10.15% ROI)
- $35 USD (0.38 SOL) Micro-Pilot Engine:
  * 55 Trades | 35 Wins | 20 Fails -> 63.6% Win Rate (climbed from 42% -> 53.5% -> 63.6% after ATA + temporal upgrades)
  * Net PnL: +0.0839 SOL ($7.55 USD) | Balance: 0.4639 SOL (+22.1% ROI)

3. DEBATE QUESTIONS FOR THE THREE FRONTIER MODELS:
Q1: How does the new 63.6% shadow win rate on the micro instance and $0 net ATA rent lockup change the math for live deployment?
Q2: With the 1.0 SOL engine producing a steady 69.6% win rate (+0.1015 SOL PnL) and the micro engine at 63.6% win rate, what is your updated verdict on:
    - Putting in $35 USD (0.38 SOL)
    - Putting in $50 USD (0.55 SOL)
    - Putting in $100 USD (1.10 SOL)
Q3: What is the optimal, mathematically justified step-by-step roadmap for the user to go live safely?
`;

    // 1. GLM-5
    console.log("🤖 [1/3] Querying GLM-5 for Updated System Verdict...");
    const glm5 = await queryModelWithRetry("zai.glm-5", "You are GLM-5, Senior Solana MEV Quantitative Risk Officer. Debate the updated system with 63.6% micro win rate and Ephemeral ATAs.", payload);
    fs.writeFileSync(path.join(__dirname, 'debate_v3_glm5.md'), glm5);
    console.log("✅ GLM-5 verdict delivered.\n");

    // 2. Kimi k2.5
    console.log("🤖 [2/3] Querying Kimi k2.5 for HFT Math & Execution Risk...");
    const kimi = await queryModelWithRetry("moonshotai.kimi-k2.5", "You are Kimi k2.5, HFT Execution Auditor. Cross-examine the updated 63.6% win rate, 180k CUs, and Ephemeral ATAs.", `${payload}\n\n=== GLM-5 VERDICT ===\n${glm5}`);
    fs.writeFileSync(path.join(__dirname, 'debate_v3_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 verdict delivered.\n");

    // 3. Qwen 3 Coder 480B
    console.log("🤖 [3/3] Querying Qwen 3 Coder 480B for Final Council Consensus...");
    const qwen = await queryModelWithRetry("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the final unanimous verdict of the council on $35 vs $50 vs $100 live deployment.", `${payload}\n\n=== GLM-5 VERDICT ===\n${glm5}\n\n=== KIMI K2.5 VERDICT ===\n${kimi}`);
    fs.writeFileSync(path.join(__dirname, 'debate_v3_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B delivered the final consensus.\n");
}

runUpdatedSystemDebate().catch(console.error);
