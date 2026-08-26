const fs = require('fs');
const path = require('path');
const https = require('https');

const MANTLE_API_URL = 'https://bedrock-mantle.us-east-1.api.aws/v1/chat/completions';
const MANTLE_BEARER_TOKEN = 'ABSKTWFudGxlQXBpS2V5LW95em9rNWttLWF0LTQwNDY4ODE5NTY3NzpEc0kwS3hxaGdrMm9hOHV1akM2eUlmOWdLRGJOV1Q0ZnpSckFzajl1NTB1OURBSXdCeGFiT0VpRGNGOD0=';

function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

async function queryModel(modelName, systemPrompt, userPrompt, retries = 2) {
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
                    timeout: 45000
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
                req.on('timeout', () => {
                    req.destroy();
                    reject(new Error("Request timed out after 45s"));
                });
                req.write(postData);
                req.end();
            });
        } catch (err) {
            console.warn(`⚠️ [Attempt ${attempt}/${retries}] Failed for ${modelName}: ${err.message}`);
            if (attempt === retries) throw err;
            await sleep(1500);
        }
    }
}

async function runCouncilDebateV30() {
    console.log("🏛️ [COUNCIL DEBATE] Convening 4-Model Council on Golden Micro-Alpha Calibration (V30)...");

    const systemPrompt = "You are a Principal Solana MEV & Rust Systems Architect serving on the 4-Model High Consensus Governance Council (GLM-5, Kimi k2.5, DeepSeek V3.2, Qwen 3 Coder 480B).";

    const userPrompt = `
SYSTEM ARCHITECTURAL PROPOSAL: VERSION 30 (GOLDEN MICRO-ALPHA CALIBRATION)

Context & Live Diagnostic Finding:
In Version 29, the live engine detected active cross-DEX spreads on Morty, POPCAT, PENGU, but aborted before dispatch due to:
1. Sizing vs Pool Slippage: A 0.150 SOL ($150M lamports) trade size in a $20k-$80k pool caused ~1.60% self-inflicted roundtrip price impact + 0.55% DEX fees, making a +1.20% spread return 0.1475 SOL (net -0.0025 SOL loss). The invariant shield properly aborted to protect funds.
2. Rigid $30k TVL Cap: Clamping maximum TVL to $30k filtered out 95% of active multi-DEX pools (e.g. Morty $429k, CARDS $160k, POPCAT $94k).

Proposal:
1. Nimble Trade Sizing (0.040 SOL / 40,000,000 lamports):
   - Slashes roundtrip price impact from ~1.60% to < 0.15%.
   - Turns +0.80% to +2.5% gross spreads into positive net SOL returns.
   - Keeps bankroll ($0.3981 SOL) safely partitioned across multiple parallel opportunities.

2. Sweet-Spot Liquidity Band ($1,500 – $2,500,000 TVL):
   - Re-admits 40+ active multi-DEX pools (Raydium CPMM vs Meteora DLMM vs Orca) where high retail trading volume creates continuous price dislocations.
   - At 0.040 SOL position size, trading in $50k–$2M pools creates virtually zero self-impact (< 0.10%).

3. Dynamic 40% Margin-Share Jito Tip (15,000 lamport floor):
   - Formula: tip_lamports = max(15_000, (gross_profit_lamports * 40) / 100).
   - Leaves 60% of gross profit as pure net yield while remaining highly competitive for block leader inclusion.

4. Non-Negotiable Capital Safety Invariant:
   - min_final_lamports > input_lamports + tip_lamports + network_gas.
   - Any sub-par route aborts with 0 on-chain loss.

Please evaluate:
1. Mathematical validity of dropping sizing to 0.040 SOL to eliminate price impact barrier.
2. Expansion of TVL band to $1.5k–$2.5M for optimal multi-DEX pool density.
3. 40% margin-share Jito auction efficiency.
4. Unconditional Verdict: Do you RATIFY and AUTHORIZE Version 30?
`;

    const models = [
        { id: "moonshotai.kimi-k2.5", name: "Kimi k2.5" },
        { id: "qwen.qwen3-coder-480b-a35b-instruct", name: "Qwen 3 Coder 480B" },
        { id: "zai.glm-5", name: "GLM-5" }
    ];

    await Promise.all(models.map(async (m) => {
        console.log("📡 Querying " + m.name + " (" + m.id + ")...");
        try {
            const resp = await queryModel(m.id, systemPrompt, userPrompt);
            fs.writeFileSync(path.join(__dirname, "council_v30_golden_calibration_" + m.name.replace(/[^a-z0-9]/gi, '_') + ".md"), resp);
            console.log("✅ [" + m.name + "] Verdict Received (" + resp.length + " chars).\n");
        } catch (e) {
            console.error("❌ [" + m.name + "] Failed: " + e.message);
        }
    }));

    console.log("🏛️ [COUNCIL DEBATE COMPLETE] All models consulted.");
}

runCouncilDebateV30();
