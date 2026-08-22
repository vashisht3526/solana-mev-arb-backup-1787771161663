const express = require('express');
const http = require('http');
const https = require('https');
const WebSocket = require('ws');
const fs = require('fs');
const path = require('path');
const os = require('os');
const { exec } = require('child_process');

const app = express();
const server = http.createServer(app);
const wss = new WebSocket.Server({ server });

const PORT = process.env.PORT || 3000;
const DATA_DIR = path.join(__dirname, '..', 'data');
const PAPER_TRADES_MICRO_PATH = path.join(DATA_DIR, 'paper_trades_micro.ndjson');
const PAPER_TRADES_1SOL_PATH = path.join(DATA_DIR, 'paper_trades_1sol.ndjson');
const PAPER_TRADES_10SOL_PATH = path.join(DATA_DIR, 'paper_trades_10sol.ndjson');
const LIVE_TRADES_PATH = path.join(DATA_DIR, 'live_trades.ndjson');
const LIVE_CONFIG_PATH = path.join(DATA_DIR, 'live_trading_config.json');

app.use(express.static(path.join(__dirname, 'public')));
app.use(express.json());

let liveSolPrice = 88.50;

// Persistent Live Configuration
let liveTradingConfig = {
    wallet_pubkey: process.env.SOLANA_WALLET_PUBKEY || '',
    wallet_balance_sol: 0.38,
    is_engine_running: false,
    max_position_sol: 0.08,
    min_spread_pct: 0.50,
    min_net_profit_sol: 0.00030,
    jito_tip_pct: 25,
    reserve_floor_sol: 0.10,
    max_drawdown_sol: 0.05,
    status_label: 'IDLE / READY TO TRADE'
};

// Load saved config from disk on startup
if (fs.existsSync(LIVE_CONFIG_PATH)) {
    try {
        const saved = JSON.parse(fs.readFileSync(LIVE_CONFIG_PATH, 'utf-8'));
        liveTradingConfig = { ...liveTradingConfig, ...saved };
        console.log(`📁 Loaded persistent live config for wallet: ${liveTradingConfig.wallet_pubkey || 'None'}`);
    } catch (e) {
        console.warn('⚠️ Could not parse live config file');
    }
}

function saveLiveConfigToDisk() {
    try {
        if (!fs.existsSync(DATA_DIR)) fs.mkdirSync(DATA_DIR, { recursive: true });
        fs.writeFileSync(LIVE_CONFIG_PATH, JSON.stringify(liveTradingConfig, null, 2));
    } catch (e) {
        console.error('⚠️ Failed to save live config to disk:', e.message);
    }
}

// Periodically check PM2 status of rust-mev-arb-live
function checkPm2EngineState() {
    exec('pm2 jlist', (err, stdout) => {
        if (!err && stdout) {
            try {
                const list = JSON.parse(stdout);
                const liveProc = list.find(p => p.name === 'rust-mev-arb-live');
                if (liveProc && liveProc.pm2_env && liveProc.pm2_env.status === 'online') {
                    liveTradingConfig.is_engine_running = true;
                    liveTradingConfig.status_label = 'LIVE ACTIVE (Trading Real SOL)';
                } else {
                    liveTradingConfig.is_engine_running = false;
                    liveTradingConfig.status_label = 'PAUSED / STANDBY';
                }
            } catch (e) {}
        }
    });
}
setInterval(checkPm2EngineState, 4000);
checkPm2EngineState();

function fetchLiveSolPrice() {
    https.get('https://api.binance.com/api/v3/ticker/price?symbol=SOLUSDT', { headers: { 'User-Agent': 'Mozilla/5.0' } }, (res) => {
        let d = '';
        res.on('data', c => d += c);
        res.on('end', () => {
            try {
                const json = JSON.parse(d);
                if (json.price) {
                    liveSolPrice = parseFloat(json.price);
                }
            } catch (e) {}
        });
    }).on('error', () => {});
}
setInterval(fetchLiveSolPrice, 3000);
fetchLiveSolPrice();

function parseLogFile(filePath, initialBalance) {
    if (!fs.existsSync(filePath)) {
        return {
            initial_balance_sol: initialBalance,
            virtual_balance_sol: initialBalance,
            total_trades: 0,
            winning_trades: 0,
            failed_trades: 0,
            win_rate: '0.0',
            total_pnl_sol: 0.0,
            total_pnl_usd: 0.0,
            recent_trades: [],
            equity_history: [{ time: 'Start', balance: initialBalance }]
        };
    }

    try {
        const content = fs.readFileSync(filePath, 'utf-8');
        const lines = content.trim().split('\n').filter(Boolean);
        const rawTrades = lines.map(line => {
            try { return JSON.parse(line); } catch (e) { return null; }
        }).filter(Boolean);

        if (rawTrades.length === 0) {
            return {
                initial_balance_sol: initialBalance,
                virtual_balance_sol: initialBalance,
                total_trades: 0,
                winning_trades: 0,
                failed_trades: 0,
                win_rate: '0.0',
                total_pnl_sol: 0.0,
                total_pnl_usd: 0.0,
                recent_trades: [],
                equity_history: [{ time: 'Start', balance: initialBalance }]
            };
        }

        const wins = rawTrades.filter(t => t.live_status === 'LANDED_CONFIRMED' || (t.status === 'FILLED' && t.net_pnl_sol > 0));
        const fails = rawTrades.filter(t => (t.status && t.status !== 'FILLED') || (t.live_status && t.live_status !== 'LANDED_CONFIRMED'));
        const totalPnlSol = rawTrades.reduce((acc, t) => acc + (t.net_pnl_sol || t.expected_net_profit_sol || 0), 0);
        const totalPnlUsd = totalPnlSol * liveSolPrice;
        const currentBalance = initialBalance + totalPnlSol;
        const winRate = rawTrades.length > 0 ? ((wins.length / rawTrades.length) * 100).toFixed(1) : '0.0';

        const equityHistory = [
            { time: 'Start', balance: initialBalance }
        ];

        rawTrades.forEach((t, idx) => {
            equityHistory.push({
                time: t.iso_time ? t.iso_time.substring(11, 19) : `#${idx + 1}`,
                balance: Number((t.post_balance_sol || t.ending_wallet_balance_sol || (initialBalance + totalPnlSol)).toFixed(4))
            });
        });

        return {
            initial_balance_sol: initialBalance,
            virtual_balance_sol: Number(currentBalance.toFixed(4)),
            total_trades: rawTrades.length,
            winning_trades: wins.length,
            failed_trades: fails.length,
            win_rate: winRate,
            total_pnl_sol: Number(totalPnlSol.toFixed(4)),
            total_pnl_usd: Number(totalPnlUsd.toFixed(2)),
            recent_trades: rawTrades.slice(-50).reverse(),
            equity_history: equityHistory
        };
    } catch (e) {
        return {
            initial_balance_sol: initialBalance,
            virtual_balance_sol: initialBalance,
            total_trades: 0,
            winning_trades: 0,
            failed_trades: 0,
            win_rate: '0.0',
            total_pnl_sol: 0.0,
            total_pnl_usd: 0.0,
            recent_trades: [],
            equity_history: [{ time: 'Start', balance: initialBalance }]
        };
    }
}

// Global System Metrics
let systemState = {
    sol_price_usd: 88.50,
    jito_latency_ms: 11.3,
    server_uptime_sec: 0,
    cpu_usage_pct: 0,
    ram_usage_mb: 0,
    active_pools_count: 0,
    live_dex_monitor: [],
    events_per_sec: 0,
    total_events_processed: 0
};

let activeVolatileRegistry = new Map();

function runDiscoveryScout() {
    const endpoints = [
        'https://api.dexscreener.com/token-boosts/top/v1',
        'https://api.dexscreener.com/token-boosts/latest/v1',
        'https://api.dexscreener.com/token-profiles/latest/v1',
        'https://api.dexscreener.com/latest/dex/search?q=pump',
        'https://api.dexscreener.com/latest/dex/search?q=solana',
        'https://api.dexscreener.com/latest/dex/search?q=meteora'
    ];

    let candidateMints = new Set();

    endpoints.forEach(ep => {
        https.get(ep, { headers: { 'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)' } }, (res) => {
            let d = '';
            res.on('data', c => d += c);
            res.on('end', () => {
                try {
                    const json = JSON.parse(d);
                    if (Array.isArray(json)) {
                        json.forEach(it => {
                            if (it.chainId === 'solana' && it.tokenAddress) {
                                candidateMints.add(it.tokenAddress);
                            }
                        });
                    } else if (json.pairs && Array.isArray(json.pairs)) {
                        json.pairs.forEach(p => {
                            if (p.chainId === 'solana' && p.baseToken?.address) {
                                candidateMints.add(p.baseToken.address);
                            }
                        });
                    }

                    candidateMints.forEach(mint => {
                        https.get(`https://api.dexscreener.com/latest/dex/tokens/${mint}`, { headers: { 'User-Agent': 'Mozilla/5.0' } }, (tRes) => {
                            let td = '';
                            tRes.on('data', tc => td += tc);
                            tRes.on('end', () => {
                                try {
                                    const tJson = JSON.parse(td);
                                    const pairs = tJson.pairs || [];
                                    if (pairs.length === 0) return;

                                    let symbol = pairs[0]?.baseToken?.symbol || 'TOKEN';
                                    let name = pairs[0]?.baseToken?.name || 'Token';
                                    let total5mVol = 0;
                                    let totalLiq = 0;

                                    let pools = { raydium: null, meteora: null, pumpswap: null, orca: null };

                                    pairs.forEach(p => {
                                        const dex = (p.dexId || '').toLowerCase();
                                        const liq = p.liquidity?.usd || 0;
                                        const vol5m = p.volume?.m5 || 0;
                                        total5mVol += vol5m;
                                        totalLiq += liq;
                                        const price = parseFloat(p.priceUsd);

                                        if (liq >= 1000 && !isNaN(price) && price > 0 && price < 10000) {
                                            if (dex === 'raydium' && (!pools.raydium || liq > pools.raydium.liq)) {
                                                pools.raydium = { price, liq };
                                            } else if (dex === 'meteora' && (!pools.meteora || liq > pools.meteora.liq)) {
                                                pools.meteora = { price, liq };
                                            } else if (dex === 'pumpswap' && (!pools.pumpswap || liq > pools.pumpswap.liq)) {
                                                pools.pumpswap = { price, liq };
                                            } else if (dex === 'orca' && (!pools.orca || liq > pools.orca.liq)) {
                                                pools.orca = { price, liq };
                                            }
                                        }
                                    });

                                    const activePools = [
                                        { dex: 'Raydium', data: pools.raydium },
                                        { dex: 'Meteora', data: pools.meteora },
                                        { dex: 'PumpSwap', data: pools.pumpswap },
                                        { dex: 'Orca', data: pools.orca }
                                    ].filter(p => p.data !== null);

                                    if (activePools.length >= 2 && total5mVol >= 10) {
                                        activePools.sort((a, b) => a.data.price - b.data.price);
                                        const low = activePools[0];
                                        const high = activePools[activePools.length - 1];
                                        const grossSpread = ((high.data.price - low.data.price) / low.data.price) * 100;

                                        if (grossSpread <= 15.0) {
                                            const netSpread = grossSpread - 0.50;
                                            const entry = {
                                                time: new Date().toLocaleTimeString(),
                                                symbol,
                                                name,
                                                mint,
                                                raydiumPrice: pools.raydium ? `$${pools.raydium.price.toFixed(6)}` : '—',
                                                meteoraPrice: pools.meteora ? `$${pools.meteora.price.toFixed(6)}` : '—',
                                                pumpPrice: pools.pumpswap ? `$${pools.pumpswap.price.toFixed(6)}` : '—',
                                                orcaPrice: pools.orca ? `$${pools.orca.price.toFixed(6)}` : '—',
                                                bestRoute: `${low.dex} ➔ ${high.dex}`,
                                                grossSpreadPct: grossSpread.toFixed(2),
                                                netSpreadPct: (netSpread >= 0 ? '+' : '') + netSpread.toFixed(2),
                                                isProfitable: netSpread >= 0.40,
                                                totalLiquidity: `$${Math.round(totalLiq).toLocaleString()}`,
                                                volume5m: `$${Math.round(total5mVol).toLocaleString()}`
                                            };

                                            activeVolatileRegistry.set(mint, entry);
                                            systemState.live_dex_monitor = Array.from(activeVolatileRegistry.values());
                                            systemState.active_pools_count = systemState.live_dex_monitor.length;
                                            systemState.total_events_processed++;
                                            broadcast({ type: 'DEX_MONITOR_UPDATE', data: entry });
                                        }
                                    }
                                } catch (e) {}
                            });
                        }).on('error', () => {});
                    });
                } catch (e) {}
            });
        }).on('error', () => {});
    });
}

setInterval(runDiscoveryScout, 4000);
runDiscoveryScout();

setInterval(() => {
    systemState.server_uptime_sec = process.uptime();
    const mem = process.memoryUsage();
    systemState.ram_usage_mb = Math.round(mem.rss / 1024 / 1024);
    systemState.cpu_usage_pct = Math.round(os.loadavg()[0] * 10) / 10;
    systemState.sol_price_usd = liveSolPrice;
}, 3000);

function broadcast(data) {
    const msg = JSON.stringify(data);
    wss.clients.forEach(client => {
        if (client.readyState === WebSocket.OPEN) {
            client.send(msg);
        }
    });
}

if (fs.existsSync(DATA_DIR)) {
    fs.watch(DATA_DIR, (eventType, filename) => {
        if (filename && (filename.includes('paper_trades') || filename.includes('live_trades'))) {
            broadcast({ type: 'TRADES_UPDATE', payload: getAllStats() });
        }
    });
}

function getAllStats() {
    return {
        system: systemState,
        instance_micro: parseLogFile(PAPER_TRADES_MICRO_PATH, 0.38),
        instance_1sol: parseLogFile(PAPER_TRADES_1SOL_PATH, 1.0),
        instance_10sol: parseLogFile(PAPER_TRADES_10SOL_PATH, 10.0),
        instance_live: parseLogFile(LIVE_TRADES_PATH, liveTradingConfig.wallet_balance_sol),
        live_config: liveTradingConfig
    };
}

// === LIVE CONTROL API ENDPOINTS ===

app.get('/api/stats', (req, res) => {
    res.json(getAllStats());
});

app.get('/api/live/status', (req, res) => {
    res.json({
        config: liveTradingConfig,
        stats: parseLogFile(LIVE_TRADES_PATH, liveTradingConfig.wallet_balance_sol)
    });
});

app.post('/api/live/wallet', (req, res) => {
    const { publicKey } = req.body;
    if (publicKey && publicKey.length >= 32) {
        liveTradingConfig.wallet_pubkey = publicKey;
        saveLiveConfigToDisk();

        // Verify balance from public RPC
        const postData = JSON.stringify({
            jsonrpc: '2.0',
            id: 1,
            method: 'getBalance',
            params: [publicKey]
        });

        const reqRpc = https.request({
            hostname: 'api.mainnet-beta.solana.com',
            port: 443,
            path: '/',
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Content-Length': Buffer.byteLength(postData)
            }
        }, (rpcRes) => {
            let rd = '';
            rpcRes.on('data', c => rd += c);
            rpcRes.on('end', () => {
                try {
                    const rpcJson = JSON.parse(rd);
                    if (rpcJson.result && rpcJson.result.value !== undefined) {
                        const balSol = rpcJson.result.value / 1_000_000_000.0;
                        liveTradingConfig.wallet_balance_sol = Number(balSol.toFixed(4));
                        saveLiveConfigToDisk();
                    }
                } catch (e) {}
                broadcast({ type: 'TRADES_UPDATE', payload: getAllStats() });
                res.json({ success: true, config: liveTradingConfig });
            });
        });
        reqRpc.on('error', () => {
            res.json({ success: true, config: liveTradingConfig });
        });
        reqRpc.write(postData);
        reqRpc.end();
    } else {
        res.status(400).json({ error: 'Invalid Solana Public Key' });
    }
});

app.post('/api/live/config', (req, res) => {
    const { max_position_sol, min_spread_pct, min_net_profit_sol, jito_tip_pct } = req.body;
    if (max_position_sol !== undefined) liveTradingConfig.max_position_sol = parseFloat(max_position_sol);
    if (min_spread_pct !== undefined) liveTradingConfig.min_spread_pct = parseFloat(min_spread_pct);
    if (min_net_profit_sol !== undefined) liveTradingConfig.min_net_profit_sol = parseFloat(min_net_profit_sol);
    if (jito_tip_pct !== undefined) liveTradingConfig.jito_tip_pct = parseInt(jito_tip_pct);

    saveLiveConfigToDisk();
    broadcast({ type: 'TRADES_UPDATE', payload: getAllStats() });
    res.json({ success: true, config: liveTradingConfig });
});

app.post('/api/live/control', (req, res) => {
    const { action } = req.body;
    if (action === 'start') {
        liveTradingConfig.is_engine_running = true;
        liveTradingConfig.status_label = 'LIVE ACTIVE (Trading Real SOL)';
        saveLiveConfigToDisk();
        exec('cd /home/ubuntu/solana-mev-arb && (pm2 restart rust-mev-arb-live || pm2 start ./target/release/solana-mev-arb --name rust-mev-arb-live -- --live)', (err) => {
            broadcast({ type: 'TRADES_UPDATE', payload: getAllStats() });
            res.json({ success: true, status: liveTradingConfig.status_label });
        });
    } else if (action === 'stop' || action === 'kill') {
        liveTradingConfig.is_engine_running = false;
        liveTradingConfig.status_label = action === 'kill' ? 'EMERGENCY SHUTDOWN TRIGGERED' : 'PAUSED / STANDBY';
        saveLiveConfigToDisk();
        exec('cd /home/ubuntu/solana-mev-arb && pm2 stop rust-mev-arb-live', (err) => {
            broadcast({ type: 'TRADES_UPDATE', payload: getAllStats() });
            res.json({ success: true, status: liveTradingConfig.status_label });
        });
    } else {
        res.status(400).json({ error: 'Invalid control action' });
    }
});

app.get('/api/health', (req, res) => {
    res.json({ status: 'HEALTHY', region: 'eu-central-1 (Frankfurt)', solPrice: liveSolPrice });
});

wss.on('connection', (ws) => {
    ws.send(JSON.stringify({ type: 'INIT', payload: getAllStats() }));
});

server.listen(PORT, '0.0.0.0', () => {
    console.log(`🚀 Solana Live MEV Control Terminal running on http://0.0.0.0:${PORT}`);
});
