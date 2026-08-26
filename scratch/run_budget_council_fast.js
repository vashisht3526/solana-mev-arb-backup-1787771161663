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
            timeout: 90000
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
    const prompt = `
A comprehensive institutional architecture paper on Solana MEV Arbitrage was submitted.
The paper outlines:
1. Bare-metal AMD EPYC servers with XDP, SO_BUSY_POLL, kernel bypass, and fiber backbones.
2. Yellowstone gRPC Geyser & Jito ShredStream for sub-slot memory ingestion.
3. In-memory zero-RPC AMM solvers (Native Pump.fun bonding curves, Meteora DLMM bin step math, Raydium CPMM/CLMM).
4. Dynamic Jito tip streams and atomic bundle anti-bandit defenses (jitodontfront).
5. Capital dilution of friction (scaling from 0.035 SOL to 1-2+ SOL to drop friction from 2.5% to 0.1%).

### REAL-WORLD CONSTRAINT:
The user states: "definitely we cannot afford that bare-metal enterprise infrastructure ($2,000-$5,000/mo server clusters + private Geyser dedicated nodes)."
The user currently has:
- Current standard cloud VM in Frankfurt (AWS EC2 eu-central-1, ~$20-$40/mo).
- $38 starting bankroll (0.4046 SOL).
- Direct free PumpPortal WebSocket stream.
- Free Jito Block Engine bundle endpoint.
- Free Frankfurt Jupiter Lite API.

### YOUR MANDATE:
1. What software & mathematical insights in this paper can be implemented **100% FOR FREE in pure Rust code** without enterprise hardware?
2. How to extract the maximum latency advantage (Local Rust AMM math, Pump.fun bonding curve parsing, dynamic tip stream) on our current budget?
3. What specific high-spread niches (Pump.fun migrations, new bonding curve launches, high-volatility spikes) allow a $38 bankroll to win +5% to +15% spreads consistently?
4. Provide a concrete, phased growth roadmap from $38 -> $100 -> $500.
`;

    console.log("🚀 Launching DeepSeek V3.2, Kimi k2.5, and Qwen 3 Coder in parallel...");

    const [deepseekRes, kimiRes, qwenRes] = await Promise.allSettled([
        queryModel("deepseek.v3.2", "You are DeepSeek V3.2, Quantitative MEV Strategist. Focus on niche selection, capital growth from $38 to $500, and game-theoretic bundle economics on budget hardware.", prompt),
        queryModel("moonshotai.kimi-k2.5", "You are Kimi k2.5, Chief HFT Protocol Engineer. Focus on zero-cost software latency optimizations, local Rust AMM solvers, and Pump.fun migration mechanics.", prompt),
        queryModel("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize unanimous consensus and deliver the Master Implementation Blueprint for low-cost, high-profit Solana MEV.", prompt)
    ]);

    if (deepseekRes.status === 'fulfilled') {
        fs.writeFileSync(path.join(__dirname, 'budget_council_deepseek.md'), deepseekRes.value);
        console.log("✅ DeepSeek V3.2 Delivered.");
    } else {
        console.warn("⚠️ DeepSeek failed:", deepseekRes.reason.message);
    }

    if (kimiRes.status === 'fulfilled') {
        fs.writeFileSync(path.join(__dirname, 'budget_council_kimi.md'), kimiRes.value);
        console.log("✅ Kimi k2.5 Delivered.");
    } else {
        console.warn("⚠️ Kimi failed:", kimiRes.reason.message);
    }

    if (qwenRes.status === 'fulfilled') {
        fs.writeFileSync(path.join(__dirname, 'budget_council_qwen_master_blueprint.md'), qwenRes.value);
        console.log("✅ Qwen 3 Coder Delivered.");
    } else {
        console.warn("⚠️ Qwen failed:", qwenRes.reason.message);
    }

    console.log("🏆 ALL MODELS COMPLETED!");
}

main().catch(console.error);
