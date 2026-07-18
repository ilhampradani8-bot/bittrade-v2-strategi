import pandas as pd
import numpy as np
import statsmodels.api as sm
from statsmodels.regression.rolling import RollingOLS
import time

def process_combination(df, interval_str, window_size, z_entry=2.0, z_exit=0.2, fee=0.0016):
    print(f"\nProcessing {interval_str} window {window_size}...")
    
    # Resample
    if interval_str != '1m':
        df_resampled = df.resample(interval_str).last().dropna()
    else:
        df_resampled = df.copy()
        
    df_resampled['ln_price_a'] = np.log(df_resampled['price_a'])
    df_resampled['ln_price_b'] = np.log(df_resampled['price_b'])
    
    # Rolling OLS (ln_price_a = alpha + beta * ln_price_b)
    endog = df_resampled['ln_price_a']
    exog = sm.add_constant(df_resampled['ln_price_b'])
    
    # We use statsmodels RollingOLS
    rols = RollingOLS(endog, exog, window=window_size)
    rres = rols.fit()
    
    params = rres.params
    df_resampled['alpha'] = params['const']
    df_resampled['beta'] = params['ln_price_b']
    
    # Calculate residuals
    df_resampled['residual'] = df_resampled['ln_price_a'] - (df_resampled['alpha'] + df_resampled['beta'] * df_resampled['ln_price_b'])
    
    # Calculate rolling std of residuals
    df_resampled['std_err'] = df_resampled['residual'].rolling(window=window_size).std()
    
    # Calculate rolling mean of residuals (should be close to 0 but let's calculate it)
    df_resampled['rolling_mean'] = df_resampled['residual'].rolling(window=window_size).mean()
    
    # Z-Score
    df_resampled['z_score'] = (df_resampled['residual'] - df_resampled['rolling_mean']) / df_resampled['std_err']
    
    # R2 calculation is complex for rolling, we can approximate it or skip since statsmodels provides it?
    # R2 = 1 - (SSR / SST)
    # Actually for simplicity let's assume R2 > 0.85 pass rate. Since we just want to know the edge.
    # To keep it exact, we can use rsquared from statsmodels if available, but RollingOLS doesn't expose it easily in vectorized way without looping.
    # We will simulate the R2 logic: SST = variance of ln_price_a * (window-1), SSR = variance of residuals * (window-1)
    df_resampled['var_a'] = df_resampled['ln_price_a'].rolling(window=window_size).var()
    df_resampled['var_res'] = df_resampled['residual'].rolling(window=window_size).var()
    df_resampled['r2'] = 1 - (df_resampled['var_res'] / df_resampled['var_a'])
    
    df_valid = df_resampled.dropna().copy()
    
    # Filter R2 >= 0.85
    r2_pass_rate = (df_valid['r2'] >= 0.85).mean() * 100
    
    # Calculate Implied Edge
    reversion_distance = z_entry - z_exit
    df_valid['implied_edge'] = reversion_distance * df_valid['std_err']
    
    # Forward Validation
    # Find points where abs(z_score) >= z_entry AND R2 >= 0.85 AND implied_edge > fee * buffer
    buffer = 1.0 # default
    signals = df_valid[(df_valid['z_score'].abs() >= z_entry) & 
                       (df_valid['r2'] >= 0.85) & 
                       (df_valid['implied_edge'] > fee * buffer)]
    
    # Filter overlapping signals (only take first signal in a sequence)
    # Simple way: iterate
    trades = []
    in_position = False
    entry_idx = None
    entry_z = None
    entry_dir = 0
    
    for idx, row in df_valid.iterrows():
        if not in_position:
            if row['z_score'] >= z_entry and row['r2'] >= 0.85 and row['implied_edge'] > fee * buffer:
                in_position = True
                entry_z = row['z_score']
                entry_dir = 1
                entry_idx = idx
            elif row['z_score'] <= -z_entry and row['r2'] >= 0.85 and row['implied_edge'] > fee * buffer:
                in_position = True
                entry_z = row['z_score']
                entry_dir = -1
                entry_idx = idx
        else:
            # check exit
            if entry_dir == 1 and row['z_score'] <= z_exit:
                gross_pnl_ratio = (entry_z - row['z_score']) * row['std_err']
                net_pnl = gross_pnl_ratio - fee
                trades.append(net_pnl)
                in_position = False
            elif entry_dir == -1 and row['z_score'] >= -z_exit:
                gross_pnl_ratio = (abs(entry_z) - abs(row['z_score'])) * row['std_err']
                net_pnl = gross_pnl_ratio - fee
                trades.append(net_pnl)
                in_position = False
            # check stop loss (e.g. z_score > 4.0)
            elif (entry_dir == 1 and row['z_score'] >= 4.0) or (entry_dir == -1 and row['z_score'] <= -4.0):
                gross_pnl_ratio = (abs(entry_z) - 4.0) * row['std_err'] # negative
                net_pnl = gross_pnl_ratio - fee
                trades.append(net_pnl)
                in_position = False
                
    win_rate = 0
    avg_net_pnl = 0
    if len(trades) > 0:
        win_rate = np.mean(np.array(trades) > 0) * 100
        avg_net_pnl = np.mean(trades) * 100
        
    return {
        'interval': interval_str,
        'window': window_size,
        'r2_pass_rate': r2_pass_rate,
        'median_std_err': df_valid['std_err'].median() * 100,
        'p90_std_err': df_valid['std_err'].quantile(0.9) * 100,
        'signals_count': len(signals),
        'trades_executed': len(trades),
        'win_rate': win_rate,
        'avg_net_pnl': avg_net_pnl
    }

def main():
    btc = pd.read_csv('btc_1m.csv', parse_dates=['open_time'], index_col='open_time')
    eth = pd.read_csv('eth_1m.csv', parse_dates=['open_time'], index_col='open_time')
    
    # Merge on index
    df = btc.join(eth, lsuffix='_btc', rsuffix='_eth', how='inner')
    df = df.rename(columns={'close_eth': 'price_a', 'close_btc': 'price_b'})
    df = df[['price_a', 'price_b']]
    
    combinations = [
        ('1min', 60), ('1min', 240), ('1min', 720),
        ('5min', 96), ('5min', 288),
        ('15min', 96), ('15min', 384)
    ]
    
    results = []
    for interval, window in combinations:
        res = process_combination(df, interval, window)
        results.append(res)
        
    res_df = pd.DataFrame(results)
    print("\n\n--- SWEEP RESULTS ---")
    print(res_df.to_markdown(index=False))
    
    with open('sweep_results.md', 'w') as f:
        f.write(res_df.to_markdown(index=False))

if __name__ == "__main__":
    main()
