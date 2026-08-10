#!/usr/bin/env python3
import sys
import os
import psycopg2
from datetime import datetime
from urllib.parse import urlparse, unquote
from dotenv import load_dotenv

# Load env file
load_dotenv(dotenv_path=os.path.join(os.path.dirname(__file__), "../.env"))
DATABASE_URL = os.getenv("DATABASE_URL")

CATEGORIES = {
    # EXTREME
    "UTKUSDT": "EXTREME", "TRBUSDT": "EXTREME", "LPTUSDT": "EXTREME", "WINGUSDT": "EXTREME", 
    "GASUSDT": "EXTREME", "UNFIUSDT": "EXTREME", "CREAMUSDT": "EXTREME", "LOOMUSDT": "EXTREME", 
    "BONDUSDT": "EXTREME", "PHBUSDT": "EXTREME",
    # HYPER
    "ACEUSDT": "HYPER", "NFPUSDT": "HYPER", "BICOUSDT": "HYPER", "VICUSDT": "HYPER", 
    "HFTUSDT": "HYPER", "DODOUSDT": "HYPER", "CYBERUSDT": "HYPER", "WOOUSDT": "HYPER", 
    "MINAUSDT": "HYPER", "PENDLEUSDT": "HYPER",
    # HIGH
    "CTSIUSDT": "HIGH", "COTIUSDT": "HIGH", "C98USDT": "HIGH", "JTOUSDT": "HIGH", 
    "PYTHUSDT": "HIGH", "ENSUSDT": "HIGH", "OPUSDT": "HIGH", "ARBUSDT": "HIGH", 
    "DYDXUSDT": "HIGH", "LDOUSDT": "HIGH",
    # LOW
    "BTCUSDT": "LOW", "ETHUSDT": "LOW", "SOLUSDT": "LOW", "BNBUSDT": "LOW", 
    "XRPUSDT": "LOW", "ADAUSDT": "LOW", "DOGEUSDT": "LOW", "SHIBUSDT": "LOW", 
    "DOTUSDT": "LOW", "LTCUSDT": "LOW"
}

# Current default params based on classifier.rs
CURRENT_PARAMS = {
    "EXTREME": {"stop_loss": 7.0, "tp_trigger": 3.0, "tp_pullback": 1.0},
    "HYPER": {"stop_loss": 5.0, "tp_trigger": 2.0, "tp_pullback": 0.7},
    "HIGH": {"stop_loss": 3.5, "tp_trigger": 1.5, "tp_pullback": 0.5},
    "LOW": {"stop_loss": 1.5, "tp_trigger": 0.6, "tp_pullback": 0.4}
}

def parse_db_url(url):
    parsed = urlparse(str(url))
    return {
        "host": parsed.hostname,
        "port": parsed.port or 5432,
        "dbname": parsed.path.lstrip("/"),
        "user": unquote(parsed.username) if parsed.username else None,
        "password": unquote(parsed.password) if parsed.password else None
    }

def get_trades(cursor):
    # Fetch all successful trades ordered by timestamp
    cursor.execute(
        "SELECT id, action, price, amount, timestamp, notes, symbol "
        "FROM bot_trading_history WHERE status = 'SUCCESS' ORDER BY id ASC"
    )
    rows = cursor.fetchall()
    
    trades = []
    active_buys = {}
    
    for r in rows:
        tid, action, price, amount, timestamp, notes, symbol = r
        symbol = symbol.upper()
        
        if action == "BUY":
            if symbol not in active_buys:
                active_buys[symbol] = []
            active_buys[symbol].append({
                "price": price,
                "amount": amount,
                "time": timestamp
            })
        elif action == "SELL":
            if symbol in active_buys and active_buys[symbol]:
                buys = active_buys[symbol]
                total_amount = sum(b["amount"] for b in buys)
                if total_amount <= 0:
                    continue
                avg_entry_price = sum(b["price"] * b["amount"] for b in buys) / total_amount
                start_time = buys[0]["time"]
                end_time = timestamp
                
                # Parse actual net P&L from notes if possible
                actual_pnl = 0.0
                if "P&L: $" in notes:
                    try:
                        pnl_str = notes.split("P&L: $")[1].split()[0]
                        actual_pnl = float(pnl_str.replace("+", ""))
                    except:
                        pass
                else:
                    # Fallback calculate spot P&L
                    entry_cost = avg_entry_price * total_amount
                    exit_val = price * total_amount
                    actual_pnl = exit_val - entry_cost - (entry_cost * 0.001) - (exit_val * 0.001)

                category = CATEGORIES.get(symbol, "HIGH")
                trades.append({
                    "symbol": symbol,
                    "category": category,
                    "start_time": start_time,
                    "end_time": end_time,
                    "avg_entry_price": avg_entry_price,
                    "total_amount": total_amount,
                    "actual_pnl": actual_pnl,
                    "actual_exit_price": price
                })
                active_buys[symbol] = [] # Reset buys
                
    return trades

def fetch_candles(cursor, symbol, start_time, end_time):
    cursor.execute(
        "SELECT open_time, high_price, low_price, close_price "
        "FROM crypto_klines WHERE symbol = %s AND open_time >= %s AND open_time <= %s ORDER BY open_time ASC",
        (symbol, start_time, end_time)
    )
    rows = cursor.fetchall()
    return [{"time": r[0], "high": r[1], "low": r[2], "close": r[3]} for r in rows]

def simulate_trade_with_params(candles, avg_entry, amount, sl, tg, pb, actual_exit_price):
    if not candles:
        # Fallback to actual trade exit
        return (actual_exit_price - avg_entry) * amount - (avg_entry * amount * 0.001) - (actual_exit_price * amount * 0.001)

    hwm = avg_entry
    sl_limit = sl / 100.0
    tg_limit = tg / 100.0
    pb_limit = pb / 100.0

    for c in candles:
        hwm = max(hwm, c["high"])
        peak_profit = (hwm - avg_entry) / avg_entry
        drop_from_peak = (hwm - c["low"]) / hwm
        drop_from_buy = (c["low"] - avg_entry) / avg_entry

        # 1. Stop Loss Check
        if drop_from_buy <= -sl_limit:
            exit_price = avg_entry * (1.0 - sl_limit)
            return (exit_price - avg_entry) * amount - (avg_entry * amount * 0.001) - (exit_price * amount * 0.001)

        # 2. Trailing Take Profit Check
        if peak_profit >= tg_limit and drop_from_peak >= pb_limit:
            exit_price = hwm * (1.0 - pb_limit)
            return (exit_price - avg_entry) * amount - (avg_entry * amount * 0.001) - (exit_price * amount * 0.001)

    # 3. Default exit if trade finishes without trigger
    exit_price = actual_exit_price
    return (exit_price - avg_entry) * amount - (avg_entry * amount * 0.001) - (exit_price * amount * 0.001)

def main():
    print("=" * 60)
    print("      BITTRADE PORTFOLIO PARAMETER OPTIMIZATION ENGINE      ")
    print("=" * 60)
    print("Analyzing database transaction history...")

    if not DATABASE_URL:
        print("[ERROR] DATABASE_URL env variable not found.")
        sys.exit(1)

    db_params = parse_db_url(DATABASE_URL)
    conn = psycopg2.connect(**db_params)
    cursor = conn.cursor()

    trades = get_trades(cursor)
    print(f"Reconstructed {len(trades)} completed trade cycles from history.\n")

    if not trades:
        print("No completed trades found. Run the bot longer to accumulate transaction logs first.")
        sys.exit(0)

    # Categorize trades
    trades_by_cat = {}
    for t in trades:
        cat = t["category"]
        if cat not in trades_by_cat:
            trades_by_cat[cat] = []
        trades_by_cat[cat].append(t)

    # Pre-fetch candle paths for each trade to speed up optimization grid search
    print("Loading 1-minute price path for each trade cycle...")
    for idx, t in enumerate(trades):
        t["candles"] = fetch_candles(cursor, t["symbol"], t["start_time"], t["end_time"])
        if idx % 10 == 0 and idx > 0:
            print(f"  Processed {idx}/{len(trades)} trades...")

    print("\nStarting Parameter Grid Search Optimization...")
    print("-" * 60)

    # Search Space
    sl_options = [1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 5.0, 6.0, 7.0]
    tg_options = [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0]
    pb_options = [0.3, 0.5, 0.7, 1.0, 1.5]

    optimized_configs = {}

    for cat, cat_trades in trades_by_cat.items():
        print(f"\nOptimizing category: {cat} ({len(cat_trades)} trades)")
        current = CURRENT_PARAMS.get(cat, {"stop_loss": 3.0, "tp_trigger": 1.5, "tp_pullback": 0.5})
        
        # Calculate actual cumulative profit
        actual_total_pnl = sum(t["actual_pnl"] for t in cat_trades)
        
        best_pnl = -999999.0
        best_params = None

        # Grid search
        for sl in sl_options:
            for tg in tg_options:
                for pb in pb_options:
                    # Pullback must be smaller than trigger
                    if pb >= tg:
                        continue
                    
                    sim_total_pnl = 0.0
                    for t in cat_trades:
                        sim_pnl = simulate_trade_with_params(
                            t["candles"], 
                            t["avg_entry_price"], 
                            t["total_amount"], 
                            sl, tg, pb, 
                            t["actual_exit_price"]
                        )
                        sim_total_pnl += sim_pnl

                    if sim_total_pnl > best_pnl:
                        best_pnl = sim_total_pnl
                        best_params = (sl, tg, pb)

        optimized_configs[cat] = {
            "current": current,
            "actual_pnl": actual_total_pnl,
            "best_pnl": best_pnl,
            "best_params": best_params
        }

        # Print quick summary for category
        bp = best_params
        improvement = best_pnl - actual_total_pnl
        print(f"  * Actual P&L: ${actual_total_pnl:+.2f}")
        if bp:
            print(f"  * Optimized P&L: ${best_pnl:+.2f} (SL: -{bp[0]}%, Trigger: +{bp[1]}%, Pullback: -{bp[2]}%)")
            print(f"  * Improvement: ${improvement:+.2f} ({'🚀' if improvement > 0 else '📉'})")
        else:
            print("  * No better parameter set found.")

    # Render overall report in markdown table
    print("\n" + "=" * 60)
    print("                   OPTIMIZATION REPORT                    ")
    print("=" * 60)
    
    report_path = os.path.join(os.path.dirname(__file__), "../trading_journal/parameter_optimization_report.md")
    os.makedirs(os.path.dirname(report_path), exist_ok=True)

    with open(report_path, "w") as f:
        f.write("# Trading Parameter Optimization Report\n\n")
        f.write(f"Generated on: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n")
        f.write("Analyzed historical trade logs to find optimal dynamic Stop Loss & Trailing TP.\n\n")
        
        f.write("## Category Performance Comparison\n\n")
        f.write("| Volatility Category | Trades | Actual P&L ($) | Optimized P&L ($) | Improvement ($) | Recommended Parameters |\n")
        f.write("| --- | --- | --- | --- | --- | --- |\n")
        
        total_actual = 0.0
        total_opt = 0.0

        for cat, res in optimized_configs.items():
            trades_count = len(trades_by_cat[cat])
            actual = res["actual_pnl"]
            best = res["best_pnl"]
            bp = res["best_params"]
            improvement = best - actual
            
            total_actual += actual
            total_opt += best

            bp_str = f"SL: -{bp[0]}% / TG: +{bp[1]}% / PB: -{bp[2]}%" if bp else "Keep original"
            f.write(f"| {cat} | {trades_count} | ${actual:+.2f} | ${best:+.2f} | ${improvement:+.2f} | {bp_str} |\n")

        total_improvement = total_opt - total_actual
        f.write(f"| **TOTAL PORTFOLIO** | **{len(trades)}** | **${total_actual:+.2f}** | **${total_opt:+.2f}** | **${total_improvement:+.2f}** | **-** |\n\n")

        f.write("## Original vs Recommended Configuration Comparison\n\n")
        f.write("| Category | Original Configuration | Recommended Configuration |\n")
        f.write("| --- | --- | --- |\n")
        for cat, res in optimized_configs.items():
            curr = res["current"]
            bp = res["best_params"]
            curr_str = f"SL: -{curr['stop_loss']}% / TG: +{curr['tp_trigger']}% / PB: -{curr['tp_pullback']}%"
            bp_str = f"SL: -{bp[0]}% / TG: +{bp[1]}% / PB: -{bp[2]}%" if bp else curr_str
            f.write(f"| {cat} | {curr_str} | {bp_str} |\n")

    # Check if --apply is passed
    apply = "--apply" in sys.argv
    if apply:
        print("\nApplying optimized parameters to database table bot_a_parameters...")
        cursor.execute(
            "CREATE TABLE IF NOT EXISTS bot_a_parameters ("
            "category VARCHAR(20) PRIMARY KEY, "
            "stop_loss_limit DOUBLE PRECISION NOT NULL, "
            "uptrend_tp_trail_trigger DOUBLE PRECISION NOT NULL, "
            "uptrend_tp_trail_pullback DOUBLE PRECISION NOT NULL)"
        )
        for cat, res in optimized_configs.items():
            bp = res["best_params"]
            if bp:
                sl_decimal = -abs(bp[0] / 100.0)
                tg_decimal = bp[1] / 100.0
                pb_decimal = bp[2] / 100.0
                cursor.execute(
                    "INSERT INTO bot_a_parameters (category, stop_loss_limit, uptrend_tp_trail_trigger, uptrend_tp_trail_pullback) "
                    "VALUES (%s, %s, %s, %s) "
                    "ON CONFLICT (category) DO UPDATE SET "
                    "stop_loss_limit = EXCLUDED.stop_loss_limit, "
                    "uptrend_tp_trail_trigger = EXCLUDED.uptrend_tp_trail_trigger, "
                    "uptrend_tp_trail_pullback = EXCLUDED.uptrend_tp_trail_pullback",
                    (cat, sl_decimal, tg_decimal, pb_decimal)
                )
                print(f"  * Category {cat}: Applied SL: {sl_decimal}, TG: {tg_decimal}, PB: {pb_decimal}")
        conn.commit()
        print("Database updates committed successfully.")

    print(f"\nSuccessfully generated detailed markdown report at:\n{report_path}")
    print("=" * 60)

    cursor.close()
    conn.close()

if __name__ == "__main__":
    main()
