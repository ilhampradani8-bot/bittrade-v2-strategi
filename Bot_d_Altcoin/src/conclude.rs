use crate::{ArbPositionInfo, FundingData};





/// Tentukan apakah perlu MENGUMPULKAN funding payment untuk posisi yang sudah buka
/// Funding dibayarkan setiap 8 jam oleh Binance (00:00, 08:00, 16:00 UTC)
pub fn should_collect_funding(pos: &ArbPositionInfo) -> bool {
    // Cek apakah sudah waktunya kumpulkan funding berdasarkan waktu pembayaran terakhir
    // Jika sudah melewati waktu funding dan belum dikumpulkan dalam 2 menit terakhir
    match pos.last_funding_payment_at {
        None => {
            // Belum pernah dapat pembayaran — cek apakah sudah 8 jam sejak entry
            let hours_open = chrono::Utc::now()
                .signed_duration_since(pos.opened_at)
                .num_hours();
            hours_open >= 8
        }
        Some(last_payment) => {
            // Harus minimal 7.9 jam sejak pembayaran terakhir (buffer 6 menit)
            let hours_since = chrono::Utc::now()
                .signed_duration_since(last_payment)
                .num_minutes();
            hours_since >= 474  // 7 jam 54 menit
        }
    }
}

/// Tentukan apakah perlu MENUTUP posisi arb
/// Returns Some(alasan) jika harus ditutup, None jika terus hold
pub fn should_close_arb(pos: &ArbPositionInfo, current_fr: f64) -> Option<String> {
    // Exit 1: Funding Rate berbalik negatif selama 2 periode berturut-turut
    // Di struct ArbPositionInfo kita track `consecutive_negative_fr`
    if pos.consecutive_negative_fr >= 2 {
        return Some(format!(
            "[FR Negatif] Funding Rate negatif {:.4}% selama 2 periode berturut. Tutup posisi.",
            current_fr * 100.0
        ));
    }

    // Exit 2: FR sangat negatif dalam satu periode (segera keluar)
    if current_fr < -0.005 / 100.0 {
        return Some(format!(
            "[Emergency] Funding Rate sangat negatif ({:.4}%). Exit darurat untuk lindungi modal.",
            current_fr * 100.0
        ));
    }

    // Exit 3: Posisi sudah terlalu lama (> 7 hari) — rotasi ke peluang lebih baik
    let days_open = chrono::Utc::now()
        .signed_duration_since(pos.opened_at)
        .num_hours() as f64 / 24.0;
    if days_open >= 7.0 {
        return Some(format!(
            "[Rotasi] Posisi sudah {:.1} hari. Tutup untuk rotasi ke peluang FR lebih tinggi.",
            days_open
        ));
    }

    None
}
