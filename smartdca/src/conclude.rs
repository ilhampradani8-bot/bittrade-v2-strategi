use crate::AppState;

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Buy { layer: u8, usdt_to_spend: f64, reason: String },
    Sell { reason: String },
    Wait,
}

pub async fn evaluate_exit_only(symbol: &str, current_price: f64, state: &AppState) -> Decision {
    let layers_filled = {
        let layers_map = state.layers_filled.read().await;
        layers_map.get(symbol).copied().unwrap_or(0)
    };

    if layers_filled > 0 {
        let cycle_id = {
            let cycles = state.current_cycle_ids.read().await;
            cycles.get(symbol).copied().unwrap_or(1)
        };

        // Query weighted average entry dari active positions
        let active_pos: Result<Option<(f64, f64, f64)>, sqlx::Error> = sqlx::query_as::<_, (f64, f64, f64)>(
            "SELECT COALESCE(SUM(price * amount) / NULLIF(SUM(amount), 0), 0.0), COALESCE(SUM(amount), 0.0), COALESCE(SUM(usdt_spent), 0.0) 
             FROM dca_active_positions WHERE symbol = $1 AND cycle_id = $2"
        )
        .bind(symbol)
        .bind(cycle_id)
        .fetch_optional(&state.db)
        .await;

        if let Ok(Some((avg_entry, total_btc, total_usdt_spent))) = active_pos {
            if total_btc > 0.0 && total_usdt_spent > 0.0 && avg_entry > 0.0 {
                // A. Check Liquidation (For 3x Leverage simulation)
                let total_debt = total_usdt_spent * 2.0;
                let position_value = current_price * total_btc;
                if position_value <= total_debt {
                    return Decision::Sell {
                        reason: "LIQUIDATION".to_string(),
                    };
                }

                let current_profit_pct = (current_price - avg_entry) / avg_entry;

                // B. Emergency Cut Loss di -5% (Mencegah kerugian besar pada leverage 3x)
                if current_profit_pct <= -0.05 {
                    return Decision::Sell {
                        reason: "[Darurat] Cut Loss DCA -5%".to_string(),
                    };
                }

                // C. Hard Take Profit di +2.5%
                if current_profit_pct >= 0.025 {
                    return Decision::Sell {
                        reason: "[SmartDCA] Hard Take Profit +2.5%".to_string(),
                    };
                }

                // D. Trailing Take Profit (Profit >= 1.5% dan drop 0.8% dari HWM)
                let hwm = {
                    let hwms = state.cycle_high_water_marks.read().await;
                    hwms.get(symbol).copied().unwrap_or(0.0)
                };
                if current_profit_pct >= 0.015 && hwm > 0.0 {
                    let drop_from_hwm = (hwm - current_price) / hwm;
                    if drop_from_hwm >= 0.008 {
                        return Decision::Sell {
                            reason: "[SmartDCA] Trailing Profit Lock".to_string(),
                        };
                    }
                }
            }
        }
    }

    Decision::Wait
}

pub async fn analyze_market(symbol: &str, current_price: f64, state: &AppState) -> Decision {
    let exit_decision = evaluate_exit_only(symbol, current_price, state).await;
    if exit_decision != Decision::Wait {
        return exit_decision;
    }

    let layers_filled = {
        let layers_map = state.layers_filled.read().await;
        layers_map.get(symbol).copied().unwrap_or(0)
    };

    // ==========================================
    // 2. DETEKSI ZONA DISKON (Hanya jika layers_filled < 3)
    // ==========================================
    if layers_filled < 3 {
        // A. Ambil high_4h: harga tertinggi dari 240 kline 1 menit terakhir
        let high_4h: Option<f64> = sqlx::query_scalar(
            "SELECT MAX(high_price) FROM (
                SELECT high_price FROM dca_klines WHERE symbol = $1 ORDER BY open_time DESC LIMIT 240
             ) as last_240"
        )
        .bind(symbol)
        .fetch_one(&state.db)
        .await
        .unwrap_or(None);

        if let Some(h4h) = high_4h {
            if h4h > 0.0 {
                let drop_pct = (current_price - h4h) / h4h * 100.0;

                // B. Hitung RSI-14 dan Dynamic Min RSI dari 1000 kline terakhir
                let mut close_prices: Vec<f64> = sqlx::query_scalar(
                    "SELECT close_price FROM dca_klines WHERE symbol = $1 ORDER BY open_time DESC LIMIT 1000"
                )
                .bind(symbol)
                .fetch_all(&state.db)
                .await
                .unwrap_or_default();

                if close_prices.len() >= 15 {
                    close_prices.reverse(); // Urutan tertua ke terbaru
                    
                    // Gunakan 15 kline penutupan terakhir untuk RSI
                    let rsi_slice = &close_prices[close_prices.len() - 15..];
                    let rsi = calculate_rsi(rsi_slice, 14);
                    
                    // Gunakan 195 kline terakhir untuk Min RSI 3 jam
                    let rsi_3h_slice = if close_prices.len() >= 195 {
                        &close_prices[close_prices.len() - 195..]
                    } else {
                        &close_prices[..]
                    };
                    let min_rsi = calculate_min_rsi_3h(rsi_3h_slice, 14);
                    let dynamic_limit = f64::min(min_rsi + 5.0, 25.0);
                    let rsi_allowed = !(rsi < 40.0 && rsi > dynamic_limit);

                    // Hitung EMA-750 (Trend Filter setara 15m EMA-50)
                    let ema_750 = calculate_ema(&close_prices, 750);
                    let trend_ok = if ema_750 > 0.0 {
                        current_price > ema_750
                    } else {
                        true
                    };

                    // C. Hitung volume panic dump (volume candle terakhir tidak boleh > 3x rata-rata 20 candle sebelumnya)
                    let volumes: Vec<f64> = sqlx::query_scalar(
                        "SELECT volume FROM dca_klines WHERE symbol = $1 ORDER BY open_time DESC LIMIT 21"
                    )
                    .bind(symbol)
                    .fetch_all(&state.db)
                    .await
                    .unwrap_or_default();

                    let mut volume_safe = true;
                    if volumes.len() >= 2 {
                        let last_volume = volumes[0];
                        let sum_vol: f64 = volumes[1..].iter().sum();
                        let avg_vol = sum_vol / (volumes.len() - 1) as f64;
                        if avg_vol > 0.0 && last_volume > 3.0 * avg_vol {
                            volume_safe = false; // Ada lonjakan volume penjualan panik
                        }
                    }

                    // D. Evaluasi Layer Decisions jika pasar aman
                    if volume_safe && rsi < 65.0 {
                        // Calculate total equity
                        let total_equity = {
                            let sim_bal = *state.simulated_balance.read().await;
                            let mut total_token_value = 0.0;
                            {
                                let balances = state.token_balances.read().await;
                                let prices = state.current_prices.read().await;
                                for (sym, &bal) in balances.iter() {
                                    let p = prices.get(sym).copied().unwrap_or(0.0);
                                    total_token_value += bal * p;
                                }
                            }
                            sim_bal + total_token_value
                        };
                        let coin_budget = total_equity / 5.0;

                        // Layer 1
                        if drop_pct <= -2.5 && layers_filled == 0 && rsi < 50.0 && rsi_allowed && trend_ok {
                            let spend = coin_budget * 0.40;
                            return Decision::Buy {
                                layer: 1,
                                usdt_to_spend: spend,
                                reason: format!("[SmartDCA] Layer 1 — Drop {:.2}% (RSI: {:.1}, Lim: {:.1})", drop_pct, rsi, dynamic_limit),
                            };
                        }

                        // Layer 2
                        if drop_pct <= -5.0 && layers_filled == 1 && rsi < 50.0 {
                            let spend = coin_budget * 0.30;
                            return Decision::Buy {
                                layer: 2,
                                usdt_to_spend: spend,
                                reason: format!("[SmartDCA] Layer 2 — Drop {:.2}% (RSI: {:.1}, Lim: {:.1})", drop_pct, rsi, dynamic_limit),
                            };
                        }

                        // Layer 3
                        if drop_pct <= -8.0 && layers_filled == 2 && rsi < 40.0 {
                            let spend = coin_budget * 0.30;
                            return Decision::Buy {
                                layer: 3,
                                usdt_to_spend: spend,
                                reason: format!("[SmartDCA] Layer 3 — Deep Discount {:.2}% (RSI: {:.1}, Lim: {:.1})", drop_pct, rsi, dynamic_limit),
                            };
                        }
                    }
                }
            }
        }
    }

    Decision::Wait
}

fn calculate_rsi(prices: &[f64], period: usize) -> f64 {
    if prices.len() < period + 1 {
        return 50.0;
    }
    
    let changes: Vec<f64> = prices.windows(2)
        .map(|w| w[1] - w[0])
        .collect();
    
    let recent = &changes[changes.len() - period..];
    
    let avg_gain = recent.iter()
        .filter(|&&x| x > 0.0)
        .sum::<f64>() / period as f64;
    
    let avg_loss = recent.iter()
        .filter(|&&x| x < 0.0)
        .map(|&x| x.abs())
        .sum::<f64>() / period as f64;
    
    if avg_loss == 0.0 {
        return 100.0;
    }
    
    let rs = avg_gain / avg_loss;
    100.0 - (100.0 / (1.0 + rs))
}

fn calculate_min_rsi_3h(prices: &[f64], period: usize) -> f64 {
    if prices.len() < period + 1 {
        return 50.0;
    }
    let mut min_rsi = 100.0;
    for i in period..prices.len() {
        let sub_prices = &prices[i - period ..= i];
        let rsi_val = calculate_rsi(sub_prices, period);
        if rsi_val < min_rsi {
            min_rsi = rsi_val;
        }
    }
    min_rsi
}

fn calculate_ema(prices: &[f64], period: usize) -> f64 {
    if prices.len() < period {
        return 0.0;
    }
    let multiplier = 2.0 / (period as f64 + 1.0);
    let mut ema = prices[0];
    for &price in &prices[1..] {
        ema = (price - ema) * multiplier + ema;
    }
    ema
}
