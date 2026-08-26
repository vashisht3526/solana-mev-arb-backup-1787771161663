import json

path = '/home/ubuntu/solana-mev-arb/data/paper_trades_10sol.ndjson'
with open(path, 'r', encoding='utf-8') as f:
    lines = [json.loads(line) for line in f if line.strip()]

print(f"Total lines: {len(lines)}")
outliers = [t for t in lines if t.get('net_pnl_sol', 0) > 0.05 or t.get('net_pnl_sol', 0) < -0.05]
print(f"Found {len(outliers)} outlier trades!")
for t in outliers[:10]:
    print(f"  Outlier: {t.get('symbol')} | Spread: {t.get('gross_spread_pct')}% | PnL: {t.get('net_pnl_sol')} SOL | Status: {t.get('status')}")
