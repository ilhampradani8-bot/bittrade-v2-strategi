use futures_util::StreamExt;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use serde_json::Value;
use crate::AppState;

pub async fn start_price_listener(state: AppState) {
    loop {
        let symbols = {
            let active = state.active_symbols.read().await;
            active.clone()
        };

        if symbols.is_empty() {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            continue;
        }

        let streams = symbols.iter()
            .map(|s| format!("{}@ticker", s.to_lowercase()))
            .collect::<Vec<_>>()
            .join("/");

        let url = format!("wss://stream.binance.com:9443/stream?streams={}", streams);

        match connect_async(&url).await {
            Ok((mut ws_stream, _)) => {
                let _ = crate::add_log(&state, &format!("Terhubung ke Binance WebSocket untuk koin: {:?}", symbols)).await;

                while let Some(msg) = ws_stream.next().await {
                    // Check if active symbols changed in State
                    let current_active = {
                        let active = state.active_symbols.read().await;
                        active.clone()
                    };
                    if current_active != symbols {
                        let _ = crate::add_log(&state, "Daftar koin aktif berubah. Memutus WebSocket untuk rekoneksi dinamis...").await;
                        let _ = ws_stream.close(None).await;
                        break; // will reconnect with new symbols
                    }

                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Ok(json) = serde_json::from_str::<Value>(&text) {
                                if let Some(stream) = json.get("stream").and_then(|s| s.as_str()) {
                                    let symbol_upper = stream.split('@').next().unwrap_or("").to_uppercase();
                                    if let Some(data) = json.get("data") {
                                        if let Some(price_str) = data.get("c").and_then(|c| c.as_str()) {
                                            if let Ok(price) = price_str.parse::<f64>() {
                                                {
                                                    let mut prices = state.current_prices.write().await;
                                                    prices.insert(symbol_upper.clone(), price);
                                                }

                                                // Update high water mark jika ada posisi aktif
                                                let layers = {
                                                    let layers_map = state.layers_filled.read().await;
                                                    layers_map.get(&symbol_upper).copied().unwrap_or(0)
                                                };
                                                if layers > 0 {
                                                    let mut hwms = state.cycle_high_water_marks.write().await;
                                                    let hwm = hwms.entry(symbol_upper.clone()).or_insert(0.0);
                                                    if price > *hwm {
                                                        *hwm = price;
                                                    }
                                                }

                                                *state.last_ws_activity.write().await = chrono::Utc::now();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(Message::Close(_)) => {
                            let _ = crate::add_log(&state, "Koneksi WebSocket ditutup oleh server").await;
                            break;
                        }
                        Err(e) => {
                            let _ = crate::add_log(&state, &format!("WebSocket Error: {}", e)).await;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("Gagal terhubung ke WebSocket: {}. Mencoba ulang dalam 5 detik...", e);
                eprintln!("{}", err_msg);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }
}

pub async fn sync_klines_for_symbol(db: &sqlx::PgPool, symbol: &str, limit: i32) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = format!("https://api.binance.com/api/v3/klines?symbol={}&interval=1m&limit={}", symbol, limit);
    let resp = reqwest::get(&url).await?.json::<Vec<Vec<serde_json::Value>>>().await?;

    for kline in resp {
        if kline.len() >= 6 {
            let open_time_ms = kline[0].as_i64().ok_or("Gagal parse open_time")?;
            let open_time = chrono::DateTime::from_timestamp_millis(open_time_ms).ok_or("Invalid timestamp")?;

            let open_price = kline[1].as_str().ok_or("Gagal parse open_price")?.parse::<f64>()?;
            let high_price = kline[2].as_str().ok_or("Gagal parse high_price")?.parse::<f64>()?;
            let low_price = kline[3].as_str().ok_or("Gagal parse low_price")?.parse::<f64>()?;
            let close_price = kline[4].as_str().ok_or("Gagal parse close_price")?.parse::<f64>()?;
            let volume = kline[5].as_str().ok_or("Gagal parse volume")?.parse::<f64>()?;

            sqlx::query(
                "INSERT INTO dca_klines (symbol, open_time, open_price, high_price, low_price, close_price, volume)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (symbol, open_time) DO UPDATE SET
                    open_price = EXCLUDED.open_price,
                    high_price = EXCLUDED.high_price,
                    low_price = EXCLUDED.low_price,
                    close_price = EXCLUDED.close_price,
                    volume = EXCLUDED.volume"
            )
            .bind(symbol)
            .bind(open_time)
            .bind(open_price)
            .bind(high_price)
            .bind(low_price)
            .bind(close_price)
            .bind(volume)
            .execute(db)
            .await?;
        }
    }
    Ok(())
}

pub async fn get_rest_price_for_symbol(symbol: &str) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!("https://api.binance.com/api/v3/ticker/price?symbol={}", symbol);
    let resp = reqwest::get(&url)
        .await?
        .json::<serde_json::Value>()
        .await?;
    let price_str = resp["price"].as_str().ok_or("Invalid price field")?;
    let price = price_str.parse::<f64>()?;
    Ok(price)
}

pub async fn fetch_top_volatile_symbols(db: &sqlx::PgPool, state_opt: Option<&AppState>) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let url = "https://api.binance.com/api/v3/ticker/24hr";
    let resp = reqwest::get(url).await?.json::<Vec<serde_json::Value>>().await?;

    let mut candidates = Vec::new();
    let now_ms = chrono::Utc::now().timestamp_millis();
    for ticker in resp {
        if let Some(symbol) = ticker.get("symbol").and_then(|s| s.as_str()) {
            if symbol.ends_with("USDT") && !symbol.contains("UP") && !symbol.contains("DOWN") && !symbol.contains("BUSD") {
                let close_time = ticker.get("closeTime").and_then(|t| t.as_i64()).unwrap_or(0);
                if now_ms - close_time > 300_000 {
                    continue; // Skip delisted or inactive coins
                }

                let vol_usd = ticker.get("quoteVolume").and_then(|v| v.as_str()).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                let high = ticker.get("highPrice").and_then(|v| v.as_str()).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                let low = ticker.get("lowPrice").and_then(|v| v.as_str()).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);

                if vol_usd > 1000000.0 && low > 0.0 { // Minimum $1M daily volume to ensure liquidity
                    let volatility = (high - low) / low * 100.0;
                    candidates.push((symbol.to_string(), vol_usd, volatility));
                }
            }
        }
    }

    // Sort by volume descending first to get top 100 volume coins
    candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    if candidates.len() > 100 {
        candidates.truncate(100);
    }

    // Now sort these top 100 by volatility descending
    candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    // Find currently active/held symbols from database
    let active_held: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT symbol FROM dca_active_positions"
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let mut selected = active_held.clone();

    // Fill the remaining spots up to 5 symbols from the top volatile list
    for (sym, _, _) in &candidates {
        if selected.len() >= 5 {
            break;
        }
        if !selected.contains(sym) {
            selected.push(sym.clone());
        }
    }

    // Fallback if empty
    if selected.is_empty() {
        selected.push("BTCUSDT".to_string());
    }

    // Update scanner_candidates in state if state_opt is Some
    if let Some(state) = state_opt {
        let mut api_candidates = Vec::new();
        for (sym, vol, volatility) in &candidates {
            let is_act = selected.contains(sym);
            let layers = {
                let layers_map = state.layers_filled.read().await;
                layers_map.get(sym).copied().unwrap_or(0)
            };
            api_candidates.push(crate::ScannerCandidate {
                symbol: sym.clone(),
                volume: *vol,
                volatility: *volatility,
                is_active: is_act,
                layers_filled: layers,
            });
        }
        let mut lock = state.scanner_candidates.write().await;
        *lock = api_candidates;
    }

    Ok(selected)
}
