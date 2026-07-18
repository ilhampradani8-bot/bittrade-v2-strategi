import pandas as pd
import numpy as np
import statsmodels.api as sm
from statsmodels.regression.rolling import RollingOLS
import urllib.request
import json
import time

def fetch_exchange_limits(symbol):
    url = "https://fapi.binance.com/fapi/v1/exchangeInfo"
    req = urllib.request.Request(url)
    with urllib.request.urlopen(req) as response:
        data = json.loads(response.read().decode())
        for sym in data["symbols"]:
            if sym["symbol"] == symbol:
                step_size = None
                min_notional = 5.0
                for filter in sym["filters"]:
                    if filter["filterType"] == "LOT_SIZE":
                        step_size = float(filter["stepSize"])
                    elif filter["filterType"] == "MIN_NOTIONAL":
                        min_notional = float(filter["notional"])
                return step_size, min_notional
    return None, None

def fetch_historical_klines(symbol, interval="1m", days=30):
    limit = 1500
    total_bars = days * 24 * 60 if interval == "1m" else days * 24 * 12
    calls_needed = int(np.ceil(total_bars / limit))
    
    end_time = int(time.time() * 1000)
    all_klines = []
    
    for i in range(calls_needed):
        url = f"https://fapi.binance.com/fapi/v1/klines?symbol={symbol}&interval={interval}&limit={limit}&endTime={end_time}"
        try:
            req = urllib.request.Request(url)
            with urllib.request.urlopen(req) as response:
                data = json.loads(response.read().decode())
                if not data: break
                all_klines = data + all_klines
                end_time = data[0][0] - 1
                time.sleep(0.05)
        except Exception as e:
            print(f"Error fetching {symbol}: {e}")
            break
            
    df = pd.DataFrame(all_klines, columns=['open_time', 'open', 'high', 'low', 'close', 'volume', 'close_time', 'qav', 'num_trades', 'taker_base_vol', 'taker_quote_vol', 'ignore'])
    df['close'] = df['close'].astype(float)
    df['open_time'] = pd.to_datetime(df['open_time'], unit='ms')
    return df[['open_time', 'close']].drop_duplicates('open_time').set_index('open_time')

def run_simulation(df, step_size_a, step_size_b, min_notional_a, min_notional_b, start_idx, end_idx):
    fee_rate = 0.001 # 0.10% total round-trip per leg
    window = 120 # using a wider window (2 hours) as previously redesigned
    z_entry = 2.0
    z_exit = 0.2
    sl_sigma = 4.0
    capital = 200.0
    
    # Slice the period
    sub_df = df.iloc[start_idx:end_idx].copy()
    if len(sub_df) < window + 50: return {"trades": 0, "win_rate": 0, "net_pnl_usd": 0}
    
    # Calculate OLS
    y = np.log(sub_df['close_a'])
    x = np.log(sub_df['close_b'])
    endog = y
    exog = sm.add_constant(x)
    rols = RollingOLS(endog, exog, window=window)
    rres = rols.fit()
    sub_df['alpha'] = rres.params['const']
    sub_df['beta'] = rres.params['close_b']
    sub_df['res'] = y - (sub_df['alpha'] + sub_df['beta'] * x)
    sub_df['std_err'] = sub_df['res'].rolling(window).std()
    sub_df['z_score'] = sub_df['res'] / sub_df['std_err']
    
    in_position = False
    pos_type = 0
    entry_price_a = 0
    entry_price_b = 0
    qty_a = 0
    qty_b = 0
    entry_z = 0
    
    trades = 0
    wins = 0
    total_net_pnl = 0
    
    for i in range(window, len(sub_df)):
        row = sub_df.iloc[i]
        z = row['z_score']
        
        if pd.isna(z): continue
        
        if not in_position:
            if abs(z) > z_entry and abs(z) < sl_sigma:
                # Calculate quantities
                price_a = row['close_a']
                price_b = row['close_b']
                beta = row['beta']
                
                # Position sizing for $200 capital
                target_pos_usd = capital * 0.4 # max 40% per leg -> $80
                pos_a_usd = min(target_pos_usd, max(20.0, min_notional_a))
                pos_b_usd = min(pos_a_usd * abs(beta), target_pos_usd)
                
                if pos_b_usd < min_notional_b:
                    pos_b_usd = min_notional_b
                    pos_a_usd = pos_b_usd / abs(beta)
                    
                if pos_a_usd > target_pos_usd or pos_b_usd > target_pos_usd:
                    continue # Exchange limits prevent entry
                    
                raw_qty_a = pos_a_usd / price_a
                raw_qty_b = pos_b_usd / price_b
                
                qty_a = round(raw_qty_a / step_size_a) * step_size_a
                qty_b = round(raw_qty_b / step_size_b) * step_size_b
                
                if qty_a * price_a < min_notional_a or qty_b * price_b < min_notional_b:
                    continue
                
                in_position = True
                pos_type = 1 if z > 0 else -1
                entry_price_a = price_a
                entry_price_b = price_b
                entry_z = z
        else:
            price_a = row['close_a']
            price_b = row['close_b']
            
            # Exit logic
            exit_signal = False
            if pos_type == 1 and (z < z_exit or z > sl_sigma): exit_signal = True
            if pos_type == -1 and (z > -z_exit or z < -sl_sigma): exit_signal = True
            
            if exit_signal:
                # Calculate PnL (absolute $)
                if pos_type == 1:
                    pnl_a = (entry_price_a - price_a) * qty_a # Short A
                    pnl_b = (price_b - entry_price_b) * qty_b # Long B
                else:
                    pnl_a = (price_a - entry_price_a) * qty_a # Long A
                    pnl_b = (entry_price_b - price_b) * qty_b # Short B
                    
                gross_pnl = pnl_a + pnl_b
                
                # Fee calculation
                trade_val_a = entry_price_a * qty_a + price_a * qty_a
                trade_val_b = entry_price_b * qty_b + price_b * qty_b
                total_fee = (trade_val_a + trade_val_b) * (fee_rate / 2) # fee_rate is round-trip
                
                net_pnl = gross_pnl - total_fee
                
                trades += 1
                total_net_pnl += net_pnl
                if net_pnl > 0: wins += 1
                
                in_position = False
                
    return {
        "trades": trades,
        "win_rate": (wins / trades * 100) if trades > 0 else 0,
        "net_pnl_usd": total_net_pnl
    }

def main():
    df_res = pd.read_csv("statARB/stage1_screening_results.csv")
    passed_pairs = df_res[df_res['Passed'] == True].head(5)
    
    print("Stage 2 Deep Validation (Walk-Forward)")
    print(f"Testing top 5 passed candidates: {passed_pairs['Pair'].tolist()}")
    
    results = []
    
    for idx, row in passed_pairs.iterrows():
        pair_name = row['Pair']
        sym_a, sym_b = pair_name.split(" / ")
        
        print(f"\nFetching data for {sym_a} and {sym_b}...")
        step_a, min_a = fetch_exchange_limits(sym_a)
        step_b, min_b = fetch_exchange_limits(sym_b)
        
        df_a = fetch_historical_klines(sym_a, "1m", 30)
        df_b = fetch_historical_klines(sym_b, "1m", 30)
        
        if df_a.empty or df_b.empty: continue
        
        df = df_a.join(df_b, lsuffix='_a', rsuffix='_b', how='inner')
        total_bars = len(df)
        period_len = total_bars // 3
        
        print(f"Testing {pair_name} across 3 periods (Total Bars: {total_bars})...")
        p1 = run_simulation(df, step_a, step_b, min_a, min_b, 0, period_len)
        p2 = run_simulation(df, step_a, step_b, min_a, min_b, period_len, period_len*2)
        p3 = run_simulation(df, step_a, step_b, min_a, min_b, period_len*2, total_bars)
        
        positive_periods = 0
        if p1['net_pnl_usd'] > 0: positive_periods += 1
        if p2['net_pnl_usd'] > 0: positive_periods += 1
        if p3['net_pnl_usd'] > 0: positive_periods += 1
        
        is_lulus = positive_periods >= 2 and p1['trades'] >= 15 and p2['trades'] >= 15 and p3['trades'] >= 15
        
        results.append({
            "Pair": pair_name,
            "P1_Trades": p1['trades'], "P1_Net$": round(p1['net_pnl_usd'], 2),
            "P2_Trades": p2['trades'], "P2_Net$": round(p2['net_pnl_usd'], 2),
            "P3_Trades": p3['trades'], "P3_Net$": round(p3['net_pnl_usd'], 2),
            "Status": "LULUS" if is_lulus else "TUNDA"
        })
        
    res_df = pd.DataFrame(results)
    print("\n\nFinal Stage 2 Decision:")
    print(res_df.to_markdown(index=False))

if __name__ == "__main__":
    main()
