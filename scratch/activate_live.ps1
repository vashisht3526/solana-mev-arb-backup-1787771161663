$SSH_KEY = "C:\Users\Vashisht\.ssh\solana_aws_key"
$REMOTE_HOST = "ubuntu@52.58.226.98"

$script = @'
python3 -c "
import json
cfg = {
    'wallet_pubkey': 'AA1ihbehbPrMW4L9jWtdxJfkH76Up5F2azbeepQ1wixs',
    'wallet_balance_sol': 0.4003,
    'is_engine_running': True,
    'max_position_sol': 0.05,
    'min_spread_pct': 0.50,
    'min_net_profit_sol': 0.00030,
    'jito_tip_pct': 25,
    'reserve_floor_sol': 0.10,
    'max_drawdown_sol': 0.05,
    'status_label': 'LIVE ACTIVE (Trading Real SOL)'
}
with open('/home/ubuntu/solana-mev-arb/data/live_trading_config.json', 'w') as f:
    json.dump(cfg, f, indent=2)
print('Config updated successfully')
"
pm2 restart all
'@

$script | ssh -i $SSH_KEY $REMOTE_HOST "bash -s"
