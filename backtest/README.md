# 📊 Panduan Backtest Strategi Bittrade

Direktori ini berisi skrip pengujian historis (backtest) terisolasi untuk strategi-strategi market regime Bittrade.

---

## 📂 Struktur File

| File Backtest | Fungsi Utama | Regime Pasar |
| :--- | :--- | :--- |
| [`run_backtest_uptrend.py`](file:///root/bittrade-v2-strategi/backtest/run_backtest_uptrend.py) | Strategi Trend-Following (EMA Golden Cross) | Uptrend (Pasar Naik) |
| [`run_backtest_sideways.py`](file:///root/bittrade-v2-strategi/backtest/run_backtest_sideways.py) | Strategi Volatility & Grid-Style | Sideways (Pasar Datar/Konsolidasi) |
| [`run_backtest_breakout.py`](file:///root/bittrade-v2-strategi/backtest/run_backtest_breakout.py) | Strategi Breakout Volume & BB Bands | Breakout (Pergerakan Eksplosif) |
| [`run_backtest_downtrend.py`](file:///root/bittrade-v2-strategi/backtest/run_backtest_downtrend.py) | Bearish Climax Rebound Catcher | Downtrend (Pasar Turun/Koreksi) |
| [`run_backtest_smartdca.py`](file:///root/bittrade-v2-strategi/backtest/run_backtest_smartdca.py) | Strategi Akumulasi Bertahap (DCA 3-layer) dengan 3x Leverage | Volatile/Multi-Coin (SmartDCA) |
| [`run_backtest.py`](file:///root/bittrade-v2-strategi/backtest/run_backtest.py) | Combined Multi-Regime Coordinator | Semua Regime Terpadu |

---

## 🚀 Cara Menjalankan Backtest

Setiap skrip menerima parameter baris perintah (CLI) berikut:
```bash
python3 run_backtest_[regime].py [modal_awal] [tp_hard] [sl_limit] [mode]
```

### 1. Eksekusi Uptrend:
```bash
# Mode Optimized Dinamis (Default)
python3 run_backtest_uptrend.py 1000.0 0.03 -0.015 optimized

# Mode Safe (Konservatif)
python3 run_backtest_uptrend.py 1000.0 0.03 -0.015 safe
```

### 2. Eksekusi Sideways:
```bash
# Mode Optimized (Perbaikan, Nyaris Impas)
python3 run_backtest_sideways.py 1000.0 0.03 -0.015 optimized

# Mode Safe (Bawaan Asli, Sangat Merugi)
python3 run_backtest_sideways.py 1000.0 0.03 -0.015 safe
```

### 3. Eksekusi SmartDCA (Multi-Coin):
```bash
# Menguji koin BTC (default fallback dari btc_klines)
python3 run_backtest_smartdca.py 1000.0 BTCUSDT

# Menguji koin volatil lain (menggunakan data dca_klines)
python3 run_backtest_smartdca.py 1000.0 ACEUSDT
```

---

## 🛠️ Detail Mode Pengujian

### A. STRATEGI UPTREND (`run_backtest_uptrend.py`)

#### 1. Mode `optimized`
* **Karakteristik:** Menggunakan filter eksklusif, alokasi modal dinamis (*dynamic position sizing*), dan optimasi trailing take profit.
* **Filter Utama:** Durasi Golden Cross $\le 35$m, RSI Entry **64 s/d 70**, RSI Slope 15m $\ge 5.0$, Vol Surge 3m $\ge 0.8x$.
* **Alokasi Modal Dinamis:** **40% modal** jika `RSI Slope 15m >= 8.0` (High Conf), sisanya **10% modal**.
* **Statistik Performa:** Win Rate **60.47%** | Drawdown **0.91%** | Status **Profit (+$14.38)**.

#### 2. Mode `safe`
* **Karakteristik:** Fokus pada keamanan modal dengan menyaring transaksi se-selektif mungkin.
* **Filter Utama:** Durasi Golden Cross $\le 15$m, Blokir **RSI 75 s/d 80**, RSI Slope 7m $\le 8.0$.
* **Alokasi Modal:** Flat **20% modal**.
* **Statistik Performa:** Win Rate **47.37%** | Drawdown **1.26%** | Status **Hampir Impas (-$8.74)**.

---

### B. STRATEGI SIDEWAYS (`run_backtest_sideways.py`)

#### 1. Mode `optimized` (Sangat Direkomendasikan)
* **Karakteristik:** Menyaring kebisingan pasar (*noise*) dengan mempersempit volatilitas, memperketat Bollinger Bands, serta menyaring momentum buruk menggunakan 3 filter eksklusif.
* **Parameter:** Bollinger Bands **20 Period, 2.5 Multiplier**.
* **Filter Volatilitas:** Hanya entry jika volatilitas berada di kisaran **`0.15% s/d 0.25%`**.
* **Filter Momentum:** Memblokir entry pisau jatuh (`price_drop_3m < -0.48%`) dan pergerakan datar tanpa momentum (`price_drop_3m > -0.18%` atau `rsi_slope_3m > -4.0`).
* **Alokasi Modal:** Flat **10% modal**.
* **Statistik Performa:** Win Rate **53.57%** | Drawdown **0.40%** | Status **Profit (+$3.48)**.

#### 2. Mode `safe` (Bawaan Asli)
* **Karakteristik:** Membiarkan transaksi berjalan pada volatilitas yang sangat rendah sehingga modal habis dimakan biaya transaksi (*fee*).
* **Parameter:** Bollinger Bands **50 Period, 2.0 Multiplier**.
* **Filter Volatilitas:** `< 0.085%`.
* **Alokasi Modal:** Dinamis **30% - 45%**.
* **Statistik Performa:** Win Rate **29.32%** | Drawdown **62.97%** | Status **Sangat Merugi (-$629.38)**.

---

### C. STRATEGI DOWNTREND (`run_backtest_downtrend.py`)

#### 1. Mode `optimized` (Sangat Direkomendasikan)
* **Karakteristik:** Menggunakan filter diskon ekstrem terhadap VWAP harian dan Micro TP untuk mengamankan profit pantulan cepat.
* **Filter Utama:** `vwap_dist <= -0.80%`, RSI Entry `< 30.0`, Vol Surge $\ge 3.0x$, BB Width $\ge 0.5\%$, Konfirmasi 2 candle hijau berturut-turut.
* **Take Profit:** Micro TP `+0.80%`.
* **Kunci Jual (Lock Time):** `12 Menit`.
* **Alokasi Modal:** Flat **30% modal**.
* **Statistik Performa:** Win Rate **75.00%** | Drawdown **0.40%** | Status **Profit (+$0.69)**.

#### 2. Mode `safe` (Bawaan Asli)
* **Karakteristik:** Menangkap rebound tanpa filter VWAP, sehingga terjebak pantulan palsu dan terpaksa melakukan sell rugi di menit ke-15.
* **Filter Utama:** RSI Entry `< 30.0`, Vol Surge $\ge 3.0x$, BB Width $\ge 0.5\%$.
* **Take Profit:** `+3.00%`.
* **Kunci Jual (Lock Time):** `15 Menit`.
* **Alokasi Modal:** Flat **30% modal**.
* **Statistik Performa:** Win Rate **30.00%** | Drawdown **1.38%** | Status **Rugi (-$13.84)**.

---

### D. STRATEGI BREAKOUT (`run_backtest_breakout.py`)

#### 1. Mode `optimized` (Sangat Direkomendasikan)
* **Karakteristik:** Menyaring volatilitas rendah, riak kecil, membatasi jarak aman dari VWAP harian, dan memastikan konfirmasi kekuatan tren EMA sebelum masuk ketika ada lonjakan.
* **Filter Utama:** `StdDev >= 30.0` (volatilitas), `spike_pct >= 0.5%` (ukuran lonjakan), `RSI >= 65.0` (momentum), `vwap_dist <= 1.5%` (batas atas VWAP), `e13_gap >= 0.05%` jika spike > 0.6%.
* **Take Profit:** `+3.00%` hard / Trailing TP (+1.5% trigger, 1.0% pullback).
* **Kunci Jual (Lock Time):** `15 Menit`.
* **Alokasi Modal:** Flat **25% modal**.
* **Statistik Performa:** Win Rate **64.71%** | Drawdown **0.50%** | Status **Profit (+$0.26)**.

#### 2. Mode `safe` (Bawaan Asli)
* **Karakteristik:** Melompat masuk ke setiap lonjakan kecil saat pasar sepi, memicu ratusan transaksi jebakan puncak harga (*fake breakout*).
* **Filter Utama:** `StdDev >= 5.0`, volume meledak $\ge 3.0x$ di atas rata-rata.
* **Take Profit:** `+3.00%`.
* **Kunci Jual (Lock Time):** `15 Menit`.
* **Alokasi Modal:** Flat **25% modal**.
* **Statistik Performa:** Win Rate **18.35%** | Drawdown **11.89%** | Status **Sangat Merugi (-$118.91)**.

---

### E. KOORDINATOR TERPADU (`run_backtest.py`)

#### 1. Mode `optimized` (Sangat Direkomendasikan)
* **Karakteristik:** Menjalankan ke-4 strategi teroptimasi secara paralel tanpa saling mengunci. Masing-masing strategi bebas membuka posisi secara independen hingga batas maksimal global 2 layer aktif.
* **Filter Utama:** Menggunakan kombinasi BB20 (Sideways), BB50 (Downtrend/Breakout), Golden Cross Duration (Uptrend), limit VWAP distance, dan konfirmasi gap EMA.
* **Statistik Performa:** Win Rate **59.49%** | Drawdown **1.87%** | Status **Profit Terpadu (+$12.78)**.

#### 2. Mode `safe` (Bawaan Asli)
* **Karakteristik:** Menjalankan ke-4 strategi bawaan asli yang saling bersaing tidak terkontrol, menggerus modal akibat transaksi palsu di pasar sideways.
* **Statistik Performa:** Win Rate **26.05%** | Drawdown **71.39%** | Status **Sangat Merugi (-$712.80)**.

---

### F. STRATEGI SMARTDCA (`run_backtest_smartdca.py`)
* **Karakteristik:** Pembelian bertahap multi-layer (Layer 1: 40%, Layer 2: 30%, Layer 3: 30% dari budget koin) dengan simulasi leverage 3x dan perhitungan likuidasi serta Stop Loss.
* **Filter Utama:**
  * **Layer 1:** Drop $\le$ -2.5%, RSI < 50.0, lolos `rsi_allowed` (falling knife), dan harga di atas `ema_750` (`trend_ok`).
  * **Layer 2:** Drop $\le$ -5.0%, RSI < 50.0 (Tanpa filter `rsi_allowed`).
  * **Layer 3:** Drop $\le$ -8.0%, RSI < 40.0 (Tanpa filter `rsi_allowed`).
* **Keluar Posisi:** Hard Take Profit `+2.5%` | Trailing TP (Profit $\ge$ 1.5% dan drop 0.8% dari HWM) | Stop Loss Darurat `-5.0%`.
* **Statistik Performa (ACEUSDT):** Win Rate **77.55%** | Drawdown **35.88%** | Status **Profit Terkomparasi (+$155.33)**.

---

## 🔄 Riwayat Evolusi Arsitektur Bot A

| Fase | Tanggal | Perubahan Utama | Hasil Live |
| :--- | :--- | :--- | :--- |
| **Fase 4.1** | ~Jun 2026 | Pengetatan pyramiding (max 3 layer), filter whipsaw 2 menit | Win Rate 30%, Net P&L -$12.22 |
| **Fase 5.0 (QPS)** | 12 Jul 2026 | Quarter Kelly Criterion + OBI filter; `threshold` statis menyebabkan 0 transaksi selama 9 hari | 0 eksekusi (over-filtered) |
| **Fase 5.1 (Modular Engines)** | Jul 2026 | 4 mesin terpisah: `uptrend.rs`, `sideways.rs`, `downtrend.rs`, `breakout.rs` | Win Rate 59.49%, Drawdown 1.87% |
| **Fase 5.2 (Multi-Coin Scalper)** | 9 Agu 2026 | WebSocket All-Market ticker Binance, scan hingga **100 koin** sekaligus, klasifikasi volatilitas otomatis (LOW / HIGH / HYPER / EXTREME), trailing TP per kategori | **Win Rate 100%** (25 transaksi, live sejak 9 Agu 2026) |

---

## 📝 Catatan Pemeliharaan

- **Database:** Semua skrip memanggil data klines historis dari tabel PostgreSQL `btc_klines` (untuk BTC) atau `dca_klines` (untuk Altcoin) menggunakan kredensial di file `.env` root.
- **Pemisahan Logika:** Jangan menyatukan logika antarfile backtest agar perbaikan atau optimalisasi satu strategi tidak memicu bug di strategi lainnya.
- **Arsitektur Aktif (Fase 5.2):** Bot A kini berjalan menggunakan engine Rust dengan WebSocket *all-market mini-ticker* Binance (`wss://stream.binance.com/stream?streams=!miniTicker@arr`), mendukung pemrosesan **Binary Frame** dan **Text Frame** secara transparan. Data ticker 100 koin disimpan di `Arc<RwLock<HashMap>>` dan diperbarui tanpa REST polling.
- **Dashboard Live:** [https://tradingsafe.mijdigital.my/bota/](https://tradingsafe.mijdigital.my/bota/)
- **Versi Engine:** Rust `rust_bot` v0.1.0 — dikelola melalui PM2 (`bittrade-v2-bot-a`, port **8087**).
