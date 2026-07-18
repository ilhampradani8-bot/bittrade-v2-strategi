import pandas as pd
import numpy as np
import json

def calculate_ols_beta(y, x):
    if len(y) < 10: return None
    mean_x, mean_y = np.mean(x), np.mean(y)
    var_x = np.sum((x - mean_x)**2)
    if var_x < 1e-12: return None
    return np.sum((x - mean_x) * (y - mean_y)) / var_x

def evaluate_subperiod(df_sub, combinations):
    results = []
    
    BTC_STEP_DOLLAR = 62.0
    ETH_MIN_NOTIONAL = 20.0
    POSITION_SIZE_USDT = 130.0
    FEE_RATE = 0.0010
    EV_BUFFER_MULTIPLIER = 2.5
    HURDLE = FEE_RATE * EV_BUFFER_MULTIPLIER
    
    for interval_str, window_size in combinations:
        
        # Resample
        if interval_str != '1m':
            df_resampled = df_sub.resample(interval_str).last().dropna()
        else:
            df_resampled = df_sub.copy()
            
        df_resampled['log_btc'] = np.log(df_resampled['price_b'])
        df_resampled['log_eth'] = np.log(df_resampled['price_a'])
        
        z_entry = 2.0
        z_exit = 0.2
        sl_threshold = 4.0
        
        trades = 0
        rejected = 0
        wins = 0
        total_net_pnl = 0.0
        
        in_pos = False
        pos_dir = 0
        entry_price_a = 0
        entry_price_b = 0
        qty_a = 0
        qty_b = 0
        
        std_err_list = []
        
        for i in range(window_size, len(df_resampled)):
            window_df = df_resampled.iloc[i-window_size:i]
            beta = calculate_ols_beta(window_df['log_eth'].values, window_df['log_btc'].values)
            if beta is None: continue
            
            alpha = np.mean(window_df['log_eth'].values) - beta * np.mean(window_df['log_btc'].values)
            res = window_df['log_eth'].values - (beta * window_df['log_btc'].values + alpha)
            std_err = np.sqrt(np.sum(res**2) / len(res))
            std_err_list.append(std_err)
            
            curr_y = df_resampled['log_eth'].iloc[i]
            curr_x = df_resampled['log_btc'].iloc[i]
            curr_spread = curr_y - beta * curr_x
            z = (curr_spread - alpha) / std_err if std_err > 0 else 0
            
            if not in_pos:
                if abs(z) > z_entry and std_err > HURDLE:
                    abs_beta = abs(beta)
                    w_btc = (abs_beta * POSITION_SIZE_USDT) / (1 + abs_beta)
                    w_eth = POSITION_SIZE_USDT / (1 + abs_beta)
                    
                    if w_btc < BTC_STEP_DOLLAR or w_eth < ETH_MIN_NOTIONAL:
                        rejected += 1
                        continue
                        
                    in_pos = True
                    pos_dir = -np.sign(z)
                    entry_price_a = df_resampled['price_a'].iloc[i]
                    entry_price_b = df_resampled['price_b'].iloc[i]
                    qty_a = w_eth / entry_price_a
                    qty_b = w_btc / entry_price_b
            else:
                hit_exit = (pos_dir == 1 and z > -z_exit) or (pos_dir == -1 and z < z_exit)
                hit_sl = (pos_dir == 1 and z < -sl_threshold) or (pos_dir == -1 and z > sl_threshold)
                
                if hit_exit or hit_sl:
                    curr_price_a = df_resampled['price_a'].iloc[i]
                    curr_price_b = df_resampled['price_b'].iloc[i]
                    
                    # Exact dollar PnL calculation
                    if pos_dir == 1:
                        # BUY_SPREAD: Long ETH, Short BTC
                        pnl_a = qty_a * (curr_price_a - entry_price_a)
                        pnl_b = qty_b * (entry_price_b - curr_price_b)
                    else:
                        # SELL_SPREAD: Short ETH, Long BTC
                        pnl_a = qty_a * (entry_price_a - curr_price_a)
                        pnl_b = qty_b * (curr_price_b - entry_price_b)
                        
                    gross_usd = pnl_a + pnl_b
                    gross_pct = gross_usd / POSITION_SIZE_USDT
                    net = gross_pct - FEE_RATE
                    
                    trades += 1
                    total_net_pnl += net
                    if net > 0: wins += 1
                    in_pos = False

        win_rate = (wins/trades*100) if trades > 0 else 0
        avg_net = (total_net_pnl/trades*100) if trades > 0 else 0
        total_signals = trades + rejected
        rejection_rate = (rejected/total_signals*100) if total_signals > 0 else 0
        
        median_se = np.median(std_err_list) * 100 if len(std_err_list) > 0 else 0
        
        results.append({
            'interval': interval_str,
            'window': window_size,
            'median_std_err': median_se,
            'signals': total_signals,
            'rejection_rate': rejection_rate,
            'trades': trades,
            'win_rate': win_rate,
            'avg_net_pnl': avg_net,
            'total_net_pnl': total_net_pnl * 100
        })
        
    return results

def main():
    btc = pd.read_csv('statARB/btc_1m.csv', parse_dates=['open_time'], index_col='open_time')
    eth = pd.read_csv('statARB/eth_1m.csv', parse_dates=['open_time'], index_col='open_time')
    
    df = btc.join(eth, lsuffix='_btc', rsuffix='_eth', how='inner')
    df = df.rename(columns={'close_eth': 'price_a', 'close_btc': 'price_b'})
    df = df[['price_a', 'price_b']]
    
    combinations = [
        ('1min', 60), ('1min', 240), ('1min', 720),
        ('5min', 96), ('5min', 288),
        ('15min', 96), ('15min', 384)
    ]
    
    total_days = (df.index[-1] - df.index[0]).days
    print(f"Total days in dataset: {total_days}")
    
    # Split into 3 sub-periods
    period_length = len(df) // 3
    df_p1 = df.iloc[:period_length]
    df_p2 = df.iloc[period_length:2*period_length]
    df_p3 = df.iloc[2*period_length:]
    
    dfs = [df_p1, df_p2, df_p3]
    
    all_results = []
    
    for i, sub_df in enumerate(dfs):
        print(f"\nProcessing Sub-Period {i+1} ({len(sub_df)} rows, ~{len(sub_df)//(24*60)} days)...")
        res = evaluate_subperiod(sub_df, combinations)
        for r in res:
            r['period'] = f'P{i+1}'
        all_results.extend(res)
        
    df_res = pd.DataFrame(all_results)
    print("\n--- WALK-FORWARD SWEEP RESULTS ---")
    print(df_res.to_string(index=False))
    
    df_res.to_csv("statARB/sweep2_results.csv", index=False)

if __name__ == "__main__":
    main()
