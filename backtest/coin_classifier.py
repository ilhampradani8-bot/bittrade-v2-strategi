import requests
import statistics
import sys

# Hardcoded baselines for symbols in our local database
HARDCODED_TIERS = {
    # EXTREME: avg_vol_pct >= 3.0%
    "UTKUSDT": "EXTREME",
    "HEIUSDT": "HYPER",  # 2.55%
    "NFPUSDT": "HYPER",  # 2.41%
    "BANKUSDT": "HYPER", # 2.31%
    # HYPER: avg_vol_pct >= 1.5% and < 3.0%
    "ACEUSDT": "HYPER",
    "VICUSDT": "HYPER",
    "HFTUSDT": "HYPER",
    "DODOUSDT": "HYPER",
    "HOMEUSDT": "HYPER",
    "GIGGLEUSDT": "HYPER",
    "EPICUSDT": "HYPER",
    "PYRUSDT": "HYPER",
    "BICOUSDT": "HYPER",
    # HIGH: avg_vol_pct >= 0.5% and < 1.5%
    "CTSIUSDT": "HIGH",
    "COTIUSDT": "HIGH",
    "TUTUSDT": "HIGH",
    "TSTUSDT": "HIGH",
    "MMTUSDT": "HIGH",
    "ZBTUSDT": "HIGH",
    "ACXUSDT": "HIGH",
    "ESPUSDT": "HIGH",
    # LOW: avg_vol_pct < 0.2%
    "BTCUSDT": "LOW",
}

def classify_coin(symbol: str, db_klines: list = None) -> str:
    """
    Classify a coin into: 'EXTREME' | 'HYPER' | 'HIGH' | 'MEDIUM' | 'LOW'
    using its last 500 candles volatility.
    If db_klines is provided, uses it. Otherwise fetches from Binance API.
    If Binance API fails, falls back to hardcoded map or HIGH/MEDIUM default.
    """
    symbol = symbol.upper().strip()
    
    if not db_klines and symbol in HARDCODED_TIERS:
        return HARDCODED_TIERS[symbol]
        
    # 1. Check if we have db_klines
    closes = []
    if db_klines and len(db_klines) >= 50:
        closes = [float(k[4]) for k in db_klines[-500:]]
    
    # 2. If no db_klines, try Binance API
    if not closes:
        try:
            url = "https://api.binance.com/api/v3/klines"
            params = {"symbol": symbol, "interval": "1m", "limit": 500}
            resp = requests.get(url, params=params, timeout=5)
            if resp.status_code == 200:
                candles = resp.json()
                closes = [float(c[4]) for c in candles]
        except Exception:
            pass # ignore and fallback
            
    # 3. If we got closes, calculate volatility
    if closes and len(closes) >= 50:
        vols = []
        for i in range(50, len(closes), 10):
            chunk = closes[i-50:i]
            mean = sum(chunk) / len(chunk)
            if mean > 0:
                std = statistics.stdev(chunk)
                vols.append((std / mean) * 100)
        
        avg_vol = sum(vols) / len(vols) if vols else 0.0
        
        if avg_vol >= 3.0:
            return "EXTREME"
        elif avg_vol >= 1.5:
            return "HYPER"
        elif avg_vol >= 0.5:
            return "HIGH"
        elif avg_vol >= 0.2:
            return "MEDIUM"
        else:
            return "LOW"
            
    # 4. Fallback to hardcoded list or defaults
    if symbol in HARDCODED_TIERS:
        return HARDCODED_TIERS[symbol]
        
    if "BTC" in symbol or "ETH" in symbol:
        return "LOW"
    return "HIGH"

def get_params_for_category(category: str, symbol: str = "ALT") -> dict:
    """
    Returns parameter mapping dictionary for the given category.
    """
    params = {}
    
    # Common overrides
    if category == "EXTREME":
        params["stop_loss_limit"] = -0.070
        
        # Uptrend
        params["uptrend_gc_max_dur"] = 35
        params["uptrend_rsi_min"] = 55.0
        params["uptrend_rsi_max"] = 75.0
        params["uptrend_block_rsi_75_80"] = False
        params["uptrend_max_rsi_slope_7m"] = 999.0
        params["uptrend_min_rsi_slope_15m"] = 12.0
        params["uptrend_min_vol_surge_3m"] = 0.5
        params["uptrend_is_dynamic_sizing"] = True
        params["uptrend_tp_trail_trigger"] = 0.030
        params["uptrend_tp_trail_pullback"] = 0.010
        params["uptrend_vwap_max_normal"] = 4.5
        params["uptrend_vwap_max_volatile"] = 4.5
        params["uptrend_ema_spread_min"] = 0.0025
        params["uptrend_lock_duration"] = 2400
        
        # Sideways
        params["sideways_bb_period"] = 20
        params["sideways_bb_mult"] = 2.5
        params["sideways_min_vol_pct"] = 0.35
        params["sideways_max_vol_pct"] = 1.20
        params["sideways_flat_budget_pct"] = 0.10
        
        # Downtrend
        params["downtrend_max_vwap_dist"] = -0.30
        params["downtrend_tp"] = 0.05
        params["downtrend_hold_lock"] = 1800
        params["downtrend_stop_loss"] = -0.05
        params["downtrend_rsi_limit"] = 35.0
        params["downtrend_vol_surge_limit"] = 1.0
        
        # Breakout
        params["breakout_min_std_dev"] = 0.03
        params["breakout_min_spike_pct"] = 0.3
        params["breakout_min_rsi"] = 60.0
        params["breakout_max_vwap_dist"] = 3.5
        params["breakout_ema_gap_if_big"] = 0.05

    elif category == "HYPER":
        params["stop_loss_limit"] = -0.050
        
        # Uptrend
        params["uptrend_gc_max_dur"] = 35
        params["uptrend_rsi_min"] = 55.0
        params["uptrend_rsi_max"] = 75.0
        params["uptrend_block_rsi_75_80"] = False
        params["uptrend_max_rsi_slope_7m"] = 999.0
        params["uptrend_min_rsi_slope_15m"] = 10.0 if symbol.upper() == "BICOUSDT" else 6.0
        params["uptrend_min_vol_surge_3m"] = 0.5
        params["uptrend_is_dynamic_sizing"] = True
        params["uptrend_tp_trail_trigger"] = 0.020
        params["uptrend_tp_trail_pullback"] = 0.007
        params["uptrend_vwap_max_normal"] = 3.5
        params["uptrend_vwap_max_volatile"] = 3.5
        params["uptrend_ema_spread_min"] = 0.0020
        params["uptrend_lock_duration"] = 1800
        
        # Sideways
        params["sideways_bb_period"] = 20
        params["sideways_bb_mult"] = 2.0
        params["sideways_min_vol_pct"] = 0.30
        params["sideways_max_vol_pct"] = 1.00
        params["sideways_flat_budget_pct"] = 0.10
        
        # Downtrend
        params["downtrend_max_vwap_dist"] = -0.50
        params["downtrend_tp"] = 0.03
        params["downtrend_hold_lock"] = 1200
        params["downtrend_stop_loss"] = -0.03
        params["downtrend_rsi_limit"] = 35.0
        params["downtrend_vol_surge_limit"] = 1.5
        
        # Breakout
        params["breakout_min_std_dev"] = 0.02
        params["breakout_min_spike_pct"] = 0.2
        params["breakout_min_rsi"] = 60.0
        params["breakout_max_vwap_dist"] = 2.5
        params["breakout_ema_gap_if_big"] = 0.05

    elif category == "HIGH":
        params["stop_loss_limit"] = -0.035
        
        # Uptrend
        params["uptrend_gc_max_dur"] = 35
        params["uptrend_rsi_min"] = 55.0
        params["uptrend_rsi_max"] = 75.0
        params["uptrend_block_rsi_75_80"] = False
        params["uptrend_max_rsi_slope_7m"] = 999.0
        params["uptrend_min_rsi_slope_15m"] = 6.0
        params["uptrend_min_vol_surge_3m"] = 0.5
        params["uptrend_is_dynamic_sizing"] = True
        params["uptrend_tp_trail_trigger"] = 0.015
        params["uptrend_tp_trail_pullback"] = 0.005
        params["uptrend_vwap_max_normal"] = 3.5
        params["uptrend_vwap_max_volatile"] = 3.5
        params["uptrend_ema_spread_min"] = 0.0015
        params["uptrend_lock_duration"] = 1200
        
        # Sideways
        params["sideways_bb_period"] = 20
        params["sideways_bb_mult"] = 2.0
        params["sideways_min_vol_pct"] = 0.20
        params["sideways_max_vol_pct"] = 0.80
        params["sideways_flat_budget_pct"] = 0.10
        
        # Downtrend
        params["downtrend_max_vwap_dist"] = -0.50
        params["downtrend_tp"] = 0.03
        params["downtrend_hold_lock"] = 1200
        params["downtrend_stop_loss"] = -0.03
        params["downtrend_rsi_limit"] = 35.0
        params["downtrend_vol_surge_limit"] = 1.5
        
        # Breakout
        params["breakout_min_std_dev"] = 0.02
        params["breakout_min_spike_pct"] = 0.2
        params["breakout_min_rsi"] = 60.0
        params["breakout_max_vwap_dist"] = 2.5
        params["breakout_ema_gap_if_big"] = 0.05

    elif category == "MEDIUM":
        params["stop_loss_limit"] = -0.020
        
        # Uptrend
        params["uptrend_gc_max_dur"] = 35
        params["uptrend_rsi_min"] = 60.0
        params["uptrend_rsi_max"] = 72.0
        params["uptrend_block_rsi_75_80"] = False
        params["uptrend_max_rsi_slope_7m"] = 999.0
        params["uptrend_min_rsi_slope_15m"] = 4.0
        params["uptrend_min_vol_surge_3m"] = 0.6
        params["uptrend_is_dynamic_sizing"] = True
        params["uptrend_tp_trail_trigger"] = 0.010
        params["uptrend_tp_trail_pullback"] = 0.004
        params["uptrend_vwap_max_normal"] = 2.5
        params["uptrend_vwap_max_volatile"] = 1.5
        params["uptrend_ema_spread_min"] = 0.0010
        params["uptrend_lock_duration"] = 900
        
        # Sideways
        params["sideways_bb_period"] = 20
        params["sideways_bb_mult"] = 2.2
        params["sideways_min_vol_pct"] = 0.15
        params["sideways_max_vol_pct"] = 0.30
        params["sideways_flat_budget_pct"] = 0.10
        
        # Downtrend
        params["downtrend_max_vwap_dist"] = -0.60
        params["downtrend_tp"] = 0.015
        params["downtrend_hold_lock"] = 900
        params["downtrend_stop_loss"] = -0.02
        params["downtrend_rsi_limit"] = 32.0
        params["downtrend_vol_surge_limit"] = 2.0
        
        # Breakout
        params["breakout_min_std_dev"] = 0.01
        params["breakout_min_spike_pct"] = 0.1
        params["breakout_min_rsi"] = 62.0
        params["breakout_max_vwap_dist"] = 2.0
        params["breakout_ema_gap_if_big"] = 0.05

    else: # LOW (like BTCUSDT)
        params["stop_loss_limit"] = -0.015
        
        # Uptrend
        params["uptrend_gc_max_dur"] = 35
        params["uptrend_rsi_min"] = 64.0
        params["uptrend_rsi_max"] = 70.0
        params["uptrend_block_rsi_75_80"] = False
        params["uptrend_max_rsi_slope_7m"] = 999.0
        params["uptrend_min_rsi_slope_15m"] = 5.0
        params["uptrend_min_vol_surge_3m"] = 0.8
        params["uptrend_is_dynamic_sizing"] = True
        params["uptrend_tp_trail_trigger"] = 0.006
        params["uptrend_tp_trail_pullback"] = 0.004
        params["uptrend_vwap_max_normal"] = 1.5
        params["uptrend_vwap_max_volatile"] = 0.5
        params["uptrend_ema_spread_min"] = 0.0005
        params["uptrend_lock_duration"] = 900
        
        # Sideways
        params["sideways_bb_period"] = 20
        params["sideways_bb_mult"] = 2.5
        params["sideways_min_vol_pct"] = 0.15
        params["sideways_max_vol_pct"] = 0.25
        params["sideways_flat_budget_pct"] = 0.10
        
        # Downtrend
        params["downtrend_max_vwap_dist"] = -0.80
        params["downtrend_tp"] = 0.0080
        params["downtrend_hold_lock"] = 720
        params["downtrend_stop_loss"] = -0.015
        params["downtrend_rsi_limit"] = 30.0
        params["downtrend_vol_surge_limit"] = 3.0
        
        # Breakout
        params["breakout_min_std_dev"] = 30.0
        params["breakout_min_spike_pct"] = 0.5
        params["breakout_min_rsi"] = 65.0
        params["breakout_max_vwap_dist"] = 1.5
        params["breakout_ema_gap_if_big"] = 0.05

    return params

if __name__ == "__main__":
    sym = sys.argv[1].upper() if len(sys.argv) > 1 else "BTCUSDT"
    cat = classify_coin(sym)
    print(f"Symbol: {sym} | Category: {cat}")
