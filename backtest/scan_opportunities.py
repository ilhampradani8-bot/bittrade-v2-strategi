import sys
import os
import requests
import statistics
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
import subprocess
import json

# Import classification utilities
from coin_classifier import classify_coin, get_params_for_category

# Colors for terminal output
GREEN = "\033[92m"
YELLOW = "\033[93m"
RED = "\033[91m"
CYAN = "\033[96m"
MAGENTA = "\033[95m"
BOLD = "\033[1m"
RESET = "\033[0m"

def get_binance_usdt_pairs():
    """Fetch all active USDT trading pairs from Binance."""
    try:
        resp = requests.get("https://api.binance.com/api/v3/exchangeInfo", timeout=10)
        if resp.status_code != 200:
            return []
        data = resp.json()
        pairs = [
            s["symbol"] for s in data["symbols"]
            if s["symbol"].endswith("USDT") and s["status"] == "TRADING"
        ]
        return sorted(pairs)
    except Exception as e:
        print(f"{RED}[ERROR] Failed to fetch exchange info from Binance: {e}{RESET}")
        return []

def fetch_klines(symbol, limit=500):
    """Fetch recent klines from Binance API."""
    try:
        url = "https://api.binance.com/api/v3/klines"
        params = {"symbol": symbol, "interval": "1m", "limit": limit}
        resp = requests.get(url, params=params, timeout=5)
        if resp.status_code == 200:
            return resp.json()
    except Exception:
        pass
    return []

# Helper math functions
def calculate_emas(prices, period):
    emas = [0.0] * len(prices)
    if not prices: return emas
    k = 2.0 / (period + 1.0)
    emas[0] = prices[0]
    for i in range(1, len(prices)):
        emas[i] = (prices[i] * k) + (emas[i-1] * (1.0 - k))
    return emas

def calculate_rsis(prices, period=14):
    rsis = [50.0] * len(prices)
    if len(prices) <= period: return rsis
    gains = [0.0] * len(prices)
    losses = [0.0] * len(prices)
    for i in range(1, len(prices)):
        diff = prices[i] - prices[i-1]
        if diff >= 0.0:
            gains[i] = diff
        else:
            losses[i] = -diff
    avg_gain = sum(gains[1:period+1]) / period
    avg_loss = sum(losses[1:period+1]) / period
    if avg_loss == 0.0:
        rsis[period] = 100.0
    else:
        rs = avg_gain / avg_loss
        rsis[period] = 100.0 - (100.0 / (1.0 + rs))
    for i in range(period + 1, len(prices)):
        avg_gain = (avg_gain * (period - 1) + gains[i]) / period
        avg_loss = (avg_loss * (period - 1) + losses[i]) / period
        if avg_loss == 0.0:
            rsis[i] = 100.0
        else:
            rs = avg_gain / avg_loss
            rsis[i] = 100.0 - (100.0 / (1.0 + rs))
    return rsis

def calculate_bollinger_bands(prices, period=50):
    smas = [0.0] * len(prices)
    stddevs = [0.0] * len(prices)
    running_sum = 0.0
    running_sq_sum = 0.0
    for i in range(len(prices)):
        running_sum += prices[i]
        running_sq_sum += prices[i] * prices[i]
        if i >= period:
            running_sum -= prices[i - period]
            running_sq_sum -= prices[i - period] * prices[i - period]
        if i >= period - 1:
            mean = running_sum / period
            variance = (running_sq_sum / period) - (mean * mean)
            variance = max(0.0, variance)
            smas[i] = mean
            stddevs[i] = variance ** 0.5
    return smas, stddevs

def score_opportunity(symbol, klines, category):
    """
    Score a symbol from 0 to 100 on potential entry setups.
    Returns: {"score": int, "strategy": str, "rsi": float, "vol_surge": float, "desc": str}
    """
    if len(klines) < 60:
        return {"score": 0, "strategy": "None", "rsi": 50.0, "vol_surge": 1.0, "desc": "Insufficient data"}
    
    closes = [float(k[4]) for k in klines]
    volumes = [float(k[5]) for k in klines]
    
    # Calculate indicators
    ema13 = calculate_emas(closes, 13)
    ema34 = calculate_emas(closes, 34)
    rsi14 = calculate_rsis(closes, 14)
    sma50, stddev50 = calculate_bollinger_bands(closes, 50)
    sma20, stddev20 = calculate_bollinger_bands(closes, 20)
    
    idx = len(closes) - 1
    close_price = closes[idx]
    volume = volumes[idx]
    
    # VWAP estimation
    # Average of last 100 candle PV sum
    pv_sum = sum(float(klines[i][4]) * float(klines[i][5]) for i in range(max(0, idx - 100), idx + 1))
    v_sum = sum(float(klines[i][5]) for i in range(max(0, idx - 100), idx + 1))
    vwap = pv_sum / v_sum if v_sum > 0 else close_price
    
    # Vol surge
    avg_v_50 = sum(volumes[idx-50:idx]) / 50.0 if idx >= 50 else 1.0
    vol_surge = volume / avg_v_50 if avg_v_50 > 0 else 1.0
    
    params = get_params_for_category(category, symbol)
    
    # 1. Uptrend Score
    ut_score = 0
    ut_desc = ""
    # Conditions: e13 > e34, close > vwap, rsi between min and max
    if ema13[idx] > ema34[idx] and close_price > vwap:
        ut_score += 30
        rsi_val = rsi14[idx]
        rsi_min = params.get("uptrend_rsi_min", 55.0)
        rsi_max = params.get("uptrend_rsi_max", 75.0)
        if rsi_min <= rsi_val <= rsi_max:
            ut_score += 30
        elif rsi_val > rsi_max:
            ut_score += 10 # slightly overbought
        
        # RSI slope
        rsi_slope_15m = rsi_val - rsi14[idx-15] if idx >= 15 else 0.0
        if rsi_slope_15m >= params.get("uptrend_min_rsi_slope_15m", 6.0):
            ut_score += 20
            
        # Vol surge
        if vol_surge >= params.get("uptrend_min_vol_surge_3m", 0.5):
            ut_score += 20
        ut_desc = f"EMA Cross Up, RSI: {rsi_val:.1f}, Vol Surge: {vol_surge:.1f}x"
        
    # 2. Downtrend Score
    dt_score = 0
    dt_desc = ""
    # Conditions: close below vwap, oversold rsi
    if close_price < vwap:
        dt_score += 20
        rsi_val = rsi14[idx]
        rsi_limit = params.get("downtrend_rsi_limit", 35.0)
        if rsi_val < rsi_limit:
            dt_score += 40
            # How close is the rebound? (last 2 candles green)
            if closes[idx] > closes[idx-1] and closes[idx-1] > closes[idx-2]:
                dt_score += 30
            # Vol surge
            if vol_surge >= params.get("downtrend_vol_surge_limit", 1.5):
                dt_score += 10
        dt_desc = f"Price < VWAP, Oversold RSI: {rsi_val:.1f}, Green streak check"

    # 3. Breakout Score
    bo_score = 0
    bo_desc = ""
    std_dev_pct = (stddev50[idx] / close_price) if close_price > 0 else 0.0
    if std_dev_pct >= params.get("breakout_min_std_dev", 0.02) and close_price > vwap:
        bo_score += 30
        if rsi14[idx] >= params.get("breakout_min_rsi", 60.0):
            bo_score += 30
        # Volume spike
        if vol_surge >= 2.0:
            bo_score += 40
        elif vol_surge >= 1.5:
            bo_score += 20
        bo_desc = f"Volatility Expansion ({std_dev_pct*100:.1f}%), RSI: {rsi14[idx]:.1f}, Vol Surge: {vol_surge:.1f}x"

    # 4. Sideways Score
    sw_score = 0
    sw_desc = ""
    volatility_pct_20 = (stddev20[idx] / close_price) * 100.0 if close_price > 0 else 0.0
    sw_min_vol = params.get("sideways_min_vol_pct", 0.20)
    sw_max_vol = params.get("sideways_max_vol_pct", 0.80)
    if sw_min_vol <= volatility_pct_20 < sw_max_vol:
        sw_score += 40
        # Touch lower band
        lower_band = sma20[idx] - (params.get("sideways_bb_mult", 2.0) * stddev20[idx])
        if close_price <= lower_band:
            sw_score += 40
        elif close_price <= lower_band * 1.005:
            sw_score += 25
        # Volume surge should be low
        if vol_surge <= 1.5:
            sw_score += 20
        sw_desc = f"Sideways volatility {volatility_pct_20:.2f}%, close near lower BB"

    # Pick the best opportunity
    scores = [
        (ut_score, "Uptrend", ut_desc),
        (dt_score, "Downtrend", dt_desc),
        (bo_score, "Breakout", bo_desc),
        (sw_score, "Sideways", sw_desc)
    ]
    scores.sort(reverse=True, key=lambda x: x[0])
    best_score, best_strat, best_desc = scores[0]
    
    return {
        "score": min(100, best_score),
        "strategy": best_strat,
        "rsi": rsi14[idx],
        "vol_surge": vol_surge,
        "desc": best_desc
    }

def process_symbol(symbol):
    """Fetch data, classify, and score a single symbol."""
    klines = fetch_klines(symbol)
    if not klines:
        return None
    category = classify_coin(symbol, klines)
    opp = score_opportunity(symbol, klines, category)
    return {
        "symbol": symbol,
        "category": category,
        "opp": opp
    }

def run_single_backtest(symbol, balance=1000.0, tp=0.03, sl=-0.015):
    """Run a single backtest as a subprocess and parse result."""
    try:
        cmd = ["python3", "run_backtest.py", str(balance), str(tp), str(sl), "optimized", symbol]
        proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        stdout, stderr = proc.communicate()
        
        # Parse output for results line
        # e.g., "  Final Equity: $1053.69 (+5.37%)"
        # and "  Win Rate: 83.33% | Profit Factor: 2.26 | Max DD: 2.42%"
        final_equity = balance
        win_rate = 0.0
        profit_factor = 0.0
        max_dd = 0.0
        net_profit_pct = 0.0
        
        for line in stdout.split("\n"):
            if "Final Equity:" in line:
                parts = line.strip().split("$")
                if len(parts) > 1:
                    final_equity = float(parts[1].split()[0])
                if "(" in line:
                    net_profit_pct = float(line.split("(")[1].replace("%", "").replace(")", "").strip())
            elif "Win Rate:" in line:
                # "  Win Rate: 83.33% | Profit Factor: 2.26 | Max DD: 2.42%"
                parts = line.split("|")
                for p in parts:
                    if "Win Rate:" in p:
                        win_rate = float(p.split(":")[1].replace("%", "").strip())
                    elif "Profit Factor:" in p:
                        profit_factor = float(p.split(":")[1].strip())
                    elif "Max DD:" in p:
                        max_dd = float(p.split(":")[1].replace("%", "").strip())
                        
        return {
            "symbol": symbol,
            "success": True,
            "final_equity": final_equity,
            "net_profit_pct": net_profit_pct,
            "win_rate": win_rate,
            "profit_factor": profit_factor,
            "max_dd": max_dd,
            "output": stdout
        }
    except Exception as e:
        return {
            "symbol": symbol,
            "success": False,
            "error": str(e)
        }

def main():
    import argparse
    parser = argparse.ArgumentParser(description="Multi-Coin Opportunity Scanner & Backtester")
    parser.add_argument("--top", type=int, default=10, help="Number of top opportunities to display/backtest")
    parser.add_argument("--categories", type=str, default="EXTREME,HYPER,HIGH", help="Categories to filter (comma separated)")
    parser.add_argument("--min-score", type=int, default=50, help="Minimum opportunity score to display")
    parser.add_argument("--backtest", action="store_true", help="Run parallel backtesting on top N opportunities")
    parser.add_argument("--balance", type=float, default=1000.0, help="Starting balance per backtested coin")
    parser.add_argument("--tp", type=float, default=0.03, help="Take profit parameter for backtesting")
    parser.add_argument("--sl", type=float, default=-0.015, help="Stop loss parameter for backtesting")
    parser.add_argument("--threads", type=int, default=20, help="Number of concurrent scanner threads")
    args = parser.parse_args()

    target_categories = [c.strip().upper() for c in args.categories.split(",")]
    
    print(f"\n{BOLD}{CYAN}=== STARTING MULTI-COIN OPPORTUNITY SCANNER ==={RESET}")
    print(f"Scanning Binance USDT pairs concurrent (threads: {args.threads})...")
    
    pairs = get_binance_usdt_pairs()
    if not pairs:
        print(f"{RED}[ERROR] No symbols fetched from Binance. Exiting.{RESET}")
        return
        
    print(f"Total symbols found on Binance: {len(pairs)}")
    print(f"Filtering for categories: {target_categories}")
    
    # Step 1: Scan all pairs concurrently
    results = []
    with ThreadPoolExecutor(max_workers=args.threads) as executor:
        futures = {executor.submit(process_symbol, pair): pair for pair in pairs}
        completed_count = 0
        for future in as_completed(futures):
            res = future.result()
            completed_count += 1
            if res:
                if res["category"] in target_categories and res["opp"]["score"] >= args.min_score:
                    results.append(res)
            
            # Print a progress indicator
            if completed_count % 50 == 0 or completed_count == len(pairs):
                sys.stdout.write(f"\rProgress: {completed_count}/{len(pairs)} pairs scanned...")
                sys.stdout.flush()
    print("\nScan completed!\n")
    
    # Sort by opportunity score descending
    results.sort(key=lambda x: x["opp"]["score"], reverse=True)
    top_opportunities = results[:args.top]
    
    # Step 2: Print Leaderboard
    print(f"{BOLD}{YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━{RESET}")
    print(f"{BOLD}{YELLOW}                         TOP OPPORTUNITY LEADERBOARD                                {RESET}")
    print(f"{BOLD}{YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━{RESET}")
    header = f"{'Rank':4} | {'Symbol':10} | {'Category':8} | {'Score':5} | {'Strategy':9} | {'RSI':5} | {'VolSg':5} | {'Description'}"
    print(header)
    print("-" * len(header))
    
    for i, item in enumerate(top_opportunities, 1):
        opp = item["opp"]
        score_color = GREEN if opp["score"] >= 80 else (YELLOW if opp["score"] >= 60 else RESET)
        print(
            f"{i:<4} | "
            f"{item['symbol']:10} | "
            f"{item['category']:8} | "
            f"{score_color}{opp['score']:>4}%{RESET} | "
            f"{opp['strategy']:9} | "
            f"{opp['rsi']:5.1f} | "
            f"{opp['vol_surge']:4.1f}x | "
            f"{opp['desc']}"
        )
    print(f"{BOLD}{YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━{RESET}\n")
    
    if not top_opportunities:
        print("No opportunities matching the criteria found.")
        return
        
    # Step 3: Run Parallel Backtesting if requested
    if args.backtest:
        top_symbols = [item["symbol"] for item in top_opportunities]
        print(f"{BOLD}{CYAN}=== RUNNING PARALLEL BACKTESTS FOR TOP {len(top_symbols)} COINS ==={RESET}")
        print(f"Starting balance per coin: ${args.balance:.2f} | TP: {args.tp*100}% | SL: {args.sl*100}%")
        
        backtest_results = []
        with ThreadPoolExecutor(max_workers=5) as executor:
            futures = {
                executor.submit(run_single_backtest, sym, args.balance, args.tp, args.sl): sym
                for sym in top_symbols
            }
            for future in as_completed(futures):
                res = future.result()
                backtest_results.append(res)
                if res["success"]:
                    print(f"  {GREEN}[SUCCESS]{RESET} Backtest for {res['symbol']} finished: {res['net_profit_pct']:+.2f}% P&L (Win Rate: {res['win_rate']}%)")
                else:
                    print(f"  {RED}[FAILED]{RESET} Backtest for {res['symbol']} failed: {res.get('error')}")
                    
        # Print summary report of all backtests
        print(f"\n{BOLD}{CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━{RESET}")
        print(f"{BOLD}{CYAN}                         COMBINED BACKTEST REPORT                                   {RESET}")
        print(f"{BOLD}{CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━{RESET}")
        bt_header = f"{'Symbol':10} | {'Final Equity':12} | {'Net P&L %':10} | {'Win Rate':8} | {'Profit Factor':13} | {'Max DD':6}"
        print(bt_header)
        print("-" * len(bt_header))
        
        total_initial = len(backtest_results) * args.balance
        total_final = 0.0
        wins_total = 0
        total_valid = 0
        
        for res in backtest_results:
            if not res or not res.get("success"):
                continue
            total_final += res["final_equity"]
            total_valid += 1
            pnl_color = GREEN if res["net_profit_pct"] >= 0 else RED
            print(
                f"{res['symbol']:10} | "
                f"${res['final_equity']:11.2f} | "
                f"{pnl_color}{res['net_profit_pct']:+9.2f}%{RESET} | "
                f"{res['win_rate']:7.2f}% | "
                f"{res['profit_factor']:13.2f} | "
                f"{res['max_dd']:5.2f}%"
            )
            
        if total_valid > 0:
            total_pnl_pct = ((total_final - (total_valid * args.balance)) / (total_valid * args.balance)) * 100
            pnl_color = GREEN if total_pnl_pct >= 0 else RED
            print("-" * len(bt_header))
            print(
                f"{BOLD}{'TOTAL':10} | "
                f"${total_final:11.2f} | "
                f"{pnl_color}{total_pnl_pct:+9.2f}%{RESET} | "
                f"{'N/A':8} | "
                f"{'N/A':13} | "
                f"{'N/A':6}"
            )
        print(f"{BOLD}{CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━{RESET}\n")

if __name__ == "__main__":
    main()
