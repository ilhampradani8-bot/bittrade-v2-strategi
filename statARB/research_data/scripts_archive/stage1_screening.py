import urllib.request
import json
import pandas as pd
import numpy as np
import time
import os
import statsmodels.api as sm
from statsmodels.regression.rolling import RollingOLS
import warnings
warnings.filterwarnings('ignore')

def fetch_exchange_info():
    url = "https://fapi.binance.com/fapi/v1/exchangeInfo"
    req = urllib.request.Request(url)
    symbols = []
    with urllib.request.urlopen(req) as response:
        data = json.loads(response.read().decode())
        for sym in data["symbols"]:
            if sym["status"] == "TRADING" and sym["quoteAsset"] == "USDT" and sym["contractType"] == "PERPETUAL":
                symbols.append(sym["symbol"])
    return symbols

def fetch_klines(symbol, interval="15m", limit=1440):
    url = f"https://fapi.binance.com/fapi/v1/klines?symbol={symbol}&interval={interval}&limit={limit}"
    try:
        req = urllib.request.Request(url)
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode())
            if not data: return None
            df = pd.DataFrame(data, columns=['open_time', 'open', 'high', 'low', 'close', 'volume', 'close_time', 'qav', 'num_trades', 'taker_base_vol', 'taker_quote_vol', 'ignore'])
            df['close'] = df['close'].astype(float)
            df['open_time'] = pd.to_datetime(df['open_time'], unit='ms')
            return df[['open_time', 'close']].set_index('open_time')
    except Exception as e:
        print(f"Error fetching {symbol}: {e}")
        return None

def analyze_pair(df_a, df_b, window=96):
    df = df_a.join(df_b, lsuffix='_a', rsuffix='_b', how='inner')
    if len(df) < window * 2: return None, None
    
    y = np.log(df['close_a'])
    x = np.log(df['close_b'])
    
    # Calculate global R2
    beta_global = np.cov(x, y)[0,1] / np.var(x)
    alpha_global = np.mean(y) - beta_global * np.mean(x)
    res_global = y - (beta_global * x + alpha_global)
    r2 = 1 - (np.var(res_global) / np.var(y)) if np.var(y) > 0 else 0
    
    # Calculate rolling OLS
    endog = y
    exog = sm.add_constant(x)
    rols = RollingOLS(endog, exog, window=window)
    rres = rols.fit()
    
    params = rres.params
    df['alpha'] = params['const']
    df['beta'] = params['close_b']
    
    df['res'] = y - (df['alpha'] + df['beta'] * x)
    df['std_err'] = df['res'].rolling(window=window).std()
    
    median_std_err = df['std_err'].median()
    return r2, median_std_err

def main():
    print("Fetching active symbols...")
    all_symbols = fetch_exchange_info()
    all_symbols = [s for s in all_symbols if s not in ["BTCUSDT", "ETHUSDT"]]
    print(f"Found {len(all_symbols)} altcoin pairs.")
    
    print("Fetching BTC and ETH benchmarks...")
    df_btc = fetch_klines("BTCUSDT", "15m", 1440)
    df_eth = fetch_klines("ETHUSDT", "15m", 1440)
    
    results = []
    
    for i, sym in enumerate(all_symbols):
        if i % 20 == 0:
            print(f"Processing {i}/{len(all_symbols)}...")
        df_alt = fetch_klines(sym, "15m", 1440)
        if df_alt is None: continue
        
        # Test against BTC
        r2_btc, se_btc = analyze_pair(df_alt, df_btc)
        if r2_btc is not None:
            implied_edge_btc = (se_btc * 1.8) * 100 # In percentage
            results.append({
                "Pair": f"{sym} / BTCUSDT",
                "R2": r2_btc,
                "Median Std_Err (%)": se_btc * 100,
                "Implied Edge (%)": implied_edge_btc,
                "Passed": (r2_btc >= 0.85 and implied_edge_btc > 0.15)
            })
            
        # Test against ETH
        r2_eth, se_eth = analyze_pair(df_alt, df_eth)
        if r2_eth is not None:
            implied_edge_eth = (se_eth * 1.8) * 100
            results.append({
                "Pair": f"{sym} / ETHUSDT",
                "R2": r2_eth,
                "Median Std_Err (%)": se_eth * 100,
                "Implied Edge (%)": implied_edge_eth,
                "Passed": (r2_eth >= 0.85 and implied_edge_eth > 0.15)
            })
            
        time.sleep(0.05) # Rate limit protection
        
    df_res = pd.DataFrame(results)
    df_res = df_res.sort_values("Implied Edge (%)", ascending=False)
    
    passed_count = df_res['Passed'].sum()
    print(f"\nStage 1 Screening Complete. Tested {len(df_res)} combinations.")
    print(f"Passed strict filter (R2 >= 0.85 & Implied Edge > 0.15%): {passed_count}")
    
    df_res.to_csv("statARB/stage1_screening_results.csv", index=False)
    
    print("\nTop 20 Pairs:")
    print(df_res.head(20).to_markdown(index=False))
    print("\nBottom 10 Pairs:")
    print(df_res.tail(10).to_markdown(index=False))
    
if __name__ == "__main__":
    main()
