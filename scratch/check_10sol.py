import json
import os

path = '/home/ubuntu/solana-mev-arb/data/paper_trades_10sol.ndjson'
if os.path.exists(path):
    with open(path, 'r', encoding='utf-8') as f:
        lines = [json.loads(line) for line in f if line.strip()]
    print(f"Total 10 SOL trades: {len(lines)}")
    if lines:
        print("First 3 trades:")
        for t in lines[:3]:
            print(f"  {t.get('iso_time')} | {t.get('symbol')} | Status: {t.get('status')} | Net PnL: {t.get('net_pnl_sol')} | Start: {t.get('starting_wallet_balance_sol')} | End: {t.get('ending_wallet_balance_sol')}")
        print("Last 3 trades:")
        for t in lines[-3:]:
            print(f"  {t.get('iso_time')} | {t.get('symbol')} | Status: {t.get('status')} | Net PnL: {t.get('net_pnl_sol')} | Start: {t.get('starting_wallet_balance_sol')} | End: {t.get('ending_wallet_balance_sol')}")
        
        # Check sum of net_pnl_sol
        total_pnl = sum(t.get('net_pnl_sol', 0) for t in lines)
        print(f"Calculated Total PnL: {total_pnl:.4f} SOL")
