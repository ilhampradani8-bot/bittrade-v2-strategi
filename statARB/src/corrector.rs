use tokio::time::{sleep, Duration};
use chrono::Utc;
use crate::{AppState, add_log_with_level, LogLevel, executor};

pub async fn start_corrector_loop(state: AppState) {
    add_log_with_level(&state, LogLevel::INFO, "Starting system corrector and safety monitor loop...").await;

    loop {
        // Run corrector checks every 60 seconds
        sleep(Duration::from_secs(60)).await;

        let now = Utc::now();
        *state.last_corrector_activity.write().await = now;

        // Perform safety checks
        let active_positions = {
            let active_map = state.active_positions.read().await;
            active_map.clone()
        };

        let pair_stats_opt = {
            let stats_map = state.pair_stats.read().await;
            stats_map.get("ETHUSDT-BTCUSDT").cloned()
        };

        for (pair, pos) in active_positions {
            // FIX: Actionable Corrector: auto force-close open positions after 24 hours
            let duration = now.signed_duration_since(pos.opened_at);
            if duration.num_hours() >= 24 {
                if let Some(stats) = pair_stats_opt.clone() {
                    add_log_with_level(
                        &state,
                        LogLevel::WARN,
                        &format!(
                            "Safety Warning & Execution: Position {} has been open for {} hours. Initiating auto force-close due to timeout (limit: 24h).",
                            pair,
                            duration.num_hours()
                        ),
                    ).await;

                    if let Err(e) = executor::close_position(&state, &pos, &stats, "FORCE_CLOSE_TIMEOUT").await {
                        let err_msg = format!("Failed to force close timed-out position {}: {:?}", pair, e);
                        add_log_with_level(&state, LogLevel::CRITICAL, &err_msg).await;
                        
                        let mut backoff = Duration::from_millis(100);
                        for attempt in 1..=3 {
                            let res = sqlx::query(
                                "INSERT INTO starb_corrections (error_type, reason, severity) VALUES ($1, $2, $3)"
                            )
                            .bind("FORCE_CLOSE_FAILURE")
                            .bind(&err_msg)
                            .bind("CRITICAL")
                            .execute(&state.db)
                            .await;
                            if res.is_ok() {
                                break;
                            }
                            if attempt < 3 {
                                sleep(backoff).await;
                                backoff *= 2;
                            }
                        }
                    }
                } else {
                    add_log_with_level(
                        &state,
                        LogLevel::ERROR,
                        &format!(
                            "Failed to force close position {} because latest pair statistics are not available.",
                            pair
                        )
                    ).await;
                }
            }
        }

        // Database balance history logging
        let bal = *state.simulated_balance.read().await;
        let deployed: f64 = state.active_positions.read().await.values().map(|p| p.deployed_usdt).sum();
        let total = bal + deployed;

        let mut backoff = Duration::from_millis(100);
        let mut success = false;
        for attempt in 1..=3 {
            let db_insert_result = sqlx::query(
                "INSERT INTO starb_balance_history (simulated_balance, deployed_balance, total_equity) VALUES ($1, $2, $3)"
            )
            .bind(bal)
            .bind(deployed)
            .bind(total)
            .execute(&state.db)
            .await;

            if db_insert_result.is_ok() {
                success = true;
                break;
            }
            if attempt < 3 {
                sleep(backoff).await;
                backoff *= 2;
            }
        }

        if !success {
            eprintln!("[ERROR] Corrector failed to log balance history to DB after retries.");
        }
    }
}
