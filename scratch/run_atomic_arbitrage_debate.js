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

                const parsedUrl = new URL(MANTLE_API_URL);
                const req = https.request({
                    hostname: parsedUrl.hostname,
                    port: 443,
                    path: parsedUrl.pathname,
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'Authorization': `Bearer ${MANTLE_BEARER_TOKEN}`,
                        'Content-Length': Buffer.byteLength(postData)
                    }
                }, (res) => {
                    let data = '';
                    res.on('data', chunk => data += chunk);
                    res.on('end', () => {
                        try {
                            const json = JSON.parse(data);
                            if (json.choices && json.choices.length > 0) {
                                resolve(json.choices[0].message.content);
                            } else {
                                reject(new Error(`Invalid response structure: ${data.substring(0, 200)}`));
                            }
                        } catch (e) {
                            reject(e);
                        }
                    });
                });

                req.on('error', reject);
                req.setTimeout(45000, () => {
                    req.destroy(new Error("Request timed out after 45s"));
                });
                req.write(postData);
                req.end();
            });
        } catch (err) {
            console.warn(`⚠️ [Attempt ${attempt}/${retries} failed for ${modelName}]: ${err.message}`);
            if (attempt === retries) throw err;
            await sleep(3000 * attempt);
        }
    }
}

async function runCouncilDebate() {
    console.log("===============================================================================");
    console.log("🏛️ EMERGENCY ARCHITECTURAL COUNCIL DEBATE: ATOMIC CIRCULAR ARBITRAGE (SOL -> SOL)");
    console.log("   Auditors: GLM-5 | Kimi k2.5 | Qwen 3 Coder 480B");
    console.log("===============================================================================\n");

    const codeJitoBuilder = fs.readFileSync(path.join(__dirname, '..', 'src', 'jito_builder.rs'), 'utf-8');
    const codeLiveEngine = fs.readFileSync(path.join(__dirname, '..', 'src', 'live_engine.rs'), 'utf-8');

    const debatePrompt = `
You are participating in an Emergency Architectural Debate for a Solana Native Rust MEV Arbitrage Engine.

### 🚨 THE CRITICAL INCIDENT & ROOT CAUSE:
The live execution engine executed a 1-WAY swap (SOL -> SPL Token) because 'jito_builder.rs' requested a single swap quote (inputMint=SOL, outputMint=TOKEN) from Jupiter. As a result, real SOL was converted into an SPL token holding in the user's wallet, instead of completing the full atomic circular arbitrage cycle back to SOL in the same transaction!

### 🎯 CORE INVARIANT & MANDATORY REQUIREMENTS:
1. **100% ZERO INVENTORY / DIRECTIONAL RISK**: The engine MUST NEVER hold SPL tokens. Every arbitrage execution MUST start with SOL and end with SOL in the EXACT SAME ATOMIC TRANSACTION.
2. **CIRCULAR ATOMIC ROUTING (SOL -> TOKEN -> SOL)**:
   - Option A: Jupiter Circular Routing (inputMint=SOL, outputMint=SOL through intermediate token route, e.g., Raydium -> Meteora).
   - Option B: Atomic 2-Instruction VersionedTransaction or Atomic Jito Bundle (Buy on DEX A + Sell on DEX B) with dynamic compute units and tip.
3. **FAIL-SAFE ON-CHAIN REVERT**: If the final SOL received is less than the input SOL, the transaction/bundle MUST fail on-chain with zero token purchase.
4. **NATIVE RUST IMPLEMENTATION**: Complete, copy-paste ready Rust code for 'src/jito_builder.rs' and 'src/live_engine.rs'.

### 📋 CURRENT CODE:
--- src/jito_builder.rs ---
${codeJitoBuilder}

--- src/live_engine.rs ---
${codeLiveEngine}

Please deliver your rigorous architectural critique and complete Rust implementation of true atomic circular arbitrage (SOL -> SOL).
`;

    // 1. Query GLM-5
    console.log("🤖 [1/3] Querying GLM-5 for Atomic Circular Arbitrage Architecture...");
    const glm5Response = await queryModelWithRetry(
        "zai.glm-5",
        "You are the Principal Solana MEV Security Auditor (GLM-5). Rigorously audit atomic circular arbitrage.",
        debatePrompt
    );
    fs.writeFileSync(path.join(__dirname, 'atomic_debate_glm5.md'), glm5Response);
    console.log("✅ GLM-5 Debate Report saved to scratch/atomic_debate_glm5.md");

    // 2. Query Kimi k2.5
    console.log("\n🤖 [2/3] Querying Kimi k2.5 with GLM-5's critique for Cross-Examination...");
    const kimiPrompt = `
${debatePrompt}

### 📝 GLM-5's ARCHITECTURAL CRITIQUE:
${glm5Response}

Now evaluate GLM-5's proposed solution, point out any remaining edge cases or security flaws, and provide your refined, production-grade Rust implementation for atomic circular arbitrage.
`;
    const kimiResponse = await queryModelWithRetry(
        "moonshotai.kimi-k2.5",
        "You are the Chief HFT Protocol Engineer & Low-Latency Auditor (Kimi k2.5). Cross-examine GLM-5 and construct flawless atomic circular Rust code.",
        kimiPrompt
    );
    fs.writeFileSync(path.join(__dirname, 'atomic_debate_kimi.md'), kimiResponse);
    console.log("✅ Kimi k2.5 Debate Report saved to scratch/atomic_debate_kimi.md");

    // 3. Query Qwen 3 Coder 480B
    console.log("\n🤖 [3/3] Querying Qwen 3 Coder 480B for Final Synthesis & Production Rust Code...");
    const qwenPrompt = `
You are the Chief Synthesis Judge presiding over the Council.
Review the critiques and implementations from GLM-5 and Kimi k2.5:

### GLM-5 REPORT:
${glm5Response}

### KIMI k2.5 REPORT:
${kimiResponse}

Synthesize their unanimous consensus into:
1. Formal Architectural Verdict
2. Exact, production-ready Rust code for 'src/jito_builder.rs' and 'src/live_engine.rs' that guarantees 100% atomic SOL->TOKEN->SOL execution with zero directional risk.
3. Verification checklist.
`;
    const qwenResponse = await queryModelWithRetry(
        "qwen.qwen3-coder-480b-a35b-instruct",
        "You are the Chief Synthesis Judge & Systems Architect (Qwen 3 Coder 480B). Synthesize unanimous council consensus into production Rust code.",
        qwenPrompt
    );
    fs.writeFileSync(path.join(__dirname, 'atomic_debate_qwen_synthesis.md'), qwenResponse);
    console.log("✅ Qwen 3 Coder Final Synthesis saved to scratch/atomic_debate_qwen_synthesis.md");

    console.log("\n🏛️ COUNCIL DEBATE COMPLETED SUCCESSFULLY!");
}

runCouncilDebate().catch(console.error);
