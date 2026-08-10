use crate::{ArbPositionInfo, FundingData};





use chrono::{Datelike, Timelike, TimeZone};

pub fn get_last_settlement_time(t: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
    let hour = t.hour();
    let settlement_hour = if hour >= 16 {
        16
    } else if hour >= 8 {
        8
    } else {
        0
    };

    chrono::Utc
        .with_ymd_and_hms(t.year(), t.month(), t.day(), settlement_hour, 0, 0)
        .unwrap()
}

/// Tentukan apakah perlu MENGUMPULKAN funding payment untuk posisi yang sudah buka
/// Funding dibayarkan setiap 8 jam oleh Binance (00:00, 08:00, 16:00 UTC)
pub fn should_collect_funding(pos: &ArbPositionInfo) -> bool {
    let now = chrono::Utc::now();
    let last_settlement = get_last_settlement_time(now);

    // Posisi harus dibuka sebelum waktu settlement terakhir
    if pos.opened_at >= last_settlement {
        return false;
    }

    // Belum pernah dikumpulkan untuk settlement ini
    match pos.last_funding_payment_at {
        None => true,
        Some(last_payment) => last_payment < last_settlement,
    }
}

/// Tentukan apakah perlu MENUTUP posisi arb
/// Returns Some(alasan) jika harus ditutup, None jika terus hold
///
/// `best_available_fr`: Funding rate tertinggi dari kandidat koin lain yang tersedia saat ini.
///   Jika None, tidak ada kandidat lain → tidak perlu rotasi.
pub fn should_close_arb(pos: &ArbPositionInfo, current_fr: f64, best_available_fr: Option<f64>) -> Option<String> {
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

    // Exit 3: Rotasi Oportunistik
    // Tutup HANYA jika ada koin lain dengan FR minimal 2x lebih tinggi,
    // DAN posisi sudah berjalan minimal 24 jam (sudah dapat 3 kali funding).
    // Tidak ada batas waktu paksa — selama FR positif & tidak ada yang lebih baik, tetap hold.
    let hours_open = chrono::Utc::now()
        .signed_duration_since(pos.opened_at)
        .num_hours();

    if let Some(best_fr) = best_available_fr {
        let current_pos_fr = current_fr.max(0.0);
        if hours_open >= 24 && best_fr > current_pos_fr * 2.0 && best_fr > 0.0 {
            return Some(format!(
                "[Rotasi] Peluang lebih baik ditemukan! FR saat ini {:.4}% → kandidat terbaik {:.4}% (2x lipat). Rotasi posisi.",
                current_pos_fr * 100.0,
                best_fr * 100.0,
            ));
        }
    }

    None
}
