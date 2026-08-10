// INI ADALAH FILE breakout.rs
use crate::AppState;
use crate::conclude::Decision;

pub async fn evaluate_exit(
    symbol: &str,
    price: f64,
    btc_bal: f64,
    buy_reason: &str,
    buy_time: chrono::DateTime<chrono::Utc>,
    ema_fast: f64,
    ema_slow: f64,
    vwap: f64,
    trend_15m_bullish: bool,
    state: &AppState,
) -> Option<Decision> {
    if btc_bal > 0.0001 && buy_reason.starts_with("[Breakout]") {
        let cat = {
            let map = state.coin_states.read().await;
            map.get(symbol).map(|c| c.volatility_category.clone()).unwrap_or_else(|| "LOW".to_string())
        };
        let params = crate::classifier::get_params_for_category(&cat, symbol);
        let duration = chrono::Utc::now().signed_duration_since(buy_time);
        let can_normal_sell = duration.num_seconds() >= params.uptrend_lock_duration;

        // Sinyal exit breakout: (ema_fast < ema_slow && price < vwap) atau tren 15m bearish
        let is_sell_signal = (ema_fast < ema_slow && price < vwap) || !trend_15m_bullish;
        if is_sell_signal {
            let mut streak = state.ema_death_cross_streak.write().await;
            *streak += 1;
            let new_streak = *streak;
            drop(streak);

            if new_streak >= 2 && can_normal_sell {
                *state.ema_death_cross_streak.write().await = 0;
                println!("[{}] Sinyal exit terkonfirmasi (2m streak). Sinyal SELL.", symbol);
                return Some(Decision::Sell(btc_bal, "[Breakout] Quant Exit Conditions confirmed".to_string()));
            } else if is_sell_signal {
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
    vwap: f64,
    std_dev: f64,
    vol_surge: f64,
    ema_fast: f64,
    ema_slow: f64,
    trend_15m_bullish: bool,
    prices: &[f64],
    klines: &[(f64, f64, chrono::DateTime<chrono::Utc>, f64, f64)],
    state: &AppState,
) -> Option<Decision> {
    let cat = {
        let map = state.coin_states.read().await;
        map.get(symbol).map(|c| c.volatility_category.clone()).unwrap_or_else(|| "LOW".to_string())
    };
    let params = crate::classifier::get_params_for_category(&cat, symbol);
    let last_change = prices[0] - prices[1];
    
    // 1. Pemicu awal: lonjakan di atas 2.5 std dev & volatilitas minimum std dev
    let std_dev_pct = (std_dev / price) * 100.0;
    let is_sudden_pump = last_change > (2.5 * std_dev) && std_dev_pct >= params.breakout_min_std_dev;

    if is_sudden_pump && sim_bal > 10.0 && trend_15m_bullish {
        let spike_pct_bo = (last_change / prices[1]) * 100.0;
        let vwap_dist_bo = ((price - vwap) / vwap) * 100.0;

        // A. Cek filter dasar optimized
        let is_spike_ok = spike_pct_bo >= params.breakout_min_spike_pct;
        let rsi_now = crate::conclude::calculate_rsi(klines, 14).unwrap_or(50.0);
        let is_rsi_ok = rsi_now >= params.breakout_min_rsi;

        if is_spike_ok && is_rsi_ok {
            // B. Anti Fake-Breakout Filter
            let is_fake_breakout = vol_surge <= 3.0 && vwap_dist_bo <= 0.8;
            if is_fake_breakout {
                println!("[{}] BLOKIR - Fake Breakout terdeteksi! Vol Surge {:.1}x (<=3x) & VWAP Dist {:.2}% (<=0.8%).", symbol, vol_surge, vwap_dist_bo);
                return Some(Decision::Wait);
            }

            // C. Filter Atas VWAP (max_vwap_dist)
            if vwap_dist_bo > params.breakout_max_vwap_dist {
                println!("[{}] BLOKIR - Harga terlalu jauh di atas VWAP ({:.2}% > {:.2}%).", symbol, vwap_dist_bo, params.breakout_max_vwap_dist);
                return Some(Decision::Wait);
            }

            // D. Filter konfirmasi gap EMA (jika spike besar, gap EMA harus >= breakout_ema_gap_if_big)
            let e13_gap_pct = (ema_fast - ema_slow) / ema_slow * 100.0;
            if spike_pct_bo > 0.6 && e13_gap_pct < params.breakout_ema_gap_if_big {
                println!("[{}] BLOKIR - Spike besar ({:.2}%) tapi EMA gap lemah ({:.3}% < {:.3}%).", symbol, spike_pct_bo, e13_gap_pct, params.breakout_ema_gap_if_big);
                return Some(Decision::Wait);
            }

            // FASE 5.0a: Order Book Imbalance Check
            let obi = {
                let map = state.coin_states.read().await;
                map.get(symbol).map(|c| c.order_book_imbalance).unwrap_or(0.5)
            };
            if obi < 0.40 {
                println!("[{}] OBI = {:.2} (Sell Wall dominan). Memblokir Breakout BUY.", symbol, obi);
                return Some(Decision::Wait);
            }

            let dynamic_pct = crate::risk::calculate_dynamic_budget(state, 0.25).await;
            let budget = if dynamic_pct == -1.0 {
                println!("[{}] QPS Aktif: Mode Pemulihan Modal Minimum ($5.05) berjalan.", symbol);
                5.05
            } else if dynamic_pct <= 0.0 {
                println!("[{}] Sharpe negatif. Memblokir entry Breakout.", symbol);
                return Some(Decision::Wait);
            } else {
                sim_bal * dynamic_pct
            };
            return Some(Decision::Buy(budget / price, format!("[Breakout] Pump Mendadak (Vol {:.1}x, VWAP Dist {:.2}%, Spike {:.2}%)", vol_surge, vwap_dist_bo, spike_pct_bo)));
        }
    }
    None
}
