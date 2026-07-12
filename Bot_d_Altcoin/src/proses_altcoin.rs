use crate::AppState;
use crate::{conclude, validate, executor, trader_cek, head_allusdt};
use tokio::time::{sleep, Duration};

/// Main loop engine untuk Funding Rate Arbitrage
/// Berjalan setiap 60 detik, scan kandidat & monitor posisi aktif
pub async fn start_arb_engine(state: AppState) {
    crate::add_log(&state, "🚀 Funding Rate Arbitrage Engine aktif. Memulai scan pasar...").await;

    // Beri waktu 10 detik agar WebSocket mendapat data awal
    sleep(Duration::from_secs(10)).await;

    loop {
        *state.last_engine_activity.write().await = chrono::Utc::now();

        // Proactively fetch funding rates via REST API if the WebSocket map is empty
        let is_empty = { state.funding_data.read().await.is_empty() };
        if is_empty {
            println!("[ENGINE] 🔄 Peta WebSocket kosong. Mengambil data dari REST API fapi...");
            match fetch_all_funding_rates_via_rest().await {
                Ok(rates) => {
                    let mut fd_map = state.funding_data.write().await;
                    let now = chrono::Utc::now();
                    let count = rates.len();
                    for fd in rates {
                        fd_map.insert(fd.symbol.clone(), fd);
                    }
                    println!("[ENGINE] ✅ Sukses load {} simbol dari REST API", count);
                }
                Err(e) => {
                    eprintln!("[ENGINE] ❌ Gagal load dari REST API: {}", e);
                }
            }
        }

        // ================================================================
        // FASE 1: Monitor & update posisi yang sudah terbuka
        // ================================================================
        let open_symbols: Vec<String> = {
            let positions = state.arb_positions.read().await;
            positions.keys().cloned().collect()
        };

        for symbol in &open_symbols {
            // Ambil funding data terkini untuk simbol ini
            let fd_opt = {
                let fd_map = state.funding_data.read().await;
                fd_map.get(symbol.as_str()).cloned()
            };

            if let Some(fd) = fd_opt {
                // Update harga terkini di in-memory position
                {
                    let mut positions = state.arb_positions.write().await;
                    if let Some(pos) = positions.get_mut(symbol.as_str()) {
                        pos.current_mark_price = fd.mark_price;
                        pos.current_spot_price = fd.index_price;
                        pos.current_funding_rate = fd.funding_rate;
                        pos.annualized_yield = fd.funding_rate * 3.0 * 365.0 * 100.0;
                    }
                }

                let (should_collect, should_close, close_reason, _pos_init_fr, _pos_consec_neg) = {
                    let positions = state.arb_positions.read().await;
                    if let Some(pos) = positions.get(symbol.as_str()) {
                        let sc = conclude::should_collect_funding(pos);
                        let (close, reason) = match conclude::should_close_arb(pos, fd.funding_rate) {
                            Some(r) => (true, r),
                            None => (false, String::new()),
                        };
                        (sc, close, reason, pos.initial_funding_rate, pos.consecutive_negative_fr)
                    } else {
                        continue;
                    }
                };

                // A. Cek apakah perlu tutup posisi lebih dulu
                if should_close {
                    // Cek anti-pattern sebelum tutup
                    let pos_clone = {
                        let positions = state.arb_positions.read().await;
                        positions.get(symbol.as_str()).cloned()
                    };

                    if let Some(ref pos) = pos_clone {
                        if let Some(warn) = trader_cek::check_arb_exit(pos, fd.funding_rate, &close_reason) {
                            if warn.is_warning && !close_reason.contains("[Emergency]") && !close_reason.contains("[FR Negatif]") {
                                // Catat peringatan tapi tetap proses
                                crate::corrector::log_error(&state, &warn.pattern_type, &warn.diagnostic_msg).await;
                            }
                        }
                    }

                    if validate::validate_close_arb(symbol, &state).await {
                        match executor::close_arb_position(symbol, &close_reason, Some(&fd), &state).await {
                            Ok(pnl) => {
                                crate::add_log(&state, &format!(
                                    "✅ Posisi {} ditutup. Net P&L: ${:+.4}", symbol, pnl
                                )).await;
                            }
                            Err(e) => {
                                crate::corrector::log_error(&state, "CLOSE_ARB_ERROR",
                                    &format!("Gagal tutup {}: {}", symbol, e)).await;
                            }
                        }
                        continue; // Lanjut ke simbol berikutnya
                    }
                }

                // B. Cek apakah perlu kumpulkan funding payment
                if should_collect {
                    match executor::collect_funding_payment(symbol, fd.funding_rate, &state).await {
                        Ok(payment) => {
                            if payment < 0.0 {
                                // FR negatif — increment counter (sudah dihandle di executor)
                                // Jika sudah 2 kali berturut, periode berikutnya akan trigger exit
                            }
                        }
                        Err(e) => {
                            crate::corrector::log_error(&state, "COLLECT_FUNDING_ERROR",
                                &format!("Gagal kumpulkan funding {}: {}", symbol, e)).await;
                        }
                    }
                }
            } else {
                // Data funding tidak tersedia untuk simbol ini (stale/disconnected)
                crate::add_log(&state, &format!(
                    "⚠️ Data funding tidak tersedia untuk {}. Posisi tetap open.", symbol
                )).await;
            }
        }

        // ================================================================
        // FASE 2: Scan kandidat baru untuk dibuka posisi arb
        // ================================================================
        let candidates = {
            let fd_map = state.funding_data.read().await;
            let positions = state.arb_positions.read().await;
            head_allusdt::get_funding_candidates(
                &fd_map,
                &positions,
                state.min_funding_rate,
                state.max_positions,
            )
        };

        let position_size = state.position_size_usdt;

        if candidates.is_empty() {
            let open_count = state.arb_positions.read().await.len();
            let fd_count = state.funding_data.read().await.len();
            crate::add_log(&state, &format!(
                "📊 Scan selesai. {} simbol dipantau | {} posisi aktif | Tidak ada kandidat baru memenuhi threshold",
                fd_count, open_count
            )).await;
        }

        for candidate in candidates {
            let fd = &candidate.fd;

            // Cek anti-pattern sebelum entry
            let fr_history = fetch_fr_history(&fd.symbol).await;
            if let Some(warn) = trader_cek::check_arb_entry(fd, &fr_history) {
                crate::add_log(&state, &warn.diagnostic_msg).await;
                crate::corrector::log_error(&state, &warn.pattern_type, &warn.diagnostic_msg).await;
                // Jika basis terlalu lebar, skip. Jika hanya FR spike, tetap log tapi masih entry
                if warn.pattern_type == "WIDE_BASIS_RISK" {
                    continue;
                }
            }

            // Validasi kecukupan balance & kondisi lain
            if !validate::validate_open_arb(fd, position_size, &state).await {
                continue;
            }

            // Buka posisi
            match executor::open_arb_position(fd, position_size, &state).await {
                Ok(db_id) => {
                    crate::add_log(&state, &format!(
                        "🟢 Posisi Arb #{} dibuka untuk {} | FR: {:.4}% | APR: {:.2}%",
                        db_id, fd.symbol, fd.funding_rate * 100.0, candidate.annualized_yield
                    )).await;
                }
                Err(e) => {
                    crate::corrector::log_error(&state, "OPEN_ARB_ERROR",
                        &format!("Gagal buka arb {}: {}", fd.symbol, e)).await;
                }
            }
        }

        // ================================================================
        // FASE 3: Log ringkasan status setiap siklus
        // ================================================================
        {
            let positions = state.arb_positions.read().await;
            let total_collected = *state.total_funding_collected.read().await;
            let bal = *state.simulated_balance.read().await;
            let deployed: f64 = positions.values().map(|p| p.position_size_usdt).sum();

            // Hitung rata-rata APR dari semua posisi aktif
            let avg_apr = if positions.is_empty() {
                0.0
            } else {
                positions.values().map(|p| p.annualized_yield).sum::<f64>() / positions.len() as f64
            };

            crate::add_log(&state, &format!(
                "📈 Status: Saldo ${:.2} | Deployed ${:.0} | {} Posisi Aktif | Total Funding: ${:.4} | Avg APR: {:.2}%",
                bal, deployed, positions.len(), total_collected, avg_apr
            )).await;
        }

        // Simpan snapshot balance ke database
        save_balance_snapshot(&state).await;

        // Tunggu 60 detik sebelum siklus berikutnya
        sleep(Duration::from_secs(60)).await;
    }
}

/// Ambil riwayat funding rate terakhir dari Binance Futures REST API
/// Digunakan oleh anti-pattern checker untuk deteksi FR spike
async fn fetch_fr_history(symbol: &str) -> Vec<f64> {
    let url = format!(
        "https://fapi.binance.com/fapi/v1/fundingRate?symbol={}&limit=8",
        symbol
    );
    match reqwest::get(&url).await {
        Ok(resp) => {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                if let Some(arr) = data.as_array() {
                    return arr
                        .iter()
                        .filter_map(|item| {
                            item.get("fundingRate")
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse::<f64>().ok())
                        })
                        .collect();
                }
            }
        }
        Err(e) => {
            eprintln!("[fetch_fr_history] Error untuk {}: {}", symbol, e);
        }
    }
    Vec::new()
}

/// Simpan snapshot saldo dan ekuitas ke alt_balance_history
async fn save_balance_snapshot(state: &AppState) {
    let bal = *state.simulated_balance.read().await;
    let deployed: f64 = {
        let positions = state.arb_positions.read().await;
        positions.values().map(|p| p.position_size_usdt).sum()
    };
    let _total_funding = *state.total_funding_collected.read().await;
    let total_equity = bal + deployed;

    let _ = sqlx::query(
        "INSERT INTO alt_balance_history
            (simulated_balance, btc_balance, btc_value, total_value)
         VALUES ($1, $2, $3, $4)"
    )
    .bind(bal)
    .bind(0.0_f64)          // btc_balance: tidak relevan, diisi 0
    .bind(deployed)          // btc_value: gunakan untuk "deployed capital"
    .bind(total_equity)
    .execute(&state.db)
    .await;
}

/// Helper untuk mengambil semua funding rate terbaru dari Binance Futures REST API
async fn fetch_all_funding_rates_via_rest() -> Result<Vec<crate::FundingData>, Box<dyn std::error::Error + Send + Sync>> {
    let url = "https://fapi.binance.com/fapi/v1/premiumIndex";
    let resp = reqwest::get(url).await?.json::<serde_json::Value>().await?;

    let mut results = Vec::new();
    let now = chrono::Utc::now();

    if let Some(arr) = resp.as_array() {
        for item in arr {
            let sym = match item.get("symbol").and_then(|v| v.as_str()) {
                Some(s) if s.ends_with("USDT") => s.to_string(),
                _ => continue,
            };

            let mark_price = item
                .get("markPrice")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);

            let index_price = item
                .get("indexPrice")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(mark_price);

            let funding_rate = item
                .get("lastFundingRate")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);

            let next_funding_time_ms = item
                .get("nextFundingTime")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            if mark_price > 0.0 {
                results.push(crate::FundingData {
                    symbol: sym,
                    mark_price,
                    index_price,
                    funding_rate,
                    next_funding_time_ms,
                    last_update: now,
                });
            }
        }
    }
    Ok(results)
}

