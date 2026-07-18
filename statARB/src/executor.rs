use chrono::Utc;
use chrono::Duration as ChronoDuration;
use crate::{AppState, PairStats, SpreadPosition, add_log_with_level, LogLevel};

/// Helper: Menghitung ukuran modal posisi secara dinamis (Kelly / Sharpe & Volatility Adaptive)
pub async fn calculate_dynamic_size(state: &AppState, stats: &PairStats) -> f64 {
    let default_size = state.position_size_usdt;
    let balance = *state.simulated_balance.read().await;

    // UPGRADE: Fetch performance logs with retry mechanism
    let mut records: Result<Vec<f64>, _> = Err(sqlx::Error::RowNotFound);
    let mut backoff = tokio::time::Duration::from_millis(100);
    for _ in 1..=3 {
        records = sqlx::query_scalar(
            "SELECT net_pnl FROM starb_trading_history WHERE action LIKE 'CLOSE_%' ORDER BY id DESC LIMIT 20"
        )
        .fetch_all(&state.db)
        .await;
        if records.is_ok() {
            break;
        }
        tokio::time::sleep(backoff).await;
        backoff *= 2;
    }

    let mut performance_multiplier = 1.0;
    if let Ok(pnls) = records {
        if pnls.len() >= 5 {
            let n = pnls.len() as f64;
            let mean_pnl = pnls.iter().sum::<f64>() / n;
            let variance = pnls.iter().map(|&x| (x - mean_pnl).powi(2)).sum::<f64>() / n;
            let std_dev = variance.sqrt();

            if std_dev > 0.0001 {
                let sharpe = mean_pnl / std_dev;
                if sharpe < -0.3 {
                    // Quarter Kelly / Defensive Mode
                    performance_multiplier = 0.5;
                } else if sharpe > 1.0 {
                    performance_multiplier = 1.3;
                }
            }
        }
    }

    // FIX: vol_pct — rolling_std (log-residual std) is already a fractional volatility measure.
    // rolling_mean is now the ETH/BTC price ratio (~0.02), dividing a log-std by a price ratio
    // gives a meaningless number. Use rolling_std directly, same as validate.rs fix.
    let z_strength = (stats.z_score.abs() / 2.0).clamp(0.8, 1.4);
    let vol_pct = (stats.rolling_std / stats.rolling_mean.abs()).clamp(0.005, 0.10);
    let vol_discount = (1.0 - (vol_pct * 15.0)).clamp(0.6, 1.0);

    let optimal_size = default_size * performance_multiplier * z_strength * vol_discount;
    // Cap to 40% of available balance (dynamic adaptation) and never exceed 1.5x default
    optimal_size.clamp(1.0, balance * 0.40).min(default_size * 1.5)
}

pub async fn open_position(
    state: &AppState,
    stats: &PairStats,
    direction: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let pair_name = "ETHUSDT-BTCUSDT";
    let size = calculate_dynamic_size(state, stats).await;

    let price_a = stats.price_a;
    let price_b = stats.price_b;

    // FIX: Weight position allocation based on OLS Beta instead of 50:50 dollar split
    let beta = stats.beta.abs().max(0.01);
    let size_a = size / (1.0 + beta);
    let size_b = (beta * size) / (1.0 + beta);
    
    // FIX: Dynamic Exchange Limits Validation
    let btc_step_size = *state.btc_step_size.read().await;
    let eth_step_size = *state.eth_step_size.read().await;
    let btc_min_notional = *state.btc_min_notional.read().await;
    let eth_min_notional = *state.eth_min_notional.read().await;

    let btc_step_dollar_value = btc_step_size * price_b;
    let eth_step_dollar_value = eth_step_size * price_a;

    let btc_floor = btc_min_notional.max(btc_step_dollar_value);
    let eth_floor = eth_min_notional.max(eth_step_dollar_value);

    if size_b < btc_floor || size_a < eth_floor {
        return Err(format!("Signal rejected: Allocated sizes (ETH: ${:.2}, BTC: ${:.2}) fail to meet exchange limits (ETH floor: ${:.2}, BTC floor: ${:.2})", size_a, size_b, eth_floor, btc_floor).into());
    }

    let qty_a = size_a / price_a;
    let qty_b = size_b / price_b;

    // Deduct simulated balance
    {
        let mut balance = state.simulated_balance.write().await;
        *balance -= size;
    }

    // Insert into database with retry mechanism
    let mut row_id: Option<i32> = None;
    let mut backoff = tokio::time::Duration::from_millis(100);
    for attempt in 1..=3 {
        let db_res = sqlx::query_scalar::<_, i32>(
            "INSERT INTO starb_active_positions 
             (pair_name, direction, entry_z_score, entry_ratio, entry_price_a, entry_price_b, qty_a, qty_b, deployed_usdt, status, opened_at, entry_beta, entry_r2)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'OPEN', CURRENT_TIMESTAMP, $10, $11)
             RETURNING id"
        )
        .bind(pair_name)
        .bind(direction)
        .bind(stats.z_score)
        .bind(stats.current_ratio)
        .bind(price_a)
        .bind(price_b)
        .bind(qty_a)
        .bind(qty_b)
        .bind(size)
        .bind(stats.beta)
        .bind(stats.r2)
        .fetch_one(&state.db)
        .await;

        match db_res {
            Ok(id) => {
                row_id = Some(id);
                break;
            }
            Err(e) => {
                if attempt == 3 {
                    return Err(Box::new(e));
                }
                tokio::time::sleep(backoff).await;
                backoff *= 2;
            }
        }
    }
    let row_id = row_id.unwrap();

    let pos = SpreadPosition {
        id: row_id,
        pair_name: pair_name.to_string(),
        direction: direction.to_string(),
        entry_z_score: stats.z_score,
        entry_ratio: stats.current_ratio,
        entry_price_a: price_a,
        entry_price_b: price_b,
        qty_a,
        qty_b,
        deployed_usdt: size,
        status: "OPEN".to_string(),
        opened_at: Utc::now(),
        exit_price_a: None,
        exit_price_b: None,
        exit_ratio: None,
        exit_z_score: None,
        net_pnl: 0.0,
        closed_at: None,
        entry_beta: Some(stats.beta),
        entry_r2: Some(stats.r2),
    };

    // Insert in-memory
    {
        let mut active_map = state.active_positions.write().await;
        active_map.insert(pair_name.to_string(), pos.clone());
    }

    // Log to trade history with retry mechanism
    let mut backoff = tokio::time::Duration::from_millis(100);
    for _ in 1..=3 {
        let res = sqlx::query(
            "INSERT INTO starb_trading_history 
             (pair_name, action, z_score, ratio, price_a, price_b, amount_a, amount_b, timestamp, notes, beta, r2)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, CURRENT_TIMESTAMP, $9, $10, $11)"
        )
        .bind(pair_name)
        .bind(format!("OPEN_{}", direction))
        .bind(stats.z_score)
        .bind(stats.current_ratio)
        .bind(price_a)
        .bind(price_b)
        .bind(qty_a)
        .bind(qty_b)
        .bind(format!("Opened statistical arbitrage spread position. Direction: {}, Size: ${:.2}, Beta: {:.4}, R2: {:.4}", direction, size, stats.beta, stats.r2))
        .bind(stats.beta)
        .bind(stats.r2)
        .execute(&state.db)
        .await;

        if res.is_ok() {
            break;
        }
        tokio::time::sleep(backoff).await;
        backoff *= 2;
    }

    add_log_with_level(
        state,
        LogLevel::INFO,
        &format!(
            "Opened position {}. Direction: {}. Price A: {:.2}, Price B: {:.2}, Qty A: {:.4}, Qty B: {:.4}",
            pair_name, direction, price_a, price_b, qty_a, qty_b
        ),
    ).await;

    // Log balance history
    let _ = log_balance(state).await;

    Ok(())
}

pub async fn close_position(
    state: &AppState,
    pos: &SpreadPosition,
    stats: &PairStats,
    reason: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let price_a = stats.price_a;
    let price_b = stats.price_b;

    // Calculate Leg A P&L
    let pnl_a = if pos.direction == "BUY_SPREAD" {
        pos.qty_a * (price_a - pos.entry_price_a)
    } else {
        pos.qty_a * (pos.entry_price_a - price_a)
    };

    // Calculate Leg B P&L
    let pnl_b = if pos.direction == "BUY_SPREAD" {
        pos.qty_b * (pos.entry_price_b - price_b) // Short B
    } else {
        pos.qty_b * (price_b - pos.entry_price_b) // Long B
    };

    // Fees: Taker fee 0.04% * 2 legs * 2 actions (open/close) = 0.16%
    let fees = pos.deployed_usdt * state.fee_rate;
    let net_pnl = pnl_a + pnl_b - fees;

    // Return capital + P&L to simulated balance
    {
        let mut balance = state.simulated_balance.write().await;
        *balance += pos.deployed_usdt + net_pnl;
    }

    // Update DB row with retry mechanism
    let mut backoff = tokio::time::Duration::from_millis(100);
    for _ in 1..=3 {
        let res = sqlx::query(
            "UPDATE starb_active_positions 
             SET status = 'CLOSED', exit_price_a = $1, exit_price_b = $2, exit_ratio = $3, exit_z_score = $4, net_pnl = $5, closed_at = CURRENT_TIMESTAMP
             WHERE id = $6"
        )
        .bind(price_a)
        .bind(price_b)
        .bind(stats.current_ratio)
        .bind(stats.z_score)
        .bind(net_pnl)
        .bind(pos.id)
        .execute(&state.db)
        .await;

        if res.is_ok() {
            break;
        }
        tokio::time::sleep(backoff).await;
        backoff *= 2;
    }

    // Remove in-memory
    {
        let mut active_map = state.active_positions.write().await;
        active_map.remove(&pos.pair_name);
    }

    // Update in-memory aggregate statistics
    {
        let mut pnl_val = state.total_pnl.write().await;
        *pnl_val += net_pnl;

        let mut trades_val = state.total_trades.write().await;
        *trades_val += 1;
    }

    // Log to trade history with retry mechanism
    let mut backoff = tokio::time::Duration::from_millis(100);
    for _ in 1..=3 {
        let res = sqlx::query(
            "INSERT INTO starb_trading_history 
             (pair_name, action, z_score, ratio, price_a, price_b, amount_a, amount_b, net_pnl, timestamp, notes, beta, r2)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, CURRENT_TIMESTAMP, $10, $11, $12)"
        )
        .bind(&pos.pair_name)
        .bind(format!("CLOSE_{}", pos.direction))
        .bind(stats.z_score)
        .bind(stats.current_ratio)
        .bind(price_a)
        .bind(price_b)
        .bind(pos.qty_a)
        .bind(pos.qty_b)
        .bind(net_pnl)
        .bind(format!(
            "Closed statistical arbitrage position. Reason: {}, Net PnL: ${:.2} (Leg A: ${:.2}, Leg B: ${:.2}, Fees: ${:.2})",
            reason, net_pnl, pnl_a, pnl_b, fees
        ))
        .bind(stats.beta)
        .bind(stats.r2)
        .execute(&state.db)
        .await;

        if res.is_ok() {
            break;
        }
        tokio::time::sleep(backoff).await;
        backoff *= 2;
    }

    // FIX: Implement cooldown and circuit breakers on exit
    let now = Utc::now();
    let cooldown_time = now + ChronoDuration::seconds(state.cooldown_seconds);
    {
        let mut cooldowns = state.cooldowns.write().await;
        cooldowns.insert(pos.pair_name.clone(), cooldown_time);
    }

    {
        let mut cb_map = state.circuit_breakers.write().await;
        let cb = cb_map.entry(pos.pair_name.clone()).or_insert(crate::PairCircuitBreaker {
            consecutive_sl: 0,
            paused_until: None,
        });

        if reason == "STOP_LOSS_DIVERGENCE" {
            cb.consecutive_sl += 1;
            if cb.consecutive_sl >= state.max_consecutive_sl {
                let pause_until = now + ChronoDuration::minutes(state.pause_duration_mins);
                cb.paused_until = Some(pause_until);
                let alert_msg = format!(
                    "Circuit breaker tripped for {}. Pausing trading until {} (duration: {} mins) due to {} consecutive stop losses.",
                    pos.pair_name, pause_until, state.pause_duration_mins, cb.consecutive_sl
                );
                add_log_with_level(state, LogLevel::CRITICAL, &alert_msg).await;
            }
        } else {
            cb.consecutive_sl = 0;
            cb.paused_until = None;
        }
    }

    add_log_with_level(
        state,
        LogLevel::INFO,
        &format!(
            "Closed position {}. Reason: {}. P&L: ${:.2} (A: ${:.2}, B: ${:.2}, Fees: ${:.2})",
            pos.pair_name, reason, net_pnl, pnl_a, pnl_b, fees
        ),
    ).await;

    // Log balance history
    let _ = log_balance(state).await;

    Ok(())
}

pub async fn recover_positions(state: &AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Fetch open positions from DB
    let rows = sqlx::query_as::<_, DBSpreadPosition>(
        "SELECT id, pair_name, direction, entry_z_score, entry_ratio, entry_price_a, entry_price_b, qty_a, qty_b, deployed_usdt, status, opened_at, entry_beta, entry_r2
         FROM starb_active_positions
         WHERE status = 'OPEN'"
    )
    .fetch_all(&state.db)
    .await?;

    {
        let mut active_map = state.active_positions.write().await;
        for r in rows {
            active_map.insert(
                r.pair_name.clone(),
                SpreadPosition {
                    id: r.id,
                    pair_name: r.pair_name,
                    direction: r.direction,
                    entry_z_score: r.entry_z_score,
                    entry_ratio: r.entry_ratio,
                    entry_price_a: r.entry_price_a,
                    entry_price_b: r.entry_price_b,
                    qty_a: r.qty_a,
                    qty_b: r.qty_b,
                    deployed_usdt: r.deployed_usdt,
                    status: r.status,
                    opened_at: r.opened_at,
                    exit_price_a: None,
                    exit_price_b: None,
                    exit_ratio: None,
                    exit_z_score: None,
                    net_pnl: 0.0,
                    closed_at: None,
                    entry_beta: r.entry_beta,
                    entry_r2: r.entry_r2,
                },
            );
        }
    }

    // 2. Fetch aggregate stats
    let total_trades_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM starb_trading_history WHERE action LIKE 'CLOSE_%'"
    )
    .fetch_one(&state.db)
    .await?;
    *state.total_trades.write().await = total_trades_count as u32;

    let sum_pnl: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(net_pnl), 0.0) FROM starb_active_positions WHERE status = 'CLOSED'"
    )
    .fetch_one(&state.db)
    .await?;
    *state.total_pnl.write().await = sum_pnl;

    add_log_with_level(
        state,
        LogLevel::INFO,
        &format!(
            "Recovered active positions: {}. Recovered history: {} trades, total PnL: ${:.2}",
            state.active_positions.read().await.len(),
            total_trades_count,
            sum_pnl
        ),
    ).await;

    Ok(())
}

async fn log_balance(state: &AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bal = *state.simulated_balance.read().await;
    let deployed: f64 = state.active_positions.read().await.values().map(|p| p.deployed_usdt).sum();
    let total = bal + deployed;

    let mut backoff = tokio::time::Duration::from_millis(100);
    for _ in 1..=3 {
        let res = sqlx::query(
            "INSERT INTO starb_balance_history (simulated_balance, deployed_balance, total_equity) VALUES ($1, $2, $3)"
        )
        .bind(bal)
        .bind(deployed)
        .bind(total)
        .execute(&state.db)
        .await;
        if res.is_ok() {
            break;
        }
        tokio::time::sleep(backoff).await;
        backoff *= 2;
    }

    Ok(())
}

// Database helper struct
#[derive(sqlx::FromRow)]
struct DBSpreadPosition {
    id: i32,
    pair_name: String,
    direction: String,
    entry_z_score: f64,
    entry_ratio: f64,
    entry_price_a: f64,
    entry_price_b: f64,
    qty_a: f64,
    qty_b: f64,
    deployed_usdt: f64,
    status: String,
    opened_at: chrono::DateTime<Utc>,
    entry_beta: Option<f64>,
    entry_r2: Option<f64>,
}
