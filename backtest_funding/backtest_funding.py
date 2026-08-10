import sys
import os
import psycopg2
from datetime import datetime, timezone
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

def main():
    starting_balance   = float(sys.argv[1]) if len(sys.argv) > 1 else 200.0
    min_funding_rate   = float(sys.argv[2]) if len(sys.argv) > 2 else 0.0005
    max_positions      = int(sys.argv[3])   if len(sys.argv) > 3 else 3
    base_position_size = float(sys.argv[4]) if len(sys.argv) > 4 else 60.0
    leverage           = float(sys.argv[5]) if len(sys.argv) > 5 else 10.0

    SPOT_FEE_PCT        = 0.001
    FUTURES_FEE_PCT     = 0.0004
    LIQUIDATION_FEE_PCT = 0.005
    BUFFER_RATIO        = 0.95
    MAINTENANCE_MARGIN  = 0.004

    print("====================================================================")
    print("FUNDING RATE ARBITRAGE BACKTEST - MODE REALISTIS")
    print("====================================================================")
    print(f"Modal Awal       : ${starting_balance:,.2f}")
    print(f"Min Funding Rate : {min_funding_rate * 100.0:.4f}% per 8 jam")
    print(f"Max Posisi       : {max_positions} koin")
    print(f"Ukuran Awal/Koin : ${base_position_size:,.2f} (min floor)")
    print(f"Leverage Futures : {leverage}x")
    print(f"Margin per Koin  : ${base_position_size / leverage:.2f} ({100/leverage:.0f}%)")
    print(f"Likuidasi pada   : harga naik >{(1/leverage - MAINTENANCE_MARGIN)*100:.1f}% dari entry")
    print("--------------------------------------------------------------------")
    print("Fitur Aktif: Margin Call, Likuidasi Paksa, Dynamic Compounding,")
    print("             Rotasi Oportunistik (FR 2x), Fee lengkap, Buffer 5%")
    print("====================================================================")

    print("\n[Backtest] Mengambil data historis dari database...")
    try:
        db_params = parse_db_url(DATABASE_URL)
        conn = psycopg2.connect(**db_params)
        cursor = conn.cursor()
        cursor.execute(
            "SELECT symbol, funding_time, funding_rate, mark_price, index_price "
            "FROM fr_history ORDER BY funding_time ASC"
        )
        rows = cursor.fetchall()
        cursor.close()
        conn.close()
    except Exception as e:
        print(f"[ERROR] Koneksi database gagal: {e}")
        sys.exit(1)

    total_records = len(rows)
    print(f"[Backtest] {total_records} record funding rate dimuat.")
    if total_records == 0:
        print("[WARN] Tidak ada data di tabel fr_history. Keluar.")
        sys.exit(0)

    grouped_data = {}
    for row in rows:
        symbol, f_time, f_rate, mark_p, index_p = row
        f_time_utc = f_time.astimezone(timezone.utc) if (hasattr(f_time, "tzinfo") and f_time.tzinfo) else f_time
        if f_time_utc not in grouped_data:
            grouped_data[f_time_utc] = []
        grouped_data[f_time_utc].append({
            "symbol": symbol, "funding_rate": f_rate,
            "mark_price": mark_p, "index_price": index_p
        })

    sorted_timestamps = sorted(grouped_data.keys())
    print(f"[Backtest] {len(sorted_timestamps)} periode settlement.")
    print(f"[Backtest] {sorted_timestamps[0]} s/d {sorted_timestamps[-1]}\n")

    simulated_balance      = starting_balance
    active_positions       = {}
    trade_history          = []
    liquidation_events     = []
    peak_equity            = starting_balance
    max_drawdown           = 0.0
    total_liquidations     = 0
    total_liquidation_cost = 0.0

    print("[Backtest] Menjalankan simulasi...\n")

    for t in sorted_timestamps:
        period_data  = grouped_data[t]
        period_coins = {c["symbol"]: c for c in period_data}
        symbols_to_close = []

        for symbol, pos in list(active_positions.items()):
            current_pos_size = pos["position_size"]
            futures_margin   = current_pos_size / leverage

            if symbol not in period_coins:
                continue

            coin_data   = period_coins[symbol]
            fr          = coin_data["funding_rate"]
            mark_price  = coin_data["mark_price"]
            index_price = coin_data["index_price"]

            pos["current_mark_price"] = mark_price
            pos["current_spot_price"] = index_price

            # Cek Margin Call / Likuidasi Paksa
            futures_loss = (mark_price - pos["futures_entry_price"]) / pos["futures_entry_price"] * current_pos_size
            futures_loss = max(0.0, futures_loss)
            maint_margin = current_pos_size * MAINTENANCE_MARGIN
            sisa_margin  = futures_margin - futures_loss

            if sisa_margin <= maint_margin:
                spot_gain        = (index_price - pos["spot_entry_price"]) / pos["spot_entry_price"] * current_pos_size
                liq_fee          = current_pos_size * LIQUIDATION_FEE_PCT
                # Kita kehilangan futures_margin (jadi 0), tapi spot tetap bisa dijual penuh (current_pos_size + spot_gain)
                capital_returned = current_pos_size + spot_gain - liq_fee + pos["total_funding"]
                simulated_balance      += capital_returned
                total_liquidations     += 1
                total_liquidation_cost += liq_fee
                spike_pct = (mark_price - pos["futures_entry_price"]) / pos["futures_entry_price"] * 100
                liquidation_events.append({
                    "symbol": symbol, "time": t,
                    "price_spike_pct": spike_pct, "liq_fee": liq_fee,
                    "funding_collected": pos["total_funding"],
                    "net": capital_returned - (current_pos_size * 1.1)
                })
                symbols_to_close.append(symbol)
                continue

            # Kumpulkan Funding Payment
            payment = current_pos_size * fr
            pos["total_funding"]  += payment
            pos["payment_count"]  += 1
            simulated_balance     += payment

            if fr < 0.0:
                pos["consecutive_neg"] += 1
            else:
                pos["consecutive_neg"]  = 0

            # Cek Exit Normal
            exit_reason = None
            if pos["consecutive_neg"] >= 2:
                exit_reason = f"[FR Negatif] FR: {fr*100:.4f}%"
            elif fr < -0.0005:
                exit_reason = f"[Emergency] FR: {fr*100:.4f}%"

            # Rotasi Oportunistik (hanya setelah 3 periode / 24 jam)
            if exit_reason is None and pos["payment_count"] >= 3:
                best_fr = max(
                    (c["funding_rate"] for c in period_data
                     if c["symbol"] not in active_positions
                     and c["funding_rate"] > min_funding_rate
                     and c["mark_price"] > 0 and c["index_price"] > 0
                     and abs((c["mark_price"] - c["index_price"]) / c["index_price"] * 100) < 0.5),
                    default=0.0
                )
                cur_fr = max(fr, 0.0)
                if best_fr > cur_fr * 2.0 and best_fr > 0:
                    exit_reason = f"[Rotasi] {cur_fr*100:.4f}% -> kandidat {best_fr*100:.4f}%"

            if exit_reason:
                spot_pnl    = (index_price - pos["spot_entry_price"]) / pos["spot_entry_price"] * current_pos_size
                futures_pnl = (pos["futures_entry_price"] - mark_price) / pos["futures_entry_price"] * current_pos_size
                exit_fee    = current_pos_size * (SPOT_FEE_PCT + FUTURES_FEE_PCT)
                net_pnl     = pos["total_funding"] + spot_pnl + futures_pnl - exit_fee
                simulated_balance += (current_pos_size * 1.1) + net_pnl
                trade_history.append({
                    "symbol": symbol, "opened_at": pos["opened_at"], "closed_at": t,
                    "position_size": current_pos_size, "payments_count": pos["payment_count"],
                    "funding": pos["total_funding"], "spot_pnl": spot_pnl,
                    "futures_pnl": futures_pnl, "net_pnl": net_pnl, "reason": exit_reason
                })
                symbols_to_close.append(symbol)

        for symbol in symbols_to_close:
            active_positions.pop(symbol, None)

        # Dynamic Compounding Size
        deployed_cap     = sum(p["position_size"] * 1.1 for p in active_positions.values())
        total_equity_now = simulated_balance + deployed_cap
        dynamic_size     = (total_equity_now * BUFFER_RATIO) / (max_positions * 1.1)
        position_size    = max(dynamic_size, base_position_size)
        open_cost        = (position_size * 1.1) + position_size * (SPOT_FEE_PCT + FUTURES_FEE_PCT)

        # Buka Posisi Baru
        candidates = [
            c for c in period_data
            if c["symbol"] not in active_positions
            and c["funding_rate"] >= min_funding_rate
            and c["mark_price"] > 0 and c["index_price"] > 0
            and abs((c["mark_price"] - c["index_price"]) / c["index_price"] * 100) <= 0.5
        ]
        candidates.sort(key=lambda x: x["funding_rate"], reverse=True)

        available_slots = max_positions - len(active_positions)
        for cand in candidates[:available_slots]:
            if simulated_balance < open_cost:
                break
            simulated_balance -= open_cost
            active_positions[cand["symbol"]] = {
                "opened_at": t, "position_size": position_size,
                "spot_entry_price": cand["index_price"],
                "futures_entry_price": cand["mark_price"],
                "current_spot_price": cand["index_price"],
                "current_mark_price": cand["mark_price"],
                "total_funding": 0.0, "payment_count": 0,
                "consecutive_neg": 0, "initial_fr": cand["funding_rate"]
            }

        # Hitung Ekuitas & Drawdown
        unrealized   = sum(
            (p["current_spot_price"] - p["spot_entry_price"]) / p["spot_entry_price"] * p["position_size"]
            + (p["futures_entry_price"] - p["current_mark_price"]) / p["futures_entry_price"] * p["position_size"]
            for p in active_positions.values()
        )
        deployed_now   = sum(p["position_size"] * 1.1 for p in active_positions.values())
        current_equity = simulated_balance + deployed_now + unrealized
        peak_equity    = max(peak_equity, current_equity)
        dd             = (peak_equity - current_equity) / peak_equity * 100.0 if peak_equity > 0 else 0.0
        max_drawdown   = max(max_drawdown, dd)

    # Hasil Akhir
    total_trades   = len(trade_history)
    winning_trades = sum(1 for tr in trade_history if tr["net_pnl"] >= 0.0)
    win_rate       = (winning_trades / total_trades * 100.0) if total_trades > 0 else 0.0
    total_funding  = sum(tr["funding"] for tr in trade_history)
    total_funding += sum(p["total_funding"] for p in active_positions.values())
    total_net_pnl  = current_equity - starting_balance
    roi_pct        = (total_net_pnl / starting_balance) * 100.0
    n_days         = len(sorted_timestamps) / 3.0
    daily_avg      = total_net_pnl / n_days if n_days > 0 else 0
    monthly_avg    = daily_avg * 30

    print("====================================================================")
    print("HASIL BACKTEST REALISTIS")
    print("====================================================================")
    print(f"Durasi           : {len(sorted_timestamps)} periode ({n_days:.0f} hari)")
    print(f"Modal Awal       : ${starting_balance:,.2f}")
    print(f"Modal Akhir      : ${current_equity:,.2f}")
    print(f"Net Profit/Loss  : ${total_net_pnl:+,.2f} ({roi_pct:+.2f}%)")
    print(f"Avg Profit/hari  : ${daily_avg:+.4f}")
    print(f"Avg Profit/bulan : ${monthly_avg:+.4f}")
    print(f"Max Drawdown     : {max_drawdown:.2f}%")
    print(f"Total Funding FR : ${total_funding:,.4f}")
    print("--------------------------------------------------------------------")
    print(f"Total Trades     : {total_trades}")
    print(f"Win Rate         : {win_rate:.1f}% ({winning_trades}/{total_trades})")
    print(f"Posisi Masih Buka: {len(active_positions)}")
    print(f"Likuidasi Paksa  : {total_liquidations}x | Biaya: ${total_liquidation_cost:.4f}")
    if liquidation_events:
        print("\nDetail Likuidasi (maks 3):")
        for liq in liquidation_events[:3]:
            print(f"  {liq['symbol']} spike +{liq['price_spike_pct']:.1f}% | "
                  f"Fee: ${liq['liq_fee']:.4f} | FR sebelumnya: ${liq['funding_collected']:.4f} | "
                  f"Net: ${liq['net']:+.4f}")
    if total_trades > 0:
        print("\n5 Trade Terakhir:")
        for i, tr in enumerate(trade_history[-5:]):
            print(f"  {tr['symbol']:<20} Size:${tr['position_size']:.0f} "
                  f"{tr['payments_count']}x FR:${tr['funding']:+.4f} "
                  f"Net:${tr['net_pnl']:+.4f} | {tr['reason']}")
    print("====================================================================")
    print("CATATAN: Hasil sudah termasuk margin call, likuidasi, compounding,")
    print("         rotasi oportunistik, dan semua biaya fee.")
    print("====================================================================")

if __name__ == "__main__":
    main()
