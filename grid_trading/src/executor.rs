use crate::{AppState, grid_logic::Decision};

pub async fn execute_trade(
    decision: &Decision,
    price: f64,
    state: &AppState,
    symbol: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match decision {
        Decision::Buy(amount, reason, grid_level_price) => {
            let cost = price * amount;
            let fee = cost * 0.001;
            let total_cost = cost + fee;

            {
                let mut sim_bal = state.simulated_balance.write().await;
                if *sim_bal < total_cost {
                    return Err(format!(
                        "Saldo USDT tidak cukup. Butuh ${:.4}, punya ${:.4}",
                        total_cost, *sim_bal
                    ).into());
                }
                *sim_bal -= total_cost;

                let mut bals = state.asset_balances.write().await;
                let current_bal = *bals.get(symbol).unwrap_or(&0.0);
                bals.insert(symbol.to_string(), current_bal + amount);
            }

            let insert_result = sqlx::query(
                "INSERT INTO grid_active_positions (symbol, buy_price, high_water_mark, amount) VALUES ($1, $2, $3, $4)"
            )
            .bind(symbol)
            .bind(*grid_level_price) 
            .bind(price)             
            .bind(*amount)
            .execute(&state.db)
            .await;

            if let Err(e) = insert_result {
                let mut sim_bal = state.simulated_balance.write().await;
                *sim_bal += total_cost;
                let mut bals = state.asset_balances.write().await;
                let current_bal = *bals.get(symbol).unwrap_or(&0.0);
                bals.insert(symbol.to_string(), current_bal - amount);
                return Err(format!("Gagal insert posisi {} ke DB: {}", symbol, e).into());
            }

            let notes = format!(
                "{} | Harga: ${:.2} | Grid Level: ${:.2} | Qty: {:.6} | Fee: ${:.4}",
                reason, price, grid_level_price, amount, fee
            );
            sqlx::query(
                "INSERT INTO grid_trading_history (symbol, action, price, amount, status, notes) VALUES ($1, $2, $3, $4, $5, $6)"
            )
            .bind(symbol)
            .bind("BUY")
            .bind(price)
            .bind(*amount)
            .bind("SUCCESS")
            .bind(notes)
            .execute(&state.db)
            .await?;
        }

        Decision::Sell(amount, reason) => {
            let is_emergency = reason.contains("Emergency") || reason.contains("Stop Loss");

            let target_positions: Vec<(i32, f64, f64)> = if is_emergency {
                sqlx::query_as::<_, (i32, f64, f64)>(
                    "SELECT id, buy_price, amount FROM grid_active_positions WHERE symbol = $1 ORDER BY buy_price ASC"
                )
                .bind(symbol)
                .fetch_all(&state.db)
                .await
                .unwrap_or_default()
            } else {
                sqlx::query_as::<_, (i32, f64, f64)>(
                    "SELECT id, buy_price, amount FROM grid_active_positions WHERE symbol = $1 AND $2 >= buy_price * 1.004 ORDER BY buy_price DESC LIMIT 1"
                )
                .bind(symbol)
                .bind(price)
                .fetch_all(&state.db)
                .await
                .unwrap_or_default()
            };

            if target_positions.is_empty() {
                let asset_bal = {
                    let bals = state.asset_balances.read().await;
                    *bals.get(symbol).unwrap_or(&0.0)
                };
                if asset_bal < 0.0001 {
                    println!("[EXECUTOR] Tidak ada posisi atau {} untuk dijual. Skip.", symbol);
                    return Ok(());
                }
                let sell_qty = asset_bal.min(*amount);
                if sell_qty < 0.0001 {
                    return Ok(());
                }
                self_execute_sell(price, sell_qty, reason, state, symbol, None, None).await?;
                return Ok(());
            }

            for (pos_id, buy_price, pos_amount) in &target_positions {
                let asset_bal_now = {
                    let bals = state.asset_balances.read().await;
                    *bals.get(symbol).unwrap_or(&0.0)
                };
                let sell_qty = pos_amount.min(asset_bal_now).min(*amount);
                if sell_qty < 0.0001 {
                    continue;
                }
                self_execute_sell(price, sell_qty, reason, state, symbol, Some(*pos_id), Some(*buy_price)).await?;

                if !is_emergency {
                    break;
                }
            }
        }

        Decision::Wait => {}
    }
    Ok(())
}

async fn self_execute_sell(
    price: f64,
    sell_qty: f64,
    reason: &str,
    state: &AppState,
    symbol: &str,
    pos_id: Option<i32>,
    buy_price: Option<f64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let revenue = price * sell_qty;
    let fee = revenue * 0.001;
    let net_revenue = revenue - fee;

    {
        let mut sim_bal = state.simulated_balance.write().await;
        *sim_bal += net_revenue;

        let mut bals = state.asset_balances.write().await;
        let current_bal = *bals.get(symbol).unwrap_or(&0.0);
        bals.insert(symbol.to_string(), (current_bal - sell_qty).max(0.0));
    }

    if let Some(id) = pos_id {
        let _ = sqlx::query("DELETE FROM grid_active_positions WHERE id = $1")
            .bind(id)
            .execute(&state.db)
            .await;
    }

    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM grid_active_positions WHERE symbol = $1")
        .bind(symbol)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    if remaining == 0 {
        let mut hwms = state.high_water_marks.write().await;
        hwms.insert(symbol.to_string(), 0.0);
    }

    let (pnl_str, net_pnl) = if let Some(bp) = buy_price {
        let buy_cost = bp * sell_qty;
        let buy_fee = buy_cost * 0.001;
        let net_pnl = revenue - buy_cost - buy_fee - fee;
        (format!(" | P&L: ${:+.4}", net_pnl), net_pnl)
    } else {
        (String::new(), 0.0)
    };

    let status = if net_pnl >= 0.0 || buy_price.is_none() { "SUCCESS" } else { "LOSS" };

    let notes = format!(
        "{} | Sell@${:.2} | Qty: {:.6} | Fee: ${:.4}{}",
        reason, price, sell_qty, fee, pnl_str
    );
    sqlx::query(
        "INSERT INTO grid_trading_history (symbol, action, price, amount, status, notes) VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(symbol)
    .bind("SELL")
    .bind(price)
    .bind(sell_qty)
    .bind(status)
    .bind(notes)
    .execute(&state.db)
    .await?;

    Ok(())
}
