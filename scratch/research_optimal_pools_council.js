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

async function runOptimalPoolResearch() {
    console.log("🏛️ STARTING 4-MODEL COUNCIL RESEARCH: OPTIMAL LIQUIDITY TIERS & LOW-COMPETITION NICHES\n");

    const promptPayload = `
================================================================================
QUANTITATIVE RESEARCH: OPTIMAL POOL TIERS FOR MICRO-SIZE (0.015-0.035 SOL) MEV ARBITRAGE
================================================================================

Context & Problem:
Our live Solana engine is executing in AWS Frankfurt (< 4.5ms total pipeline) with 0.3981 SOL bankroll and 0.020 SOL ($1.90) trade sizing.
In micro-cap pools (< $25k TVL), a nominal +3.95% spread is wiped out by 1.5% price impact per leg + 0.50% LP fees, causing the on-chain invariant guard to reject the trade.
In mega-cap pools (> $500k TVL), institutional HFTs (Jump, Wintermute) with co-located validator nodes dominate sub-millisecond slot zero.

Research Questions for Council:
1. What is the mathematical "Goldilocks Zone" of Pool TVL ($TVL_{min}$ and $TVL_{max}$) where:
   - Price impact for a 0.020 SOL swap is negligible (< 0.10% per leg).
   - LP fees (0.25% - 0.50%) can be cleanly cleared by realistic retail-driven price disparities (+1.2% to +3.0%).
   - Institutional whales cannot participate because their minimum viable size (5 - 50 SOL) causes 5-15% price impact in these pools.

2. Which specific DEX pool mechanics on Solana create the highest frequency of un-arbitraged retail mispricings:
   - Raydium CPMM vs Meteora DLMM vs Orca Concentrated Liquidity?
   - Newly graduated Raydium Launchlab pools vs PumpSwap residual liquidity?

3. How should our scanner dynamic sizing function calculate the optimal capital allocation $S^*$ per pool to guarantee positive net yield on every trigger?
`;

    // 1. GLM-5
    console.log("📐 [1/3] GLM-5 Quantitative Modeling & Pricing Research...");
    const glm5 = await queryModel("zai.glm-5", "You are GLM-5, Quantitative DeFi Researcher. Model the exact price impact curves, fee hurdle rates, and optimal liquidity sweet-spot for 0.02 SOL MEV arbitrage on Solana.", promptPayload);
    fs.writeFileSync(path.join(__dirname, 'pool_research_glm5.md'), glm5);
    console.log("✅ GLM-5 Research Complete (" + glm5.length + " chars)\n");

    // 2. Kimi k2.5
    console.log("⚡ [2/3] Kimi k2.5 Institutional Competition & Microstructure Analysis...");
    const kimi = await queryModel("moonshotai.kimi-k2.5", "You are Kimi k2.5, HFT Microstructure Specialist. Analyze institutional searcher blind spots, retail order flow velocity, and low-competition DEX niches on Solana.", `${promptPayload}\n\n=== GLM-5 QUANT ANALYSIS ===\n${glm5}`);
    fs.writeFileSync(path.join(__dirname, 'pool_research_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 Analysis Complete (" + kimi.length + " chars)\n");

    // 3. Qwen 3 Coder 480B
    console.log("🛡️ [3/3] Qwen 3 Coder 480B Synthesis & Implementation Blueprint...");
    const qwen = await queryModel("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B. Synthesize the findings into an actionable quantitative report with optimal pool criteria, dynamic sizing equations, and scanner filter recommendations.", `${promptPayload}\n\n=== GLM-5 QUANT ANALYSIS ===\n${glm5}\n\n=== KIMI MICROSTRUCTURE ANALYSIS ===\n${kimi}`);
    fs.writeFileSync(path.join(__dirname, 'pool_research_qwen_synthesis.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B Synthesis Complete (" + qwen.length + " chars)\n");

    console.log("🏆 RESEARCH DELIBERATION FINISHED!");
}

runOptimalPoolResearch().catch(console.error);
