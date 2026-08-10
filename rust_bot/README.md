# 🤖 BitTrade-v2 Bot A Trading Engine (QPS Version v5.0_qps)

Selamat datang di kode sumber utama **Bot A** (BitTrade-v2 Engine). Bot ini dibangun menggunakan bahasa pemrograman **Rust** yang dirancang asinkron, performa tinggi, dan tangguh untuk live trading pasangan BTC/USDT secara real-time.

Arsitektur sistem ini menggunakan pendekatan **Quant Professional System (QPS)** dengan pembagian modul strategi dinamis yang diselaraskan dengan parameter emas hasil optimasi backtest.

---

## 📂 Struktur Folder & Komponen Utama

Untuk mempermudah pemeliharaan dan pengujian, logika keputusan bot dipecah menjadi beberapa modul strategis:

```
rust_bot/
├── src/
│   ├── main.rs         - Pintu masuk aplikasi, inisialisasi database, REST/WS Binance, & web server.
│   ├── conclude.rs     - Otak koordinator. Menghitung ATR/VWAP/StdDev, memanggil sub-strategi, & Stop Loss.
│   ├── risk.rs         - Manajemen risiko Kelly Criterion & Sharpe Ratio berdasarkan 20 trade terakhir.
│   ├── validate.rs     - Validator transaksi (mengecek pyramiding max 2 layer, anti-FOMO, & cooldown).
│   ├── executor.rs     - Mengeksekusi transaksi buy/sell, update balance, dan menyimpan history ke DB.
│   ├── uptrend.rs      - [NEW] Strategi Bullish: EMA Golden Cross, RSI, RSI Slope, & volume momentum.
│   ├── sideways.rs     - [NEW] Strategi Ranging: Bollinger Bands 20 (multiplier 2.5) & filter anti-pisau jatuh.
│   ├── downtrend.rs    - [NEW] Strategi Bearish Climax Rebound: oversold catcher (<30), Micro TP +0.80%.
│   ├── breakout.rs     - [NEW] Strategi Breakout Pump: StdDev >= 30, OBI >= 0.40, & VWAP distance filter.
│   ├── get.rs          - Sinkronisasi kline historis & harga market via REST API/WebSocket.
│   └── corrector.rs    - Log error penanganan mandiri (self-correction).
```

---

## 📈 Parameter Emas & Formula Strategi

Setiap modul strategi berjalan secara independen dan dievaluasi setiap menit tanpa saling mengunci (*no exclusivity locks*), mendukung hingga **maksimal 2 layer posisi aktif** secara global.

### 1. Modul Uptrend (Trending Bullish)
*   **Kondisi Entry:** Golden Cross `EMA_FAST > EMA_SLOW` + Harga di atas VWAP Sesi + Tren 15m Bullish.
*   **Filter Proteksi:**
    *   Durasi Golden Cross harus $\le 35$ menit (mencegah beli di pucuk).
    *   RSI 14-period berada di range aman `[64.0, 70.0]`.
    *   RSI Slope 15m $\ge +5.0$.
    *   Volume Surge 3m $\ge 0.8x$.
    *   Order Book Imbalance (OBI) $\ge 0.40$.
*   **Sizing Anggaran:** Dinamis (40% modal jika RSI Slope 15m $\ge 8.0$ (sangat kuat), jika tidak 10% modal).
*   **Exit:** Death Cross terkonfirmasi 2 menit berturut-turut + harga di bawah VWAP atau tren 15m bearish.

### 2. Modul Sideways (Ranging Market)
*   **Klasifikasi Sideways:** Volatilitas Bollinger Bands 20-period berada di kisaran `0.15% s/d 0.25%`.
*   **Kondisi Entry:** Harga menyentuh batas bawah `BB20` dengan `stddev_multiplier = 2.5`.
*   **Filter Anti-Pisau Jatuh:**
    *   Blokir jika harga jatuh terlalu tajam: `price_drop_3m < -0.48%`.
    *   Blokir jika harga terlalu datar/lemas: `price_drop_3m > -0.18%` atau `rsi_slope_3m > -4.0`.
    *   Order Book Imbalance (OBI) $\ge 0.35$.
*   **Sizing Anggaran:** Flat 10% anggaran agar aman.
*   **Exit:** Harga menyentuh batas atas Bollinger Bands 20.

### 3. Modul Downtrend (Bearish Climax Rebound)
*   **Kondisi Entry:** `EMA_FAST < EMA_SLOW` + RSI < 30 (Oversold) + Volume Surge $\ge 3.0x$ + Konfirmasi 2 candle hijau berturut-turut + lebar BB > 0.5% + VWAP diskon $\le -0.80%$.
*   **Sizing Anggaran:** Flat 30% anggaran.
*   **Exit:** **Micro Take Profit `+0.80%`** dari harga entry atau Death Cross 2 menit berturut-turut setelah lock-time 12 menit.

### 4. Modul Breakout (Spike Pump Catcher)
*   **Kondisi Entry:** Lonjakan perubahan harga 1m > 2.5 * StdDev50 + Volatilitas minimum StdDev $\ge 30.0$ + Tren 15m Bullish + Spike $\ge 0.5\%$ + RSI $\ge 65.0$.
*   **Filter Proteksi:**
    *   Blokir jika harga terlalu jauh di atas VWAP: `vwap_dist > 1.5%`.
    *   Anti Fake-Breakout: Jika volume rendah (`vol_surge <= 3.0`) dan harga terlalu dekat ke VWAP (`vwap_dist <= 0.8%`).
    *   Konfirmasi Tren: Jika spike > 0.6%, gap `EMA13 - EMA34` harus $\ge 0.05\%$.
    *   Order Book Imbalance (OBI) $\ge 0.40$.
*   **Sizing Anggaran:** Flat 25% anggaran.
*   **Exit:** EMA Death Cross di bawah VWAP terkonfirmasi 2 menit berturut-turut setelah lock-time 20 menit.

---

## 🛠️ Langkah Menjalankan Bot Live

### 1. Persiapan Kompilasi
Kompilasi bot menggunakan Rust compiler dengan bendera optimasi rilis penuh:
```bash
cargo build --release
```
Binary hasil kompilasi akan berada di `/target/release/rust_bot`.

### 2. Konfigurasi Database & Environment
Bot memerlukan file konfigurasi `.env` berisi database PostgreSQL.
Pastikan `DATABASE_URL` di-export sebelum menjalankan bot agar tidak terjadi kesalahan pemuatan file relatif:
```bash
export DATABASE_URL="postgresql://username:password@localhost:5432/database_name"
```

### 3. Eksekusi Background (24 Jam Non-stop)
Jalankan bot menggunakan `nohup` agar bot tetap aktif berjalan meskipun sesi SSH ditutup:
```bash
nohup ./target/release/rust_bot > bot.log 2>&1 &
```
Log keluaran aktivitas real-time bot dapat dipantau melalui database log `qps_market_metrics_log` atau file `bot.log`.
