use futures_util::StreamExt;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use serde_json::Value;
use crate::AppState;
use crate::corrector;

pub async fn start_price_listener(state: AppState) {
    let url = "wss://stream.binance.com:9443/ws/btcusdt@miniTicker";
    
    loop {
        match connect_async(url).await {
            Ok((mut ws_stream, _)) => {
                let _ = crate::add_log(&state, "Terhubung ke Binance WebSocket untuk harga real-time BTC/USDT").await;
                
                while let Some(msg) = ws_stream.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Ok(json) = serde_json::from_str::<Value>(&text) {
                                if let Some(price_str) = json["c"].as_str() {
                                    if let Ok(price) = price_str.parse::<f64>() {
                                        let mut price_lock = state.current_btc_price.write().await;
                                        *price_lock = price;
                                        *state.last_ws_activity.write().await = chrono::Utc::now();
                                    }
                                }
                            }
                        }
                        Ok(Message::Close(_)) => {
                            let _ = crate::add_log(&state, "Koneksi WebSocket ditutup oleh server").await;
                            break;
                        }
                        Err(e) => {
                            let err_msg = format!("WebSocket Error: {}", e);
                            let _ = crate::add_log(&state, &err_msg).await;
                            corrector::log_error(&state, "WEBSOCKET_ERROR", &err_msg).await;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("Gagal terhubung ke WebSocket: {}. Mencoba ulang dalam 5 detik...", e);
                eprintln!("{}", err_msg);
                corrector::log_error(&state, "WEBSOCKET_CONNECT_ERROR", &err_msg).await;
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }
}

// Fungsi untuk sync historical data (kline/candle 1 menit) dari Binance ke PostgreSQL
pub async fn sync_klines(db: &sqlx::PgPool, limit: i32) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = format!("https://api.binance.com/api/v3/klines?symbol=BTCUSDT&interval=1m&limit={}", limit);
    let resp = reqwest::get(&url).await?.json::<Vec<Vec<serde_json::Value>>>().await?;
    
    for kline in resp {
        if kline.len() >= 6 {
            let open_time_ms = kline[0].as_i64().ok_or("Gagal parse open_time")?;
            let open_time = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                chrono::NaiveDateTime::from_timestamp_millis(open_time_ms).ok_or("Invalid timestamp")?,
                chrono::Utc
            );
            
            let open_price = kline[1].as_str().ok_or("Gagal parse open_price")?.parse::<f64>()?;
            let high_price = kline[2].as_str().ok_or("Gagal parse high_price")?.parse::<f64>()?;
            let low_price = kline[3].as_str().ok_or("Gagal parse low_price")?.parse::<f64>()?;
            let close_price = kline[4].as_str().ok_or("Gagal parse close_price")?.parse::<f64>()?;
            let volume = kline[5].as_str().ok_or("Gagal parse volume")?.parse::<f64>()?;
            
            sqlx::query(
                "INSERT INTO btc_klines (open_time, open_price, high_price, low_price, close_price, volume)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (open_time) DO UPDATE SET
                    open_price = EXCLUDED.open_price,
                    high_price = EXCLUDED.high_price,
                    low_price = EXCLUDED.low_price,
                    close_price = EXCLUDED.close_price,
                    volume = EXCLUDED.volume"
            )
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

pub async fn get_rest_price() -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    let resp = reqwest::get("https://api.binance.com/api/v3/ticker/price?symbol=BTCUSDT")
        .await?
        .json::<serde_json::Value>()
        .await?;
    let price_str = resp["price"].as_str().ok_or("Invalid price field")?;
    let price = price_str.parse::<f64>()?;
    Ok(price)
}
