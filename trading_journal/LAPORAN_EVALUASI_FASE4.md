# 📈 LAPORAN EVALUASI BOT TRADING FASE 4 (Quant-Grade)

**Periode Evaluasi:** Sejak lusa kemarin 2026-06-29 17:00 WIB s/d 2026-07-01 11:11 WIB
**Durasi Operasional:** Sekitar 42.2 Jam

## 📊 Rangkuman Kinerja Portofolio

| Parameter | Nilai Awal (29 Juni, 17:00) | Nilai Sekarang | Perubahan Bersih | Perubahan (%) |
| --- | --- | --- | --- | --- |
| **Total Equity** | $965.34 | $949.35 | $-15.98 | -1.66% |
| **Saldo USDT** | $542.65 | $949.35 | $+406.70 | - |
| **Saldo BTC** | 0.007039 BTC | 0.000000 BTC | -0.007039 BTC | - |

## 🎯 Statistik Trading Per-Siklus (Cycle-based Metrics)
> **Catatan Konsep:** Bot beroperasi dengan melakukan akumulasi bertahap (pyramiding/scaling-in) di fase trending (maksimal 20% budget per 5 menit) dan melakukan close-all saat sinyal sell terdeteksi. Sehingga 1 siklus trading = akumulasi beberapa BUY -> 1 SELL.

- **Total Siklus Selesai:** 13
- **Siklus Profit (Win):** 3 (23.1%)
- **Siklus Rugi (Loss):** 10 (76.9%)
- **Profit Factor:** 0.25
- **Rata-rata Profit Per Win:** $1.74
- **Rata-rata Rugi Per Loss:** $-2.12
- **Rasio Risk/Reward Riil:** 1 : 0.82
- **Max Drawdown Portofolio:** 2.04%

### 📌 Statistik Alasan Penutupan Posisi (Sell Reasons)
- **Jual simulasi. Biaya admin 0.1%: $0.7936. P&L: $-3.37:** 1 kali
- **[Trending] Death Cross SMA:** 1 kali
- **[Darurat] Trailing Take Profit (Puncak Cuaca +2.9%):** 1 kali
- **[Trending] Quant EMA13/34 Sell:** 10 kali

## ⚠️ TEMUAN LOGIKA & BUG SISTEM (Penting untuk Developer)

> [!WARNING]
> **Bug Kalkulasi P&L pada Log Database (`notes`):**
> Ditemukan bahwa kode di `rust_bot/src/executor.rs` salah menghitung realized P&L untuk dicatat di kolom `notes` database. 
> Rumus yang digunakan bot saat ini:
> `let gross_pnl = (price - buy_price) * amount;` 
> di mana `buy_price` diambil dari order BUY terakhir (`LIMIT 1`), sedangkan `amount` adalah akumulasi seluruh BTC di dompet. Hal ini menghasilkan nilai P&L tercatat di log database yang **sangat salah** (menyamakan seluruh modal beli dengan harga beli order terakhir). 
> *P&L sebenarnya telah kami hitung ulang di bawah secara presisi berdasarkan harga beli rata-rata tertimbang (weighted average entry price).*

## 📝 Rincian Histori Siklus Trading (Siklus 1 - Selesai)

| Siklus | Waktu Mulai (WIB) | Waktu Selesai (WIB) | Jumlah Buy | Avg Entry | Exit Price | Total Spent | Net P&L ($) | Net P&L (%) | Alasan Keluar | Catatan Log DB (Salah) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| #1 | 2026-06-29T16:47:43.340218 | 2026-06-29T17:31:52.324712 | 6 | $60,076.39 | $60,117.39 | $793.89 | 🔴 **-1.05** | **-0.13%** | Jual simulasi. Biaya admin 0.1%: $0.7936. P&L: $-3.37 | $-3.37 |
| #2 | 2026-06-29T18:31:59.942217 | 2026-06-29T18:47:03.982679 | 1 | $60,094.00 | $59,872.01 | $241.32 | 🔴 **-1.37** | **-0.57%** | [Trending] Death Cross SMA | $-1.37 |
| #3 | 2026-06-29T18:59:00.522411 | 2026-06-29T19:25:14.441681 | 5 | $60,313.27 | $60,437.99 | $734.81 | 🟢 **+0.05** | **+0.01%** | [Darurat] Trailing Take Profit (Puncak Cuaca +2.9%) | $-4.61 |
| #4 | 2026-06-29T20:12:08.522424 | 2026-06-29T20:37:13.635291 | 3 | $60,213.61 | $59,878.85 | $470.31 | 🔴 **-3.55** | **-0.75%** | [Trending] Quant EMA13/34 Sell | $-3.08 |
| #5 | 2026-06-29T22:38:35.078703 | 2026-06-29T23:15:41.765675 | 5 | $59,866.58 | $59,758.01 | $645.44 | 🔴 **-2.46** | **-0.38%** | [Trending] Quant EMA13/34 Sell | $-1.30 |
| #6 | 2026-06-29T23:41:46.402038 | 2026-06-30T01:07:01.780545 | 15 | $60,023.29 | $60,418.01 | $923.44 | 🟢 **+4.22** | **+0.46%** | [Trending] Quant EMA13/34 Sell | $-2.20 |
| #7 | 2026-06-30T01:15:04.323353 | 2026-06-30T01:30:07.53161 | 1 | $60,520.00 | $60,276.20 | $192.43 | 🔴 **-1.16** | **-0.60%** | [Trending] Quant EMA13/34 Sell | $-1.16 |
| #8 | 2026-06-30T02:25:17.426696 | 2026-06-30T03:58:32.829545 | 9 | $60,416.24 | $60,300.00 | $831.47 | 🔴 **-3.26** | **-0.39%** | [Trending] Quant EMA13/34 Sell | $-2.88 |
| #9 | 2026-06-30T04:15:35.551754 | 2026-06-30T05:11:44.367524 | 5 | $60,463.30 | $60,413.92 | $643.65 | 🔴 **-1.81** | **-0.28%** | [Trending] Quant EMA13/34 Sell | $-1.93 |
| #10 | 2026-06-30T08:19:17.446506 | 2026-06-30T08:50:22.703023 | 4 | $59,988.51 | $59,888.93 | $564.21 | 🔴 **-2.06** | **-0.37%** | [Trending] Quant EMA13/34 Sell | $-1.60 |
| #11 | 2026-06-30T09:27:29.044933 | 2026-06-30T10:25:38.409374 | 6 | $59,997.19 | $59,849.91 | $703.48 | 🔴 **-3.13** | **-0.44%** | [Trending] Quant EMA13/34 Sell | $-2.89 |
| #12 | 2026-07-01T07:53:34.051965 | 2026-07-01T08:08:36.416476 | 1 | $58,588.01 | $58,280.01 | $190.15 | 🔴 **-1.38** | **-0.72%** | [Trending] Quant EMA13/34 Sell | $-1.38 |
| #13 | 2026-07-01T08:31:40.027276 | 2026-07-01T10:21:58.174179 | 17 | $58,728.74 | $58,906.82 | $927.14 | 🟢 **+0.95** | **+0.10%** | [Trending] Quant EMA13/34 Sell | $-3.79 |

## 🧠 Analisis Strategi untuk Claude Max

Berikut adalah analisis perilaku strategi bittrade-v2 setelah upgrade Fase 4:

### 1. Perilaku Pyramiding / Scaling-in
- Bot melakukan pembelian berulang (pyramiding) setiap 5 menit ketika kondisi trending bullish terpenuhi (`EMA-13 > EMA-34`, harga di atas `VWAP`, dan tren 15 menit bullish).
- Hal ini menyebabkan ukuran posisi membesar secara eksponensial di awal tren, namun porsi budget yang digunakan semakin mengecil (karena 20% dari *sisa* simulated balance). Sebagai contoh, jika saldo $1000, bot membeli $200, lalu $160, lalu $128, dst.
- **Masalah:** Pembelian berulang ini sering kali menaikkan harga rata-rata entry (*average cost basis*) bot mendekati puncak tren. Ketika tren berbalik arah sedikit saja, bot terpaksa keluar dengan kerugian karena harga jual berada di bawah harga beli rata-rata, meskipun harga saat sell masih lebih tinggi dari order BUY pertama.

### 2. Kinerja Proteksi Modal vs Whipsaw
- **Emergency Stop Loss -1.2%**: Sangat disiplin membatasi kerugian besar. Ini mencegah terjadinya kerugian dalam (deep losses), namun sering terpicu jika terjadi koreksi kecil pada timeframe 1 menit.
- **Trailing Take Profit**: Cukup baik mengunci keuntungan saat terjadi lonjakan harga cepat, tetapi jarang tercapai jika tren naik secara perlahan dan kemudian berbalik arah mendadak.
- **Normal Sell Sinyal**: Sinyal jual normal (`EMA-13 < EMA-34` atau tren 15 menit bearish) sering kali terlambat bertindak dibandingkan dengan kecepatan koreksi harga di timeframe 1 menit, sehingga keuntungan yang sempat diperoleh menguap dan berubah menjadi rugi kecil.

### 3. Rekomendasi Perbaikan untuk Didiskusikan dengan Claude Max
1. **Batasi Maksimum Pyramiding (Max Pyramiding Layers):** Jangan izinkan bot melakukan BUY lebih dari misalnya 2 atau 3 kali dalam satu siklus trending. Hal ini mencegah naiknya rata-rata entry price ke area rawan koreksi.
2. **Perbaiki Rumus P&L Logging:** Perbaiki bug di `executor.rs` agar merekam rata-rata harga beli berbobot (weighted average buy price) dari tabel `bot_active_positions` untuk menghitung realized P&L yang akurat.
3. **Gunakan Waktu Jeda Cooldown Antar-Buy yang Lebih Panjang:** Atau naikkan threshold volume surge confirmation untuk order buy tambahan (pyramiding).
4. **Trailing Stop Berdasarkan Average Entry Price:** Trailing take profit dan stop loss sebaiknya dihitung dari *weighted average buy price* posisi aktif saat ini, bukan dari *last buy price*.

## 🛠️ Struktur File Sistem (Arsitektur Modul Rust)

Untuk membantu Claude Max memahami bagaimana kode Rust diatur, berikut adalah rincian fungsionalitas dan fungsi utama dari setiap file di `rust_bot/src`:

### 1. `main.rs` (Orchestrator Utama)
- **Fungsi Utama:** Merupakan entry point (`fn main()`) bot. Mengatur inisialisasi koneksi PostgreSQL pool, migrasi/pembuatan skema tabel database jika belum ada, dan memulihkan state bot saat crash/restart dengan membaca tabel `bot_active_positions` (Crash Recovery).
- **Alur Kerja:** 
  - Memulai WebSocket Price Listener di background via thread tokio spawn untuk mengambil harga real-time.
  - Menjalankan Loop Trader Menit Baru (`loop` 60 detik) yang mensinkronkan 5 data kline terbaru dari Binance API, melakukan fallback ke REST API jika WebSocket stale (>30 detik), memanggil modul `conclude::analyze_market` untuk analisa indikator, memvalidasi order via `validate::validate_decision`, mengeksekusi order via `executor::execute_trade`, dan mencatat saldo terbaru ke `bot_balance_history`.
  - Menjalankan HTTP Axum web server di port `8087` untuk menyajikan REST API (`/api/status`, `/api/history`, `/api/logs`, dll.) untuk frontend dashboard.

### 2. `conclude.rs` (Otak Analisis Indikator & Keputusan)
- **Fungsi Utama:** Melakukan kalkulasi teknikal real-time dan mengeluarkan keputusan trading (`Decision::Buy`, `Decision::Sell`, atau `Decision::Wait`).
- **Kalkulasi & Logika:**
  - `calculate_ema(prices, period)`: Menghitung Exponential Moving Average (EMA) secara efisien.
  - `analyze_market(price, state)`:
    - Melakukan pengecekan pengaman darurat (Emergency Stop Loss -1.2%, Trailing Take Profit pullback 1.0% jika profit sempat menyentuh >=1.5%, atau Hard Take Profit +3.0% dari harga beli posisi aktif).
    - Membaca 50 candle kline dari DB dan memvalidasi kesenjangan waktu (missing candle check).
    - Menghitung EMA-13 dan EMA-34.
    - Menghitung True Session VWAP (akumulasi volume sejak 00:00 UTC).
    - Menghitung Bollinger Bands 50-period dan Volatilitas.
    - Mengklasifikasikan Regime Pasar (SIDEWAYS jika Volatilitas < 0.085%, jika tidak masuk ke TRENDING: BULLISH jika EMA13 > EMA34, BEARISH jika sebaliknya).
    - Mengeksekusi strategi regime: Sideways menggunakan Bollinger Bands 50 Mean Reversion (minimal lebar band 1.0%), sedangkan Trending menggunakan Golden/Death Cross EMA-13/34 dikonfirmasi oleh posisi harga di atas/bawah VWAP dan filter HTF tren 15 menit (`prices[0] > prices[15]`).

### 3. `validate.rs` (Modul Validasi Keamanan)
- **Fungsi Utama:** Berfungsi sebagai gerbang pertahanan sebelum order dikirim ke executor (`validate_decision`).
- **Aturan Validasi:**
  - Mengabaikan validasi cooldown jika keputusan berlabel `[Darurat]` (Stop Loss / Trailing TP / Dump Mendadak) agar bot bisa langsung melikuidasi posisi tanpa hambatan.
  - **Minimum Holding Time (15 Menit):** Mencegah aksi SELL normal jika posisi baru dibuka kurang dari 15 menit (mengatasi whipsaw noise).
  - **Cooldown Transaksi (5 Menit):** Membatasi agar bot tidak melakukan aksi transaksi berturut-turut yang sama (BUY setelah BUY atau SELL setelah SELL) dalam jeda 5 menit.
  - **Min Size & Balance Check:** Memastikan nominal transaksi minimal $5.0 dan saldo USDT (untuk BUY) atau saldo BTC (untuk SELL) mencukupi (termasuk perhitungan fee admin 0.1%).

### 4. `executor.rs` (Simulator Eksekutor Transaksi)
- **Fungsi Utama:** Mensimulasikan eksekusi order riil di database dan memori (`execute_trade`).
- **Alur Kerja:**
  - **BUY:** Memotong saldo simulated USDT, menambahkan btc_balance, memperbarui high water mark di memori, mencatat detail pembelian di tabel `bot_active_positions` (untuk tracking trailing profit), dan meng-insert log transaksi sukses ke tabel `bot_trading_history`.
  - **SELL:** Menambahkan simulated USDT (dikurangi fee admin 0.1%), mengurangi btc_balance, mereset high water mark ke 0, menghapus posisi aktif di `bot_active_positions`, menghitung realized P&L, dan mencatat transaksi sukses ke `bot_trading_history`.
  - **Bug Kalkulasi P&L:** Di dalam blok `SELL`, formula pencatatan realized P&L mengambil harga beli (`buy_price`) dari order BUY terakhir saja (`LIMIT 1`) dari database, bukan rata-rata tertimbang, sehingga memicu kesalahan logging P&L jika bot melakukan pyramiding (membeli berkali-kali).

### 5. `get.rs` (Konektor WebSocket & REST API Binance)
- **Fungsi Utama:** Menyediakan jembatan data harga dan kline dari Binance API.
- **Metode Utama:**
  - `start_price_listener(state)`: Membuka WebSocket stream ke Binance (`wss://stream.binance.com:9443/stream?streams=...`) secara asynchronous untuk memperbarui harga real-time BTC, ETH, BNB, SOL, dan XRP di memori state, serta memperbarui high water mark secara real-time.
  - `sync_klines(db, limit)`: Melakukan query HTTP REST API ke endpoint klines Binance (`/api/v3/klines`) untuk mengambil data candlestick 1 menit terakhir dan melakukan `INSERT ON CONFLICT DO UPDATE` ke tabel `btc_klines` PostgreSQL.
  - `get_rest_price()`: Mendapatkan harga spot BTCUSDT secara REST API untuk fallback jika WebSocket stale.

### 6. `corrector.rs` (Pencatat Kesalahan Sistem)
- **Fungsi Utama:** Menyediakan fungsi `log_error(state, error_type, reason)` untuk mencatat log kegagalan operasional (seperti kegagalan eksekusi saldo, kegagalan koneksi DB, stale websocket, dsb.) ke tabel `bot_corrections` secara asynchronous dan memperbarui indikator LED corrector di dashboard.
