import psycopg2
import pandas as pd
import numpy as np

def main():
    conn = psycopg2.connect("postgresql://bottrade_user:%40Dani22334455D@localhost:5432/bottrade_db")
    
    # 1. Fetch trades
    query = """
    WITH opens AS (
        SELECT id, pair_name, action, z_score, ratio, price_a, price_b, amount_a, amount_b, timestamp, notes
        FROM starb_trading_history
        WHERE action LIKE 'OPEN_%'
    ),
    closes AS (
        SELECT id, pair_name, action, z_score, ratio, price_a, price_b, amount_a, amount_b, net_pnl, timestamp, notes
        FROM starb_trading_history
        WHERE action LIKE 'CLOSE_%'
    )
    SELECT 
        o.id as open_id, o.timestamp as open_time, o.z_score as open_z, o.amount_a, o.price_a, o.amount_b, o.price_b, 
        c.net_pnl, c.timestamp as close_time, c.notes
    FROM opens o
    JOIN closes c ON c.id > o.id AND c.pair_name = o.pair_name
    WHERE NOT EXISTS (
        SELECT 1 FROM opens o2 WHERE o2.id > o.id AND o2.id < c.id AND o2.pair_name = o.pair_name
    )
    """
    trades = pd.read_sql(query, conn)
    print(f"Loaded {len(trades)} completed trades from history.")
    
    # 2. Get rolling_std for each trade
    # Fetch all stats
    stats = pd.read_sql("SELECT timestamp, rolling_std FROM starb_pair_stats ORDER BY timestamp", conn)
    stats['timestamp'] = pd.to_datetime(stats['timestamp'], utc=True)
    trades['open_time'] = pd.to_datetime(trades['open_time'], utc=True)
    
    # Sort for merge_asof
    stats = stats.sort_values('timestamp')
    trades = trades.sort_values('open_time')
    
    merged = pd.merge_asof(trades, stats, left_on='open_time', right_on='timestamp', direction='backward')
    
    # 3. Calculate predicted vs realized
    # deployed_usdt = amount_a * price_a + amount_b * price_b
    merged['deployed_usdt'] = merged['amount_a'] * merged['price_a'] + merged['amount_b'] * merged['price_b']
    
    z_exit_threshold = 0.2
    # reversion_distance = |open_z| - z_exit_threshold (approx)
    merged['reversion_distance'] = merged['open_z'].abs() - z_exit_threshold
    
    merged['predicted_capture'] = merged['reversion_distance'] * merged['rolling_std'] * merged['deployed_usdt']
    
    # Realized capture = net_pnl + fees (gross pnl).
    # Wait, the prompt says: "realized_capture = gross P&L aktual (pnl_a + pnl_b, sebelum fee) dari trade tersebut."
    # Let's extract fees from notes if possible, or calculate it.
    # Note format: "Closed statistical arbitrage position. Reason: MEAN_REVERSION, Net PnL: $-1.45 (Leg A: $0.67, Leg B: $-0.52, Fees: $1.60)"
    def extract_gross(row):
        notes = row['notes']
        if not isinstance(notes, str): return np.nan
        import re
        m = re.search(r'Fees:\s*\$([0-9.]+)', notes)
        if m:
            fees = float(m.group(1))
            return row['net_pnl'] + fees
        return np.nan
        
    merged['realized_capture'] = merged.apply(extract_gross, axis=1)
    
    merged = merged.dropna(subset=['predicted_capture', 'realized_capture'])
    # Filter where predicted is positive
    merged = merged[merged['predicted_capture'] > 0]
    
    merged['ratio'] = merged['realized_capture'] / merged['predicted_capture']
    
    print("\n--- Distribution of realized_capture / predicted_capture ---")
    print(merged['ratio'].describe(percentiles=[0.1, 0.25, 0.5, 0.75, 0.9]))
    print(f"Trades where realized < predicted: {(merged['ratio'] < 1.0).mean()*100:.2f}%")
    
    # 4. Sensitivity Analysis (Step 4)
    # Re-evaluate the 10,000 stats rows with different combinations of fee_rate and buffer_multiplier
    print("\n--- Sensitivity Analysis ---")
    stats_10k = pd.read_sql("SELECT z_score, rolling_std, rolling_mean, r2 FROM starb_pair_stats ORDER BY id DESC LIMIT 10000", conn)
    
    fee_options = [0.0016, 0.0006, 0.0004] # Normal Taker, VIP/BNB Taker, Futures Taker
    buffer_options = [0.5, 1.0, 1.5, 2.0, 2.5]
    
    for f in fee_options:
        for b in buffer_options:
            stats_10k['expected_reversion'] = stats_10k['z_score'].abs() - z_exit_threshold
            stats_10k['implied_move'] = stats_10k['expected_reversion'] * stats_10k['rolling_std']
            stats_10k['fee_cost_ratio'] = f * b
            passed = ((stats_10k['expected_reversion'] > 0) & (stats_10k['r2'] >= 0.85) & (stats_10k['implied_move'] > stats_10k['fee_cost_ratio'])).sum()
            print(f"Fee {f:.4f} | Buffer {b:.1f} | Passed: {passed} ({(passed/10000)*100:.2f}%)")

if __name__ == "__main__":
    main()
