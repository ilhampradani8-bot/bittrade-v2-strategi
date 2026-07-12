use tokio::time::{sleep, Duration};
use chrono::Utc;
use crate::{AppState, PairStats, add_log_with_level, LogLevel, validate};
use crate::bars::{BarAggregator, fetch_historical_klines};

// UPGRADE: Pure mathematical function for OLS regression on slice references
fn calculate_ols(y: &[f64], x: &[f64]) -> Option<(f64, f64, f64, f64)> {
    let n = y.len() as f64;
    if n < 10.0 {
        return None;
    }
    let sum_x: f64 = x.iter().sum();
    let sum_y: f64 = y.iter().sum();
    let mean_x = sum_x / n;
    let mean_y = sum_y / n;

    let mut cov_xy = 0.0;
    let mut var_x = 0.0;
    let mut ss_tot = 0.0; // SS_tot = Σ(y - ȳ)²
    for i in 0..y.len() {
        let dx = x[i] - mean_x;
        let dy = y[i] - mean_y;
        cov_xy += dx * dy;
        var_x += dx * dx;
        ss_tot += dy * dy;
    }

    if var_x < 1e-12 {
        return None;
    }

    let beta = cov_xy / var_x;
    let alpha = mean_y - beta * mean_x;

    let mut ss_res = 0.0;
    for i in 0..y.len() {
        let res = y[i] - (beta * x[i] + alpha);
        ss_res += res * res;
    }

    let r2 = if ss_tot > 1e-12 {
        1.0 - (ss_res / ss_tot)
    } else {
        0.0
    };

    let std_err = (ss_res / n).sqrt();

    Some((beta, alpha, std_err, r2))
}

pub async fn start_analysis_loop(state: AppState) {
    let interval_secs = state.interval_secs;
    let max_history_size = state.min_samples_for_signal; // Re-purposed as window size
    
    let mut eth_bars = BarAggregator::new(interval_secs, max_history_size);
    let mut btc_bars = BarAggregator::new(interval_secs, max_history_size);
    
    add_log_with_level(&state, LogLevel::INFO, &format!("Initializing BarAggregator for ETH and BTC with interval {}s and window {} bars...", interval_secs, max_history_size)).await;
    
    // Fetch initial historical bars to zero-delay warmup
    let interval_str = if interval_secs % 3600 == 0 {
        format!("{}h", interval_secs / 3600)
    } else if interval_secs % 60 == 0 {
        format!("{}m", interval_secs / 60)
    } else {
        format!("{}s", interval_secs)
    };
    
    match fetch_historical_klines("ETHUSDT", &interval_str, max_history_size).await {
        Ok(closes) => {
            eth_bars.closes = closes;
            add_log_with_level(&state, LogLevel::INFO, &format!("Loaded {} historical bars for ETHUSDT.", eth_bars.closes.len())).await;
        }
        Err(e) => add_log_with_level(&state, LogLevel::ERROR, &format!("Failed to load historical bars for ETHUSDT: {}", e)).await,
    }
    
    match fetch_historical_klines("BTCUSDT", &interval_str, max_history_size).await {
        Ok(closes) => {
            btc_bars.closes = closes;
            add_log_with_level(&state, LogLevel::INFO, &format!("Loaded {} historical bars for BTCUSDT.", btc_bars.closes.len())).await;
        }
        Err(e) => add_log_with_level(&state, LogLevel::ERROR, &format!("Failed to load historical bars for BTCUSDT: {}", e)).await,
    }

    add_log_with_level(&state, LogLevel::INFO, "Starting statistical arbitrage analysis loop...").await;

    loop {
        sleep(Duration::from_millis(1000)).await;

        let eth_opt = {
            let prices = state.prices.read().await;
            prices.get("ETHUSDT").cloned()
        };
        let btc_opt = {
            let prices = state.prices.read().await;
            prices.get("BTCUSDT").cloned()
        };

        if let (Some(eth_data), Some(btc_data)) = (eth_opt, btc_opt) {
            let price_eth = eth_data.price;
            let price_btc = btc_data.price;
            let ts_ms = Utc::now().timestamp_millis();
            
            // Outlier check based on last closed bar
            if let (Some(last_eth), Some(last_btc)) = (eth_bars.latest_close(), btc_bars.latest_close()) {
                let change_eth = (price_eth - last_eth).abs() / last_eth;
                let change_btc = (price_btc - last_btc).abs() / last_btc;
                if change_eth > 0.05 || change_btc > 0.05 {
                    add_log_with_level(
                        &state,
                        LogLevel::WARN,
                        &format!(
                            "Outlier tick detected and filtered: ETH=${:.2} ({:.2}%), BTC=${:.2} ({:.2}%)",
                            price_eth, change_eth * 100.0, price_btc, change_btc * 100.0
                        )
                    ).await;
                    continue;
                }
            }

            eth_bars.add_tick(ts_ms, price_eth, 0.0);
            btc_bars.add_tick(ts_ms, price_btc, 0.0);
            
            // Only use fully closed bars for OLS to ensure consistency
            let len = eth_bars.closes.len().min(btc_bars.closes.len());
            
            // Update warmup progress in state
            {
                let mut current_samples = state.current_samples.write().await;
                *current_samples = len;
            }

            if len >= max_history_size {
                let mean_ratio: f64 = eth_bars.closes.iter().zip(btc_bars.closes.iter())
                    .map(|(a, b)| a / b)
                    .sum::<f64>() / len as f64;

                let y: Vec<f64> = eth_bars.closes.iter().map(|p| p.ln()).collect();
                let x: Vec<f64> = btc_bars.closes.iter().map(|p| p.ln()).collect();

                if let Some((beta, alpha, std_err, r2)) = calculate_ols(&y, &x) {
                    let current_y = price_eth.ln();
                    let current_x = price_btc.ln();
                    let current_spread = current_y - beta * current_x;

                    let z_score = if std_err > 0.0 {
                        (current_spread - alpha) / std_err
                    } else {
                        0.0
                    };

                    let current_ratio = price_eth / price_btc;

                    let stats = PairStats {
                        symbol_a: "ETHUSDT".to_string(),
                        symbol_b: "BTCUSDT".to_string(),
                        price_a: price_eth,
                        price_b: price_btc,
                        current_ratio,
                        rolling_mean: mean_ratio,
                        rolling_std: std_err,
                        z_score,
                        last_update: Utc::now(),
                        beta,
                        r2,
                        ols_alpha: alpha,
                    };

                    {
                        let mut ps_map = state.pair_stats.write().await;
                        ps_map.insert("ETHUSDT-BTCUSDT".to_string(), stats.clone());
                    }

                    {
                        let mut last_activity = state.last_engine_activity.write().await;
                        *last_activity = Utc::now();
                    }

                    if state.mode == "live" || state.mode == "paper" || state.mode == "shadow" {
                        let _ = validate::evaluate_signals(&state, &stats).await;
                    }
                }
            }
        }
    }
}
