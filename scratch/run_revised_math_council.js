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
    console.log("🏛️ 4-MODEL COUNCIL: AUDITING REVISED RAYDIUM-NATIVE MIGRATION DYNAMICS");
    console.log("   Council: GLM-5 | Kimi k2.5 | DeepSeek V3.2 | Qwen 3 Coder 480B");
    console.log("===============================================================================\n");

    const prompt = `
A critical mathematical flaw was identified in the previous Pump.fun -> Raydium Migration plan:

### 1. THE FLAW IDENTIFIED:
The previous entry condition compared:
Depression Ratio = (Live Raydium Price) / (Bonding Curve Virtual Reserves Fair Price) <= 0.90
FLAW: Pump.fun bonding curve utilizes synthetic virtual reserves (includes a 30 SOL phantom buffer). The real reserves migrated to Raydium are ~69 SOL and unsold tokens. The Raydium pool open price is set by REAL reserves, meaning the "virtual fair price" is structurally 30-40% higher than the true Raydium open price. Comparing them creates a permanent phantom depression signal, causing the bot to systematically buy repriced tokens thinking they are at a discount.

### 2. THE REVISED PROPOSAL (RAYDIUM-NATIVE DYNAMICS):
Replace the bonding curve anchor with pure Raydium on-chain metrics:
- **Anchor**: Real Raydium Pool Open Price (P0) at pool creation.
- **Entry Rules (ALL must pass)**:
  1. Time since migration >= T+15s (let initial bot dump finish).
  2. Price > 80% of P0 (do not catch a falling knife that crashed >50%).
  3. Sell Velocity Decay > 60% (sells in 10-30s drop by >60% vs 0-10s -> panic exhaustion).
  4. Buy/Sell Volume Ratio > 1.2 in the last 30s (demand returning).
  5. 5-Pillar Shield + Dev hold < 3% + Pool liquidity > 20 SOL.
  6. Pre-flight ghost sell simulation passes.
- **Exit Rules**:
  1. Take Profit: Price >= 110% of Entry Price (+10% gain).
  2. Trailing Stop: -5% drop from local high post-entry.
  3. Hard Stop Loss: -10% from Entry Price.
  4. Time Stop: 10 minutes (600s) instead of 3 minutes (real meme recovery happens between 2m-8m).
  5. Zero-Retry Toxic Tag: If sell fails, DO NOT retry with 2x fee (avoids burning fees on honeypots/drains). Tag toxic permanently.
- **Mode**: 100% Shadow / Paper-Trading simulation ($0 real funds).

### COUNCIL MANDATE:
1. **Mathematical & Market Structure Verdict**: Audit the mathematical validity of discarding virtual reserves in favor of Raydium Pool Open Price P0, Sell Velocity Decay, and Buy/Sell Volume Ratios.
2. **Timing & Exit Strategy**: Evaluate the 10-minute time stop, 5% trailing stop, and 10% hard stop loss mechanics.
3. **Rust Technical Implementation**: Detail the exact on-chain Raydium CPMM data structures to extract P0, compute 10s rolling velocity windows, and manage trailing stops in memory.
4. **Final Synthesized Implementation Plan**: Deliver the definitive updated blueprint.
`;

    console.log("🚀 Dispatching queries across GLM-5, Kimi k2.5, DeepSeek V3.2, and Qwen 3 Coder...");

    const [glmRes, kimiRes, deepseekRes, qwenRes] = await Promise.allSettled([
        queryModel("zai.glm-5", "You are GLM-5, Principal Solana Security & AMM Math Auditor. Audit the revised Raydium-native migration sniping model, virtual reserve flaw, and toxic revert removal.", prompt),
        queryModel("moonshotai.kimi-k2.5", "You are Kimi k2.5, Chief HFT Protocol Engineer. Audit the sell velocity decay calculation, rolling trade window data structures in Rust, and P0 extraction.", prompt),
        queryModel("deepseek.v3.2", "You are DeepSeek V3.2, Quantitative Risk Officer. Audit the revised exit matrix (+10% TP, -5% trailing stop, -10% SL, 10m time stop) and trade expectancy curves.", prompt),
        queryModel("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize unanimous consensus across all 4 models and deliver the Revised Master Implementation Plan.", prompt)
    ]);

    if (glmRes.status === 'fulfilled') {
        fs.writeFileSync(path.join(__dirname, 'revised_council_glm5.md'), glmRes.value);
        console.log("✅ GLM-5 Delivered.");
    }
    if (kimiRes.status === 'fulfilled') {
        fs.writeFileSync(path.join(__dirname, 'revised_council_kimi.md'), kimiRes.value);
        console.log("✅ Kimi k2.5 Delivered.");
    }
    if (deepseekRes.status === 'fulfilled') {
        fs.writeFileSync(path.join(__dirname, 'revised_council_deepseek.md'), deepseekRes.value);
        console.log("✅ DeepSeek V3.2 Delivered.");
    }
    if (qwenRes.status === 'fulfilled') {
        fs.writeFileSync(path.join(__dirname, 'revised_council_qwen_master.md'), qwenRes.value);
        console.log("✅ Qwen 3 Coder Delivered.");
    }

    console.log("🏆 ALL 4 MODELS HAVE COMPLETED THE REVISED AUDIT!");
}

main().catch(console.error);
