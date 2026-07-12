import psycopg2
import math

def simulate_ev_gate():
    conn = psycopg2.connect("postgresql://bottrade_user:%40Dani22334455D@localhost:5432/bottrade_db")
    cursor = conn.cursor()
    
    # Check if table exists
    cursor.execute("SELECT to_regclass('public.starb_pair_stats')")
    if not cursor.fetchone()[0]:
        print("Table starb_pair_stats not found!")
        return

    cursor.execute('''
        SELECT z_score, rolling_std, rolling_mean, r2
        FROM starb_pair_stats
        ORDER BY id DESC LIMIT 10000
    ''')
    rows = cursor.fetchall()
    
    if not rows:
        print("No historical data found in starb_pair_stats.")
        return
        
    passed = 0
    blocked_r2 = 0
    blocked_ev = 0
    blocked_no_reversion = 0
    
    z_exit_threshold = 0.2
    fee_rate = 0.0016
    buffer_multiplier = 1.0
    min_r2 = 0.85
    
    for z_score, rolling_std, rolling_mean, r2 in rows:
        if r2 is None or r2 < min_r2:
            blocked_r2 += 1
            continue
            
        expected_reversion = abs(z_score) - z_exit_threshold
        if expected_reversion > 0.0:
            implied_log_move = expected_reversion * rolling_std
            # Size doesn't matter for the inequality since it's on both sides, 
            # we just compare implied_log_move with (fee_rate * buffer_multiplier)
            expected_profit_ratio = implied_log_move
            fee_cost_ratio = fee_rate * buffer_multiplier
            
            if expected_profit_ratio < fee_cost_ratio:
                blocked_ev += 1
            else:
                passed += 1
        else:
            blocked_no_reversion += 1
            
    total = len(rows)
    print(f"Total Rows Evaluated: {total}")
    print(f"Passed EV Gate: {passed} ({(passed/total)*100:.2f}%)")
    print(f"Blocked by Low R2: {blocked_r2} ({(blocked_r2/total)*100:.2f}%)")
    print(f"Blocked by No Expected Reversion (Z too low): {blocked_no_reversion} ({(blocked_no_reversion/total)*100:.2f}%)")
    print(f"Blocked by EV (Profit < Fee Cost): {blocked_ev} ({(blocked_ev/total)*100:.2f}%)")

if __name__ == "__main__":
    simulate_ev_gate()
