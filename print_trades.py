import json
with open("backtest/backtest_results.json") as f:
    d = json.load(f)
for c in d["cycles"][-15:]:
    pnl = "+" if c["net_pnl"] > 0 else ""
    print(f"- **Cycle #{c['cycle_id']}** | {c['start_time']} | Exit: {c['exit_reason']} | P&L: ${c['net_pnl']:.2f} ({pnl}{c['pnl_pct']:.2f}%)")
