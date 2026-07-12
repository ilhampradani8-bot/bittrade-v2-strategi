use crate::AppState;

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Buy { layer: u8, usdt_to_spend: f64, reason: String },
    Sell { reason: String },
    Wait,
}

pub async fn analyze_market(current_price: f64, state: &AppState) -> Decision {
    let layers_filled = *state.layers_filled.read().await;

    // ==========================================
    // 1. EMERGENCY CHECKS (Prioritas Tertinggi)
    // ==========================================
    if layers_filled > 0 {
        // Query weighted average entry dari active positions
        let active_pos: Result<Option<(f64, f64, f64)>, sqlx::Error> = sqlx::query_as::<_, (f64, f64, f64)>(
            "SELECT COALESCE(SUM(price * amount) / NULLIF(SUM(amount), 0), 0.0), COALESCE(SUM(amount), 0.0), COALESCE(SUM(usdt_spent), 0.0) 
             FROM dca_active_positions WHERE cycle_id = $1"
        )
        .bind(*state.current_cycle_id.read().await)
        .fetch_optional(&state.db)
        .await;

        if let Ok(Some((avg_entry, total_btc, total_usdt_spent))) = active_pos {
            if total_btc > 0.0 && total_usdt_spent > 0.0 && avg_entry > 0.0 {
                let current_profit_pct = (current_price - avg_entry) / avg_entry;

                // A. Emergency Cut Loss di -5%
                if current_profit_pct <= -0.05 {
                    return Decision::Sell {
                        reason: "[Darurat] Cut Loss DCA -5%".to_string(),
                    };
                }

                // B. Hard Take Profit di +2.5%
                if current_profit_pct >= 0.025 {
                    return Decision::Sell {
                        reason: "[SmartDCA] Hard Take Profit +2.5%".to_string(),
                    };
                }

                // C. Trailing Take Profit (Profit >= 1.5% dan drop 0.8% dari HWM)
                let hwm = *state.cycle_high_water_mark.read().await;
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

    // ==========================================
    // 2. DETEKSI ZONA DISKON (Hanya jika layers_filled < 3)
    // ==========================================
    if layers_filled < 3 {
        // A. Ambil high_4h: harga tertinggi dari 240 kline 1 menit terakhir
        let high_4h: Option<f64> = sqlx::query_scalar(
            "SELECT MAX(high_price) FROM (
                SELECT high_price FROM btc_klines ORDER BY open_time DESC LIMIT 240
             ) as last_240"
        )
        .fetch_one(&state.db)
        .await
        .unwrap_or(None);

        if let Some(h4h) = high_4h {
            if h4h > 0.0 {
                let drop_pct = (current_price - h4h) / h4h * 100.0;

                // B. Hitung RSI-14 dari 15 kline terakhir (memerlukan 14 rentang perubahan)
                let mut close_prices: Vec<f64> = sqlx::query_scalar(
                    "SELECT close_price FROM btc_klines ORDER BY open_time DESC LIMIT 15"
                )
                .fetch_all(&state.db)
                .await
                .unwrap_or_default();

                if close_prices.len() >= 15 {
                    close_prices.reverse(); // Urutan tertua ke terbaru
                    let rsi = calculate_rsi(&close_prices, 14);

                    // C. Hitung volume panic dump (volume candle terakhir tidak boleh > 3x rata-rata 20 candle sebelumnya)
                    let volumes: Vec<f64> = sqlx::query_scalar(
                        "SELECT volume FROM btc_klines ORDER BY open_time DESC LIMIT 21"
                    )
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
                        let current_bal = *state.simulated_balance.read().await;

                        // Layer 1
                        if drop_pct <= -2.0 && layers_filled == 0 && rsi < 60.0 {
                            let spend = current_bal * 0.20;
                            return Decision::Buy {
                                layer: 1,
                                usdt_to_spend: spend,
                                reason: format!("[SmartDCA] Layer 1 — Drop {:.2}% (RSI: {:.1})", drop_pct, rsi),
                            };
                        }

                        // Layer 2
                        if drop_pct <= -4.0 && layers_filled == 1 && rsi < 50.0 {
                            let spend = current_bal * 0.30;
                            return Decision::Buy {
                                layer: 2,
                                usdt_to_spend: spend,
                                reason: format!("[SmartDCA] Layer 2 — Drop {:.2}% (RSI: {:.1})", drop_pct, rsi),
                            };
                        }

                        // Layer 3
                        if drop_pct <= -6.0 && layers_filled == 2 && rsi < 40.0 {
                            let spend = current_bal * 0.70;
                            return Decision::Buy {
                                layer: 3,
                                usdt_to_spend: spend,
                                reason: format!("[SmartDCA] Layer 3 — Deep Discount {:.2}% (RSI: {:.1})", drop_pct, rsi),
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
