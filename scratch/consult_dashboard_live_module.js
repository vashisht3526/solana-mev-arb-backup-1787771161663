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

async function consultDashboardModule() {
    console.log("🛡️ CONSULTING COUNCIL ON DASHBOARD LIVE TRADING MODULE & API SECURITY\n");

    const promptPayload = `
We are adding a dedicated "Live Real-Money Trading Control Center" module to our Solana MEV Dashboard (Node.js/Express + Tailwind HTML frontend).

REQUIREMENTS:
1. 'dashboard/server.js':
   - Parse 'data/live_trades.ndjson' into 'instance_live' with full PnL, win rate, and recent landed trades.
   - API endpoint 'GET /api/live/status' -> returns PM2 live engine status (online/offline), wallet balance, active position sizing.
   - API endpoint 'POST /api/live/control' -> allows Starting, Stopping, or Triggering Emergency Killswitch on 'rust-mev-arb-live'.
   - API endpoint 'POST /api/live/config' -> dynamically updates trading parameters (max position size, min spread trigger, min profit floor).
   - API endpoint 'POST /api/live/wallet' -> verifies connected wallet public address and queries live SOL balance via Mainnet RPC.

2. 'dashboard/public/index.html':
   - Add a 4th top tab: "🛡️ LIVE REAL-MONEY TRADING" alongside the existing shadow instances.
   - Dedicated UI with:
     * Wallet Connection card (Public address input / connect, live SOL balance, USD equivalent).
     * Engine Control card (Start Live, Pause, Emergency Killswitch).
     * Parameter Optimizer sliders (Position Size 0.05-0.15 SOL, Min Spread +0.3%-+2.0%, Min Net Profit, Jito Tip %).
     * Circuit Breaker Live Status grid (Reserve Floor 0.10 SOL, Max Drawdown 0.05 SOL, Cooldown, Ephemeral ATAs).
     * Live Landed Trades table with Solscan links and net profits.

Please review this architectural design, check for security and UX flaws, and give your formal recommendations.
`;

    // 1. GLM-5
    console.log("🤖 [1/3] Consulting GLM-5...");
    const glm5 = await queryModelWithRetry("zai.glm-5", "You are GLM-5, Senior Web & Security Auditor. Audit the proposed dashboard live trading module.", promptPayload);
    fs.writeFileSync(path.join(__dirname, 'dashboard_audit_glm5.md'), glm5);
    console.log("✅ GLM-5 recommendations delivered.\n");

    // 2. Kimi k2.5
    console.log("🤖 [2/3] Consulting Kimi k2.5...");
    const kimi = await queryModelWithRetry("moonshotai.kimi-k2.5", "You are Kimi k2.5, HFT UX & API Systems Auditor. Provide optimizations for the dashboard live trading module.", `${promptPayload}\n\n=== GLM-5 AUDIT ===\n${glm5}`);
    fs.writeFileSync(path.join(__dirname, 'dashboard_audit_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 recommendations delivered.\n");

    // 3. Qwen 3 Coder 480B
    console.log("🤖 [3/3] Consulting Qwen 3 Coder 480B for Consensus...");
    const qwen = await queryModelWithRetry("qwen.qwen3-coder-480b-a35b-instruct", "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the final consensus implementation for dashboard/server.js and dashboard/public/index.html.", `${promptPayload}\n\n=== GLM-5 AUDIT ===\n${glm5}\n\n=== KIMI K2.5 AUDIT ===\n${kimi}`);
    fs.writeFileSync(path.join(__dirname, 'dashboard_audit_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B consensus synthesized.\n");
}

consultDashboardModule().catch(console.error);
