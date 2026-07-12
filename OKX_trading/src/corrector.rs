use crate::AppState;

pub async fn log_error(state: &AppState, error_type: &str, reason: &str) {
    let result = sqlx::query(
        "INSERT INTO okx_corrections (error_type, reason) VALUES ($1, $2)"
    )
    .bind(error_type)
    .bind(reason)
    .execute(&state.db)
    .await;

    if let Err(e) = result {
        eprintln!("Gagal menyimpan koreksi ke database: {}", e);
    }

    // Update active light indicator
    *state.last_corrector_activity.write().await = chrono::Utc::now();
}
