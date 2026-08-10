use crate::AppState;
use crate::conclude::Decision;
use sqlx::Row;

pub async fn execute_trade(decision: &Decision, symbol: &str, price: f64, state: &AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match decision {
        Decision::Buy { layer, usdt_to_spend, reason } => {
            let mut balance_lock = state.simulated_balance.write().await;
            if *balance_lock < *usdt_to_spend {
                return Err("Saldo USDT tidak mencukupi untuk melakukan BUY".into());
            }

            // Potong saldo USDT
            *balance_lock -= *usdt_to_spend;

            // Hitung token yang dibeli (dikurangi biaya 0.1% dengan 3x leverage)
            let fee_multiplier = 1.0 - 0.001;
            let token_bought = (*usdt_to_spend * 3.0 * fee_multiplier) / price;

            // Tambahkan token ke balance
            {
                let mut balances = state.token_balances.write().await;
                let bal = balances.entry(symbol.to_string()).or_insert(0.0);
                *bal += token_bought;
            }

            // Increment layers_filled
            let new_layers = {
                let mut layers_map = state.layers_filled.write().await;
                let layers = layers_map.entry(symbol.to_string()).or_insert(0);
                *layers += 1;
                *layers
            };

            let cycle_id = {
                let cycles = state.current_cycle_ids.read().await;
                cycles.get(symbol).copied().unwrap_or(1)
            };

            // Simpan posisi aktif ke database
            sqlx::query(
                "INSERT INTO dca_active_positions (cycle_id, symbol, layer, price, amount, usdt_spent) 
                 VALUES ($1, $2, $3, $4, $5, $6)"
            )
            .bind(cycle_id)
            .bind(symbol)
            .bind(*layer as i32)
            .bind(price)
            .bind(token_bought)
            .bind(*usdt_to_spend)
            .execute(&state.db)
            .await?;

            // Simpan riwayat transaksi ke database
            sqlx::query(
                "INSERT INTO dca_trading_history (cycle_id, symbol, action, layer, price, amount, usdt_spent, notes) 
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
            )
            .bind(cycle_id)
            .bind(symbol)
            .bind("BUY")
            .bind(*layer as i32)
            .bind(price)
            .bind(token_bought)
            .bind(*usdt_to_spend)
            .bind(reason)
            .execute(&state.db)
            .await?;

            // Set High Water Mark ke harga pembelian jika ini layer pertama
            {
                let mut hwms = state.cycle_high_water_marks.write().await;
                let hwm = hwms.entry(symbol.to_string()).or_insert(0.0);
                if new_layers == 1 {
                    *hwm = price;
                } else if price > *hwm {
                    *hwm = price;
                }
            }

            crate::add_log(state, &format!("[SmartDCA] BUY {} Layer {} @ ${:.4} | {:.6} tokens | Spent: ${:.2} | Reason: {}", symbol, layer, price, token_bought, usdt_to_spend, reason)).await;
        }

        Decision::Sell { reason } => {
            let cycle_id = {
                let cycles = state.current_cycle_ids.read().await;
                cycles.get(symbol).copied().unwrap_or(1)
            };
            let layers_used = {
                let layers_map = state.layers_filled.read().await;
                layers_map.get(symbol).copied().unwrap_or(0)
            };

            // 1. Ambil data akumulasi entry dan waktu mulai siklus dari dca_active_positions
            let row = sqlx::query(
                "SELECT 
                    COALESCE(SUM(price * amount) / NULLIF(SUM(amount), 0), 0.0) as avg_entry, 
                    COALESCE(SUM(amount), 0.0) as total_tokens, 
                    COALESCE(SUM(usdt_spent), 0.0) as total_usdt_spent,
                    MIN(timestamp) as start_time
                 FROM dca_active_positions WHERE symbol = $1 AND cycle_id = $2"
            )
            .bind(symbol)
            .bind(cycle_id)
            .fetch_one(&state.db)
            .await?;

            let avg_entry: f64 = row.try_get("avg_entry")?;
            let total_tokens: f64 = row.try_get("total_tokens")?;
            let total_usdt_spent: f64 = row.try_get("total_usdt_spent")?;
            let start_time: chrono::DateTime<chrono::Utc> = row.try_get("start_time")?;

            if total_tokens <= 0.0 {
                return Err(format!("Tidak ada saldo {} aktif di cycle ini untuk dijual", symbol).into());
            }

            // 2. Hitung realized P&L dengan 3x leverage & penanganan likuidasi
            let total_debt = total_usdt_spent * 2.0;
            let sell_fee = price * total_tokens * 0.001;
            let exit_value = (price * total_tokens) - sell_fee;

            let (usdt_received, net_pnl, pnl_pct) = if reason == "LIQUIDATION" {
                (0.0, -total_usdt_spent, -100.0)
            } else {
                let rec = f64::max(0.0, exit_value - total_debt);
                let pnl = rec - total_usdt_spent;
                let pct = if total_usdt_spent > 0.0 { (pnl / total_usdt_spent) * 100.0 } else { 0.0 };
                (rec, pnl, pct)
            };

            // 3. Update saldo
            let mut balance_lock = state.simulated_balance.write().await;
            *balance_lock += usdt_received;

            {
                let mut balances = state.token_balances.write().await;
                balances.insert(symbol.to_string(), 0.0);
            }

            // Reset layers
            {
                let mut layers_map = state.layers_filled.write().await;
                layers_map.insert(symbol.to_string(), 0);
            }

            // Reset High Water Mark
            {
                let mut hwms = state.cycle_high_water_marks.write().await;
                hwms.insert(symbol.to_string(), 0.0);
            }

            // 4. Catat ringkasan siklus di dca_cycle_summary
            let status_str = if net_pnl > 0.0 { "WIN" } else { "LOSS" };
            sqlx::query(
                "INSERT INTO dca_cycle_summary (cycle_id, symbol, start_time, end_time, layers_used, avg_entry_price, exit_price, total_spent, net_pnl, pnl_pct, exit_reason, status) 
                 VALUES ($1, $2, $3, NOW(), $4, $5, $6, $7, $8, $9, $10, $11)"
            )
            .bind(cycle_id)
            .bind(symbol)
            .bind(start_time)
            .bind(layers_used as i32)
            .bind(avg_entry)
            .bind(price)
            .bind(total_usdt_spent)
            .bind(net_pnl)
            .bind(pnl_pct)
            .bind(reason)
            .bind(status_str)
            .execute(&state.db)
            .await?;

            // 5. Catat riwayat transaksi jual
            sqlx::query(
                "INSERT INTO dca_trading_history (cycle_id, symbol, action, price, amount, notes) 
                 VALUES ($1, $2, $3, $4, $5, $6)"
            )
            .bind(cycle_id)
            .bind(symbol)
            .bind("SELL")
            .bind(price)
            .bind(total_tokens)
            .bind(reason)
            .execute(&state.db)
            .await?;

            // 6. Hapus posisi aktif di database
            sqlx::query("DELETE FROM dca_active_positions WHERE symbol = $1 AND cycle_id = $2")
                .bind(symbol)
                .bind(cycle_id)
                .execute(&state.db)
                .await?;

            // 7. Increment cycle ID
            {
                let mut cycles = state.current_cycle_ids.write().await;
                let cid = cycles.entry(symbol.to_string()).or_insert(1);
                *cid += 1;
            }

            crate::add_log(state, &format!("[SmartDCA] SELL {} Cycle {} @ ${:.4} | P&L: ${:.2} ({:.2}%) | Reason: {}", symbol, cycle_id, price, net_pnl, pnl_pct, reason)).await;
        }

        Decision::Wait => {}
    }
    Ok(())
}
