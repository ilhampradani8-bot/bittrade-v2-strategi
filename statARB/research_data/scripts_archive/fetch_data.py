import requests
import pandas as pd
import time
from datetime import datetime, timedelta

def fetch_klines(symbol, interval, days):
    end_time = int(time.time() * 1000)
    start_time = end_time - (days * 24 * 60 * 60 * 1000)
    
    all_klines = []
    current_start = start_time
    
    print(f"Fetching {symbol} {interval} data for {days} days...")
    
    while current_start < end_time:
        url = "https://api.binance.com/api/v3/klines"
        params = {
            "symbol": symbol,
            "interval": interval,
            "startTime": current_start,
            "endTime": end_time,
            "limit": 1000
        }
        
        try:
            res = requests.get(url, params=params)
            data = res.json()
            
            if not data:
                break
                
            all_klines.extend(data)
            current_start = data[-1][0] + 1
            time.sleep(0.1)  # rate limit safety
            
        except Exception as e:
            print(f"Error fetching data: {e}")
            time.sleep(2)
            
    df = pd.DataFrame(all_klines, columns=[
        'open_time', 'open', 'high', 'low', 'close', 'volume',
        'close_time', 'quote_asset_volume', 'number_of_trades',
        'taker_buy_base_asset_volume', 'taker_buy_quote_asset_volume', 'ignore'
    ])
    
    df['open_time'] = pd.to_datetime(df['open_time'], unit='ms')
    df['close'] = df['close'].astype(float)
    df = df[['open_time', 'close']]
    df = df.set_index('open_time')
    
    # check for gaps
    expected_diff = pd.Timedelta(minutes=1)
    diffs = df.index.to_series().diff()
    gaps = diffs[diffs > expected_diff]
    if not gaps.empty:
        print(f"Found {len(gaps)} gaps in {symbol} data.")
    
    return df

def main():
    days = 30 # Fetch 30 days of data
    btc = fetch_klines("BTCUSDT", "1m", days)
    eth = fetch_klines("ETHUSDT", "1m", days)
    
    btc.to_csv("btc_1m.csv")
    eth.to_csv("eth_1m.csv")
    print("Saved btc_1m.csv and eth_1m.csv")

if __name__ == "__main__":
    main()
