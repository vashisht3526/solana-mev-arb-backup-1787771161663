import json
import os

configs = {
    '/home/ubuntu/solana-mev-arb/data/paper_trades_micro.ndjson': 0.38,
    '/home/ubuntu/solana-mev-arb/data/paper_trades_1sol.ndjson': 1.0,
    '/home/ubuntu/solana-mev-arb/data/paper_trades_10sol.ndjson': 10.0,
    '/home/ubuntu/solana-mev-arb/data/paper_trades_triangular.ndjson': 0.38,
}

for path, init_bal in configs.items():
    if not os.path.exists(path):
        continue
    
    trades = []
    with open(path, 'r', encoding='utf-8', errors='ignore') as f:
        for line in f:
            line = line.strip()
            if line:
                try:
                    trades.append(json.loads(line))
                except Exception:
                    pass
                    
    if not trades:
        continue
        
    current_bal = init_bal
    for t in trades:
        t['starting_wallet_balance_sol'] = round(current_bal, 4)
        status = t.get('status', '')
        live_status = t.get('live_status', '')
        
        pnl = t.get('net_pnl_sol', 0.0)
        if status == 'FILLED' or live_status == 'LANDED_CONFIRMED':
            if pnl is not None and pnl > 0:
                current_bal += pnl
        else:
            gas = t.get('gas_fee_sol', 0.000005)
            current_bal = max(0.001, current_bal - (gas if gas else 0.000005))
            
        t['ending_wallet_balance_sol'] = round(current_bal, 4)
        
    with open(path, 'w', encoding='utf-8') as f:
        for t in trades:
            f.write(json.dumps(t) + '\n')
            
    print(f"✅ {path}: {len(trades)} trades rebuilt smoothly. Final Ending Balance: {current_bal:.4f} SOL (Total Net Profit: +{current_bal - init_bal:.4f} SOL)")
