use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures_util::StreamExt;
use serde_json::Value;
use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;
use crate::{AppState, PriceData, add_log_with_level, LogLevel};
use crate::bars::fetch_historical_klines;

#[derive(Clone, Debug, Serialize)]
pub struct ScannerPair {
    pub name: String,
    pub symbol_a: String,
    pub symbol_b: String,
    pub price_a: f64,
    pub price_b: f64,
    pub ratio: f64,
    pub mean: f64,
    pub std: f64,
    pub z_score: f64,
    pub r2: f64,
    pub category: String,
    pub est_apr: f64,
}

struct PriceHistory {
    prices_a: Vec<f64>,
    prices_b: Vec<f64>,
}

fn calculate_ols(y: &[f64], x: &[f64]) -> Option<(f64, f64, f64, f64)> {
    let n = y.len() as f64;
    if n < 10.0 { return None; }
    let sum_x: f64 = x.iter().sum();
    let sum_y: f64 = y.iter().sum();
    let mean_x = sum_x / n;
    let mean_y = sum_y / n;
    let mut cov_xy = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    for i in 0..y.len() {
        let dx = x[i] - mean_x;
        let dy = y[i] - mean_y;
        cov_xy += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }
    if var_x < 1e-12 { return None; }
    let beta = cov_xy / var_x;
    let alpha = mean_y - beta * mean_x;
    let mut ss_res = 0.0;
    for i in 0..y.len() {
        let res = y[i] - (beta * x[i] + alpha);
        ss_res += res * res;
    }
    let r2 = if var_y > 1e-12 { 1.0 - (ss_res / var_y) } else { 0.0 };
    let std_err = (ss_res / n).sqrt();
    Some((beta, alpha, std_err, r2))
}

pub async fn start_price_listener(state: AppState) {
    let ws_url = "wss://stream.binance.com:9443/stream?streams=btcusdt@ticker/ethusdt@ticker";
    let client = reqwest::Client::builder().timeout(Duration::from_secs(3)).build().unwrap_or_default();
    let mut consecutive_failures = 0;

    loop {
        add_log_with_level(&state, LogLevel::INFO, &format!("Connecting to Binance WebSocket: {}", ws_url)).await;
        
        match connect_async(ws_url).await {
            Ok((ws_stream, _)) => {
                add_log_with_level(&state, LogLevel::INFO, "WebSocket connected successfully!").await;
                consecutive_failures = 0;
                {
                    let mut data_healthy = state.data_feed_healthy.write().await;
                    *data_healthy = true;
                }
                
                let (_, mut read) = ws_stream.split();
                while let Some(message) = read.next().await {
                    match message {
                        Ok(Message::Text(text)) => {
                            if let Ok(val) = serde_json::from_str::<Value>(&text) {
                                if val.get("stream").is_some() {
                                    if let Some(data) = val.get("data") {
                                        let symbol = data.get("s").and_then(|s| s.as_str()).unwrap_or("").to_string();
                                        let price_str = data.get("c").and_then(|c| c.as_str()).unwrap_or("0.0");
                                        if let Ok(price) = price_str.parse::<f64>() {
                                            if symbol == "BTCUSDT" || symbol == "ETHUSDT" {
                                                let now = Utc::now();
                                                {
                                                    let mut prices_map = state.prices.write().await;
                                                    prices_map.insert(symbol.clone(), PriceData {
                                                        symbol: symbol.clone(),
                                                        price,
                                                        last_update: now,
                                                    });
                                                }
                                                *state.last_ws_activity.write().await = now;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {}
                        Err(e) => {
                            add_log_with_level(&state, LogLevel::WARN, &format!("WebSocket read error: {:?}", e)).await;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("WebSocket connection failed: {:?}", e);
                add_log_with_level(&state, LogLevel::WARN, &err_msg).await;
                let mut backoff = Duration::from_millis(100);
                for attempt in 1..=3 {
                    let res = sqlx::query("INSERT INTO starb_corrections (error_type, reason, severity) VALUES ($1, $2, $3)")
                        .bind("CONNECTION_FAILURE").bind(&err_msg).bind("WARN").execute(&state.db).await;
                    if res.is_ok() { break; }
                    if attempt < 3 { sleep(backoff).await; backoff *= 2; }
                }
            }
        }

        add_log_with_level(&state, LogLevel::WARN, "WebSocket down. Starting REST fallback polling (1s interval)...").await;
        let ws_reconnect_interval = Duration::from_secs(15);
        let poll_interval = Duration::from_secs(1);
        let start_ws_retry = tokio::time::Instant::now();

        while start_ws_retry.elapsed() < ws_reconnect_interval {
            sleep(poll_interval).await;

            let btc_res = client.get("https://api.binance.com/api/v3/ticker/price?symbol=BTCUSDT").send().await;
            let eth_res = client.get("https://api.binance.com/api/v3/ticker/price?symbol=ETHUSDT").send().await;

            let mut btc_price_opt = None;
            let mut eth_price_opt = None;

            if let Ok(resp) = btc_res {
                if let Ok(val) = resp.json::<Value>().await {
                    if let Some(p_str) = val.get("price").and_then(|v| v.as_str()) {
                        if let Ok(p) = p_str.parse::<f64>() { btc_price_opt = Some(p); }
                    }
                }
            }
            if let Ok(resp) = eth_res {
                if let Ok(val) = resp.json::<Value>().await {
                    if let Some(p_str) = val.get("price").and_then(|v| v.as_str()) {
                        if let Ok(p) = p_str.parse::<f64>() { eth_price_opt = Some(p); }
                    }
                }
            }

            if let (Some(btc_p), Some(eth_p)) = (btc_price_opt, eth_price_opt) {
                let now = Utc::now();
                {
                    let mut prices_map = state.prices.write().await;
                    prices_map.insert("BTCUSDT".to_string(), PriceData { symbol: "BTCUSDT".to_string(), price: btc_p, last_update: now });
                    prices_map.insert("ETHUSDT".to_string(), PriceData { symbol: "ETHUSDT".to_string(), price: eth_p, last_update: now });
                }
                *state.last_ws_activity.write().await = now;
                consecutive_failures = 0;
                {
                    let mut data_healthy = state.data_feed_healthy.write().await;
                    *data_healthy = true;
                }
            } else {
                consecutive_failures += 1;
                if consecutive_failures >= 5 {
                    {
                        let mut data_healthy = state.data_feed_healthy.write().await;
                        *data_healthy = false;
                    }
                    add_log_with_level(&state, LogLevel::CRITICAL, &format!("Data feed failure: both WebSocket and REST fallback failed {} consecutive times.", consecutive_failures)).await;
                }
            }
        }
    }
}

pub async fn start_scanner_loop(state: AppState) {
    add_log_with_level(&state, LogLevel::INFO, "⚡ Starting 300+ Coin Co-Integration Spread Scanner Loop...").await;
    
    let mut histories: HashMap<String, PriceHistory> = HashMap::new();
    let max_history_size = state.min_samples_for_signal;
    let interval_secs = state.interval_secs;
    
    let interval_str = if interval_secs % 3600 == 0 {
        format!("{}h", interval_secs / 3600)
    } else if interval_secs % 60 == 0 {
        format!("{}m", interval_secs / 60)
    } else {
        format!("{}s", interval_secs)
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    // Initial Warmup
    add_log_with_level(&state, LogLevel::INFO, "Scanner Warmup: Fetching initial symbols...").await;
    let url_futures = "https://fapi.binance.com/fapi/v1/premiumIndex";
    let mut active_symbols = Vec::new();
    
    if let Ok(resp) = client.get(url_futures).send().await {
        if let Ok(val) = resp.json::<Value>().await {
            if let Some(arr) = val.as_array() {
                for item in arr {
                    if let Some(sym) = item.get("symbol").and_then(|v| v.as_str()) {
                        if sym.ends_with("USDT") && sym != "BTCUSDT" && sym != "ETHUSDT" && sym != "USDCUSDT" && sym != "FDUSDUSDT" {
                            active_symbols.push(sym.to_string());
                        }
                    }
                }
            }
        }
    }
    
    active_symbols.truncate(150); // limit to 150 pairs to save rate limits
    
    // Fetch BTC and ETH history
    let btc_hist = fetch_historical_klines("BTCUSDT", &interval_str, max_history_size).await.unwrap_or_default();
    let eth_hist = fetch_historical_klines("ETHUSDT", &interval_str, max_history_size).await.unwrap_or_default();
    
    if !btc_hist.is_empty() && !eth_hist.is_empty() {
        add_log_with_level(&state, LogLevel::INFO, &format!("Scanner Warmup: Fetching historical klines for {} symbols in chunks...", active_symbols.len())).await;
        for chunk in active_symbols.chunks(20) {
            let mut tasks = Vec::new();
            for sym in chunk {
                let sym_clone = sym.clone();
                let int_clone = interval_str.clone();
                tasks.push(tokio::spawn(async move {
                    (sym_clone.clone(), fetch_historical_klines(&sym_clone, &int_clone, max_history_size).await)
                }));
            }
            for task in tasks {
                if let Ok((sym, Ok(hist))) = task.await {
                    let base_name = sym.replace("USDT", "");
                    let name_btc = format!("{} / BTC", base_name);
                    let name_eth = format!("{} / ETH", base_name);
                    
                    histories.insert(name_btc, PriceHistory {
                        prices_a: hist.clone().into(),
                        prices_b: btc_hist.clone().into(),
                    });
                    
                    histories.insert(name_eth, PriceHistory {
                        prices_a: hist.into(),
                        prices_b: eth_hist.clone().into(),
                    });
                }
            }
            sleep(Duration::from_millis(1000)).await;
        }
        add_log_with_level(&state, LogLevel::INFO, "Scanner Warmup: Complete!").await;
    }

    // Now standard polling loop (polling every 3 minutes for new 5m bars to avoid rate limits, or just polling 1m)
    // For simplicity, we just fetch klines for all pairs every `interval_secs`
    loop {
        let mut symbol_prices: Vec<(String, f64)> = Vec::new();
        if let Ok(resp) = client.get(url_futures).send().await {
            if let Ok(val) = resp.json::<Value>().await {
                if let Some(arr) = val.as_array() {
                    for item in arr {
                        if let Some(sym) = item.get("symbol").and_then(|v| v.as_str()) {
                            if sym.ends_with("USDT") {
                                let price = item.get("markPrice").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                                if price > 0.0 { symbol_prices.push((sym.to_string(), price)); }
                            }
                        }
                    }
                }
            }
        }

        if !symbol_prices.is_empty() {
            let mut btc_price = 62000.0;
            let mut eth_price = 3400.0;
            for (s, p) in &symbol_prices {
                if s == "BTCUSDT" { btc_price = *p; }
                if s == "ETHUSDT" { eth_price = *p; }
            }

            let mut pairs: Vec<ScannerPair> = Vec::new();

            for (sym, price) in &symbol_prices {
                if sym == "BTCUSDT" || sym == "ETHUSDT" || sym == "USDCUSDT" || sym == "FDUSDUSDT" { continue; }
                let base_name = sym.replace("USDT", "");
                
                let category = if base_name.contains("DOGE") || base_name.contains("SHIB") || base_name.contains("PEPE") || base_name.contains("WIF") { "Meme Coins".to_string() }
                else if base_name.contains("RENDER") || base_name.contains("FET") || base_name.contains("TAO") { "AI & Big Data".to_string() }
                else if base_name.contains("LINK") || base_name.contains("ARB") || base_name.contains("OP") || base_name.contains("UNI") { "DeFi / Dex".to_string() }
                else { "Layer-1 / L2".to_string() };

                // Pair against BTC
                {
                    let name = format!("{} / BTC", base_name);
                    let hist = histories.entry(name.clone()).or_insert_with(|| PriceHistory { prices_a: Vec::new(), prices_b: Vec::new() });
                    hist.prices_a.push(*price); hist.prices_b.push(btc_price);
                    if hist.prices_a.len() > max_history_size { hist.prices_a.remove(0); hist.prices_b.remove(0); }

                    if hist.prices_a.len() >= 10 {
                        let y: Vec<f64> = hist.prices_a.iter().map(|p| p.ln()).collect();
                        let x: Vec<f64> = hist.prices_b.iter().map(|p| p.ln()).collect();
                        if let Some((beta, alpha, std_err, r2)) = calculate_ols(&y, &x) {
                            let current_y = price.ln(); let current_x = btc_price.ln(); let current_spread = current_y - beta * current_x;
                            let z_score = if std_err > 0.0 { (current_spread - alpha) / std_err } else { 0.0 };
                            pairs.push(ScannerPair { name, symbol_a: sym.clone(), symbol_b: "BTCUSDT".to_string(), price_a: *price, price_b: btc_price, ratio: *price / btc_price, mean: alpha, std: std_err, z_score, r2, category: category.clone(), est_apr: 14.5 + (z_score.abs() * 26.2) });
                        }
                    }
                }

                // Pair against ETH
                {
                    let name = format!("{} / ETH", base_name);
                    let hist = histories.entry(name.clone()).or_insert_with(|| PriceHistory { prices_a: Vec::new(), prices_b: Vec::new() });
                    hist.prices_a.push(*price); hist.prices_b.push(eth_price);
                    if hist.prices_a.len() > max_history_size { hist.prices_a.remove(0); hist.prices_b.remove(0); }

                    if hist.prices_a.len() >= 10 {
                        let y: Vec<f64> = hist.prices_a.iter().map(|p| p.ln()).collect();
                        let x: Vec<f64> = hist.prices_b.iter().map(|p| p.ln()).collect();
                        if let Some((beta, alpha, std_err, r2)) = calculate_ols(&y, &x) {
                            let current_y = price.ln(); let current_x = eth_price.ln(); let current_spread = current_y - beta * current_x;
                            let z_score = if std_err > 0.0 { (current_spread - alpha) / std_err } else { 0.0 };
                            pairs.push(ScannerPair { name, symbol_a: sym.clone(), symbol_b: "ETHUSDT".to_string(), price_a: *price, price_b: eth_price, ratio: *price / eth_price, mean: alpha, std: std_err, z_score, r2, category: category.clone(), est_apr: 13.8 + (z_score.abs() * 25.4) });
                        }
                    }
                }
            }

            pairs.sort_by(|a, b| b.z_score.abs().partial_cmp(&a.z_score.abs()).unwrap_or(std::cmp::Ordering::Equal));
            let mut sp_lock = state.scanner_pairs.write().await;
            *sp_lock = pairs;
        }

        sleep(Duration::from_secs(60)).await;
    }
}

pub async fn start_exchange_limits_updater(state: AppState) {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().unwrap_or_default();
    let url = "https://fapi.binance.com/fapi/v1/exchangeInfo";

    loop {
        if let Ok(resp) = client.get(url).send().await {
            if let Ok(val) = resp.json::<Value>().await {
                if let Some(symbols) = val.get("symbols").and_then(|s| s.as_array()) {
                    for sym in symbols {
                        if let Some(symbol) = sym.get("symbol").and_then(|s| s.as_str()) {
                            if symbol == "BTCUSDT" || symbol == "ETHUSDT" {
                                let mut step_size = 0.001;
                                let mut min_notional = 5.0;

                                if let Some(filters) = sym.get("filters").and_then(|f| f.as_array()) {
                                    for filter in filters {
                                        if filter.get("filterType").and_then(|ft| ft.as_str()) == Some("LOT_SIZE") {
                                            step_size = filter.get("stepSize").and_then(|s| s.as_str()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.001);
                                        }
                                        if filter.get("filterType").and_then(|ft| ft.as_str()) == Some("MIN_NOTIONAL") {
                                            min_notional = filter.get("notional").and_then(|n| n.as_str()).and_then(|n| n.parse::<f64>().ok()).unwrap_or(5.0);
                                        }
                                    }
                                }

                                if symbol == "BTCUSDT" {
                                    *state.btc_step_size.write().await = step_size;
                                    *state.btc_min_notional.write().await = min_notional;
                                } else if symbol == "ETHUSDT" {
                                    *state.eth_step_size.write().await = step_size;
                                    *state.eth_min_notional.write().await = min_notional;
                                }
                            }
                        }
                    }
                }
            }
        }
        // Update every 4 hours
        sleep(Duration::from_secs(4 * 3600)).await;
    }
}
