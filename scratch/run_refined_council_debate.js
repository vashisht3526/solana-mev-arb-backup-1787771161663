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
            max_tokens: 3000
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
    console.log("===============================================================================");
    console.log("🏛️ 4-MODEL COUNCIL: REFINED ARCHITECTURAL DECISION & PURE SHADOW SIMULATION");
    console.log("===============================================================================\n");

    const prompt = `
User clarification on the Solana Bot Architecture:
1. NO REAL FUNDS will be committed to the new Migration Sniping engine for now. It will be built strictly as a 100% Shadow / Paper-Trading / Simulation Engine. Only after long-term proof of edge will funding even be considered.
2. The architectural question (Keep vs Replace) is purely from a software design and strategic edge perspective:
   - Should we structure the system as a Modular Multi-Strategy Engine (running both Atomic MEV Arbitrage and the Migration Sniper side-by-side in high-precision shadow simulation), OR
   - Should we completely gut the atomic arb code and turn the repository 100% into a single-purpose Migration Sniping system?

Audit questions for council:
1. Software Architecture: What are the engineering pros & cons of a Modular Multi-Strategy Engine vs Single-Purpose Pivot?
2. Simulation Fidelity: How to design the Migration Sniper shadow engine so that simulated fills, latency, slippage, and stop-loss execution match real mainnet execution 99.9% without risking $0.01 of real SOL?
3. Metrics & Dashboard: How to pipe both strategy streams into the dashboard (http://52.58.226.98:3000) for real-time telemetry comparison?
`;

    console.log("🚀 Querying Qwen 3 Coder 480B and DeepSeek V3.2...");

    const [qwenRes, deepseekRes] = await Promise.allSettled([
        queryModel("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the optimal software architecture for a zero-risk shadow simulation engine.", prompt),
        queryModel("deepseek.v3.2", "You are DeepSeek V3.2, Quantitative MEV Strategist. Analyze multi-strategy engine design vs single-purpose pivot from a quant research perspective.", prompt)
    ]);

    if (qwenRes.status === 'fulfilled') {
        fs.writeFileSync(path.join(__dirname, 'refined_council_qwen.md'), qwenRes.value);
        console.log("✅ Qwen 3 Coder Delivered.");
    }
    if (deepseekRes.status === 'fulfilled') {
        fs.writeFileSync(path.join(__dirname, 'refined_council_deepseek.md'), deepseekRes.value);
        console.log("✅ DeepSeek V3.2 Delivered.");
    }

    console.log("🏆 REFINED COUNCIL COMPLETE!");
}

main().catch(console.error);
