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
                    temperature: 0.20,
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

async function runLiveDeploymentDebate() {
    console.log("⚔️ STARTING EXHAUSTIVE TRIPLE-MODEL DEBATE: $35 USD (0.38 SOL) LIVE MAINNET DEPLOYMENT\n");

    const comprehensiveContext = `
================================================================================
COMPLETE CODEBASE ARCHITECTURE & PRODUCTION TELEMETRY FOR THE DEBATE
================================================================================

1. ON-CHAIN EXECUTION LOGIC (Rust Engine):
- Atomic Router Invariant (src/router.rs):
  require!(final_sol >= input_sol + min_net_profit, "ArbitrageUnprofitable");
  -> 0 token principal lost on trade reverts. Reverts only burn base network fee + priority fee.
- Compute Unit Optimization:
  ComputeBudgetInstruction::set_compute_unit_limit(110_000);
  ComputeBudgetInstruction::set_compute_unit_price(5_000); // Micro-lamports per CU (~0.00010 - 0.00015 SOL total per tx)
- Jito Dynamic MEV Tip:
  25% of gross profit margin clamped in [0.00005, 0.01000 SOL]. Paid ONLY upon successful bundle execution.
- Mathematical Solver (src/math.rs):
  Newton-Raphson CPMM optimal trade sizing root solver calculating dx* in < 50 iterations with 1e-9 tolerance.
- Infrastructure:
  AWS EC2 in eu-central-1 (Frankfurt). Direct PumpPortal streaming WebSocket + DexScreener parallel scout.

2. PRODUCTION TELEMETRY FROM FRANKFURT EC2 (Continuous Live Mainnet Run):
- 10.0 SOL Growth Instance (0.50 SOL pos, 0.20% spread):
  * 1,054 trades (632 Wins / 422 Fails -> 60.0% Win Rate) | Net PnL: +4.6291 SOL (~$420 USD) | Balance: 14.6291 SOL
- 1.0 SOL Conservative Instance (0.12 SOL pos, 0.40% spread):
  * 1,392 trades (793 Wins / 599 Fails -> 57.0% Win Rate) | Net PnL: +1.5980 SOL (~$145 USD) | Balance: 2.5980 SOL
- $35 USD (0.38 SOL) Micro-Pilot Instance (0.12 SOL pos, 0.00050 SOL min profit floor, +0.80% spread floor):
  * 40 trades (17 Wins / 23 Fails -> 42.5% Win Rate) | Net PnL: +0.0581 SOL (~$5.14 USD) | Balance: 0.3800 -> 0.4381 SOL (+15.3% ROI)
  * Realized winning fills: PIXELCAT (+0.00790 SOL), TOESCOIN (+0.00230 SOL), BORG (+0.00170 SOL), JUP (+0.00130 SOL).

3. THE EXACT FINANCIAL SITUATION & PROPOSAL:
- Capital to deploy: Exactly $35 USD (~0.38 SOL).
- Position Size: Strictly 0.12 SOL single-trade lock (1 trade in flight at a time).
- Remaining Capital: 0.26 SOL reserved exclusively for Solana network fees and ATA rent.
- Spread Floor: +0.80% Net (filters out noisy low-spread competition).

4. THE THREE SPECIFIC QUESTIONS FOR THE COUNCIL:
Q1: On-Chain Friction Audit: With 0.38 SOL total capital, will Associated Token Account (ATA) rent (~0.00204 SOL per token) and priority fee reverts drain the 0.26 SOL gas reserve before winning trades compound it?
Q2: Live Real-World Success Rate: In live mainnet execution from Frankfurt EC2 (with ~80-100ms latency), what is the realistic expected live win rate (e.g. 25-35% vs 42.5% in shadow sim), and is 1 win (+0.0020 - 0.0079 SOL) mathematically sufficient to overcome live revert fees?
Q3: Definitive Go/No-Go Decision: Do you unanimously approve deploying this $35 (0.38 SOL) live to mainnet now under a strict single-trade lock, or what is the exact prerequisite checklist before doing so?
`;

    // --- ROUND 1: GLM-5 ---
    console.log("🤖 [ROUND 1: GLM-5] Performing Quantitative Audit & On-Chain Economics Analysis...");
    const glm5System = "You are GLM-5, an elite Solana MEV quantitative systems researcher. Provide a brutal, mathematically rigorous audit of deploying $35 (0.38 SOL) live on Solana mainnet. Calculate ATA rent burdens, gas friction, live win rates, and state your clear stance.";
    const glm5Response = await queryModelWithRetry("zai.glm-5", glm5System, comprehensiveContext);
    fs.writeFileSync(path.join(__dirname, 'verdict_debate_glm5.md'), glm5Response);
    console.log("✅ GLM-5 analysis complete.\n");

    // --- ROUND 2: Kimi k2.5 ---
    console.log("🤖 [ROUND 2: Kimi k2.5] Auditing GLM-5's Math & Testing 0.38 SOL Mainnet Viability...");
    const kimiSystem = "You are Kimi k2.5, a high-frequency trading quant and on-chain systems auditor. Rigorously cross-examine GLM-5's numbers, audit ATA rent lifecycle, microsecond execution dynamics, and evaluate the $35 live deployment.";
    const kimiPrompt = `${comprehensiveContext}\n\n=== GLM-5 AUDIT ===\n${glm5Response}\n\nCritique GLM-5's analysis, verify the math, and provide your findings.`;
    const kimiResponse = await queryModelWithRetry("moonshotai.kimi-k2.5", kimiSystem, kimiPrompt);
    fs.writeFileSync(path.join(__dirname, 'verdict_debate_kimi.md'), kimiResponse);
    console.log("✅ Kimi k2.5 critique complete.\n");

    // --- ROUND 3: Qwen 3 Coder 480B ---
    console.log("🤖 [ROUND 3: Qwen 3 Coder 480B] Synthesizing Unanimous Final Consensus Verdict...");
    const qwenSystem = "You are Qwen 3 Coder 480B, Chief Quantitative Systems Architect. Reconcile GLM-5 and Kimi k2.5 into a single, definitive, unified consensus conclusion that all three models agree on without ambiguity. Deliver the final operational verdict for the user.";
    const qwenPrompt = `${comprehensiveContext}\n\n=== GLM-5 AUDIT ===\n${glm5Response}\n\n=== KIMI K2.5 CRITIQUE ===\n${kimiResponse}\n\nDeliver the final unanimous consensus verdict, expected metrics, and exact step-by-step go/no-go directive.`;
    const qwenResponse = await queryModelWithRetry("qwen.qwen3-coder-480b-a35b-instruct", qwenSystem, qwenPrompt);
    fs.writeFileSync(path.join(__dirname, 'verdict_debate_qwen_consensus.md'), qwenResponse);
    console.log("✅ Qwen 3 Coder 480B delivered the final unanimous consensus.\n");
}

runLiveDeploymentDebate().catch(console.error);
