# BitTrade-v2: Bot Trading BTC Real-time (Rust & Axum)

Sistem bot trading Bitcoin (BTC) otomatis dan real-time yang ditulis dalam bahasa pemrograman Rust dengan arsitektur modular yang tangguh, aman, dan berkinerja tinggi. Dilengkapi dengan dashboard monitoring web interaktif yang responsif untuk mobile dan desktop.

---

## 🚀 Strategi Trading Sistem (Market Regime-Based)

Bot ini menggunakan pendekatan dinamis berdasarkan deteksi **Market Regime (Kondisi Pasar)** untuk memilih strategi terbaik secara otomatis demi memaksimalkan profitabilitas dan meminimalisir risiko.

### 1. Deteksi Kondisi Pasar (Market Regime Detection)
Setiap menit, bot akan menganalisis volatilitas harga BTC selama 20 menit terakhir menggunakan standar deviasi (StdDev):
*   **Volatilitas Persentase**: $\text{Volatility \%} = \left(\frac{\text{StdDev}}{\text{Harga Saat Ini}}\right) \times 100\%$
*   **Klasifikasi Pasar**:
    *   Jika Volatilitas < **0.075%**: Pasar diklasifikasikan sebagai **SIDEWAYS (Ranging)**.
    *   Jika Volatilitas $\ge$ **0.075%**: Pasar diklasifikasikan sebagai **TRENDING (Bullish/Bearish)**.

---

### 2. Strategi A: Mean Reversion (Bollinger Bands)
*Dijalankan saat kondisi pasar terdeteksi **SIDEWAYS**.*
Strategi ini berasumsi bahwa harga cenderung kembali ke nilai rata-ratanya setelah menyimpang jauh.
*   **Indikator**: Bollinger Bands (20-period SMA, 2x Standar Deviasi).
*   **Sinyal Beli (BUY)**: Ketika harga BTC menyentuh atau berada di bawah **Lower Band**.
*   **Sinyal Jual (SELL)**: Ketika harga BTC menyentuh atau berada di atas **Upper Band**.

---

### 3. Strategi B: Trend Following (SMA Crossover)
*Dijalankan saat kondisi pasar terdeteksi **TRENDING**.*
Strategi ini bertujuan untuk mengikuti arah pergerakan tren yang kuat.
*   **Indikator**: 5-period SMA (Fast) dan 15-period SMA (Slow).
*   **Sinyal Beli (Golden Cross - BULLISH)**: Ketika SMA-5 memotong ke atas SMA-15.
*   **Sinyal Jual (Death Cross - BEARISH)**: Ketika SMA-5 memotong ke bawah SMA-15.

---

### 4. Manajemen Risiko & Proteksi Modal
*   **Stop Loss (SL)**: Pembatasan kerugian maksimal sebesar **2.0%** dari harga beli awal.
*   **Take Profit (TP)**: Pengamanan keuntungan otomatis sebesar **3.0%** dari harga beli awal.
*   **Biaya Transaksi (Trading Fee)**: Simulasi biaya admin sebesar **0.1%** per transaksi (BUY dan SELL) dicatat secara transparan di database.
*   **Jeda Transaksi (Cool-down)**: Bot menerapkan jeda minimal **5 menit** untuk aksi berturut-turut yang sama untuk menghindari eksekusi berlebihan akibat volatilitas sesaat.

---

## 🛠️ Tech Stack & Arsitektur Kode

Sistem terbagi menjadi modul-modul terpisah (*Separation of Concerns*) untuk stabilitas jangka panjang:

*   **`main.rs`**: Mengatur siklus per-menit, sinkronisasi data Binance WebSocket feed, inisialisasi state, dan routing API Axum.
*   **`conclude.rs`**: Otak analis pasar yang menghitung indikator teknikal (SMA, Bollinger Bands, StdDev) dan mendeteksi tren pasar untuk mengeluarkan keputusan trading (Buy, Sell, Wait).
*   **`validate.rs`**: Melakukan pengecekan ketat sebelum eksekusi (kecukupan saldo USDT/BTC, aturan cool-down, dsb).
*   **`executor.rs`**: Melakukan transaksi simulasi, menghitung biaya fee admin, merekam P&L bersih, serta mencatat hasil ke tabel `bot_trading_history`.
*   **`corrector.rs`**: Menangani error sistem dan kegagalan logika trading, mencatat log kesalahan secara aman ke database `bot_corrections`.

### Teknologi Utama:
*   **Rust (Asynchronous)**: Logika bot berperforma tinggi menggunakan runtime `tokio`.
*   **Axum**: Web framework super cepat dan ringan untuk menyajikan REST API.
*   **PostgreSQL & SQLx**: Database penyimpanan transaksional yang aman dengan koneksi pool yang efisien.
*   **HTML/CSS/JS (Vanilla)**: Dashboard pemantau real-time yang ringan, mobile-responsive, dilengkapi diagram Chart.js dan indikator lampu LED status proses sistem.

---

## 🗄️ Skema Database (PostgreSQL)

Bot terhubung dengan 3 tabel utama di PostgreSQL:

1.  **`bot_trading_history`**:
    *   `id` (SERIAL PRIMARY KEY)
    *   `action` (VARCHAR - BUY / SELL)
    *   `price` (DOUBLE PRECISION - Harga BTC)
    *   `amount` (DOUBLE PRECISION - Jumlah BTC)
    *   `timestamp` (TIMESTAMP WITH TIME ZONE)
    *   `status` (VARCHAR - SUCCESS / FAILED)
    *   `notes` (TEXT - Catatan detail fee admin dan realized P&L nominal $)

2.  **`bot_corrections`**:
    *   `id` (SERIAL PRIMARY KEY)
    *   `error_type` (VARCHAR)
    *   `reason` (TEXT)
    *   `timestamp` (TIMESTAMP WITH TIME ZONE)

3.  **`bot_balance_history`**:
    *   `id` (SERIAL PRIMARY KEY)
    *   `simulated_balance` (DOUBLE PRECISION - Sisa USDT)
    *   `btc_balance` (DOUBLE PRECISION)
    *   `total_value` (DOUBLE PRECISION - Total Equity USDT)
    *   `timestamp` (TIMESTAMP WITH TIME ZONE)

---

## ⚙️ Cara Menjalankan & Deployment

### 1. Konfigurasi Environment (`.env`)
Buat file `.env` di root direktori dengan konfigurasi database PostgreSQL Anda:
```env
DATABASE_URL=postgres://bottrade_user:password@localhost:5432/bottrade_db
```

### 2. Jalankan Bot di Latar Belakang (Production/Detached Mode)
Agar bot tetap aktif berjalan 24/7 di VPS Anda walaupun terminal SSH atau koneksi Antigravity ditutup, jalankan dengan menggunakan perintah `nohup`:
```bash
# Pindah ke direktori rust_bot
cd rust_bot

# Jalankan bot secara independen di background
nohup ./target/debug/rust_bot > bot.log 2>&1 &
```
*   **Melihat Log Jalannya Bot**: `tail -f bot.log`
*   **Mematikan Bot**: `fuser -k 8087/tcp`

### 3. Akses Dashboard Real-time
Buka browser Anda dan akses alamat berikut:
```
http://<IP_VPS_ANDA>:8087/
```
Dashboard akan menampilkan grafik pergerakan harga real-time, grafik pertumbuhan modal, winrate, state tren pasar (Bullish, Bearish, Sideways) menggunakan indikator LED, dan tabel riwayat transaksi lengkap dengan nominal P&L dalam USD.
