// INI ADALAH FILE get.rs
use futures_util::StreamExt;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use serde_json::Value;
use crate::AppState;
use crate::corrector;

async fn handle_ws_payload(state: &AppState, text: &str) {
    if let Ok(json) = serde_json::from_str::<Value>(text) {
        if let Some(arr) = json.as_array() {
            // STEP 1: Parse semua data dari JSON tanpa menahan lock apapun
            struct TickerUpdate {
                symbol: String,
                price: f64,
                change_24h: f64,
                quote_volume: f64,
                daily_volatility: f64,
                volatility_category: String,
            }

            let mut updates: Vec<TickerUpdate> = Vec::with_capacity(arr.len());

            for item in arr {
                if let Some(symbol_str) = item.get("s").and_then(|s| s.as_str()) {
                    if !symbol_str.ends_with("USDT") {
                        continue;
                    }
                    let symbol = symbol_str.to_string();
                    let price = item.get("c")
                        .and_then(|c| c.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0);
                    let open_price = item.get("o")
                        .and_then(|o| o.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0);
                    let high = item.get("h")
                        .and_then(|h| h.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0);
                    let low = item.get("l")
                        .and_then(|l| l.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0);
                    let quote_volume = item.get("q")
                        .and_then(|q| q.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0);

                    let change_24h = if open_price > 0.0 {
                        ((price - open_price) / open_price) * 100.0
                    } else {
                        0.0
                    };

                    let vol = if low > 0.0 {
                        ((high - low) / low) * 100.0
                    } else {
                        0.0
                    };

                    let category = if vol >= 12.0 {
                        "EXTREME".to_string()
                    } else if vol >= 7.0 {
                        "HYPER".to_string()
                    } else if vol >= 3.0 {
                        "HIGH".to_string()
                    } else {
                        "LOW".to_string()
                    };

                    updates.push(TickerUpdate {
                        symbol,
                        price,
                        change_24h,
                        quote_volume,
                        daily_volatility: vol,
                        volatility_category: category,
                    });
                }
            }

            // STEP 2: Acquire write lock SEKALI, lakukan batch update, lalu SEGERA lepas
            {
                let mut map = state.coin_states.write().await;
                for u in &updates {
                    map.entry(u.symbol.clone())
                        .and_modify(|c| {
                            c.price = u.price;
                            c.change_24h = u.change_24h;
                            c.volatility_category = u.volatility_category.clone();
                            c.quote_volume = u.quote_volume;
                            c.daily_volatility = u.daily_volatility;
                        })
                        .or_insert(crate::CoinState {
                            symbol: u.symbol.clone(),
                            price: u.price,
                            change_24h: u.change_24h,
                            order_book_imbalance: 0.5,
                            market_regime: "SIDEWAYS".to_string(),
                            volatility_category: u.volatility_category.clone(),
                            trend_status: "SIDEWAYS".to_string(),
                            quote_volume: u.quote_volume,
                            daily_volatility: u.daily_volatility,
                        });
                }
            } // write lock dilepas di sini

            // STEP 3: Update harga global per-koin SETELAH coin_states lock dilepas
            for u in &updates {
                match u.symbol.as_str() {
                    "BTCUSDT" => *state.current_btc_price.write().await = u.price,
                    "ETHUSDT" => *state.current_eth_price.write().await = u.price,
                    "BNBUSDT" => *state.current_bnb_price.write().await = u.price,
                    "SOLUSDT" => *state.current_sol_price.write().await = u.price,
                    "XRPUSDT" => *state.current_xrp_price.write().await = u.price,
                    _ => {}
                }
            }

            *state.last_ws_activity.write().await = chrono::Utc::now();
        }
    }
}

pub async fn start_price_listener(state: AppState) {
    let url = "wss://stream.binance.com:9443/ws/!miniTicker@arr";
    
    // Throttle: hanya update coin_states 1x per 60 detik
    let mut last_update = tokio::time::Instant::now()
        .checked_sub(tokio::time::Duration::from_secs(61))
        .unwrap_or(tokio::time::Instant::now());

    loop {
        match connect_async(url).await {
            Ok((mut ws_stream, _)) => {
                let _ = crate::add_log(&state, "Terhubung ke Binance WebSocket untuk data harga real-time seluruh koin").await;
                
                while let Some(msg) = ws_stream.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            let now = tokio::time::Instant::now();
                            if now.duration_since(last_update) >= tokio::time::Duration::from_secs(60) {
                                handle_ws_payload(&state, &text).await;
                                last_update = now;
                            }
                        }
                        Ok(Message::Binary(bin)) => {
                            let now = tokio::time::Instant::now();
                            if now.duration_since(last_update) >= tokio::time::Duration::from_secs(60) {
                                if let Ok(text) = String::from_utf8(bin) {
                                    handle_ws_payload(&state, &text).await;
                                    last_update = now;
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

// Fungsi untuk sync historical data (kline/candle 1 menit) dari Binance ke PostgreSQL crypto_klines
pub async fn sync_klines(db: &sqlx::PgPool, symbol: &str, limit: i32) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
                "INSERT INTO crypto_klines (symbol, open_time, open_price, high_price, low_price, close_price, volume)
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

pub async fn get_rest_price(symbol: &str) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!("https://api.binance.com/api/v3/ticker/price?symbol={}", symbol.to_uppercase());
    let resp = reqwest::get(&url)
        .await?
        .json::<serde_json::Value>()
        .await?;
    let price_str = resp["price"].as_str().ok_or("Invalid price field")?;
    let price = price_str.parse::<f64>()?;
    Ok(price)
}
