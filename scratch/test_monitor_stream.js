const https = require('https');

const discoveryEndpoints = [
    'https://api.dexscreener.com/token-boosts/top/v1',
    'https://api.dexscreener.com/token-boosts/latest/v1',
    'https://api.dexscreener.com/token-profiles/latest/v1',
    'https://api.dexscreener.com/latest/dex/search?q=solana',
    'https://api.dexscreener.com/latest/dex/search?q=raydium',
    'https://api.dexscreener.com/latest/dex/search?q=meteora',
    'https://api.dexscreener.com/latest/dex/search?q=pump',
    'https://api.dexscreener.com/latest/dex/search?q=orca'
];

let activeVolatileRegistry = new Map();

function runDynamicDiscoveryScout() {
    let completed = 0;
    discoveryEndpoints.forEach(ep => {
        https.get(ep, { headers: { 'User-Agent': 'Mozilla/5.0' } }, (res) => {
            let d = '';
            res.on('data', c => d += c);
            res.on('end', () => {
                completed++;
                try {
                    const json = JSON.parse(d);
                    let candidateMints = new Set();

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

                    const mintsArray = Array.from(candidateMints).slice(0, 30);
                    if (mintsArray.length > 0) {
                        https.get(`https://api.dexscreener.com/latest/dex/tokens/${mintsArray.join(',')}`, { headers: { 'User-Agent': 'Mozilla/5.0' } }, (tRes) => {
                            let td = '';
                            tRes.on('data', tc => td += tc);
                            tRes.on('end', () => {
                                try {
                                    const tJson = JSON.parse(td);
                                    const pairs = tJson.pairs || [];
                                    const mintPairsMap = new Map();
                                    pairs.forEach(p => {
                                        const m = p.baseToken?.address;
                                        if (!m) return;
                                        if (!mintPairsMap.has(m)) mintPairsMap.set(m, []);
                                        mintPairsMap.get(m).push(p);
                                    });

                                    mintPairsMap.forEach((tokenPairs, mint) => {
                                        let symbol = tokenPairs[0]?.baseToken?.symbol || 'TOKEN';
                                        let name = tokenPairs[0]?.baseToken?.name || 'Token';
                                        let total5mVol = 0;
                                        let totalLiq = 0;

                                        let pools = { raydium: null, meteora: null, pumpswap: null, orca: null };

                                        tokenPairs.forEach(p => {
                                            const dex = (p.dexId || '').toLowerCase();
                                            const liq = p.liquidity?.usd || 0;
                                            const vol5m = p.volume?.m5 || 0;
                                            total5mVol += vol5m;
                                            totalLiq += liq;
                                            const price = parseFloat(p.priceUsd);

                                            if (!isNaN(price) && price > 0 && price < 100000) {
                                                if (dex.includes('raydium') && (!pools.raydium || liq > pools.raydium.liq)) {
                                                    pools.raydium = { price, liq };
                                                } else if (dex.includes('meteora') && (!pools.meteora || liq > pools.meteora.liq)) {
                                                    pools.meteora = { price, liq };
                                                } else if ((dex.includes('pump') || dex.includes('moonshot')) && (!pools.pumpswap || liq > pools.pumpswap.liq)) {
                                                    pools.pumpswap = { price, liq };
                                                } else if (dex.includes('orca') && (!pools.orca || liq > pools.orca.liq)) {
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

                                        if (activePools.length >= 2) {
                                            activePools.sort((a, b) => a.data.price - b.data.price);
                                            const low = activePools[0];
                                            const high = activePools[activePools.length - 1];
                                            const grossSpread = ((high.data.price - low.data.price) / low.data.price) * 100;

                                            if (grossSpread <= 30.0 && grossSpread >= 0.10) {
                                                const netSpread = grossSpread - 0.50;
                                                const entry = {
                                                    symbol,
                                                    name,
                                                    mint,
                                                    bestRoute: `${low.dex} ➔ ${high.dex}`,
                                                    grossSpreadPct: grossSpread.toFixed(2),
                                                    netSpreadPct: (netSpread >= 0 ? '+' : '') + netSpread.toFixed(2),
                                                    totalLiquidity: `$${Math.round(totalLiq).toLocaleString()}`,
                                                    volume5m: `$${Math.round(total5mVol).toLocaleString()}`
                                                };
                                                activeVolatileRegistry.set(mint, entry);
                                            }
                                        }
                                    });
                                } catch (e) {}
                            });
                        });
                    }
                } catch (e) {}
            });
        });
    });
}

runDynamicDiscoveryScout();

setTimeout(() => {
    console.log(`✅ Discovered Tokens: ${activeVolatileRegistry.size}`);
    Array.from(activeVolatileRegistry.values()).slice(0, 15).forEach((t, i) => {
        console.log(`${i+1}. ${t.symbol} | Route: ${t.bestRoute} | Spread: ${t.grossSpreadPct}% | Liq: ${t.totalLiquidity}`);
    });
    process.exit(0);
}, 4000);
