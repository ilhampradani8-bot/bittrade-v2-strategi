use crate::{AppState, FundingData};
use crate::corrector;

/// Validasi sebelum MEMBUKA posisi arb baru
pub async fn validate_open_arb(
    fd: &FundingData,
    position_size_usdt: f64,
    state: &AppState,
) -> bool {
    let sim_bal = *state.simulated_balance.read().await;
    let open_positions = state.arb_positions.read().await;
    let max_pos = state.max_positions;

    // Cek 1: Sudah ada posisi untuk simbol ini?
    if open_positions.contains_key(&fd.symbol) {
        return false;
    }

    // Cek 2: Jumlah posisi sudah maksimum?
    if open_positions.len() >= max_pos {
        let msg = format!(
            "Max posisi {} tercapai ({}/{}). Skip entry {}.",
            max_pos, open_positions.len(), max_pos, fd.symbol
        );
        corrector::log_error(state, "VALIDATION_MAX_POSITIONS", &msg).await;
        return false;
    }

    // Cek 3: Saldo mencukupi (butuh 2x untuk simulasikan kedua leg + fee)
    // Spot: position_size_usdt + 0.1% fee
    // Futures: position_size_usdt margin (simulasi, asumsikan 1x leverage)
    let required = position_size_usdt * 2.0 * 1.001;
    if sim_bal < required {
        let msg = format!(
            "Saldo tidak cukup untuk entry {}. Butuh ${:.2}, tersedia ${:.2}.",
            fd.symbol, required, sim_bal
        );
        corrector::log_error(state, "VALIDATION_INSUFFICIENT_BALANCE", &msg).await;
        return false;
    }

    // Cek 4: Harga harus valid
    if fd.mark_price <= 0.0 || fd.index_price <= 0.0 {
        return false;
    }

    true
}

/// Validasi sebelum MENUTUP posisi arb
pub async fn validate_close_arb(symbol: &str, state: &AppState) -> bool {
    let positions = state.arb_positions.read().await;
    if !positions.contains_key(symbol) {
        eprintln!("[VALIDATE] Posisi {} tidak ditemukan untuk ditutup.", symbol);
        return false;
    }
    true
}
