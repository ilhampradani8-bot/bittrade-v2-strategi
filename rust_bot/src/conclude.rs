// INI ADALAH FILE conclude.rs
use crate::AppState;

#[derive(Debug, Clone)]
pub enum Decision {
    Buy(f64, String),
    Sell(f64, String),
    Wait,
}

// Helper: Menghitung Exponential Moving Average (EMA) secepat kilat di Rust
fn calculate_ema(prices: &[f64], period: usize) -> f64 {
    if prices.is_empty() {
        return 0.0;
    }
    let k = 2.0 / (period as f64 + 1.0);
    // Karena prices diurutkan DESC (indeks 0 terbaru), kita hitung dari yang terlama
    let mut ema = prices[prices.len() - 1];
    for i in (0..prices.len() - 1).rev() {
        ema = (prices[i] * k) + (ema * (1.0 - k));
    }
    ema
}

pub async fn analyze_market(price: f64, state: &AppState) -> Decision {
    let sim_bal = *state.simulated_balance.read().await;
    let btc_bal = *state.btc_balance.read().await;

    // 1. Tarik data kline historis 50 candle untuk konteks pasar & perhitungan ATR
    let klines: Vec<(f64, f64, chrono::DateTime<chrono::Utc>, f64, f64)> = sqlx::query_as::<_, (f64, f64, chrono::DateTime<chrono::Utc>, f64, f64)>(
        "SELECT close_price, volume, open_time, high_price, low_price FROM btc_klines ORDER BY open_time DESC LIMIT 50"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    if klines.len() < 34 {
        println!("[ANALIS] Data historis di DB belum mencukupi ({} dari 34). Menunggu...", klines.len());
        return Decision::Wait;
    }

    // FASE 5.0b: Hitung Average True Range (ATR 14-Period)
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
    let atr_pct = atr / price; // Persentase fluktuasi rata-rata per menit

    // 2. Cek Pengaman (Stop Loss Dinamis ATR & Trailing TP)
    let last_buy: Option<(f64, chrono::DateTime<chrono::Utc>)> = sqlx::query_as::<_, (f64, chrono::DateTime<chrono::Utc>)>(
        "SELECT price, timestamp FROM bot_trading_history WHERE action = 'BUY' AND status = 'SUCCESS' ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    if let Some((buy_p, _buy_time)) = last_buy {
        if btc_bal > 0.0001 {
            // A. FASE 5.0b: Emergency Stop Loss Dinamis (ATR x 3.0) - Min 0.8%, Max 3.0%
            let dynamic_sl_pct = (atr_pct * 3.0).clamp(0.008, 0.030);
            if price <= buy_p * (1.0 - dynamic_sl_pct) {
                println!("[ANALIS] 🚨 ATR STOP LOSS TERPICU! Harga Beli: ${:.2} | Harga Saat Ini: ${:.2} (Turun >= {:.2}%)", buy_p, price, dynamic_sl_pct * 100.0);
                return Decision::Sell(btc_bal, format!("[Darurat] ATR Stop Loss -{:.2}%", dynamic_sl_pct * 100.0));
            }
            // B. Trailing Take Profit Dinamis (Optimasi ringan dari RAM / AppState)
            let mut hwm = state.high_water_mark.write().await;
            if price > *hwm {
                *hwm = price;
                let db_clone = state.db.clone();
                tokio::spawn(async move {
                    let _ = sqlx::query("UPDATE bot_active_positions SET high_water_mark = $1 WHERE high_water_mark < $1").bind(price).execute(&db_clone).await;
                });
            }
            let peak_price = (*hwm).max(price).max(buy_p);
            let profit_pct_from_peak = ((peak_price - price) / peak_price) * 100.0;
            let profit_pct_from_buy = ((price - buy_p) / buy_p) * 100.0;
            let peak_profit_pct = ((peak_price - buy_p) / buy_p) * 100.0;

            // Jika harga sempat naik minimal +1.5% dari harga beli, sistem Trailing otomatis aktif
            if peak_profit_pct >= 1.5 {
                // Jika harga turun 1.0% dari pucuk tertinggi (Peak), bungkus profit sekarang juga!
                if profit_pct_from_peak >= 1.0 {
                    println!("[ANALIS] 🎯 TRAILING TAKE PROFIT TERPICU! Puncak: ${:.2} (+{:.2}%) | Saat Ini: ${:.2} (+{:.2}%) | Turun {:.2}% dari pucuk.", peak_price, peak_profit_pct, price, profit_pct_from_buy, profit_pct_from_peak);
                    return Decision::Sell(btc_bal, format!("[Darurat] Trailing Take Profit (Puncak Cuaca +{:.1}%)", peak_profit_pct));
                }
            } else if profit_pct_from_buy >= 3.0 {
                // Hard Take Profit jika langsung loncat >= 3%
                println!("[ANALIS] 🎯 TAKE PROFIT TERPICU! Harga Beli: ${:.2} | Harga Saat Ini: ${:.2} (Naik >= 3%)", buy_p, price);
                return Decision::Sell(btc_bal, "[Darurat] Take Profit +3%".to_string());
            }
        }
    }


    if klines.len() < 34 {
        println!("[ANALIS] Data historis di DB belum mencukupi ({} dari 34). Menunggu...", klines.len());
        return Decision::Wait;
    }

    // Validasi kesinambungan data (Data gap / Missing candle check)
    for i in 0..klines.len() - 1 {
        let diff = (klines[i].2 - klines[i + 1].2).num_seconds().abs();
        if diff > 90 {
            println!("[ANALIS] ⚠️ Terdeteksi lompatan/gap data candle > 90 detik ({}s). Menunggu sinkronisasi agar indikator akurat...", diff);
            return Decision::Wait;
        }
    }

    let prices: Vec<f64> = klines.iter().map(|k| k.0).collect();

    // Hitung Bollinger Bands 50-period (Lebih luas & akurat memisahkan Sideways vs Trending)
    let bb_len = prices.len().min(50);
    let sma_bb: f64 = prices[0..bb_len].iter().sum::<f64>() / bb_len as f64;
    let variance: f64 = prices[0..bb_len].iter()
        .map(|&p| {
            let diff = p - sma_bb;
            diff * diff
        })
        .sum::<f64>() / bb_len as f64;
    let std_dev = variance.sqrt();
    
    // FASE 5.0b: Adaptive EMA Parameters
    // Deteksi apakah pasar sedang bergerak cepat (volatile) atau lambat
    let vol_pct = (std_dev / price) * 100.0;
    let (ema_fast_len, ema_slow_len) = if vol_pct > 0.15 {
        (9, 21) // Pasar Cepat: Gunakan EMA 9/21 agar lebih responsif
    } else {
        (13, 34) // Pasar Lambat: Gunakan EMA 13/34 untuk menyaring noise
    };

    let ema_fast = calculate_ema(&prices, ema_fast_len);
    let ema_slow = calculate_ema(&prices, ema_slow_len);

    // Hitung True Session VWAP (Akumulasi volume sejak jam 00:00 hari ini)
    let session_vwap: Option<f64> = sqlx::query_scalar(
        "SELECT SUM(close_price * volume) / NULLIF(SUM(volume), 0) FROM btc_klines WHERE open_time >= date_trunc('day', CURRENT_TIMESTAMP)"
    )
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
    let is_sudden_pump = last_change > (2.5 * std_dev) && std_dev > 5.0;
    let is_sudden_dump = last_change < -(2.5 * std_dev) && std_dev > 5.0;

    // Tentukan State Pasar (Trending vs Sideways) dengan batas volatilitas adaptif 50-period
    let volatility_pct = (std_dev / price) * 100.0;
    let is_sideways = volatility_pct < 0.085;

    let regime_str = if is_sideways {
        "SIDEWAYS".to_string()
    } else {
        if ema_fast > ema_slow {
            "BULLISH".to_string()
        } else {
            "BEARISH".to_string()
        }
    };
    *state.market_regime.write().await = regime_str.clone();

    // TAHAP 1: QPS Market Metrics Logging
    let _ = sqlx::query(
        "INSERT INTO qps_market_metrics_log (obi_value, volatility_pct, vol_surge, btc_price)
         VALUES ($1, $2, $3, $4)"
    )
    .bind(*state.order_book_imbalance.read().await)
    .bind(volatility_pct)
    .bind(vol_surge)
    .bind(price)
    .execute(&state.db)
    .await;

    println!(
        "[ANALIS] State: {} | EMA({}/{}): ${:.1}/${:.1} | VWAP Sesi: ${:.2} | Vol Surge: {:.1}x | BB50: [${:.2} - ${:.2}] | Tren 15m: {}", 
        regime_str, 
        ema_fast_len, ema_slow_len,
        ema_fast, ema_slow,
        vwap,
        vol_surge,
        lower_band, 
        upper_band,
        if trend_15m_bullish { "BULLISH" } else { "BEARISH" }
    );

    // --- PEMILIHAN STRATEGI BERDASARKAN KONDISI PASAR ---

    // A. DETEKSI DUMP MENDADAK (Defensive Action)
    if is_sudden_dump {
        if btc_bal > 0.0001 {
            println!("[ANALIS] ⚠️ DETEKSI DUMP MENDADAK! Mengamankan aset, JUAL semua.");
            return Decision::Sell(btc_bal, "[Darurat] Deteksi Dump Mendadak".to_string());
        }
    }

    // B. DETEKSI PUMP MENDADAK (Breakout Action)
    if is_sudden_pump {
        if sim_bal > 10.0 && trend_15m_bullish {
            println!("[ANALIS] ⚡ DETEKSI BREAKOUT PUMP! Masuk pasar segera.");
            
            // FASE 5.0a: Order Book Imbalance Check
            let obi = *state.order_book_imbalance.read().await;
            if obi < 0.40 {
                println!("[FASE 5.0a] OBI = {:.2} (Sell Wall dominan). Memblokir Breakout BUY untuk menghindari fake-out trap.", obi);
                return Decision::Wait;
            }

            let dynamic_pct = crate::risk::calculate_dynamic_budget(state, 0.25).await;
            if dynamic_pct <= 0.0 {
                println!("[QPS-RISK] Sharpe negatif. Memblokir entry Breakout untuk perlindungan modal.");
                return Decision::Wait;
            }
            let budget = sim_bal * dynamic_pct;
            return Decision::Buy(budget / price, "[Breakout] Pump Mendadak".to_string());
        }
    }

    // C. STRATEGI UTAMA: SIDEWAYS (Bollinger Bands 50 Mean Reversion)
    if is_sideways {
        let band_width_pct = ((upper_band - lower_band) / price) * 100.0;
        // Lebar BB minimal 1.0% untuk menjamin profit bersih menutupi fee & spread
        if band_width_pct < 1.0 {
            println!("[ANALIS-SIDEWAYS] WAIT - Lebar BB sempit ({:.3}%). Butuh min 1.0% untuk tutup fee & spread.", band_width_pct);
            return Decision::Wait;
        }

        if price <= lower_band {
            if sim_bal > 10.0 {
                println!("[ANALIS-SIDEWAYS] Harga menyentuh batas bawah BB50 (${:.2}). Spekulasi BUY.", lower_band);
                
                // FASE 5.0a: Order Book Imbalance Check
                let obi = *state.order_book_imbalance.read().await;
                if obi < 0.35 { // Sideways sedikit lebih toleran
                    println!("[FASE 5.0a] OBI = {:.2} (Sell Wall dominan). Memblokir Sideways BUY untuk menghindari breakdown trap.", obi);
                    return Decision::Wait;
                }

                let dynamic_pct = crate::risk::calculate_dynamic_budget(state, 0.15).await;
                if dynamic_pct <= 0.0 {
                    println!("[QPS-RISK] Sharpe negatif. Memblokir entry Sideways untuk perlindungan modal.");
                    return Decision::Wait;
                }
                let budget = sim_bal * dynamic_pct;
                return Decision::Buy(budget / price, "[Sideways] Sentuh Bawah BB50".to_string());
            }
        } else if price >= upper_band {
            if btc_bal > 0.0001 {
                println!("[ANALIS-SIDEWAYS] Harga menyentuh batas atas BB50 (${:.2}). Ambil profit SELL.", upper_band);
                return Decision::Sell(btc_bal, "[Sideways] Sentuh Atas BB50".to_string());
            }
        }
    } 
    // D. STRATEGI UTAMA: TRENDING (Adaptive EMA + VWAP Sesi + Konfirmasi 15m)
    else {
        let current_streak = { *state.ema_death_cross_streak.read().await };
        let diff = ema_fast - ema_slow;
        println!("[DEBUG] EMA streak: {} | EMA_FAST({}): {:.2} | EMA_SLOW({}): {:.2} | Selisih: {:.2}", current_streak, ema_fast_len, ema_fast, ema_slow_len, ema_slow, diff);

        // Golden Cross EMA_FAST > EMA_SLOW DAN harga di atas VWAP Sesi DAN Tren 15 Menit Bullish
        if ema_fast > ema_slow && price > vwap && trend_15m_bullish {
            *state.ema_death_cross_streak.write().await = 0; // reset streak on bullish signal
            if sim_bal > 10.0 {
                // FIX #2: Minimum volume surge untuk entry BUY trending
                // Data menunjukkan entry di Vol 0.0x-0.1x hampir selalu rugi (fake-out)
                if vol_surge < 0.5 {
                    println!("[ANALIS-TRENDING] WAIT - Volume terlalu rendah ({:.1}x). Min 0.5x rata-rata untuk entry.", vol_surge);
                    return Decision::Wait;
                }
                println!("[ANALIS-TRENDING] Quant Golden Cross (EMA{} > EMA{}), Di atas VWAP (${:.2}), & Tren 15m Bullish. Sinyal BUY.", ema_fast_len, ema_slow_len, vwap);
                
                // FASE 5.0a: Order Book Imbalance Check
                let obi = *state.order_book_imbalance.read().await;
                if obi < 0.40 {
                    println!("[FASE 5.0a] OBI = {:.2} (Sell Wall 60%+ dominan). Memblokir Trending BUY (Golden Cross ini berpotensi fake-out reversal trap).", obi);
                    return Decision::Wait;
                }

                let dynamic_pct = crate::risk::calculate_dynamic_budget(state, 0.20).await;
                if dynamic_pct <= 0.0 {
                    println!("[QPS-RISK] Sharpe negatif. Memblokir entry Trending untuk perlindungan modal.");
                    return Decision::Wait;
                }
                let budget = sim_bal * dynamic_pct;
                return Decision::Buy(budget / price, format!("[Trending] Adaptive EMA{}/{} Buy (Vol {:.1}x)", ema_fast_len, ema_slow_len, vol_surge));
            }
        } else if (ema_fast < ema_slow && price < vwap) || !trend_15m_bullish {
            if btc_bal > 0.0001 {
                let mut streak = state.ema_death_cross_streak.write().await;
                *streak += 1;
                let new_streak = *streak;
                // Drop write lock before print to be safe (although not strictly necessary here, it is clean)
                drop(streak);
                println!("[DEBUG] EMA streak: {} | EMA_FAST({}): {:.2} | EMA_SLOW({}): {:.2} | Selisih: {:.2} (After Increment)", new_streak, ema_fast_len, ema_fast, ema_slow_len, ema_slow, diff);
                
                if new_streak >= 2 {
                    *state.ema_death_cross_streak.write().await = 0; // reset streak after confirmation
                    println!("[ANALIS-TRENDING] Sinyal melemah/Death Cross terkonfirmasi 2 menit berturut. Sinyal SELL.");
                    return Decision::Sell(btc_bal, format!("[Trending] Adaptive EMA{}/{} Sell Confirmed (2m streak)", ema_fast_len, ema_slow_len));
                } else {
                    println!("[ANALIS-TRENDING] Menunggu konfirmasi streak 2 menit. Saat ini: 1 menit.");
                    return Decision::Wait;
                }
            } else {
                *state.ema_death_cross_streak.write().await = 0; // reset if we don't hold BTC
            }
        } else {
            *state.ema_death_cross_streak.write().await = 0; // reset on other neutral conditions
        }
    }

    Decision::Wait
}
