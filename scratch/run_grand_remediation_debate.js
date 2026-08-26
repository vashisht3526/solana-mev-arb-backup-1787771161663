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

async function runGrandDebate() {
    console.log("===============================================================================");
    console.log("⚔️ GRAND COUNCIL MULTI-MODEL DEBATE: COMPLETE ON-CHAIN SOLANA MEV ENGINE FIX");
    console.log("   Participants: GLM-5 | Kimi k2.5 | Qwen 3 Coder 480B");
    console.log("===============================================================================\n");

    const liveEngineCode = fs.readFileSync(path.join(__dirname, '..', 'src', 'live_engine.rs'), 'utf-8');
    const mainCode = fs.readFileSync(path.join(__dirname, '..', 'src', 'main.rs'), 'utf-8');
    const routerCode = fs.readFileSync(path.join(__dirname, '..', 'src', 'router.rs'), 'utf-8');
    const jitoCode = fs.readFileSync(path.join(__dirname, '..', 'src', 'jito.rs'), 'utf-8');
    const jitoBuilderCode = fs.readFileSync(path.join(__dirname, '..', 'src', 'jito_builder.rs'), 'utf-8');
    const serverCode = fs.readFileSync(path.join(__dirname, '..', 'dashboard', 'server.js'), 'utf-8');
    const cargoToml = fs.readFileSync(path.join(__dirname, '..', 'Cargo.toml'), 'utf-8');

    const debateContext = `
================================================================================
COMPLETE CODEBASE DOSSIER & ACTUAL LIVE TELEMETRY
================================================================================

1. ACTIVE ENVIRONMENT:
   - Target Wallet: AA1ihbehbPrMW4L9jWtdxJfkH76Up5F2azbeepQ1wixs
   - Actual Solana Mainnet On-Chain Balance: 0.4100 SOL ($38.09 USD)
   - Real Keypair Path: /home/ubuntu/.config/solana/mev_live.json (0400 permissions)
   - Jito Block Engine Frankfurt: https://frankfurt.mainnet.block-engine.jito.wtf/api/v1/bundles
   - Public RPC: https://api.mainnet-beta.solana.com

2. THE 4 BLOCKING DEFECTS IDENTIFIED IN PIV:
   a) Fake / Placeholder Bundle Payload in live_engine.rs: Must construct real versioned transaction bundles encoded in Base58/Base64.
   b) Missing Jito Bundle Status Polling: Must call 'getBundleStatuses' via JSON-RPC to confirm on-chain inclusion before recording profit.
   c) Ephemeral ATA PDA Derivation: Must compute true Associated Token Account PDAs.
   d) Complete Zero-Simulation Guarantee: Dashboard & Rust engine must strictly only report real landed on-chain swaps.

3. CURRENT CARGO.TOML:
\`\`\`toml
${cargoToml}
\`\`\`

4. CURRENT SRC/LIVE_ENGINE.RS:
\`\`\`rust
${liveEngineCode}
\`\`\`

5. CURRENT SRC/ROUTER.RS:
\`\`\`rust
${routerCode}
\`\`\`

6. CURRENT SRC/JITO_BUILDER.RS & SRC/JITO.RS:
\`\`\`rust
${jitoBuilderCode}
---
${jitoCode}
\`\`\`

7. CURRENT DASHBOARD/SERVER.JS:
\`\`\`javascript
${serverCode}
\`\`\`
`;

    // 1. GLM-5 Round 1
    console.log("🤖 [1/3] Round 1: GLM-5 (Solana Core & Cryptographic Instruction Architect)...");
    const glm5 = await queryModelWithRetry(
        "zai.glm-5",
        "You are GLM-5, Principal Solana Core & MEV Security Auditor. Provide the exact Rust code architecture for real Solana instruction serialization, ATA PDA calculation, and transaction signing for Jito bundles.",
        `${debateContext}\n\nTASK: Propose the exact, complete Rust code for 'src/live_engine.rs', 'src/jito_builder.rs', and Cargo.toml dependencies to make Jito bundle creation and signing 100% on-chain real.`
    );
    fs.writeFileSync(path.join(__dirname, 'grand_debate_round1_glm5.md'), glm5);
    console.log("✅ GLM-5 Round 1 Proposal delivered.\n");

    // 2. Kimi k2.5 Round 2
    console.log("🤖 [2/3] Round 2: Kimi k2.5 (HFT Protocol & Jito Polling Engineer)...");
    const kimi = await queryModelWithRetry(
        "moonshotai.kimi-k2.5",
        "You are Kimi k2.5, Chief HFT Protocol Engineer. Review GLM-5's proposal, cross-examine edge cases (blockhash expiry, Jito tip account selection, bundle status polling timeout, slippage enforcement), and improve the pipeline.",
        `${debateContext}\n\n=== GLM-5 PROPOSAL ===\n${glm5}\n\nTASK: Cross-examine GLM-5's code and provide the exact asynchronous Jito 'getBundleStatuses' polling worker and real RPC blockhash fetcher.`
    );
    fs.writeFileSync(path.join(__dirname, 'grand_debate_round2_kimi.md'), kimi);
    console.log("✅ Kimi k2.5 Round 2 Cross-Examination delivered.\n");

    // 3. Qwen 3 Coder 480B Round 3
    console.log("🤖 [3/3] Round 3: Qwen 3 Coder 480B (Chief Systems Architect) Synthesis & Final Production Code...");
    const qwen = await queryModelWithRetry(
        "qwen.qwen3-coder-480b-a35b-instruct",
        "You are Qwen 3 Coder 480B, Chief Systems Architect. Synthesize the unanimous council consensus and provide the complete, finalized, copy-paste ready code for 'src/live_engine.rs', 'src/jito_builder.rs', 'src/router.rs', and 'Cargo.toml'.",
        `${debateContext}\n\n=== GLM-5 PROPOSAL ===\n${glm5}\n\n=== KIMI K2.5 REVIEW ===\n${kimi}\n\nTASK: Output the final production code for all files to be deployed directly onto the Frankfurt server.`
    );
    fs.writeFileSync(path.join(__dirname, 'grand_debate_round3_qwen_consensus.md'), qwen);
    console.log("✅ Qwen 3 Coder 480B Final Consensus Blueprint synthesized.\n");

    console.log("🏆 ALL 3 MODELS HAVE CONCLUDED THE GRAND REMEDIATION DEBATE!");
}

runGrandDebate().catch(console.error);
