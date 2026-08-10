use std::collections::HashMap;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct VolatilityParams {
    pub stop_loss_limit: f64,
    
    // Uptrend
    pub uptrend_gc_max_dur: usize,
    pub uptrend_rsi_min: f64,
    pub uptrend_rsi_max: f64,
    pub uptrend_block_rsi_75_80: bool,
    pub uptrend_max_rsi_slope_7m: f64,
    pub uptrend_min_rsi_slope_15m: f64,
    pub uptrend_min_vol_surge_3m: f64,
    pub uptrend_is_dynamic_sizing: bool,
    pub uptrend_tp_trail_trigger: f64,
    pub uptrend_tp_trail_pullback: f64,
    pub uptrend_vwap_max_normal: f64,
    pub uptrend_vwap_max_volatile: f64,
    pub uptrend_ema_spread_min: f64,
    pub uptrend_lock_duration: i64,

    // Sideways
    pub sideways_bb_period: usize,
    pub sideways_bb_mult: f64,
    pub sideways_min_vol_pct: f64,
    pub sideways_max_vol_pct: f64,
    pub sideways_flat_budget_pct: Option<f64>,

    // Downtrend
    pub downtrend_max_vwap_dist: f64,
    pub downtrend_tp: f64,
    pub downtrend_hold_lock: i64,
    pub downtrend_stop_loss: f64,
    pub downtrend_rsi_limit: f64,
    pub downtrend_vol_surge_limit: f64,

    // Breakout
    pub breakout_min_std_dev: f64,
    pub breakout_min_spike_pct: f64,
    pub breakout_min_rsi: f64,
    pub breakout_max_vwap_dist: f64,
    pub breakout_ema_gap_if_big: f64,
}

pub async fn classify_coin_rust(symbol: &str) -> String {
    let sym = symbol.to_uppercase().trim().to_string();
    
    // 2. Fetch klines from Binance API
    let url = format!("https://api.binance.com/api/v3/klines?symbol={}&interval=1m&limit=500", sym);
    match reqwest::get(&url).await {
        Ok(resp) => {
            if let Ok(candles) = resp.json::<Vec<Vec<Value>>>().await {
                let closes: Vec<f64> = candles.iter()
                    .filter_map(|c| c.get(4).and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok()))
                    .collect();
                    
                if closes.len() >= 50 {
                    let mut vols = Vec::new();
                    for i in (50..closes.len()).step_by(10) {
                        let chunk = &closes[i-50..i];
                        let mean = chunk.iter().sum::<f64>() / chunk.len() as f64;
                        if mean > 0.0 {
                            let variance = chunk.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / (chunk.len() - 1) as f64;
                            let std_dev = variance.sqrt();
                            vols.push((std_dev / mean) * 100.0);
                        }
                    }
                    
                    let avg_vol = if !vols.is_empty() { vols.iter().sum::<f64>() / vols.len() as f64 } else { 0.0 };
                    
                    if avg_vol >= 3.0 {
                        return "EXTREME".to_string();
                    } else if avg_vol >= 1.5 {
                        return "HYPER".to_string();
                    } else if avg_vol >= 0.5 {
                        return "HIGH".to_string();
                    } else if avg_vol >= 0.2 {
                        return "MEDIUM".to_string();
                    } else {
                        return "LOW".to_string();
                    }
                }
            }
        }
        Err(_) => {}
    }
    
    // Default fallback
    if sym.contains("BTC") || sym.contains("ETH") {
        "LOW".to_string()
    } else {
        "HIGH".to_string()
    }
}

use std::sync::OnceLock;
use std::sync::RwLock;

pub static DYNAMIC_PARAMS: OnceLock<RwLock<HashMap<String, VolatilityParams>>> = OnceLock::new();

pub fn get_params_cache() -> &'static RwLock<HashMap<String, VolatilityParams>> {
    DYNAMIC_PARAMS.get_or_init(|| RwLock::new(HashMap::new()))
}

pub fn get_params_for_category(category: &str, symbol: &str) -> VolatilityParams {
    if let Ok(cache) = get_params_cache().read() {
        if let Some(params) = cache.get(category) {
            return params.clone();
        }
    }
    get_default_params_for_category(category, symbol)
}

pub fn get_default_params_for_category(category: &str, symbol: &str) -> VolatilityParams {
    let sym = symbol.to_uppercase();
    match category {
        "EXTREME" => VolatilityParams {
            stop_loss_limit: -0.070,
            uptrend_gc_max_dur: 35,
            uptrend_rsi_min: 55.0,
            uptrend_rsi_max: 75.0,
            uptrend_block_rsi_75_80: false,
            uptrend_max_rsi_slope_7m: 999.0,
            uptrend_min_rsi_slope_15m: 12.0,
            uptrend_min_vol_surge_3m: 0.5,
            uptrend_is_dynamic_sizing: true,
            uptrend_tp_trail_trigger: 0.030,
            uptrend_tp_trail_pullback: 0.010,
            uptrend_vwap_max_normal: 4.5,
            uptrend_vwap_max_volatile: 4.5,
            uptrend_ema_spread_min: 0.0025,
            uptrend_lock_duration: 2400,
            sideways_bb_period: 20,
            sideways_bb_mult: 2.5,
            sideways_min_vol_pct: 0.35,
            sideways_max_vol_pct: 1.20,
            sideways_flat_budget_pct: Some(0.10),
            downtrend_max_vwap_dist: -0.30,
            downtrend_tp: 0.05,
            downtrend_hold_lock: 1800,
            downtrend_stop_loss: -0.05,
            downtrend_rsi_limit: 35.0,
            downtrend_vol_surge_limit: 1.0,
            breakout_min_std_dev: 0.03,
            breakout_min_spike_pct: 0.3,
            breakout_min_rsi: 60.0,
            breakout_max_vwap_dist: 3.5,
            breakout_ema_gap_if_big: 0.05,
        },
        "HYPER" => VolatilityParams {
            stop_loss_limit: -0.050,
            uptrend_gc_max_dur: 35,
            uptrend_rsi_min: 55.0,
            uptrend_rsi_max: 75.0,
            uptrend_block_rsi_75_80: false,
            uptrend_max_rsi_slope_7m: 999.0,
            uptrend_min_rsi_slope_15m: if sym == "BICOUSDT" { 10.0 } else { 6.0 },
            uptrend_min_vol_surge_3m: 0.5,
            uptrend_is_dynamic_sizing: true,
            uptrend_tp_trail_trigger: 0.020,
            uptrend_tp_trail_pullback: 0.007,
            uptrend_vwap_max_normal: 3.5,
            uptrend_vwap_max_volatile: 3.5,
            uptrend_ema_spread_min: 0.0020,
            uptrend_lock_duration: 1800,
            sideways_bb_period: 20,
            sideways_bb_mult: 2.0,
            sideways_min_vol_pct: 0.30,
            sideways_max_vol_pct: 1.00,
            sideways_flat_budget_pct: Some(0.10),
            downtrend_max_vwap_dist: -0.50,
            downtrend_tp: 0.03,
            downtrend_hold_lock: 1200,
            downtrend_stop_loss: -0.03,
            downtrend_rsi_limit: 35.0,
            downtrend_vol_surge_limit: 1.5,
            breakout_min_std_dev: 0.02,
            breakout_min_spike_pct: 0.2,
            breakout_min_rsi: 60.0,
            breakout_max_vwap_dist: 2.5,
            breakout_ema_gap_if_big: 0.05,
        },
        "HIGH" => VolatilityParams {
            stop_loss_limit: -0.035,
            uptrend_gc_max_dur: 35,
            uptrend_rsi_min: 55.0,
            uptrend_rsi_max: 75.0,
            uptrend_block_rsi_75_80: false,
            uptrend_max_rsi_slope_7m: 999.0,
            uptrend_min_rsi_slope_15m: 6.0,
            uptrend_min_vol_surge_3m: 0.5,
            uptrend_is_dynamic_sizing: true,
            uptrend_tp_trail_trigger: 0.015,
            uptrend_tp_trail_pullback: 0.005,
            uptrend_vwap_max_normal: 3.5,
            uptrend_vwap_max_volatile: 3.5,
            uptrend_ema_spread_min: 0.0015,
            uptrend_lock_duration: 1200,
            sideways_bb_period: 20,
            sideways_bb_mult: 2.0,
            sideways_min_vol_pct: 0.20,
            sideways_max_vol_pct: 0.80,
            sideways_flat_budget_pct: Some(0.10),
            downtrend_max_vwap_dist: -0.50,
            downtrend_tp: 0.03,
            downtrend_hold_lock: 1200,
            downtrend_stop_loss: -0.03,
            downtrend_rsi_limit: 35.0,
            downtrend_vol_surge_limit: 1.5,
            breakout_min_std_dev: 0.02,
            breakout_min_spike_pct: 0.2,
            breakout_min_rsi: 60.0,
            breakout_max_vwap_dist: 2.5,
            breakout_ema_gap_if_big: 0.05,
        },
        "MEDIUM" => VolatilityParams {
            stop_loss_limit: -0.020,
            uptrend_gc_max_dur: 35,
            uptrend_rsi_min: 60.0,
            uptrend_rsi_max: 72.0,
            uptrend_block_rsi_75_80: false,
            uptrend_max_rsi_slope_7m: 999.0,
            uptrend_min_rsi_slope_15m: 4.0,
            uptrend_min_vol_surge_3m: 0.6,
            uptrend_is_dynamic_sizing: true,
            uptrend_tp_trail_trigger: 0.010,
            uptrend_tp_trail_pullback: 0.004,
            uptrend_vwap_max_normal: 2.5,
            uptrend_vwap_max_volatile: 1.5,
            uptrend_ema_spread_min: 0.0010,
            uptrend_lock_duration: 900,
            sideways_bb_period: 20,
            sideways_bb_mult: 2.2,
            sideways_min_vol_pct: 0.15,
            sideways_max_vol_pct: 0.30,
            sideways_flat_budget_pct: Some(0.10),
            downtrend_max_vwap_dist: -0.60,
            downtrend_tp: 0.015,
            downtrend_hold_lock: 900,
            downtrend_stop_loss: -0.02,
            downtrend_rsi_limit: 32.0,
            downtrend_vol_surge_limit: 2.0,
            breakout_min_std_dev: 0.01,
            breakout_min_spike_pct: 0.1,
            breakout_min_rsi: 62.0,
            breakout_max_vwap_dist: 2.0,
            breakout_ema_gap_if_big: 0.05,
        },
        _ => VolatilityParams { // LOW (Default / BTCUSDT)
            stop_loss_limit: -0.015,
            uptrend_gc_max_dur: 35,
            uptrend_rsi_min: 64.0,
            uptrend_rsi_max: 70.0,
            uptrend_block_rsi_75_80: false,
            uptrend_max_rsi_slope_7m: 999.0,
            uptrend_min_rsi_slope_15m: 5.0,
            uptrend_min_vol_surge_3m: 0.8,
            uptrend_is_dynamic_sizing: true,
            uptrend_tp_trail_trigger: 0.006,
            uptrend_tp_trail_pullback: 0.004,
            uptrend_vwap_max_normal: 1.5,
            uptrend_vwap_max_volatile: 0.5,
            uptrend_ema_spread_min: 0.0005,
            uptrend_lock_duration: 900,
            sideways_bb_period: 20,
            sideways_bb_mult: 2.5,
            sideways_min_vol_pct: 0.15,
            sideways_max_vol_pct: 0.25,
            sideways_flat_budget_pct: Some(0.10),
            downtrend_max_vwap_dist: -0.80,
            downtrend_tp: 0.0080,
            downtrend_hold_lock: 720,
            downtrend_stop_loss: -0.015,
            downtrend_rsi_limit: 30.0,
            downtrend_vol_surge_limit: 3.0,
            breakout_min_std_dev: 30.0,
            breakout_min_spike_pct: 0.5,
            breakout_min_rsi: 65.0,
            breakout_max_vwap_dist: 1.5,
            breakout_ema_gap_if_big: 0.05,
        }
    }
}
