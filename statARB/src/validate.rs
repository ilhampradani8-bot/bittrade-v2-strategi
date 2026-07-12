use crate::{AppState, PairStats, add_log_with_level, LogLevel, executor};
use chrono::Utc;

pub async fn evaluate_signals(state: &AppState, stats: &PairStats) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let pair_name = "ETHUSDT-BTCUSDT";

    // 1. Data Feed Health & Staleness Check
    if !*state.data_feed_healthy.read().await {
        return Ok(());
    }

    let now = Utc::now();
    let ws_stale_secs = now.signed_duration_since(*state.last_ws_activity.read().await).num_seconds();
    if ws_stale_secs > 15 {
        add_log_with_level(state, LogLevel::WARN, &format!("Evaluating signals skipped: WebSocket price stream is stale by {} seconds.", ws_stale_secs)).await;
        return Ok(());
    }

    // 2. Cooldown Gate
    {
        let cooldowns = state.cooldowns.read().await;
        if let Some(cooldown_until) = cooldowns.get(pair_name) {
            if now < *cooldown_until {
                return Ok(());
            }
        }
    }

    // 3. Circuit Breaker Gate
    {
        let cb_map = state.circuit_breakers.read().await;
        if let Some(cb) = cb_map.get(pair_name) {
            if let Some(paused_until) = cb.paused_until {
                if now < paused_until {
                    return Ok(());
                }
            }
        }
    }

    // 4. Warmup progress check
    {
        let cur_samples = *state.current_samples.read().await;
        if cur_samples < state.min_samples_for_signal {
            return Ok(());
        }
    }

    // Check if we have an active position
    let active_pos_opt = {
        let active_map = state.active_positions.read().await;
        active_map.get(pair_name).cloned()
    };

    // FIX Bug #3: vol_pct — rolling_std / rolling_mean.abs()
    let vol_pct = (stats.rolling_std / stats.rolling_mean.abs()).clamp(0.005, 0.10);

    match active_pos_opt {
        None => {
            // No active position. Check for entry signals.
            let z = stats.z_score;

            // R2 Cointegration Gate
            if stats.r2 < state.min_r2 {
                // R2 is too low, meaning cointegration relationship is currently weak
                return Ok(());
            }

            // Balance Gates: Dynamic position sizing already caps to balance * 40%.
            // What we need to check:
            // 1. Minimum absolute balance: don't trade if saldo terlalu kecil untuk profit > fee
            // 2. required_size is already balance-adaptive from calculate_dynamic_size
            let balance = *state.simulated_balance.read().await;
            let required_size = executor::calculate_dynamic_size(state, stats).await;

            // Gate 1: Minimum effective balance (50 USDT). Below this, fee drag makes trading
            // uneconomical and risks depleting the remaining capital completely.
            let min_effective_balance = 50.0_f64;
            if balance < min_effective_balance {
                return Ok(()); // Suspend entry: saldo terlalu rendah untuk trade bermakna
            }

            // Gate 2: Minimum position size check (required_size is already balance-adaptive,
            // but add explicit floor to ensure we have enough for meaningful trades)
            if required_size < 20.0 || balance < required_size {
                return Ok(());
            }

            // Check if we reached the maximum positions constraint
            let active_count = {
                let active_map = state.active_positions.read().await;
                active_map.len()
            };
            if active_count >= state.max_positions {
                return Ok(());
            }

            // Expected Value (EV) Gate before entry
            let expected_reversion = z.abs() - state.z_exit_threshold;
            if expected_reversion > 0.0 {
                // Implied log move = reversion distance * std_err (rolling_std)
                let implied_log_move = expected_reversion * stats.rolling_std;
                // Estimated capture USD = size * implied_log_move
                let expected_profit = required_size * implied_log_move;
                // Fee cost: total_size * fee_rate * multiplier
                let fee_cost = required_size * state.fee_rate * state.expected_value_buffer_multiplier;

                if expected_profit < fee_cost {
                    // Fail the EV gate due to excessive fee drag relative to edge
                    return Ok(());
                }
            } else {
                return Ok(());
            }

            // Ambang batas masuk dinamis (Adaptive Entry Threshold)
            let dynamic_entry_z = (state.z_entry_threshold + (vol_pct * 25.0)).clamp(state.z_entry_threshold, 2.8);

            if z > dynamic_entry_z {
                // Sell Spread: Asset A (ETH) overvalued, Asset B (BTC) undervalued
                add_log_with_level(state, LogLevel::INFO, &format!("Z-Score {:.2} > adaptive entry {:.2} (Vol {:.3}%, R2 {:.2}%). Triggering SELL_SPREAD.", z, dynamic_entry_z, vol_pct * 100.0, stats.r2 * 100.0)).await;
                executor::open_position(state, stats, "SELL_SPREAD").await?;
            } else if z < -dynamic_entry_z {
                // Buy Spread: Asset A (ETH) undervalued, Asset B (BTC) overvalued
                add_log_with_level(state, LogLevel::INFO, &format!("Z-Score {:.2} < adaptive entry {:.2} (Vol {:.3}%, R2 {:.2}%). Triggering BUY_SPREAD.", z, -dynamic_entry_z, vol_pct * 100.0, stats.r2 * 100.0)).await;
                executor::open_position(state, stats, "BUY_SPREAD").await?;
            }
        }
        Some(pos) => {
            // We have an active position. Check exit conditions.
            let z = stats.z_score;
            let mut should_exit = false;
            let mut reason = "";

            // FIX Bug #1: Use actual unrealized P&L at current prices instead of
            // log-space approximation (old: deployed_usdt * z_delta * rolling_std).
            // rolling_std is log-residual std; multiplying it with dollar size gave
            // inconsistent units. Actual P&L from price movement is always correct.
            let (unrealized_a, unrealized_b) = match pos.direction.as_str() {
                "BUY_SPREAD" => (
                    pos.qty_a * (stats.price_a - pos.entry_price_a),   // Long A: profit when price_a rises
                    pos.qty_b * (pos.entry_price_b - stats.price_b),   // Short B: profit when price_b falls
                ),
                _ => (
                    pos.qty_a * (pos.entry_price_a - stats.price_a),   // Short A: profit when price_a falls
                    pos.qty_b * (stats.price_b - pos.entry_price_b),   // Long B: profit when price_b rises
                ),
            };
            let estimated_capture = unrealized_a + unrealized_b;
            let fee_cost = pos.deployed_usdt * state.fee_rate;

            // Stop loss dinamis berbasis volatilitas
            let dynamic_sl_z = (3.2 + (vol_pct * 35.0)).clamp(3.2, 4.8);

            if z.abs() > dynamic_sl_z {
                should_exit = true;
                reason = "STOP_LOSS_DIVERGENCE";
            } else {
                match pos.direction.as_str() {
                    "BUY_SPREAD" => {
                        // We bought the spread (Long A, Short B)
                        // A. Mean Reversion Take Profit (Ensure estimated capture >= fee cost OR Z is past exit threshold)
                        if z >= -state.z_exit_threshold && estimated_capture >= fee_cost {
                            should_exit = true;
                            reason = "MEAN_REVERSION";
                        }
                        // B. Trailing Take Profit Dinamis
                        else if z > -1.5 && z < -1.0 && pos.entry_z_score < -1.8 && estimated_capture >= fee_cost {
                            should_exit = true;
                            reason = "TRAILING_TAKE_PROFIT";
                        }
                    }
                    "SELL_SPREAD" => {
                        // We sold the spread (Short A, Long B)
                        // A. Mean Reversion Take Profit (Ensure estimated capture >= fee cost OR Z is past exit threshold)
                        if z <= state.z_exit_threshold && estimated_capture >= fee_cost {
                            should_exit = true;
                            reason = "MEAN_REVERSION";
                        }
                        // B. Trailing Take Profit Dinamis
                        else if z < 1.5 && z > 1.0 && pos.entry_z_score > 1.8 && estimated_capture >= fee_cost {
                            should_exit = true;
                            reason = "TRAILING_TAKE_PROFIT";
                        }
                    }
                    _ => {}
                }
            }

            if should_exit {
                add_log_with_level(state, LogLevel::INFO, &format!("Triggering exit for {} position. Reason: {} (Z: {:.2}, Adaptive SL: {:.2}, Est. Capture: ${:.2}, Fee Cost: ${:.2}).", pos.direction, reason, z, dynamic_sl_z, estimated_capture, fee_cost)).await;
                executor::close_position(state, &pos, stats, reason).await?;
            }
        }
    }

    Ok(())
}
