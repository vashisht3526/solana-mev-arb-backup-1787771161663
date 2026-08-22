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

async function runMultiRoundLiveEngineDebate() {
    console.log("🏛️ STARTING MULTI-ROUND COUNCIL DEBATE: ARCHITECTING THE 'RUST-MEV-ARB-LIVE' REAL MONEY ENGINE\n");

    const baseArchitectureContext = `
================================================================================
MISSION: ARCHITECT A 100% BUG-FREE, ULTRA-SAFE LIVE MAINNET TRADING ENGINE
================================================================================

OBJECTIVE:
Design and specify a separate, dedicated production engine binary/process ('rust-mev-arb-live') to trade real funds on Solana mainnet with maximum safety, starting with small balances ($35 - $100 / 0.38 - 1.10 SOL).

CORE TOPICS FOR THE COUNCIL:

1. KEYPAIR & WALLET ISOLATION:
   - How should the private key be ingested and held in memory (e.g. zeroize-on-drop, read from secure ENV / isolated keypair file)?
   - How to ensure the engine only accesses the intended trading wallet and never exposes credentials in logs?

2. PRE-FLIGHT SAFETY SHIELDS & SIMULATION:
   - On-chain simulation before bundle dispatch: Should we call RPC simulateTransaction to verify output before paying Jito tips?
   - In-bundle atomic revert guarantee: 'require!(final_sol >= input_sol + min_net_profit)'.
   - Jito Tip account rotation across the 8 official Jito tip accounts.

3. HARD PROGRAMMATIC CIRCUIT BREAKERS:
   - Minimum Reserve Floor: e.g. If wallet balance <= 0.15 SOL, engine immediately refuses all new trades to prevent running out of gas.
   - Max Drawdown Stop-Loss: e.g. If net balance drops by 0.05 SOL in 24h, automatically trigger a hard emergency shutdown and alert.
   - Consecutive Failure Cooldown: If 3 consecutive Jito bundles are dropped or fail, pause trading for 300 seconds.
   - Position Sizing Envelope: Hard cap position size to min(0.08 SOL, 20% of active liquid balance).

4. EPHEMERAL ATA & GAS MANAGEMENT:
   - In-transaction Create-Swap-Swap-Close lifecycle.
   - Dynamic 180,000 CU Compute Budget with priority fee cap.

5. ERROR HANDLING & TELEMETRY:
   - Real-time logging of live on-chain transaction signatures, actual Jito landed slots, and real lamport PnL.
   - PM2 process isolation alongside the existing shadow engines.
`;

    // =========================================================================
    // ROUND 1: INITIAL ARCHITECTURAL PROPOSALS
    // =========================================================================
    console.log("⚔️ [ROUND 1] Gathering Initial Architecture Proposals from GLM-5 and Kimi k2.5...");
    
    const r1_glm5 = await queryModelWithRetry(
        "zai.glm-5",
        "You are GLM-5, Senior Solana MEV Quantitative Risk Officer & Smart Contract Auditor. Propose the complete architecture, safety invariants, and circuit breakers for the live trading engine.",
        baseArchitectureContext
    );
    fs.writeFileSync(path.join(__dirname, 'live_engine_r1_glm5.md'), r1_glm5);
    console.log("✅ Round 1: GLM-5 proposal delivered.");

    const r1_kimi = await queryModelWithRetry(
        "moonshotai.kimi-k2.5",
        "You are Kimi k2.5, Chief HFT Execution Engineer & Solana Kernel Auditor. Propose the zero-bug execution pipeline, Jito bundle manager, and memory security for the live engine.",
        baseArchitectureContext
    );
    fs.writeFileSync(path.join(__dirname, 'live_engine_r1_kimi.md'), r1_kimi);
    console.log("✅ Round 1: Kimi k2.5 proposal delivered.\n");

    // =========================================================================
    // ROUND 2: CROSS-EXAMINATION & THREAT MODELING
    // =========================================================================
    console.log("⚔️ [ROUND 2] Cross-Examining Proposals for Edge-Case Vulnerabilities & Failure Modes...");

    const r2_cross_prompt = `
${baseArchitectureContext}

=== GLM-5 ROUND 1 PROPOSAL ===
${r1_glm5}

=== KIMI K2.5 ROUND 1 PROPOSAL ===
${r1_kimi}

CROSS-EXAMINATION TASKS:
1. Identify any flaws, race conditions, or unhandled edge cases in the other model's proposal.
2. Specifically critique:
   - What happens if the Jito RPC endpoint experiences a 5-second HTTP timeout?
   - What happens if a token account closure fails due to 1 lamport dust?
   - How to prevent front-running on the simulation RPC?
   - What exact circuit breaker values are mathematically optimal for a $35 - $100 bankroll?
`;

    const r2_glm5 = await queryModelWithRetry(
        "zai.glm-5",
        "You are GLM-5, Senior Risk Officer. Cross-examine Kimi k2.5's technical proposal and point out every potential edge case or vulnerability.",
        r2_cross_prompt
    );
    fs.writeFileSync(path.join(__dirname, 'live_engine_r2_glm5.md'), r2_glm5);
    console.log("✅ Round 2: GLM-5 cross-examination delivered.");

    const r2_kimi = await queryModelWithRetry(
        "moonshotai.kimi-k2.5",
        "You are Kimi k2.5, Chief HFT Engineer. Defend and refine your execution model, address GLM-5's critiques, and finalize the execution safeguards.",
        r2_cross_prompt
    );
    fs.writeFileSync(path.join(__dirname, 'live_engine_r2_kimi.md'), r2_kimi);
    console.log("✅ Round 2: Kimi k2.5 defense delivered.\n");

    // =========================================================================
    // ROUND 3: FINAL SYNTHESIS & UNANIMOUS ARCHITECTURAL SPECIFICATION
    // =========================================================================
    console.log("⚔️ [ROUND 3] Synthesizing Final Consensus Architecture with Qwen 3 Coder 480B...");

    const r3_consensus_prompt = `
${baseArchitectureContext}

=== ROUND 1 PROPOSALS ===
GLM-5:
${r1_glm5}

KIMI K2.5:
${r1_kimi}

=== ROUND 2 CROSS-EXAMINATION & DEBATE ===
GLM-5:
${r2_glm5}

KIMI K2.5:
${r2_kimi}

TASK FOR CHIEF SYSTEMS ARCHITECT (Qwen 3 Coder 480B):
Synthesize the complete, definitive, unanimous blueprint for 'rust-mev-arb-live':
1. Architectural Blueprint: The exact struct hierarchy, concurrency model, and keypair security.
2. The 5 Immutable Safety Invariants (Circuit Breakers, Minimum Reserve Floor, Max Drawdown, Tip Caps, Dust Management).
3. The Exact Atomic Jito Execution Flow (Build -> Ephemeral ATA -> Simulate -> Sign -> Jito Bundle Broadcast -> Confirm/Abort).
4. Full Rust Implementation Code Structure for:
   - 'src/live_engine.rs' (Live execution worker)
   - Circuit breaker configuration and runtime parameters.
`;

    const r3_qwen = await queryModelWithRetry(
        "qwen.qwen3-coder-480b-a35b-instruct",
        "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the final, definitive unanimous consensus blueprint for the live trading engine.",
        r3_consensus_prompt
    );
    fs.writeFileSync(path.join(__dirname, 'live_engine_r3_qwen_consensus.md'), r3_qwen);
    console.log("✅ Round 3: Qwen 3 Coder 480B synthesized the final unanimous blueprint.\n");

    console.log("🏆 ALL 3 ROUNDS OF COUNCIL DEBATE SUCCESSFULLY COMPLETED!");
}

runMultiRoundLiveEngineDebate().catch(console.error);
