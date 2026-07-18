use crate::AppState;

pub async fn calculate_dynamic_budget(state: &AppState, default_pct: f64, symbol: &str) -> f64 {
    let records: Result<Vec<(String, f64, f64)>, _> = sqlx::query_as(
        "SELECT notes, price, amount FROM grid_trading_history 
         WHERE symbol = $1 AND action = 'SELL' AND status = 'SUCCESS' 
         AND notes LIKE '%P&L:%' 
         ORDER BY id DESC LIMIT 20"
    )
    .bind(symbol)
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
            
            println!("[GRID-RISK] [{}] Sharpe: {:.2} | Full Kelly: {:.1}% | Rekomendasi Alokasi: {:.1}%", 
                symbol, sharpe_ratio, full_kelly * 100.0, quarter_kelly * 100.0);

            if quarter_kelly <= 0.0 {
                return 0.0;
            }
            return quarter_kelly.clamp(0.05, 0.35); 
        } else if mean_return > 0.0 {
             return 0.20; 
        } else {
             return 0.0;
        }
    }

    default_pct
}
