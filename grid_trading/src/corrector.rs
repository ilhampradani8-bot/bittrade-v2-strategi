use crate::AppState;

pub async fn log_error(state: &AppState, symbol: &str, error_type: &str, reason: &str) {
    let result = sqlx::query(
        "INSERT INTO grid_corrections (symbol, error_type, reason) VALUES ($1, $2, $3)"
    )
    .bind(symbol)
    .bind(error_type)
    .bind(reason)
    .execute(&state.db)
    .await;

    if let Err(e) = result {
        eprintln!("Gagal menyimpan koreksi ke database: {}", e);
    }

    *state.last_corrector_activity.write().await = chrono::Utc::now();
}
