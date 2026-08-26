const fs = require('fs');
const path = require('path');
const https = require('https');

const MANTLE_API_URL = 'https://bedrock-mantle.us-east-1.api.aws/v1/chat/completions';
const MANTLE_BEARER_TOKEN = 'ABSKTWFudGxlQXBpS2V5LW95em9rNWttLWF0LTQwNDY4ODE5NTY3NzpEc0kwS3hxaGdrMm9hOHV1akM2eUlmOWdLRGJOV1Q0ZnpSckFzajl1NTB1OURBSXdCeGFiT0VpRGNGOD0=';

function queryModel(modelName, systemPrompt, userPrompt) {
    return new Promise((resolve, reject) => {
        const postData = JSON.stringify({
            model: modelName,
            messages: [
                { role: "system", content: systemPrompt },
                { role: "user", content: userPrompt }
            ],
            temperature: 0.2,
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
            res.on('data', c => data += c);
            res.on('end', () => {
                try {
                    const json = JSON.parse(data);
                    if (json.choices && json.choices.length > 0) {
                        resolve(json.choices[0].message.content);
                    } else {
                        reject(new Error("API returned error: " + data.substring(0, 100)));
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
}

async function main() {
    console.log("===============================================================================");
    console.log("🏛️ 4-MODEL COUNCIL: DATA PIPELINE FEASIBILITY & TIMING WINDOW CALIBRATION");
    console.log("   Council: GLM-5 | Kimi k2.5 | DeepSeek V3.2 | Qwen 3 Coder 480B");
    console.log("===============================================================================\n");

    const prompt = `
The Raydium-Native Migration Sniping Plan is being audited for data pipeline feasibility and timing calibration:

### 1. THE DATA PIPELINE CHALLENGE:
The plan requires rolling trade volume, sell velocity decay, and price updates on fresh Raydium pools in real time.
- Standard DexScreener REST has 1-2s caching delays (too slow).
- Enterprise Helius/QuickNode plans cost $50-$200/mo (out of user budget).
- QUESTION: Can we use standard zero-cost Solana WebSocket subscriptions from our Frankfurt instance?
  * Option A: \`accountSubscribe(RaydiumPoolPubkey, encoding: base64, commitment: processed)\` for slot-by-slot (400ms) reserve updates (R_sol, R_tok) directly from pool account bytes.
  * Option B: \`logsSubscribe(mentions: [poolPubkey], commitment: processed)\` for real-time swap logs.
  * Option C: Combined PumpPortal migration trigger + instant WebSocket \`accountSubscribe\` to the newly created Raydium pool.

### 2. TIMING & RATIO CALIBRATION:
- At T+15s post-migration, the Buy/Sell volume ratio is almost always 0.1-0.3 because initial bonding curve dumpers dominate.
- Requiring Buy/Sell Ratio > 1.2 at T+15s will result in 0 trades.
- PROPOSAL: Shift entry evaluation window to **T+30s to T+45s**, measuring volume and velocity over the [T+15s, T+45s] window where the panic dump has exhausted and organic buyers actually appear.

### 3. COUNTERFACTUAL SHADOW LOGGING:
- The shadow engine must record both **Accepted Trades** and **Rejected Migrations** (tracking what happened to rejected tokens over 10 minutes) so we can empirically tune filters during the 7-day shadow phase.

### COUNCIL MANDATE:
1. **Zero-Cost Data Pipeline Audit**: What is the most efficient, 100% free Rust architecture to ingest real-time Raydium pool swaps/reserves on Frankfurt EC2?
2. **Timing Window Calibration**: Validate the shift to T+30s / T+45s for Buy/Sell Ratio and Sell Velocity Decay.
3. **Counterfactual Analytics Design**: Define the data structure for logging rejected migrations to tune false-positive vs false-negative filter thresholds.
4. **Final Synthesized Implementation Plan**: Deliver the definitive updated blueprint.
`;

    console.log("🚀 Dispatching queries across GLM-5, Kimi k2.5, DeepSeek V3.2, and Qwen 3 Coder...");

    const [glmRes, kimiRes, deepseekRes, qwenRes] = await Promise.allSettled([
        queryModel("zai.glm-5", "You are GLM-5, Principal Solana Security & RPC Protocol Engineer. Audit the zero-cost WebSocket data pipeline (accountSubscribe vs logsSubscribe on Raydium pools) and counterfactual logging.", prompt),
        queryModel("moonshotai.kimi-k2.5", "You are Kimi k2.5, Chief HFT Protocol Engineer. Audit the timing window calibration (T+30s-T+45s), rolling trade buffers, and slot-by-slot account decoding in Rust.", prompt),
        queryModel("deepseek.v3.2", "You are DeepSeek V3.2, Quantitative Risk Officer. Audit the statistical distributions of meme coin panic troughs, buyer inflection points, and counterfactual shadow tuning curves.", prompt),
        queryModel("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize unanimous consensus across all 4 models and deliver the Final Complete Implementation Plan.", prompt)
    ]);

    if (glmRes.status === 'fulfilled') {
        fs.writeFileSync(path.join(__dirname, 'pipeline_council_glm5.md'), glmRes.value);
        console.log("✅ GLM-5 Delivered.");
    }
    if (kimiRes.status === 'fulfilled') {
        fs.writeFileSync(path.join(__dirname, 'pipeline_council_kimi.md'), kimiRes.value);
        console.log("✅ Kimi k2.5 Delivered.");
    }
    if (deepseekRes.status === 'fulfilled') {
        fs.writeFileSync(path.join(__dirname, 'pipeline_council_deepseek.md'), deepseekRes.value);
        console.log("✅ DeepSeek V3.2 Delivered.");
    }
    if (qwenRes.status === 'fulfilled') {
        fs.writeFileSync(path.join(__dirname, 'pipeline_council_qwen_master.md'), qwenRes.value);
        console.log("✅ Qwen 3 Coder Delivered.");
    }

    console.log("🏆 ALL 4 MODELS HAVE COMPLETED THE DATA & TIMING AUDIT!");
}

main().catch(console.error);
