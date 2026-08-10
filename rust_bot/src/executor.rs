// INI ADALAH FILE executor.rs
use crate::{AppState, conclude::Decision};

pub async fn execute_trade(decision: &Decision, symbol: &str, price: f64, state: &AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match decision {
        Decision::Buy(amount, reason) => {
            let cost = price * amount;
            let fee = cost * 0.001; // Biaya admin 0.1%
            let total_cost = cost + fee;
            
            let mut sim_bal = state.simulated_balance.write().await;
            if *sim_bal >= total_cost {
                *sim_bal -= total_cost;
                
                let _ = sqlx::query("INSERT INTO bot_active_positions (symbol, buy_price, high_water_mark, amount) VALUES ($1, $2, $3, $4)")
                    .bind(symbol)
                    .bind(price)
                    .bind(price)
                    .bind(*amount)
                    .execute(&state.db)
                    .await;
                
                let notes = format!("{} | Biaya admin 0.1%: ${:.4}", reason, fee);
                sqlx::query(
                    "INSERT INTO bot_trading_history (action, price, amount, status, notes, symbol, strategy_version) VALUES ($1, $2, $3, $4, $5, $6, $7)"
                )
                .bind("BUY")
                .bind(price)
                .bind(*amount)
                .bind("SUCCESS")
                .bind(notes)
                .bind(symbol)
                .bind(crate::CURRENT_STRATEGY_VERSION)
                .execute(&state.db)
                .await?;
            } else {
                return Err("Saldo USDT tidak cukup (termasuk fee)".into());
            }
        },
        Decision::Sell(amount, reason) => {
            // 1. Query weighted average entry price from bot_active_positions for this symbol BEFORE deleting them!
            let row: (Option<f64>, Option<f64>) = sqlx::query_as::<_, (Option<f64>, Option<f64>)>(
                "SELECT 
                    SUM(buy_price * amount) / NULLIF(SUM(amount), 0) AS avg_entry,
                    SUM(amount) AS total_amount
                 FROM bot_active_positions WHERE symbol = $1"
            )
            .bind(symbol)
            .fetch_one(&state.db)
            .await
            .unwrap_or((None, None));

            let avg_entry_price = row.0.unwrap_or(price);
            let total_amount_held = row.1.unwrap_or(*amount);

            if total_amount_held > 0.0 {
                let revenue = price * amount;
                let fee = revenue * 0.001; // Biaya admin 0.1%
                let net_revenue = revenue - fee;
                
                let mut sim_bal = state.simulated_balance.write().await;
                *sim_bal += net_revenue;
                
                // 2. Now clean the active positions for this symbol since we have extracted the metrics
                let _ = sqlx::query("DELETE FROM bot_active_positions WHERE symbol = $1")
                    .bind(symbol)
                    .execute(&state.db)
                    .await;

                // 3. Calculate true Net P&L based on weighted average entry cost
                let buy_spent = avg_entry_price * total_amount_held;
                let buy_fee = buy_spent * 0.001; // buy fee 0.1%
                let gross_pnl = (price - avg_entry_price) * total_amount_held;
                let net_pnl = gross_pnl - buy_fee - fee;
                
                // Catat waktu jika ini adalah SELL yang profit (anti-FOMO cooldown)
                if net_pnl > 0.0 {
                    *state.last_profitable_sell_at.write().await = Some(chrono::Utc::now());
                    println!("[{}] Profit sell tercatat (P&L: ${:+.2}). Cooldown 15 menit aktif untuk BUY berikutnya.", symbol, net_pnl);
                }
                
                let pnl_str = format!(" | P&L: ${:+.2}", net_pnl);

                let notes = format!("{} | Biaya admin 0.1%: ${:.4}{}", reason, fee, pnl_str);
                sqlx::query(
                    "INSERT INTO bot_trading_history (action, price, amount, status, notes, symbol, strategy_version) VALUES ($1, $2, $3, $4, $5, $6, $7)"
                )
                .bind("SELL")
                .bind(price)
                .bind(*amount)
                .bind("SUCCESS")
                .bind(notes)
                .bind(symbol)
                .bind(crate::CURRENT_STRATEGY_VERSION)
                .execute(&state.db)
                .await?;
            } else {
                return Err("Saldo koin tidak cukup untuk dijual".into());
            }
        },
        Decision::Wait => {}
    }
    Ok(())
}
