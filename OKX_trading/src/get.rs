use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use serde_json::Value;
use crate::AppState;

pub async fn start_price_listener(state: AppState) {
    let url = "wss://ws.okx.com:8443/ws/v5/public";
    
    loop {
        match connect_async(url).await {
            Ok((mut ws_stream, _)) => {
                let _ = crate::add_log(&state, "Terhubung ke OKX WebSocket untuk harga real-time").await;
                
                // Kirim request subscribe
                let sub_msg = serde_json::json!({
                    "op": "subscribe",
                    "args": [
                        {"channel": "tickers", "instId": "BTC-USDT"},
                        {"channel": "tickers", "instId": "ETH-USDT"},
                        {"channel": "tickers", "instId": "BNB-USDT"},
                        {"channel": "tickers", "instId": "SOL-USDT"},
                        {"channel": "tickers", "instId": "XRP-USDT"}
                    ]
                });
                
                if let Err(e) = ws_stream.send(Message::Text(sub_msg.to_string())).await {
                    eprintln!("Gagal mengirim subscribe request ke OKX: {}", e);
                    continue;
                }
                
                while let Some(msg) = ws_stream.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Ok(json) = serde_json::from_str::<Value>(&text) {
                                if let (Some(arg), Some(data_arr)) = (json.get("arg"), json.get("data")) {
                                    if let Some(inst_id) = arg.get("instId").and_then(|id| id.as_str()) {
                                        if let Some(data) = data_arr.get(0) {
                                            if let Some(price_str) = data.get("last").and_then(|p| p.as_str()) {
                                                if let Ok(price) = price_str.parse::<f64>() {
                                                    if inst_id == "BTC-USDT" {
                                                        *state.current_btc_price.write().await = price;
                                                        let mut hwm = state.high_water_mark.write().await;
                                                        if price > *hwm && *hwm > 0.0 {
                                                            *hwm = price;
                                                            let db_clone = state.db.clone();
                                                            tokio::spawn(async move {
                                                                let _ = sqlx::query("UPDATE okx_active_positions SET high_water_mark = $1 WHERE high_water_mark < $1").bind(price).execute(&db_clone).await;
                                                            });
                                                        }
                                                    } else if inst_id == "ETH-USDT" {
                                                        *state.current_eth_price.write().await = price;
                                                    } else if inst_id == "BNB-USDT" {
                                                        *state.current_bnb_price.write().await = price;
                                                    } else if inst_id == "SOL-USDT" {
                                                        *state.current_sol_price.write().await = price;
                                                    } else if inst_id == "XRP-USDT" {
                                                        *state.current_xrp_price.write().await = price;
                                                    }
                                                    *state.last_ws_activity.write().await = chrono::Utc::now();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(Message::Close(_)) => {
                            let _ = crate::add_log(&state, "Koneksi OKX WebSocket ditutup oleh server").await;
                            break;
                        }
                        Err(e) => {
                            let err_msg = format!("OKX WebSocket Error: {}", e);
                            let _ = crate::add_log(&state, &err_msg).await;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("Gagal terhubung ke OKX WebSocket: {}. Mencoba ulang dalam 5 detik...", e);
                eprintln!("{}", err_msg);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }
}

// Fungsi untuk sync historical data (kline/candle 1 menit) dari OKX ke PostgreSQL database
pub async fn sync_klines(state: &AppState, limit: i32) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = format!("https://www.okx.com/api/v5/market/candles?instId=BTC-USDT&bar=1m&limit={}", limit);
    
    let client = reqwest::Client::new();
    let resp = client.get(&url)
        .header("User-Agent", "okx-trading-bot")
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
        
    if resp["code"] != "0" {
        return Err(format!("OKX API Error: {}", resp["msg"]).into());
    }
    
    if let Some(arr) = resp["data"].as_array() {
        for kline in arr {
            if let Some(kline_arr) = kline.as_array() {
                if kline_arr.len() >= 6 {
                    let ts_str = kline_arr[0].as_str().ok_or("Gagal parse timestamp")?;
                    let ts_ms = ts_str.parse::<i64>()?;
                    let open_time = chrono::DateTime::from_timestamp_millis(ts_ms).ok_or("Invalid timestamp")?;
                    
                    let open_price = kline_arr[1].as_str().ok_or("Gagal parse open")?.parse::<f64>()?;
                    let high_price = kline_arr[2].as_str().ok_or("Gagal parse high")?.parse::<f64>()?;
                    let low_price = kline_arr[3].as_str().ok_or("Gagal parse low")?.parse::<f64>()?;
                    let close_price = kline_arr[4].as_str().ok_or("Gagal parse close")?.parse::<f64>()?;
                    let volume = kline_arr[5].as_str().ok_or("Gagal parse volume")?.parse::<f64>()?;
                    
                    sqlx::query(
                        "INSERT INTO okx_klines (open_time, open_price, high_price, low_price, close_price, volume)
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
                    .execute(&state.db)
                    .await?;
                }
            }
        }
    }
    
    Ok(())
}

pub async fn get_rest_price() -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    let url = "https://www.okx.com/api/v5/market/ticker?instId=BTC-USDT";
    let client = reqwest::Client::new();
    let resp = client.get(url)
        .header("User-Agent", "okx-trading-bot")
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
        
    if resp["code"] != "0" {
        return Err(format!("OKX REST API Error: {}", resp["msg"]).into());
    }
    
    let price_str = resp["data"][0]["last"].as_str().ok_or("Invalid price field")?;
    let price = price_str.parse::<f64>()?;
    Ok(price)
}
