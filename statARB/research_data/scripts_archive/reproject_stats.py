import pandas as pd
import numpy as np
import json

def calculate_ols_beta(y, x):
    if len(y) < 10: return None
    mean_x, mean_y = np.mean(x), np.mean(y)
    var_x = np.sum((x - mean_x)**2)
    if var_x < 1e-12: return None
    return np.sum((x - mean_x) * (y - mean_y)) / var_x

btc = pd.read_csv("statARB/btc_1m.csv")
eth = pd.read_csv("statARB/eth_1m.csv")
btc['open_time'] = pd.to_datetime(btc['open_time'])
eth['open_time'] = pd.to_datetime(eth['open_time'])

btc_5m = btc.set_index('open_time').resample('5min').agg({'close': 'last'}).dropna()
eth_5m = eth.set_index('open_time').resample('5min').agg({'close': 'last'}).dropna()

df = btc_5m.join(eth_5m, lsuffix='_btc', rsuffix='_eth', how='inner')
df['log_btc'] = np.log(df['close_btc'])
df['log_eth'] = np.log(df['close_eth'])

BTC_STEP_DOLLAR = 62.0
ETH_MIN_NOTIONAL = 20.0

def run_simulation(fee_rate, position_size_usdt, apply_rejection=True):
    EV_BUFFER_MULTIPLIER = 2.5
    HURDLE = fee_rate * EV_BUFFER_MULTIPLIER
    
    trades = 0
    rejected_trades = 0
    total_net = 0.0
    wins = 0
    window = 96
    z_entry = 2.0
    z_exit = 0.2
    
    in_pos = False
    pos_dir = 0
    entry_spread = 0
    entry_beta = 0
    
    for i in range(window, len(df)):
        window_df = df.iloc[i-window:i]
        beta = calculate_ols_beta(window_df['log_eth'].values, window_df['log_btc'].values)
        if beta is None: continue
        
        alpha = np.mean(window_df['log_eth'].values) - beta * np.mean(window_df['log_btc'].values)
        res = window_df['log_eth'].values - (beta * window_df['log_btc'].values + alpha)
        std_err = np.sqrt(np.sum(res**2) / len(res))
        
        curr_y = df['log_eth'].iloc[i]
        curr_x = df['log_btc'].iloc[i]
        curr_spread = curr_y - beta * curr_x
        z = (curr_spread - alpha) / std_err if std_err > 0 else 0
        
        if not in_pos:
            if abs(z) > z_entry and std_err > HURDLE:
                # Dynamic allocation check
                w_btc = (abs(beta) * position_size_usdt) / (1 + abs(beta))
                w_eth = position_size_usdt / (1 + abs(beta))
                
                if apply_rejection and (w_btc < BTC_STEP_DOLLAR or w_eth < ETH_MIN_NOTIONAL):
                    rejected_trades += 1
                    continue
                
                in_pos = True
                pos_dir = -np.sign(z)
                entry_spread = curr_spread
                entry_beta = beta
        else:
            # We must use entry_beta to calculate exit spread to measure actual PnL of the fixed position
            exit_spread = curr_y - entry_beta * curr_x
            
            # Note: the z-score used for exit decision should ideally also be relative to entry parameters,
            # but we use the rolling z-score here as per original script.
            if (pos_dir == 1 and z > -z_exit) or (pos_dir == -1 and z < z_exit):
                gross = (exit_spread - entry_spread) * pos_dir
                gross_pct = gross / (1 + abs(entry_beta))
                net = gross_pct - fee_rate
                
                trades += 1
                total_net += net
                if net > 0: wins += 1
                
                in_pos = False
                
    win_rate = (wins/trades*100) if trades > 0 else 0
    total_signals = trades + rejected_trades
    rejection_rate = (rejected_trades / total_signals * 100) if total_signals > 0 else 0
    return {
        "trades": trades,
        "rejected": rejected_trades,
        "rejection_rate": rejection_rate,
        "win_rate": win_rate,
        "avg_net_pct": (total_net/trades*100 if trades > 0 else 0),
        "total_net_pct": total_net*100
    }

print("FEE RECONCILIATION AND BACKTEST")
# Standard VIP0 Fee is 0.10% (0.0010) round-trip total for BOTH legs combined.
fee_vip0 = 0.0010

# Skenario A: Size 130, no rejection (Optimis/Lama)
res_a = run_simulation(fee_vip0, 130.0, apply_rejection=False)
print(f"Scenario A (Optimistic, $130, No Rejection):")
print(f"  Trades: {res_a['trades']} | WinRate: {res_a['win_rate']:.1f}% | Avg Net: {res_a['avg_net_pct']:.3f}%")

# Skenario B: Size 130, with rejection (Realistis)
res_b = run_simulation(fee_vip0, 130.0, apply_rejection=True)
print(f"Scenario B (Realistic, $130, With Rejection):")
print(f"  Trades: {res_b['trades']} | Rejected: {res_b['rejected']} ({res_b['rejection_rate']:.1f}%) | WinRate: {res_b['win_rate']:.1f}% | Avg Net: {res_b['avg_net_pct']:.3f}%")

# Skenario C: Size 183, with rejection (Worst-case floor)
res_c = run_simulation(fee_vip0, 183.0, apply_rejection=True)
print(f"Scenario C (Floor $183, With Rejection):")
print(f"  Trades: {res_c['trades']} | Rejected: {res_c['rejected']} ({res_c['rejection_rate']:.1f}%) | WinRate: {res_c['win_rate']:.1f}% | Avg Net: {res_c['avg_net_pct']:.3f}%")
