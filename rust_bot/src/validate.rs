// INI ADALAH FILE validate.rs
use crate::{AppState, conclude::Decision, corrector};
use chrono::Utc;

pub async fn validate_decision(decision: &Decision, symbol: &str, price: f64, state: &AppState) -> bool {
    // 1. Ambil data transaksi terakhir untuk pengecekan Cool-down koin ini
    let last_trade: Option<(String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as::<sqlx::Postgres, (String, chrono::DateTime<chrono::Utc>)>(
        "SELECT action, timestamp FROM bot_trading_history WHERE symbol = $1 AND status = 'SUCCESS' ORDER BY id DESC LIMIT 1"
    )
    .bind(symbol)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let (current_action, reason) = match decision {
        Decision::Buy(_, r) => ("BUY", r.as_str()),
        Decision::Sell(_, r) => ("SELL", r.as_str()),
        Decision::Wait => ("WAIT", ""),
    };

    let is_exempt_sell = reason.starts_with("[Darurat]")
        || reason.starts_with("[Sideways]")
        || reason.starts_with("[Downtrend]")
        || reason.starts_with("[Breakout]")
        || reason.starts_with("[Trending]");

    if !is_exempt_sell {
        // 1. Minimum Holding Time (Mencegah Whipsaw Sell)
        if current_action == "SELL" {
            let last_buy_time: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
                "SELECT timestamp FROM bot_trading_history WHERE symbol = $1 AND action = 'BUY' AND status = 'SUCCESS' ORDER BY id DESC LIMIT 1"
            )
            .bind(symbol)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

            if let Some(buy_time) = last_buy_time {
                let duration = Utc::now().signed_duration_since(buy_time);
                if duration.num_minutes() < 15 {
                    let err = format!(
                        "[{}] Mencegah Whipsaw (Holding time < 15 menit). Beli terakhir {} menit lalu. Sinyal normal ({}) diabaikan.",
                        symbol,
                        duration.num_minutes(),
                        reason
                    );
                    println!("{}", err);
                    return false;
                }
            }
        }

        // 2. Cooldown 10 menit untuk transaksi BUY yang sama (cooldown SELL tetap 5 menit)
        if let Some((last_action, last_time)) = last_trade {
            let duration = Utc::now().signed_duration_since(last_time);
            let cooldown_minutes = if current_action == "BUY" { 10 } else { 5 };
            if current_action == last_action && duration.num_minutes() < cooldown_minutes {
                let err = format!(
                    "[{}] Mencegah transaksi berulang (Cool-down {} menit aktif). Aksi terakhir {} pada {} ({} menit lalu)",
                    symbol,
                    cooldown_minutes,
                    last_action,
                    last_time.with_timezone(&chrono::FixedOffset::east_opt(7 * 3600).unwrap()).format("%H:%M:%S"),
                    duration.num_minutes()
                );
                println!("{}", err);
                corrector::log_error(state, "VALIDATION_COOLDOWN", &err).await;
                return false;
            }
        }
    }

    match decision {
        Decision::Buy(amount, _) => {
            // Check active positions count for this coin (Max 2 layers per coin)
            let active_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM bot_active_positions WHERE symbol = $1"
            )
            .bind(symbol)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

            if active_count >= 2 {
                let err = format!("[{}] Max pyramiding (2 layer) tercapai. Tunggu siklus selesai (posisi aktif saat ini: {}).", symbol, active_count);
                println!("{}", err);
                corrector::log_error(state, "VALIDATION_MAX_PYRAMIDING", &err).await;
                return false;
            }

            // Larang pyramiding jika harga belum turun 0.3% dari entry terakhir
            if active_count >= 1 {
                let last_entry_price: Option<f64> = sqlx::query_scalar(
                    "SELECT buy_price FROM bot_active_positions WHERE symbol = $1 ORDER BY id DESC LIMIT 1"
                )
                .bind(symbol)
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None);

                if let Some(last_price) = last_entry_price {
                    let required_discount = last_price * 0.997; // Harus turun minimal 0.3%
                    if price > required_discount {
                        let err = format!(
                            "[{}] Pyramiding ditolak: harga ${:.4} belum turun 0.3% dari entry terakhir ${:.4} (butuh <= ${:.4})",
                            symbol, price, last_price, required_discount
                        );
                        println!("{}", err);
                        corrector::log_error(state, "VALIDATION_PYRAMID_PRICE", &err).await;
                        return false;
                    }
                }
            }

            let cost = price * amount;
            let fee = cost * 0.001;
            let total_cost = cost + fee;
            let sim_bal = state.simulated_balance.read().await;
            
            // Validasi ukuran transaksi minimum
            if cost < 5.0 {
                let err = format!("[{}] Nilai transaksi beli terlalu kecil (${:.2}), minimum $5.0", symbol, cost);
                println!("{}", err);
                corrector::log_error(state, "VALIDATION_MIN_SIZE", &err).await;
                return false;
            }
            
            if total_cost > *sim_bal {
                let err = format!("[{}] Saldo USDT tidak cukup. Ingin membeli {} (${:.2}), saldo cuma ${:.2}", symbol, amount, cost, *sim_bal);
                println!("{}", err);
                corrector::log_error(state, "VALIDATION_INSUFFICIENT_USDT", &err).await;
                return false;
            }
            
            if *amount <= 0.0 {
                return false;
            }
            true
        },
        Decision::Sell(amount, _) => {
            let coin_holding: f64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount), 0.0) FROM bot_active_positions WHERE symbol = $1"
            )
            .bind(symbol)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0.0);

            let value = price * amount;
            
            // Validasi ukuran transaksi minimum
            if value < 5.0 {
                let err = format!("[{}] Nilai transaksi jual terlalu kecil (${:.2}), minimum $5.0", symbol, value);
                println!("{}", err);
                corrector::log_error(state, "VALIDATION_MIN_SIZE", &err).await;
                return false;
            }
            
            if *amount > coin_holding {
                let err = format!("[{}] Saldo koin kurang. Ingin menjual {} koin, kepemilikan cuma {} koin", symbol, amount, coin_holding);
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
