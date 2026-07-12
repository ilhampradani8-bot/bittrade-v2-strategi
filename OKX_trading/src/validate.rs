use crate::{AppState, conclude::Decision, corrector};
use chrono::Utc;

pub async fn validate_decision(decision: &Decision, price: f64, state: &AppState) -> bool {
    // 1. Ambil data transaksi terakhir untuk pengecekan Cool-down
    let last_trade: Option<(String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as::<sqlx::Postgres, (String, chrono::DateTime<chrono::Utc>)>(
        "SELECT action, timestamp FROM okx_trading_history WHERE status = 'SUCCESS' ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let (current_action, reason) = match decision {
        Decision::Buy(_, r) => ("BUY", r.as_str()),
        Decision::Sell(_, r) => ("SELL", r.as_str()),
        Decision::Wait => ("WAIT", ""),
    };

    let is_emergency = reason.starts_with("[Darurat]");

    if !is_emergency {
        // 1. Minimum Holding Time (Mencegah Whipsaw Sell)
        if current_action == "SELL" {
            let last_buy_time: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
                "SELECT timestamp FROM okx_trading_history WHERE action = 'BUY' AND status = 'SUCCESS' ORDER BY id DESC LIMIT 1"
            )
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

            if let Some(buy_time) = last_buy_time {
                let duration = Utc::now().signed_duration_since(buy_time);
                if duration.num_minutes() < 15 {
                    let err = format!(
                        "Mencegah Whipsaw (Holding time < 15 menit). Beli terakhir {} menit lalu. Sinyal normal ({}) diabaikan.",
                        duration.num_minutes(),
                        reason
                    );
                    println!("{}", err);
                    return false;
                }
            }
        }

        // 2. Cooldown 5 menit untuk transaksi yang sama
        if let Some((last_action, last_time)) = last_trade {
            let duration = Utc::now().signed_duration_since(last_time);
            if current_action == last_action && duration.num_minutes() < 5 {
                let err = format!(
                    "Mencegah transaksi berulang (Cool-down aktif). Aksi terakhir {} pada {} ({} menit lalu)",
                    last_action,
                    last_time.with_timezone(&chrono::FixedOffset::east_opt(7 * 3600).unwrap()).format("%H:%M:%S"),
                    duration.num_minutes()
                );
                println!("{}", err);
                let _ = corrector::log_error(state, "VALIDATION_COOLDOWN", &err).await;
                return false;
            }
        }
    }

    match decision {
        Decision::Buy(amount, _) => {
            // Check active positions count (Max 3 layers)
            let active_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM okx_active_positions"
            )
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

            if active_count >= 3 {
                let err = format!("Max pyramiding (3 layer) tercapai. Tunggu siklus selesai (posisi aktif saat ini: {}).", active_count);
                println!("{}", err);
                let _ = corrector::log_error(state, "VALIDATION_MAX_PYRAMIDING", &err).await;
                return false;
            }

            let cost = price * amount;
            let fee = cost * 0.001;
            let total_cost = cost + fee;
            let sim_bal = state.simulated_balance.read().await;
            
            // Validasi ukuran transaksi minimum
            if cost < 5.0 {
                let err = format!("Nilai transaksi beli terlalu kecil (${:.2}), minimum $5.0", cost);
                println!("{}", err);
                let _ = corrector::log_error(state, "VALIDATION_MIN_SIZE", &err).await;
                return false;
            }
            
            if total_cost > *sim_bal {
                let err = format!("Saldo USDT tidak cukup (termasuk fee). Ingin membeli {} BTC (${:.2}), saldo cuma ${:.2}", amount, cost, *sim_bal);
                println!("{}", err);
                let _ = corrector::log_error(state, "VALIDATION_INSUFFICIENT_USDT", &err).await;
                return false;
            }
            
            if *amount <= 0.0 {
                return false;
            }
            true
        },
        Decision::Sell(amount, _) => {
            let btc_bal = state.btc_balance.read().await;
            let value = price * amount;
            
            // Validasi ukuran transaksi minimum
            if value < 5.0 {
                let err = format!("Nilai transaksi jual terlalu kecil (${:.2}), minimum $5.0", value);
                println!("{}", err);
                let _ = corrector::log_error(state, "VALIDATION_MIN_SIZE", &err).await;
                return false;
            }
            
            if *amount > *btc_bal {
                let err = format!("Saldo BTC kurang. Ingin menjual {} BTC, kepemilikan cuma {} BTC", amount, *btc_bal);
                println!("{}", err);
                let _ = corrector::log_error(state, "VALIDATION_INSUFFICIENT_BTC", &err).await;
                return false;
            }
            
            if *amount <= 0.0 {
                return false;
            }
            true
        },
        Decision::Wait => false,
    }
}
