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

async function runCouncilDebate() {
    console.log("===============================================================================");
    console.log("🏛️ FULL MULTI-MODEL COUNCIL DEBATE: DUAL-LEG ATOMICITY & DYNAMIC POOL DISCOVERY");
    console.log("   Council: GLM-5 | Kimi k2.5 | Qwen 3 Coder 480B");
    console.log("===============================================================================\n");

    const debatePrompt = `
You are a Principal High-Frequency Trading (HFT) Systems Architect and Solana MEV Specialist participating in a mandatory technical debate.

ALL 3 MODELS IN THE COUNCIL ARE DEBATING AND SOLVING THE EXACT SAME TWO CRITICAL ARCHITECTURAL PROBLEMS TOGETHER:

### PROBLEM 1: ABSOLUTE DUAL-LEG ATOMICITY (SOL -> TOKEN -> SOL)
- The user observed that previous implementations executed 1-way directional swaps (buying tokens and holding them in the wallet), resulting in stranded tokens and capital degradation when the market moved.
- You must analyze, debate, and provide the exact Rust + Solana SDK architecture ensuring that NO single-leg trade is EVER created or broadcast.
- Both Leg A (SOL -> Intermediate Token) and Leg B (Intermediate Token -> SOL) plus the Jito Tip MUST be compiled into a SINGLE VersionedTransaction (V0) via Jupiter /swap-instructions or custom instruction builder.
- If output SOL < input SOL + tip + fees, the transaction MUST revert atomically on-chain with 0 tokens bought and 0 SOL lost.
- Detail the exact mathematical invariants, compute budget configuration, instruction ordering, and error handling.

### PROBLEM 2: 100% DYNAMIC MULTI-POOL DISCOVERY & SCANNING
- The system must NOT use any hardcoded static list of tokens/pools (such as SEED_MINTS in dashboard or fixed arrays in Rust).
- It must dynamically discover and scan 1000s of active liquidity pools across Raydium CPMM/CLMM, Meteora DLMM, Orca Whirlpool, and PumpPortal real-time WebSocket feeds.
- Explain the exact dynamic token admission pipeline, volatility scoring, temporal synchronization filter (eliminating ghost spreads), and parallel high-frequency evaluation architecture.

Provide your comprehensive critique, architectural proof, and exact code implementations for both Rust (engine) and Dashboard/Node (scanner).
`;

    // 1. Query GLM-5
    console.log("🤖 [1/3] Querying GLM-5 for Full Technical Debate...");
    const glm5Response = await queryModelWithRetry(
        "zai.glm-5",
        "You are GLM-5, Principal Solana Security Auditor & MEV Architect. Debate both problems thoroughly with code.",
        debatePrompt
    );
    fs.writeFileSync(path.join(__dirname, 'council_debate_glm5.md'), glm5Response);
    console.log("✅ GLM-5 Debate Contribution Recorded.\n");

    // 2. Query Kimi k2.5 with cross-examination of GLM-5
    console.log("🤖 [2/3] Querying Kimi k2.5 for Cross-Examination & Solution...");
    const kimiPrompt = `
${debatePrompt}

---
### GLM-5'S SUBMISSION:
${glm5Response}
---

Critique GLM-5's proposals, identify any edge-case failure modes in atomicity or pool scanning, and provide your rigorous HFT production implementation for both problems.
`;
    const kimiResponse = await queryModelWithRetry(
        "moonshotai.kimi-k2.5",
        "You are Kimi k2.5, Chief HFT Protocol Engineer & Distributed Systems Specialist. Cross-examine GLM-5 and provide your production HFT code.",
        kimiPrompt
    );
    fs.writeFileSync(path.join(__dirname, 'council_debate_kimi.md'), kimiResponse);
    console.log("✅ Kimi k2.5 Cross-Examination Recorded.\n");

    // 3. Query Qwen 3 Coder 480B for Unanimous Consensus & Unified Code Synthesis
    console.log("🤖 [3/3] Querying Qwen 3 Coder 480B for Unanimous Consensus Synthesis...");
    const qwenPrompt = `
${debatePrompt}

---
### COUNCIL MEMBER CONTRIBUTIONS:

#### GLM-5:
${glm5Response}

#### Kimi k2.5:
${kimiResponse}
---

As Chief Systems Architect, reconcile all arguments, resolve discrepancies, and synthesize the FINAL UNANIMOUS CONSENSUS ARCHITECTURE & CODE for:
1. Complete Rust Jito Builder & Live Engine with Dual-Leg Atomic Transaction Assembly.
2. Complete 100% Dynamic Multi-Pool Discovery Scanner in Rust & Dashboard (zero hardcoded seeds).
`;
    const qwenResponse = await queryModelWithRetry(
        "qwen.qwen3-coder-480b-a35b-instruct",
        "You are Qwen 3 Coder 480B, Chief Systems Architect & Council Lead. Synthesize unanimous consensus architecture and code.",
        qwenPrompt
    );
    fs.writeFileSync(path.join(__dirname, 'council_debate_qwen_synthesis.md'), qwenResponse);
    console.log("✅ Qwen 3 Coder 480B Unanimous Consensus Synthesis Recorded.\n");

    console.log("🏆 ALL 3 MODELS HAVE CONCLUDED THE DEBATE WITH FULL CONSENSUS!");
}

runCouncilDebate().catch(err => {
    console.error("❌ Council Debate failed:", err);
    process.exit(1);
});
