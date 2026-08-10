use crate::AppState;

pub async fn log_error(state: &AppState, error_type: &str, reason: &str) {
    *state.last_corrector_activity.write().await = chrono::Utc::now();
    eprintln!("[CORRECTOR] {} | {}", error_type, reason);
    let _ = sqlx::query(
        "INSERT INTO alt_corrections (error_type, reason, timestamp) VALUES ($1, $2, CURRENT_TIMESTAMP)"
    )
    .bind(error_type)
    .bind(reason)
    .execute(&state.db)
    .await;
}
