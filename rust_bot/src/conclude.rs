use crate::AppState;

#[derive(Debug)]
pub enum Decision {
    Buy(f64),
    Sell(f64),
    Wait,
}

pub async fn analyze_market(price: f64, state: &AppState) -> Decision {
    let sim_bal = *state.simulated_balance.read().await;
    let btc_bal = *state.btc_balance.read().await;

    // 1. Cek Pengaman (Stop Loss 2% & Take Profit 3%)
    let last_buy_price: Option<f64> = sqlx::query_scalar(
        "SELECT price FROM bot_trading_history WHERE action = 'BUY' AND status = 'SUCCESS' ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    if let Some(buy_p) = last_buy_price {
        if btc_bal > 0.0001 {
            // A. Emergency Stop Loss (2%)
            if price <= buy_p * 0.98 {
                println!("[ANALIS] 🚨 EMERGENCY STOP LOSS TERPICU! Harga Beli Terakhir: ${:.2} | Harga Saat Ini: ${:.2} (Turun >= 2%)", buy_p, price);
                return Decision::Sell(btc_bal);
            }
            // B. Take Profit (3%)
            if price >= buy_p * 1.03 {
                println!("[ANALIS] 🎯 TAKE PROFIT TERPICU! Harga Beli Terakhir: ${:.2} | Harga Saat Ini: ${:.2} (Naik >= 3%)", buy_p, price);
                return Decision::Sell(btc_bal);
            }
        }
    }

    // 2. Tarik data kline historis untuk deteksi regime pasar
    let rows: Vec<f64> = sqlx::query_scalar(
        "SELECT close_price FROM btc_klines ORDER BY open_time DESC LIMIT 20"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    if rows.len() < 20 {
        println!("[ANALIS] Data historis di DB belum mencukupi ({} dari 20). Menunggu...", rows.len());
        return Decision::Wait;
    }

    // Hitung SMA-5 & SMA-15
    let sum_5: f64 = rows[0..5].iter().sum();
    let sma_5 = sum_5 / 5.0;

    let sum_15: f64 = rows[0..15].iter().sum();
    let sma_15 = sum_15 / 15.0;

    // Hitung Standar Deviasi (Volatility) untuk Bollinger Bands
    let variance: f64 = rows[0..15].iter()
        .map(|&p| {
            let diff = p - sma_15;
            diff * diff
        })
        .sum::<f64>() / 15.0;
    let std_dev = variance.sqrt();

    let upper_band = sma_15 + (2.0 * std_dev);
    let lower_band = sma_15 - (2.0 * std_dev);

    // Deteksi Perubahan Mendadak (Spike / Sudden Pump or Dump)
    let last_change = rows[0] - rows[1];
    let is_sudden_pump = last_change > (2.5 * std_dev) && std_dev > 5.0;
    let is_sudden_dump = last_change < -(2.5 * std_dev) && std_dev > 5.0;

    // Tentukan State Pasar (Trending vs Sideways) secara dinamis/persentase
    // Batasan: Jika volatilitas standar deviasi di bawah 0.075% dari harga BTC saat ini, pasar dideteksi Sideways
    let volatility_pct = (std_dev / price) * 100.0;
    let is_sideways = volatility_pct < 0.075;

    let regime_str = if is_sideways {
        "SIDEWAYS".to_string()
    } else {
        if sma_5 > sma_15 {
            "BULLISH".to_string()
        } else {
            "BEARISH".to_string()
        }
    };
    *state.market_regime.write().await = regime_str.clone();

    println!(
        "[ANALIS] State: {} | Volatilitas (StdDev): ${:.2} ({:.3}%) | Bollinger Band: [${:.2} - ${:.2}]", 
        regime_str, 
        std_dev,
        volatility_pct,
        lower_band, 
        upper_band
    );

    // --- PEMILIHAN STRATEGI BERDASARKAN KONDISI PASAR ---

    // A. DETEKSI DUMP MENDADAK (Defensive Action)
    if is_sudden_dump {
        if btc_bal > 0.0001 {
            println!("[ANALIS] ⚠️ DETEKSI DUMP MENDADAK! Mengamankan aset, JUAL semua.");
            return Decision::Sell(btc_bal);
        }
    }

    // B. DETEKSI PUMP MENDADAK (Breakout Action)
    if is_sudden_pump {
        if sim_bal > 10.0 {
            println!("[ANALIS] ⚡ DETEKSI BREAKOUT PUMP! Masuk pasar segera.");
            let budget = sim_bal * 0.30;
            return Decision::Buy(budget / price);
        }
    }

    // C. STRATEGI UTAMA: SIDEWAYS (Bollinger Bands Mean Reversion)
    if is_sideways {
        if price <= lower_band {
            if sim_bal > 10.0 {
                println!("[ANALIS-SIDEWAYS] Harga menyentuh batas bawah BB (${:.2}). Spekulasi BUY.", lower_band);
                let budget = sim_bal * 0.20;
                return Decision::Buy(budget / price);
            }
        } else if price >= upper_band {
            if btc_bal > 0.0001 {
                println!("[ANALIS-SIDEWAYS] Harga menyentuh batas atas BB (${:.2}). Ambil profit SELL.", upper_band);
                return Decision::Sell(btc_bal);
            }
        }
    } 
    // D. STRATEGI UTAMA: TRENDING (SMA Crossover)
    else {
        if sma_5 > sma_15 {
            if sim_bal > 10.0 {
                println!("[ANALIS-TRENDING] Golden Cross Terdeteksi (SMA-5 > SMA-15). Sinyal BUY.");
                let budget = sim_bal * 0.25;
                return Decision::Buy(budget / price);
            }
        } else if sma_5 < sma_15 {
            if btc_bal > 0.0001 {
                println!("[ANALIS-TRENDING] Death Cross Terdeteksi (SMA-5 < SMA-15). Sinyal SELL.");
                return Decision::Sell(btc_bal);
            }
        }
    }

    Decision::Wait
}
