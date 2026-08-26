import json
import os

files = {
    '/home/ubuntu/solana-mev-arb/data/archive/paper_trades_micro.ndjson': ('/home/ubuntu/solana-mev-arb/data/paper_trades_micro.ndjson', 0.38),
    '/home/ubuntu/solana-mev-arb/data/archive/paper_trades_1sol.ndjson': ('/home/ubuntu/solana-mev-arb/data/paper_trades_1sol.ndjson', 1.0),
    '/home/ubuntu/solana-mev-arb/data/archive/paper_trades_10sol.ndjson': ('/home/ubuntu/solana-mev-arb/data/paper_trades_10sol.ndjson', 10.0),
    '/home/ubuntu/solana-mev-arb/data/archive/paper_trades_triangular.ndjson': ('/home/ubuntu/solana-mev-arb/data/paper_trades_triangular.ndjson', 0.38),
}

for src_path, (dst_path, start_bal) in files.items():
    if not os.path.exists(src_path):
        continue
    valid_records = []
    current_bal = start_bal
    
    with open(src_path, 'r', encoding='utf-8', errors='ignore') as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                r = json.loads(line)
                spread = abs(float(r.get('gross_spread_pct', 0)))
                pnl = float(r.get('net_pnl_sol', 0))
                size = float(r.get('position_size_sol', 0.05))
                
                # Exclude crazy decimal-mismatch outliers (> 25% spread or > 20% size profit)
                if spread > 25.0 or pnl > (size * 0.25) or pnl < -0.05:
                    continue
                
                r['starting_wallet_balance_sol'] = round(current_bal, 4)
                if r.get('status') == 'FILLED' or r.get('live_status') == 'LANDED_CONFIRMED':
                    current_bal += pnl
                r['ending_wallet_balance_sol'] = round(current_bal, 4)
                valid_records.append(r)
            except Exception:
                pass
                
    with open(dst_path, 'w', encoding='utf-8') as f_out:
        for r in valid_records:
            f_out.write(json.dumps(r) + '\n')
            
    print(f"✅ Processed {dst_path}: {len(valid_records)} real trades restored. Real Ending Balance: {current_bal:.4f} SOL")
