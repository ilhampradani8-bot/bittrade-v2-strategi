// INI ADALAH FILE uptrend.rs
use crate::AppState;
use crate::conclude::Decision;
use chrono::Timelike;

pub async fn evaluate_exit(
    symbol: &str,
    price: f64,
    btc_bal: f64,
    buy_reason: &str,
    ema_fast: f64,
    ema_slow: f64,
    vwap: f64,
    trend_15m_bullish: bool,
    ema_fast_len: usize,
    ema_slow_len: usize,
    state: &AppState,
) -> Option<Decision> {
    if btc_bal > 0.0001 && buy_reason.starts_with("[Trending]") {
        let is_sell_signal = (ema_fast < ema_slow) && ((price < vwap) || !trend_15m_bullish);
        if is_sell_signal {
            let mut streak = state.ema_death_cross_streak.write().await;
            *streak += 1;
            let new_streak = *streak;
            drop(streak);
            
            if new_streak >= 2 {
                *state.ema_death_cross_streak.write().await = 0; // reset streak after confirmation
                println!("[{}] Sinyal melemah/Death Cross terkonfirmasi 2 menit berturut. Sinyal SELL.", symbol);
                return Some(Decision::Sell(btc_bal, format!("[Trending] Adaptive EMA{}/{} Sell Confirmed (2m streak)", ema_fast_len, ema_slow_len)));
            } else {
                println!("[{}] Menunggu konfirmasi streak 2 menit. Saat ini: 1 menit.", symbol);
                return Some(Decision::Wait);
            }
        } else {
            *state.ema_death_cross_streak.write().await = 0;
        }
    }
    None
}

pub async fn evaluate_entry(
    symbol: &str,
    price: f64,
    sim_bal: f64,
    ema_fast: f64,
    ema_slow: f64,
    vwap: f64,
    trend_15m_bullish: bool,
    vol_surge: f64,
    ema_fast_len: usize,
    ema_slow_len: usize,
    ema_spread_pct: f64,
    klines: &[(f64, f64, chrono::DateTime<chrono::Utc>, f64, f64)],
    state: &AppState,
) -> Option<Decision> {
    let cat = {
        let map = state.coin_states.read().await;
        map.get(symbol).map(|c| c.volatility_category.clone()).unwrap_or_else(|| "LOW".to_string())
    };
    let params = crate::classifier::get_params_for_category(&cat, symbol);
    
    // 1. UPTREND LOGIC: Golden Cross EMA_FAST > EMA_SLOW DAN harga di atas VWAP Sesi DAN Tren 15 Menit Bullish
    if ema_fast > ema_slow && price > vwap && trend_15m_bullish {
        *state.ema_death_cross_streak.write().await = 0; // reset streak on bullish signal
        if sim_bal > 10.0 {
            let rsi_now = crate::conclude::calculate_rsi(klines, 14).unwrap_or(50.0);
            let vwap_dist_pct = ((price - vwap) / vwap) * 100.0;
            
            // LOSS FILTER 1: RSI lemah (50-55) + VWAP dist tipis (0.2-1.2%) → 100% LOSS PATTERN, blokir! (Only block for LOW category)
            if cat == "LOW" && rsi_now >= 50.0 && rsi_now <= 55.0 && vwap_dist_pct >= 0.2 && vwap_dist_pct <= 1.2 {
                println!("[{}] BLOKIR - Trending Lemas terdeteksi! RSI {:.1} (50-55) & VWAP Dist {:.2}% (0.2-1.2%). 100% LOSS pattern.", symbol, rsi_now, vwap_dist_pct);
                return Some(Decision::Wait);
            }

            // LOSS FILTER 2: RSI Slope < +2.5 dalam 3 candle terakhir = momentum melemah (Only block for LOW category)
            let rsi_slope = crate::conclude::calculate_rsi_slope(klines, 14, 3);
            if cat == "LOW" && rsi_slope < 2.5 {
                println!("[{}] BLOKIR - RSI Slope lemah ({:.2}). Min +2.5 di bawah limit.", symbol, rsi_slope);
                return Some(Decision::Wait);
            }

            // LOSS FILTER 3: Jam volatile (08-12 & 16-21 UTC) → perketat VWAP Distance ke vwap_max_volatile
            let current_hour = klines[0].2.time().hour();
            let is_volatile_hour = (8..=12).contains(&current_hour) || (16..=21).contains(&current_hour);
            let vwap_max_allowed = if is_volatile_hour { params.uptrend_vwap_max_volatile } else { params.uptrend_vwap_max_normal };

            // Minimum & Maximum volume surge untuk entry BUY trending
            if vol_surge < 0.5 {
                println!("[{}] WAIT - Volume terlalu rendah ({:.1}x). Min 0.5x rata-rata untuk entry.", symbol, vol_surge);
                return Some(Decision::Wait);
            }
            if vol_surge > 5.0 {
                println!("[{}] WAIT - Volume terlalu tinggi ({:.1}x). Maks 5.0x untuk menghindari puncak buying climax.", symbol, vol_surge);
                return Some(Decision::Wait);
            }

            // Anti-Whipsaw: EMA Spread must be >= uptrend_ema_spread_min to filter out noisy crossings
            let ema_spread_min_pct = params.uptrend_ema_spread_min * 100.0;
            if ema_spread_pct < ema_spread_min_pct {
                println!("[{}] WAIT - Jarak EMA sempit ({:.3}%). Butuh min {:.3}% untuk konfirmasi tren.", symbol, ema_spread_pct, ema_spread_min_pct);
                return Some(Decision::Wait);
            }

            // Overextension Filter: Price must not be > vwap_max_allowed above VWAP
            if vwap_dist_pct > vwap_max_allowed {
                println!("[{}] WAIT - Harga terlalu jauh di atas VWAP ({:.2}%). Maks {:.1}% {} saat ini.", symbol, vwap_dist_pct, vwap_max_allowed, if is_volatile_hour { "(jam volatile)" } else { "" });
                return Some(Decision::Wait);
            }

            // Durasi Golden Cross Check
            let mut gc_dur = 0;
            for i in 0..klines.len() {
                let prices_past = klines.iter().skip(i).map(|k| k.0).collect::<Vec<f64>>();
                let ema_f_past = crate::conclude::calculate_ema(&prices_past, ema_fast_len);
                let ema_s_past = crate::conclude::calculate_ema(&prices_past, ema_slow_len);
                if ema_f_past > ema_s_past {
                    gc_dur += 1;
                } else {
                    break;
                }
            }
            if gc_dur > params.uptrend_gc_max_dur {
                println!("[{}] WAIT - Golden Cross sudah terlalu lama ({}m > {}m). Berisiko beli di pucuk.", symbol, gc_dur, params.uptrend_gc_max_dur);
                return Some(Decision::Wait);
            }

            // RSI Range Check
            if rsi_now < params.uptrend_rsi_min || rsi_now > params.uptrend_rsi_max {
                println!("[{}] WAIT - RSI {:.1} di luar range aman ({}-{}).", symbol, rsi_now, params.uptrend_rsi_min, params.uptrend_rsi_max);
                return Some(Decision::Wait);
            }

            // RSI Slope 15m >= uptrend_min_rsi_slope_15m
            let rsi_slope_15m = crate::conclude::calculate_rsi_slope(klines, 14, 15);
            if rsi_slope_15m < params.uptrend_min_rsi_slope_15m {
                println!("[{}] WAIT - RSI Slope 15m {:.1} kurang dari +{:.1}.", symbol, rsi_slope_15m, params.uptrend_min_rsi_slope_15m);
                return Some(Decision::Wait);
            }

            // Volume Surge 3m >= uptrend_min_vol_surge_3m
            let mut sum_v3 = 0.0;
            for i in 0..3.min(klines.len()) {
                sum_v3 += klines[i].1;
            }
            let avg_v3 = sum_v3 / 3.0;
            let vol_surge_3m = if avg_v3 > 0.0 { klines[0].1 / avg_v3 } else { 1.0 };
            if vol_surge_3m < params.uptrend_min_vol_surge_3m {
                println!("[{}] WAIT - Volume Surge 3m ({:.1}x) kurang dari {:.1}x.", symbol, vol_surge_3m, params.uptrend_min_vol_surge_3m);
                return Some(Decision::Wait);
            }

            println!("[{}] ✅ RSI Slope +{:.2} | VWAP Dist {:.2}% (<={:.1}%) | Vol {:.1}x. Sinyal BUY valid.", symbol, rsi_slope, vwap_dist_pct, vwap_max_allowed, vol_surge);
            
            // FASE 5.0a: Order Book Imbalance Check
            let obi = {
                let map = state.coin_states.read().await;
                map.get(symbol).map(|c| c.order_book_imbalance).unwrap_or(0.5)
            };
            if obi < 0.40 {
                println!("[{}] OBI = {:.2} (Sell Wall 60%+ dominan). Memblokir Trending BUY.", symbol, obi);
                return Some(Decision::Wait);
            }

            // Dynamic Sizing: 40% modal jika RSI Slope 15m >= 8.0, jika tidak 10% modal
            let base_pct = if rsi_slope_15m >= 8.0 { 0.40 } else { 0.10 };
            let dynamic_pct = crate::risk::calculate_dynamic_budget(state, base_pct).await;
            let budget = if dynamic_pct == -1.0 {
                println!("[{}] QPS Aktif: Mode Pemulihan Modal Minimum ($5.05) berjalan.", symbol);
                5.05
            } else if dynamic_pct <= 0.0 {
                println!("[{}] Sharpe negatif. Memblokir entry Trending.", symbol);
                return Some(Decision::Wait);
            } else {
                sim_bal * dynamic_pct
            };
            return Some(Decision::Buy(budget / price, format!("[Trending] Adaptive EMA{}/{} Buy (RSI Slope +{:.2}, Vol {:.1}x, VWAP Dist: {:.2}%)", ema_fast_len, ema_slow_len, rsi_slope, vol_surge, vwap_dist_pct)));
        }
    }
    None
}
