import urllib.request
import json
import pandas as pd
import time
import os

symbols = ["UNIUSDT", "AAVEUSDT", "1000PEPEUSDT", "1000FLOKIUSDT", "SOLUSDT", "AVAXUSDT"]

print("Fetching exchange limits...")
url = "https://fapi.binance.com/fapi/v1/exchangeInfo"
req = urllib.request.Request(url)
limits = {}
try:
    with urllib.request.urlopen(req) as response:
        data = json.loads(response.read().decode())
        for sym in data["symbols"]:
            if sym["symbol"] in symbols:
                lot_size = next((f for f in sym["filters"] if f["filterType"] == "LOT_SIZE"), None)
                min_notional = next((f for f in sym["filters"] if f["filterType"] == "MIN_NOTIONAL"), None)
                if lot_size and min_notional:
                    limits[sym["symbol"]] = {
                        "stepSize": float(lot_size["stepSize"]),
                        "minNotional": float(min_notional["notional"])
                    }
except Exception as e:
    print(f"Failed to fetch exchangeInfo: {e}")

print("Limits:", json.dumps(limits, indent=2))
with open("statARB/altcoin_limits.json", "w") as f:
    json.dump(limits, f)

def fetch_historical(symbol, days=30):
    print(f"Fetching {days} days of 1m data for {symbol}...")
    limit = 1500
    end_time = int(time.time() * 1000)
    start_time_limit = end_time - (days * 24 * 60 * 60 * 1000)
    
    all_klines = []
    
    while end_time > start_time_limit:
        url = f"https://fapi.binance.com/fapi/v1/klines?symbol={symbol}&interval=1m&limit={limit}&endTime={end_time}"
        try:
            req = urllib.request.Request(url)
            with urllib.request.urlopen(req) as response:
                data = json.loads(response.read().decode())
                if not data:
                    break
                all_klines = data + all_klines
                end_time = data[0][0] - 1
                time.sleep(0.2)
        except Exception as e:
            print(f"Error fetching {symbol}: {e}")
            break
            
    df = pd.DataFrame(all_klines, columns=['open_time', 'open', 'high', 'low', 'close', 'volume', 'close_time', 'qav', 'num_trades', 'taker_base_vol', 'taker_quote_vol', 'ignore'])
    df['close'] = df['close'].astype(float)
    df['open_time'] = pd.to_datetime(df['open_time'], unit='ms')
    df = df.drop_duplicates(subset=['open_time'])
    df = df[df['open_time'] >= pd.to_datetime(start_time_limit, unit='ms')]
    
    # Save to CSV
    df.to_csv(f"statARB/{symbol}_1m.csv", index=False)
    print(f"Saved {len(df)} rows for {symbol}.")

for sym in symbols:
    fetch_historical(sym, 30)
