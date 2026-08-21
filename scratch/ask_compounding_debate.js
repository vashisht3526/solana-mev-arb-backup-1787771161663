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
                    temperature: 0.25,
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
                    timeout: 120000
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
            await sleep(2000 * attempt);
        }
    }
}

async function runCompoundingDebate() {
    console.log("⚔️ STARTING HARD-HITTING TRIPLE-MODEL DEBATE: HIGH-FREQUENCY COMPOUNDING & 1/3RD LIVE REALITY\n");

    const contextPrompt = `
ACTUAL PRODUCTION TELEMETRY FROM FRANKFURT EC2 (3.8 HOURS CONTINUOUS MAINNET RUN):
1. 1.0 SOL Conservative Instance (0.12 SOL pos size, 0.00008 min profit, 0.40% spread):
   - Starting Capital: 1.0000 SOL ($90)
   - Evaluated Dislocation Events: 42,349 arbs checked across 62 active tokens
   - Opportunities Triggered: 1,392 trades (793 Wins / 599 Fails -> 57.0% win rate)
   - Total Realized Net PnL: +1.5980 SOL ($145.00 USD)
   - Ending Balance: 2.5980 SOL (+159.8% capital growth in 3.8 hours)
   - Average net profit per winning fill: ~+0.00202 SOL ($0.18)
   - Failure simulator: 43.0% failure rate injected (-0.000005 SOL gas fee deducted per failed attempt)

2. 10.0 SOL Growth Instance (0.50 SOL pos size, 0.00030 min profit, 0.20% spread):
   - Starting Capital: 10.0000 SOL ($900)
   - Opportunities Triggered: 1,054 trades (632 Wins / 422 Fails -> 60.0% win rate)
   - Total Realized Net PnL: +4.6291 SOL ($420.00 USD)
   - Ending Balance: 14.6291 SOL (+46.3% capital growth in 3.8 hours)
   - Average net profit per winning fill: ~+0.00732 SOL ($0.66)

THE USER'S HYPOTHESIS & CHALLENGE:
"Bro, you originally said it will generate profit in cents only for $100 capital. But look: in 3.8 hours, the high velocity compounded micro-cents into +1.59 SOL ($145) doubling the 1 SOL bankroll!
Even if live real trading only captures 1/3rd (33%) of this profit due to live competition, that is still ~$48 in 3.5 hours on $90 capital!
Why shouldn't I put 1 SOL of real money now if:
1. The on-chain CPI invariant 'require!(final >= in + profit)' guarantees 0 principal loss.
2. Even capturing 30-35% of this velocity is massively profitable.
3. The CU limit is optimized to 110k for fast block packing."

Please debate this with brutal quantitative honesty, address each other's points, and deliver a definitive verdict.
`;

    // --- ROUND 1: GLM-5 ---
    console.log("🤖 [ROUND 1: GLM-5] Analyzing High-Frequency Compounding Physics & 1/3rd Live Conversion...");
    const glm5System = "You are GLM-5, an elite quantitative Solana MEV researcher and systems engineer. Evaluate high-frequency compounding, shadow simulation vs live mempool execution gaps, and provide mathematical truth.";
    const glm5Response = await queryModelWithRetry("zai.glm-5", glm5System, contextPrompt);
    fs.writeFileSync(path.join(__dirname, 'compounding_debate_glm5.md'), glm5Response);
    console.log("✅ GLM-5 completed evaluation.\n");

    // --- ROUND 2: Kimi k2.5 ---
    console.log("🤖 [ROUND 2: Kimi k2.5] Critiquing GLM-5 & Analyzing Real-World Mainnet Friction...");
    const kimiSystem = "You are Kimi k2.5, a high-frequency trading quant and on-chain systems auditor. Critique GLM-5's assessment, test the 1/3rd profit capture hypothesis on live Solana mainnet, and expose all microsecond frictions.";
    const kimiPrompt = `${contextPrompt}\n\n=== GLM-5 ARGUMENT ===\n${glm5Response}\n\nCritique GLM-5's assessment and determine whether 1/3rd profit capture on live 1 SOL is realistic.`;
    const kimiResponse = await queryModelWithRetry("moonshotai.kimi-k2.5", kimiSystem, kimiPrompt);
    fs.writeFileSync(path.join(__dirname, 'compounding_debate_kimi.md'), kimiResponse);
    console.log("✅ Kimi k2.5 completed critique.\n");

    // --- ROUND 3: Qwen 3 Coder 480B ---
    console.log("🤖 [ROUND 3: Qwen 3 Coder 480B] Synthesizing Consensus & Delivering Final Live Deployment Verdict...");
    const qwenSystem = "You are Qwen 3 Coder 480B, Chief Quantitative Systems Architect. Synthesize GLM-5 and Kimi k2.5's debates into a definitive, mathematically grounded verdict on 1.0 SOL live deployment, 1/3rd profit capture, and the exact step-by-step transition roadmap.";
    const qwenPrompt = `${contextPrompt}\n\n=== GLM-5 ARGUMENT ===\n${glm5Response}\n\n=== KIMI K2.5 CRITIQUE ===\n${kimiResponse}\n\nProvide the final synthesis and clear directive for the user.`;
    const qwenResponse = await queryModelWithRetry("qwen.qwen3-coder-480b-a35b-instruct", qwenSystem, qwenPrompt);
    fs.writeFileSync(path.join(__dirname, 'compounding_debate_qwen_consensus.md'), qwenResponse);
    console.log("✅ Qwen 3 Coder 480B delivered the consensus verdict.\n");
}

runCompoundingDebate().catch(console.error);
