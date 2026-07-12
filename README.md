# BitTrade-v2: Multi-Bot Trading System & Unified Dashboard (Rust & Axum)

Sistem perdagangan aset kripto otomatis multi-bot berkinerja tinggi yang ditulis dalam Rust, mendukung eksekusi mandiri, isolasi skema basis data PostgreSQL, dan dipantau melalui satu Dashboard Utama terintegrasi.

---

## 🚀 Ikhtisar Arsitektur Multi-Bot

Sistem ini terdiri dari **empat bot independen** yang berjalan di background, masing-masing dengan port dan tabel database yang terisolasi untuk menghindari tabrakan data:

| Bot / Modul | Port | Deskripsi Engine | Prefix Tabel DB | Direktori Kerja |
| :--- | :---: | :--- | :---: | :--- |
| **Bot A** (Trend/Scalper) | `8087` | Bot Binance BTC/USDT berbasis tren EMA-13/34 & VWAP | `bot_` | `rust_bot/` |
| **Bot B** (SmartDCA) | `8088` | Akumulasi bertahap (DCA 3-layer) di zona diskon RSI < 30 | `dca_` | `smartdca/` |
| **Bot C** (OKX Engine) | `8091` | Mesin trading spot mandiri khusus API Publik OKX | `okx_` | `OKX_trading/` |
| **Bot D** (Altcoin Engine)| `8092` | Kloning Bot A untuk target altcoin Binance | `alt_` | `Bot_d_Altcoin/` |
| **Bot E** (statARB Engine)| `8093` | Mesin statistical arbitrage (ETH/BTC co-integration spread) | `starb_` | `statARB/` |

---

## 🛠️ Desain Mekanisme & Strategi Trading

### 1. Bot A & Bot D: Market Regime-Based (Trend confirmation)
*   **Deteksi Kondisi Pasar**: Menganalisis StdDev volatilitas persentase 20 kline terakhir.
    *   **Sideways** (< 0.075%): Menggunakan **Mean Reversion (Bollinger Bands 50-Period)** dengan filter lebar minimal spread 1.0% (mencegah overtrading akibat komisi exchange).
    *   **Trending** ($\ge$ 0.075%): Menggunakan **EMA Crossover (EMA-13/34)** dengan konfirmasi volume (VWAP reset harian 00:00 UTC & Volume Surge).
*   **Trailing Take Profit**: Melacak HWM (High Water Mark). Mengunci profit setelah menyentuh target minimal (+1.5%) jika terjadi pullback sebesar 1.0%.
*   **Emergency Protection**: Stop Loss mutlak diperketat pada 1.2% dan diabaikan dari cooldown.

### 2. Bot B: SmartDCA (Discount-Zone Accumulation)
*   **Layer Entry**: Membuka Layer 1 saat RSI-14 jenuh jual (< 30).
*   **Perlindungan Tambahan**: Mengeksekusi Layer 2 (-1.5%) dan Layer 3 (-3.0%) untuk menurunkan basis harga rata-rata.
*   **Panic Dump Delay**: Menangguhkan pembelian selama 15 menit jika terjadi Volume Surge > 5.0x dengan penurunan tajam > 0.5%.
*   **Exit**: Target keuntungan +1.2% bersih, dilengkapi Trailing Exit (pullback 0.5% dari HWM) dan Hard SL 5.0%.

### 3. Bot C: OKX Engine
*   Engine terisolasi yang berjalan dengan arsitektur menyerupai Bot A tetapi dioptimalkan secara asinkron untuk berinteraksi langsung dengan orderbook dan data pasar OKX.

### 4. Bot E: Statistical Arbitrage Engine (statARB)
*   **Co-Integration Spread Modeling**: Memantau rasio harga ETH/BTC secara real-time pada resolusi 1-detik dan menghitung statistik rolling window (Mean & StdDev).
*   **Dual-Leg Spread Entry**: 
    *   **SELL_SPREAD**: Masuk posisi Short ETHUSDT & Long BTCUSDT saat Z-score > +2.0 (ETH relatif overpriced terhadap BTC).
    *   **BUY_SPREAD**: Masuk posisi Long ETHUSDT & Short BTCUSDT saat Z-score < -2.0 (ETH relatif underpriced terhadap BTC).
*   **Mean Reversion Exit**: Menutup kedua kaki spread secara simultan saat Z-score kembali ke area netral (Z-score < +0.2 untuk Sell Spread, atau Z-score > -0.2 untuk Buy Spread).
*   **Capital Allocation & Fee Friction**: Alokasi modal dibagi rata 50:50 pada kedua leg, dengan memperhitungkan beban komisi taker 0.04% per leg (total friction drag 0.16% per siklus).

---

## 🗄️ Skema Tabel Basis Data (PostgreSQL)

Setiap bot menggunakan koneksi PgPool yang sama dari `.env` root, namun memisahkan data ke tabelnya masing-masing secara otomatis saat startup:

### Bot A (`bot_` Prefix)
*   `bot_trading_history`: Riwayat transaksi (BUY/SELL) beserta realize P&L di kolom `notes`.
*   `bot_corrections`: Catatan kegagalan/error sistem secara kronologis.
*   `bot_balance_history`: Log historis USDT balance, BTC balance, dan Total Equity.
*   `bot_active_positions`: Melacak posisi terbuka aktif dan nilai HWM berjalan.

### Bot B (`dca_` Prefix)
*   `dca_trading_history` & `dca_active_positions` & `dca_balance_history`
*   `dca_cycle_summary`: Laporan agregat per siklus DCA (total layer, P&L nominal, alasan keluar).

### Bot C (`okx_` Prefix)
*   `okx_trading_history` & `okx_corrections` & `okx_balance_history` & `okx_active_positions`
*   `okx_klines`: Data kline 1 menit historis OKX.

### Bot D (`alt_` Prefix)
*   `alt_trading_history` & `alt_corrections` & `alt_balance_history` & `alt_active_positions`

### Bot E (`starb_` Prefix)
*   `starb_trading_history`: Riwayat eksekusi spread (action BUY_SPREAD / SELL_SPREAD / CLOSE_SPREAD).
*   `starb_active_positions`: Melacak posisi spread aktif beserta rasio entry dan Z-score entry.
*   `starb_balance_history`: Log historis saldo simulasi, modal terdeploy, dan total ekuitas.
*   `starb_pair_stats`: Log pemantauan statistik rasio pasangan ETH/BTC real-time dan nilai Z-score berjalan.
*   `starb_corrections`: Catatan kegagalan/error sistem secara kronologis.

> [!NOTE]
> Tabel public `btc_klines` digunakan bersama oleh bot berbasis Binance untuk menghemat query sinkronisasi histori kline ke exchange.

---

## 🌐 Dashboard Pemantauan Utama Terintegrasi

Dashboard utama menyatukan kelima bot ke dalam satu tampilan web responsif premium berdesain "Warm Paper":

1.  **Pemisahan Frekuensi Fetching (Bebas Lag)**:
    *   **Lightweight Real-time Status (Setiap 3 Detik)**: Hanya mengunduh status proses (LED status aktif, pipa data WS, analis, validator, eksekutor, serta harga dan P&L real-time). Membuat halaman menyala instan tanpa menunggu data berat.
    *   **Heavy Analytics & Charting (Setiap 30 Detik)**: Mengunduh data historis balance dan orderbook untuk grafik dan tabel gabungan. Mencegah penumpukan antrean koneksi di browser.
2.  **Filter Riwayat Transaksi Instan**: Tab penapis client-side (Semua Bot, Bot A, Bot B, Bot C, Bot D, Bot E) yang menyaring tabel transaksi secara asinkron tanpa reload halaman.
3.  **Grafik Pertumbuhan Modal Aligned**: Membandingkan kurva pertumbuhan modal kelima bot secara kronologis dengan opsi resolusi (Per Hari, Per Jam, Semua Data).
4.  **Kalkulasi Real-time & Normalisasi Bot D (Altcoin Engine)**:
    *   *Real-time Metrics*: Dashboard utama mengambil data langsung dari `/alt/api/alt_coins` secara realtime untuk mengagregasikan sisa saldo USDT (Cash) dan estimasi nilai aset altcoin dari seluruh pekerja koin aktif. P&L dihitung secara akurat berdasarkan total alokasi awal (jumlah koin aktif * $1,000).
    *   *Normalisasi Grafik*: Mengingat total ekuitas Bot D mencakup kumulatif seluruh koin aktif (sekitar ~$65k+), data historisnya dinormalisasi kembali ke basis modal awal `$1000` di dalam grafik terpadu agar kurva performa persentase antar-bot sebanding tanpa mendistorsi skala sumbu-Y.
5.  **Antarmuka Spesifik Bot E (statARB Engine)**:
    *   *Tab-Mode Dashboard*: Mengadopsi tata letak tab yang interaktif (*Overview*, *Scanner*, *History*, *Logs*) untuk menavigasi metrik yang padat tanpa membebani browser.
    *   *Real-time 300+ Pairs Scanner*: Menampilkan tabel *live-scanning* kointegrasi pada lebih dari 300 pasar USDT-M secara asinkron dengan filter sektoral.
    *   *Interactive Academic Paper*: Menyediakan publikasi kertas kerja teknis (Bilingual/Dwibahasa) mengenai model *mean reversion* dalam format "buku digital" (*slide-pages*) untuk presentasi eksekutif di `/paper_statarb`.

---

## ⚙️ Petunjuk Menjalankan Aplikasi & Deployment

### 1. Konfigurasi Kunci Lingkungan (`.env`)
Pastikan file `.env` di root `/root/bittrade-v2-strategi/.env` telah terisi dengan benar:
```env
DATABASE_URL=postgres://postgres:password@127.0.0.1:5432/bittrade
```

### 2. Membangun Binary Produksi (Release Mode)
Untuk kinerja optimal dan latensi komputasi terendah, kompilasi semua bot dengan flag `--release`:
```bash
# Bot A
cd /root/bittrade-v2-strategi/rust_bot && cargo build --release

# Bot B
cd /root/bittrade-v2-strategi/smartdca && cargo build --release

# Bot C
cd /root/bittrade-v2-strategi/OKX_trading && cargo build --release

# Bot D
cd /root/bittrade-v2-strategi/Bot_d_Altcoin && cargo build --release

# Bot E
cd /root/bittrade-v2-strategi/statARB && cargo build --release
```

### 3. Menjalankan Bot di Background (Detached Mode)
Gunakan perintah `nohup` untuk memastikan bot terus beroperasi 24/7 setelah sesi SSH/terminal diakhiri:
```bash
# Jalankan Bot A (Port 8087)
cd /root/bittrade-v2-strategi/rust_bot
nohup ./target/release/rust_bot > bot.log 2>&1 &

# Jalankan Bot B (Port 8088)
cd /root/bittrade-v2-strategi/smartdca
nohup ./target/release/smartdca > dca.log 2>&1 &

# Jalankan Bot C (Port 8091)
cd /root/bittrade-v2-strategi/OKX_trading
nohup ./target/release/okx_trading > okx.log 2>&1 &

# Jalankan Bot D (Port 8092)
cd /root/bittrade-v2-strategi/Bot_d_Altcoin
nohup ./target/release/bot_d_altcoin > bot_d.log 2>&1 &

# Jalankan Bot E (Port 8093)
cd /root/bittrade-v2-strategi/statARB
nohup ./target/release/stat_arb_engine > statarb.log 2>&1 &
```

### 4. Perintah Pemeliharaan (Maintenance Commands)
*   **Melihat Log Proses**:
    *   Bot A: `tail -n 100 -f /root/bittrade-v2-strategi/rust_bot/bot.log`
    *   Bot B: `tail -n 100 -f /root/bittrade-v2-strategi/smartdca/dca.log`
    *   Bot C: `tail -n 100 -f /root/bittrade-v2-strategi/OKX_trading/okx.log`
    *   Bot D: `tail -n 100 -f /root/bittrade-v2-strategi/Bot_d_Altcoin/bot_d.log`
    *   Bot E: `tail -n 100 -f /root/bittrade-v2-strategi/statARB/statarb.log`
*   **Menghentikan Proses Bot**:
    *   Bot A: `fuser -k 8087/tcp`
    *   Bot B: `fuser -k 8088/tcp`
    *   Bot C: `fuser -k 8091/tcp`
    *   Bot D: `fuser -k 8092/tcp`
    *   Bot E: `fuser -k 8093/tcp`

### 5. Akses Web Portal Utama (Melalui HTTPS Proxy Apache)
Seluruh API dan dashboard utama dilewatkan melalui reverse-proxy Apache. Buka peramban di:
```
https://tradingsafe.mijdigital.my/
```
Apache akan mengarahkan sub-path ke masing-masing engine secara transparan:
*   `/` $\rightarrow$ Menyajikan frontend statis Astro dari `/var/www/tradingsafe` dan meneruskan `/api/` ke Port `8087` (Bot A API)
*   `/dca/` $\rightarrow$ Menyajikan dashboard DCA statis dan meneruskan `/dca/api/` ke Port `8088` (Bot B API)
*   `/okx/` $\rightarrow$ Meneruskan data statis & API `/okx/` ke Port `8091` (Bot C Engine)
*   `/arbitrage/` $\rightarrow$ Menyajikan dashboard Arbitrage statis dan meneruskan `/arbitrage/api/` ke Port `8092` (Bot D API)
*   `/statarb/` $\rightarrow$ Menyajikan dashboard statARB statis dan meneruskan `/statarb/api/` ke Port `8093` (Bot E API)
