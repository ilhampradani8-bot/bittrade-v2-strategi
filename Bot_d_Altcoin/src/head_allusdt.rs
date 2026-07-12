use crate::{AppState, FundingData, ArbPositionInfo};
use std::collections::HashMap;

/// Data struktur untuk satu kandidat posisi arb
pub struct ArbCandidate {
    pub fd: FundingData,
    pub annualized_yield: f64,
    pub basis_pct: f64,
}

/// Scan semua funding data dan kembalikan kandidat layak untuk dibuka posisi arb
pub fn get_funding_candidates(
    funding_data: &HashMap<String, FundingData>,
    open_positions: &HashMap<String, ArbPositionInfo>,
    min_fr: f64,       // Default: 0.0001 (0.01%)
    max_positions: usize,
) -> Vec<ArbCandidate> {
    let current_open = open_positions.len();
    if current_open >= max_positions {
        return Vec::new();
    }

    let mut candidates: Vec<ArbCandidate> = Vec::new();

    for (sym, fd) in funding_data.iter() {
        // Skip jika posisi sudah terbuka untuk simbol ini
        if open_positions.contains_key(sym) {
            continue;
        }

        // Filter 1: Funding rate harus >= threshold
        if fd.funding_rate < min_fr {
            continue;
        }

        // Filter 2: Harga harus valid
        if fd.mark_price <= 0.0 || fd.index_price <= 0.0 {
            continue;
        }

        // Filter 3: Hitung basis, harus dalam range yang wajar (< 0.5%)
        let basis_pct = ((fd.mark_price - fd.index_price) / fd.index_price) * 100.0;
        if basis_pct.abs() > 0.5 {
            continue;
        }

        // Hitung annualized yield: FR × 3 periode per hari × 365 hari
        let annualized_yield = fd.funding_rate * 3.0 * 365.0 * 100.0; // dalam persen

        candidates.push(ArbCandidate {
            fd: fd.clone(),
            annualized_yield,
            basis_pct,
        });
    }

    // Urutkan dari APR tertinggi
    candidates.sort_by(|a, b| {
        b.fd.funding_rate.partial_cmp(&a.fd.funding_rate).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Batasi jumlah kandidat sesuai slot yang tersedia
    let available_slots = max_positions - current_open;
    candidates.truncate(available_slots);
    candidates
}

/// Load posisi arb yang masih OPEN dari database ke in-memory state
pub async fn load_arb_positions_from_db(state: &AppState) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, symbol, spot_entry_price, futures_entry_price, position_size_usdt,
                initial_funding_rate, total_funding_collected, funding_payments_count,
                opened_at, last_funding_payment_at
         FROM alt_arb_positions WHERE status = 'OPEN'"
    )
    .fetch_all(&state.db)
    .await?;

    let mut positions = state.arb_positions.write().await;
    let mut total_collected = 0.0_f64;

    for row in rows {
        use sqlx::Row;
        let db_id: i32 = row.get("id");
        let symbol: String = row.get("symbol");
        let spot_entry_price: f64 = row.get("spot_entry_price");
        let futures_entry_price: f64 = row.get("futures_entry_price");
        let position_size_usdt: f64 = row.get("position_size_usdt");
        let initial_funding_rate: f64 = row.get("initial_funding_rate");
        let total_funding_collected: f64 = row.get("total_funding_collected");
        let funding_payments_count: i32 = row.get("funding_payments_count");
        let opened_at: chrono::DateTime<chrono::Utc> = row.get("opened_at");
        let last_funding_payment_at: Option<chrono::DateTime<chrono::Utc>> =
            row.try_get("last_funding_payment_at").ok().flatten();

        total_collected += total_funding_collected;

        positions.insert(
            symbol.clone(),
            ArbPositionInfo {
                db_id,
                symbol,
                spot_entry_price,
                futures_entry_price,
                position_size_usdt,
                initial_funding_rate,
                total_funding_collected,
                funding_payments_count: funding_payments_count as u32,
                opened_at,
                last_funding_payment_at,
                current_mark_price: 0.0,
                current_spot_price: 0.0,
                current_funding_rate: 0.0,
                annualized_yield: initial_funding_rate * 3.0 * 365.0 * 100.0,
                consecutive_negative_fr: 0,
            },
        );
    }

    println!(
        "[CRASH RECOVERY] Loaded {} posisi arb dari DB. Total funding dikumpulkan: ${:.4}",
        positions.len(),
        total_collected
    );

    *state.total_funding_collected.write().await = total_collected;
    Ok(())
}
