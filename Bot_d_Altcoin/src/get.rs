use futures_util::StreamExt;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use serde_json::Value;
use crate::AppState;
use crate::FundingData;

/// Mendengarkan stream !markPrice@arr@1s dari Binance Futures (fstream)
/// Stream ini memberikan: mark price, index price (≈ spot), dan funding rate untuk SEMUA perp futures
pub async fn start_funding_rate_listener(state: AppState) {
    // fstream.binance.com = Binance Futures WebSocket
    let url = "wss://fstream.binance.com/ws/!markPrice@arr@1s";

    loop {
        match connect_async(url).await {
            Ok((mut ws_stream, _)) => {
                let _ = crate::add_log(
                    &state,
                    "✅ Terhubung ke fstream.binance.com !markPrice@arr@1s — Memantau Funding Rate semua USDT Perp Futures",
                )
                .await;

                while let Some(msg) = ws_stream.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Ok(arr) = serde_json::from_str::<Value>(&text) {
                                if let Some(items) = arr.as_array() {
                                    let now = chrono::Utc::now();
                                    let mut fd_map = state.funding_data.write().await;

                                    for item in items {
                                        // Hanya proses simbol yang berakhiran USDT
                                        let sym = match item.get("s").and_then(|v| v.as_str()) {
                                            Some(s) if s.ends_with("USDT") => s.to_string(),
                                            _ => continue,
                                        };

                                        let mark_price = item
                                            .get("p")
                                            .and_then(|v| v.as_str())
                                            .and_then(|s| s.parse::<f64>().ok())
                                            .unwrap_or(0.0);

                                        let index_price = item
                                            .get("i")
                                            .and_then(|v| v.as_str())
                                            .and_then(|s| s.parse::<f64>().ok())
                                            .unwrap_or(mark_price);

                                        let funding_rate = item
                                            .get("r")
                                            .and_then(|v| v.as_str())
                                            .and_then(|s| s.parse::<f64>().ok())
                                            .unwrap_or(0.0);

                                        let next_funding_time_ms = item
                                            .get("T")
                                            .and_then(|v| v.as_i64())
                                            .unwrap_or(0);

                                        if mark_price > 0.0 {
                                            fd_map.insert(
                                                sym.clone(),
                                                FundingData {
                                                    symbol: sym,
                                                    mark_price,
                                                    index_price,
                                                    funding_rate,
                                                    next_funding_time_ms,
                                                    last_update: now,
                                                },
                                            );
                                        }
                                    }

                                    // Update heartbeat
                                    drop(fd_map);
                                    *state.last_ws_activity.write().await = now;
                                }
                            }
                        }
                        Ok(Message::Close(_)) => {
                            let _ = crate::add_log(&state, "⚠️ Koneksi fstream ditutup server. Reconnect...").await;
                            break;
                        }
                        Err(e) => {
                            eprintln!("[fstream ERROR] {}", e);
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                eprintln!("[fstream] Gagal terhubung: {}. Retry dalam 5 detik...", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }

        // Jeda sebelum reconnect
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }
}

/// Ambil top funding rate candidates melalui REST API Binance Futures
/// Digunakan saat startup untuk inisialisasi sebelum WebSocket aktif
pub async fn fetch_initial_funding_rates() -> Result<Vec<FundingData>, Box<dyn std::error::Error + Send + Sync>> {
    let url = "https://fapi.binance.com/fapi/v1/premiumIndex";
    let resp = reqwest::get(url).await?.json::<Value>().await?;

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
                results.push(FundingData {
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

    // Urutkan dari funding rate tertinggi ke terendah
    results.sort_by(|a, b| b.funding_rate.partial_cmp(&a.funding_rate).unwrap_or(std::cmp::Ordering::Equal));
    Ok(results)
}
