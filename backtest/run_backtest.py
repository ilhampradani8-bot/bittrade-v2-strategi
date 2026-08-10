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
from coin_classifier import classify_coin, get_params_for_category

# Load database config
load_dotenv(dotenv_path=os.path.join(os.path.dirname(__file__), "../.env"))
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

def calculate_rsis(prices, period=14):
    rsis = [50.0] * len(prices)
    if len(prices) <= period:
        return rsis
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

def main():
    # User-defined parameters via CLI
    starting_balance = float(sys.argv[1]) if len(sys.argv) > 1 else 1000.0
    tp_hard_input = float(sys.argv[2]) if len(sys.argv) > 2 else 0.03 # Default: +3.0%
    stop_loss_input = float(sys.argv[3]) if len(sys.argv) > 3 else -0.012 # Default: -1.2%
    mode = sys.argv[4].lower() if len(sys.argv) > 4 else "safe"
    if mode not in ["safe", "optimized"]:
        mode = "safe"
    symbol = sys.argv[5].upper() if len(sys.argv) > 5 else "BTCUSDT"
    
    # Ensure proper signs
    tp_hard = abs(tp_hard_input)
    stop_loss_limit = -abs(stop_loss_input)

    print(f"[Backtest Engine] Running in {mode.upper()} mode for symbol {symbol}...")

    # ── PARAMETER CONFIGURATION BASED ON MODE ─────────────────────
    if mode == "optimized":
        category = classify_coin(symbol)
        print(f"[Backtest Engine] Auto-detected volatility category for {symbol}: {category}")
        params = get_params_for_category(category, symbol)
        
        # 1. Uptrend optimized parameters
        uptrend_gc_max_dur = params["uptrend_gc_max_dur"]
        uptrend_rsi_min = params["uptrend_rsi_min"]
        uptrend_rsi_max = params["uptrend_rsi_max"]
        uptrend_block_rsi_75_80 = params["uptrend_block_rsi_75_80"]
        uptrend_max_rsi_slope_7m = params["uptrend_max_rsi_slope_7m"]
        uptrend_min_rsi_slope_15m = params["uptrend_min_rsi_slope_15m"]
        uptrend_min_vol_surge_3m = params["uptrend_min_vol_surge_3m"]
        uptrend_is_dynamic_sizing = params["uptrend_is_dynamic_sizing"]
        uptrend_tp_trail_trigger = params["uptrend_tp_trail_trigger"]
        uptrend_tp_trail_pullback = params["uptrend_tp_trail_pullback"]
        uptrend_vwap_max_normal = params["uptrend_vwap_max_normal"]
        uptrend_vwap_max_volatile = params["uptrend_vwap_max_volatile"]
        stop_loss_limit = params["stop_loss_limit"]
        uptrend_ema_spread_min = params["uptrend_ema_spread_min"]
        uptrend_lock_duration = params["uptrend_lock_duration"]

        # 2. Sideways optimized parameters
        sideways_bb_period = params["sideways_bb_period"]
        sideways_bb_mult = params["sideways_bb_mult"]
        sideways_min_vol_pct = params["sideways_min_vol_pct"]
        sideways_max_vol_pct = params["sideways_max_vol_pct"]
        sideways_flat_budget_pct = params["sideways_flat_budget_pct"]

        # 3. Downtrend optimized parameters
        downtrend_max_vwap_dist = params["downtrend_max_vwap_dist"]
        downtrend_tp = params["downtrend_tp"]
        downtrend_hold_lock = params["downtrend_hold_lock"]
        downtrend_stop_loss = params["downtrend_stop_loss"]
        downtrend_rsi_limit = params["downtrend_rsi_limit"]
        downtrend_vol_surge_limit = params["downtrend_vol_surge_limit"]

        # 4. Breakout optimized parameters
        breakout_min_std_dev = params["breakout_min_std_dev"]
        breakout_min_spike_pct = params["breakout_min_spike_pct"]
        breakout_min_rsi = params["breakout_min_rsi"]
        breakout_max_vwap_dist = params["breakout_max_vwap_dist"]
        breakout_ema_gap_if_big = params["breakout_ema_gap_if_big"]
    else: # safe (bawaan asli)
        # 1. Uptrend safe parameters
        uptrend_gc_max_dur = 15
        uptrend_rsi_min = 0.0
        uptrend_rsi_max = 100.0
        uptrend_block_rsi_75_80 = True
        uptrend_max_rsi_slope_7m = 8.0
        uptrend_min_rsi_slope_15m = -999.0
        uptrend_min_vol_surge_3m = 0.0
        uptrend_is_dynamic_sizing = False
        uptrend_tp_trail_trigger = 0.006
        uptrend_tp_trail_pullback = 0.003
        uptrend_vwap_max_normal = 1.5
        uptrend_vwap_max_volatile = 0.5
        uptrend_ema_spread_min = 0.0005
        uptrend_lock_duration = 900

        # 2. Sideways safe parameters
        sideways_bb_period = 50
        sideways_bb_mult = 2.0
        sideways_min_vol_pct = 0.0
        sideways_max_vol_pct = 0.085
        sideways_flat_budget_pct = None  # dynamic sizing 30%-45%

        # 3. Downtrend safe parameters
        downtrend_max_vwap_dist = 999.0
        downtrend_tp = tp_hard
        downtrend_hold_lock = 900

        # 4. Breakout safe parameters
        breakout_min_std_dev = 5.0
        breakout_min_spike_pct = 0.0
        breakout_min_rsi = 0.0
        breakout_max_vwap_dist = 999.0
        breakout_ema_gap_if_big = -999.0

    # Trailing TP settings (Breakout/Default)
    tp_trailing_trigger = 0.015  # 1.5% peak
    tp_trailing_pullback = 0.010 # 1.0% drop
    
    print(f"[Backtest Engine] Loading historical klines for symbol {symbol}...")
    
    try:
        db_params = parse_db_url(DATABASE_URL)
        conn = psycopg2.connect(**db_params)
        cursor = conn.cursor()
        
        # Check if the symbol exists in dca_klines
        cursor.execute("SELECT COUNT(*) FROM dca_klines WHERE symbol = %s", (symbol,))
        count = cursor.fetchone()[0]
        
        if count > 0:
            print(f"[Backtest Engine] Loading data for {symbol} from dca_klines ({count:,} rows)...")
            cursor.execute(
                "SELECT open_time, open_price, high_price, low_price, close_price, volume "
                "FROM dca_klines WHERE symbol = %s ORDER BY open_time ASC", 
                (symbol,)
            )
        else:
            if symbol == "BTCUSDT":
                print("[Backtest Engine] Loading data for BTCUSDT from btc_klines...")
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
    sma20, stddev20 = calculate_bollinger_bands(close_prices, 20)
    rsi14 = calculate_rsis(close_prices, 14)

    # Pre-calculate Golden Cross Duration per candle (for Uptrend)
    golden_cross_duration = [0] * total_candles
    for i in range(1, total_candles):
        if ema13[i] > ema34[i]:
            golden_cross_duration[i] = golden_cross_duration[i-1] + 1
        else:
            golden_cross_duration[i] = 0
    
    print("[Backtest Engine] Starting simulation loop...")
    
    # Simulation state
    balance = starting_balance
    btc_balance = 0.0
    active_positions = [] # Elements: (buy_price, amount, buy_fee, time_epoch_sec, strat_label)
    
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
        
        # Calculate current candle indicators for global exits
        std_dev = stddev50[idx]
        volatility_pct = (std_dev / close_price) * 100.0 if close_price > 0 else 0.0
        
        # Volatility check for each specific strategy
        volatility_pct_20 = (stddev20[idx] / close_price) * 100.0 if close_price > 0 else 0.0
        volatility_pct_50 = volatility_pct
        
        # Sideways classification check
        vol_pct_sideways = volatility_pct_20 if sideways_bb_period == 20 else volatility_pct_50
        is_sideways_market = sideways_min_vol_pct <= vol_pct_sideways < sideways_max_vol_pct
        
        trend_15m_bullish = close_price > close_prices[idx - 15]
        
        # 2. Check for Exits (If holding BTC)
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
            # B. Hard Take Profit (skip if downtrend which has its own micro TP exit)
            elif current_profit_pct >= tp_hard and "[Downtrend]" not in active_positions[0][4]:
                is_sell = True
                sell_reason = f"Emergency Hard Take Profit +{tp_hard*100:.1f}%"
            # C. Trailing/Micro Take Profit
            elif cycle_hwm > 0:
                first_entry_reason = active_positions[0][4] if len(active_positions[0]) > 4 else ""
                if "[Downtrend]" in first_entry_reason:
                    # Downtrend: cek micro TP langsung
                    if (close_price - active_positions[0][0]) / active_positions[0][0] >= downtrend_tp:
                        is_sell = True
                        sell_reason = f"[Downtrend] Micro Take Profit +{downtrend_tp*100:.2f}%"
                elif "[Trending]" in first_entry_reason:
                    trailing_trigger = uptrend_tp_trail_trigger
                    trailing_pullback = uptrend_tp_trail_pullback
                else:
                    trailing_trigger = tp_trailing_trigger  # 1.5% peak
                    trailing_pullback = tp_trailing_pullback # 1.0% drop
                
                if not is_sell and peak_profit_pct >= trailing_trigger:
                    drop_from_hwm = (cycle_hwm - close_price) / cycle_hwm
                    if drop_from_hwm >= trailing_pullback:
                        is_sell = True
                        sell_reason = f"Emergency Trailing Take Profit (Peak: +{peak_profit_pct*100:.2f}%, Drop: {drop_from_hwm*100:.1f}%)"
            # D. Sudden Dump Defensive Check
            if not is_sell:
                last_change = close_price - close_prices[idx - 1]
                std_dev_now = stddev50[idx]
                if last_change < -2.5 * std_dev_now and std_dev_now > 5.0:
                    is_sell = True
                    sell_reason = "Emergency Sudden Dump Detected"
            
            # E. Normal Strategy Exits (Only if no emergency exit was triggered)
            if not is_sell:
                first_entry_reason = active_positions[0][4] if len(active_positions[0]) > 4 else ""
                
                # E1. Sideways normal exit (BB Upper Band touch)
                if "[Sideways]" in first_entry_reason:
                    latest_buy_time = active_positions[-1][3] if active_positions else 0
                    can_normal_sell = (time_epoch_sec - latest_buy_time) >= 900
                    upper_band = (sma20[idx] + (sideways_bb_mult * stddev20[idx])) if sideways_bb_period == 20 else (sma50[idx] + (sideways_bb_mult * stddev50[idx]))
                    if close_price >= upper_band and can_normal_sell:
                        is_sell = True
                        sell_reason = f"[Sideways] Sentuh Atas BB (Period {sideways_bb_period})"
                
                # E2. Downtrend normal exit
                elif "[Downtrend]" in first_entry_reason:
                    latest_buy_time = active_positions[-1][3] if active_positions else 0
                    can_normal_sell = (time_epoch_sec - latest_buy_time) >= downtrend_hold_lock
                    is_sell_signal = (ema13[idx] < ema34[idx])
                    if is_sell_signal:
                        death_cross_streak += 1
                        if death_cross_streak >= 2 and can_normal_sell:
                            is_sell = True
                            sell_reason = "[Downtrend] Quant EMA13/34 Death Cross confirmed"
                    else:
                        death_cross_streak = 0

                # E3. Breakout normal exit
                elif "[Breakout]" in first_entry_reason:
                    latest_buy_time = active_positions[-1][3] if active_positions else 0
                    can_normal_sell = (time_epoch_sec - latest_buy_time) >= 1200 # 20m lock
                    # Exit mode A breakout
                    is_sell_signal = (ema13[idx] < ema34[idx] and close_price < vwap) or (not trend_15m_bullish)
                    if is_sell_signal:
                        death_cross_streak += 1
                        if death_cross_streak >= 2 and can_normal_sell:
                            is_sell = True
                            sell_reason = "[Breakout] Quant Exit Conditions confirmed"
                    else:
                        death_cross_streak = 0

                # E4. Trending (Uptrend) normal exit
                elif "[Trending]" in first_entry_reason:
                    latest_buy_time = active_positions[-1][3] if active_positions else 0
                    can_normal_sell = (time_epoch_sec - latest_buy_time) >= uptrend_lock_duration
                    is_sell_signal = (ema13[idx] < ema34[idx]) and (close_price < vwap or not trend_15m_bullish)
                    if is_sell_signal:
                        death_cross_streak += 1
                        if death_cross_streak >= 2 and can_normal_sell:
                            is_sell = True
                            sell_reason = "[Trending] Death Cross (2m)"
                    else:
                        death_cross_streak = 0
                    
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
                
        # 5-minute buy cooldown
        can_buy = (time_epoch_sec - last_buy_time) >= 300
        
        # A. Sideways Strategy (BB Mean Reversion)
        if is_sideways_market:
            lower_band = (sma20[idx] - (sideways_bb_mult * stddev20[idx])) if sideways_bb_period == 20 else (sma50[idx] - (sideways_bb_mult * stddev50[idx]))
            
            if close_price <= lower_band and len(active_positions) < 2 and can_buy:
                # Hitung momentum penurunan 3 menit terakhir
                rsi_slope_3m = rsi14[idx] - rsi14[idx-3] if idx >= 3 else 0.0
                price_drop_3m = (close_price - close_prices[idx-3]) / close_prices[idx-3] * 100.0 if idx >= 3 else 0.0
                
                is_too_sharp = (mode == "optimized") and (price_drop_3m < -0.48)
                is_too_flat = (mode == "optimized") and (price_drop_3m > -0.18 or rsi_slope_3m > -4.0)
                
                if not is_too_sharp and not is_too_flat:
                    sum_v = sum(volumes[idx-sideways_bb_period:idx])
                    avg_v = sum_v / sideways_bb_period if sum_v > 0 else 1.0
                    vol_surge = volume / avg_v if avg_v > 0 else 1.0
                    
                    if vol_surge <= 1.5:
                        vwap_dist_pct_sw = ((close_price - vwap) / vwap) * 100.0 if vwap > 0 else 0.0
                        is_high_win_pattern = -0.5 <= vwap_dist_pct_sw <= 0.5 and vol_surge >= 1.0
                        
                        if sideways_flat_budget_pct is not None:
                            budget_pct = sideways_flat_budget_pct
                        else:
                            budget_pct = 0.45 if is_high_win_pattern else 0.30
                        
                        budget = balance * budget_pct
                        if budget >= 5.0:
                            buy_fee = budget * 0.001
                            btc_bought = (budget - buy_fee) / close_price
                            balance -= budget
                            btc_balance += btc_bought
                            
                            active_positions.append((close_price, btc_bought, buy_fee, time_epoch_sec, "[Sideways]"))
                            last_buy_time = time_epoch_sec
                            
                            if len(active_positions) == 1:
                                cycle_hwm = close_price
                                
                            label = "[HIGH-WIN]" if is_high_win_pattern else ""
                            trade_logs.append(f"[{dt_str}] BUY Layer {len(active_positions)} @ ${close_price:.2f} (BB Lower Band) | Budget: ${budget:.2f} {label} | Reason: [Sideways] Mean Reversion Entry (Vol Surge: {vol_surge:.1f}x, VWAP Dist: {vwap_dist_pct_sw:.2f}%)")
                    
        # B. Trending Strategy (EMA 13/34 Crossover + VWAP + 15m Trend)
        else:
            e13 = ema13[idx]
            e34 = ema34[idx]
            ema_spread_pct = abs(e13 - e34) / close_price if close_price > 0 else 0.0
            
            # B1. Trending / Uptrend Strategy Entry
            is_sideways_for_uptrend = volatility_pct_50 < (0.25 if mode == "optimized" else 0.085)
            is_market_active = e13 > e34 and close_price > vwap and trend_15m_bullish and len(active_positions) < 2 and can_buy
            if (symbol == "BTCUSDT" and not is_sideways_for_uptrend and is_market_active) or (symbol != "BTCUSDT" and is_market_active):
                sum_v = sum(volumes[idx-50:idx])
                avg_v = sum_v / 50.0 if sum_v > 0 else 1.0
                vol_surge = volume / avg_v if avg_v > 0 else 1.0
                vwap_dist_pct = ((close_price - vwap) / vwap) * 100.0 if vwap > 0 else 0.0
                
                # Filters
                rsi_now = rsi14[idx]
                is_trending_lemas = 50.0 <= rsi_now <= 55.0 and 0.2 <= vwap_dist_pct <= 1.2
                
                rsi_slope = rsi14[idx] - rsi14[idx - 3] if idx >= 3 else 0.0
                is_rsi_accelerating = rsi_slope >= 2.5
                
                current_hour_utc = int(dt_str.split()[1].split(':')[0])
                is_volatile_hour = (8 <= current_hour_utc <= 12) or (16 <= current_hour_utc <= 21)
                vwap_max_allowed = uptrend_vwap_max_volatile if is_volatile_hour else uptrend_vwap_max_normal
                
                gc_dur = golden_cross_duration[idx]
                is_fresh_golden_cross = gc_dur <= uptrend_gc_max_dur
                
                is_rsi_in_range = uptrend_rsi_min <= rsi_now <= uptrend_rsi_max
                is_rsi_danger_zone = uptrend_block_rsi_75_80 and (75.0 <= rsi_now <= 80.0)
                
                rsi_slope_7m = rsi14[idx] - rsi14[idx - 7] if idx >= 7 else 0.0
                is_momentum_too_hot = rsi_slope_7m > uptrend_max_rsi_slope_7m
                
                rsi_slope_15m = rsi14[idx] - rsi14[idx - 15] if idx >= 15 else 0.0
                is_trend_15m_active = rsi_slope_15m >= uptrend_min_rsi_slope_15m
                
                sum_v3 = sum(volumes[idx - 3:idx])
                avg_v3 = sum_v3 / 3.0 if sum_v3 > 0 else 1.0
                vol_surge_3m = volume / avg_v3 if avg_v3 > 0 else 1.0
                is_vol_3m_active = vol_surge_3m >= uptrend_min_vol_surge_3m
                
                if symbol != "BTCUSDT":
                    is_entry_valid = (
                        55.0 <= rsi_now <= 75.0 and
                        rsi_slope_15m >= uptrend_min_rsi_slope_15m and
                        ema_spread_pct >= 0.0015 and
                        vol_surge >= 0.5 and
                        vwap_dist_pct <= 3.5 and
                        gc_dur <= 35
                    )
                else:
                    is_entry_valid = (
                        not is_trending_lemas and
                        is_rsi_accelerating and
                        is_fresh_golden_cross and
                        is_rsi_in_range and
                        not is_rsi_danger_zone and
                        not is_momentum_too_hot and
                        is_trend_15m_active and
                        is_vol_3m_active and
                        ema_spread_pct >= uptrend_ema_spread_min and
                        vwap_dist_pct <= vwap_max_allowed and
                        vol_surge <= 5.0
                    )

                if is_entry_valid:
                    if uptrend_is_dynamic_sizing:
                        size_pct = 0.40 if rsi_slope_15m >= 8.0 else 0.10
                    else:
                        size_pct = 0.20
                        
                    budget = balance * size_pct
                    if budget >= 5.0:
                        buy_fee = budget * 0.001
                        btc_bought = (budget - buy_fee) / close_price
                        balance -= budget
                        btc_balance += btc_bought
                        
                        active_positions.append((close_price, btc_bought, buy_fee, time_epoch_sec, "[Trending]"))
                        last_buy_time = time_epoch_sec
                        
                        if len(active_positions) == 1:
                            cycle_hwm = close_price
                            
                        trade_logs.append(f"[{dt_str}] BUY Layer {len(active_positions)} @ ${close_price:.2f} (EMA Golden Cross) | Budget: ${budget:.2f} | Reason: [Trending] EMA13/34 Buy (Vol Surge: {vol_surge:.1f}x, VWAP Dist: {vwap_dist_pct:.2f}%)")
            
            # B2. Downtrend Strategy Entry
            elif e13 < e34 and len(active_positions) == 0 and can_buy:
                rsi_val = rsi14[idx]
                is_green_streak = close_price > close_prices[idx - 1] and close_prices[idx - 1] > close_prices[idx - 2]
                bb_width_pct = (4.0 * std_dev) / close_price * 100.0
                
                sum_v = sum(volumes[idx-50:idx])
                avg_v = sum_v / 50.0 if sum_v > 0 else 1.0
                vol_surge = volume / avg_v if avg_v > 0 else 1.0
                vwap_dist_dt = ((close_price - vwap) / vwap) * 100.0 if vwap > 0 else 0.0
                
                if (rsi_val < 30.0 and vol_surge >= 3.0 and is_green_streak
                        and bb_width_pct > 0.5 and can_buy
                        and vwap_dist_dt <= downtrend_max_vwap_dist):
                    budget = balance * 0.30
                    if budget >= 5.0:
                        buy_fee = budget * 0.001
                        btc_bought = (budget - buy_fee) / close_price
                        balance -= budget
                        btc_balance += btc_bought
                        
                        active_positions.append((close_price, btc_bought, buy_fee, time_epoch_sec, f"[Downtrend]"))
                        last_buy_time = time_epoch_sec
                        cycle_hwm = close_price
                        
                        trade_logs.append(f"[{dt_str}] BUY Layer 1 @ ${close_price:.2f} (Bearish Climax Rebound) | Budget: ${budget:.2f} | Reason: [Downtrend] Rebound Catcher (RSI {rsi_val:.1f}, VWAP Dist: {vwap_dist_dt:.2f}%)")
                    
        # C. Breakout Sudden Pump Check
        last_change = close_price - close_prices[idx - 1]
        spike_pct_bo = (last_change / close_prices[idx - 1]) * 100.0 if close_prices[idx - 1] > 0 else 0.0
        if (len(active_positions) < 2
                and last_change > 2.5 * std_dev
                and std_dev >= breakout_min_std_dev
                and spike_pct_bo >= breakout_min_spike_pct
                and rsi14[idx] >= breakout_min_rsi
                and can_buy):
            sum_v_bo = sum(volumes[idx-50:idx])
            avg_v_bo = sum_v_bo / 50.0 if sum_v_bo > 0 else 1.0
            vol_surge_bo = volume / avg_v_bo if avg_v_bo > 0 else 1.0
            vwap_dist_bo = ((close_price - vwap) / vwap) * 100.0 if vwap > 0 else 0.0
            is_fake_breakout = vol_surge_bo <= 3.0 and vwap_dist_bo <= 0.8
            if not is_fake_breakout:
                if vwap_dist_bo > breakout_max_vwap_dist:
                    pass
                else:
                    e13_gap_pct = (ema13[idx] - ema34[idx]) / ema34[idx] * 100.0 if ema34[idx] > 0 else 0.0
                    if spike_pct_bo > 0.6 and e13_gap_pct < breakout_ema_gap_if_big:
                        pass
                    else:
                        budget = balance * 0.25
                        if budget >= 5.0:
                            buy_fee = budget * 0.001
                            btc_bought = (budget - buy_fee) / close_price
                            balance -= budget
                            btc_balance += btc_bought
                            
                            active_positions.append((close_price, btc_bought, buy_fee, time_epoch_sec, "[Breakout]"))
                            last_buy_time = time_epoch_sec
                            
                            if len(active_positions) == 1:
                                cycle_hwm = close_price
                                
                            trade_logs.append(f"[{dt_str}] BUY Layer {len(active_positions)} @ ${close_price:.2f} (Sudden Pump Breakout) | Budget: ${budget:.2f} | Reason: [Breakout] Sudden Price Spike (Vol Surge: {vol_surge_bo:.1f}x, VWAP Dist: {vwap_dist_bo:.2f}%, Spike: {spike_pct_bo:.2f}%)")

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
        "cycles": completed_cycles,  # Save all completed cycles
        "equity_curve": equity_curve,
        "logs": trade_logs  # Save all logs
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
