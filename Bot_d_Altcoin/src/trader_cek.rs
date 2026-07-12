use crate::{FundingData, ArbPositionInfo};

pub struct ArbCheckResult {
    pub is_warning: bool,
    pub pattern_type: String,
    pub diagnostic_msg: String,
}

/// Deteksi pola kesalahan saat MEMBUKA posisi arb baru
/// Mencegah entry saat funding rate adalah spike sesaat (noise)
pub fn check_arb_entry(
    fd: &FundingData,
    fr_history: &[f64],  // Riwayat FR beberapa periode terakhir
) -> Option<ArbCheckResult> {
    // Anti-Pattern 1: FOMO Entry - FR saat ini jauh di atas rata-rata historis
    // Ini tanda FR akan segera mean-revert turun, posisi kita tidak menguntungkan lama
    if fr_history.len() >= 3 {
        let avg_fr: f64 = fr_history.iter().sum::<f64>() / fr_history.len() as f64;
        if avg_fr > 0.0 && fd.funding_rate > avg_fr * 3.0 {
            return Some(ArbCheckResult {
                is_warning: true,
                pattern_type: "FOMO_ARB_ENTRY".to_string(),
                diagnostic_msg: format!(
                    "⚠️ FR Spike terdeteksi pada {}! FR saat ini {:.4}% vs rata-rata historis {:.4}%. Kemungkinan noise sesaat.",
                    fd.symbol,
                    fd.funding_rate * 100.0,
                    avg_fr * 100.0,
                ),
            });
        }
    }

    // Anti-Pattern 2: Basis Terlalu Lebar - Futures premium vs spot > 0.5%
    // Jika basis sangat besar, ada risiko basis compression yang merugikan
    if fd.index_price > 0.0 {
        let basis_pct = ((fd.mark_price - fd.index_price) / fd.index_price) * 100.0;
        if basis_pct.abs() > 0.5 {
            return Some(ArbCheckResult {
                is_warning: true,
                pattern_type: "WIDE_BASIS_RISK".to_string(),
                diagnostic_msg: format!(
                    "⚠️ Basis terlalu lebar pada {}! Basis: {:.3}% (Mark: ${:.4} vs Index: ${:.4}). Entry berisiko.",
                    fd.symbol, basis_pct, fd.mark_price, fd.index_price,
                ),
            });
        }
    }

    None
}

/// Deteksi pola kesalahan saat MENUTUP posisi arb
/// Mencegah panic exit saat FR turun sementara
pub fn check_arb_exit(
    pos: &ArbPositionInfo,
    current_fr: f64,
    exit_reason: &str,
) -> Option<ArbCheckResult> {
    // Anti-Pattern: Panic Exit saat FR masih positif tapi baru turun sedikit
    if current_fr > 0.0 && current_fr >= pos.initial_funding_rate * 0.3 {
        if !exit_reason.contains("[Emergency]") && !exit_reason.contains("[FR Negatif]") {
            return Some(ArbCheckResult {
                is_warning: true,
                pattern_type: "PREMATURE_ARB_EXIT".to_string(),
                diagnostic_msg: format!(
                    "⚠️ Keluar terlalu dini dari {} padahal FR masih {:.4}% (initial: {:.4}%). Reason: {}",
                    pos.symbol,
                    current_fr * 100.0,
                    pos.initial_funding_rate * 100.0,
                    exit_reason,
                ),
            });
        }
    }

    None
}
