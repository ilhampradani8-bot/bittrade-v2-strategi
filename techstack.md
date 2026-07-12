# 🛠️ BitTrade-v2 Technical Stack & Architecture

Dokumentasi ini menjelaskan tumpukan teknologi (*technology stack*) nyata yang digunakan di server produksi untuk mendukung sistem perdagangan multi-bot **BitTrade-v2**.

---

## 🌐 1. Frontend (User & Admin Dashboard)

Frontend telah sepenuhnya dimigrasi dari template lama ke arsitektur web modern yang terintegrasi dan berkinerja tinggi.

| Teknologi | Fungsi / Peran | Lokasi Kode |
| :--- | :--- | :--- |
| **Astro Framework** | Static Site Generation (SSG) untuk menghasilkan halaman HTML statis yang super ringan dan cepat. | `web-ui/` |
| **Vanilla CSS** | Desain bertema *academic-paper-inspired* ("Warm Paper") yang minimalis, premium, dan responsif tanpa menggunakan framework CSS berat. | `web-ui/src/styles/global.css` |
| **Chart.js** | Visualisasi kurva pertumbuhan ekuitas gabungan (*unified equity growth*) dan diagram statistik Z-Score real-time. | Sisi Klien (`web-ui/public/js/`, `statarb/js/`) |
| **Vanilla JavaScript (ES Modules)** | Logika polling asinkron (pemisahan interval 3 detik untuk data real-time, dan 30 detik untuk visualisasi grafik berat) serta manajemen status tombol burger/navigasi. | `web-ui/public/js/dashboard_main.js`, `public/dca/js/dashboard.js`, dll. |

---

## ⚙️ 2. Backend & Trading Engine (Rust & Axum)

Arsitektur backend berjalan secara asinkron menggunakan bahasa pemrograman Rust untuk memastikan latensi minimal, keandalan multi-threading, dan keamanan tipe data.

| Modul Bot | Port | Peran Engine / Strategi | Database Prefix | Direktori |
| :--- | :---: | :--- | :---: | :--- |
| **Bot A** (Trend/Scalper) | `8087` | Trend Crossover (EMA-13/34) & Mean Reversion (Bollinger Bands 50-Period) dengan konfirmasi volume (VWAP). | `bot_` | `rust_bot/` |
| **Bot B** (SmartDCA) | `8088` | Akumulasi bertahap (3-layer DCA) dengan filter zona diskon RSI < 30 dan penundaan Panic Dump. | `dca_` | `smartdca/` |
| **Bot C** (OKX Spot) | `8091` | Mesin trading spot mandiri khusus API Publik OKX. | `okx_` | `OKX_trading/` |
| **Bot D** (Altcoin Engine)| `8092` | Kloning Bot A yang dioptimalkan untuk multi-koin Futures/Spot Binance. | `alt_` | `Bot_d_Altcoin/` |
| **Bot E** (statARB Engine)| `8093` | Mesin Statistical Arbitrage (ETH/BTC co-integration spread) dengan dual-leg execution. | `starb_` | `statARB/` |

---

## 🗄️ 3. Database Layer (PostgreSQL)

Setiap bot terhubung ke kluster database PostgreSQL terpusat yang sama menggunakan koneksi `PgPool` asinkron, namun diisolasi melalui penamaan skema tabel (*table prefix*):

*   **Penyimpanan K-Line Bersama (`btc_klines`)**: Digunakan secara bersama-sama oleh seluruh bot Binance untuk meminimalkan beban pemanggilan API kline eksternal.
*   **Isolasi Tabel**:
    *   `bot_*` (Bot A)
    *   `dca_*` (Bot B)
    *   `okx_*` (Bot C)
    *   `alt_*` (Bot D)
    *   `starb_*` (Bot E)

---

## 🚀 4. Gateway & Reverse Proxy (Apache HTTP Server)

Web server Apache berfungsi sebagai gerbang utama (Gateway) yang menangani protokol keamanan SSL (Let's Encrypt), menyajikan frontend statis Astro, dan mengalihkan lalu lintas API ke port internal masing-masing bot:

*   **Penyajian Dashboard Utama (Port 443 / HTTPS)**: Dilayani langsung secara statis dari direktori `/var/www/tradingsafe/` (hasil build Astro).
*   **Konfigurasi Proxy (`/etc/apache2/sites-available/tradingsafe-le-ssl.conf`)**:
    *   `/` $\rightarrow$ Menyajikan static build `/var/www/tradingsafe` dan meneruskan `/api/` ke Port `8087` (Bot A).
    *   `/dca/` $\rightarrow$ Meneruskan data statis & API `/dca/api/` ke Port `8088` (Bot B).
    *   `/okx/` $\rightarrow$ Meneruskan data statis & API `/okx/` ke Port `8091` (Bot C).
    *   `/arbitrage/` $\rightarrow$ Meneruskan data statis & API `/arbitrage/api/` ke Port `8092` (Bot D).
    *   `/statarb/` $\rightarrow$ Meneruskan data statis & API `/statarb/api/` ke Port `8093` (Bot E).

---
*Terakhir Diperbarui: Juli 2026*
