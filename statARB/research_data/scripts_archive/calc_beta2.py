import pandas as pd
import numpy as np

def calculate_ols_beta(y, x):
    if len(y) < 10: return None
    mean_x = np.mean(x)
    mean_y = np.mean(y)
    var_x = np.sum((x - mean_x)**2)
    if var_x < 1e-12: return None
    return np.sum((x - mean_x) * (y - mean_y)) / var_x

try:
    btc = pd.read_csv("statARB/btc_1m.csv")
    eth = pd.read_csv("statARB/eth_1m.csv")
    btc['open_time'] = pd.to_datetime(btc['open_time'])
    eth['open_time'] = pd.to_datetime(eth['open_time'])
    btc_5m = btc.set_index('open_time').resample('5min').agg({'close': 'last'}).dropna()
    eth_5m = eth.set_index('open_time').resample('5min').agg({'close': 'last'}).dropna()
    df = btc_5m.join(eth_5m, lsuffix='_btc', rsuffix='_eth', how='inner')
    df['log_btc'] = np.log(df['close_btc'])
    df['log_eth'] = np.log(df['close_eth'])
    betas = []
    window = 96
    for i in range(window, len(df)):
        window_df = df.iloc[i-window:i]
        beta = calculate_ols_beta(window_df['log_eth'].values, window_df['log_btc'].values)
        if beta is not None: betas.append(beta)
    
    print(f"5th Percentile Beta: {np.percentile(betas, 5):.4f}")
    print(f"95th Percentile Beta: {np.percentile(betas, 95):.4f}")
except Exception as e:
    print(f"Error: {e}")
