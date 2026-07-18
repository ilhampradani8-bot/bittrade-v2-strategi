use crate::{AppState, grid_logic::Decision, corrector};

pub async fn validate_decision(decision: &Decision, price: f64, state: &AppState, symbol: &str) -> bool {
    match decision {
        Decision::Buy(amount, _, grid_level_price) => {
            let active_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM grid_active_positions WHERE symbol = $1"
            )
            .bind(symbol)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

            if active_count >= 10 {
                let err = format!("Max grid layers (10) sudah tercapai untuk {}. Aktif: {}", symbol, active_count);
                println!("[VALIDATE] ⛔ {}", err);
                let _ = corrector::log_error(state, symbol, "VALIDATION_MAX_GRID_LAYERS", &err).await;
                return false;
            }

            let dup: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM grid_active_positions WHERE symbol = $1 AND ABS(buy_price - $2) / $2 < 0.0025"
            )
            .bind(symbol)
            .bind(grid_level_price)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

            if dup > 0 {
                println!(
                    "[VALIDATE] ⛔ Level grid {} ${:.2} sudah terisi. Tidak perlu double buy.",
                    symbol, grid_level_price
                );
                return false;
            }

            let cost = price * amount;
            if cost < 5.0 {
                let err = format!("Order terlalu kecil (${:.2}), minimum $5.00", cost);
                println!("[VALIDATE] ⛔ {}", err);
                let _ = corrector::log_error(state, symbol, "VALIDATION_MIN_SIZE", &err).await;
                return false;
            }

            let total_cost = cost * 1.001;
            let sim_bal = *state.simulated_balance.read().await;
            if total_cost > sim_bal {
                let err = format!(
                    "Saldo tidak cukup. Butuh ${:.4} (inc. fee), ada USDT ${:.4}",
                    total_cost, sim_bal
                );
                println!("[VALIDATE] ⛔ {}", err);
                let _ = corrector::log_error(state, symbol, "VALIDATION_INSUFFICIENT_USDT", &err).await;
                return false;
            }

            if *amount <= 0.0 {
                println!("[VALIDATE] ⛔ Amount {} tidak valid: {}", symbol, amount);
                return false;
            }

            println!(
                "[VALIDATE] ✅ BUY {} valid. Level: ${:.2} | Qty: {:.6} | Cost: ${:.4}",
                symbol, grid_level_price, amount, cost
            );
            true
        }

        Decision::Sell(amount, reason) => {
            let asset_bal = {
                let bals = state.asset_balances.read().await;
                *bals.get(symbol).unwrap_or(&0.0)
            };
            if asset_bal < 0.0001 {
                let err = format!("Tidak ada {} untuk dijual (balance: {:.6})", symbol, asset_bal);
                println!("[VALIDATE] ⛔ {}", err);
                let _ = corrector::log_error(state, symbol, "VALIDATION_INSUFFICIENT_ASSET", &err).await;
                return false;
            }

            let is_emergency = reason.contains("Emergency") || reason.contains("Stop Loss");
            let effective_amount = if is_emergency { asset_bal } else { *amount };

            let value = price * effective_amount;
            if value < 5.0 && !is_emergency {
                let err = format!("Nilai jual terlalu kecil (${:.2}), minimum $5.00", value);
                println!("[VALIDATE] ⛔ {}", err);
                let _ = corrector::log_error(state, symbol, "VALIDATION_MIN_SELL_SIZE", &err).await;
                return false;
            }

            if !is_emergency {
                let has_position: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM grid_active_positions WHERE symbol = $1 AND $2 >= buy_price * 1.004"
                )
                .bind(symbol)
                .bind(price)
                .fetch_one(&state.db)
                .await
                .unwrap_or(0);

                if has_position == 0 {
                    println!("[VALIDATE] ⛔ Tidak ada posisi {} yang mencapai profit target di harga ${:.2}", symbol, price);
                    return false;
                }
            }

            println!(
                "[VALIDATE] ✅ SELL {} valid. Qty: {:.6} | Value: ${:.4} | Emergency: {}",
                symbol, effective_amount, value, is_emergency
            );
            true
        }

        Decision::Wait => false,
    }
}
