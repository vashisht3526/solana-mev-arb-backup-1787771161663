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

async function runCouncilConsultation() {
    console.log("🏛️ STARTING 3-MODEL COUNCIL CONSULTATION: WALLET KEY MANAGEMENT & ON-CHAIN SIGNING READINESS\n");

    const systemStatusPayload = `
================================================================================
SYSTEM AUDIT & TELEMETRY REPORT: SOLANA MEV ARBITRAGE SYSTEM (FRANKFURT AWS EC2)
================================================================================

1. ENGINE ARCHITECTURE & CODEBASE:
   - Rust native binary (solana-mev-arb v0.1.0) running under PM2 in Frankfurt AWS EC2.
   - Real-Time WebSocket stream connected to PumpPortal ('wss://pumpportal.fun/api/data').
   - Actively tracking 36 volatile tokens and scanning 1,420+ DEX liquidity pools across Raydium, Meteora DLMM, PumpSwap, and Orca Whirlpools.
   - Execution Safeguards:
     * In-Transaction Same-Block Ephemeral ATAs (CreateIdempotentATA -> Swap Leg 1 -> Swap Leg 2 -> CloseAccount -> RequireProfit). 0 SOL net rent locked.
     * Dynamic 180,000 Compute Budget configuration for concentrated AMMs.
     * 1,500ms Temporal Jitter Filter / 4-slot max variance (eliminates ghost spreads).
     * 4-Pillar Token Risk Shields (Freeze/Mint authority checked, Transfer Fee <= 5 bps, $1k USD liquidity floor).
     * 5 Immutable Programmatic Circuit Breakers (0.10 SOL Gas Reserve Floor, 0.05 SOL Max Drawdown Stop-Loss, 3-drop Consecutive Cooldown, 0.08 SOL Position Sizing Cap, Jito 25% Tip Clamp).

2. DASHBOARD & CONTROL PLANE:
   - Node.js/Express Web Control Center running on Port 3000 (http://52.58.226.98:3000).
   - Real-time on-chain RPC balance fetching.
   - Collapsible Parameter & Sizing Optimizer.
   - Disk and browser localStorage configuration persistence.

3. CURRENT SITUATION & USER'S PILOT WALLET:
   - User has $38.63 USD (0.41 SOL) ready to deploy for live trading.
   - User created a dedicated wallet in Phantom ('@SecretSand79 MEV-Live-Bot').
   - In Phantom multi-chain wallet, account keys are derived from a 12-word master Secret Recovery Phrase.
   
TWO PROPOSED WALLET INITIALIZATION METHODS:
- OPTION A (Server-Generated Burner Keypair):
  Generate a brand-new keypair directly on the Frankfurt Linux server via 'solana-keygen new --outfile ~/.config/solana/mev_live.json' with 'chmod 400'. The server outputs a public address, and the user simply sends the 0.41 SOL ($38.63) from Phantom to this address. The private key never travels across the internet or browser.
- OPTION B (Phantom Recovery Phrase Import):
  Import the 12-word recovery phrase from Phantom onto the server via 'solana-keygen recover'.

QUESTIONS FOR THE COUNCIL:
1. Security & MEV Best Practice Evaluation: Which method (Option A vs Option B) is strictly recommended from a cryptographic and MEV operations perspective?
2. Architectural Readiness: Is the live trading engine ready to execute on-chain once funded and armed with the signing key?
3. Risk & Capital Guidelines: How should the 0.41 SOL ($38.63) bankroll be managed during the first 24-48 hours of live execution?
`;

    // 1. GLM-5
    console.log("🤖 [1/3] Consulting GLM-5 (Senior Quantitative MEV Risk Officer)...");
    const glm5 = await queryModelWithRetry(
        "zai.glm-5",
        "You are GLM-5, Senior Solana MEV Risk Officer and Security Auditor. Provide a rigorous cryptographic and operational evaluation of the two wallet methods and assess system readiness.",
        systemStatusPayload
    );
    fs.writeFileSync(path.join(__dirname, 'council_wallet_glm5.md'), glm5);
    console.log("✅ GLM-5 consultation delivered.\n");

    // 2. Kimi k2.5
    console.log("🤖 [2/3] Consulting Kimi k2.5 (HFT Execution & Key Management Architect)...");
    const kimi = await queryModelWithRetry(
        "moonshotai.kimi-k2.5",
        "You are Kimi k2.5, Chief HFT Execution Engineer. Evaluate the execution pipeline, server keypair security, and Jito bundle safety for the 0.41 SOL pilot deployment.",
        `${systemStatusPayload}\n\n=== GLM-5 AUDIT ===\n${glm5}`
    );
    fs.writeFileSync(path.join(__dirname, 'council_wallet_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 consultation delivered.\n");

    // 3. Qwen 3 Coder 480B
    console.log("🤖 [3/3] Consulting Qwen 3 Coder 480B (Chief Systems Architect) for Consensus...");
    const qwen = await queryModelWithRetry(
        "qwen.qwen3-coder-480b-a35b-instruct",
        "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the unanimous council consensus verdict on wallet connection, security protocols, and live trading activation.",
        `${systemStatusPayload}\n\n=== GLM-5 AUDIT ===\n${glm5}\n\n=== KIMI K2.5 AUDIT ===\n${kimi}`
    );
    fs.writeFileSync(path.join(__dirname, 'council_wallet_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B consensus synthesized.\n");

    console.log("🏆 ALL 3 MODELS HAVE DELIVERED THEIR EXPERT AUDITS!");
}

runCouncilConsultation().catch(console.error);
