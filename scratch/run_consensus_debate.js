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
            timeout: 180000
        }, (res) => {
            let data = '';
            res.on('data', (chunk) => data += chunk);
            res.on('end', () => {
                try {
                    const json = JSON.parse(data);
                    if (json.choices && json.choices.length > 0) {
                        resolve(json.choices[0].message.content);
                    } else {
                        reject(new Error("API error: " + data));
                    }
                } catch (e) {
                    reject(new Error("JSON parse error: " + data));
                }
            });
        });

        req.on('error', reject);
        req.write(postData);
        req.end();
    });
}

async function runMasterConsensusDebate() {
    console.log("===============================================================================");
    console.log("⚔️ RUNNING 4-WAY FRONTIER AI COUNCIL DEBATE (GLM-5 + KIMI 2.5 + QWEN 3 + ANTIGRAVITY)");
    console.log("===============================================================================\n");

    const auditDossier = `
### COMPLETE CODEBASE AUDIT DOSSIER (PRESENTED TO THE COUNCIL)

We have conducted a complete line-by-line audit of the repository (\`solana-mev-arb\`).
Current Wallet State: 0.404674 SOL ($38.05 USD) on AWS EC2 Frankfurt (52.58.226.98).
Target Strategy: Sub-millisecond Solana MEV Arbitrage (SOL -> Token -> SOL in 1 Atomic Tx) to Jito Frankfurt Block Engine.

HERE ARE THE VERIFIED FATAL FLAWS FOUND IN THE REPOSITORY:

1. DATA INGESTION & LATENCY COLLAPSE:
   - \`scanner.rs\` and \`server.js\` poll the DexScreener REST API (\`api.dexscreener.com\`) every 1.5s-3s.
   - DexScreener data is 30-120 seconds stale.
   - Comparing real-time PumpPortal events with 60-second-old DexScreener cache generates 100% "phantom/ghost spreads" (0.5% - 15%) that do not exist on-chain.
   - The scanner also makes up to 100 req/sec to DexScreener, triggering HTTP 429 rate limits.
   - \`websocket_feed.rs\` subscribes to \`subscribeNewToken\` and \`subscribeRaydiumLiquidity\` but omits \`subscribeTokenTrade\`.

2. JUPITER API SEQUENTIAL LATENCY & TRANSACTION SIZE EXPLOSION:
   - \`jito_builder.rs\` makes 4 sequential HTTP requests to Jupiter quote-api (quote A -> quote B -> swap-ix A -> swap-ix B), taking 800ms - 1500ms. In Solana MEV, slots are 400ms and opportunities vanish in <30ms.
   - Jupiter is an aggregator that normalizes routes; circular quotes through Jupiter rarely yield positive net profit.
   - \`Message::try_compile\` passes empty lookup tables (\`&[]\`). 2 Jupiter swaps require 50-70 accounts, exceeding the 1,232-byte IPv6 MTU packet limit and failing to compile/land.

3. COUPLING & INSUFFICIENT FUNDS REVERT:
   - Leg B is constructed with \`inAmount = quote_res_a.outAmount\`. If Leg A slips by even 1 lamport, Leg B fails with InsufficientFunds.

4. MATHEMATICAL & SOLVER BUGS:
   - \`math.rs\` line 74: Profit formula P(delta) has mismatched asset dimensions and subtracts tokens from SOL.
   - \`math.rs\` line 191: Output formula \`gamma_a * r_tok_a * delta / (r_sol_a + delta)\` omits gamma_a in the denominator (should be \`r_sol_a + gamma_a * delta\`).
   - \`amm.rs\` assumes all pools (including Meteora DLMM and Orca Whirlpools) are 50/50 CPMM.

5. SECURITY & MOCK ARTIFACTS:
   - \`src/bin/reclaim_rent.rs\` has a real private key hardcoded in plaintext on line 18.
   - \`dashboard/live_executor.js\` is a 1-way token buyer (SOL -> Token) that never sells back and marks losses as profit 0 with "LANDED_CONFIRMED".
   - \`router.rs\` is a mock string generator never used in the pipeline.
   - \`paper_engine.rs\` uses a random coin toss (\`roll < failure_probability\`) giving +0.05 SOL on win and -0.000005 SOL on loss, creating an artificial compounding balance illusion.

COUNCIL MANDATE:
All 3 models must debate each issue, propose concrete and uncompromising mathematical/code fixes, and reach a single unified consensus on how to make this system truly profitable and resilient.
`;

    // Round 1: GLM-5 Deep Technical Prescription
    console.log("⚡ [1/3] Querying GLM-5 (Principal Solana Security Auditor)...");
    const glm5Prompt = `
You are GLM-5, Principal Solana Security Auditor.
Carefully review the complete audit dossier above.
Provide your uncompromising analysis:
1. Address the DexScreener lag vs Yellowstone gRPC / Solana WebSocket architecture.
2. Address the Jupiter 4x HTTP latency vs direct local instruction assembly.
3. Fix the mathematical formulas in \`math.rs\` and \`amm.rs\` with exact LaTeX equations.
4. Fix the ALT (Address Lookup Table) resolution and Leg B token coupling in \`jito_builder.rs\`.
5. Provide the exact security refactoring roadmap.
`;
    const glm5Response = await queryModel(
        "zai.glm-5",
        "You are GLM-5, lead Solana security auditor and quantitative engineer.",
        auditDossier + "\n\n" + glm5Prompt
    );
    fs.writeFileSync(path.join(__dirname, 'debate_v4_glm5.md'), glm5Response);
    console.log("✅ GLM-5 Round 1 Complete! Saved to debate_v4_glm5.md");

    // Round 2: Kimi k2.5 HFT Optimization & Execution Challenge
    console.log("\n⚡ [2/3] Querying Kimi k2.5 (Chief HFT Protocol Engineer)...");
    const kimiPrompt = `
You are Kimi k2.5, Chief HFT Protocol Engineer.
Review the audit dossier and GLM-5's technical prescription below:

\`\`\`markdown
${glm5Response}
\`\`\`

YOUR TASK AS KIMI K2.5:
1. Challenge any suboptimal suggestions in GLM-5's proposal.
2. Formulate the fastest possible hot-path architecture for Frankfurt EC2 -> Jito Frankfurt (sub-10ms pipeline).
3. Specify the exact mathematical closed-form / Newton-Raphson CPMM optimal trade size equation.
4. Detail the Address Lookup Table resolution algorithm for VersionedTransaction V0.
5. Provide strict invariant guards to ensure 0.00 SOL token balance remains after execution.
`;
    const kimiResponse = await queryModel(
        "moonshotai.kimi-k2.5",
        "You are Kimi k2.5, expert quant HFT engineer and Solana MEV architect.",
        kimiPrompt
    );
    fs.writeFileSync(path.join(__dirname, 'debate_v4_kimi.md'), kimiResponse);
    console.log("✅ Kimi k2.5 Round 2 Complete! Saved to debate_v4_kimi.md");

    // Round 3: Qwen 3 Coder 480B Unified Consensus Synthesis
    console.log("\n⚡ [3/3] Querying Qwen 3 Coder 480B (Chief Systems Architect & Final Judge)...");
    const qwenPrompt = `
You are Qwen 3 Coder 480B, Chief Systems Architect and Presiding Judge of the Solana MEV Council.
You have the complete audit dossier, GLM-5's security prescription, and Kimi k2.5's HFT protocol design:

GLM-5 PRESCRIPTION:
\`\`\`markdown
${glm5Response}
\`\`\`

KIMI K2.5 HFT ARCHITECTURE:
\`\`\`markdown
${kimiResponse}
\`\`\`

YOUR TASK AS QWEN 3 CODER 480B:
Synthesize the definitive, unified consensus specification across all 4 models (Antigravity + GLM-5 + Kimi + Qwen3).
Provide:
1. UNIFIED ARCHITECTURAL BLUEPRINT (Data Ingestion, Sizing Solver, Transaction Builder, Jito Dispatch).
2. EXACT MATHEMATICAL EQUATIONS for \`math.rs\` and \`amm.rs\` (derivation of CPMM optimal input size delta* with asymmetric fees gamma_1, gamma_2).
3. EXACT TRANSACTION INSTRUCTION COMPOSITION (ALT fetching, WSOL/ATA deduplication, Leg B coupling, dynamic Jito tip).
4. STEP-BY-STEP IMPLEMENTATION ROADMAP FOR ALL REPOSITORY FILES.
`;
    const qwenResponse = await queryModel(
        "qwen.qwen3-coder-480b-a35b-instruct",
        "You are Qwen 3 Coder 480B, lead Solana runtime architect and final judge.",
        qwenPrompt
    );
    fs.writeFileSync(path.join(__dirname, 'debate_v4_qwen_consensus.md'), qwenResponse);
    console.log("✅ Qwen 3 Coder 480B Consensus Synthesis Complete! Saved to debate_v4_qwen_consensus.md");

    console.log("\n🏆 ALL 3 FRONTIER MODELS HAVE COMPLETED THE DEBATE AND DELIVERED THE UNANIMOUS CONSENSUS!");
}

runMasterConsensusDebate().catch(e => {
    console.error("❌ Debate Execution Error:", e);
    process.exit(1);
});
