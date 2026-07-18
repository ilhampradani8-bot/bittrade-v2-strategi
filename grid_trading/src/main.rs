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
use std::collections::HashMap;

pub mod corrector;
pub mod executor;
pub mod grid_logic;
pub mod validate;
pub mod get;
pub mod risk;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub simulated_balance: Arc<RwLock<f64>>,
    
    // HashMaps for multi-asset
    pub asset_balances: Arc<RwLock<HashMap<String, f64>>>,
    pub prices: Arc<RwLock<HashMap<String, f64>>>,
    pub market_regimes: Arc<RwLock<HashMap<String, String>>>,
    pub volatilities: Arc<RwLock<HashMap<String, f64>>>,
    pub high_water_marks: Arc<RwLock<HashMap<String, f64>>>,
    pub obis: Arc<RwLock<HashMap<String, f64>>>,
    pub total_realized_pnls: Arc<RwLock<HashMap<String, f64>>>,
    
    pub logs: Arc<RwLock<Vec<String>>>,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub whale_detected: Arc<RwLock<bool>>,
    
    // Process indicators
    pub last_ws_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_conclude_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_validate_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_executor_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_corrector_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    
    pub sys: Arc<RwLock<sysinfo::System>>,
}

#[derive(Serialize)]
pub struct BotStatus {
    pub simulated_balance: f64,
    pub asset_balances: HashMap<String, f64>,
    pub prices: HashMap<String, f64>,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub uptime_seconds: i64,
    pub market_regimes: HashMap<String, String>,
    pub whale_detected: bool,
    pub volatilities: HashMap<String, f64>,
    pub winrate: f64,
    pub sys_cpu_pct: f32,
    pub sys_mem_mb: f64,
    
    pub ws_active: bool,
    pub conclude_active: bool,
    pub validate_active: bool,
    pub executor_active: bool,
    pub corrector_active: bool,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct TradeHistory {
    pub id: i32,
    pub symbol: String,
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
    pub symbol: String,
    pub error_type: String,
    pub reason: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct BalanceHistory {
    pub id: i32,
    pub simulated_balance: f64,
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

pub const SYMBOLS: [&str; 5] = ["BTCUSDT", "SOLUSDT", "DOGEUSDT", "AVAXUSDT", "LINKUSDT"];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Memulai Multi-Asset Grid Bot...");
    dotenvy::from_filename("../.env").ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new().max_connections(10).connect(&db_url).await?;

    // Create Tables with symbol column
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS grid_trading_history (
            id SERIAL PRIMARY KEY,
            symbol VARCHAR(20) NOT NULL,
            action VARCHAR(50) NOT NULL,
            price FLOAT NOT NULL,
            amount FLOAT NOT NULL,
            timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
            status VARCHAR(50),
            notes TEXT
        );"
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS grid_corrections (
            id SERIAL PRIMARY KEY,
            symbol VARCHAR(20) NOT NULL,
            error_type VARCHAR(255) NOT NULL,
            reason TEXT NOT NULL,
            timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        );"
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS grid_klines (
            symbol VARCHAR(20) NOT NULL,
            open_time TIMESTAMPTZ NOT NULL,
            open_price FLOAT NOT NULL,
            high_price FLOAT NOT NULL,
            low_price FLOAT NOT NULL,
            close_price FLOAT NOT NULL,
            volume FLOAT NOT NULL,
            PRIMARY KEY (symbol, open_time)
        );"
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS grid_balance_history (
            id SERIAL PRIMARY KEY,
            simulated_balance FLOAT NOT NULL,
            total_value FLOAT NOT NULL,
            timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        );"
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS grid_active_positions (
            id SERIAL PRIMARY KEY,
            symbol VARCHAR(20) NOT NULL,
            buy_price DOUBLE PRECISION NOT NULL,
            high_water_mark DOUBLE PRECISION NOT NULL,
            amount DOUBLE PRECISION NOT NULL,
            opened_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        );"
    ).execute(&pool).await?;

    let last_balance: Option<f64> = sqlx::query_scalar(
        "SELECT simulated_balance FROM grid_balance_history ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);

    let initial_capital: f64 = env::var("GRID_INITIAL_CAPITAL").unwrap_or_else(|_| "200.0".to_string()).parse().unwrap_or(200.0);
    let initial_sim = last_balance.unwrap_or(initial_capital);

    let mut asset_balances = HashMap::new();
    let mut prices = HashMap::new();
    let mut market_regimes = HashMap::new();
    let mut volatilities = HashMap::new();
    let mut high_water_marks = HashMap::new();
    let mut obis = HashMap::new();
    let mut total_realized_pnls = HashMap::new();

    for sym in SYMBOLS {
        asset_balances.insert(sym.to_string(), 0.0);
        prices.insert(sym.to_string(), 0.0);
        market_regimes.insert(sym.to_string(), "GRID".to_string());
        volatilities.insert(sym.to_string(), 0.0);
        
        let hwm: f64 = sqlx::query_scalar("SELECT MAX(high_water_mark) FROM grid_active_positions WHERE symbol = $1")
            .bind(sym)
            .fetch_optional(&pool).await.unwrap_or(None).unwrap_or(0.0);
            
        high_water_marks.insert(sym.to_string(), hwm);
        obis.insert(sym.to_string(), 0.5);
        total_realized_pnls.insert(sym.to_string(), 0.0);
    }

    let now = chrono::Utc::now();
    let state = AppState {
        db: pool.clone(),
        simulated_balance: Arc::new(RwLock::new(initial_sim)),
        asset_balances: Arc::new(RwLock::new(asset_balances)),
        prices: Arc::new(RwLock::new(prices)),
        market_regimes: Arc::new(RwLock::new(market_regimes)),
        volatilities: Arc::new(RwLock::new(volatilities)),
        high_water_marks: Arc::new(RwLock::new(high_water_marks)),
        obis: Arc::new(RwLock::new(obis)),
        total_realized_pnls: Arc::new(RwLock::new(total_realized_pnls)),
        logs: Arc::new(RwLock::new(Vec::new())),
        start_time: now,
        whale_detected: Arc::new(RwLock::new(false)),
        last_ws_activity: Arc::new(RwLock::new(now)),
        last_conclude_activity: Arc::new(RwLock::new(now)),
        last_validate_activity: Arc::new(RwLock::new(now)),
        last_executor_activity: Arc::new(RwLock::new(now)),
        last_corrector_activity: Arc::new(RwLock::new(now)),
        sys: Arc::new(RwLock::new(sysinfo::System::new_all())),
    };

    println!("Melakukan sinkronisasi data kline awal dari Binance...");
    for sym in SYMBOLS {
        if let Err(e) = get::sync_klines(&state, 200, sym).await {
            eprintln!("Warning: Gagal sync data kline awal {}: {}", sym, e);
        }
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM grid_balance_history").fetch_one(&pool).await.unwrap_or(0);
    if count == 0 {
        let _ = sqlx::query("INSERT INTO grid_balance_history (simulated_balance, total_value) VALUES ($1, $2)")
        .bind(initial_sim).bind(initial_sim).execute(&pool).await;
    }

    let listener_state = state.clone();
    tokio::spawn(get::start_price_listener(listener_state));

    let worker_state = state.clone();
    tokio::spawn(async move {
        sleep(Duration::from_secs(3)).await;
        loop {
            let last_ws = *worker_state.last_ws_activity.read().await;
            let ws_stale = (chrono::Utc::now() - last_ws).num_seconds() > 30;

            let mut total_assets_value = 0.0;
            
            for sym in SYMBOLS {
                if let Err(e) = get::sync_klines(&worker_state, 3, sym).await {
                    eprintln!("Gagal sync 3 kline {}: {}", sym, e);
                }

                let price = {
                    let p = worker_state.prices.read().await;
                    *p.get(sym).unwrap_or(&0.0)
                };

                if price > 0.0 {
                    *worker_state.last_conclude_activity.write().await = chrono::Utc::now();
                    let decision = grid_logic::analyze_market(price, &worker_state, sym).await;
                    
                    *worker_state.last_validate_activity.write().await = chrono::Utc::now();
                    let is_valid = validate::validate_decision(&decision, price, &worker_state, sym).await;

                    if is_valid {
                        *worker_state.last_executor_activity.write().await = chrono::Utc::now();
                        add_log(&worker_state, &format!("[{}] Mengeksekusi transaksi...", sym)).await;
                        match executor::execute_trade(&decision, price, &worker_state, sym).await {
                            Ok(_) => {
                                let bal = worker_state.simulated_balance.read().await;
                                let asset_bal = *worker_state.asset_balances.read().await.get(sym).unwrap_or(&0.0);
                                add_log(&worker_state, &format!("[{}] Eksekusi sukses. Saldo USDT: ${:.2}, Koin: {:.4}", sym, *bal, asset_bal)).await;
                            },
                            Err(e) => {
                                let err_msg = e.to_string();
                                add_log(&worker_state, &format!("[{}] Eksekusi Gagal: {}", sym, err_msg)).await;
                                corrector::log_error(&worker_state, sym, "EXECUTION_ERROR", &err_msg).await;
                            }
                        }
                    }
                    
                    let asset_bal = *worker_state.asset_balances.read().await.get(sym).unwrap_or(&0.0);
                    total_assets_value += asset_bal * price;
                }
            }
            
            let sim_bal = *worker_state.simulated_balance.read().await;
            let total_val = sim_bal + total_assets_value;
            let _ = sqlx::query("INSERT INTO grid_balance_history (simulated_balance, total_value) VALUES ($1, $2)")
            .bind(sim_bal).bind(total_val).execute(&worker_state.db).await;

            sleep(Duration::from_secs(30)).await;
        }
    });

    let app = Router::new()
        .route("/", get(serve_dashboard))
        .route("/dashboard_grid.html", get(serve_dashboard))
        .route("/includes/header.html", get(serve_header))
        .route("/includes/footer.html", get(serve_footer))
        .route("/js/dashboard_grid.js", get(serve_js))
        .route("/api/status", get(get_status))
        .route("/api/history", get(get_history))
        .route("/api/corrections", get(get_corrections))
        .route("/api/balance_history", get(get_balance_history))
        .route("/api/journal", get(get_journal))
        .route("/api/grid_positions", get(get_grid_positions))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8091").await?;
    println!("Web Server Grid aktif di http://127.0.0.1:8091");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn serve_dashboard() -> impl IntoResponse {
    let mut html_content = std::fs::read_to_string("../dashboard_grid.html").unwrap_or_else(|_| "<h1>File not found</h1>".to_string());
    let mut header = std::fs::read_to_string("../includes/header.html").unwrap_or_default();
    header = header.replace("BitTrade Engine", "BitTrade Bot Grid");
    header = header.replace("BitTrade Menu", "BitTrade Menu Grid");
    let footer = std::fs::read_to_string("../includes/footer.html").unwrap_or_default();
    html_content = html_content.replace("<!-- INCLUDE HEADER -->", &header).replace("<!-- INCLUDE FOOTER -->", &footer);
    axum::response::Html(html_content)
}

async fn serve_header() -> impl IntoResponse {
    axum::response::Html(std::fs::read_to_string("../includes/header.html").unwrap_or_default().replace("BitTrade Engine", "BitTrade Bot Grid"))
}

async fn serve_footer() -> impl IntoResponse {
    axum::response::Html(std::fs::read_to_string("../includes/footer.html").unwrap_or_default())
}

async fn serve_js() -> impl IntoResponse {
    axum::response::Response::builder().header("content-type", "application/javascript").body(std::fs::read_to_string("../js/dashboard_grid.js").unwrap_or_default()).unwrap()
}

async fn get_status(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let sim_bal = *state.simulated_balance.read().await;
    let whale = *state.whale_detected.read().await;
    
    let (cpu, mem) = {
        let mut sys = state.sys.write().await;
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        (sys.global_cpu_usage() as f32, (sys.used_memory() as f64) / (1024.0 * 1024.0))
    };

    let total_sells: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM grid_trading_history WHERE action = 'SELL' AND status = 'SUCCESS'").fetch_one(&state.db).await.unwrap_or(0);
    let win_sells: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM grid_trading_history WHERE action = 'SELL' AND status = 'SUCCESS' AND (notes LIKE '%P&L: $+%' OR notes LIKE '%P&L: +%')").fetch_one(&state.db).await.unwrap_or(0);
    let winrate = if total_sells > 0 { (win_sells as f64 / total_sells as f64) * 100.0 } else { 100.0 };

    let uptime = (Utc::now() - state.start_time).num_seconds();
    let now = Utc::now();
    let ws_active = (now - *state.last_ws_activity.read().await).num_seconds() < 10;
    
    Json(BotStatus {
        simulated_balance: sim_bal,
        asset_balances: state.asset_balances.read().await.clone(),
        prices: state.prices.read().await.clone(),
        start_time: state.start_time,
        uptime_seconds: uptime,
        market_regimes: state.market_regimes.read().await.clone(),
        whale_detected: whale,
        volatilities: state.volatilities.read().await.clone(),
        winrate,
        sys_cpu_pct: cpu,
        sys_mem_mb: mem,
        ws_active,
        conclude_active: (now - *state.last_conclude_activity.read().await).num_seconds() < 70,
        validate_active: (now - *state.last_validate_activity.read().await).num_seconds() < 70,
        executor_active: (now - *state.last_executor_activity.read().await).num_seconds() < 70,
        corrector_active: (now - *state.last_corrector_activity.read().await).num_seconds() < 70,
    })
}

async fn get_history(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let history: Vec<TradeHistory> = sqlx::query_as("SELECT id, symbol, action, price, amount, timestamp, status, notes FROM grid_trading_history ORDER BY id DESC LIMIT 50").fetch_all(&state.db).await.unwrap_or_default();
    Json(history)
}

async fn get_corrections(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let corrections: Vec<ErrorCorrection> = sqlx::query_as("SELECT id, symbol, error_type, reason, timestamp FROM grid_corrections ORDER BY id DESC LIMIT 50").fetch_all(&state.db).await.unwrap_or_default();
    Json(corrections)
}

async fn get_balance_history(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let history: Vec<BalanceHistory> = sqlx::query_as("SELECT id, simulated_balance, total_value, timestamp FROM (SELECT id, simulated_balance, total_value, timestamp FROM grid_balance_history ORDER BY id DESC LIMIT 5000) sub ORDER BY id ASC").fetch_all(&state.db).await.unwrap_or_default();
    Json(history)
}

async fn get_journal(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let logs = state.logs.read().await;
    let mut journal_str = String::new();
    for log in logs.iter() {
        journal_str.push_str(log);
        journal_str.push('\n');
    }
    journal_str
}

#[derive(sqlx::FromRow, Serialize)]
struct GridPosition {
    pub id: i32,
    pub symbol: String,
    pub buy_price: f64,
    pub high_water_mark: f64,
    pub amount: f64,
    pub opened_at: chrono::DateTime<chrono::Utc>,
}

async fn get_grid_positions(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let positions: Vec<GridPosition> = sqlx::query_as("SELECT id, symbol, buy_price, high_water_mark, amount, opened_at FROM grid_active_positions ORDER BY buy_price ASC").fetch_all(&state.db).await.unwrap_or_default();
    Json(positions)
}
