// INI ADALAH FILE sideways.rs
use crate::AppState;
use crate::conclude::Decision;

pub async fn evaluate_exit(
    symbol: &str,
    price: f64,
    btc_bal: f64,
    buy_reason: &str,
    klines: &[(f64, f64, chrono::DateTime<chrono::Utc>, f64, f64)],
    state: &AppState,
) -> Option<Decision> {
    if btc_bal > 0.0001 && buy_reason.starts_with("[Sideways]") {
        let cat = {
            let map = state.coin_states.read().await;
            map.get(symbol).map(|c| c.volatility_category.clone()).unwrap_or_else(|| "LOW".to_string())
        };
        let params = crate::classifier::get_params_for_category(&cat, symbol);
        let prices: Vec<f64> = klines.iter().map(|k| k.0).collect();
        let prices_bb = &prices[0..params.sideways_bb_period.min(prices.len())];
        let sma: f64 = prices_bb.iter().sum::<f64>() / prices_bb.len() as f64;
        let variance: f64 = prices_bb.iter()
            .map(|&p| {
                let diff = p - sma;
                diff * diff
            })
            .sum::<f64>() / prices_bb.len() as f64;
        let std_dev = variance.sqrt();
        let upper_band = sma + (params.sideways_bb_mult * std_dev);

        if price >= upper_band {
            println!("[{}] Harga menyentuh batas atas BB{} (${:.4}). Ambil profit SELL.", symbol, params.sideways_bb_period, upper_band);
            return Some(Decision::Sell(btc_bal, format!("[Sideways] Sentuh Atas BB{}", params.sideways_bb_period)));
        }
    }
    None
}

pub async fn evaluate_entry(
    symbol: &str,
    price: f64,
    sim_bal: f64,
    vwap: f64,
    klines: &[(f64, f64, chrono::DateTime<chrono::Utc>, f64, f64)],
    state: &AppState,
) -> Option<Decision> {
    let cat = {
        let map = state.coin_states.read().await;
        map.get(symbol).map(|c| c.volatility_category.clone()).unwrap_or_else(|| "LOW".to_string())
    };
    let params = crate::classifier::get_params_for_category(&cat, symbol);
    let prices: Vec<f64> = klines.iter().map(|k| k.0).collect();
    let prices_bb = &prices[0..params.sideways_bb_period.min(prices.len())];
    let sma: f64 = prices_bb.iter().sum::<f64>() / prices_bb.len() as f64;
    let variance: f64 = prices_bb.iter()
        .map(|&p| {
            let diff = p - sma;
            diff * diff
        })
        .sum::<f64>() / prices_bb.len() as f64;
    let std_dev = variance.sqrt();
    let lower_band = sma - (params.sideways_bb_mult * std_dev);
    
    let volatility_pct = (std_dev / price) * 100.0;
    
    let is_sideways = volatility_pct >= params.sideways_min_vol_pct && volatility_pct < params.sideways_max_vol_pct;

    if is_sideways && price <= lower_band {
        if sim_bal > 10.0 {
            // Hitung momentum penurunan 3 menit terakhir
            let rsi_slope_3m = crate::conclude::calculate_rsi_slope(klines, 14, 3);
            let price_drop_3m = (price - prices[3]) / prices[3] * 100.0;
            
            // Cek 3 filter eksklusif di mode optimized
            let is_too_sharp = price_drop_3m < -0.48; // Jatuh terlalu dalam
            let is_too_flat = price_drop_3m > -0.18 || rsi_slope_3m > -4.0; // Terlalu datar / kurang momentum turun

            if !is_too_sharp && !is_too_flat {
                let current_vol = klines[0].1;
                let sum_v: f64 = klines.iter().map(|(_, v, _, _, _)| v).sum();
                let avg_vol = sum_v / klines.len() as f64;
                let vol_surge = if avg_vol > 0.0 { current_vol / avg_vol } else { 1.0 };

                if vol_surge <= 1.5 {
                    let vwap_dist_sw = ((price - vwap) / vwap) * 100.0;
                    
                    // FASE 5.0a: Order Book Imbalance Check
                    let obi = {
                        let map = state.coin_states.read().await;
                        map.get(symbol).map(|c| c.order_book_imbalance).unwrap_or(0.5)
                    };
                    if obi < 0.35 {
                        println!("[{}] OBI = {:.2} (Sell Wall dominan). Memblokir Sideways BUY.", symbol, obi);
                        return Some(Decision::Wait);
                    }

                    // Budget flat 10% untuk Sideways agar aman
                    let base_pct = params.sideways_flat_budget_pct.unwrap_or(0.10);
                    let dynamic_pct = crate::risk::calculate_dynamic_budget(state, base_pct).await;
                    let budget = if dynamic_pct == -1.0 {
                        println!("[{}] QPS Aktif: Mode Pemulihan Modal Minimum ($5.05) berjalan.", symbol);
                        5.05
                    } else if dynamic_pct <= 0.0 {
                        println!("[{}] Sharpe negatif. Memblokir entry Sideways.", symbol);
                        return Some(Decision::Wait);
                    } else {
                        sim_bal * dynamic_pct
                    };
                    return Some(Decision::Buy(budget / price, format!("[Sideways] Sentuh Bawah BB{} (Vol {:.1}x, VWAP Dist {:.2}%)", params.sideways_bb_period, vol_surge, vwap_dist_sw)));
                }
            }
        }
    }
    None
}
