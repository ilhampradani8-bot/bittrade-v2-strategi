#!/usr/bin/env python3
import sys
import os
import psycopg2
from datetime import datetime, timezone
from urllib.parse import urlparse, unquote
from dotenv import load_dotenv

# Load database config
load_dotenv(dotenv_path=os.path.join(os.path.dirname(__file__), "../.env"))
DATABASE_URL = os.getenv("DATABASE_URL")

def parse_db_url(url):
    parsed = urlparse(str(url))
    return {
        "host": parsed.hostname,
        "port": parsed.port or 5432,
        "dbname": parsed.path.lstrip("/"),
        "user": unquote(parsed.username) if parsed.username else None,
        "password": unquote(parsed.password) if parsed.password else None
    }

def calculate_rsi(prices, period=14):
    if len(prices) < period + 1:
        return 50.0
    changes = [prices[i] - prices[i-1] for i in range(1, len(prices))]
    recent = changes[-period:]
    gains = [c for c in recent if c > 0.0]
    losses = [abs(c) for c in recent if c < 0.0]
    avg_gain = sum(gains) / period
    avg_loss = sum(losses) / period
    if avg_loss == 0.0:
        return 100.0
    rs = avg_gain / avg_loss
    return 100.0 - (100.0 / (1.0 + rs))

def calculate_ema(prices, period):
    if len(prices) < period:
        return 0.0
    multiplier = 2.0 / (period + 1.0)
    ema = prices[0]
    for p in prices[1:]:
        ema = (p - ema) * multiplier + ema
    return ema

def calculate_min_rsi_3h(prices, period=14):
    if len(prices) < period + 1:
        return 50.0
    min_rsi = 100.0
    for i in range(period, len(prices)):
        sub_prices = prices[i - period : i + 1]
        rsi_val = calculate_rsi(sub_prices, period)
        if rsi_val < min_rsi:
            min_rsi = rsi_val
    return min_rsi

def main():
    starting_balance = float(sys.argv[1]) if len(sys.argv) > 1 else 1000.0
    symbol = sys.argv[2] if len(sys.argv) > 2 else "BTCUSDT"
    print(f"[Backtest SmartDCA] Starting balance: ${starting_balance:.2f} | Target symbol: {symbol}")
    print("[Backtest SmartDCA] Loading historical klines from database...")
    
    try:
        db_params = parse_db_url(DATABASE_URL)
        conn = psycopg2.connect(**db_params)
        cursor = conn.cursor()
        
        # Check if the symbol exists in dca_klines
        cursor.execute("SELECT COUNT(*) FROM dca_klines WHERE symbol = %s", (symbol,))
        count = cursor.fetchone()[0]
        
        if count > 0:
            print(f"[Backtest SmartDCA] Loading data for {symbol} from dca_klines ({count:,} rows)...")
            cursor.execute(
                "SELECT open_time, open_price, high_price, low_price, close_price, volume "
                "FROM dca_klines WHERE symbol = %s ORDER BY open_time ASC", 
                (symbol,)
            )
        else:
            if symbol == "BTCUSDT":
                print("[Backtest SmartDCA] Loading data for BTCUSDT from btc_klines...")
                cursor.execute(
                    "SELECT open_time, open_price, high_price, low_price, close_price, volume "
                    "FROM btc_klines ORDER BY open_time ASC"
                )
            else:
                print(f"[ERROR] No historical data found for {symbol} in dca_klines table.")
                sys.exit(1)
                
        klines = cursor.fetchall()
        cursor.close()
        conn.close()
    except Exception as e:
        print(f"[ERROR] Database query failed: {e}")
        sys.exit(1)
        
    total_candles = len(klines)
    print(f"[Backtest SmartDCA] Loaded {total_candles:,} klines. Processing...")
    
    # Initialize strategy state
    balance = starting_balance
    btc_balance = 0.0
    
    # Active cycle tracking
    cycle_id = 1
    layers_filled = 0
    cycle_hwm = 0.0
    active_positions = [] # list of dicts: {"price": p, "amount": a, "usdt_spent": u}
    
    completed_cycles = []
    equity_curve = []
    
    # Rolling windows
    close_prices = []
    volumes = []
    high_prices = []
    
    for idx, kline in enumerate(klines):
        open_time, open_price, high_price, low_price, close_price, volume = kline
        
        close_prices.append(close_price)
        volumes.append(volume)
        high_prices.append(high_price)
        
        # Limit window size to 1000 to match live Rust db queries
        if len(close_prices) > 1000:
            close_prices.pop(0)
            volumes.pop(0)
            high_prices.pop(0)
            
        if len(close_prices) < 750:
            continue
            
        # Get indicators
        high_4h = max(high_prices[-240:])
        drop_pct = (close_price - high_4h) / high_4h * 100.0
        
        rsi_slice = close_prices[-15:]
        rsi = calculate_rsi(rsi_slice, 14)
        
        rsi_3h_slice = close_prices[-195:]
        min_rsi = calculate_min_rsi_3h(rsi_3h_slice, 14)
        dynamic_limit = min(min_rsi + 5.0, 25.0)
        rsi_allowed = not (rsi < 40.0 and rsi > dynamic_limit)
        
        ema_750 = calculate_ema(close_prices, 750)
        trend_ok = (close_price > ema_750) if ema_750 > 0.0 else True
        
        # Volume Surge Detector
        prev_vols = volumes[-21:-1]
        avg_vol = sum(prev_vols) / len(prev_vols) if prev_vols else 0.0
        volume_safe = True
        if avg_vol > 0.0 and volume > 3.0 * avg_vol:
            volume_safe = False
            
        # Exit evaluation (only if we have open positions)
        if layers_filled > 0:
            if close_price > cycle_hwm:
                cycle_hwm = close_price
                
            # Calculate WAEP
            total_btc = sum(pos["amount"] for pos in active_positions)
            total_usdt_spent = sum(pos["usdt_spent"] for pos in active_positions)
            avg_entry = sum(pos["price"] * pos["amount"] for pos in active_positions) / total_btc
            
            # PNL calculation
            current_profit_pct = (close_price - avg_entry) / avg_entry
            
            # Check exits
            is_exit = False
            exit_reason = ""
            
            # A. Check Liquidation (simulated 3x leverage)
            total_debt = total_usdt_spent * 2.0
            position_value = close_price * total_btc
            if position_value <= total_debt:
                is_exit = True
                exit_reason = "LIQUIDATION"
            # B. Emergency Cut Loss at -5.0%
            elif current_profit_pct <= -0.05:
                is_exit = True
                exit_reason = "[Darurat] Cut Loss DCA -5%"
            # C. Hard Take Profit at +2.5%
            elif current_profit_pct >= 0.025:
                is_exit = True
                exit_reason = "[SmartDCA] Hard Take Profit +2.5%"
            # D. Trailing Take Profit (Profit >= 1.5% and drop 0.8% from HWM)
            elif current_profit_pct >= 0.015 and cycle_hwm > 0.0:
                drop_from_hwm = (cycle_hwm - close_price) / cycle_hwm
                if drop_from_hwm >= 0.008:
                    is_exit = True
                    exit_reason = "[SmartDCA] Trailing Profit Lock"
                    
            if is_exit:
                sell_fee = close_price * total_btc * 0.001
                exit_value = (close_price * total_btc) - sell_fee
                total_debt = total_usdt_spent * 2.0
                
                if exit_reason == "LIQUIDATION":
                    usdt_received = 0.0
                    net_pnl = -total_usdt_spent
                    pnl_pct = -100.0
                else:
                    usdt_received = max(0.0, exit_value - total_debt)
                    net_pnl = usdt_received - total_usdt_spent
                    pnl_pct = (net_pnl / total_usdt_spent) * 100.0
                
                balance += usdt_received
                completed_cycles.append({
                    "cycle_id": cycle_id,
                    "layers_used": layers_filled,
                    "avg_entry": avg_entry,
                    "exit_price": close_price,
                    "total_spent": total_usdt_spent,
                    "net_pnl": net_pnl,
                    "pnl_pct": pnl_pct,
                    "exit_reason": exit_reason,
                    "status": "WIN" if net_pnl > 0.0 else "LOSS"
                })
                
                # Reset cycle state
                btc_balance = 0.0
                layers_filled = 0
                cycle_hwm = 0.0
                active_positions = []
                cycle_id += 1
                
        # Entry evaluation (only if we have room for more layers)
        if layers_filled < 3 and volume_safe and rsi < 65.0:
            total_equity = balance + btc_balance * close_price
            coin_budget = total_equity / 5.0
            
            # Layer 1
            if drop_pct <= -2.5 and layers_filled == 0 and rsi < 50.0 and rsi_allowed and trend_ok:
                spend = coin_budget * 0.40
                if spend > balance:
                    spend = balance
                if spend >= 10.0:
                    fee_multiplier = 1.0 - 0.001
                    btc_bought = (spend * 3.0 * fee_multiplier) / close_price
                    balance -= spend
                    btc_balance += btc_bought
                    layers_filled = 1
                    cycle_hwm = close_price
                    active_positions.append({"price": close_price, "amount": btc_bought, "usdt_spent": spend})
            
            # Layer 2
            elif drop_pct <= -5.0 and layers_filled == 1 and rsi < 50.0:
                spend = coin_budget * 0.30
                if spend > balance:
                    spend = balance
                if spend >= 10.0:
                    fee_multiplier = 1.0 - 0.001
                    btc_bought = (spend * 3.0 * fee_multiplier) / close_price
                    balance -= spend
                    btc_balance += btc_bought
                    layers_filled = 2
                    cycle_hwm = max(cycle_hwm, close_price)
                    active_positions.append({"price": close_price, "amount": btc_bought, "usdt_spent": spend})
                    
            # Layer 3
            elif drop_pct <= -8.0 and layers_filled == 2 and rsi < 40.0:
                spend = coin_budget * 0.30
                if spend > balance:
                    spend = balance
                if spend >= 10.0:
                    fee_multiplier = 1.0 - 0.001
                    btc_bought = (spend * 3.0 * fee_multiplier) / close_price
                    balance -= spend
                    btc_balance += btc_bought
                    layers_filled = 3
                    cycle_hwm = max(cycle_hwm, close_price)
                    active_positions.append({"price": close_price, "amount": btc_bought, "usdt_spent": spend})

        # Track equity curve
        current_equity = balance + (btc_balance * close_price)
        if idx % 60 == 0 or idx == total_candles - 1:
            equity_curve.append({
                "time": open_time.strftime("%Y-%m-%d %H:%M:%S"),
                "equity": current_equity,
                "price": close_price,
                "layers": layers_filled
            })

    # Output stats
    final_equity = balance + (btc_balance * close_prices[-1])
    net_profit = final_equity - starting_balance
    net_profit_pct = (net_profit / starting_balance) * 100.0
    total_trades = len(completed_cycles)
    wins = [c for c in completed_cycles if c["net_pnl"] > 0]
    losses = [c for c in completed_cycles if c["net_pnl"] <= 0]
    win_rate = (len(wins) / total_trades * 100.0) if total_trades > 0 else 0.0
    
    # Max Drawdown calculation
    max_equity = starting_balance
    max_dd = 0.0
    for eq in equity_curve:
        val = eq["equity"]
        if val > max_equity:
            max_equity = val
        dd = (max_equity - val) / max_equity * 100.0
        if dd > max_dd:
            max_dd = dd
            
    print("\n================ SMARTDCA BACKTEST RESULTS ================")
    print(f"  Starting Balance : ${starting_balance:.2f}")
    print(f"  Final Equity     : ${final_equity:.2f} ({net_profit_pct:+.2f}%)")
    print(f"  Total Cycles     : {total_trades} (Wins: {len(wins)} | Losses: {len(losses)})")
    print(f"  Win Rate         : {win_rate:.2f}%")
    print(f"  Max Drawdown     : {max_dd:.2f}%")
    
    reasons = {}
    for c in completed_cycles:
        r = c["exit_reason"].split(" (")[0]
        reasons[r] = reasons.get(r, 0) + 1
    print("\n  Exit Reasons Breakdown:")
    for r, count in reasons.items():
        print(f"    - {r}: {count}")
    print("===========================================================\n")

if __name__ == "__main__":
    main()
