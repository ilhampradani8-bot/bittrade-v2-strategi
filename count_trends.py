with open("backtest/run_backtest.py", "r") as f:
    code = f.read()

# Modify the code to print the counts at the end instead of saving JSON
new_code = code.replace(
    'results = {', 
    '''
    up_count = sum(1 for log in trade_logs if "[Trending]" in log and "BUY" in log)
    side_count = sum(1 for log in trade_logs if "[Sideways]" in log and "BUY" in log)
    break_count = sum(1 for log in trade_logs if "[Breakout]" in log and "BUY" in log)
    print(f"Uptrend Trades: {up_count}")
    print(f"Sideways Trades: {side_count}")
    print(f"Breakout Trades: {break_count}")
    sys.exit(0)
    results = {
    '''
)
with open("backtest/run_backtest_count.py", "w") as f:
    f.write(new_code)
