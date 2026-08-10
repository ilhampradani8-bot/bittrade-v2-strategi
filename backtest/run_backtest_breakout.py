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
    else: rsis[period] = 100.0 - (100.0 / (1.0 + avg_gain / avg_loss))
    for i in range(period + 1, len(prices)):
        avg_gain = (avg_gain * (period - 1) + gains[i]) / period
        avg_loss = (avg_loss * (period - 1) + losses[i]) / period
        if avg_loss == 0.0: rsis[i] = 100.0
        else: rsis[i] = 100.0 - (100.0 / (1.0 + avg_gain / avg_loss))
    return rsis

def main():
    starting_balance = float(sys.argv[1]) if len(sys.argv) > 1 else 1000.0
    tp_hard = float(sys.argv[2]) if len(sys.argv) > 2 else 0.03
    stop_loss_limit = float(sys.argv[3]) if len(sys.argv) > 3 else -0.015
    mode = sys.argv[4] if len(sys.argv) > 4 else "safe"
    symbol = sys.argv[5].upper() if len(sys.argv) > 5 else "BTCUSDT"

    print(f"[Backtest Breakout] Running in {mode.upper()} mode for symbol {symbol}...")
    print(f"[Backtest Breakout] Loading historical klines for symbol {symbol}...")
    try:
        db_params = parse_db_url(DATABASE_URL)
        conn = psycopg2.connect(**db_params)
        cursor = conn.cursor()
        
        # Check if the symbol exists in dca_klines
        cursor.execute("SELECT COUNT(*) FROM dca_klines WHERE symbol = %s", (symbol,))
        count = cursor.fetchone()[0]
        
        if count > 0:
            print(f"[Backtest Breakout] Loading data for {symbol} from dca_klines ({count:,} rows)...")
            cursor.execute(
                "SELECT open_time, open_price, high_price, low_price, close_price, volume "
                "FROM dca_klines WHERE symbol = %s ORDER BY open_time ASC", 
                (symbol,)
            )
        else:
            if symbol == "BTCUSDT":
                print("[Backtest Breakout] Loading data for BTCUSDT from btc_klines...")
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
    sma50, stddev50 = calculate_bollinger_bands(close_prices, 50)
    rsi14 = calculate_rsis(close_prices, 14)

    # Set parameters based on mode
    if mode == "safe":
        min_std_dev_pct = 0.0077
        min_spike_pct = 0.0
        min_rsi = 0.0
        max_vwap_dist = 999.0
        min_ema_gap_if_big_spike = -999.0
    else: # optimized
        category = classify_coin(symbol)
        print(f"[Backtest Breakout] Auto-detected volatility category for {symbol}: {category}")
        params = get_params_for_category(category, symbol)
        
        min_std_dev_pct = params["breakout_min_std_dev"]
        min_spike_pct = params["breakout_min_spike_pct"]
        min_rsi = params["breakout_min_rsi"]
        max_vwap_dist = params["breakout_max_vwap_dist"]
        min_ema_gap_if_big_spike = params["breakout_ema_gap_if_big"]

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
            # Standard Trailing TP (+1.5% trigger, 1.0% pullback)
            elif cycle_hwm > 0 and peak_profit_pct >= 0.015:
                drop_from_hwm = (cycle_hwm - close_price) / cycle_hwm
                if drop_from_hwm >= 0.010:
                    is_sell = True
                    sell_reason = f"Emergency Trailing Take Profit (Peak: +{peak_profit_pct*100:.2f}%, Drop: {drop_from_hwm*100:.1f}%)"
            # Sudden Dump Defense
            else:
                last_change = close_price - close_prices[idx - 1]
                if last_change < -2.5 * std_dev and std_dev > 5.0:
                    is_sell = True
                    sell_reason = "Emergency Sudden Dump Detected"

            # Normal Death Cross Exit
            if not is_sell:
                latest_buy_time = active_positions[-1][3]
                can_normal_sell = (time_epoch_sec - latest_buy_time) >= 900
                is_sell_signal = (e13 < e34 and close_price < vwap) or (not trend_15m_bullish)
                if is_sell_signal:
                    death_cross_streak += 1
                    if death_cross_streak >= 2 and can_normal_sell:
                        is_sell = True
                        sell_reason = "[Breakout] Quant EMA13/34 Death Cross confirmed"
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

        # 2. ENTRY EVALUATION (Breakout Only)
        can_buy = (time_epoch_sec - last_buy_time) >= 300
        last_change = close_price - close_prices[idx - 1]
        
        # Calculate 1-minute spike percentage
        spike_pct = (last_change / close_prices[idx - 1]) * 100.0 if close_prices[idx - 1] > 0 else 0.0
        std_dev_pct = (std_dev / close_price) * 100.0 if close_price > 0 else 0.0
        
        if (last_change > 2.5 * std_dev
                and std_dev_pct >= min_std_dev_pct
                and spike_pct >= min_spike_pct
                and rsi14[idx] >= min_rsi
                and len(active_positions) < 2
                and can_buy):
            sum_v_bo = sum(volumes[idx-50:idx])
            avg_v_bo = sum_v_bo / 50.0 if sum_v_bo > 0 else 1.0
            vol_surge_bo = volume / avg_v_bo if avg_v_bo > 0 else 1.0
            vwap_dist_bo = ((close_price - vwap) / vwap) * 100.0 if vwap > 0 else 0.0

            # Anti Fake-Breakout Filter
            is_fake_breakout = vol_surge_bo <= 3.0 and vwap_dist_bo <= 0.8
            if not is_fake_breakout:
                # FILTER MULTI-INDIKATOR BARU (mode optimized)
                if mode == "optimized":
                    if vwap_dist_bo > max_vwap_dist:
                        continue
                    e13_gap_pct = (e13 - e34) / e34 * 100.0 if e34 > 0 else 0.0
                    if spike_pct > 0.6 and e13_gap_pct < min_ema_gap_if_big_spike:
                        continue

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
                    trade_logs.append(f"[{dt_str}] BUY Layer {len(active_positions)} @ ${close_price:.2f} (Sudden Pump Breakout) | Budget: ${budget:.2f} | Reason: [Breakout] Sudden Price Spike (Vol Surge: {vol_surge_bo:.1f}x, VWAP Dist: {vwap_dist_bo:.2f}%)")

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

    print(f"\n================ BREAKOUT STRATEGY RESULTS ================")
    print(f"  Final Equity   : ${final_equity:.2f} ({net_profit_pct:+.2f}%)")
    print(f"  Total Trades   : {total_trades} (Wins: {len(wins)} | Losses: {len(losses)})")
    print(f"  Win Rate       : {win_rate:.2f}%")
    print(f"  Profit Factor  : {profit_factor:.2f}")
    print(f"  Max Drawdown   : {max_dd:.2f}%")
    print(f"===========================================================\n")

if __name__ == "__main__":
    main()
