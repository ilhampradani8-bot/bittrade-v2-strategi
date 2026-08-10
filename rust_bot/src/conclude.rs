// INI ADALAH FILE conclude.rs
use crate::AppState;
use crate::{uptrend, sideways, downtrend, breakout};

#[derive(Debug, Clone)]
pub enum Decision {
    Buy(f64, String),
    Sell(f64, String),
    Wait,
}

// Helper: Menghitung Exponential Moving Average (EMA) secepat kilat di Rust
pub fn calculate_ema(prices: &[f64], period: usize) -> f64 {
    if prices.is_empty() {
        return 0.0;
    }
    let k = 2.0 / (period as f64 + 1.0);
    // Karena prices diurutkan DESC (indeks 0 terbaru), kita hitung dari yang terlama
    let mut ema = prices[prices.len() - 1];
    for i in (0..prices.len() - 1).rev() {
        ema = prices[i] * k + ema * (1.0 - k);
    }
    ema
}

// Helper: Menghitung Relative Strength Index (RSI 14) secara efisien
pub fn calculate_rsi(klines: &[(f64, f64, chrono::DateTime<chrono::Utc>, f64, f64)], period: usize) -> Option<f64> {
    if klines.len() <= period {
        return None;
    }
    
    // Urutan klines DESC (indeks 0 adalah terbaru)
    let mut gains = 0.0;
    let mut losses = 0.0;
    
    // Ambil `period` transisi pertama dari yang terlama
    let start_idx = klines.len() - 1 - period;
    for i in (start_idx..klines.len() - 1).rev() {
        let diff = klines[i].0 - klines[i + 1].0;
        if diff > 0.0 {
            gains += diff;
        } else {
            losses -= diff;
        }
    }
    
    let mut avg_gain = gains / period as f64;
    let mut avg_loss = losses / period as f64;
    
    // Gunakan smoothing Wilder untuk sisanya hingga candle terbaru
    for i in (0..start_idx).rev() {
        let diff = klines[i].0 - klines[i + 1].0;
        if diff > 0.0 {
            avg_gain = (avg_gain * (period - 1) as f64 + diff) / period as f64;
            avg_loss = (avg_loss * (period - 1) as f64) / period as f64;
        } else {
            avg_gain = (avg_gain * (period - 1) as f64) / period as f64;
            avg_loss = (avg_loss * (period - 1) as f64 - diff) / period as f64;
        }
    }
    
    if avg_loss == 0.0 {
        return Some(100.0);
    }
    let rs = avg_gain / avg_loss;
    Some(100.0 - (100.0 / (1.0 + rs)))
}

// Helper: Menghitung Slope/Kemiringan RSI untuk konfirmasi momentum
pub fn calculate_rsi_slope(klines: &[(f64, f64, chrono::DateTime<chrono::Utc>, f64, f64)], period: usize, lookback: usize) -> f64 {
    if klines.len() < period + lookback {
        return 0.0;
    }
    let rsi_now = calculate_rsi(klines, period).unwrap_or(50.0);
    // Potong data klines seolah-olah lookback menit yang lalu
    let klines_past = &klines[lookback..];
    let rsi_past = calculate_rsi(klines_past, period).unwrap_or(50.0);
    rsi_now - rsi_past
}

pub async fn analyze_market(price: f64, state: &AppState) -> Decision {
    analyze_market_for_symbol(&state.active_symbol, price, state).await
}

pub async fn analyze_market_for_symbol(symbol: &str, price: f64, state: &AppState) -> Decision {
    let sim_bal = *state.simulated_balance.read().await;

    // Hitung sisa kepemilikan koin ini dari tabel bot_active_positions
    let coin_bal: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount), 0.0) FROM bot_active_positions WHERE symbol = $1"
    )
    .bind(symbol)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0.0);

    // 1. Tarik data kline historis 50 candle untuk konteks pasar & perhitungan ATR
    let klines: Vec<(f64, f64, chrono::DateTime<chrono::Utc>, f64, f64)> = sqlx::query_as::<_, (f64, f64, chrono::DateTime<chrono::Utc>, f64, f64)>(
        "SELECT close_price, volume, open_time, high_price, low_price FROM crypto_klines WHERE symbol = $1 ORDER BY open_time DESC LIMIT 50"
    )
    .bind(symbol)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    if klines.len() < 34 {
        println!("[{}] Data historis di DB belum mencukupi ({} dari 34). Menunggu...", symbol, klines.len());
        return Decision::Wait;
    }

    // Hitung Average True Range (ATR 14-Period)
    let mut tr_sum = 0.0;
    let atr_period = 14.min(klines.len() - 1);
    for i in 0..atr_period {
        let high = klines[i].3;
        let low = klines[i].4;
        let prev_close = klines[i+1].0;
        let tr1 = high - low;
        let tr2 = (high - prev_close).abs();
        let tr3 = (low - prev_close).abs();
        tr_sum += tr1.max(tr2).max(tr3);
    }
    let atr = tr_sum / atr_period as f64;

    // 2. Cek Pengaman (Stop Loss Dinamis ATR & Trailing TP)
    let last_buy = match sqlx::query_as::<_, (f64, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT price, notes AS reason, timestamp 
         FROM bot_trading_history 
         WHERE symbol = $1 AND action = 'BUY' AND status = 'SUCCESS' 
           AND timestamp >= (SELECT COALESCE(MIN(opened_at), '2999-12-31 00:00:00+00'::timestamptz) FROM bot_active_positions WHERE symbol = $1)
         ORDER BY id ASC LIMIT 1"
    )
    .bind(symbol)
    .fetch_optional(&state.db)
    .await {
        Ok(opt) => opt,
        Err(e) => {
            eprintln!("[{}] 🚨 Gagal mengambil data last_buy dari database: {}", symbol, e);
            None
        }
    };

    let cat = {
        let map = state.coin_states.read().await;
        map.get(symbol).map(|c| c.volatility_category.clone()).unwrap_or_else(|| "LOW".to_string())
    };
    let params = crate::classifier::get_params_for_category(&cat, symbol);

    if let Some((buy_p, ref buy_reason, _buy_time)) = last_buy {
        if coin_bal > 0.0001 {
            let sl_limit = params.stop_loss_limit.abs();
            if price <= buy_p * (1.0 - sl_limit) {
                println!("[{}] 🚨 STOP LOSS TERPICU! Harga Beli: ${:.4} | Harga Saat Ini: ${:.4} (Turun >= {:.2}%)", symbol, buy_p, price, sl_limit * 100.0);
                return Decision::Sell(coin_bal, format!("[Darurat] Stop Loss -{:.2}%", sl_limit * 100.0));
            }

            // B. Trailing Take Profit Dinamis
            let db_hwm: f64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(high_water_mark), 0.0) FROM bot_active_positions WHERE symbol = $1"
            )
            .bind(symbol)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0.0);

            if price > db_hwm {
                let _ = sqlx::query("UPDATE bot_active_positions SET high_water_mark = $1 WHERE symbol = $2 AND high_water_mark < $1")
                    .bind(price)
                    .bind(symbol)
                    .execute(&state.db)
                    .await;
            }

            let peak_price = db_hwm.max(price).max(buy_p);
            let profit_pct_from_peak = ((peak_price - price) / peak_price) * 100.0;
            let profit_pct_from_buy = ((price - buy_p) / buy_p) * 100.0;
            let peak_profit_pct = ((peak_price - buy_p) / buy_p) * 100.0;

            // B1. [Trending] Trailing TP Khusus
            let tp_trigger_pct = params.uptrend_tp_trail_trigger * 100.0;
            let tp_pullback_pct = params.uptrend_tp_trail_pullback * 100.0;
            if buy_reason.contains("[Trending]") {
                if peak_profit_pct >= tp_trigger_pct && profit_pct_from_peak >= tp_pullback_pct {
                    println!("[{}] 🎯 TRAILING TP [Trending] TERPICU! Puncak: ${:.4} (+{:.2}%) | Saat Ini: ${:.4} | Turun {:.2}% dari puncak.", symbol, peak_price, peak_profit_pct, price, profit_pct_from_peak);
                    return Decision::Sell(coin_bal, format!("[Trending] Trailing TP (Puncak +{:.2}%, Turun {:.2}%)", peak_profit_pct, profit_pct_from_peak));
                }
            }
            // B2. Trailing TP umum untuk breakout
            else if buy_reason.contains("[Breakout]") {
                if peak_profit_pct >= 1.5 && profit_pct_from_peak >= 1.0 {
                    println!("[{}] 🎯 TRAILING TAKE PROFIT [Breakout] TERPICU! Puncak: ${:.4} (+{:.2}%) | Saat Ini: ${:.4} | Turun {:.2}% dari pucuk.", symbol, peak_price, peak_profit_pct, price, profit_pct_from_peak);
                    return Decision::Sell(coin_bal, format!("[Breakout] Trailing Take Profit (Puncak +{:.1}%)", peak_profit_pct));
                }
            } else if profit_pct_from_buy >= 3.0 && !buy_reason.contains("[Downtrend]") {
                // Hard Take Profit jika langsung loncat >= 3%
                println!("[{}] 🎯 TAKE PROFIT TERPICU! Harga Beli: ${:.4} | Harga Saat Ini: ${:.4} (Naik >= 3%)", symbol, buy_p, price);
                return Decision::Sell(coin_bal, "[Darurat] Take Profit +3%".to_string());
            }
        }
    }

    // Validasi kesinambungan data (Data gap / Missing candle check)
    for i in 0..klines.len() - 1 {
        let diff = (klines[i].2 - klines[i + 1].2).num_seconds().abs();
        if diff > 90 {
            println!("[{}] ⚠️ Terdeteksi lompatan/gap data candle > 90 detik ({}s). Menunggu sinkronisasi agar indikator akurat...", symbol, diff);
            return Decision::Wait;
        }
    }

    let prices: Vec<f64> = klines.iter().map(|k| k.0).collect();

    // Hitung Bollinger Bands 50-period
    let bb_len = prices.len().min(50);
    let sma_bb: f64 = prices[0..bb_len].iter().sum::<f64>() / bb_len as f64;
    let variance: f64 = prices[0..bb_len].iter()
        .map(|&p| {
            let diff = p - sma_bb;
            diff * diff
        })
        .sum::<f64>() / bb_len as f64;
    let std_dev = variance.sqrt();
    
    // Adaptive EMA Parameters
    let vol_pct = (std_dev / price) * 100.0;
    let (ema_fast_len, ema_slow_len) = if vol_pct > 0.15 {
        (9, 21) // Pasar Cepat: Gunakan EMA 9/21 agar lebih responsif
    } else {
        (13, 34) // Pasar Lambat: Gunakan EMA 13/34 untuk menyaring noise
    };

    let ema_fast = calculate_ema(&prices, ema_fast_len);
    let ema_slow = calculate_ema(&prices, ema_slow_len);

    // Hitung True Session VWAP untuk koin ini
    let session_vwap: Option<f64> = sqlx::query_scalar(
        "SELECT SUM(close_price * volume) / NULLIF(SUM(volume), 0) FROM crypto_klines WHERE symbol = $1 AND open_time >= date_trunc('day', CURRENT_TIMESTAMP)"
    )
    .bind(symbol)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let vwap = session_vwap.unwrap_or(price);
    
    let current_vol = klines[0].1;
    let sum_v: f64 = klines.iter().map(|(_, v, _, _, _)| v).sum();
    let avg_vol = sum_v / klines.len() as f64;
    let vol_surge = if avg_vol > 0.0 { current_vol / avg_vol } else { 1.0 };

    let upper_band = sma_bb + (2.0 * std_dev);
    let lower_band = sma_bb - (2.0 * std_dev);

    // Filter Multi-Timeframe (Konfirmasi tren 15 menit terakhir)
    let trend_15m_bullish = klines.len() > 15 && prices[0] > prices[15];

    // Deteksi Perubahan Mendadak (Spike / Sudden Pump or Dump)
    let last_change = prices[0] - prices[1];
    let is_sudden_dump = last_change < -(2.5 * std_dev) && std_dev > 5.0;

    // Tentukan State Pasar (Trending vs Sideways) secara adaptif menggunakan parameter CVC
    let bb_len_20 = prices.len().min(20);
    let sma_bb_20: f64 = prices[0..bb_len_20].iter().sum::<f64>() / bb_len_20 as f64;
    let variance_20: f64 = prices[0..bb_len_20].iter()
        .map(|&p| {
            let diff = p - sma_bb_20;
            diff * diff
        })
        .sum::<f64>() / bb_len_20 as f64;
    let std_dev_20 = variance_20.sqrt();
    let volatility_pct_20 = (std_dev_20 / price) * 100.0;
    
    let is_sideways = volatility_pct_20 >= params.sideways_min_vol_pct && volatility_pct_20 < params.sideways_max_vol_pct;

    let regime_str = if is_sideways {
        "SIDEWAYS".to_string()
    } else {
        if ema_fast > ema_slow {
            "BULLISH".to_string()
        } else {
            "BEARISH".to_string()
        }
    };

    // Log OBI untuk QPS Market Metrics
    let obi = {
        let map = state.coin_states.read().await;
        map.get(symbol).map(|c| c.order_book_imbalance).unwrap_or(0.5)
    };
    let _ = sqlx::query(
        "INSERT INTO qps_market_metrics_log (obi_value, volatility_pct, vol_surge, btc_price)
         VALUES ($1, $2, $3, $4)"
    )
    .bind(obi)
    .bind(vol_pct)
    .bind(vol_surge)
    .bind(price)
    .execute(&state.db)
    .await;

    println!(
        "[{}] State: {} | EMA({}/{}): ${:.4}/${:.4} | VWAP Sesi: ${:.4} | Vol Surge: {:.1}x | BB50: [${:.4} - ${:.4}] | Tren 15m: {}", 
        symbol,
        regime_str, 
        ema_fast_len, ema_slow_len,
        ema_fast, ema_slow,
        vwap,
        vol_surge,
        lower_band, 
        upper_band,
        if trend_15m_bullish { "BULLISH" } else { "BEARISH" }
    );

    // A. DETEKSI DUMP MENDADAK (Defensive Action)
    if is_sudden_dump {
        if coin_bal > 0.0001 {
            println!("[{}] ⚠️ DETEKSI DUMP MENDADAK! Mengamankan aset, JUAL semua.", symbol);
            return Decision::Sell(coin_bal, "[Darurat] Deteksi Dump Mendadak".to_string());
        }
    }

    // B. EXIT STRATEGI (Mandiri & Terdistribusi)
    if coin_bal > 0.0001 {
        if let Some((buy_p, ref buy_reason, buy_time)) = last_buy {
            // 1. Sideways Exit
            if let Some(dec) = sideways::evaluate_exit(symbol, price, coin_bal, buy_reason, &klines, state).await {
                return dec;
            }
            // 2. Downtrend Exit
            if let Some(dec) = downtrend::evaluate_exit(symbol, price, coin_bal, buy_p, buy_reason, buy_time, ema_fast, ema_slow, state).await {
                return dec;
            }
            // 3. Breakout Exit
            if let Some(dec) = breakout::evaluate_exit(symbol, price, coin_bal, buy_reason, buy_time, ema_fast, ema_slow, vwap, trend_15m_bullish, state).await {
                return dec;
            }
            // 4. Uptrend Exit
            if let Some(dec) = uptrend::evaluate_exit(symbol, price, coin_bal, buy_reason, ema_fast, ema_slow, vwap, trend_15m_bullish, ema_fast_len, ema_slow_len, state).await {
                return dec;
            }
        }
    }

    // C. ENTRY STRATEGI (Evaluasi Mandiri)
    
    // C1. Breakout Strategy
    if let Some(dec) = breakout::evaluate_entry(symbol, price, sim_bal, vwap, std_dev, vol_surge, ema_fast, ema_slow, trend_15m_bullish, &prices, &klines, state).await {
        return dec;
    }

    // C2. Sideways Strategy
    if let Some(dec) = sideways::evaluate_entry(symbol, price, sim_bal, vwap, &klines, state).await {
        return dec;
    }

    // C3. Downtrend Strategy (Bearish Climax Rebound)
    if let Some(dec) = downtrend::evaluate_entry(symbol, price, sim_bal, ema_fast, ema_slow, vwap, vol_surge, upper_band, lower_band, &klines, state).await {
        return dec;
    }

    // C4. Uptrend Strategy (Trending Bullish)
    let ema_spread_pct = ((ema_fast - ema_slow) / price) * 100.0;
    if let Some(dec) = uptrend::evaluate_entry(symbol, price, sim_bal, ema_fast, ema_slow, vwap, trend_15m_bullish, vol_surge, ema_fast_len, ema_slow_len, ema_spread_pct, &klines, state).await {
        return dec;
    }

    Decision::Wait
}
