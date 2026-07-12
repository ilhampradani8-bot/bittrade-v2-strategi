#!/usr/bin/env python3
"""
BitTrade-v2 Quantitative Backtest Engine (Bot A / Bot 1 Simulation)
Simulates the exact logic from conclude.rs & executor.rs of Bot A.
Reads historical 1m klines from database and writes results to a JSON file.
"""

import sys
import os
import json
import time
from datetime import datetime, timezone
import psycopg2
from urllib.parse import urlparse, unquote
from dotenv import load_dotenv

# Load database config
load_dotenv(dotenv_path="../.env")
DATABASE_URL = os.getenv("DATABASE_URL")

def parse_db_url(url):
    if not url:
        raise ValueError("DATABASE_URL is not set in environment or .env file.")
    parsed = urlparse(url)
    return {
        "host": parsed.hostname,
        "port": parsed.port or 5432,
        "dbname": parsed.path.lstrip('/'),
        "user": unquote(parsed.username) if parsed.username else None,
        "password": unquote(parsed.password) if parsed.password else None
    }

def calculate_emas(prices, period):
    emas = [0.0] * len(prices)
    if not prices:
        return emas
    k = 2.0 / (period + 1.0)
    emas[0] = prices[0]
    for i in range(1, len(prices)):
        emas[i] = (prices[i] * k) + (emas[i-1] * (1.0 - k))
    return emas

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

def main():
    # User-defined parameters via CLI
    starting_balance = float(sys.argv[1]) if len(sys.argv) > 1 else 1000.0
    tp_hard_input = float(sys.argv[2]) if len(sys.argv) > 2 else 0.03 # Default: +3.0%
    stop_loss_input = float(sys.argv[3]) if len(sys.argv) > 3 else -0.012 # Default: -1.2%
    
    # Ensure proper signs
    tp_hard = abs(tp_hard_input)
    stop_loss_limit = -abs(stop_loss_input)
    
    # Trailing TP settings
    tp_trailing_trigger = 0.015  # 1.5% peak
    tp_trailing_pullback = 0.010 # 1.0% drop
    
    print("[Backtest Engine] Loading historical klines for Bot A...")
    
    try:
        db_params = parse_db_url(DATABASE_URL)
        conn = psycopg2.connect(**db_params)
        cursor = conn.cursor()
        
        cursor.execute("""
            SELECT open_time, open_price, high_price, low_price, close_price, volume
            FROM btc_klines
            ORDER BY open_time ASC
        """)
        klines = cursor.fetchall()
        cursor.close()
        conn.close()
    except Exception as e:
        print(f"[ERROR] Database connection/query failed: {e}")
        sys.exit(1)
        
    total_candles = len(klines)
    if total_candles < 50:
        print("[ERROR] Insufficient historical data (needs at least 50 candles).")
        sys.exit(1)
        
    print(f"[Backtest Engine] Loaded {total_candles:,} klines. Pre-calculating indicators...")
    
    # Extract chronological close prices and volumes
    close_prices = [k[4] for k in klines]
    volumes = [k[5] for k in klines]
    
    # Precompute indicators O(N)
    ema13 = calculate_emas(close_prices, 13)
    ema34 = calculate_emas(close_prices, 34)
    sma50, stddev50 = calculate_bollinger_bands(close_prices, 50)
    
    print("[Backtest Engine] Starting simulation loop...")
    
    # Simulation state
    balance = starting_balance
    btc_balance = 0.0
    active_positions = [] # Elements: (buy_price, amount, buy_fee, time_epoch_sec)
    
    cycle_id = 1
    cycle_hwm = 0.0
    last_buy_time = 0 # epoch seconds
    death_cross_streak = 0
    
    completed_cycles = []
    equity_curve = []
    trade_logs = []
    
    last_day = None
    vwap_volume_sum = 0.0
    vwap_pv_sum = 0.0
    
    start_sim_time = time.time()
    
    # Iterate through klines (start from index 50 to allow indicators to warm up)
    for idx in range(50, total_candles):
        open_time = klines[idx][0]
        open_price = klines[idx][1]
        high_price = klines[idx][2]
        low_price = klines[idx][3]
        close_price = klines[idx][4]
        volume = klines[idx][5]
        
        # open_time is a datetime object in UTC
        time_epoch_sec = int(open_time.timestamp())
        dt_utc = open_time
        dt_str = dt_utc.strftime('%Y-%m-%d %H:%M:%S')
        
        # 1. Update daily Session VWAP
        candle_day = dt_utc.date()
        if candle_day != last_day:
            last_day = candle_day
            vwap_volume_sum = 0.0
            vwap_pv_sum = 0.0
            
        vwap_volume_sum += volume
        vwap_pv_sum += close_price * volume
        vwap = vwap_pv_sum / vwap_volume_sum if vwap_volume_sum > 0 else close_price
        
        # 2. Check for Emergency and Trailing Exits (If holding BTC)
        if len(active_positions) > 0:
            total_spent = sum(pos[0] * pos[1] for pos in active_positions)
            total_btc = sum(pos[1] for pos in active_positions)
            avg_entry = total_spent / total_btc if total_btc > 0 else 0.0
            
            # Update global High Water Mark (HWM)
            if close_price > cycle_hwm:
                cycle_hwm = close_price
                
            current_profit_pct = (close_price - avg_entry) / avg_entry
            peak_profit_pct = (cycle_hwm - avg_entry) / avg_entry if cycle_hwm > 0 else 0.0
            
            is_sell = False
            sell_reason = ""
            
            # A. Emergency Stop Loss
            if current_profit_pct <= stop_loss_limit:
                is_sell = True
                sell_reason = f"Emergency Stop Loss {stop_loss_limit*100:.1f}%"
            # B. Hard Take Profit
            elif current_profit_pct >= tp_hard:
                is_sell = True
                sell_reason = f"Emergency Hard Take Profit +{tp_hard*100:.1f}%"
            # C. Trailing Take Profit (Peak >= 1.5% profit, drop 1.0% from HWM)
            elif peak_profit_pct >= tp_trailing_trigger and cycle_hwm > 0:
                drop_from_hwm = (cycle_hwm - close_price) / cycle_hwm
                if drop_from_hwm >= tp_trailing_pullback:
                    is_sell = True
                    sell_reason = f"Emergency Trailing Take Profit (Peak: +{peak_profit_pct*100:.2f}%, Drop: {drop_from_hwm*100:.1f}%)"
            # D. Sudden Dump Defensive Check
            else:
                last_change = close_price - close_prices[idx - 1]
                std_dev = stddev50[idx]
                if last_change < -2.5 * std_dev and std_dev > 5.0:
                    is_sell = True
                    sell_reason = "Emergency Sudden Dump Detected"
                    
            if is_sell:
                # Execute SELL
                revenue = close_price * total_btc
                sell_fee = revenue * 0.001
                net_revenue = revenue - sell_fee
                
                total_buy_fees = sum(pos[2] for pos in active_positions)
                net_pnl = net_revenue - total_spent - total_buy_fees
                pnl_pct = (net_pnl / total_spent) * 100.0 if total_spent > 0 else 0.0
                
                balance += net_revenue
                
                completed_cycles.append({
                    "cycle_id": cycle_id,
                    "start_time": datetime.fromtimestamp(active_positions[0][3], tz=timezone.utc).strftime('%Y-%m-%d %H:%M:%S'),
                    "end_time": dt_str,
                    "layers_used": len(active_positions),
                    "avg_entry_price": avg_entry,
                    "exit_price": close_price,
                    "total_spent": total_spent,
                    "net_pnl": net_pnl,
                    "pnl_pct": pnl_pct,
                    "exit_reason": sell_reason,
                    "status": "WIN" if net_pnl > 0 else "LOSS"
                })
                
                trade_logs.append(f"[{dt_str}] SELL ALL Cycle #{cycle_id} @ ${close_price:.2f} | P&L: ${net_pnl:.2f} ({pnl_pct:+.2f}%) | Reason: {sell_reason}")
                
                # Reset state
                btc_balance = 0.0
                cycle_hwm = 0.0
                active_positions = []
                cycle_id += 1
                death_cross_streak = 0
                continue
                
        # 3. Market Regime & Strategy Decision Loop
        # Volatility percentage based on Bollinger Bands StdDev
        std_dev = stddev50[idx]
        volatility_pct = (std_dev / close_price) * 100.0 if close_price > 0 else 0.0
        is_sideways = volatility_pct < 0.085
        
        # Trend 15m bullish check
        trend_15m_bullish = close_price > close_prices[idx - 15]
        
        # 5-minute buy cooldown & 15-minute sell holding time checks
        can_buy = (time_epoch_sec - last_buy_time) >= 300
        
        latest_buy_time = active_positions[-1][3] if active_positions else 0
        can_normal_sell = (time_epoch_sec - latest_buy_time) >= 900
        
        # A. Sideways Strategy (BB Mean Reversion)
        if is_sideways:
            upper_band = sma50[idx] + (2.0 * std_dev)
            lower_band = sma50[idx] - (2.0 * std_dev)
            bb_width_pct = ((upper_band - lower_band) / close_price) * 100.0 if close_price > 0 else 0.0
            
            # BUY
            if close_price <= lower_band and bb_width_pct >= 1.0 and len(active_positions) < 3 and can_buy:
                budget = balance * 0.15
                if budget >= 5.0:
                    buy_fee = budget * 0.001
                    btc_bought = (budget - buy_fee) / close_price
                    balance -= budget
                    btc_balance += btc_bought
                    
                    active_positions.append((close_price, btc_bought, buy_fee, time_epoch_sec))
                    last_buy_time = time_epoch_sec
                    
                    if len(active_positions) == 1:
                        cycle_hwm = close_price
                        
                    trade_logs.append(f"[{dt_str}] BUY Layer {len(active_positions)} @ ${close_price:.2f} (BB Lower Band) | Budget: ${budget:.2f} | Reason: [Sideways] Mean Reversion Entry")
                    
            # NORMAL SELL
            elif close_price >= upper_band and len(active_positions) > 0 and can_normal_sell:
                total_spent = sum(pos[0] * pos[1] for pos in active_positions)
                total_btc = sum(pos[1] for pos in active_positions)
                avg_entry = total_spent / total_btc
                
                revenue = close_price * total_btc
                sell_fee = revenue * 0.001
                net_revenue = revenue - sell_fee
                
                total_buy_fees = sum(pos[2] for pos in active_positions)
                net_pnl = net_revenue - total_spent - total_buy_fees
                pnl_pct = (net_pnl / total_spent) * 100.0
                
                balance += net_revenue
                
                completed_cycles.append({
                    "cycle_id": cycle_id,
                    "start_time": datetime.fromtimestamp(active_positions[0][3], tz=timezone.utc).strftime('%Y-%m-%d %H:%M:%S'),
                    "end_time": dt_str,
                    "layers_used": len(active_positions),
                    "avg_entry_price": avg_entry,
                    "exit_price": close_price,
                    "total_spent": total_spent,
                    "net_pnl": net_pnl,
                    "pnl_pct": pnl_pct,
                    "exit_reason": "Sideways Upper Band",
                    "status": "WIN" if net_pnl > 0 else "LOSS"
                })
                
                trade_logs.append(f"[{dt_str}] SELL ALL Cycle #{cycle_id} @ ${close_price:.2f} (BB Upper Band) | P&L: ${net_pnl:.2f} ({pnl_pct:+.2f}%) | Reason: [Sideways] Sentuh Atas BB50")
                
                btc_balance = 0.0
                cycle_hwm = 0.0
                active_positions = []
                cycle_id += 1
                death_cross_streak = 0
                
        # B. Trending Strategy (EMA 13/34 Crossover + VWAP + 15m Trend)
        else:
            e13 = ema13[idx]
            e34 = ema34[idx]
            
            # BUY
            if e13 > e34 and close_price > vwap and trend_15m_bullish and len(active_positions) < 3 and can_buy:
                budget = balance * 0.20
                if budget >= 5.0:
                    buy_fee = budget * 0.001
                    btc_bought = (budget - buy_fee) / close_price
                    balance -= budget
                    btc_balance += btc_bought
                    
                    active_positions.append((close_price, btc_bought, buy_fee, time_epoch_sec))
                    last_buy_time = time_epoch_sec
                    
                    if len(active_positions) == 1:
                        cycle_hwm = close_price
                        
                    # Calculate volume surge factor
                    sum_v = sum(volumes[idx-50:idx])
                    avg_v = sum_v / 50.0 if sum_v > 0 else 1.0
                    vol_surge = volume / avg_v if avg_v > 0 else 1.0
                    
                    trade_logs.append(f"[{dt_str}] BUY Layer {len(active_positions)} @ ${close_price:.2f} (EMA Golden Cross) | Budget: ${budget:.2f} | Reason: [Trending] EMA13/34 Buy (Vol Surge: {vol_surge:.1f}x)")
                    
            # NORMAL SELL (Requires 2-minute death cross confirmation streak)
            elif len(active_positions) > 0:
                is_sell_signal = (e13 < e34 and close_price < vwap) or (not trend_15m_bullish)
                
                if is_sell_signal:
                    death_cross_streak += 1
                    if death_cross_streak >= 2 and can_normal_sell:
                        total_spent = sum(pos[0] * pos[1] for pos in active_positions)
                        total_btc = sum(pos[1] for pos in active_positions)
                        avg_entry = total_spent / total_btc
                        
                        revenue = close_price * total_btc
                        sell_fee = revenue * 0.001
                        net_revenue = revenue - sell_fee
                        
                        total_buy_fees = sum(pos[2] for pos in active_positions)
                        net_pnl = net_revenue - total_spent - total_buy_fees
                        pnl_pct = (net_pnl / total_spent) * 100.0
                        
                        balance += net_revenue
                        
                        completed_cycles.append({
                            "cycle_id": cycle_id,
                            "start_time": datetime.fromtimestamp(active_positions[0][3], tz=timezone.utc).strftime('%Y-%m-%d %H:%M:%S'),
                            "end_time": dt_str,
                            "layers_used": len(active_positions),
                            "avg_entry_price": avg_entry,
                            "exit_price": close_price,
                            "total_spent": total_spent,
                            "net_pnl": net_pnl,
                            "pnl_pct": pnl_pct,
                            "exit_reason": "Trending Death Cross (2m)",
                            "status": "WIN" if net_pnl > 0 else "LOSS"
                        })
                        
                        trade_logs.append(f"[{dt_str}] SELL ALL Cycle #{cycle_id} @ ${close_price:.2f} (EMA Crossover) | P&L: ${net_pnl:.2f} ({pnl_pct:+.2f}%) | Reason: [Trending] Quant EMA13/34 Death Cross confirmed")
                        
                        btc_balance = 0.0
                        cycle_hwm = 0.0
                        active_positions = []
                        cycle_id += 1
                        death_cross_streak = 0
                else:
                    death_cross_streak = 0
                    
        # C. Breakout Sudden Pump Check
        last_change = close_price - close_prices[idx - 1]
        if last_change > 2.5 * std_dev and std_dev > 5.0 and len(active_positions) < 3 and can_buy:
            budget = balance * 0.25
            if budget >= 5.0:
                buy_fee = budget * 0.001
                btc_bought = (budget - buy_fee) / close_price
                balance -= budget
                btc_balance += btc_bought
                
                active_positions.append((close_price, btc_bought, buy_fee, time_epoch_sec))
                last_buy_time = time_epoch_sec
                
                if len(active_positions) == 1:
                    cycle_hwm = close_price
                    
                trade_logs.append(f"[{dt_str}] BUY Layer {len(active_positions)} @ ${close_price:.2f} (Sudden Pump Breakout) | Budget: ${budget:.2f} | Reason: [Breakout] Sudden Price Spike")

        # Record equity curve points hourly or at final kline
        current_equity = balance + (btc_balance * close_price)
        if idx % 60 == 0 or idx == total_candles - 1:
            equity_curve.append({
                "time": dt_str,
                "equity": round(current_equity, 2),
                "price": close_price
            })

    # Final calculations
    final_equity = balance + (btc_balance * close_prices[-1])
    total_net_profit = final_equity - starting_balance
    net_profit_pct = (total_net_profit / starting_balance) * 100.0
    
    total_trades = len(completed_cycles)
    wins = [c for c in completed_cycles if c["net_pnl"] > 0]
    losses = [c for c in completed_cycles if c["net_pnl"] <= 0]
    
    total_wins = len(wins)
    total_losses = len(losses)
    win_rate = (total_wins / total_trades * 100.0) if total_trades > 0 else 0.0
    
    gross_profits = sum(c["net_pnl"] for c in wins)
    gross_losses = sum(abs(c["net_pnl"]) for c in losses)
    profit_factor = gross_profits / gross_losses if gross_losses > 0 else (99.9 if gross_profits > 0 else 1.0)
    
    max_equity = starting_balance
    max_dd = 0.0
    for eq in equity_curve:
        val = eq["equity"]
        if val > max_equity:
            max_equity = val
        dd = (max_equity - val) / max_equity * 100.0
        if dd > max_dd:
            max_dd = dd
            
    results = {
        "summary": {
            "starting_balance": starting_balance,
            "final_balance": round(final_equity, 2),
            "total_net_profit": round(total_net_profit, 2),
            "net_profit_pct": round(net_profit_pct, 2),
            "total_trades": total_trades,
            "total_wins": total_wins,
            "total_losses": total_losses,
            "win_rate": round(win_rate, 2),
            "profit_factor": round(profit_factor, 2),
            "max_drawdown": round(max_dd, 2),
            "simulation_time_sec": round(time.time() - start_sim_time, 2)
        },
        "cycles": completed_cycles[-50:],  # Last 50 completed cycles
        "equity_curve": equity_curve,
        "logs": trade_logs[-100:]  # Last 100 logs
    }
    
    try:
        with open("backtest_results.json", "w") as f:
            json.dump(results, f, indent=2)
        print(f"[Backtest Engine] Done! Results saved to backtest_results.json")
        print(f"  Final Equity: ${final_equity:.2f} ({net_profit_pct:+.2f}%)")
        print(f"  Win Rate: {win_rate:.2f}% | Profit Factor: {profit_factor:.2f} | Max DD: {max_dd:.2f}%")
    except Exception as e:
        print(f"[ERROR] Failed to save results to JSON: {e}")

if __name__ == "__main__":
    main()
