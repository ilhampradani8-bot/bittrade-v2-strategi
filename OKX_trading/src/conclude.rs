use crate::AppState;

#[derive(Debug, Clone)]
pub enum Decision {
    Buy(f64, String),
    Sell(f64, String),
    Wait,
}

// Helper: Menghitung Exponential Moving Average (EMA)
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

    // 1. Cek Pengaman (Stop Loss 1.2% & Trailing Take Profit Dinamis Pullback 1.0%)
    let last_buy: Option<(f64, chrono::DateTime<chrono::Utc>)> = sqlx::query_as::<_, (f64, chrono::DateTime<chrono::Utc>)>(
        "SELECT price, timestamp FROM okx_trading_history WHERE action = 'BUY' AND status = 'SUCCESS' ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    if let Some((buy_p, _buy_time)) = last_buy {
        if btc_bal > 0.0001 {
            // A. Emergency Stop Loss (-1.2%)
            if price <= buy_p * 0.988 {
                println!("[ANALIS] 🚨 EMERGENCY STOP LOSS TERPICU! Harga Beli: ${:.2} | Harga Saat Ini: ${:.2} (Turun >= 1.2%)", buy_p, price);
                return Decision::Sell(btc_bal, "[Darurat] Stop Loss -1.2%".to_string());
            }

            // B. Trailing Take Profit Dinamis
            let mut hwm = state.high_water_mark.write().await;
            if price > *hwm {
                *hwm = price;
                let db_clone = state.db.clone();
                tokio::spawn(async move {
                    let _ = sqlx::query("UPDATE okx_active_positions SET high_water_mark = $1 WHERE high_water_mark < $1").bind(price).execute(&db_clone).await;
                });
            }
            let peak_price = (*hwm).max(price).max(buy_p);
            let profit_pct_from_peak = ((peak_price - price) / peak_price) * 100.0;
            let profit_pct_from_buy = ((price - buy_p) / buy_p) * 100.0;
            let peak_profit_pct = ((peak_price - buy_p) / buy_p) * 100.0;

            // Jika harga sempat naik minimal +1.5% dari harga beli, trailing aktif
            if peak_profit_pct >= 1.5 {
                if profit_pct_from_peak >= 1.0 {
                    println!("[ANALIS] 🎯 TRAILING TAKE PROFIT TERPICU! Puncak: ${:.2} (+{:.2}%) | Saat Ini: ${:.2} (+{:.2}%) | Turun {:.2}% dari pucuk.", peak_price, peak_profit_pct, price, profit_pct_from_buy, profit_pct_from_peak);
                    return Decision::Sell(btc_bal, format!("[Darurat] Trailing Take Profit (Puncak Cuaca +{:.1}%)", peak_profit_pct));
                }
            } else if profit_pct_from_buy >= 3.0 {
                // Hard Take Profit
                println!("[ANALIS] 🎯 TAKE PROFIT TERPICU! Harga Beli: ${:.2} | Harga Saat Ini: ${:.2} (Naik >= 3%)", buy_p, price);
                return Decision::Sell(btc_bal, "[Darurat] Take Profit +3%".to_string());
            }
        }
    }

    // 2. Tarik data kline historis 50 candle untuk konteks pasar yang mendalam
    let klines: Vec<(f64, f64, chrono::DateTime<chrono::Utc>)> = sqlx::query_as::<_, (f64, f64, chrono::DateTime<chrono::Utc>)>(
        "SELECT close_price, volume, open_time FROM okx_klines ORDER BY open_time DESC LIMIT 50"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

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

    // Hitung EMA-13 & EMA-34
    let ema_13 = calculate_ema(&prices, 13);
    let ema_34 = calculate_ema(&prices, 34);

    // Hitung True Session VWAP (Akumulasi volume sejak jam 00:00 hari ini)
    let session_vwap: Option<f64> = sqlx::query_scalar(
        "SELECT SUM(close_price * volume) / NULLIF(SUM(volume), 0) FROM okx_klines WHERE open_time >= date_trunc('day', CURRENT_TIMESTAMP)"
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let vwap = session_vwap.unwrap_or(price);
    
    let current_vol = klines[0].1;
    let sum_v: f64 = klines.iter().map(|(_, v, _)| v).sum();
    let avg_vol = sum_v / klines.len() as f64;
    let vol_surge = if avg_vol > 0.0 { current_vol / avg_vol } else { 1.0 };

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

    let upper_band = sma_bb + (2.0 * std_dev);
    let lower_band = sma_bb - (2.0 * std_dev);

    // Filter Multi-Timeframe
    let trend_15m_bullish = klines.len() > 15 && prices[0] > prices[15];

    // Deteksi Perubahan Mendadak
    let last_change = prices[0] - prices[1];
    let is_sudden_pump = last_change > (2.5 * std_dev) && std_dev > 5.0;
    let is_sudden_dump = last_change < -(2.5 * std_dev) && std_dev > 5.0;

    // Tentukan State Pasar
    let volatility_pct = (std_dev / price) * 100.0;
    let is_sideways = volatility_pct < 0.085;

    let regime_str = if is_sideways {
        "SIDEWAYS".to_string()
    } else {
        if ema_13 > ema_34 {
            "BULLISH".to_string()
        } else {
            "BEARISH".to_string()
        }
    };
    *state.market_regime.write().await = regime_str.clone();

    println!(
        "[ANALIS] State: {} | EMA13/34: ${:.1}/${:.1} | VWAP Sesi: ${:.2} | Vol Surge: {:.1}x | BB50: [${:.2} - ${:.2}] | Tren 15m: {}", 
        regime_str, 
        ema_13,
        ema_34,
        vwap,
        vol_surge,
        lower_band, 
        upper_band,
        if trend_15m_bullish { "BULLISH" } else { "BEARISH" }
    );

    // A. DETEKSI DUMP MENDADAK
    if is_sudden_dump {
        if btc_bal > 0.0001 {
            println!("[ANALIS] ⚠️ DETEKSI DUMP MENDADAK! Mengamankan aset, JUAL semua.");
            return Decision::Sell(btc_bal, "[Darurat] Deteksi Dump Mendadak".to_string());
        }
    }

    // B. DETEKSI PUMP MENDADAK
    if is_sudden_pump {
        if sim_bal > 10.0 && trend_15m_bullish {
            println!("[ANALIS] ⚡ DETEKSI BREAKOUT PUMP! Masuk pasar segera.");
            let budget = sim_bal * 0.25;
            return Decision::Buy(budget / price, "[Breakout] Pump Mendadak".to_string());
        }
    }

    // C. STRATEGI UTAMA: SIDEWAYS
    if is_sideways {
        let band_width_pct = ((upper_band - lower_band) / price) * 100.0;
        if band_width_pct < 1.0 {
            println!("[ANALIS-SIDEWAYS] WAIT - Lebar BB sempit ({:.3}%).", band_width_pct);
            return Decision::Wait;
        }

        if price <= lower_band {
            if sim_bal > 10.0 {
                println!("[ANALIS-SIDEWAYS] Harga menyentuh batas bawah BB50 (${:.2}). Spekulasi BUY.", lower_band);
                let budget = sim_bal * 0.15;
                return Decision::Buy(budget / price, "[Sideways] Sentuh Bawah BB50".to_string());
            }
        } else if price >= upper_band {
            if btc_bal > 0.0001 {
                println!("[ANALIS-SIDEWAYS] Harga menyentuh batas atas BB50 (${:.2}). Ambil profit SELL.", upper_band);
                return Decision::Sell(btc_bal, "[Sideways] Sentuh Atas BB50".to_string());
            }
        }
    } 
    // D. STRATEGI UTAMA: TRENDING
    else {
        let current_streak = { *state.ema_death_cross_streak.read().await };
        let diff = ema_13 - ema_34;
        println!("[DEBUG] EMA streak: {} | EMA13: {:.2} | EMA34: {:.2} | Selisih: {:.2}", current_streak, ema_13, ema_34, diff);

        if ema_13 > ema_34 && price > vwap && trend_15m_bullish {
            *state.ema_death_cross_streak.write().await = 0;
            if sim_bal > 10.0 {
                println!("[ANALIS-TRENDING] Quant Golden Cross (EMA13 > EMA34), Di atas VWAP (${:.2}), & Tren 15m Bullish. Sinyal BUY.", vwap);
                let budget = sim_bal * 0.20;
                return Decision::Buy(budget / price, format!("[Trending] Quant EMA13/34 Buy (Vol {:.1}x)", vol_surge));
            }
        } else if (ema_13 < ema_34 && price < vwap) || !trend_15m_bullish {
            if btc_bal > 0.0001 {
                let mut streak = state.ema_death_cross_streak.write().await;
                *streak += 1;
                let new_streak = *streak;
                drop(streak);
                
                if new_streak >= 2 {
                    *state.ema_death_cross_streak.write().await = 0;
                    println!("[ANALIS-TRENDING] Sinyal melemah/Death Cross terkonfirmasi 2 menit berturut. Sinyal SELL.");
                    return Decision::Sell(btc_bal, "[Trending] Quant EMA13/34 Sell Confirmed (2m streak)".to_string());
                } else {
                    println!("[ANALIS-TRENDING] Menunggu konfirmasi streak 2 menit. Saat ini: 1 menit.");
                    return Decision::Wait;
                }
            } else {
                *state.ema_death_cross_streak.write().await = 0;
            }
        } else {
            *state.ema_death_cross_streak.write().await = 0;
        }
    }

    Decision::Wait
}
