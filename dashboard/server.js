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
const PAPER_TRADES_TRIANGULAR_PATH = path.join(DATA_DIR, 'paper_trades_triangular.ndjson');
const LIVE_TRADES_PATH = path.join(DATA_DIR, 'live_trades.ndjson');
const DUAL_LEG_LIVE_TRADES_PATH = path.join(DATA_DIR, 'dual_leg_live_trades.ndjson');
const TRIANGULAR_LIVE_TRADES_PATH = path.join(DATA_DIR, 'triangular_live_trades.ndjson');
const MIGRATION_SHADOW_TRADES_PATH = path.join(DATA_DIR, 'migration_shadow_trades.ndjson');

const DUAL_LEG_CONFIG_PATH = path.join(DATA_DIR, 'dual_leg_config.json');
const TRIANGULAR_CONFIG_PATH = path.join(DATA_DIR, 'triangular_config.json');
const LIVE_CONFIG_PATH = path.join(DATA_DIR, 'live_trading_config.json');
const TRACKED_TOKENS_PATH = path.join(DATA_DIR, 'tracked_tokens.json');

let trackedTokensCache = { mtimeMs: 0, data: [] };
function getTrackedTokens() {
    if (!fs.existsSync(TRACKED_TOKENS_PATH)) {
        return [];
    }
    try {
        const stats = fs.statSync(TRACKED_TOKENS_PATH);
        if (trackedTokensCache.data && trackedTokensCache.mtimeMs === stats.mtimeMs) {
            return trackedTokensCache.data;
        }
        const content = fs.readFileSync(TRACKED_TOKENS_PATH, 'utf-8');
        const parsed = JSON.parse(content);
        trackedTokensCache = { mtimeMs: stats.mtimeMs, data: parsed };
        return parsed;
    } catch (e) {
        return trackedTokensCache.data || [];
    }
}

app.use(express.static(path.join(__dirname, 'public')));
app.use(express.json());

let liveSolPrice = 93.50;

// === 1. DUAL-LEG ENGINE CONFIGURATION ===
let dualLegConfig = {
    wallet_pubkey: 'AA1ihbehbPrMW4L9jWtdxJfkH76Up5F2azbeepQ1wixs',
    wallet_balance_sol: 0.3981,
    is_engine_running: true,
    max_position_sol: 0.099,
    min_spread_pct: 0.50,
    min_net_profit_sol: 0.00015,
    max_slippage_pct: 0.35,
    jito_tip_pct: 35.0,
    reserve_floor_sol: 0.15,
    status_label: 'DUAL-LEG CONCENTRATED (Meteora DLMM & Orca)'
};

if (fs.existsSync(DUAL_LEG_CONFIG_PATH)) {
    try {
        const saved = JSON.parse(fs.readFileSync(DUAL_LEG_CONFIG_PATH, 'utf-8'));
        dualLegConfig = { ...dualLegConfig, ...saved };
    } catch (e) {}
}

function saveDualLegConfigToDisk() {
    try {
        if (!fs.existsSync(DATA_DIR)) fs.mkdirSync(DATA_DIR, { recursive: true });
        fs.writeFileSync(DUAL_LEG_CONFIG_PATH, JSON.stringify(dualLegConfig, null, 2));
    } catch (e) {
        console.error('⚠️ Failed to save dual-leg config:', e.message);
    }
}

// === 2. TRIANGULAR ENGINE CONFIGURATION ===
let triangularConfig = {
    wallet_pubkey: 'AA1ihbehbPrMW4L9jWtdxJfkH76Up5F2azbeepQ1wixs',
    wallet_balance_sol: 0.3981,
    is_engine_running: true,
    min_pool_tvl_usd: 10000.0,
    max_pool_tvl_usd: 60000.0,
    max_position_sol: 0.099,
    min_spread_pct: 0.60,
    min_net_profit_sol: 0.00015,
    max_compound_slippage_pct: 0.30,
    jito_tip_pct: 35.0,
    reserve_floor_sol: 0.15,
    intermediate_bridge: 'USDC',
    status_label: 'TRIANGULAR CONCENTRATED ($10k-$60k Goldilocks Zone)'
};

if (fs.existsSync(TRIANGULAR_CONFIG_PATH)) {
    try {
        const saved = JSON.parse(fs.readFileSync(TRIANGULAR_CONFIG_PATH, 'utf-8'));
        triangularConfig = { ...triangularConfig, ...saved };
    } catch (e) {}
}

function saveTriangularConfigToDisk() {
    try {
        if (!fs.existsSync(DATA_DIR)) fs.mkdirSync(DATA_DIR, { recursive: true });
        fs.writeFileSync(TRIANGULAR_CONFIG_PATH, JSON.stringify(triangularConfig, null, 2));
    } catch (e) {
        console.error('⚠️ Failed to save triangular config:', e.message);
    }
}

// V40 FIX: the on-chain-balance poller used to rewrite BOTH engine config files
// wholesale from memory every poll tick. A dashboard instance that started before
// an out-of-band config change would silently revert the files (and thus the live
// engines' sizing/tips) forever. Now it patches only wallet_balance_sol on disk.
function updateBalancesOnDisk() {
    const targets = [
        [DUAL_LEG_CONFIG_PATH, dualLegConfig.wallet_balance_sol],
        [TRIANGULAR_CONFIG_PATH, triangularConfig.wallet_balance_sol]
    ];
    for (const [configPath, balance] of targets) {
        try {
            if (!fs.existsSync(configPath)) continue;
            const current = JSON.parse(fs.readFileSync(configPath, 'utf-8'));
            if (current && typeof current === 'object') {
                current.wallet_balance_sol = balance;
                fs.writeFileSync(configPath, JSON.stringify(current, null, 2));
            }
        } catch (e) {
            console.error(`⚠️ Failed to patch balance in ${configPath}:`, e.message);
        }
    }
}

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

function fetchOnChainBalance(pubkey) {
    if (!pubkey || pubkey.length < 32) return;
    const postData = JSON.stringify({
        jsonrpc: '2.0',
        id: 1,
        method: 'getBalance',
        params: [pubkey, { commitment: 'confirmed' }]
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
                    dualLegConfig.wallet_balance_sol = Number(balSol.toFixed(4));
                    triangularConfig.wallet_balance_sol = Number(balSol.toFixed(4));
                    updateBalancesOnDisk();
                    broadcast({ type: 'TRADES_UPDATE', payload: getAllStats() });
                }
            } catch (e) {}
        });
    });
    reqRpc.on('error', () => {});
    reqRpc.write(postData);
    reqRpc.end();
}

setInterval(() => {
    if (dualLegConfig.wallet_pubkey) {
        fetchOnChainBalance(dualLegConfig.wallet_pubkey);
    }
}, 5000);
fetchOnChainBalance(dualLegConfig.wallet_pubkey);

const logFileCache = new Map();

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
        const stats = fs.statSync(filePath);
        const cached = logFileCache.get(filePath);
        if (cached && cached.mtimeMs === stats.mtimeMs) {
            return cached.data;
        }

        const content = fs.readFileSync(filePath, 'utf-8');
        const lines = content.trim().split('\n').filter(Boolean);
        const rawTrades = lines.map(line => {
            try { return JSON.parse(line); } catch (e) { return null; }
        }).filter(Boolean);

        if (rawTrades.length === 0) {
            const res = {
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
            logFileCache.set(filePath, { mtimeMs: stats.mtimeMs, data: res });
            return res;
        }

        const wins = rawTrades.filter(t => t.live_status === 'LANDED_CONFIRMED' || (t.status === 'FILLED' && (t.net_pnl_sol !== undefined ? t.net_pnl_sol > 0 : false)));
        const fails = rawTrades.filter(t => (t.status && t.status !== 'FILLED') || (t.live_status && t.live_status !== 'LANDED_CONFIRMED'));
        
        const lastTrade = rawTrades[rawTrades.length - 1];
        const currentBalance = Number((lastTrade.ending_wallet_balance_sol || lastTrade.post_balance_sol || initialBalance).toFixed(4));
        const totalPnlSol = Number((currentBalance - initialBalance).toFixed(4));
        const totalPnlUsd = Number((totalPnlSol * liveSolPrice).toFixed(2));
        const winRate = rawTrades.length > 0 ? ((wins.length / rawTrades.length) * 100).toFixed(1) : '0.0';

        const equityHistory = [
            { time: 'Start', balance: initialBalance }
        ];

        const step = Math.max(1, Math.floor(rawTrades.length / 100));
        rawTrades.forEach((t, idx) => {
            if (idx % step === 0 || idx === rawTrades.length - 1) {
                equityHistory.push({
                    time: t.iso_time ? t.iso_time.substring(11, 19) : `#${idx + 1}`,
                    balance: Number((t.ending_wallet_balance_sol || t.post_balance_sol || currentBalance).toFixed(4))
                });
            }
        });

        const result = {
            initial_balance_sol: initialBalance,
            virtual_balance_sol: currentBalance,
            total_trades: rawTrades.length,
            winning_trades: wins.length,
            failed_trades: fails.length,
            win_rate: winRate,
            total_pnl_sol: totalPnlSol,
            total_pnl_usd: totalPnlUsd,
            recent_trades: rawTrades.slice(-50).reverse(),
            equity_history: equityHistory
        };

        logFileCache.set(filePath, { mtimeMs: stats.mtimeMs, data: result });
        return result;
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

function getDualLegStats() {
    const raw = parseLogFile(DUAL_LEG_LIVE_TRADES_PATH, 0.3981);
    const realOnChainBal = Number((dualLegConfig.wallet_balance_sol || 0.3981).toFixed(4));
    const realPnlSol = Number((realOnChainBal - 0.3981).toFixed(4));
    const realPnlUsd = Number((realPnlSol * liveSolPrice).toFixed(2));

    return {
        initial_balance_sol: 0.3981,
        virtual_balance_sol: realOnChainBal,
        total_trades: raw.total_trades,
        winning_trades: raw.winning_trades,
        failed_trades: raw.failed_trades,
        win_rate: raw.win_rate,
        total_pnl_sol: realPnlSol,
        total_pnl_usd: realPnlUsd,
        recent_trades: raw.recent_trades,
        config: dualLegConfig,
        equity_history: [
            { time: 'Start', balance: 0.3981 },
            { time: 'Live', balance: realOnChainBal }
        ]
    };
}

function getTriangularStats() {
    const raw = parseLogFile(TRIANGULAR_LIVE_TRADES_PATH, 0.3981);
    const realOnChainBal = Number((triangularConfig.wallet_balance_sol || 0.3981).toFixed(4));
    const realPnlSol = Number((realOnChainBal - 0.3981).toFixed(4));
    const realPnlUsd = Number((realPnlSol * liveSolPrice).toFixed(2));

    return {
        initial_balance_sol: 0.3981,
        virtual_balance_sol: realOnChainBal,
        total_trades: raw.total_trades,
        winning_trades: raw.winning_trades,
        failed_trades: raw.failed_trades,
        win_rate: raw.win_rate,
        total_pnl_sol: realPnlSol,
        total_pnl_usd: realPnlUsd,
        recent_trades: raw.recent_trades,
        config: triangularConfig,
        equity_history: [
            { time: 'Start', balance: 0.3981 },
            { time: 'Live', balance: realOnChainBal }
        ]
    };
}

let migCache = { mtimeMs: 0, data: null };
function parseMigrationShadowLog() {
    if (!fs.existsSync(MIGRATION_SHADOW_TRADES_PATH)) {
        return {
            total_migrations_evaluated: 0,
            accepted_for_sniping: 0,
            rejected_by_shields: 0,
            completed_shadow_trades: 0,
            winning_shadow_trades: 0,
            win_rate: '0.0',
            total_shadow_pnl_sol: 0.0,
            total_shadow_pnl_usd: 0.0,
            counterfactual_alpha_saved_sol: 0.0,
            counterfactual_alpha_saved_usd: 0.0,
            recent_evaluations: []
        };
    }

    try {
        const stats = fs.statSync(MIGRATION_SHADOW_TRADES_PATH);
        if (migCache.data && migCache.mtimeMs === stats.mtimeMs) {
            return migCache.data;
        }

        const content = fs.readFileSync(MIGRATION_SHADOW_TRADES_PATH, 'utf-8');
        const lines = content.trim().split('\n').filter(Boolean);
        const records = lines.map(l => {
            try { return JSON.parse(l); } catch (e) { return null; }
        }).filter(Boolean);

        let accepted = 0;
        let rejected = 0;
        let completed = 0;
        let wins = 0;
        let totalPnlSol = 0.0;
        let counterfactualSavedSol = 0.0;

        for (const r of records) {
            if (r.decision === 'Accepted') {
                accepted++;
                if (r.pnl_sol !== undefined && r.pnl_sol !== null) {
                    completed++;
                    totalPnlSol += r.pnl_sol;
                    if (r.pnl_sol > 0) wins++;
                }
            } else {
                rejected++;
                if (r.counterfactual_pnl_sol !== undefined && r.counterfactual_pnl_sol < 0) {
                    counterfactualSavedSol += Math.abs(r.counterfactual_pnl_sol);
                }
            }
        }

        const result = {
            total_migrations_evaluated: records.length,
            accepted_for_sniping: accepted,
            rejected_by_shields: rejected,
            completed_shadow_trades: completed,
            winning_shadow_trades: wins,
            win_rate: completed > 0 ? ((wins / completed) * 100).toFixed(1) : '0.0',
            total_shadow_pnl_sol: Number(totalPnlSol.toFixed(4)),
            total_shadow_pnl_usd: Number((totalPnlSol * liveSolPrice).toFixed(2)),
            counterfactual_alpha_saved_sol: Number(counterfactualSavedSol.toFixed(4)),
            counterfactual_alpha_saved_usd: Number((counterfactualSavedSol * liveSolPrice).toFixed(2)),
            recent_evaluations: records.slice(-50).reverse()
        };

        migCache = { mtimeMs: stats.mtimeMs, data: result };
        return result;
    } catch (e) {
        return {
            total_migrations_evaluated: 0,
            accepted_for_sniping: 0,
            rejected_by_shields: 0,
            completed_shadow_trades: 0,
            winning_shadow_trades: 0,
            win_rate: '0.0',
            total_shadow_pnl_sol: 0.0,
            total_shadow_pnl_usd: 0.0,
            counterfactual_alpha_saved_sol: 0.0,
            counterfactual_alpha_saved_usd: 0.0,
            recent_evaluations: []
        };
    }
}

function getAllStats() {
    return {
        timestamp_ms: Date.now(),
        sol_price_usd: liveSolPrice,
        dual_leg: getDualLegStats(),
        triangular: getTriangularStats(),
        micro: parseLogFile(PAPER_TRADES_MICRO_PATH, 0.3800),
        '1sol': parseLogFile(PAPER_TRADES_1SOL_PATH, 1.0000),
        '10sol': parseLogFile(PAPER_TRADES_10SOL_PATH, 10.0000),
        migration: parseMigrationShadowLog(),
        tracked_tokens: getTrackedTokens(),
        system: {
            platform: os.platform(),
            uptime_sec: Math.floor(os.uptime()),
            loadavg: os.loadavg ? os.loadavg() : [0, 0, 0],
            memory_used_mb: Math.floor((os.totalmem() - os.freemem()) / (1024 * 1024)),
            memory_total_mb: Math.floor(os.totalmem() / (1024 * 1024)),
            cpu_percent: 0.5
        }
    };
}

function broadcast(msg) {
    const data = JSON.stringify(msg);
    wss.clients.forEach(client => {
        if (client.readyState === WebSocket.OPEN) {
            client.send(data);
        }
    });
}

wss.on('connection', ws => {
    ws.send(JSON.stringify({ type: 'INIT', payload: getAllStats() }));
});

setInterval(() => {
    broadcast({ type: 'TRADES_UPDATE', payload: getAllStats() });
}, 1500);

// === REST API ENDPOINTS ===

app.get('/api/overview', (req, res) => {
    res.json(getAllStats());
});

app.get('/api/tokens', (req, res) => {
    res.json(getTrackedTokens());
});

// Dual-Leg Endpoints
app.get('/api/dual/config', (req, res) => {
    res.json(dualLegConfig);
});

app.post('/api/dual/config', (req, res) => {
    const { max_position_sol, min_spread_pct, min_net_profit_sol, max_slippage_pct, jito_tip_pct, reserve_floor_sol, wallet_pubkey } = req.body;
    if (max_position_sol !== undefined) dualLegConfig.max_position_sol = parseFloat(max_position_sol);
    if (min_spread_pct !== undefined) dualLegConfig.min_spread_pct = parseFloat(min_spread_pct);
    if (min_net_profit_sol !== undefined) dualLegConfig.min_net_profit_sol = parseFloat(min_net_profit_sol);
    if (max_slippage_pct !== undefined) dualLegConfig.max_slippage_pct = parseFloat(max_slippage_pct);
    if (jito_tip_pct !== undefined) dualLegConfig.jito_tip_pct = parseFloat(jito_tip_pct);
    if (reserve_floor_sol !== undefined) dualLegConfig.reserve_floor_sol = parseFloat(reserve_floor_sol);
    if (wallet_pubkey) {
        dualLegConfig.wallet_pubkey = wallet_pubkey;
        fetchOnChainBalance(wallet_pubkey);
    }
    saveDualLegConfigToDisk();
    broadcast({ type: 'DUAL_CONFIG_UPDATE', payload: dualLegConfig });
    res.json({ success: true, config: dualLegConfig });
});

app.post('/api/dual/control', (req, res) => {
    const { action } = req.body;
    if (action === 'start') {
        dualLegConfig.is_engine_running = true;
        dualLegConfig.status_label = 'DUAL-LEG ACTIVE (SOL ➔ Token ➔ SOL)';
        exec('pm2 restart rust-mev-arb-dual-live || pm2 restart rust-mev-arb-live', () => {});
    } else if (action === 'stop' || action === 'pause') {
        dualLegConfig.is_engine_running = false;
        dualLegConfig.status_label = 'DUAL-LEG PAUSED';
        exec('pm2 stop rust-mev-arb-dual-live || pm2 stop rust-mev-arb-live', () => {});
    } else if (action === 'emergency_stop') {
        dualLegConfig.is_engine_running = false;
        dualLegConfig.status_label = 'EMERGENCY HALT (Standby)';
        exec('pm2 stop rust-mev-arb-dual-live', () => {});
    }
    saveDualLegConfigToDisk();
    broadcast({ type: 'DUAL_CONFIG_UPDATE', payload: dualLegConfig });
    res.json({ success: true, config: dualLegConfig });
});

// Triangular Endpoints
app.get('/api/triangular/config', (req, res) => {
    res.json(triangularConfig);
});

app.post('/api/triangular/config', (req, res) => {
    const { min_pool_tvl_usd, max_pool_tvl_usd, max_position_sol, min_spread_pct, min_net_profit_sol, max_compound_slippage_pct, jito_tip_pct, reserve_floor_sol, intermediate_bridge } = req.body;
    if (min_pool_tvl_usd !== undefined) triangularConfig.min_pool_tvl_usd = parseFloat(min_pool_tvl_usd);
    if (max_pool_tvl_usd !== undefined) triangularConfig.max_pool_tvl_usd = parseFloat(max_pool_tvl_usd);
    if (max_position_sol !== undefined) triangularConfig.max_position_sol = parseFloat(max_position_sol);
    if (min_spread_pct !== undefined) triangularConfig.min_spread_pct = parseFloat(min_spread_pct);
    if (min_net_profit_sol !== undefined) triangularConfig.min_net_profit_sol = parseFloat(min_net_profit_sol);
    if (max_compound_slippage_pct !== undefined) triangularConfig.max_compound_slippage_pct = parseFloat(max_compound_slippage_pct);
    if (jito_tip_pct !== undefined) triangularConfig.jito_tip_pct = parseFloat(jito_tip_pct);
    if (reserve_floor_sol !== undefined) triangularConfig.reserve_floor_sol = parseFloat(reserve_floor_sol);
    if (intermediate_bridge) triangularConfig.intermediate_bridge = intermediate_bridge;
    saveTriangularConfigToDisk();
    broadcast({ type: 'TRIANGULAR_CONFIG_UPDATE', payload: triangularConfig });
    res.json({ success: true, config: triangularConfig });
});

app.post('/api/triangular/control', (req, res) => {
    const { action } = req.body;
    if (action === 'start') {
        triangularConfig.is_engine_running = true;
        triangularConfig.status_label = 'TRIANGULAR ACTIVE ($3k-$40k Whale Moat)';
        exec('pm2 restart rust-mev-arb-triangular-live || pm2 restart rust-mev-arb-triangular', () => {});
    } else if (action === 'stop' || action === 'pause') {
        triangularConfig.is_engine_running = false;
        triangularConfig.status_label = 'TRIANGULAR PAUSED';
        exec('pm2 stop rust-mev-arb-triangular-live || pm2 stop rust-mev-arb-triangular', () => {});
    } else if (action === 'emergency_stop') {
        triangularConfig.is_engine_running = false;
        triangularConfig.status_label = 'EMERGENCY HALT (Standby)';
        exec('pm2 stop rust-mev-arb-triangular-live || pm2 stop rust-mev-arb-triangular', () => {});
    }
    saveTriangularConfigToDisk();
    broadcast({ type: 'TRIANGULAR_CONFIG_UPDATE', payload: triangularConfig });
    res.json({ success: true, config: triangularConfig });
});

// ============================================================================
// V42 GLOBAL KILL SWITCH — sub-second trading halt, no process churn.
// Creates/deletes data/KILL_SWITCH; every engine process watches that file at
// 250ms resolution and gates dispatch BEFORE signing. Works even if this
// dashboard is down:  touch data/KILL_SWITCH   (halt) / rm data/KILL_SWITCH (resume)
// ============================================================================
const KILL_SWITCH_PATH = path.join(DATA_DIR, 'KILL_SWITCH');

app.get('/api/killswitch', (req, res) => {
    res.json({ engaged: fs.existsSync(KILL_SWITCH_PATH) });
});

app.post('/api/killswitch', (req, res) => {
    const { engaged } = req.body;
    try {
        if (engaged) {
            fs.writeFileSync(KILL_SWITCH_PATH, new Date().toISOString());
            dualLegConfig.is_engine_running = false;
            triangularConfig.is_engine_running = false;
            dualLegConfig.status_label = '⛔ GLOBAL KILL SWITCH ENGAGED';
            triangularConfig.status_label = '⛔ GLOBAL KILL SWITCH ENGAGED';
        } else {
            if (fs.existsSync(KILL_SWITCH_PATH)) fs.unlinkSync(KILL_SWITCH_PATH);
            dualLegConfig.is_engine_running = true;
            triangularConfig.is_engine_running = true;
            dualLegConfig.status_label = 'DUAL-LEG ACTIVE V42 REFLEX (Event-Driven | Multi-Region | Fire-First)';
            triangularConfig.status_label = 'TRIANGULAR ACTIVE V42 REFLEX (Event-Driven | Multi-Region | Fire-First)';
        }
        saveDualLegConfigToDisk();
        saveTriangularConfigToDisk();
        broadcast({ type: 'KILLSWITCH_UPDATE', payload: { engaged: !!engaged } });
        console.log(`${engaged ? '⛔' : '✅'} Kill switch ${engaged ? 'ENGAGED' : 'RELEASED'} by operator`);
        res.json({ success: true, engaged: !!engaged });
    } catch (e) {
        console.error('Kill switch toggle failed:', e.message);
        res.status(500).json({ success: false, error: e.message });
    }
});

// Backward compatibility endpoints
app.get('/api/live/config', (req, res) => {
    res.json(dualLegConfig);
});

app.post('/api/live/config', (req, res) => {
    res.json(dualLegConfig);
});

app.post('/api/live/control', (req, res) => {
    res.json({ success: true });
});

server.listen(PORT, '0.0.0.0', () => {
    console.log(`⚡ Decoupled Solana MEV Arbitrage Dashboard running on http://0.0.0.0:${PORT}`);
});
