use axum::{
    response::IntoResponse,
    extract::Extension,
    Json,
};
use crate::AppState;
use std::collections::HashMap;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct CalibrateParamItem {
    pub category: String,
    pub stop_loss_limit: f64,
    pub uptrend_tp_trail_trigger: f64,
    pub uptrend_tp_trail_pullback: f64,
}

pub async fn get_parameters(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows: Result<Vec<CalibrateParamItem>, _> = sqlx::query_as::<_, (String, f64, f64, f64)>(
        "SELECT category, stop_loss_limit, uptrend_tp_trail_trigger, uptrend_tp_trail_pullback FROM bot_a_parameters"
    )
    .fetch_all(&state.db)
    .await
    .map(|list| {
        list.into_iter().map(|(cat, sl, tg, pb)| CalibrateParamItem {
            category: cat,
            stop_loss_limit: sl,
            uptrend_tp_trail_trigger: tg,
            uptrend_tp_trail_pullback: pb,
        }).collect()
    });

    match rows {
        Ok(list) => Json(list),
        Err(_) => Json(vec![]),
    }
}

pub async fn update_parameters(
    Extension(state): Extension<AppState>,
    axum::Json(payload): axum::Json<Vec<CalibrateParamItem>>
) -> impl IntoResponse {
    for item in payload {
        let _ = sqlx::query(
            "INSERT INTO bot_a_parameters (category, stop_loss_limit, uptrend_tp_trail_trigger, uptrend_tp_trail_pullback)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (category) DO UPDATE SET
             stop_loss_limit = EXCLUDED.stop_loss_limit,
             uptrend_tp_trail_trigger = EXCLUDED.uptrend_tp_trail_trigger,
             uptrend_tp_trail_pullback = EXCLUDED.uptrend_tp_trail_pullback"
        )
        .bind(item.category)
        .bind(item.stop_loss_limit)
        .bind(item.uptrend_tp_trail_trigger)
        .bind(item.uptrend_tp_trail_pullback)
        .execute(&state.db)
        .await;
    }

    let _ = sync_strategy_parameters(&state.db).await;
    crate::add_log(&state, "[Admin] Parameter strategi diperbarui secara manual oleh admin.").await;

    Json(serde_json::json!({ "status": "success" }))
}

pub async fn run_calibration() -> impl IntoResponse {
    tokio::spawn(async move {
        let _status = tokio::process::Command::new("python3")
            .arg("/root/bittrade-v2-strategi/backtest/optimize_parameters.py")
            .arg("--apply")
            .status()
            .await;
    });
    Json(serde_json::json!({ "status": "started" }))
}

pub async fn sync_strategy_parameters(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, f64, f64, f64)>(
        "SELECT category, stop_loss_limit, uptrend_tp_trail_trigger, uptrend_tp_trail_pullback FROM bot_a_parameters"
    )
    .fetch_all(pool)
    .await?;

    if !rows.is_empty() {
        if let Ok(mut cache) = crate::classifier::get_params_cache().write() {
            for (cat, sl, tg, pb) in rows {
                let mut p = crate::classifier::get_default_params_for_category(&cat, "");
                p.stop_loss_limit = sl;
                p.uptrend_tp_trail_trigger = tg;
                p.uptrend_tp_trail_pullback = pb;
                cache.insert(cat, p);
            }
            println!("[Sync] Berhasil menyinkronkan parameters dari database bot_a_parameters.");
        }
    }
    Ok(())
}
