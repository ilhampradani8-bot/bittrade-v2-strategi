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
    gains, losses_arr = [0.0] * len(prices), [0.0] * len(prices)
    for i in range(1, len(prices)):
        diff = prices[i] - prices[i-1]
        if diff >= 0.0: gains[i] = diff
        else: losses_arr[i] = -diff
    avg_gain = sum(gains[1:period+1]) / period
    avg_loss = sum(losses_arr[1:period+1]) / period
    if avg_loss == 0.0: rsis[period] = 100.0
    else: rsis[period] = 100.0 - (100.0 / (1.0 + avg_gain / avg_loss))
    for i in range(period + 1, len(prices)):
        avg_gain = (avg_gain * (period - 1) + gains[i]) / period
        avg_loss = (avg_loss * (period - 1) + losses_arr[i]) / period
        if avg_loss == 0.0: rsis[i] = 100.0
        else: rsis[i] = 100.0 - (100.0 / (1.0 + avg_gain / avg_loss))
    return rsis

def main():
    starting_balance = float(sys.argv[1]) if len(sys.argv) > 1 else 1000.0
    tp_hard = float(sys.argv[2]) if len(sys.argv) > 2 else 0.03
    stop_loss_limit = float(sys.argv[3]) if len(sys.argv) > 3 else -0.015
    mode = sys.argv[4] if len(sys.argv) > 4 else "safe"
    symbol = sys.argv[5].upper() if len(sys.argv) > 5 else "BTCUSDT"

    print(f"[Backtest Sideways] Running in {mode.upper()} mode for symbol {symbol}...")
    print(f"[Backtest Sideways] Loading historical klines for symbol {symbol}...")
    try:
        db_params = parse_db_url(DATABASE_URL)
        conn = psycopg2.connect(**db_params)
        cursor = conn.cursor()
        
        # Check if the symbol exists in dca_klines
        cursor.execute("SELECT COUNT(*) FROM dca_klines WHERE symbol = %s", (symbol,))
        count = cursor.fetchone()[0]
        
        if count > 0:
            print(f"[Backtest Sideways] Loading data for {symbol} from dca_klines ({count:,} rows)...")
            cursor.execute(
                "SELECT open_time, open_price, high_price, low_price, close_price, volume "
                "FROM dca_klines WHERE symbol = %s ORDER BY open_time ASC", 
                (symbol,)
            )
        else:
            if symbol == "BTCUSDT":
                print("[Backtest Sideways] Loading data for BTCUSDT from btc_klines...")
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

    # Set parameters based on mode
    if mode == "safe":
        bb_period = 50
        bb_mult = 2.0
        min_vol_pct = 0.0
        max_vol_pct = 0.085
        flat_budget_pct = None  # Gunakan budget dinamis 30%-45%
    else:  # optimized
        category = classify_coin(symbol)
        print(f"[Backtest Sideways] Auto-detected volatility category for {symbol}: {category}")
        params = get_params_for_category(category, symbol)
        
        bb_period = params["sideways_bb_period"]
        bb_mult = params["sideways_bb_mult"]
        min_vol_pct = params["sideways_min_vol_pct"]
        max_vol_pct = params["sideways_max_vol_pct"]
        flat_budget_pct = params["sideways_flat_budget_pct"]

    sma, stddev = calculate_bollinger_bands(close_prices, bb_period)
    rsi14 = calculate_rsis(close_prices, 14)

    balance = starting_balance
    btc_balance = 0.0
    active_positions = []
    cycle_id = 1
    cycle_hwm = 0.0
    last_buy_time = 0
    completed_cycles = []
    equity_curve = []
    trade_logs = []
    last_day = None
    vwap_volume_sum = 0.0
    vwap_pv_sum = 0.0

    for idx in range(bb_period, total_candles):
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

        std_dev = stddev[idx]
        volatility_pct = (std_dev / close_price) * 100.0 if close_price > 0 else 0.0
        is_sideways = min_vol_pct <= volatility_pct < max_vol_pct

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

            # Normal BB Upper Band Exit
            if not is_sell:
                latest_buy_time = active_positions[-1][3]
                can_normal_sell = (time_epoch_sec - latest_buy_time) >= 900
                upper_band = sma[idx] + (bb_mult * std_dev)
                if close_price >= upper_band and can_normal_sell:
                    is_sell = True
                    sell_reason = f"[Sideways] Sentuh Atas BB (Mult: {bb_mult})"

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
                continue

        # 2. ENTRY EVALUATION (Sideways Only)
        can_buy = (time_epoch_sec - last_buy_time) >= 300
        if is_sideways:
            lower_band = sma[idx] - (bb_mult * std_dev)
            if close_price <= lower_band and len(active_positions) < 2 and can_buy:
                # Hitung momentum penurunan 3 menit terakhir
                rsi_slope_3m = rsi14[idx] - rsi14[idx-3] if idx >= 3 else 0.0
                price_drop_3m = (close_price - close_prices[idx-3]) / close_prices[idx-3] * 100.0 if idx >= 3 else 0.0
                
                # Cek 3 filter eksklusif di mode optimized
                is_too_sharp = (mode == "optimized") and (price_drop_3m < -0.48)
                is_too_flat = (mode == "optimized") and (price_drop_3m > -0.18 or rsi_slope_3m > -4.0)
                
                if not is_too_sharp and not is_too_flat:
                    sum_v = sum(volumes[idx-bb_period:idx])
                    avg_v = sum_v / bb_period if sum_v > 0 else 1.0
                    vol_surge = volume / avg_v if avg_v > 0 else 1.0

                    if vol_surge <= 1.5:
                        vwap_dist_pct_sw = ((close_price - vwap) / vwap) * 100.0 if vwap > 0 else 0.0
                        is_high_win_pattern = -0.5 <= vwap_dist_pct_sw <= 0.5 and vol_surge >= 1.0
                        
                        if flat_budget_pct is not None:
                            budget_pct = flat_budget_pct
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

    print(f"\n================ SIDEWAYS STRATEGY RESULTS ================")
    print(f"  Final Equity   : ${final_equity:.2f} ({net_profit_pct:+.2f}%)")
    print(f"  Total Trades   : {total_trades} (Wins: {len(wins)} | Losses: {len(losses)})")
    print(f"  Win Rate       : {win_rate:.2f}%")
    print(f"  Profit Factor  : {profit_factor:.2f}")
    print(f"  Max Drawdown   : {max_dd:.2f}%")
    print(f"===========================================================\n")

if __name__ == "__main__":
    main()
