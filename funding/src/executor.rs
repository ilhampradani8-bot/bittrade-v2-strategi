use crate::{AppState, ArbPositionInfo, FundingData};

/// Buka posisi arb baru: simulasi beli spot + short futures
/// Return: db_id dari posisi yang baru dibuat
pub async fn open_arb_position(
    fd: &FundingData,
    position_size_usdt: f64,
    state: &AppState,
) -> Result<i32, Box<dyn std::error::Error + Send + Sync>> {
    let spot_entry = fd.index_price;      // Harga spot = index price
    let futures_entry = fd.mark_price;    // Harga futures = mark price
    let fee_spot = position_size_usdt * 0.001;      // 0.1% spot taker fee
    let fee_futures = position_size_usdt * 0.0004;  // 0.04% futures taker fee
    let total_cost = (1.1 * position_size_usdt) + fee_spot + fee_futures;
    let annualized_yield = fd.funding_rate * 3.0 * 365.0 * 100.0;
    let basis_pct = if fd.index_price > 0.0 {
        ((fd.mark_price - fd.index_price) / fd.index_price) * 100.0
    } else { 0.0 };

    // Kurangi saldo simulasi (locked ke posisi)
    {
        let mut bal = state.simulated_balance.write().await;
        *bal -= total_cost;
    }

    // Insert ke DB dan dapatkan ID
    let db_id: i32 = sqlx::query_scalar(
        "INSERT INTO alt_arb_positions
            (symbol, spot_entry_price, futures_entry_price, position_size_usdt,
             initial_funding_rate, total_funding_collected, funding_payments_count, status)
         VALUES ($1, $2, $3, $4, $5, 0.0, 0, 'OPEN')
         RETURNING id"
    )
    .bind(&fd.symbol)
    .bind(spot_entry)
    .bind(futures_entry)
    .bind(position_size_usdt)
    .bind(fd.funding_rate)
    .fetch_one(&state.db)
    .await?;

    // Log ke alt_trading_history sebagai OPEN_ARB
    let notes = format!(
        "[OPEN ARB] Spot Entry: ${:.4} | Futures Entry: ${:.4} | FR: {:.4}% | APR: {:.2}% | Basis: {:.3}% | Fee: ${:.4}",
        spot_entry, futures_entry, fd.funding_rate * 100.0, annualized_yield, basis_pct,
        fee_spot + fee_futures
    );
    sqlx::query(
        "INSERT INTO alt_trading_history (action, price, amount, status, notes) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind("OPEN_ARB")
    .bind(spot_entry)
    .bind(position_size_usdt / spot_entry)  // jumlah koin yang dibeli (simulasi)
    .bind("SUCCESS")
    .bind(&notes)
    .execute(&state.db)
    .await?;

    // Insert ke in-memory arb_positions map
    let now = chrono::Utc::now();
    {
        let mut positions = state.arb_positions.write().await;
        positions.insert(
            fd.symbol.clone(),
            ArbPositionInfo {
                db_id,
                symbol: fd.symbol.clone(),
                spot_entry_price: spot_entry,
                futures_entry_price: futures_entry,
                position_size_usdt,
                initial_funding_rate: fd.funding_rate,
                total_funding_collected: 0.0,
                funding_payments_count: 0,
                opened_at: now,
                last_funding_payment_at: None,
                current_mark_price: futures_entry,
                current_spot_price: spot_entry,
                current_funding_rate: fd.funding_rate,
                annualized_yield,
                consecutive_negative_fr: 0,
            },
        );
    }

    // Update statistik
    {
        let mut count = state.total_positions_opened.write().await;
        *count += 1;
    }

    crate::add_log(
        state,
        &format!(
            "🟢 BUKA ARB {} | FR: {:.4}% | APR: {:.2}% | Size: ${:.0} | Basis: {:.3}%",
            fd.symbol,
            fd.funding_rate * 100.0,
            annualized_yield,
            position_size_usdt,
            basis_pct,
        ),
    )
    .await;

    // Persist snapshot balance immediately
    crate::proses_altcoin::save_balance_snapshot(state).await;

    Ok(db_id)
}

/// Kumpulkan pembayaran funding untuk posisi aktif
pub async fn collect_funding_payment(
    symbol: &str,
    current_fr: f64,
    state: &AppState,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    let position_size;
    let db_id;
    let payments_count;

    // Baca info posisi
    {
        let positions = state.arb_positions.read().await;
        let pos = match positions.get(symbol) {
            Some(p) => p,
            None => return Err("Posisi tidak ditemukan".into()),
        };
        position_size = pos.position_size_usdt;
        db_id = pos.db_id;
        payments_count = pos.funding_payments_count;
    }

    // Kalkulasi pembayaran funding
    // Jika FR positif: Short menerima pembayaran dari Long
    // Payment = Position Size × Funding Rate
    let payment = position_size * current_fr;
    let annualized = current_fr * 3.0 * 365.0 * 100.0;

    if payment <= 0.0 {
        // FR negatif = kita bayar, bukan terima
        crate::add_log(
            state,
            &format!(
                "⚠️ [{}] Funding NEGATIF periode ini: FR={:.4}%, Bayar: ${:.4}",
                symbol, current_fr * 100.0, payment.abs()
            ),
        )
        .await;
    }

    // Tambahkan ke saldo simulasi
    {
        let mut bal = state.simulated_balance.write().await;
        *bal += payment;  // Bisa negatif jika FR negatif
    }

    // Update total funding collected
    {
        let mut total = state.total_funding_collected.write().await;
        *total += payment;
    }

    let now = chrono::Utc::now();

    // Update posisi di in-memory map
    {
        let mut positions = state.arb_positions.write().await;
        if let Some(pos) = positions.get_mut(symbol) {
            pos.total_funding_collected += payment;
            pos.funding_payments_count += 1;
            pos.last_funding_payment_at = Some(now);
            pos.current_funding_rate = current_fr;
            pos.annualized_yield = annualized;
            if current_fr < 0.0 {
                pos.consecutive_negative_fr += 1;
            } else {
                pos.consecutive_negative_fr = 0;
            }
        }
    }

    // Update DB
    sqlx::query(
        "UPDATE alt_arb_positions
         SET total_funding_collected = total_funding_collected + $1,
             funding_payments_count = funding_payments_count + 1,
             last_funding_payment_at = CURRENT_TIMESTAMP
         WHERE id = $2"
    )
    .bind(payment)
    .bind(db_id)
    .execute(&state.db)
    .await?;

    // Log ke alt_funding_log
    sqlx::query(
        "INSERT INTO alt_funding_log
            (symbol, funding_rate, payment_amount, annualized_yield, position_size_usdt)
         VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(symbol)
    .bind(current_fr)
    .bind(payment)
    .bind(annualized)
    .bind(position_size)
    .execute(&state.db)
    .await?;

    if payment > 0.0 {
        crate::add_log(
            state,
            &format!(
                "💰 FUNDING #{} [{}] | FR: {:.4}% | +${:.4} diterima | APR: {:.2}%",
                payments_count + 1, symbol, current_fr * 100.0, payment, annualized
            ),
        )
        .await;
    }

    // Persist snapshot balance immediately
    crate::proses_altcoin::save_balance_snapshot(state).await;

    Ok(payment)
}

/// Tutup posisi arb: simulasi jual spot + tutup short futures
pub async fn close_arb_position(
    symbol: &str,
    reason: &str,
    current_fd: Option<&FundingData>,
    state: &AppState,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    let pos_info;
    {
        let positions = state.arb_positions.read().await;
        pos_info = match positions.get(symbol) {
            Some(p) => p.clone(),
            None => return Err(format!("Posisi {} tidak ditemukan", symbol).into()),
        };
    }

    let current_spot = current_fd.map(|f| f.index_price).unwrap_or(pos_info.spot_entry_price);
    let current_futures = current_fd.map(|f| f.mark_price).unwrap_or(pos_info.futures_entry_price);

    // Hitung P&L dari perubahan harga (harusnya mendekati nol karena delta-neutral)
    let spot_pnl = (current_spot - pos_info.spot_entry_price) * (pos_info.position_size_usdt / pos_info.spot_entry_price);
    let futures_pnl = (pos_info.futures_entry_price - current_futures) * (pos_info.position_size_usdt / pos_info.futures_entry_price);

    // Biaya penutupan
    // Jika dilikuidasi paksa: bayar spot sell fee (0.1%) + liquidation fee (0.5%), tidak ada futures close fee
    let is_liq = reason.contains("[LIQUIDATED]");
    let fee_close = if is_liq {
        pos_info.position_size_usdt * (0.001 + 0.005)
    } else {
        pos_info.position_size_usdt * (0.001 + 0.0004)
    };

    // Total P&L = funding dikumpulkan + spot PnL + futures PnL - biaya
    let total_pnl = pos_info.total_funding_collected + spot_pnl + futures_pnl - fee_close;

    // Kembalikan modal ke saldo simulasi + tambah net PnL
    {
        let mut bal = state.simulated_balance.write().await;
        *bal += (1.1 * pos_info.position_size_usdt) + total_pnl;
    }

    // Update DB: mark CLOSED
    sqlx::query(
        "UPDATE alt_arb_positions SET status = 'CLOSED', closed_at = CURRENT_TIMESTAMP,
         close_reason = $1, close_spot_price = $2, close_futures_price = $3, net_pnl = $4
         WHERE id = $5"
    )
    .bind(reason)
    .bind(current_spot)
    .bind(current_futures)
    .bind(total_pnl)
    .bind(pos_info.db_id)
    .execute(&state.db)
    .await?;

    // Log ke alt_trading_history
    let notes = format!(
        "{} | Funding Terkumpul: ${:.4} | Spot PnL: ${:.4} | Futures PnL: ${:.4} | Fee: ${:.4} | Net P&L: ${:+.4}",
        reason,
        pos_info.total_funding_collected,
        spot_pnl,
        futures_pnl,
        fee_close,
        total_pnl,
    );
    sqlx::query(
        "INSERT INTO alt_trading_history (action, price, amount, status, notes) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind("CLOSE_ARB")
    .bind(current_spot)
    .bind(pos_info.position_size_usdt / pos_info.spot_entry_price)
    .bind(if total_pnl >= 0.0 { "SUCCESS" } else { "LOSS" })
    .bind(&notes)
    .execute(&state.db)
    .await?;

    // Hapus dari in-memory map
    {
        let mut positions = state.arb_positions.write().await;
        positions.remove(symbol);
    }

    // Update statistik
    {
        let mut count = state.total_positions_closed.write().await;
        *count += 1;
    }

    crate::add_log(
        state,
        &format!(
            "🔴 TUTUP ARB {} | Funding: ${:.4} | Net P&L: ${:+.4} | Alasan: {}",
            symbol,
            pos_info.total_funding_collected,
            total_pnl,
            reason,
        ),
    )
    .await;

    // Persist snapshot balance immediately
    crate::proses_altcoin::save_balance_snapshot(state).await;

    Ok(total_pnl)
}
