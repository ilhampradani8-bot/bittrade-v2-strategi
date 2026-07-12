import psycopg2
import pandas as pd
import numpy as np
import re

def main():
    conn = psycopg2.connect("postgresql://bottrade_user:%40Dani22334455D@localhost:5432/bottrade_db")
    
    # Do the join in SQL using a LATERAL JOIN to find the latest stats before trade open
    # We will limit to recent trades to be fast
    query = """
    WITH opens AS (
        SELECT id, pair_name, action, z_score, ratio, price_a, price_b, amount_a, amount_b, timestamp, notes
        FROM starb_trading_history
        WHERE action LIKE 'OPEN_%'
        ORDER BY id DESC LIMIT 500
    ),
    closes AS (
        SELECT id, pair_name, action, z_score, ratio, price_a, price_b, amount_a, amount_b, net_pnl, timestamp, notes
        FROM starb_trading_history
        WHERE action LIKE 'CLOSE_%'
    ),
    trades AS (
        SELECT 
            o.id as open_id, o.timestamp as open_time, o.z_score as open_z, o.amount_a, o.price_a, o.amount_b, o.price_b, 
            c.net_pnl, c.timestamp as close_time, c.notes
        FROM opens o
        JOIN closes c ON c.id > o.id AND c.pair_name = o.pair_name
        WHERE NOT EXISTS (
            SELECT 1 FROM opens o2 WHERE o2.id > o.id AND o2.id < c.id AND o2.pair_name = o.pair_name
        )
    )
    SELECT t.*, s.rolling_std
    FROM trades t
    LEFT JOIN LATERAL (
        SELECT rolling_std 
        FROM starb_pair_stats s 
        WHERE s.timestamp <= t.open_time 
        ORDER BY s.timestamp DESC 
        LIMIT 1
    ) s ON true;
    """
    trades = pd.read_sql(query, conn)
    print(f"Loaded {len(trades)} completed trades from history with stats.")
    
    if len(trades) > 0:
        trades['deployed_usdt'] = trades['amount_a'] * trades['price_a'] + trades['amount_b'] * trades['price_b']
        z_exit_threshold = 0.2
        trades['reversion_distance'] = trades['open_z'].abs() - z_exit_threshold
        trades['predicted_capture'] = trades['reversion_distance'] * trades['rolling_std'] * trades['deployed_usdt']
        
        def extract_gross(row):
            notes = row['notes']
            if not isinstance(notes, str): return np.nan
            m = re.search(r'Fees:\s*\$([0-9.]+)', notes)
            if m:
                fees = float(m.group(1))
                return row['net_pnl'] + fees
            return np.nan
            
        trades['realized_capture'] = trades.apply(extract_gross, axis=1)
        merged = trades.dropna(subset=['predicted_capture', 'realized_capture']).copy()
        merged = merged[merged['predicted_capture'] > 0]
        merged['ratio'] = merged['realized_capture'] / merged['predicted_capture']
        
        print("\n--- Distribution of realized_capture / predicted_capture ---")
        if len(merged) > 0:
            print(merged['ratio'].describe(percentiles=[0.1, 0.25, 0.5, 0.75, 0.9]))
            print(f"Trades where realized < predicted: {(merged['ratio'] < 1.0).mean()*100:.2f}%")
        else:
            print("No valid ratios found.")
    
    # Forward Validation (Step 5) - subset 2000
    print("\n--- Forward Validation & Sensitivity Analysis ---")
    stats = pd.read_sql("SELECT id, timestamp, z_score, rolling_std, rolling_mean, r2, price_a, price_b FROM starb_pair_stats ORDER BY id DESC LIMIT 2000", conn)
    stats = stats.sort_values('id').reset_index(drop=True)
    z_exit_threshold = 0.2
    
    fee_options = [0.0030, 0.0016, 0.0006] # Spot Taker, Futures Taker, VIP Futures Taker
    buffer_options = [0.2, 0.5, 0.8, 1.0, 1.5]
    
    for f in fee_options:
        for b in buffer_options:
            stats['expected_reversion'] = stats['z_score'].abs() - z_exit_threshold
            stats['implied_move'] = stats['expected_reversion'] * stats['rolling_std']
            stats['fee_cost_ratio'] = f * b
            
            signals = stats[(stats['expected_reversion'] > 0) & 
                            (stats['r2'] >= 0.85) & 
                            (stats['implied_move'] > stats['fee_cost_ratio'])]
            
            passed = len(signals)
            if passed == 0:
                print(f"Fee {f:.4f} | Buffer {b:.1f} | Passed: 0 (0.00%)")
                continue
                
            net_pnls = []
            for idx, row in signals.iterrows():
                entry_z = row['z_score']
                direction = np.sign(entry_z)
                future = stats.iloc[idx+1:]
                reverted = future[(future['z_score'] * direction) <= z_exit_threshold]
                
                if len(reverted) > 0:
                    exit_idx = reverted.index[0]
                    exit_row = reverted.iloc[0]
                    gross_pnl_ratio = (abs(entry_z) - abs(exit_row['z_score'])) * row['rolling_std']
                    net = gross_pnl_ratio - f
                    net_pnls.append(net)
            
            if len(net_pnls) > 0:
                win_rate = np.mean(np.array(net_pnls) > 0) * 100
                avg_net = np.mean(net_pnls) * 100 # in percent
                print(f"Fee {f:.4f} | Buffer {b:.1f} | Passed: {passed} | Validated: {len(net_pnls)} | WinRate: {win_rate:.1f}% | Avg Net PnL: {avg_net:.4f}%")
            else:
                print(f"Fee {f:.4f} | Buffer {b:.1f} | Passed: {passed} | Validated: 0")
                
if __name__ == "__main__":
    main()
