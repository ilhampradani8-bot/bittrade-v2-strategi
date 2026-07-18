use crate::AppState;

#[derive(Debug, Clone)]
pub enum Decision {
    Buy(f64, String, f64),
    Sell(f64, String),
    Wait,
}

pub async fn analyze_market(price: f64, state: &AppState, symbol: &str) -> Decision {
    let sim_bal = *state.simulated_balance.read().await;
    let asset_bal = {
        let bals = state.asset_balances.read().await;
        *bals.get(symbol).unwrap_or(&0.0)
    };

    let klines: Vec<(f64, f64, chrono::DateTime<chrono::Utc>, f64, f64)> = sqlx::query_as::<_, (f64, f64, chrono::DateTime<chrono::Utc>, f64, f64)>(
        "SELECT close_price, volume, open_time, high_price, low_price FROM grid_klines WHERE symbol = $1 ORDER BY open_time DESC LIMIT 50"
    )
    .bind(symbol)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    if klines.len() < 20 {
        let mut reg = state.market_regimes.write().await;
        reg.insert(symbol.to_string(), "WARMING_UP".to_string());
        return Decision::Wait;
    }

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
    let atr_pct = atr / price;

    let active_positions: Vec<(i32, f64, f64)> = sqlx::query_as::<_, (i32, f64, f64)>(
        "SELECT id, buy_price, amount FROM grid_active_positions WHERE symbol = $1 ORDER BY buy_price ASC"
    )
    .bind(symbol)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    if asset_bal > 0.0001 && !active_positions.is_empty() {
        let total_qty: f64 = active_positions.iter().map(|(_, _, qty)| qty).sum();
        let weighted_entry: f64 = active_positions.iter().map(|(_, bp, qty)| bp * qty).sum::<f64>() / total_qty.max(1e-10);

        let loss_pct = (weighted_entry - price) / weighted_entry * 100.0;
        let dynamic_sl_limit = (atr_pct * 3.0 * 100.0).clamp(2.0, 6.0);
        
        if loss_pct >= dynamic_sl_limit {
            crate::add_log(state, &format!("[GRID-GUARD] [{}] 🚨 EMERGENCY ATR STOP LOSS! Entry avg: ${:.2} | Now: ${:.2} | Loss: {:.2}% (Limit: {:.2}%)", symbol, weighted_entry, price, loss_pct, dynamic_sl_limit)).await;
            let mut reg = state.market_regimes.write().await;
            reg.insert(symbol.to_string(), "STOP_LOSS".to_string());
            return Decision::Sell(asset_bal, format!("[Grid-Emergency] {} ATR Stop Loss Aktif: -{:.2}% dari avg entry ${:.2}", symbol, loss_pct, weighted_entry));
        }
    }

    for i in 0..klines.len().min(10) - 1 {
        let diff_secs = (klines[i].2 - klines[i + 1].2).num_seconds().abs();
        if diff_secs > 120 {
            return Decision::Wait;
        }
    }

    let prices: Vec<f64> = klines.iter().map(|k| k.0).collect();
    let base_price: f64 = prices.iter().sum::<f64>() / prices.len() as f64;
    let variance: f64 = prices.iter().map(|&p| { let d = p - base_price; d * d }).sum::<f64>() / prices.len() as f64;
    let std_dev = variance.sqrt();
    let volatility_pct = (std_dev / base_price) * 100.0;
    
    {
        let mut vols = state.volatilities.write().await;
        vols.insert(symbol.to_string(), volatility_pct);
    }

    let spacing_pct = (volatility_pct * 0.5).clamp(0.4, 1.2) / 100.0;
    let grid_levels = 5usize;

    let last_change = prices[0] - prices[1];
    let is_sudden_dump = last_change < -(2.5 * std_dev) && std_dev > 5.0;

    if is_sudden_dump {
        crate::add_log(state, &format!("[GRID-GUARD] [{}] ⚠️ DETEKSI DUMP MENDADAK! Menghentikan sementara Grid BUY.", symbol)).await;
        let mut reg = state.market_regimes.write().await;
        reg.insert(symbol.to_string(), "DUMP_PROTECTION".to_string());
    } else {
        let mut reg = state.market_regimes.write().await;
        reg.insert(symbol.to_string(), "GRID".to_string());
    }

    let sell_candidates: Vec<(i32, f64, f64)> = sqlx::query_as::<_, (i32, f64, f64)>(
        "SELECT id, buy_price, amount FROM grid_active_positions WHERE symbol = $1 ORDER BY buy_price DESC"
    )
    .bind(symbol)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    for (pos_id, buy_price, pos_amount) in &sell_candidates {
        let profit_target = buy_price * (1.0 + spacing_pct);
        if price >= profit_target {
            let profit_pct = (price - buy_price) / buy_price * 100.0;
            return Decision::Sell(
                *pos_amount,
                format!("[Grid] {} Sell Pos#{} | Buy@${:.2} → Sell@${:.2} | +{:.3}%", symbol, pos_id, buy_price, price, profit_pct),
            );
        }
    }

    if is_sudden_dump {
        return Decision::Wait;
    }

    if sim_bal > 10.0 {
        let dynamic_pct = crate::risk::calculate_dynamic_budget(state, 0.20, symbol).await;
        if dynamic_pct <= 0.0 {
            return Decision::Wait;
        }
        let capital_per_grid = (sim_bal * dynamic_pct).min(sim_bal * 0.35); 
        let anchor_price = price.min(base_price * 1.005); 

        for i in 1..=grid_levels {
            let grid_level_price = anchor_price * (1.0 - (i as f64) * spacing_pct);

            let already_filled = active_positions.iter().any(|(_, bp, _)| {
                (bp - grid_level_price).abs() / grid_level_price < 0.0025
            });

            if !already_filled {
                let should_buy = price <= grid_level_price || (i == 1 && price <= anchor_price * (1.0 - spacing_pct * 0.5));
                if should_buy {
                    let obi = {
                        let obis = state.obis.read().await;
                        *obis.get(symbol).unwrap_or(&0.5)
                    };
                    if obi < 0.40 {
                        return Decision::Wait;
                    }

                    let amount_to_buy = capital_per_grid / price;
                    return Decision::Buy(
                        amount_to_buy,
                        format!("[Grid] {} Buy Level {} @ ${:.2} (Grid: ${:.2})", symbol, i, price, grid_level_price),
                        grid_level_price,
                    );
                } else {
                    break; 
                }
            }
        }
    }

    Decision::Wait
}
