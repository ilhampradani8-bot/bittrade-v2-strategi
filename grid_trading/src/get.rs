use futures_util::StreamExt;
use tokio_tungstenite::connect_async;
use serde_json::Value;
use crate::AppState;
use std::sync::OnceLock;

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn get_http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent("bittrade-grid-bot/2.0")
            .build()
            .expect("Gagal membuat HTTP client")
    })
}

pub async fn start_price_listener(state: AppState) {
    let mut streams = Vec::new();
    for sym in crate::SYMBOLS {
        let lower = sym.to_lowercase();
        streams.push(format!("{}@ticker", lower));
        streams.push(format!("{}@bookTicker", lower));
    }
    let url = format!("wss://stream.binance.com:9443/stream?streams={}", streams.join("/"));
    
    let mut retry_delay = 2u64;

    loop {
        println!("[WS] Mencoba koneksi ke Binance WebSocket...");
        match connect_async(&url).await {
            Ok((mut ws_stream, _)) => {
                let _ = crate::add_log(&state, "✅ Terhubung ke Binance WebSocket (combined stream)").await;
                retry_delay = 2;

                while let Some(msg) = ws_stream.next().await {
                    match msg {
                        Ok(tokio_tungstenite::tungstenite::protocol::Message::Text(text)) => {
                            if let Ok(json) = serde_json::from_str::<Value>(&text) {
                                process_ticker_message(&json, &state).await;
                            }
                        }
                        Ok(tokio_tungstenite::tungstenite::protocol::Message::Ping(payload)) => {
                            *state.last_ws_activity.write().await = chrono::Utc::now();
                            let _ = payload;
                        }
                        Ok(tokio_tungstenite::tungstenite::protocol::Message::Close(frame)) => {
                            let reason = frame.map(|f| f.reason.to_string()).unwrap_or_else(|| "unknown".into());
                            let _ = crate::add_log(&state, &format!("[WS] Koneksi ditutup oleh server: {}", reason)).await;
                            break;
                        }
                        Err(e) => {
                            let _ = crate::add_log(&state, &format!("[WS] Error: {}. Reconnecting...", e)).await;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                eprintln!("[WS] Gagal terhubung: {}. Retry dalam {}s...", e, retry_delay);
                tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay)).await;
                retry_delay = (retry_delay * 2).min(60);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }
}

async fn process_ticker_message(json: &Value, state: &AppState) {
    if let (Some(stream), Some(data)) = (json.get("stream"), json.get("data")) {
        if let Some(stream_name) = stream.as_str() {
            let parts: Vec<&str> = stream_name.split('@').collect();
            if parts.len() != 2 { return; }
            let sym_lower = parts[0];
            let ev_type = parts[1];
            let symbol = sym_lower.to_uppercase();
            
            if !crate::SYMBOLS.contains(&symbol.as_str()) {
                return;
            }

            if ev_type == "bookTicker" {
                if let (Some(b_str), Some(a_str)) = (data.get("b").and_then(|x| x.as_str()), data.get("a").and_then(|x| x.as_str())) {
                    if let (Ok(b_qty), Ok(a_qty)) = (b_str.parse::<f64>(), a_str.parse::<f64>()) {
                        let total = b_qty + a_qty;
                        if total > 0.0 {
                            let mut obis = state.obis.write().await;
                            obis.insert(symbol, b_qty / total);
                        }
                    }
                }
                return;
            }

            if ev_type == "ticker" {
                if let Some(price_str) = data.get("c").and_then(|p| p.as_str()) {
                    if let Ok(price) = price_str.parse::<f64>() {
                        if price <= 0.0 { return; }

                        let mut prices = state.prices.write().await;
                        prices.insert(symbol.clone(), price);
                        drop(prices);

                        let hwm = {
                            let hwms = state.high_water_marks.read().await;
                            *hwms.get(&symbol).unwrap_or(&0.0)
                        };

                        if price > hwm && hwm > 0.0 {
                            let mut hwms = state.high_water_marks.write().await;
                            hwms.insert(symbol.clone(), price);
                            
                            let db_clone = state.db.clone();
                            let symbol_clone = symbol.clone();
                            tokio::spawn(async move {
                                let _ = sqlx::query(
                                    "UPDATE grid_active_positions SET high_water_mark = $1 WHERE symbol = $2 AND high_water_mark < $1"
                                )
                                .bind(price)
                                .bind(symbol_clone)
                                .execute(&db_clone)
                                .await;
                            });
                        }
                        
                        *state.last_ws_activity.write().await = chrono::Utc::now();
                    }
                }
            }
        }
    }
}

pub async fn sync_klines(
    state: &AppState,
    limit: u32,
    symbol: &str
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = format!(
        "https://api.binance.com/api/v3/klines?symbol={}&interval=1m&limit={}",
        symbol,
        limit.min(1000)
    );

    let client = get_http_client();
    let resp = client.get(&url).send().await?.json::<serde_json::Value>().await?;

    let arr = resp.as_array().ok_or("Response bukan array")?;

    for kline in arr {
        let kline_arr = kline.as_array().ok_or("Kline bukan array")?;
        if kline_arr.len() < 6 { continue; }

        let ts_ms = kline_arr[0].as_i64().ok_or("Invalid timestamp")?;
        let open_time = chrono::DateTime::from_timestamp_millis(ts_ms).ok_or("Timestamp out of range")?;

        macro_rules! parse_f64 {
            ($idx:expr, $name:literal) => {
                kline_arr[$idx].as_str().ok_or(concat!("Gagal parse field ", $name))?.parse::<f64>()?
            };
        }

        let open_price  = parse_f64!(1, "open");
        let high_price  = parse_f64!(2, "high");
        let low_price   = parse_f64!(3, "low");
        let close_price = parse_f64!(4, "close");
        let volume      = parse_f64!(5, "volume");

        if close_price <= 0.0 || volume < 0.0 { continue; }

        sqlx::query(
            "INSERT INTO grid_klines (symbol, open_time, open_price, high_price, low_price, close_price, volume)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (symbol, open_time) DO UPDATE SET
                open_price  = EXCLUDED.open_price,
                high_price  = EXCLUDED.high_price,
                low_price   = EXCLUDED.low_price,
                close_price = EXCLUDED.close_price,
                volume      = EXCLUDED.volume"
        )
        .bind(symbol)
        .bind(open_time)
        .bind(open_price)
        .bind(high_price)
        .bind(low_price)
        .bind(close_price)
        .bind(volume)
        .execute(&state.db)
        .await?;
    }

    let _ = sqlx::query("DELETE FROM grid_klines WHERE symbol = $1 AND open_time < NOW() - INTERVAL '7 days'")
        .bind(symbol)
        .execute(&state.db)
        .await;

    Ok(())
}
