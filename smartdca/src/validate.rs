use crate::AppState;
use crate::conclude::Decision;

pub async fn validate_decision(decision: &Decision, _current_price: f64, state: &AppState) -> bool {
    match decision {
        Decision::Buy { layer: _, usdt_to_spend, reason: _ } => {
            let balance = *state.simulated_balance.read().await;
            let layers = *state.layers_filled.read().await;

            // 1. simulated_balance harus >= $10 (minimum order)
            if *usdt_to_spend < 10.0 {
                crate::add_log(state, &format!("[VALIDASI-GAGAL] Jumlah spend ${:.2} kurang dari batas minimal $10.00", usdt_to_spend)).await;
                return false;
            }

            if balance < *usdt_to_spend {
                crate::add_log(state, &format!("[VALIDASI-GAGAL] Saldo USDT ${:.2} kurang dari jumlah spend ${:.2}", balance, usdt_to_spend)).await;
                return false;
            }

            // 2. layers_filled harus < 3 (max 3 layer)
            if layers >= 3 {
                crate::add_log(state, &format!("[VALIDASI-GAGAL] Jumlah layer aktif ({}) sudah mencapai batas maksimal 3", layers)).await;
                return false;
            }

            true
        }

        Decision::Sell { reason } => {
            let btc_bal = *state.btc_balance.read().await;
            let layers = *state.layers_filled.read().await;

            // Sinyal [Darurat] bypass semua validasi lain
            if reason.contains("[Darurat]") {
                if btc_bal > 0.0 {
                    return true;
                } else {
                    crate::add_log(state, "[VALIDASI-GAGAL] Sinyal Darurat SELL diterima tetapi btc_balance kosong!").await;
                    return false;
                }
            }

            // 1. Harus ada btc_balance > 0
            if btc_bal <= 0.0 {
                crate::add_log(state, "[VALIDASI-GAGAL] Sinyal SELL diterima tetapi btc_balance kosong").await;
                return false;
            }

            // 2. layers_filled > 0 (ada posisi aktif)
            if layers == 0 {
                crate::add_log(state, "[VALIDASI-GAGAL] Sinyal SELL diterima tetapi layers_filled kosong").await;
                return false;
            }

            true
        }

        Decision::Wait => false,
    }
}
