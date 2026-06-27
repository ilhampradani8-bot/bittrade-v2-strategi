use sqlx::postgres::PgPoolOptions;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;
use chrono::Utc;
use axum::{
    routing::get,
    response::{Html, Json, IntoResponse},
    Extension, Router,
};
use tower_http::cors::CorsLayer;

mod corrector;
mod executor;
mod conclude;
mod validate;
mod get;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub simulated_balance: Arc<RwLock<f64>>,
    pub btc_balance: Arc<RwLock<f64>>,
    pub current_btc_price: Arc<RwLock<f64>>,
    pub logs: Arc<RwLock<Vec<String>>>,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub market_regime: Arc<RwLock<String>>,
    pub volatility: Arc<RwLock<f64>>,
    
    // Process indicators
    pub last_ws_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_conclude_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_validate_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_executor_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_corrector_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct TradeHistory {
    pub id: i32,
    pub action: String,
    pub price: f64,
    pub amount: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub status: Option<String>,
    pub notes: Option<String>,
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct CorrectionLog {
    pub id: i32,
    pub error_type: String,
    pub reason: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct BalanceHistory {
    pub id: i32,
    pub simulated_balance: f64,
    pub btc_balance: f64,
    pub btc_value: f64,
    pub total_value: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(serde::Serialize)]
pub struct StatusResponse {
    pub simulated_balance: f64,
    pub btc_balance: f64,
    pub current_btc_price: f64,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub ws_active: bool,
    pub conclude_active: bool,
    pub validate_active: bool,
    pub executor_active: bool,
    pub corrector_active: bool,
    pub market_regime: String,
    pub winrate: f64,
    pub market_volatility: f64,
}

pub async fn add_log(state: &AppState, msg: &str) {
    let ts = Utc::now().with_timezone(&chrono::FixedOffset::east_opt(7 * 3600).unwrap()).format("%Y-%m-%d %H:%M:%S").to_string();
    let formatted = format!("[{}] {}", ts, msg);
    println!("{}", formatted);
    let mut logs = state.logs.write().await;
    logs.push(formatted);
    if logs.len() > 100 {
        logs.remove(0);
    }
}

async fn reconstruct_balance(pool: &sqlx::PgPool) -> (f64, f64) {
    let mut usdt = 1000.0;
    let mut btc = 0.0;
    
    let trades: Vec<TradeHistory> = sqlx::query_as::<_, TradeHistory>(
        "SELECT id, action, price, amount, timestamp, status, notes FROM bot_trading_history WHERE status = 'SUCCESS' ORDER BY id ASC"
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    
    for trade in trades {
        let cost = trade.price * trade.amount;
        let fee = cost * 0.001;
        if trade.action == "BUY" {
            usdt -= cost + fee;
            btc += trade.amount;
        } else if trade.action == "SELL" {
            usdt += cost - fee;
            btc -= trade.amount;
        }
    }
    (usdt, btc)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Memulai Bot Trading BTC...");
    
    // Load config dari parent dir .env
    dotenvy::from_filename("../.env").ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    // Setup DB
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    // Inisialisasi DB (Buat tabel jika belum ada)
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS bot_trading_history (
            id SERIAL PRIMARY KEY,
            action VARCHAR(50) NOT NULL,
            price FLOAT NOT NULL,
            amount FLOAT NOT NULL,
            timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
            status VARCHAR(50),
            notes TEXT
        );"
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS bot_corrections (
            id SERIAL PRIMARY KEY,
            error_type VARCHAR(255) NOT NULL,
            reason TEXT NOT NULL,
            timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        );"
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS btc_klines (
            open_time TIMESTAMPTZ PRIMARY KEY,
            open_price FLOAT NOT NULL,
            high_price FLOAT NOT NULL,
            low_price FLOAT NOT NULL,
            close_price FLOAT NOT NULL,
            volume FLOAT NOT NULL
        );"
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS bot_balance_history (
            id SERIAL PRIMARY KEY,
            simulated_balance FLOAT NOT NULL,
            btc_balance FLOAT NOT NULL,
            btc_value FLOAT NOT NULL,
            total_value FLOAT NOT NULL,
            timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        );"
    ).execute(&pool).await?;

    // Sync 100 data KLine pertama saat startup
    println!("Melakukan sinkronisasi data historis awal dari Binance...");
    if let Err(e) = get::sync_klines(&pool, 100).await {
        eprintln!("Warning: Gagal sync data kline awal: {}", e);
    }

    let (initial_sim, initial_btc) = reconstruct_balance(&pool).await;
    println!("Rekonstruksi saldo dari riwayat transaksi: USDT: ${:.2}, BTC: {:.6}", initial_sim, initial_btc);

    let now = chrono::Utc::now();
    let state = AppState {
        db: pool,
        simulated_balance: Arc::new(RwLock::new(initial_sim)),
        btc_balance: Arc::new(RwLock::new(initial_btc)),
        current_btc_price: Arc::new(RwLock::new(0.0)),
        logs: Arc::new(RwLock::new(Vec::new())),
        start_time: now,
        market_regime: Arc::new(RwLock::new("SIDEWAYS".to_string())),
        volatility: Arc::new(RwLock::new(0.0)),
        last_ws_activity: Arc::new(RwLock::new(now)),
        last_conclude_activity: Arc::new(RwLock::new(now)),
        last_validate_activity: Arc::new(RwLock::new(now)),
        last_executor_activity: Arc::new(RwLock::new(now)),
        last_corrector_activity: Arc::new(RwLock::new(now)),
    };

    // Pemicu insert saldo awal ke balance history jika kosong
    {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bot_balance_history")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);
        if count == 0 {
            let sim_bal = *state.simulated_balance.read().await;
            let btc_bal = *state.btc_balance.read().await;
            sqlx::query(
                "INSERT INTO bot_balance_history (simulated_balance, btc_balance, btc_value, total_value) VALUES ($1, $2, $3, $4)"
            )
            .bind(sim_bal)
            .bind(btc_bal)
            .bind(0.0)
            .bind(sim_bal)
            .execute(&state.db)
            .await
            .ok();
        }
    }

    // 1. Spawn WebSocket Price Listener di background (Real-time)
    let listener_state = state.clone();
    tokio::spawn(get::start_price_listener(listener_state));

    // 2. Jalankan Background Loop Trader (Eksekusi / Analisa tiap menit)
    let worker_state = state.clone();
    tokio::spawn(async move {
        // Beri waktu 3 detik agar websocket mendapatkan harga awal
        sleep(Duration::from_secs(3)).await;

        loop {
            // Sinkronkan 5 KLine terbaru agar data historis di db selalu lengkap & update
            if let Err(e) = get::sync_klines(&worker_state.db, 5).await {
                eprintln!("Gagal sync 5 kline terbaru: {}", e);
            }

            // Dapatkan harga dari memori
            let mut price = { *worker_state.current_btc_price.read().await };
            
            // Fallback REST API jika WebSocket tidak aktif atau belum ada harga
            if price <= 0.0 {
                if let Ok(rest_p) = get::get_rest_price().await {
                    let mut price_lock = worker_state.current_btc_price.write().await;
                    *price_lock = rest_p;
                    price = rest_p;
                    add_log(&worker_state, &format!("[FALLBACK] Berhasil mengambil harga BTC via REST API: ${:.2}", price)).await;
                }
            }
            
            if price > 0.0 {
                add_log(&worker_state, &format!("Siklus Menit Baru. Harga BTC Terakhir: ${:.2}", price)).await;
                
                // Conclude: Analisa dan buat keputusan
                *worker_state.last_conclude_activity.write().await = chrono::Utc::now();
                let decision = conclude::analyze_market(price, &worker_state).await;
                add_log(&worker_state, &format!("Keputusan analis: {:?}", decision)).await;

                // Validate: Validasi apakah aman untuk transaksi
                *worker_state.last_validate_activity.write().await = chrono::Utc::now();
                let is_valid = validate::validate_decision(&decision, price, &worker_state).await;

                // Executor: Lakukan eksekusi (simulasi)
                if is_valid {
                    *worker_state.last_executor_activity.write().await = chrono::Utc::now();
                    add_log(&worker_state, "Mengeksekusi transaksi...").await;
                    match executor::execute_trade(&decision, price, &worker_state).await {
                        Ok(_) => {
                            let bal = worker_state.simulated_balance.read().await;
                            let btc = worker_state.btc_balance.read().await;
                            add_log(&worker_state, &format!("Eksekusi sukses. Saldo: ${:.2}, BTC: {:.4}", *bal, *btc)).await;
                        },
                        Err(e) => {
                            let err_msg = e.to_string();
                            add_log(&worker_state, &format!("Eksekusi Gagal: {}", err_msg)).await;
                            corrector::log_error(&worker_state, "EXECUTION_ERROR", &err_msg).await;
                        }
                    }
                } else {
                    add_log(&worker_state, "Validasi gagal / keputusan WAIT. Tidak ada eksekusi.").await;
                }

                // Simpan perkembangan modal (simulated balance + value of btc holdings) ke database
                let sim_bal = *worker_state.simulated_balance.read().await;
                let btc_bal = *worker_state.btc_balance.read().await;
                let btc_val = btc_bal * price;
                let total_val = sim_bal + btc_val;
                sqlx::query(
                    "INSERT INTO bot_balance_history (simulated_balance, btc_balance, btc_value, total_value) VALUES ($1, $2, $3, $4)"
                )
                .bind(sim_bal)
                .bind(btc_bal)
                .bind(btc_val)
                .bind(total_val)
                .execute(&worker_state.db)
                .await
                .ok();

            } else {
                add_log(&worker_state, "Menunggu harga awal dari WebSocket...").await;
            }

            // Tunggu 1 menit sebelum loop selanjutnya
            sleep(Duration::from_secs(60)).await;
        }
    });

    // 3. Jalankan HTTP Web Server (Axum) untuk Dashboard
    let app = Router::new()
        .route("/", get(serve_dashboard))
        .route("/api/status", get(get_status))
        .route("/api/history", get(get_history))
        .route("/api/corrections", get(get_corrections))
        .route("/api/logs", get(get_logs))
        .route("/api/balance_history", get(get_balance_history))
        .layer(CorsLayer::permissive())
        .layer(Extension(state));

    let port = 8087;
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    println!("Web Dashboard berjalan di http://localhost:{}", port);
    println!("Jika menggunakan VPS, buka http://<IP_VPS>:{}", port);
    
    axum::serve(listener, app).await?;

    Ok(())
}

// Handlers untuk HTTP Server
async fn serve_dashboard() -> impl IntoResponse {
    let html_content = std::fs::read_to_string("../dashboard.html")
        .or_else(|_| std::fs::read_to_string("dashboard.html"))
        .unwrap_or_else(|_| "<h1>Dashboard HTML file not found</h1>".to_string());
    Html(html_content)
}

async fn get_status(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let sim_bal = state.simulated_balance.read().await;
    let btc_bal = state.btc_balance.read().await;
    let price = state.current_btc_price.read().await;
    let regime = state.market_regime.read().await;
    let vol = state.volatility.read().await;
    
    let now = chrono::Utc::now();
    let ws_active = now.signed_duration_since(*state.last_ws_activity.read().await).num_seconds() < 3;
    let conclude_active = now.signed_duration_since(*state.last_conclude_activity.read().await).num_seconds() < 3;
    let validate_active = now.signed_duration_since(*state.last_validate_activity.read().await).num_seconds() < 3;
    let executor_active = now.signed_duration_since(*state.last_executor_activity.read().await).num_seconds() < 3;
    let corrector_active = now.signed_duration_since(*state.last_corrector_activity.read().await).num_seconds() < 10;

    // Hitung Winrate
    let total_sells: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bot_trading_history WHERE action = 'SELL' AND status = 'SUCCESS'"
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let win_sells: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bot_trading_history WHERE action = 'SELL' AND status = 'SUCCESS' AND notes LIKE '%P&L: +%'"
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let winrate = if total_sells > 0 {
        (win_sells as f64 / total_sells as f64) * 100.0
    } else {
        0.0
    };

    Json(StatusResponse {
        simulated_balance: *sim_bal,
        btc_balance: *btc_bal,
        current_btc_price: *price,
        start_time: state.start_time,
        ws_active,
        conclude_active,
        validate_active,
        executor_active,
        corrector_active,
        market_regime: regime.clone(),
        winrate,
        market_volatility: *vol,
    })
}

async fn get_history(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, TradeHistory>(
        "SELECT id, action, price, amount, timestamp, status, notes FROM bot_trading_history ORDER BY id DESC LIMIT 50"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn get_corrections(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, CorrectionLog>(
        "SELECT id, error_type, reason, timestamp FROM bot_corrections ORDER BY id DESC LIMIT 50"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn get_logs(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let logs = state.logs.read().await;
    Json(logs.clone())
}

async fn get_balance_history(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, BalanceHistory>(
        "SELECT id, simulated_balance, btc_balance, btc_value, total_value, timestamp FROM bot_balance_history ORDER BY id ASC LIMIT 100"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    Json(rows)
}
