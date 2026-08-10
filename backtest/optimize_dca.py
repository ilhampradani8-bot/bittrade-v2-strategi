#!/usr/bin/env python3
import os
import psycopg2
from urllib.parse import urlparse, unquote
from dotenv import load_dotenv

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

def run_simulation(klines, l1_drop, l2_drop, l3_drop, tp, sl, rsi_l1, rsi_l2, rsi_l3):
    balance = 1000.0
    btc_balance = 0.0
    layers_filled = 0
    cycle_hwm = 0.0
    active_positions = []
    
    wins = 0
    losses = 0
    
    close_prices = [k[4] for k in klines]
    volumes = [k[5] for k in klines]
    high_prices = [k[2] for k in klines]
    
    for idx in range(240, len(klines)):
        close_price = close_prices[idx]
        volume = volumes[idx]
        
        high_4h = max(high_prices[idx-240:idx])
        drop_pct = (close_price - high_4h) / high_4h * 100.0
        rsi = calculate_rsi(close_prices[idx-14:idx+1], 14)
        
        # Exit check
        if layers_filled > 0:
            if close_price > cycle_hwm:
                cycle_hwm = close_price
                
            total_btc = sum(pos["amount"] for pos in active_positions)
            total_usdt_spent = sum(pos["usdt_spent"] for pos in active_positions)
            avg_entry = sum(pos["price"] * pos["amount"] for pos in active_positions) / total_btc
            current_profit_pct = (close_price - avg_entry) / avg_entry
            
            is_exit = False
            exit_reason = ""
            
            if current_profit_pct <= sl:
                is_exit = True
                exit_reason = "SL"
            elif current_profit_pct >= tp + 0.01: # Hard TP
                is_exit = True
                exit_reason = "TP"
            elif current_profit_pct >= tp and cycle_hwm > 0.0:
                # Trailing TP
                drop_from_hwm = (cycle_hwm - close_price) / cycle_hwm
                if drop_from_hwm >= 0.008:
                    is_exit = True
                    exit_reason = "TTP"
                    
            if is_exit:
                usdt_returned = (close_price * total_btc) * 0.999
                net_pnl = usdt_returned - (total_usdt_spent * 0.999)
                if net_pnl > 0:
                    wins += 1
                else:
                    losses += 1
                    
                balance += usdt_returned
                btc_balance = 0.0
                layers_filled = 0
                cycle_hwm = 0.0
                active_positions = []
                
        # Entry check
        if layers_filled < 3:
            # Layer 1
            if drop_pct <= l1_drop and layers_filled == 0 and rsi < rsi_l1:
                spend = balance * 0.40
                btc_bought = (spend * 0.999) / close_price
                balance -= spend
                btc_balance += btc_bought
                layers_filled = 1
                cycle_hwm = close_price
                active_positions.append({"price": close_price, "amount": btc_bought, "usdt_spent": spend})
            # Layer 2
            elif drop_pct <= l2_drop and layers_filled == 1 and rsi < rsi_l2:
                spend = balance * 0.30
                btc_bought = (spend * 0.999) / close_price
                balance -= spend
                btc_balance += btc_bought
                layers_filled = 2
                cycle_hwm = max(cycle_hwm, close_price)
                active_positions.append({"price": close_price, "amount": btc_bought, "usdt_spent": spend})
            # Layer 3
            elif drop_pct <= l3_drop and layers_filled == 2 and rsi < rsi_l3:
                spend = balance * 0.30
                btc_bought = (spend * 0.999) / close_price
                balance -= spend
                btc_balance += btc_bought
                layers_filled = 3
                cycle_hwm = max(cycle_hwm, close_price)
                active_positions.append({"price": close_price, "amount": btc_bought, "usdt_spent": spend})
                
    total_cycles = wins + losses
    winrate = (wins / total_cycles * 100.0) if total_cycles > 0 else 0.0
    return winrate, total_cycles, balance

def main():
    db_params = parse_db_url(DATABASE_URL)
    conn = psycopg2.connect(**db_params)
    cursor = conn.cursor()
    cursor.execute("SELECT open_time, open_price, high_price, low_price, close_price, volume FROM btc_klines ORDER BY open_time ASC")
    klines = cursor.fetchall()
    cursor.close()
    conn.close()
    
    print(f"Optimizing DCA on {len(klines):,} klines...")
    
    # Grid search parameters
    l1_drops = [-2.0, -3.0]
    l2_drops = [-4.0, -5.0]
    l3_drops = [-6.0, -8.0, -10.0]
    tps = [0.008, 0.010, 0.012, 0.015] # 0.8%, 1.0%, 1.2%, 1.5%
    sls = [-0.04, -0.05, -0.07, -0.10] # -4%, -5%, -7%, -10%
    rsi_l1s = [50, 60]
    
    best_winrate = 0.0
    best_params = None
    
    count = 0
    for l1 in l1_drops:
        for l2 in l2_drops:
            for l3 in l3_drops:
                for tp in tps:
                    for sl in sls:
                        for rsi1 in rsi_l1s:
                            count += 1
                            wr, cyc, final = run_simulation(klines, l1, l2, l3, tp, sl, rsi1, 50, 40)
                            if wr >= 90.0:
                                print(f"MATCH: Winrate {wr:.2f}% | Cycles: {cyc} | Final: ${final:.2f} | L1:{l1} L2:{l2} L3:{l3} TP:{tp*100:.1f}% SL:{sl*100:.1f}% RSI1:{rsi1}")
                            if wr > best_winrate:
                                best_winrate = wr
                                best_params = (l1, l2, l3, tp, sl, rsi1, cyc, final)
                                
    print(f"\nDone! Best Winrate: {best_winrate:.2f}%")
    if best_params:
        print(f"Params: L1:{best_params[0]} L2:{best_params[1]} L3:{best_params[2]} TP:{best_params[3]*100:.1f}% SL:{best_params[4]*100:.1f}% RSI1:{best_params[5]} | Cycles: {best_params[6]} | Final: ${best_params[7]:.2f}")

if __name__ == "__main__":
    main()
