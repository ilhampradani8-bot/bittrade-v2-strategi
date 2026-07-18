// INI ADALAH FILE risk.rs
use crate::AppState;

/// Modul Manajemen Risiko QPS Berbasis Sharpe & Kelly
/// Fungsi: Menghitung persentase modal yang optimal secara dinamis 
/// berdasarkan performa 20 transaksi SELL terakhir.
pub async fn calculate_dynamic_budget(state: &AppState, default_pct: f64) -> f64 {
    // Ambil notes, price, DAN amount — bukan cuma notes saja
    let records: Result<Vec<(String, f64, f64)>, _> = sqlx::query_as(
        "SELECT notes, price, amount FROM bot_trading_history 
         WHERE action = 'SELL' AND status = 'SUCCESS' 
         AND notes LIKE '%P&L:%' AND strategy_version = $1 
         ORDER BY id DESC LIMIT 20"
    )
    .bind(crate::CURRENT_STRATEGY_VERSION)
    .fetch_all(&state.db)
    .await;

    if let Ok(rows) = records {
        if rows.len() < 5 {
            return default_pct;
        }

        let mut pnls: Vec<f64> = Vec::new();

        for (note, price, amount) in rows {
            if let Some(idx) = note.find("P&L: $") {
                let pnl_str = &note[idx + 6..];
                let clean_pnl = pnl_str.split_whitespace().next().unwrap_or(pnl_str);
                
                if let Ok(val) = clean_pnl.parse::<f64>() {
                    // FIX: bagi dengan modal AKTUAL trade ini (price * amount)
                    // bukan angka tetap $1000
                    let capital_used = price * amount;
                    if capital_used > 0.0 {
                        pnls.push(val / capital_used);
                    }
                }
            }
        }

        if pnls.is_empty() {
            return default_pct;
        }

        let n = pnls.len() as f64;
        let mean_return = pnls.iter().sum::<f64>() / n;
        let variance = pnls.iter().map(|&x| (x - mean_return).powi(2)).sum::<f64>() / n;
        let std_dev = variance.sqrt();

        if std_dev > 0.0001 {
            let sharpe_ratio = mean_return / std_dev;
            let full_kelly = mean_return / variance;
            let quarter_kelly = full_kelly * 0.25;
            
            println!("[QPS-RISK] Sharpe: {:.2} | Full Kelly: {:.1}% | Rekomendasi Alokasi: {:.1}%", 
                sharpe_ratio, full_kelly * 100.0, quarter_kelly * 100.0);

            if quarter_kelly <= 0.0 {
                return 0.0;
            }
            return quarter_kelly.clamp(0.05, 0.35);
        } else if mean_return > 0.0 {
             return 0.30;
        } else {
             return 0.0;
        }
    }

    default_pct
}
