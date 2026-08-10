import sys, os, time, json, psycopg2
from datetime import datetime, timezone
from urllib.parse import urlparse, unquote
from dotenv import load_dotenv
from coin_classifier import classify_coin, get_params_for_category

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
        if diff >= 0.0: gains[i] = diff
        else: losses[i] = -diff
    avg_gain = sum(gains[1:period+1]) / period
    avg_loss = sum(losses[1:period+1]) / period
    if avg_loss == 0.0: rsis[period] = 100.0
    else:
        rs = avg_gain / avg_loss
        rsis[period] = 100.0 - (100.0 / (1.0 + rs))
    for i in range(period + 1, len(prices)):
        avg_gain = (avg_gain * (period - 1) + gains[i]) / period
        avg_loss = (avg_loss * (period - 1) + losses[i]) / period
        if avg_loss == 0.0: rsis[i] = 100.0
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
            variance = max(0.0, (running_sq_sum / period) - (mean * mean))
            smas[i] = mean
            stddevs[i] = variance ** 0.5
    return smas, stddevs

def main():
    starting_balance = float(sys.argv[1]) if len(sys.argv) > 1 else 1000.0
    tp_hard = float(sys.argv[2]) if len(sys.argv) > 2 else 0.03
    stop_loss_limit = float(sys.argv[3]) if len(sys.argv) > 3 else -0.015

    mode = sys.argv[4].lower() if len(sys.argv) > 4 else "optimized"
    if mode not in ["safe", "optimized"]:
        mode = "optimized"
    symbol = sys.argv[5].upper() if len(sys.argv) > 5 else "BTCUSDT"

    print(f"[Backtest Uptrend] Running in {mode.upper()} mode for symbol {symbol}...")
    print(f"[Backtest Uptrend] Loading historical klines for symbol {symbol}...")
    try:
        db_params = parse_db_url(DATABASE_URL)
        conn = psycopg2.connect(**db_params)
        cursor = conn.cursor()
        
        # Check if the symbol exists in dca_klines
        cursor.execute("SELECT COUNT(*) FROM dca_klines WHERE symbol = %s", (symbol,))
        count = cursor.fetchone()[0]
        
        if count > 0:
            print(f"[Backtest Uptrend] Loading data for {symbol} from dca_klines ({count:,} rows)...")
            cursor.execute(
                "SELECT open_time, open_price, high_price, low_price, close_price, volume "
                "FROM dca_klines WHERE symbol = %s ORDER BY open_time ASC", 
                (symbol,)
            )
        else:
            if symbol == "BTCUSDT":
                print("[Backtest Uptrend] Loading data for BTCUSDT from btc_klines...")
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
    close_prices = [k[4] for k in klines]
    volumes = [k[5] for k in klines]

    ema13 = calculate_emas(close_prices, 13)
    ema34 = calculate_emas(close_prices, 34)
    rsi14 = calculate_rsis(close_prices, 14)
    sma50, stddev50 = calculate_bollinger_bands(close_prices, 50)

    # Pre-calculate Golden Cross Duration per candle
    # Berapa menit EMA13 sudah berada di atas EMA34 secara beruntun
    golden_cross_duration = [0] * total_candles
    for i in range(1, total_candles):
        if ema13[i] > ema34[i]:
            golden_cross_duration[i] = golden_cross_duration[i-1] + 1
        else:
            golden_cross_duration[i] = 0

    # Set filter parameters based on mode
    if mode == "safe":
        gc_max_dur = 15
        rsi_min = 0.0
        rsi_max = 100.0
        block_rsi_75_80 = True
        max_rsi_slope_7m = 8.0
        min_rsi_slope_15m = -999.0
        min_vol_surge_3m = 0.0
        is_dynamic_sizing = False
        tp_trail_trigger = 0.006
        tp_trail_pullback = 0.003
        ema_spread_min = 0.0005
    else:  # optimized
        category = classify_coin(symbol)
        print(f"[Backtest Uptrend] Auto-detected volatility category for {symbol}: {category}")
        params = get_params_for_category(category, symbol)
        
        gc_max_dur = params["uptrend_gc_max_dur"]
        rsi_min = params["uptrend_rsi_min"]
        rsi_max = params["uptrend_rsi_max"]
        block_rsi_75_80 = params["uptrend_block_rsi_75_80"]
        max_rsi_slope_7m = params["uptrend_max_rsi_slope_7m"]
        min_rsi_slope_15m = params["uptrend_min_rsi_slope_15m"]
        min_vol_surge_3m = params["uptrend_min_vol_surge_3m"]
        is_dynamic_sizing = params["uptrend_is_dynamic_sizing"]
        tp_trail_trigger = params["uptrend_tp_trail_trigger"]
        tp_trail_pullback = params["uptrend_tp_trail_pullback"]
        stop_loss_limit = params["stop_loss_limit"]
        ema_spread_min = params["uptrend_ema_spread_min"]

    balance = starting_balance
    btc_balance = 0.0
    active_positions = []
    cycle_id = 1
    cycle_hwm = 0.0
    last_buy_time = 0
    death_cross_streak = 0
    completed_cycles = []
    equity_curve = []
    trade_logs = []
    last_day = None
    vwap_volume_sum = 0.0
    vwap_pv_sum = 0.0

    for idx in range(50, total_candles):
        open_time = klines[idx][0]
        close_price = klines[idx][4]
        volume = klines[idx][5]
        time_epoch_sec = int(open_time.timestamp())
        dt_str = open_time.strftime('%Y-%m-%d %H:%M:%S')

        candle_day = open_time.date()
        if candle_day != last_day:
            last_day = candle_day
            vwap_volume_sum = 0.0
            vwap_pv_sum = 0.0
        vwap_volume_sum += volume
        vwap_pv_sum += close_price * volume
        vwap = vwap_pv_sum / vwap_volume_sum if vwap_volume_sum > 0 else close_price

        std_dev = stddev50[idx]
        trend_15m_bullish = close_price > close_prices[idx - 15]
        e13 = ema13[idx]
        e34 = ema34[idx]

        # 1. EXITS EVALUATION
        if len(active_positions) > 0:
            total_spent = sum(pos[0] * pos[1] for pos in active_positions)
            total_btc = sum(pos[1] for pos in active_positions)
            avg_entry = total_spent / total_btc if total_btc > 0 else close_price

            if close_price > cycle_hwm:
                cycle_hwm = close_price

            current_profit_pct = (close_price - avg_entry) / avg_entry
            peak_profit_pct = (cycle_hwm - avg_entry) / avg_entry if cycle_hwm > 0 else 0.0

            is_sell = False
            sell_reason = ""

            # Emergency SL / TP
            if current_profit_pct <= stop_loss_limit:
                is_sell = True
                sell_reason = f"Emergency Stop Loss {stop_loss_limit*100:.1f}%"
            elif current_profit_pct >= tp_hard:
                is_sell = True
                sell_reason = f"Emergency Hard Take Profit +{tp_hard*100:.1f}%"
            # Fast Trailing TP dinamis berdasarkan mode
            elif cycle_hwm > 0 and peak_profit_pct >= tp_trail_trigger:
                drop_from_hwm = (cycle_hwm - close_price) / cycle_hwm
                if drop_from_hwm >= tp_trail_pullback:
                    is_sell = True
                    sell_reason = f"Fast Trailing TP (Peak: +{peak_profit_pct*100:.2f}%, Drop: {drop_from_hwm*100:.1f}%)"
            # Sudden Dump Defense
            else:
                last_change = close_price - close_prices[idx - 1]
                if last_change < -2.5 * std_dev and std_dev > 5.0:
                    is_sell = True
                    sell_reason = "Emergency Sudden Dump Detected"

            # Normal Death Cross Exit
            if not is_sell:
                latest_buy_time = active_positions[-1][3]
                lock_duration = 1200 if symbol != "BTCUSDT" else 900
                can_normal_sell = (time_epoch_sec - latest_buy_time) >= lock_duration
                is_sell_signal = (e13 < e34) and (close_price < vwap or not trend_15m_bullish)
                if is_sell_signal:
                    death_cross_streak += 1
                    if death_cross_streak >= 2 and can_normal_sell:
                        is_sell = True
                        sell_reason = "[Uptrend] Quant EMA13/34 Death Cross confirmed"
                else:
                    death_cross_streak = 0

            if is_sell:
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
                btc_balance = 0.0
                cycle_hwm = 0.0
                active_positions = []
                cycle_id += 1
                death_cross_streak = 0
                continue

        volatility_pct = (std_dev / close_price) * 100.0 if close_price > 0 else 0.0
        is_sideways = volatility_pct < 0.25

        # 2. ENTRY EVALUATION (Uptrend Only - Requires trending market)
        can_buy = (time_epoch_sec - last_buy_time) >= 300
        is_market_active = e13 > e34 and close_price > vwap and trend_15m_bullish and len(active_positions) < 2 and can_buy
        if (symbol == "BTCUSDT" and not is_sideways and is_market_active) or (symbol != "BTCUSDT" and is_market_active):
            sum_v = sum(volumes[idx-50:idx])
            avg_v = sum_v / 50.0 if sum_v > 0 else 1.0
            vol_surge = volume / avg_v if avg_v > 0 else 1.0
            vwap_dist_pct = ((close_price - vwap) / vwap) * 100.0 if vwap > 0 else 0.0
            ema_spread_pct = abs(e13 - e34) / close_price if close_price > 0 else 0.0

            # Filter 1: RSI Lemas (50-55) + VWAP Dist tipis
            rsi_now = rsi14[idx]
            is_trending_lemas = 50.0 <= rsi_now <= 55.0 and 0.2 <= vwap_dist_pct <= 1.2
            # Filter 2: RSI Slope >= +2.5
            rsi_slope = rsi14[idx] - rsi14[idx - 3] if idx >= 3 else 0.0
            is_rsi_accelerating = rsi_slope >= 2.5
            # Filter 3: Volatile Hours (08-12 & 16-21 UTC) VWAP threshold
            current_hour_utc = int(dt_str.split()[1].split(':')[0])
            is_volatile_hour = (8 <= current_hour_utc <= 12) or (16 <= current_hour_utc <= 21)
            vwap_max_allowed = 0.5 if is_volatile_hour else 1.5

            # [BARU] Filter A: Durasi Golden Cross dinamis berdasarkan mode
            gc_dur = golden_cross_duration[idx]
            is_fresh_golden_cross = gc_dur <= gc_max_dur

            # [BARU] Filter B: Cek batas RSI dinamis berdasarkan mode
            is_rsi_in_range = rsi_min <= rsi_now <= rsi_max
            is_rsi_danger_zone = block_rsi_75_80 and (75.0 <= rsi_now <= 80.0)

            # [BARU] Filter C: RSI Slope 7m dinamis berdasarkan mode
            rsi_slope_7m = rsi14[idx] - rsi14[idx - 7] if idx >= 7 else 0.0
            is_momentum_too_hot = rsi_slope_7m > max_rsi_slope_7m

            # [BARU] Filter D: RSI Slope 15m harus naik (tren jangka menengah aktif)
            rsi_slope_15m = rsi14[idx] - rsi14[idx - 15] if idx >= 15 else 0.0
            is_trend_15m_active = rsi_slope_15m >= min_rsi_slope_15m

            # [BARU] Filter E: Volume Surge 3m tidak boleh mati total
            sum_v3 = sum(volumes[idx - 3:idx])
            avg_v3 = sum_v3 / 3.0 if sum_v3 > 0 else 1.0
            vol_surge_3m = volume / avg_v3 if avg_v3 > 0 else 1.0
            is_vol_3m_active = vol_surge_3m >= min_vol_surge_3m

            if symbol != "BTCUSDT":
                is_entry_valid = (
                    55.0 <= rsi_now <= 75.0 and
                    rsi_slope_15m >= min_rsi_slope_15m and
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
                    ema_spread_pct >= ema_spread_min and
                    vwap_dist_pct <= vwap_max_allowed and
                    vol_surge <= 5.0
                )

            if is_entry_valid:
                # Hitung alokasi modal dinamis
                if is_dynamic_sizing:
                    size_pct = 0.40 if rsi_slope_15m >= 8.0 else 0.10
                else:
                    size_pct = 0.20
                    
                budget = balance * size_pct
                if budget >= 5.0:
                    buy_fee = budget * 0.001
                    btc_bought = (budget - buy_fee) / close_price
                    balance -= budget
                    btc_balance += btc_bought
                    active_positions.append((close_price, btc_bought, buy_fee, time_epoch_sec, "[Uptrend]"))
                    last_buy_time = time_epoch_sec
                    if len(active_positions) == 1:
                        cycle_hwm = close_price
                    trade_logs.append(f"[{dt_str}] BUY Layer {len(active_positions)} @ ${close_price:.2f} (EMA Golden Cross) | Budget: ${budget:.2f} | Reason: [Uptrend] EMA13/34 Buy (Vol Surge: {vol_surge:.1f}x, VWAP Dist: {vwap_dist_pct:.2f}%)")

        current_equity = balance + (btc_balance * close_price)
        if idx % 60 == 0 or idx == total_candles - 1:
            equity_curve.append({"time": dt_str, "equity": round(current_equity, 2), "price": close_price})

    final_equity = balance + (btc_balance * close_prices[-1])
    total_net_profit = final_equity - starting_balance
    net_profit_pct = (total_net_profit / starting_balance) * 100.0
    total_trades = len(completed_cycles)
    wins = [c for c in completed_cycles if c["net_pnl"] > 0]
    losses = [c for c in completed_cycles if c["net_pnl"] <= 0]
    win_rate = (len(wins) / total_trades * 100.0) if total_trades > 0 else 0.0
    gross_profits = sum(c["net_pnl"] for c in wins)
    gross_losses = sum(abs(c["net_pnl"]) for c in losses)
    profit_factor = gross_profits / gross_losses if gross_losses > 0 else 1.0

    max_equity = starting_balance
    max_dd = 0.0
    for eq in equity_curve:
        val = eq["equity"]
        if val > max_equity: max_equity = val
        dd = (max_equity - val) / max_equity * 100.0
        if dd > max_dd: max_dd = dd

    print(f"\n================ UPTREND STRATEGY RESULTS ================")
    print(f"  Final Equity   : ${final_equity:.2f} ({net_profit_pct:+.2f}%)")
    print(f"  Total Trades   : {total_trades} (Wins: {len(wins)} | Losses: {len(losses)})")
    print(f"  Win Rate       : {win_rate:.2f}%")
    print(f"  Profit Factor  : {profit_factor:.2f}")
    print(f"  Max Drawdown   : {max_dd:.2f}%")
    print(f"==========================================================\n")

if __name__ == "__main__":
    main()
