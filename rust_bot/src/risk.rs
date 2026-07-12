use crate::AppState;

/// Modul Manajemen Risiko QPS Berbasis Sharpe & Kelly
/// Fungsi: Menghitung persentase modal yang optimal secara dinamis 
/// berdasarkan performa 20 transaksi SELL terakhir.
pub async fn calculate_dynamic_budget(state: &AppState, default_pct: f64) -> f64 {
    // Ambil 20 transaksi SELL terakhir yang mencatat P&L
    let records: Result<Vec<String>, _> = sqlx::query_scalar(
        "SELECT notes FROM bot_trading_history WHERE action = 'SELL' AND status = 'SUCCESS' AND notes LIKE '%P&L:%' ORDER BY id DESC LIMIT 20"
    )
    .fetch_all(&state.db)
    .await;

    if let Ok(notes_list) = records {
        if notes_list.len() < 5 {
            // Belum cukup data, gunakan default
            return default_pct;
        }

        let mut pnls: Vec<f64> = Vec::new();

        // Ekstrak nilai P&L dari string notes
        for note in notes_list {
            if let Some(idx) = note.find("P&L: $") {
                let pnl_str = &note[idx + 6..];
                // Pisahkan spasi atau karakter lain di akhir string
                let clean_pnl = pnl_str.split_whitespace().next().unwrap_or(pnl_str);
                
                if let Ok(val) = clean_pnl.parse::<f64>() {
                    // Normalisasi P&L relatif terhadap modal kas (asumsi $1000 agar tidak out of scale)
                    // Karena kita tidak merekam ekuitas pastinya saat trade, kita konversi P&L ke persentase kasar
                    // Misalnya profit $2 dari $1000 adalah return 0.002
                    pnls.push(val / 1000.0); 
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

        // Hitung Sharpe Ratio dan Kelly Fraction
        if std_dev > 0.0001 {
            let sharpe_ratio = mean_return / std_dev;
            
            // Full Kelly = mu / sigma^2
            let full_kelly = mean_return / variance;
            
            // Quarter Kelly (Standar Institusional)
            let quarter_kelly = full_kelly * 0.25;
            
            println!("[QPS-RISK] Sharpe: {:.2} | Full Kelly: {:.1}% | Rekomendasi Alokasi: {:.1}%", 
                sharpe_ratio, full_kelly * 100.0, quarter_kelly * 100.0);

            // Jika Sharpe/Kelly negatif, sistem harus berhenti trading (0%)
            if quarter_kelly <= 0.0 {
                return 0.0;
            }

            // Batas alokasi wajar: antara 5% (min) hingga 35% (max)
            return quarter_kelly.clamp(0.05, 0.35);
        } else if mean_return > 0.0 {
             return 0.30; // Max alokasi jika profit konsisten tanpa volatilitas
        } else {
             return 0.0; // Stop loss keras jika return negatif tanpa volatilitas
        }
    }

    default_pct
}
