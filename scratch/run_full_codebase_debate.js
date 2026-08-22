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
                    res.on('data', c => data += c);
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
            await sleep(3000 * attempt);
        }
    }
}

async function runFullCouncilDebate() {
    console.log("🏛️ COMMENCING 3-MODEL DEEP CODE & DATA AUDIT DEBATE (GLM-5, KIMI k2.5, QWEN 3 CODER 480B)\n");

    const liveEngineCode = fs.readFileSync(path.join(__dirname, '..', 'src', 'live_engine.rs'), 'utf-8');
    const mainCode = fs.readFileSync(path.join(__dirname, '..', 'src', 'main.rs'), 'utf-8');
    const scannerCode = fs.readFileSync(path.join(__dirname, '..', 'src', 'scanner.rs'), 'utf-8');
    const routerCode = fs.readFileSync(path.join(__dirname, '..', 'src', 'router.rs'), 'utf-8');
    const serverCode = fs.readFileSync(path.join(__dirname, '..', 'dashboard', 'server.js'), 'utf-8');

    const promptPayload = `
================================================================================
CRITICAL SYSTEM AUDIT: CODEBASE, DATA LOGS, WALLET ISSUE & LIVE TRADING ENGINE
================================================================================

USER QUERY & FINDINGS:
1. "pasted my private keys there still it is showing random balance":
   - Forensic Analysis: The user pasted an 88-char Base58 Private Key ("47CucmhE9pFszCin8rV6TE3ESKNytCwpoe4oTJCcPtYoUPH8KaPwDe6BUuTwKPq5fYdnJXXr2Lx1GoF5ozngVecj").
   - Bug in server.js: server.js treated this raw string as a Public Key and sent it directly to Solana RPC 'getBalance', causing an RPC error / 0 balance!
   - Decoding the secret key proves it derives Public Key "AA1ihbehbPrMW4L9jWtdxJfkH76Up5F2azbeepQ1wixs", which has exactly 0.4100 SOL ($38.63) on Solana Mainnet!
2. "maybe the trade simulator is still on in live trading engine":
   - Code Audit of src/live_engine.rs: Lines 145-175 assemble the bundle plan and check circuit breakers, BUT it simulates the outcome locally (*balance_guard += pnl_sol) instead of actually constructing, signing, and broadcasting raw versioned transaction bundles to the Frankfurt Jito Block Engine endpoint ('https://frankfurt.mainnet.block-engine.jito.wtf/api/v1/bundles')!
3. "other paper trading instances are also stopped":
   - All 3 shadow processes (10sol, 1sol, micro) are online in PM2, but need verification.

================================================================================
SOURCE CODE DUMPS:
================================================================================

--- FILE: src/live_engine.rs ---
${liveEngineCode}

--- FILE: dashboard/server.js (Wallet handler) ---
${serverCode}

--- FILE: src/main.rs ---
${mainCode}

--- FILE: src/scanner.rs (Execution dispatch) ---
${scannerCode}

================================================================================
DEBATE QUESTIONS FOR THE COUNCIL:
================================================================================
1. Forensic Audit & Confirmation: Confirm the exact bugs in wallet input handling (Private Key decoding to Public Key + on-chain balance fetching + saving to server keypair).
2. Live Execution vs Shadow Simulation Fix: Detail the exact Rust implementation needed in 'live_engine.rs' to transition from local state incrementing to real Jito bundle signing & RPC broadcast using the decoded keypair.
3. Complete Action Plan: Synthesize a step-by-step roadmap to fix server.js, live_engine.rs, PM2 instances, and execute real on-chain swaps safely within the 0.41 SOL pilot.
`;

    // 1. GLM-5
    console.log("🤖 [1/3] Round 1: GLM-5 Forensic Code & Security Audit...");
    const glm5 = await queryModelWithRetry(
        "zai.glm-5",
        "You are GLM-5, Principal Solana Core & MEV Security Auditor. Conduct an exhaustive line-by-line forensic audit of the provided code, explaining exactly why the wallet failed to show balance and why live_engine.rs was still simulating.",
        promptPayload
    );
    fs.writeFileSync(path.join(__dirname, 'audit_round1_glm5.md'), glm5);
    console.log("✅ GLM-5 audit complete.\n");

    // 2. Kimi k2.5
    console.log("🤖 [2/3] Round 2: Kimi k2.5 HFT Systems & Jito Execution Architecture...");
    const kimi = await queryModelWithRetry(
        "moonshotai.kimi-k2.5",
        "You are Kimi k2.5, Chief HFT Protocol Engineer. Provide the exact Rust code architecture and Jito bundle broadcaster implementation required for live_engine.rs to sign and execute on-chain.",
        `${promptPayload}\n\n=== GLM-5 FORENSIC AUDIT ===\n${glm5}`
    );
    fs.writeFileSync(path.join(__dirname, 'audit_round2_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 audit complete.\n");

    // 3. Qwen 3 Coder 480B
    console.log("🤖 [3/3] Round 3: Qwen 3 Coder 480B Consensus & Complete Code Fix Plan...");
    const qwen = await queryModelWithRetry(
        "qwen.qwen3-coder-480b-a35b-instruct",
        "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the unanimous council consensus, confirming all bugs and providing the exact, complete, production-ready code fixes for server.js and live_engine.rs.",
        `${promptPayload}\n\n=== GLM-5 AUDIT ===\n${glm5}\n\n=== KIMI K2.5 AUDIT ===\n${kimi}`
    );
    fs.writeFileSync(path.join(__dirname, 'audit_round3_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B consensus complete.\n");

    console.log("🏆 ALL 3 MODELS HAVE CONCLUDED THE DEEP AUDIT DEBATE!");
}

runFullCouncilDebate().catch(console.error);
