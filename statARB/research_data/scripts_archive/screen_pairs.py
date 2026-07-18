import urllib.request
import json
import pandas as pd
import numpy as np

def fetch_klines(symbol, interval, limit=1000):
    url = f"https://fapi.binance.com/fapi/v1/klines?symbol={symbol}&interval={interval}&limit={limit}"
    req = urllib.request.Request(url)
    try:
        with urllib.request.urlopen(req) as response:
            data = json.loads(response.read().decode())
            df = pd.DataFrame(data, columns=['open_time', 'open', 'high', 'low', 'close', 'volume', 'close_time', 'qav', 'num_trades', 'taker_base_vol', 'taker_quote_vol', 'ignore'])
            df['close'] = df['close'].astype(float)
            df['open_time'] = pd.to_datetime(df['open_time'], unit='ms')
            return df[['open_time', 'close']].set_index('open_time')
    except Exception as e:
        print(f"Error fetching {symbol}: {e}")
        return None

def calculate_ols_beta(y, x):
    if len(y) < 10: return None
    mean_x, mean_y = np.mean(x), np.mean(y)
    var_x = np.sum((x - mean_x)**2)
    if var_x < 1e-12: return None
    return np.sum((x - mean_x) * (y - mean_y)) / var_x

candidate_pairs = [
    ("SOLUSDT", "AVAXUSDT"), # L1
    ("1000SHIBUSDT", "DOGEUSDT"), # Meme
    ("1000PEPEUSDT", "1000FLOKIUSDT"), # Meme 2
    ("WLDUSDT", "RNDRUSDT"), # AI
    ("UNIUSDT", "AAVEUSDT"), # DeFi
    ("OPUSDT", "ARBUSDT"), # L2
    ("NEARUSDT", "FTMUSDT"), # L1 alt
    ("APTUSDT", "SUIUSDT") # Move-based L1
]

results = []

print("Screening candidates using last 1000 15m bars...")
for sym_a, sym_b in candidate_pairs:
    df_a = fetch_klines(sym_a, "15m", 1000)
    df_b = fetch_klines(sym_b, "15m", 1000)
    
    if df_a is not None and df_b is not None:
        df = df_a.join(df_b, lsuffix='_a', rsuffix='_b', how='inner')
        if len(df) < 500: continue
        
        y = np.log(df['close_a'].values)
        x = np.log(df['close_b'].values)
        
        beta = calculate_ols_beta(y, x)
        if beta is None: continue
        
        alpha = np.mean(y) - beta * np.mean(x)
        res = y - (beta * x + alpha)
        
        std_err = np.sqrt(np.sum(res**2) / len(res))
        
        var_y = np.var(y)
        var_res = np.var(res)
        r2 = 1 - (var_res / var_y) if var_y > 0 else 0
        
        results.append({
            "Pair": f"{sym_a} / {sym_b}",
            "Beta": beta,
            "Std_Err (%)": std_err * 100,
            "R2": r2
        })

df_res = pd.DataFrame(results)
print(df_res.sort_values('Std_Err (%)', ascending=False).to_markdown(index=False))

# For reference, fetch ETH/BTC to compare
df_eth = fetch_klines("ETHUSDT", "15m", 1000)
df_btc = fetch_klines("BTCUSDT", "15m", 1000)
if df_eth is not None and df_btc is not None:
    df_eb = df_eth.join(df_btc, lsuffix='_a', rsuffix='_b', how='inner')
    if len(df_eb) >= 10:
        y = np.log(df_eb['close_a'].values)
        x = np.log(df_eb['close_b'].values)
        beta = calculate_ols_beta(y, x)
        if beta is not None:
            alpha = np.mean(y) - beta * np.mean(x)
            res = y - (beta * x + alpha)
            std_err = np.sqrt(np.sum(res**2) / len(res))
            r2 = 1 - (np.var(res) / np.var(y))
            print("\nETH/BTC Benchmark:")
            print(f"Beta: {beta:.2f}, Std_Err: {std_err*100:.2f}%, R2: {r2:.4f}")
