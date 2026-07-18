import pandas as pd
import numpy as np

def calculate_ols_beta(y, x):
    if len(y) < 10: return None
    mean_x, mean_y = np.mean(x), np.mean(y)
    var_x = np.sum((x - mean_x)**2)
    if var_x < 1e-12: return None
    return np.sum((x - mean_x) * (y - mean_y)) / var_x

btc = pd.read_csv("btc_1m.csv")
eth = pd.read_csv("eth_1m.csv")
btc['open_time'] = pd.to_datetime(btc['open_time'])
eth['open_time'] = pd.to_datetime(eth['open_time'])

btc_5m = btc.set_index('open_time').resample('5min').agg({'close': 'last'}).dropna()
eth_5m = eth.set_index('open_time').resample('5min').agg({'close': 'last'}).dropna()

df = btc_5m.join(eth_5m, lsuffix='_btc', rsuffix='_eth', how='inner')
df['log_btc'] = np.log(df['close_btc'])
df['log_eth'] = np.log(df['close_eth'])

for fee_rate in [0.0016, 0.0010, 0.0009]:
    EV_BUFFER_MULTIPLIER = 2.5
    HURDLE = fee_rate * EV_BUFFER_MULTIPLIER
    
    trades = 0
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
                in_pos = True
                pos_dir = -np.sign(z)
                entry_spread = curr_spread
                entry_beta = beta
        else:
            if (pos_dir == 1 and z > -z_exit) or (pos_dir == -1 and z < z_exit):
                gross = (curr_spread - entry_spread) * pos_dir
                gross_pct = gross / (1 + entry_beta)
                net = gross_pct - fee_rate
                
                trades += 1
                total_net += net
                if net > 0: wins += 1
                
                in_pos = False
                
    win_rate = (wins/trades*100) if trades > 0 else 0
    print(f"Fee: {fee_rate*100:.2f}% | Trades: {trades} | WinRate: {win_rate:.1f}% | Avg Net: {(total_net/trades*100 if trades > 0 else 0):.3f}% | Total Net: {total_net*100:.2f}%")

