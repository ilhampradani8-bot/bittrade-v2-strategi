# 📊 Funding Rate Arbitrage Backtest Engine (Mode Realistis)

Direktori ini berisi skrip pengujian historis (backtest) terisolasi untuk strategi **Funding Rate Arbitrage (Cash-and-Carry) Delta-Neutral** yang disimulasikan secara sangat realistis mendekati kondisi live trading nyata.

---

## 🚀 Cara Menjalankan Backtest

Jalankan skrip menggunakan Python 3 dengan parameter berikut:
```bash
python3 backtest_funding.py [starting_balance] [min_funding_rate] [max_positions] [position_size] [leverage]
```

### Parameter:
1. `starting_balance`: Modal awal simulasi (default: `200.0` USDT).
2. `min_funding_rate`: Threshold minimum funding rate untuk entry posisi (default: `0.0005` atau `0.05%` per 8 jam).
3. `max_positions`: Jumlah posisi maksimal yang boleh terbuka secara bersamaan (default: `3`).
4. `position_size`: Nilai posisi spot / futures nominal awal (default: `60.0` USDT per koin).
5. `leverage`: Leverage futures yang digunakan (default: `10.0`x).

### Contoh Eksekusi:
```bash
# Menjalankan simulasi modal $200, minimal FR 0.05%, maks 3 koin, size awal $60, leverage 10x
python3 backtest_funding.py 200 0.0005 3 60 10
```

---

## 🛠️ Aturan Simulasi & Logika Realistis

Strategi ini berjalan secara delta-neutral menggunakan data historis riil dari database (`fr_history`):

1. **Alokasi Modal (10x Leverage):**
   * Setiap posisi memerlukan alokasi saldo sebesar **1.1x nominal posisi** (`position_size * 1.1`).
   * 1.0x untuk pembelian Spot penuh (tanpa leverage).
   * 0.1x untuk margin Futures Short (10% jaminan untuk 10x leverage).
2. **Dynamic Compounding (Ukuran Posisi Tumbuh):**
   * Ukuran posisi tidak kaku. Setiap koin baru yang dibuka akan dihitung ulang secara dinamis mengikuti pertumbuhan ekuitas:
     $$\text{position\_size} = \frac{\text{total\_equity} \times 0.95}{\text{max\_positions} \times 1.1}$$
     Menyisakan 5% buffer tunai aman di dalam saldo.
3. **Biaya Transaksi (Fees):**
   * **Entry:** Spot taker fee 0.1% + Futures taker fee 0.04%.
   * **Exit Normal / Rotasi:** Spot taker fee 0.1% + Futures taker fee 0.04%.
   * **Likuidasi Paksa:** Spot taker fee 0.1% + Futures Liquidation fee 0.5% (tidak ada futures close fee).
4. **Pemicu Keluar (Exits):**
   * **FR Negatif:** Jika funding rate bernilai negatif selama 2 periode settlement berturut-turut.
   * **Emergency Exit:** Jika funding rate jatuh secara ekstrem di bawah `-0.05%` dalam 1 periode.
   * **Rotasi Oportunistik:** Bot hanya menutup posisi untuk rotasi jika posisi sudah berumur minimal 24 jam **DAN** ditemukan koin lain dengan FR minimal **2x lipat lebih tinggi**.
5. **Margin Call & Likuidasi Paksa:**
   * Jika harga futures mark price melonjak naik **$\ge 9.6\%$** di atas entry price ( short futures merugi hingga memakan jaminan 10% dikurangi maintenance margin Binance 0.4%), posisi akan **dilikuidasi paksa**.
   * Ketika terlikuidasi, jaminan futures disita ($0), biaya penalti likuidasi (0.5%) dikenakan, dan koin Spot langsung dijual otomatis pada harga pasar tertinggi saat itu untuk mengamankan sisa dana.
   * **Logika Delta-Neutral Benar**: Kerugian disitanya jaminan di futures diselamatkan oleh apresiasi kenaikan nilai koin Spot, sehingga total modal keseluruhan tetap aman (hanya rugi biaya fee/denda likuidasi).
6. **Cooldown 8 Jam:**
   * Setelah sebuah koin ditutup (baik secara normal maupun karena likuidasi), koin tersebut dilarang untuk dibeli kembali selama minimal 8 jam untuk menghindari re-entry di pucuk harga atau whipsaw beruntun.

