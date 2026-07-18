import pandas as pd
import numpy as np
import json
import os

def calculate_ols_beta(y, x):
    if len(y) < 10: return None
    mean_x, mean_y = np.mean(x), np.mean(y)
    var_x = np.sum((x - mean_x)**2)
    if var_x < 1e-12: return None
    return np.sum((x - mean_x) * (y - mean_y)) / var_x

def evaluate_subperiod(df_sub, combinations, step_a, step_b, min_a, min_b):
    results = []
    
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
            
        df_resampled['log_b'] = np.log(df_resampled['price_b'])
        df_resampled['log_a'] = np.log(df_resampled['price_a'])
        
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
            beta = calculate_ols_beta(window_df['log_a'].values, window_df['log_b'].values)
            if beta is None: continue
            
            alpha = np.mean(window_df['log_a'].values) - beta * np.mean(window_df['log_b'].values)
            res = window_df['log_a'].values - (beta * window_df['log_b'].values + alpha)
            std_err = np.sqrt(np.sum(res**2) / len(res))
            std_err_list.append(std_err)
            
            curr_y = df_resampled['log_a'].iloc[i]
            curr_x = df_resampled['log_b'].iloc[i]
            curr_spread = curr_y - beta * curr_x
            z = (curr_spread - alpha) / std_err if std_err > 0 else 0
            
            if not in_pos:
                if abs(z) > z_entry and std_err > HURDLE:
                    abs_beta = abs(beta)
                    w_b = (abs_beta * POSITION_SIZE_USDT) / (1 + abs_beta)
                    w_a = POSITION_SIZE_USDT / (1 + abs_beta)
                    
                    price_a = df_resampled['price_a'].iloc[i]
                    price_b = df_resampled['price_b'].iloc[i]
                    
                    # Exchange limits check
                    raw_qty_a = w_a / price_a
                    raw_qty_b = w_b / price_b
                    
                    # Floor to stepSize
                    real_qty_a = np.floor(raw_qty_a / step_a) * step_a
                    real_qty_b = np.floor(raw_qty_b / step_b) * step_b
                    
                    val_a = real_qty_a * price_a
                    val_b = real_qty_b * price_b
                    
                    if val_a < min_a or val_b < min_b:
                        rejected += 1
                        continue
                        
                    in_pos = True
                    pos_dir = -np.sign(z)
                    entry_price_a = price_a
                    entry_price_b = price_b
                    qty_a = real_qty_a
                    qty_b = real_qty_b
            else:
                hit_exit = (pos_dir == 1 and z > -z_exit) or (pos_dir == -1 and z < z_exit)
                hit_sl = (pos_dir == 1 and z < -sl_threshold) or (pos_dir == -1 and z > sl_threshold)
                
                if hit_exit or hit_sl:
                    curr_price_a = df_resampled['price_a'].iloc[i]
                    curr_price_b = df_resampled['price_b'].iloc[i]
                    
                    # Exact dollar PnL calculation
                    if pos_dir == 1:
                        # BUY_SPREAD: Long A, Short B
                        pnl_a = qty_a * (curr_price_a - entry_price_a)
                        pnl_b = qty_b * (entry_price_b - curr_price_b)
                    else:
                        # SELL_SPREAD: Short A, Long B
                        pnl_a = qty_a * (entry_price_a - curr_price_a)
                        pnl_b = qty_b * (curr_price_b - entry_price_b)
                        
                    gross_usd = pnl_a + pnl_b
                    
                    # Exact total invested
                    invested = (qty_a * entry_price_a) + (qty_b * entry_price_b)
                    if invested <= 0: invested = 1.0
                    
                    gross_pct = gross_usd / invested
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
    with open("statARB/altcoin_limits.json", "r") as f:
        limits = json.load(f)
        
    pairs = [
        ("UNIUSDT", "AAVEUSDT"),
        ("1000PEPEUSDT", "1000FLOKIUSDT"),
        ("SOLUSDT", "AVAXUSDT")
    ]
    
    combinations = [
        ('1min', 720), # 12h
        ('5min', 96),  # 8h
        ('5min', 288), # 24h
        ('15min', 96), # 24h
    ]
    
    for sym_a, sym_b in pairs:
        print(f"\n==============================================")
        print(f"RUNNING SWEEP FOR {sym_a} / {sym_b}")
        print(f"==============================================")
        
        df_a = pd.read_csv(f'statARB/{sym_a}_1m.csv', parse_dates=['open_time'], index_col='open_time')
        df_b = pd.read_csv(f'statARB/{sym_b}_1m.csv', parse_dates=['open_time'], index_col='open_time')
        
        df = df_a.join(df_b, lsuffix='_a', rsuffix='_b', how='inner')
        df = df.rename(columns={'close_a': 'price_a', 'close_b': 'price_b'})
        df = df[['price_a', 'price_b']]
        
        step_a = limits[sym_a]["stepSize"]
        step_b = limits[sym_b]["stepSize"]
        min_a = limits[sym_a]["minNotional"]
        min_b = limits[sym_b]["minNotional"]
        
        period_length = len(df) // 3
        dfs = [df.iloc[:period_length], df.iloc[period_length:2*period_length], df.iloc[2*period_length:]]
        
        all_results = []
        for i, sub_df in enumerate(dfs):
            res = evaluate_subperiod(sub_df, combinations, step_a, step_b, min_a, min_b)
            for r in res:
                r['period'] = f'P{i+1}'
            all_results.extend(res)
            
        df_res = pd.DataFrame(all_results)
        print(df_res.to_string(index=False))

if __name__ == "__main__":
    main()
