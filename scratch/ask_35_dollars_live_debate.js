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

async function runLiveDecisionDebate() {
    console.log("⚔️ STARTING RIGOROUS COUNCIL DEBATE: SHOULD USER DEPLOY $35 USD LIVE ON MAINNET NOW?\n");

    const liveTelemetryContext = `
================================================================================
COMPLETE CODE IMPLEMENTATION & REAL-TIME TELEMETRY FROM FRANKFURT EC2
================================================================================

1. CODE IMPLEMENTATION STATUS:
- Enhanced Risk Shields active: freeze_authority == Revoked, mint_authority == Revoked, transfer_fee <= 0 bps, pool_liquidity >= $5,000 USD.
- Temporal Validator active: max_time_skew_ms = 1,500ms, max_slot_variance = 4.
- Compute Budget: Dynamic 135,000 CU limit for DLMM/CLMM and 85,000 CU for CPMM with 5,000 micro-lamports price.
- 2-Leg Spatial Router Invariant: require!(final_sol >= input_sol + min_net_profit) with Newton-Raphson dx* solver.

2. PRODUCTION TELEMETRY (18+ Hours Continuous Run, 15,705 Total Evaluated Trades):
- $35 USD (0.38 SOL) Micro-Pilot Instance:
  * Current Post-Upgrade Run: 43 trades (23 Wins / 20 Fails -> 53.5% Win Rate)
  * Net PnL: +0.0513 SOL (~$4.60 USD) | Balance: 0.3800 -> 0.4313 SOL (+13.5% gain)
  * Recent real winning fills: CYBERLEEK (+0.0015 SOL), TOESCOIN (+0.0042 SOL).
- 1.0 SOL Instance:
  * Current Post-Upgrade Run: 47 trades (33 Wins / 14 Fails -> 70.2% Win Rate)
  * Net PnL: +0.0711 SOL (~$6.40 USD) | Balance: 1.0000 -> 1.0711 SOL (+7.1% gain)
- Cumulative History: 15,705 logged trades in ndjson format across all 3 engines.

3. THE CORE USER QUESTION:
"Should I put my $35 USD (~0.38 SOL) of real money live on mainnet right now?"

4. SPECIFIC QUANTITATIVE QUESTIONS FOR THE DEBATE:
Q1: Post-Upgrade Win Rate (53.5% on $35 instance, 70.2% on 1.0 SOL): Does the improved 53.5% win rate in shadow simulation change the math on the 0.26 SOL gas buffer, or does ATA rent (0.00204 SOL per token account) still create an existential capital drag on a $35 bankroll?
Q2: Live Real Mainnet Degradation: When switching from simulated execution to live private-key signing and Jito bundle broadcasting from Frankfurt EC2, what will the real-world win rate be (e.g. 35-42% vs 53.5%), and will 0.38 SOL survive 50 consecutive live attempts?
Q3: The Unanimous Verdict: Do you unanimously advise the user to:
    A) Deploy the $35 live now, OR
    B) Keep the $35 in pocket and wait until 1.0 - 2.0 SOL ($90 - $180) is available while letting the 3 shadow engines collect data for free?
    State your exact reasons clearly.
`;

    // 1. GLM-5
    console.log("🤖 [1/3] Querying GLM-5 for Quantitative Live Mainnet Verdict...");
    const glm5 = await queryModelWithRetry(
        "zai.glm-5",
        "You are GLM-5, Senior Solana MEV Quantitative Risk Officer. Analyze the updated 53.5% win rate data and provide an unvarnished, mathematically definitive verdict on deploying $35 live.",
        liveTelemetryContext
    );
    fs.writeFileSync(path.join(__dirname, 'live_35_debate_glm5.md'), glm5);
    console.log("✅ GLM-5 verdict delivered.\n");

    // 2. Kimi k2.5
    console.log("🤖 [2/3] Querying Kimi k2.5 for On-Chain Micro-Economics & ATA Audit...");
    const kimi = await queryModelWithRetry(
        "moonshotai.kimi-k2.5",
        "You are Kimi k2.5, HFT Solana MEV Auditor. Cross-examine GLM-5's numbers, evaluate the $35 capital base against ATA rent and priority fee variance, and state your stance.",
        `${liveTelemetryContext}\n\n=== GLM-5 VERDICT ===\n${glm5}`
    );
    fs.writeFileSync(path.join(__dirname, 'live_35_debate_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 verdict delivered.\n");

    // 3. Qwen 3 Coder 480B
    console.log("🤖 [3/3] Querying Qwen 3 Coder 480B for Unanimous Consensus Synthesis...");
    const qwen = await queryModelWithRetry(
        "qwen.qwen3-coder-480b-a35b-instruct",
        "You are Qwen 3 Coder 480B, Chief Quantitative Architect. Synthesize the final, single unanimous consensus verdict on whether the user should deploy $35 live right now.",
        `${liveTelemetryContext}\n\n=== GLM-5 VERDICT ===\n${glm5}\n\n=== KIMI K2.5 AUDIT ===\n${kimi}`
    );
    fs.writeFileSync(path.join(__dirname, 'live_35_debate_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B delivered the final consensus verdict.\n");
}

runLiveDecisionDebate().catch(console.error);
