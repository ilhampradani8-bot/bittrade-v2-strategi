import json
import os
from datetime import datetime

def load_data():
    trades_path = '/root/bittrade-v2-strategi/trading_journal/trades_all.json'
    balances_path = '/root/bittrade-v2-strategi/trading_journal/balances_all.json'
    
    with open(trades_path, 'r') as f:
        trades = json.load(f)
        
    with open(balances_path, 'r') as f:
        balances = json.load(f)
        
    return trades, balances

def parse_time(t_str):
    # E.g. "2026-06-29 17:01:46.462046" or "2026-06-29 17:01:46" or with T
    t_str = t_str.replace('T', ' ')
    if '.' in t_str:
        t_str = t_str.split('.')[0]
    return datetime.strptime(t_str, "%Y-%m-%d %H:%M:%S")

def analyze():
    trades, balances = load_data()
    
    if not trades:
        print("No trades found.")
        return
        
    # Filter trades that are successful
    trades = [t for t in trades if t['status'] == 'SUCCESS']
    
    eval_start_time = datetime(2026, 6, 29, 17, 0, 0)
    
    # Find initial balance closest to 2026-06-29 17:00:00 WIB
    initial_balance_entry = None
    min_diff = float('inf')
    for b in balances:
        b_time = parse_time(b['wib_time'])
        diff = abs((b_time - eval_start_time).total_seconds())
        if diff < min_diff:
            min_diff = diff
            initial_balance_entry = b
            
    # If no close balance entry found, default to first balance entry
    if not initial_balance_entry and balances:
        initial_balance_entry = balances[0]
        
    initial_equity = initial_balance_entry['total_value'] if initial_balance_entry else 1000.0
    initial_usdt = initial_balance_entry['simulated_balance'] if initial_balance_entry else 1000.0
    initial_btc = initial_balance_entry['btc_balance'] if initial_balance_entry else 0.0
    
    # We will reconstruct ALL trading cycles from the beginning of time
    cycles = []
    current_cycle = {
        'buys': [],
        'sell': None
    }
    
    btc_position = 0.0
    
    for trade in trades:
        action = trade['action']
        price = float(trade['price'])
        amount = float(trade['amount'])
        notes = trade['notes'] or ''
        wib_time = trade['wib_time']
        
        if action == 'BUY':
            if btc_position == 0.0:
                # Start of a new cycle
                if current_cycle['buys']: # Close previous anomaly
                    cycles.append(current_cycle)
                current_cycle = {
                    'buys': [],
                    'sell': None
                }
            current_cycle['buys'].append({
                'price': price,
                'amount': amount,
                'notes': notes,
                'wib_time': wib_time
            })
            btc_position += amount
        elif action == 'SELL':
            current_cycle['sell'] = {
                'price': price,
                'amount': amount,
                'notes': notes,
                'wib_time': wib_time
            }
            btc_position = max(0.0, btc_position - amount)
            cycles.append(current_cycle)
            current_cycle = {
                'buys': [],
                'sell': None
            }
            
    if current_cycle['buys'] and not current_cycle['sell']:
        cycles.append(current_cycle)
        
    # Now, filter the cycles to only keep the ones that ended on or after eval_start_time,
    # or are currently open and started before/after.
    filtered_cycles = []
    for cycle in cycles:
        buys = cycle['buys']
        sell = cycle['sell']
        
        if not buys:
            continue
            
        is_relevant = False
        if sell:
            sell_time = parse_time(sell['wib_time'])
            if sell_time >= eval_start_time:
                is_relevant = True
        else:
            # Open position
            is_relevant = True
            
        if is_relevant:
            filtered_cycles.append(cycle)
            
    # Analyze each relevant cycle
    cycle_reports = []
    total_net_pnl = 0.0
    wins = 0
    losses = 0
    total_win_pnl = 0.0
    total_loss_pnl = 0.0
    
    sell_reasons_stats = {}
    
    for idx, cycle in enumerate(filtered_cycles):
        buys = cycle['buys']
        sell = cycle['sell']
        
        # Calculate stats for buys (this now includes ALL buys for this cycle, even if some happened before 17:00!)
        total_btc = sum(b['amount'] for b in buys)
        total_spent = sum(b['price'] * b['amount'] for b in buys)
        total_spent_with_fee = total_spent * 1.001
        
        avg_entry_price = total_spent / total_btc if total_btc > 0 else 0.0
        
        start_time = buys[0]['wib_time']
        
        if sell:
            sell_price = sell['price']
            sell_amount = sell['amount']
            sell_revenue = sell_price * sell_amount
            sell_revenue_net = sell_revenue * 0.999
            
            # True net P&L
            net_pnl = sell_revenue_net - total_spent_with_fee
            net_pnl_pct = (net_pnl / total_spent_with_fee) * 100.0 if total_spent_with_fee > 0 else 0.0
            
            end_time = sell['wib_time']
            status = 'WIN' if net_pnl > 0 else 'LOSS'
            
            if net_pnl > 0:
                wins += 1
                total_win_pnl += net_pnl
            else:
                losses += 1
                total_loss_pnl += net_pnl
                
            total_net_pnl += net_pnl
            
            # Extract sell reason
            sell_notes = sell['notes'] or ''
            reason_part = sell_notes.split('|')[0].strip()
            
            sell_reasons_stats[reason_part] = sell_reasons_stats.get(reason_part, 0) + 1
            
            # Check for P&L logging mismatch
            bot_logged_pnl = None
            if 'P&L:' in sell_notes:
                try:
                    pnl_part = sell_notes.split('P&L:')[1].strip()
                    pnl_val_str = pnl_part.replace('$', '').strip()
                    bot_logged_pnl = float(pnl_val_str)
                except Exception:
                    pass
            
            cycle_reports.append({
                'cycle_num': len(cycle_reports) + 1,
                'start_time': start_time,
                'end_time': end_time,
                'num_buys': len(buys),
                'total_btc': total_btc,
                'avg_entry_price': avg_entry_price,
                'exit_price': sell_price,
                'total_spent': total_spent_with_fee,
                'total_received': sell_revenue_net,
                'net_pnl': net_pnl,
                'net_pnl_pct': net_pnl_pct,
                'status': status,
                'reason': reason_part,
                'bot_logged_pnl': bot_logged_pnl,
                'is_open': False
            })
        else:
            cycle_reports.append({
                'cycle_num': len(cycle_reports) + 1,
                'start_time': start_time,
                'end_time': 'OPEN',
                'num_buys': len(buys),
                'total_btc': total_btc,
                'avg_entry_price': avg_entry_price,
                'exit_price': None,
                'total_spent': total_spent_with_fee,
                'total_received': None,
                'net_pnl': None,
                'net_pnl_pct': None,
                'status': 'OPEN',
                'reason': 'N/A',
                'bot_logged_pnl': None,
                'is_open': True
            })

    # Overall stats
    total_cycles = wins + losses
    win_rate = (wins / total_cycles * 100) if total_cycles > 0 else 0
    avg_win = (total_win_pnl / wins) if wins > 0 else 0
    avg_loss = (total_loss_pnl / losses) if losses > 0 else 0
    profit_factor = (total_win_pnl / abs(total_loss_pnl)) if total_loss_pnl != 0 else float('inf')
    
    # Calculate Max Drawdown from balance history starting from eval_start_time
    filtered_balances = []
    for b in balances:
        b_time = parse_time(b['wib_time'])
        if b_time >= eval_start_time:
            filtered_balances.append(b)
            
    equity_curve = [b['total_value'] for b in filtered_balances]
    max_dd = 0.0
    if equity_curve:
        peak = -999999.0
        for eq in equity_curve:
            if eq > peak:
                peak = eq
            dd = (peak - eq) / peak * 100.0
            if dd > max_dd:
                max_dd = dd
            
    current_equity = balances[-1]['total_value'] if balances else 0.0
    net_equity_change = current_equity - initial_equity
    pct_equity_change = (net_equity_change / initial_equity * 100.0) if initial_equity > 0 else 0.0

    # Write Markdown Report
    report_path = '/root/bittrade-v2-strategi/trading_journal/LAPORAN_EVALUASI_FASE4.md'
    with open(report_path, 'w') as f:
        f.write("# 📈 LAPORAN EVALUASI BOT TRADING FASE 4 (Quant-Grade)\n\n")
        f.write(f"**Periode Evaluasi:** Sejak lusa kemarin 2026-06-29 17:00 WIB s/d {datetime.now().strftime('%Y-%m-%d %H:%M WIB')}\n")
        f.write(f"**Durasi Operasional:** Sekitar {((datetime.now() - eval_start_time).total_seconds() / 3600):.1f} Jam\n\n")
        
        f.write("## 📊 Rangkuman Kinerja Portofolio\n\n")
        f.write("| Parameter | Nilai Awal (29 Juni, 17:00) | Nilai Sekarang | Perubahan Bersih | Perubahan (%) |\n")
        f.write("| --- | --- | --- | --- | --- |\n")
        f.write(f"| **Total Equity** | ${initial_equity:,.2f} | ${current_equity:,.2f} | ${net_equity_change:+,.2f} | {pct_equity_change:+.2f}% |\n")
        f.write(f"| **Saldo USDT** | ${initial_usdt:,.2f} | ${balances[-1]['simulated_balance']:,.2f} | ${(balances[-1]['simulated_balance'] - initial_usdt):+,.2f} | - |\n")
        f.write(f"| **Saldo BTC** | {initial_btc:.6f} BTC | {balances[-1]['btc_balance']:.6f} BTC | {balances[-1]['btc_balance'] - initial_btc:+.6f} BTC | - |\n\n")
        
        f.write("## 🎯 Statistik Trading Per-Siklus (Cycle-based Metrics)\n")
        f.write("> **Catatan Konsep:** Bot beroperasi dengan melakukan akumulasi bertahap (pyramiding/scaling-in) di fase trending (maksimal 20% budget per 5 menit) dan melakukan close-all saat sinyal sell terdeteksi. Sehingga 1 siklus trading = akumulasi beberapa BUY -> 1 SELL.\n\n")
        
        f.write(f"- **Total Siklus Selesai:** {total_cycles}\n")
        f.write(f"- **Siklus Profit (Win):** {wins} ({win_rate:.1f}%)\n")
        f.write(f"- **Siklus Rugi (Loss):** {losses} ({100 - win_rate:.1f}%)\n")
        f.write(f"- **Profit Factor:** {profit_factor:.2f}\n")
        f.write(f"- **Rata-rata Profit Per Win:** ${avg_win:.2f}\n")
        f.write(f"- **Rata-rata Rugi Per Loss:** ${avg_loss:.2f}\n")
        f.write(f"- **Rasio Risk/Reward Riil:** 1 : {abs(avg_win / avg_loss) if avg_loss != 0 else 0:.2f}\n")
        f.write(f"- **Max Drawdown Portofolio:** {max_dd:.2f}%\n\n")
        
        f.write("### 📌 Statistik Alasan Penutupan Posisi (Sell Reasons)\n")
        for reason, count in sell_reasons_stats.items():
            f.write(f"- **{reason}:** {count} kali\n")
        f.write("\n")
        
        f.write("## ⚠️ TEMUAN LOGIKA & BUG SISTEM (Penting untuk Developer)\n\n")
        f.write("> [!WARNING]\n")
        f.write("> **Bug Kalkulasi P&L pada Log Database (`notes`):**\n")
        f.write("> Ditemukan bahwa kode di `rust_bot/src/executor.rs` salah menghitung realized P&L untuk dicatat di kolom `notes` database. \n")
        f.write("> Rumus yang digunakan bot saat ini:\n")
        f.write("> `let gross_pnl = (price - buy_price) * amount;` \n")
        f.write("> di mana `buy_price` diambil dari order BUY terakhir (`LIMIT 1`), sedangkan `amount` adalah akumulasi seluruh BTC di dompet. Hal ini menghasilkan nilai P&L tercatat di log database yang **sangat salah** (menyamakan seluruh modal beli dengan harga beli order terakhir). \n")
        f.write("> *P&L sebenarnya telah kami hitung ulang di bawah secara presisi berdasarkan harga beli rata-rata tertimbang (weighted average entry price).*\n\n")
        
        f.write("## 📝 Rincian Histori Siklus Trading (Siklus 1 - Selesai)\n\n")
        f.write("| Siklus | Waktu Mulai (WIB) | Waktu Selesai (WIB) | Jumlah Buy | Avg Entry | Exit Price | Total Spent | Net P&L ($) | Net P&L (%) | Alasan Keluar | Catatan Log DB (Salah) |\n")
        f.write("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n")
        
        for c in cycle_reports:
            if c['is_open']:
                f.write(f"| #{c['cycle_num']} | {c['start_time']} | *OPEN POSITION* | {c['num_buys']} | ${c['avg_entry_price']:,.2f} | - | ${c['total_spent']:,.2f} | - | - | - | - |\n")
            else:
                pnl_color = "🟢" if c['status'] == 'WIN' else "🔴"
                logged_pnl_str = f"${c['bot_logged_pnl']:+.2f}" if c['bot_logged_pnl'] is not None else "-"
                f.write(f"| #{c['cycle_num']} | {c['start_time']} | {c['end_time']} | {c['num_buys']} | ${c['avg_entry_price']:,.2f} | ${c['exit_price']:,.2f} | ${c['total_spent']:,.2f} | {pnl_color} **{c['net_pnl']:+,.2f}** | **{c['net_pnl_pct']:+.2f}%** | {c['reason']} | {logged_pnl_str} |\n")
                
        f.write("\n## 🧠 Analisis Strategi untuk Claude Max\n\n")
        f.write("Berikut adalah analisis perilaku strategi bittrade-v2 setelah upgrade Fase 4:\n\n")
        f.write("### 1. Perilaku Pyramiding / Scaling-in\n")
        f.write("- Bot melakukan pembelian berulang (pyramiding) setiap 5 menit ketika kondisi trending bullish terpenuhi (`EMA-13 > EMA-34`, harga di atas `VWAP`, dan tren 15 menit bullish).\n")
        f.write("- Hal ini menyebabkan ukuran posisi membesar secara eksponensial di awal tren, namun porsi budget yang digunakan semakin mengecil (karena 20% dari *sisa* simulated balance). Sebagai contoh, jika saldo $1000, bot membeli $200, lalu $160, lalu $128, dst.\n")
        f.write("- **Masalah:** Pembelian berulang ini sering kali menaikkan harga rata-rata entry (*average cost basis*) bot mendekati puncak tren. Ketika tren berbalik arah sedikit saja, bot terpaksa keluar dengan kerugian karena harga jual berada di bawah harga beli rata-rata, meskipun harga saat sell masih lebih tinggi dari order BUY pertama.\n\n")
        
        f.write("### 2. Kinerja Proteksi Modal vs Whipsaw\n")
        f.write("- **Emergency Stop Loss -1.2%**: Sangat disiplin membatasi kerugian besar. Ini mencegah terjadinya kerugian dalam (deep losses), namun sering terpicu jika terjadi koreksi kecil pada timeframe 1 menit.\n")
        f.write("- **Trailing Take Profit**: Cukup baik mengunci keuntungan saat terjadi lonjakan harga cepat, tetapi jarang tercapai jika tren naik secara perlahan dan kemudian berbalik arah mendadak.\n")
        f.write("- **Normal Sell Sinyal**: Sinyal jual normal (`EMA-13 < EMA-34` atau tren 15 menit bearish) sering kali terlambat bertindak dibandingkan dengan kecepatan koreksi harga di timeframe 1 menit, sehingga keuntungan yang sempat diperoleh menguap dan berubah menjadi rugi kecil.\n\n")
        
        f.write("### 3. Rekomendasi Perbaikan untuk Didiskusikan dengan Claude Max\n")
        f.write("1. **Batasi Maksimum Pyramiding (Max Pyramiding Layers):** Jangan izinkan bot melakukan BUY lebih dari misalnya 2 atau 3 kali dalam satu siklus trending. Hal ini mencegah naiknya rata-rata entry price ke area rawan koreksi.\n")
        f.write("2. **Perbaiki Rumus P&L Logging:** Perbaiki bug di `executor.rs` agar merekam rata-rata harga beli berbobot (weighted average buy price) dari tabel `bot_active_positions` untuk menghitung realized P&L yang akurat.\n")
        f.write("3. **Gunakan Waktu Jeda Cooldown Antar-Buy yang Lebih Panjang:** Atau naikkan threshold volume surge confirmation untuk order buy tambahan (pyramiding).\n")
        f.write("4. **Trailing Stop Berdasarkan Average Entry Price:** Trailing take profit dan stop loss sebaiknya dihitung dari *weighted average buy price* posisi aktif saat ini, bukan dari *last buy price*.\n")
        
        f.write("\n## 🛠️ Struktur File Sistem (Arsitektur Modul Rust)\n\n")
        f.write("Untuk membantu Claude Max memahami bagaimana kode Rust diatur, berikut adalah rincian fungsionalitas dan fungsi utama dari setiap file di `rust_bot/src`:\n\n")
        f.write("### 1. `main.rs` (Orchestrator Utama)\n")
        f.write("- **Fungsi Utama:** Merupakan entry point (`fn main()`) bot. Mengatur inisialisasi koneksi PostgreSQL pool, migrasi/pembuatan skema tabel database jika belum ada, dan memulihkan state bot saat crash/restart dengan membaca tabel `bot_active_positions` (Crash Recovery).\n")
        f.write("- **Alur Kerja:** \n")
        f.write("  - Memulai WebSocket Price Listener di background via thread tokio spawn untuk mengambil harga real-time.\n")
        f.write("  - Menjalankan Loop Trader Menit Baru (`loop` 60 detik) yang mensinkronkan 5 data kline terbaru dari Binance API, melakukan fallback ke REST API jika WebSocket stale (>30 detik), memanggil modul `conclude::analyze_market` untuk analisa indikator, memvalidasi order via `validate::validate_decision`, mengeksekusi order via `executor::execute_trade`, dan mencatat saldo terbaru ke `bot_balance_history`.\n")
        f.write("  - Menjalankan HTTP Axum web server di port `8087` untuk menyajikan REST API (`/api/status`, `/api/history`, `/api/logs`, dll.) untuk frontend dashboard.\n\n")
        
        f.write("### 2. `conclude.rs` (Otak Analisis Indikator & Keputusan)\n")
        f.write("- **Fungsi Utama:** Melakukan kalkulasi teknikal real-time dan mengeluarkan keputusan trading (`Decision::Buy`, `Decision::Sell`, atau `Decision::Wait`).\n")
        f.write("- **Kalkulasi & Logika:**\n")
        f.write("  - `calculate_ema(prices, period)`: Menghitung Exponential Moving Average (EMA) secara efisien.\n")
        f.write("  - `analyze_market(price, state)`:\n")
        f.write("    - Melakukan pengecekan pengaman darurat (Emergency Stop Loss -1.2%, Trailing Take Profit pullback 1.0% jika profit sempat menyentuh >=1.5%, atau Hard Take Profit +3.0% dari harga beli posisi aktif).\n")
        f.write("    - Membaca 50 candle kline dari DB dan memvalidasi kesenjangan waktu (missing candle check).\n")
        f.write("    - Menghitung EMA-13 dan EMA-34.\n")
        f.write("    - Menghitung True Session VWAP (akumulasi volume sejak 00:00 UTC).\n")
        f.write("    - Menghitung Bollinger Bands 50-period dan Volatilitas.\n")
        f.write("    - Mengklasifikasikan Regime Pasar (SIDEWAYS jika Volatilitas < 0.085%, jika tidak masuk ke TRENDING: BULLISH jika EMA13 > EMA34, BEARISH jika sebaliknya).\n")
        f.write("    - Mengeksekusi strategi regime: Sideways menggunakan Bollinger Bands 50 Mean Reversion (minimal lebar band 1.0%), sedangkan Trending menggunakan Golden/Death Cross EMA-13/34 dikonfirmasi oleh posisi harga di atas/bawah VWAP dan filter HTF tren 15 menit (`prices[0] > prices[15]`).\n\n")
        
        f.write("### 3. `validate.rs` (Modul Validasi Keamanan)\n")
        f.write("- **Fungsi Utama:** Berfungsi sebagai gerbang pertahanan sebelum order dikirim ke executor (`validate_decision`).\n")
        f.write("- **Aturan Validasi:**\n")
        f.write("  - Mengabaikan validasi cooldown jika keputusan berlabel `[Darurat]` (Stop Loss / Trailing TP / Dump Mendadak) agar bot bisa langsung melikuidasi posisi tanpa hambatan.\n")
        f.write("  - **Minimum Holding Time (15 Menit):** Mencegah aksi SELL normal jika posisi baru dibuka kurang dari 15 menit (mengatasi whipsaw noise).\n")
        f.write("  - **Cooldown Transaksi (5 Menit):** Membatasi agar bot tidak melakukan aksi transaksi berturut-turut yang sama (BUY setelah BUY atau SELL setelah SELL) dalam jeda 5 menit.\n")
        f.write("  - **Min Size & Balance Check:** Memastikan nominal transaksi minimal $5.0 dan saldo USDT (untuk BUY) atau saldo BTC (untuk SELL) mencukupi (termasuk perhitungan fee admin 0.1%).\n\n")
        
        f.write("### 4. `executor.rs` (Simulator Eksekutor Transaksi)\n")
        f.write("- **Fungsi Utama:** Mensimulasikan eksekusi order riil di database dan memori (`execute_trade`).\n")
        f.write("- **Alur Kerja:**\n")
        f.write("  - **BUY:** Memotong saldo simulated USDT, menambahkan btc_balance, memperbarui high water mark di memori, mencatat detail pembelian di tabel `bot_active_positions` (untuk tracking trailing profit), dan meng-insert log transaksi sukses ke tabel `bot_trading_history`.\n")
        f.write("  - **SELL:** Menambahkan simulated USDT (dikurangi fee admin 0.1%), mengurangi btc_balance, mereset high water mark ke 0, menghapus posisi aktif di `bot_active_positions`, menghitung realized P&L, dan mencatat transaksi sukses ke `bot_trading_history`.\n")
        f.write("  - **Bug Kalkulasi P&L:** Di dalam blok `SELL`, formula pencatatan realized P&L mengambil harga beli (`buy_price`) dari order BUY terakhir saja (`LIMIT 1`) dari database, bukan rata-rata tertimbang, sehingga memicu kesalahan logging P&L jika bot melakukan pyramiding (membeli berkali-kali).\n\n")
        
        f.write("### 5. `get.rs` (Konektor WebSocket & REST API Binance)\n")
        f.write("- **Fungsi Utama:** Menyediakan jembatan data harga dan kline dari Binance API.\n")
        f.write("- **Metode Utama:**\n")
        f.write("  - `start_price_listener(state)`: Membuka WebSocket stream ke Binance (`wss://stream.binance.com:9443/stream?streams=...`) secara asynchronous untuk memperbarui harga real-time BTC, ETH, BNB, SOL, dan XRP di memori state, serta memperbarui high water mark secara real-time.\n")
        f.write("  - `sync_klines(db, limit)`: Melakukan query HTTP REST API ke endpoint klines Binance (`/api/v3/klines`) untuk mengambil data candlestick 1 menit terakhir dan melakukan `INSERT ON CONFLICT DO UPDATE` ke tabel `btc_klines` PostgreSQL.\n")
        f.write("  - `get_rest_price()`: Mendapatkan harga spot BTCUSDT secara REST API untuk fallback jika WebSocket stale.\n\n")
        
        f.write("### 6. `corrector.rs` (Pencatat Kesalahan Sistem)\n")
        f.write("- **Fungsi Utama:** Menyediakan fungsi `log_error(state, error_type, reason)` untuk mencatat log kegagalan operasional (seperti kegagalan eksekusi saldo, kegagalan koneksi DB, stale websocket, dsb.) ke tabel `bot_corrections` secara asynchronous dan memperbarui indikator LED corrector di dashboard.\n")
        
    print("Report written successfully to /root/bittrade-v2-strategi/trading_journal/LAPORAN_EVALUASI_FASE4.md")

if __name__ == '__main__':
    analyze()
