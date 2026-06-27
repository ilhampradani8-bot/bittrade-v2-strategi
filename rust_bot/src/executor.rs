use crate::{AppState, conclude::Decision};

pub async fn execute_trade(decision: &Decision, price: f64, state: &AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match decision {
        Decision::Buy(amount) => {
            let cost = price * amount;
            let fee = cost * 0.001; // Biaya admin 0.1%
            let total_cost = cost + fee;
            
            let mut sim_bal = state.simulated_balance.write().await;
            if *sim_bal >= total_cost {
                *sim_bal -= total_cost;
                let mut btc_bal = state.btc_balance.write().await;
                *btc_bal += amount;
                
                let notes = format!("Beli simulasi. Biaya admin 0.1%: ${:.4}", fee);
                sqlx::query(
                    "INSERT INTO bot_trading_history (action, price, amount, status, notes) VALUES ($1, $2, $3, $4, $5)"
                )
                .bind("BUY")
                .bind(price)
                .bind(*amount)
                .bind("SUCCESS")
                .bind(notes)
                .execute(&state.db)
                .await?;
            } else {
                return Err("Saldo USDT tidak cukup (termasuk fee)".into());
            }
        },
        Decision::Sell(amount) => {
            let mut btc_bal = state.btc_balance.write().await;
            if *btc_bal >= *amount {
                let revenue = price * amount;
                let fee = revenue * 0.001; // Biaya admin 0.1%
                let net_revenue = revenue - fee;
                
                let mut sim_bal = state.simulated_balance.write().await;
                *sim_bal += net_revenue;
                *btc_bal -= amount;

                // Hitung P&L dari BUY terakhir
                let last_buy: Option<(f64, f64)> = sqlx::query_as::<sqlx::Postgres, (f64, f64)>(
                    "SELECT price, amount FROM bot_trading_history WHERE action = 'BUY' AND status = 'SUCCESS' ORDER BY id DESC LIMIT 1"
                )
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None);

                let pnl_str = match last_buy {
                    Some((buy_price, buy_amount)) => {
                        let buy_fee = buy_amount * buy_price * 0.001;
                        let gross_pnl = (price - buy_price) * amount;
                        let net_pnl = gross_pnl - buy_fee - fee;
                        format!(" P&L: ${:+.2}", net_pnl)
                    }
                    None => "".to_string(),
                };

                let notes = format!("Jual simulasi. Biaya admin 0.1%: ${:.4}.{}", fee, pnl_str);
                sqlx::query(
                    "INSERT INTO bot_trading_history (action, price, amount, status, notes) VALUES ($1, $2, $3, $4, $5)"
                )
                .bind("SELL")
                .bind(price)
                .bind(*amount)
                .bind("SUCCESS")
                .bind(notes)
                .execute(&state.db)
                .await?;
            } else {
                return Err("Saldo BTC tidak cukup".into());
            }
        },
        Decision::Wait => {}
    }
    Ok(())
}
