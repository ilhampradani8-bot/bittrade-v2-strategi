// INI ADALAH FILE downtrend.rs
use crate::AppState;
use crate::conclude::Decision;

pub async fn evaluate_exit(
    symbol: &str,
    price: f64,
    btc_bal: f64,
    buy_p: f64,
    buy_reason: &str,
    buy_time: chrono::DateTime<chrono::Utc>,
    ema_fast: f64,
    ema_slow: f64,
    state: &AppState,
) -> Option<Decision> {
    if btc_bal > 0.0001 && buy_reason.starts_with("[Downtrend]") {
        let cat = {
            let map = state.coin_states.read().await;
            map.get(symbol).map(|c| c.volatility_category.clone()).unwrap_or_else(|| "LOW".to_string())
        };
        let params = crate::classifier::get_params_for_category(&cat, symbol);
        let tp_pct = params.downtrend_tp.abs();
        
        // Micro Take Profit (+tp_pct profit dari harga entry)
        if (price - buy_p) / buy_p >= tp_pct {
            println!("[{}] Micro Take Profit +{:.2}% Terpenuhi! Entry: ${:.4} -> Saat Ini: ${:.4}", symbol, tp_pct * 100.0, buy_p, price);
            return Some(Decision::Sell(btc_bal, format!("[Downtrend] Micro Take Profit +{:.2}%", tp_pct * 100.0)));
        }

        // Downtrend Normal Exit: Death Cross terkonfirmasi 2 menit berturut-turut + lock time
        let duration = chrono::Utc::now().signed_duration_since(buy_time);
        let can_normal_sell = duration.num_seconds() >= params.downtrend_hold_lock;
        
        let is_sell_signal = ema_fast < ema_slow;
        if is_sell_signal {
            let mut streak = state.ema_death_cross_streak.write().await;
            *streak += 1;
            let new_streak = *streak;
            drop(streak);
            
            if new_streak >= 2 && can_normal_sell {
                *state.ema_death_cross_streak.write().await = 0; // reset streak
                println!("[{}] Re-death cross terkonfirmasi (2m streak). Sinyal SELL.", symbol);
                return Some(Decision::Sell(btc_bal, "[Downtrend] Re-Death Cross (2m streak)".to_string()));
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
    ema_fast: f64,
    ema_slow: f64,
    vwap: f64,
    vol_surge: f64,
    upper_band: f64,
    lower_band: f64,
    klines: &[(f64, f64, chrono::DateTime<chrono::Utc>, f64, f64)],
    state: &AppState,
) -> Option<Decision> {
    if ema_fast < ema_slow {
        let cat = {
            let map = state.coin_states.read().await;
            map.get(symbol).map(|c| c.volatility_category.clone()).unwrap_or_else(|| "LOW".to_string())
        };
        let params = crate::classifier::get_params_for_category(&cat, symbol);
        let rsi = crate::conclude::calculate_rsi(klines, 14).unwrap_or(50.0);
        let rsi_oversold = rsi < params.downtrend_rsi_limit;
        
        // Reversal confirmation: 2-streak green candle (current close > prev close && prev close > prev-prev close)
        let is_green_streak = klines.len() >= 3 && klines[0].0 > klines[1].0 && klines[1].0 > klines[2].0;
        let bb_width_pct = (upper_band - lower_band) / price * 100.0;
        let vwap_dist_dt = ((price - vwap) / vwap) * 100.0;

        // Ditambah filter VWAP diskon <= downtrend_max_vwap_dist
        if rsi_oversold && vol_surge >= params.downtrend_vol_surge_limit && is_green_streak && bb_width_pct > 0.5 && vwap_dist_dt <= params.downtrend_max_vwap_dist {
            if sim_bal > 5.0 {
                let dynamic_pct = crate::risk::calculate_dynamic_budget(state, 0.30).await;
                let budget = if dynamic_pct == -1.0 {
                    println!("[{}] QPS Aktif: Mode Pemulihan Modal Minimum ($5.05) berjalan.", symbol);
                    5.05
                } else if dynamic_pct <= 0.0 {
                    println!("[{}] Sharpe negatif. Memblokir entry Downtrend.", symbol);
                    return None;
                } else {
                    sim_bal * dynamic_pct
                };
                println!("[{}] ⚡ BEARISH CLIMAX REBOUND TERDETEKSI! RSI Oversold ({:.1}), Vol Surge ({:.1}x), VWAP Dist {:.2}% (Diskon >= {:.2}%).", symbol, rsi, vol_surge, vwap_dist_dt, params.downtrend_max_vwap_dist.abs());
                return Some(Decision::Buy(budget / price, format!("[Downtrend] Bearish Climax Rebound (RSI {:.1}, VWAP Dist {:.2}%)", rsi, vwap_dist_dt)));
            }
        }
    }
    None
}
