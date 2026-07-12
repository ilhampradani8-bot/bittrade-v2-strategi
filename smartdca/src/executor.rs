use crate::AppState;
use crate::conclude::Decision;
use sqlx::Row;

pub async fn execute_trade(decision: &Decision, price: f64, state: &AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match decision {
        Decision::Buy { layer, usdt_to_spend, reason } => {
            let mut balance_lock = state.simulated_balance.write().await;
            if *balance_lock < *usdt_to_spend {
                return Err("Saldo USDT tidak mencukupi untuk melakukan BUY".into());
            }

            // Potong saldo USDT
            *balance_lock -= *usdt_to_spend;

            // Hitung BTC yang dibeli (dikurangi biaya 0.1%)
            let fee_multiplier = 1.0 - 0.001;
            let btc_bought = (*usdt_to_spend * fee_multiplier) / price;

            // Tambahkan BTC ke balance
            let mut btc_lock = state.btc_balance.write().await;
            *btc_lock += btc_bought;

            // Increment layers_filled
            let mut layers_lock = state.layers_filled.write().await;
            *layers_lock += 1;

            let cycle_id = *state.current_cycle_id.read().await;

            // Simpan posisi aktif ke database
            sqlx::query(
                "INSERT INTO dca_active_positions (cycle_id, layer, price, amount, usdt_spent) 
                 VALUES ($1, $2, $3, $4, $5)"
            )
            .bind(cycle_id)
            .bind(*layer as i32)
            .bind(price)
            .bind(btc_bought)
            .bind(*usdt_to_spend)
            .execute(&state.db)
            .await?;

            // Simpan riwayat transaksi ke database
            sqlx::query(
                "INSERT INTO dca_trading_history (cycle_id, action, layer, price, amount, usdt_spent, notes) 
                 VALUES ($1, $2, $3, $4, $5, $6, $7)"
            )
            .bind(cycle_id)
            .bind("BUY")
            .bind(*layer as i32)
            .bind(price)
            .bind(btc_bought)
            .bind(*usdt_to_spend)
            .bind(reason)
            .execute(&state.db)
            .await?;

            // Set High Water Mark ke harga pembelian jika ini layer pertama
            let mut hwm_lock = state.cycle_high_water_mark.write().await;
            if *layers_lock == 1 {
                *hwm_lock = price;
            } else if price > *hwm_lock {
                *hwm_lock = price;
            }

            crate::add_log(state, &format!("[SmartDCA] BUY Layer {} @ ${:.2} | {:.6} BTC | Spent: ${:.2} | Reason: {}", layer, price, btc_bought, usdt_to_spend, reason)).await;
        }

        Decision::Sell { reason } => {
            let cycle_id = *state.current_cycle_id.read().await;
            let layers_used = *state.layers_filled.read().await;

            // 1. Ambil data akumulasi entry dan waktu mulai siklus dari dca_active_positions
            let row = sqlx::query(
                "SELECT 
                    COALESCE(SUM(price * amount) / NULLIF(SUM(amount), 0), 0.0) as avg_entry, 
                    COALESCE(SUM(amount), 0.0) as total_btc, 
                    COALESCE(SUM(usdt_spent), 0.0) as total_usdt_spent,
                    MIN(timestamp) as start_time
                 FROM dca_active_positions WHERE cycle_id = $1"
            )
            .bind(cycle_id)
            .fetch_one(&state.db)
            .await?;

            let avg_entry: f64 = row.try_get("avg_entry")?;
            let total_btc: f64 = row.try_get("total_btc")?;
            let total_usdt_spent: f64 = row.try_get("total_usdt_spent")?;
            let start_time: chrono::DateTime<chrono::Utc> = row.try_get("start_time")?;

            if total_btc <= 0.0 {
                return Err("Tidak ada saldo BTC aktif di cycle ini untuk dijual".into());
            }

            // 2. Hitung realized P&L
            let gross_pnl = (price - avg_entry) * total_btc;
            let sell_fee = price * total_btc * 0.001;
            let net_pnl = gross_pnl - sell_fee;
            let pnl_pct = if total_usdt_spent > 0.0 {
                (net_pnl / total_usdt_spent) * 100.0
            } else {
                0.0
            };

            let usdt_received = (price * total_btc) * (1.0 - 0.001);

            // 3. Update saldo
            let mut balance_lock = state.simulated_balance.write().await;
            *balance_lock += usdt_received;

            let mut btc_lock = state.btc_balance.write().await;
            *btc_lock = 0.0;

            // Reset layers
            let mut layers_lock = state.layers_filled.write().await;
            *layers_lock = 0;

            // Reset High Water Mark
            let mut hwm_lock = state.cycle_high_water_mark.write().await;
            *hwm_lock = 0.0;

            // 4. Catat ringkasan siklus di dca_cycle_summary
            let status_str = if net_pnl > 0.0 { "WIN" } else { "LOSS" };
            sqlx::query(
                "INSERT INTO dca_cycle_summary (cycle_id, start_time, end_time, layers_used, avg_entry_price, exit_price, total_spent, net_pnl, pnl_pct, exit_reason, status) 
                 VALUES ($1, $2, NOW(), $3, $4, $5, $6, $7, $8, $9, $10)"
            )
            .bind(cycle_id)
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
                "INSERT INTO dca_trading_history (cycle_id, action, price, amount, notes) 
                 VALUES ($1, $2, $3, $4, $5)"
            )
            .bind(cycle_id)
            .bind("SELL")
            .bind(price)
            .bind(total_btc)
            .bind(reason)
            .execute(&state.db)
            .await?;

            // 6. Hapus posisi aktif di database
            sqlx::query("DELETE FROM dca_active_positions WHERE cycle_id = $1")
                .bind(cycle_id)
                .execute(&state.db)
                .await?;

            // 7. Increment cycle ID
            let mut cycle_lock = state.current_cycle_id.write().await;
            *cycle_lock += 1;

            crate::add_log(state, &format!("[SmartDCA] SELL Cycle {} @ ${:.2} | P&L: ${:.2} ({:.2}%) | Reason: {}", cycle_id, price, net_pnl, pnl_pct, reason)).await;
        }

        Decision::Wait => {}
    }
    Ok(())
}
