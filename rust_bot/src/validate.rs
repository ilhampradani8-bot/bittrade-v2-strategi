use crate::{AppState, conclude::Decision, corrector};
use chrono::Utc;

pub async fn validate_decision(decision: &Decision, price: f64, state: &AppState) -> bool {
    // 1. Ambal data transaksi terakhir untuk pengecekan Cool-down
    let last_trade: Option<(String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as::<sqlx::Postgres, (String, chrono::DateTime<chrono::Utc>)>(
        "SELECT action, timestamp FROM bot_trading_history WHERE status = 'SUCCESS' ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    if let Some((last_action, last_time)) = last_trade {
        let current_action = match decision {
            Decision::Buy(_) => "BUY",
            Decision::Sell(_) => "SELL",
            Decision::Wait => "WAIT",
        };

        // Jika keputusan sama dengan aksi terakhir dan jaraknya kurang dari 5 menit
        let duration = Utc::now().signed_duration_since(last_time);
        if current_action == last_action && duration.num_minutes() < 5 {
            let err = format!(
                "Mencegah transaksi berulang (Cool-down aktif). Aksi terakhir {} pada {} ({} menit lalu)",
                last_action,
                last_time.with_timezone(&chrono::FixedOffset::east_opt(7 * 3600).unwrap()).format("%H:%M:%S"),
                duration.num_minutes()
            );
            println!("{}", err);
            // Log ke corrector agar terlihat di dashboard
            corrector::log_error(state, "VALIDATION_COOLDOWN", &err).await;
            return false;
        }
    }

    match decision {
        Decision::Buy(amount) => {
            let cost = price * amount;
            let fee = cost * 0.001;
            let total_cost = cost + fee;
            let sim_bal = state.simulated_balance.read().await;
            
            // Validasi ukuran transaksi minimum
            if cost < 5.0 {
                let err = format!("Nilai transaksi beli terlalu kecil (${:.2}), minimum $5.0", cost);
                println!("{}", err);
                corrector::log_error(state, "VALIDATION_MIN_SIZE", &err).await;
                return false;
            }
            
            if total_cost > *sim_bal {
                let err = format!("Saldo USDT tidak cukup (termasuk fee). Ingin membeli {} BTC (${:.2}), saldo cuma ${:.2}", amount, cost, *sim_bal);
                println!("{}", err);
                corrector::log_error(state, "VALIDATION_INSUFFICIENT_USDT", &err).await;
                return false;
            }
            
            if *amount <= 0.0 {
                return false;
            }
            true
        },
        Decision::Sell(amount) => {
            let btc_bal = state.btc_balance.read().await;
            let value = price * amount;
            
            // Validasi ukuran transaksi minimum
            if value < 5.0 {
                let err = format!("Nilai transaksi jual terlalu kecil (${:.2}), minimum $5.0", value);
                println!("{}", err);
                corrector::log_error(state, "VALIDATION_MIN_SIZE", &err).await;
                return false;
            }
            
            if *amount > *btc_bal {
                let err = format!("Saldo BTC kurang. Ingin menjual {} BTC, kepemilikan cuma {} BTC", amount, *btc_bal);
                println!("{}", err);
                corrector::log_error(state, "VALIDATION_INSUFFICIENT_BTC", &err).await;
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
