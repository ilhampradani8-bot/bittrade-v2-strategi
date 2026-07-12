use axum::{
    routing::get,
    Router,
    response::IntoResponse,
    Json,
};
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;
use sqlx::{postgres::PgPoolOptions, PgPool};
use chrono::Utc;
use serde::Serialize;

mod corrector;
mod executor;
mod conclude;
mod validate;
mod get;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub simulated_balance: Arc<RwLock<f64>>,
    pub btc_balance: Arc<RwLock<f64>>,
    pub current_btc_price: Arc<RwLock<f64>>,
    pub eth_balance: Arc<RwLock<f64>>,
    pub current_eth_price: Arc<RwLock<f64>>,
    pub bnb_balance: Arc<RwLock<f64>>,
    pub current_bnb_price: Arc<RwLock<f64>>,
    pub sol_balance: Arc<RwLock<f64>>,
    pub current_sol_price: Arc<RwLock<f64>>,
    pub xrp_balance: Arc<RwLock<f64>>,
    pub current_xrp_price: Arc<RwLock<f64>>,
    pub logs: Arc<RwLock<Vec<String>>>,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub market_regime: Arc<RwLock<String>>,
    pub market_regime_eth: Arc<RwLock<String>>,
    pub market_regime_bnb: Arc<RwLock<String>>,
    pub market_regime_sol: Arc<RwLock<String>>,
    pub market_regime_xrp: Arc<RwLock<String>>,
    pub whale_detected: Arc<RwLock<bool>>,
    pub volatility: Arc<RwLock<f64>>,
    pub high_water_mark: Arc<RwLock<f64>>,
    
    // Process indicators
    pub last_ws_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_conclude_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_validate_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_executor_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_corrector_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    
    // System Metrics
    pub sys: Arc<RwLock<sysinfo::System>>,
    
    // Trading streak tracking
    pub ema_death_cross_streak: Arc<RwLock<u8>>,
}

#[derive(Serialize)]
pub struct BotStatus {
    pub simulated_balance: f64,
    pub btc_balance: f64,
    pub current_btc_price: f64,
    pub eth_balance: f64,
    pub current_eth_price: f64,
    pub bnb_balance: f64,
    pub current_bnb_price: f64,
    pub sol_balance: f64,
    pub current_sol_price: f64,
    pub xrp_balance: f64,
    pub current_xrp_price: f64,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub uptime_seconds: i64,
    pub market_regime: String,
    pub market_regime_eth: String,
    pub market_regime_bnb: String,
    pub market_regime_sol: String,
    pub market_regime_xrp: String,
    pub whale_detected: bool,
    pub market_volatility: f64,
    pub winrate: f64,
    pub sys_cpu_pct: f32,
    pub sys_mem_mb: f64,
    
    // Status alur pipeline (LEDs)
    pub ws_active: bool,
    pub conclude_active: bool,
    pub validate_active: bool,
    pub executor_active: bool,
    pub corrector_active: bool,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct TradeHistory {
    pub id: i32,
    pub action: String,
    pub price: f64,
    pub amount: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub status: Option<String>,
    pub notes: Option<String>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct ErrorCorrection {
    pub id: i32,
    pub error_type: String,
    pub reason: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct BalanceHistory {
    pub id: i32,
    pub simulated_balance: f64,
    pub btc_balance: f64,
    pub btc_value: f64,
    pub total_value: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Memulai Bot Trading OKX (Dengan PostgreSQL & Server)...");

    // Load config dari parent dir .env
    dotenvy::from_filename("../.env").ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    // Setup DB
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    // Inisialisasi DB (Buat tabel OKX jika belum ada)
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS okx_trading_history (
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
        "CREATE TABLE IF NOT EXISTS okx_corrections (
            id SERIAL PRIMARY KEY,
            error_type VARCHAR(255) NOT NULL,
            reason TEXT NOT NULL,
            timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        );"
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS okx_klines (
            open_time TIMESTAMPTZ PRIMARY KEY,
            open_price FLOAT NOT NULL,
            high_price FLOAT NOT NULL,
            low_price FLOAT NOT NULL,
            close_price FLOAT NOT NULL,
            volume FLOAT NOT NULL
        );"
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS okx_balance_history (
            id SERIAL PRIMARY KEY,
            simulated_balance FLOAT NOT NULL,
            btc_balance FLOAT NOT NULL,
            btc_value FLOAT NOT NULL,
            total_value FLOAT NOT NULL,
            timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        );"
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS okx_active_positions (
            id SERIAL PRIMARY KEY,
            buy_price DOUBLE PRECISION NOT NULL,
            high_water_mark DOUBLE PRECISION NOT NULL,
            amount DOUBLE PRECISION NOT NULL,
            opened_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        );"
    ).execute(&pool).await?;

    // Pemulihan State Aset Terakhir
    let last_balance: Option<(f64, f64)> = sqlx::query_as(
        "SELECT simulated_balance, btc_balance FROM okx_balance_history ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);

    let (initial_sim, initial_btc) = last_balance.unwrap_or((1000.0, 0.0));

    let initial_hwm: f64 = sqlx::query_scalar("SELECT high_water_mark FROM okx_active_positions ORDER BY id DESC LIMIT 1")
        .fetch_optional(&pool)
        .await
        .unwrap_or(None)
        .unwrap_or(0.0);

    let now = chrono::Utc::now();
    let state = AppState {
        db: pool.clone(),
        simulated_balance: Arc::new(RwLock::new(initial_sim)),
        btc_balance: Arc::new(RwLock::new(initial_btc)),
        current_btc_price: Arc::new(RwLock::new(0.0)),
        eth_balance: Arc::new(RwLock::new(0.0)),
        current_eth_price: Arc::new(RwLock::new(0.0)),
        bnb_balance: Arc::new(RwLock::new(0.0)),
        current_bnb_price: Arc::new(RwLock::new(0.0)),
        sol_balance: Arc::new(RwLock::new(0.0)),
        current_sol_price: Arc::new(RwLock::new(0.0)),
        xrp_balance: Arc::new(RwLock::new(0.0)),
        current_xrp_price: Arc::new(RwLock::new(0.0)),
        logs: Arc::new(RwLock::new(Vec::new())),
        start_time: now,
        market_regime: Arc::new(RwLock::new("SIDEWAYS".to_string())),
        market_regime_eth: Arc::new(RwLock::new("SIDEWAYS".to_string())),
        market_regime_bnb: Arc::new(RwLock::new("SIDEWAYS".to_string())),
        market_regime_sol: Arc::new(RwLock::new("SIDEWAYS".to_string())),
        market_regime_xrp: Arc::new(RwLock::new("SIDEWAYS".to_string())),
        whale_detected: Arc::new(RwLock::new(false)),
        volatility: Arc::new(RwLock::new(0.0)),
        high_water_mark: Arc::new(RwLock::new(initial_hwm)),
        
        last_ws_activity: Arc::new(RwLock::new(now)),
        last_conclude_activity: Arc::new(RwLock::new(now)),
        last_validate_activity: Arc::new(RwLock::new(now)),
        last_executor_activity: Arc::new(RwLock::new(now)),
        last_corrector_activity: Arc::new(RwLock::new(now)),
        sys: Arc::new(RwLock::new(sysinfo::System::new_all())),
        ema_death_cross_streak: Arc::new(RwLock::new(0)),
    };

    // 1. Sync data KLine awal dari OKX REST API
    println!("Melakukan sinkronisasi data kline awal dari OKX...");
    if let Err(e) = get::sync_klines(&state, 100).await {
        eprintln!("Warning: Gagal sync data kline awal: {}", e);
    }

    // Inisialisasi balance history awal jika tabel kosong
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM okx_balance_history")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
        
    if count == 0 {
        let btc_bal = *state.btc_balance.read().await;
        let _ = sqlx::query(
            "INSERT INTO okx_balance_history (simulated_balance, btc_balance, btc_value, total_value) VALUES ($1, $2, $3, $4)"
        )
        .bind(initial_sim)
        .bind(btc_bal)
        .bind(0.0)
        .bind(initial_sim)
        .execute(&pool)
        .await;
    }

    // 2. Spawn WebSocket Price Listener di background (Real-time OKX)
    let listener_state = state.clone();
    tokio::spawn(get::start_price_listener(listener_state));

    // 3. Jalankan Background Loop Trader (Eksekusi / Analisa tiap menit)
    let worker_state = state.clone();
    tokio::spawn(async move {
        // Beri waktu 3 detik agar websocket mendapatkan harga awal
        sleep(Duration::from_secs(3)).await;

        loop {
            // Sinkronkan 5 KLine terbaru
            if let Err(e) = get::sync_klines(&worker_state, 5).await {
                eprintln!("Gagal sync 5 kline terbaru dari OKX: {}", e);
            }

            let mut price = { *worker_state.current_btc_price.read().await };
            
            // Fallback REST API jika WebSocket tidak aktif (>30 detik)
            let last_ws = *worker_state.last_ws_activity.read().await;
            let ws_stale = (chrono::Utc::now() - last_ws).num_seconds() > 30;

            if price <= 0.0 || ws_stale {
                if let Ok(rest_p) = get::get_rest_price().await {
                    let mut price_lock = worker_state.current_btc_price.write().await;
                    *price_lock = rest_p;
                    price = rest_p;
                    if ws_stale {
                        add_log(&worker_state, &format!("[FALLBACK REST] Aliran WebSocket stale (>30 dtk). Harga di-sync via REST API OKX: ${:.2}", price)).await;
                    }
                }
            }
            
            if price > 0.0 {
                add_log(&worker_state, &format!("Siklus Menit Baru. Harga BTC Terakhir (OKX): ${:.2}", price)).await;
                
                // Conclude
                *worker_state.last_conclude_activity.write().await = chrono::Utc::now();
                let decision = conclude::analyze_market(price, &worker_state).await;
                add_log(&worker_state, &format!("Keputusan analis: {:?}", decision)).await;

                // Validate
                *worker_state.last_validate_activity.write().await = chrono::Utc::now();
                let is_valid = validate::validate_decision(&decision, price, &worker_state).await;

                // Executor
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

                // Catat balance history berkala ke DB
                let sim_bal = *worker_state.simulated_balance.read().await;
                let btc_bal = *worker_state.btc_balance.read().await;
                let btc_val = btc_bal * price;
                let total_val = sim_bal + btc_val;
                
                let _ = sqlx::query(
                    "INSERT INTO okx_balance_history (simulated_balance, btc_balance, btc_value, total_value) VALUES ($1, $2, $3, $4)"
                )
                .bind(sim_bal)
                .bind(btc_bal)
                .bind(btc_val)
                .bind(total_val)
                .execute(&worker_state.db)
                .await;
            } else {
                add_log(&worker_state, "Menunggu harga awal dari OKX WebSocket...").await;
            }

            sleep(Duration::from_secs(60)).await;
        }
    });

    // 4. Setup Axum HTTP Router
    let app = Router::new()
        .route("/", get(serve_dashboard))
        .route("/dashboard_okx.html", get(serve_dashboard))
        .route("/includes/header.html", get(serve_header))
        .route("/includes/footer.html", get(serve_footer))
        .route("/js/dashboard_okx.js", get(serve_js))
        .route("/api/status", get(get_status))
        .route("/api/history", get(get_history))
        .route("/api/corrections", get(get_corrections))
        .route("/api/balance_history", get(get_balance_history))
        .route("/api/journal", get(get_journal))
        .with_state(state.clone());

    // Bind server ke port 8091 (Sesuai proxy Apache /okx/)
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8091").await?;
    println!("Web Server OKX aktif di http://127.0.0.1:8091");
    axum::serve(listener, app).await?;

    Ok(())
}

// Handlers untuk HTTP Server
async fn serve_dashboard() -> impl IntoResponse {
    let mut html_content = std::fs::read_to_string("../dashboard_okx.html")
        .or_else(|_| std::fs::read_to_string("dashboard_okx.html"))
        .unwrap_or_else(|_| "<h1>Dashboard HTML file not found</h1>".to_string());
    
    let mut header = std::fs::read_to_string("../includes/header.html")
        .or_else(|_| std::fs::read_to_string("includes/header.html"))
        .unwrap_or_default();
        
    header = header.replace("BitTrade Engine", "BitTrade Bot OKX");
    header = header.replace("BitTrade Menu", "BitTrade Menu OKX");
    
    let footer = std::fs::read_to_string("../includes/footer.html")
        .or_else(|_| std::fs::read_to_string("includes/footer.html"))
        .unwrap_or_default();
        
    html_content = html_content.replace("<!-- INCLUDE HEADER -->", &header);
    html_content = html_content.replace("<!-- INCLUDE FOOTER -->", &footer);
    
    axum::response::Html(html_content)
}

async fn serve_header() -> impl IntoResponse {
    let mut header = std::fs::read_to_string("../includes/header.html")
        .or_else(|_| std::fs::read_to_string("includes/header.html"))
        .unwrap_or_default();
    header = header.replace("BitTrade Engine", "BitTrade Bot OKX");
    axum::response::Html(header)
}

async fn serve_footer() -> impl IntoResponse {
    let footer = std::fs::read_to_string("../includes/footer.html")
        .or_else(|_| std::fs::read_to_string("includes/footer.html"))
        .unwrap_or_default();
    axum::response::Html(footer)
}

async fn serve_js() -> impl IntoResponse {
    let js = std::fs::read_to_string("../js/dashboard_okx.js")
        .or_else(|_| std::fs::read_to_string("js/dashboard_okx.js"))
        .unwrap_or_else(|_| "".to_string());
    
    axum::response::Response::builder()
        .header("content-type", "application/javascript")
        .body(js)
        .unwrap()
}

async fn get_status(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let sim_bal = *state.simulated_balance.read().await;
    let btc_bal = *state.btc_balance.read().await;
    let btc_price = *state.current_btc_price.read().await;
    
    let eth_bal = *state.eth_balance.read().await;
    let eth_price = *state.current_eth_price.read().await;
    
    let bnb_bal = *state.bnb_balance.read().await;
    let bnb_price = *state.current_bnb_price.read().await;
    
    let sol_bal = *state.sol_balance.read().await;
    let sol_price = *state.current_sol_price.read().await;
    
    let xrp_bal = *state.xrp_balance.read().await;
    let xrp_price = *state.current_xrp_price.read().await;
    
    let market_regime = state.market_regime.read().await.clone();
    let market_regime_eth = state.market_regime_eth.read().await.clone();
    let market_regime_bnb = state.market_regime_bnb.read().await.clone();
    let market_regime_sol = state.market_regime_sol.read().await.clone();
    let market_regime_xrp = state.market_regime_xrp.read().await.clone();
    
    let whale = *state.whale_detected.read().await;
    let vol = *state.volatility.read().await;
    
    // CPU & Memory usage
    let (cpu, mem) = {
        let mut sys = state.sys.write().await;
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        let cpu = sys.global_cpu_usage() as f32;
        let mem = (sys.used_memory() as f64) / (1024.0 * 1024.0);
        (cpu, mem)
    };

    // Hitung Winrate
    let total_sells: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM okx_trading_history WHERE action = 'SELL' AND status = 'SUCCESS'"
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let win_sells: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM okx_trading_history WHERE action = 'SELL' AND status = 'SUCCESS' AND (notes LIKE '%P&L: $+%' OR notes LIKE '%P&L: +%')"
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let winrate = if total_sells > 0 {
        (win_sells as f64 / total_sells as f64) * 100.0
    } else {
        100.0
    };

    // Hitung Uptime
    let uptime = (Utc::now() - state.start_time).num_seconds();

    // Pipeline Activity status
    let now = Utc::now();
    let ws_active = (now - *state.last_ws_activity.read().await).num_seconds() < 10;
    let conclude_active = (now - *state.last_conclude_activity.read().await).num_seconds() < 70;
    let validate_active = (now - *state.last_validate_activity.read().await).num_seconds() < 70;
    let executor_active = (now - *state.last_executor_activity.read().await).num_seconds() < 70;
    let corrector_active = (now - *state.last_corrector_activity.read().await).num_seconds() < 70;

    Json(BotStatus {
        simulated_balance: sim_bal,
        btc_balance: btc_bal,
        current_btc_price: btc_price,
        eth_balance: eth_bal,
        current_eth_price: eth_price,
        bnb_balance: bnb_bal,
        current_bnb_price: bnb_price,
        sol_balance: sol_bal,
        current_sol_price: sol_price,
        xrp_balance: xrp_bal,
        current_xrp_price: xrp_price,
        start_time: state.start_time,
        uptime_seconds: uptime,
        market_regime,
        market_regime_eth,
        market_regime_bnb,
        market_regime_sol,
        market_regime_xrp,
        whale_detected: whale,
        market_volatility: vol,
        winrate,
        sys_cpu_pct: cpu,
        sys_mem_mb: mem,
        ws_active,
        conclude_active,
        validate_active,
        executor_active,
        corrector_active,
    })
}

async fn get_history(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let history: Vec<TradeHistory> = sqlx::query_as(
        "SELECT id, action, price, amount, timestamp, status, notes FROM okx_trading_history ORDER BY id DESC LIMIT 50"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    
    Json(history)
}

async fn get_corrections(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let corrections: Vec<ErrorCorrection> = sqlx::query_as(
        "SELECT id, error_type, reason, timestamp FROM okx_corrections ORDER BY id DESC LIMIT 50"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    
    Json(corrections)
}

async fn get_balance_history(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let history: Vec<BalanceHistory> = sqlx::query_as(
        "SELECT id, simulated_balance, btc_balance, btc_value, total_value, timestamp FROM ( \
         SELECT id, simulated_balance, btc_balance, btc_value, total_value, timestamp \
         FROM okx_balance_history ORDER BY id DESC LIMIT 5000 \
         ) sub ORDER BY id ASC"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    
    Json(history)
}

async fn get_journal(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let logs = state.logs.read().await;
    logs.join("\n")
}

