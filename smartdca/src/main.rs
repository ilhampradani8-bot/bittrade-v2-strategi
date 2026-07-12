use sqlx::postgres::PgPoolOptions;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;
use chrono::Utc;
use axum::{
    routing::{get, post},
    response::{Html, Json, IntoResponse},
    extract::Query,
    Extension, Router,
};
use tower_http::cors::CorsLayer;

mod conclude;
mod executor;
mod validate;
mod get;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub simulated_balance: Arc<RwLock<f64>>,
    pub btc_balance: Arc<RwLock<f64>>,
    pub current_price: Arc<RwLock<f64>>,
    pub current_cycle_id: Arc<RwLock<i32>>,
    pub layers_filled: Arc<RwLock<u8>>,
    pub cycle_high_water_mark: Arc<RwLock<f64>>,
    pub logs: Arc<RwLock<Vec<String>>>,
    pub start_time: chrono::DateTime<chrono::Utc>,
    
    // Process indicators
    pub last_ws_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_conclude_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_validate_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    pub last_executor_activity: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
    
    // System Metrics
    pub sys: Arc<RwLock<sysinfo::System>>,
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct DcaTradeHistory {
    pub id: i32,
    pub cycle_id: i32,
    pub action: String,
    pub layer: Option<i32>,
    pub price: f64,
    pub amount: f64,
    pub usdt_spent: Option<f64>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub status: Option<String>,
    pub notes: Option<String>,
    pub net_pnl: Option<f64>,
    pub pnl_pct: Option<f64>,
}

#[derive(serde::Serialize, sqlx::FromRow, Clone)]
pub struct DcaBalanceHistory {
    pub id: i32,
    pub simulated_balance: f64,
    pub btc_balance: f64,
    pub total_value: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct DcaCycleSummary {
    pub id: i32,
    pub cycle_id: i32,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub layers_used: Option<i32>,
    pub avg_entry_price: Option<f64>,
    pub exit_price: Option<f64>,
    pub total_spent: Option<f64>,
    pub net_pnl: Option<f64>,
    pub pnl_pct: Option<f64>,
    pub exit_reason: Option<String>,
    pub status: Option<String>,
}

#[derive(serde::Serialize)]
pub struct StatusResponse {
    pub current_price: f64,
    pub simulated_balance: f64,
    pub btc_balance: f64,
    pub total_equity: f64,
    pub layers_filled: u8,
    pub current_cycle_id: i32,
    pub avg_entry_price: f64,
    pub current_pnl_pct: f64,
    pub cycle_high_water_mark: f64,
    pub ws_active: bool,
    pub conclude_active: bool,
    pub validate_active: bool,
    pub executor_active: bool,
    pub sys_cpu_pct: f64,
    pub sys_mem_mb: f64,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub winrate: f64,
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
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenvy::dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL harus diset di .env");

    println!("Menghubungkan ke database PostgreSQL...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // ==========================================
    // MIGRATIONS (Database Schema dca_ Prefix)
    // ==========================================
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS btc_klines (
            open_time TIMESTAMPTZ PRIMARY KEY,
            open_price DOUBLE PRECISION NOT NULL,
            high_price DOUBLE PRECISION NOT NULL,
            low_price DOUBLE PRECISION NOT NULL,
            close_price DOUBLE PRECISION NOT NULL,
            volume DOUBLE PRECISION NOT NULL
        );"
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS dca_trading_history (
            id              SERIAL PRIMARY KEY,
            cycle_id        INTEGER NOT NULL,
            action          VARCHAR(10) NOT NULL,
            layer           INTEGER,
            price           DOUBLE PRECISION NOT NULL,
            amount          DOUBLE PRECISION NOT NULL,
            usdt_spent      DOUBLE PRECISION,
            timestamp       TIMESTAMPTZ DEFAULT NOW(),
            status          VARCHAR(20) DEFAULT 'SUCCESS',
            notes           TEXT
        );"
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS dca_active_positions (
            id              SERIAL PRIMARY KEY,
            cycle_id        INTEGER NOT NULL,
            layer           INTEGER NOT NULL,
            price           DOUBLE PRECISION NOT NULL,
            amount          DOUBLE PRECISION NOT NULL,
            usdt_spent      DOUBLE PRECISION NOT NULL,
            timestamp       TIMESTAMPTZ DEFAULT NOW()
        );"
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS dca_balance_history (
            id                  SERIAL PRIMARY KEY,
            simulated_balance   DOUBLE PRECISION NOT NULL,
            btc_balance         DOUBLE PRECISION NOT NULL,
            total_value         DOUBLE PRECISION NOT NULL,
            timestamp           TIMESTAMPTZ DEFAULT NOW()
        );"
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS dca_cycle_summary (
            id              SERIAL PRIMARY KEY,
            cycle_id        INTEGER UNIQUE NOT NULL,
            start_time      TIMESTAMPTZ,
            end_time        TIMESTAMPTZ,
            layers_used     INTEGER,
            avg_entry_price DOUBLE PRECISION,
            exit_price      DOUBLE PRECISION,
            total_spent     DOUBLE PRECISION,
            net_pnl         DOUBLE PRECISION,
            pnl_pct         DOUBLE PRECISION,
            exit_reason     TEXT,
            status          VARCHAR(20)
        );"
    )
    .execute(&pool)
    .await?;

    // ==========================================
    // STATE RECONSTRUCTION & CRASH RECOVERY
    // ==========================================
    println!("Menganalisa data crash recovery...");
    let mut initial_balance = 700.0;
    let mut initial_btc = 0.0;

    let last_balance: Option<(f64, f64)> = sqlx::query_as(
        "SELECT simulated_balance, btc_balance FROM dca_balance_history ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);

    if let Some((sim_bal, btc_bal)) = last_balance {
        initial_balance = sim_bal;
        initial_btc = btc_bal;
        println!("[CRASH RECOVERY] Saldo terpulihkan: ${:.2} USDT, {:.6} BTC", sim_bal, btc_bal);
    }

    let active_cycle: Option<i32> = sqlx::query_scalar("SELECT cycle_id FROM dca_active_positions LIMIT 1")
        .fetch_optional(&pool)
        .await
        .unwrap_or(None);

    let initial_cycle_id = if let Some(cid) = active_cycle {
        cid
    } else {
        let max_summary: Option<i32> = sqlx::query_scalar("SELECT MAX(cycle_id) FROM dca_cycle_summary")
            .fetch_optional(&pool)
            .await
            .unwrap_or(None);
        max_summary.unwrap_or(0) + 1
    };

    let initial_layers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dca_active_positions")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    let initial_layers = initial_layers as u8;

    let start_time_opt: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT MIN(timestamp) FROM dca_active_positions"
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(None);

    let initial_hwm = if let Some(st) = start_time_opt {
        let max_price: Option<f64> = sqlx::query_scalar(
            "SELECT MAX(high_price) FROM btc_klines WHERE open_time >= $1"
        )
        .bind(st)
        .fetch_one(&pool)
        .await
        .unwrap_or(None);
        max_price.unwrap_or(0.0)
    } else {
        0.0
    };

    println!("[CRASH RECOVERY] Siklus Aktif: #{} | Layer terisi: {}/3 | High Water Mark: ${:.2}", initial_cycle_id, initial_layers, initial_hwm);

    let now = chrono::Utc::now();
    let state = AppState {
        db: pool,
        simulated_balance: Arc::new(RwLock::new(initial_balance)),
        btc_balance: Arc::new(RwLock::new(initial_btc)),
        current_price: Arc::new(RwLock::new(0.0)),
        current_cycle_id: Arc::new(RwLock::new(initial_cycle_id)),
        layers_filled: Arc::new(RwLock::new(initial_layers)),
        cycle_high_water_mark: Arc::new(RwLock::new(initial_hwm)),
        logs: Arc::new(RwLock::new(Vec::new())),
        start_time: now,
        last_ws_activity: Arc::new(RwLock::new(now)),
        last_conclude_activity: Arc::new(RwLock::new(now)),
        last_validate_activity: Arc::new(RwLock::new(now)),
        last_executor_activity: Arc::new(RwLock::new(now)),
        sys: Arc::new(RwLock::new(sysinfo::System::new_all())),
    };

    // Insert saldo awal ke balance history jika kosong
    let balance_history_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dca_balance_history")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    if balance_history_count == 0 {
        sqlx::query(
            "INSERT INTO dca_balance_history (simulated_balance, btc_balance, total_value) VALUES ($1, $2, $3)"
        )
        .bind(initial_balance)
        .bind(initial_btc)
        .bind(initial_balance)
        .execute(&state.db)
        .await
        .ok();
    }

    // 1. Spawn WebSocket Price Listener di background
    let listener_state = state.clone();
    tokio::spawn(get::start_price_listener(listener_state));

    // 2. Jalankan Background Loop Trader (Eksekusi / Analisa tiap menit)
    let worker_state = state.clone();
    tokio::spawn(async move {
        // Tunggu 3 detik agar websocket mendapatkan harga awal
        sleep(Duration::from_secs(3)).await;

        loop {
            // Sinkronkan 5 kline terbaru dari Binance
            if let Err(e) = get::sync_klines(&worker_state.db, 5).await {
                eprintln!("Gagal sync 5 kline terbaru: {}", e);
            }

            let mut price = *worker_state.current_price.read().await;

            // Fallback REST API jika WebSocket tidak aktif atau data stale (>30 detik)
            let last_ws = *worker_state.last_ws_activity.read().await;
            let ws_stale = (chrono::Utc::now() - last_ws).num_seconds() > 30;

            if price <= 0.0 || ws_stale {
                if let Ok(rest_p) = get::get_rest_price().await {
                    let mut price_lock = worker_state.current_price.write().await;
                    *price_lock = rest_p;
                    price = rest_p;
                    if ws_stale {
                        add_log(&worker_state, &format!("[FALLBACK REST] Aliran WebSocket stale (>30 dtk). Harga di-sync via REST: ${:.2}", price)).await;
                    }
                }
            }

            if price > 0.0 {
                add_log(&worker_state, &format!("Siklus Menit Baru. Harga BTC Terakhir: ${:.2}", price)).await;

                // Conclude
                *worker_state.last_conclude_activity.write().await = chrono::Utc::now();
                let decision = conclude::analyze_market(price, &worker_state).await;
                add_log(&worker_state, &format!("Keputusan analis: {:?}", decision)).await;

                // Validate
                *worker_state.last_validate_activity.write().await = chrono::Utc::now();
                let is_valid = validate::validate_decision(&decision, price, &worker_state).await;

                // Executor
                *worker_state.last_executor_activity.write().await = chrono::Utc::now();
                if is_valid {
                    add_log(&worker_state, "Mengeksekusi transaksi...").await;
                    match executor::execute_trade(&decision, price, &worker_state).await {
                        Ok(_) => {
                            let bal = worker_state.simulated_balance.read().await;
                            let btc = worker_state.btc_balance.read().await;
                            add_log(&worker_state, &format!("Eksekusi sukses. Saldo: ${:.2}, BTC: {:.6}", *bal, *btc)).await;
                        }
                        Err(e) => {
                            add_log(&worker_state, &format!("Eksekusi gagal: {}", e)).await;
                        }
                    }
                } else if decision != conclude::Decision::Wait {
                    add_log(&worker_state, "Validasi gagal. Transaksi dibatalkan.").await;
                }

                // Simpan perkembangan modal ke balance history
                let sim_bal = *worker_state.simulated_balance.read().await;
                let btc_bal = *worker_state.btc_balance.read().await;
                let total_val = sim_bal + (btc_bal * price);
                sqlx::query(
                    "INSERT INTO dca_balance_history (simulated_balance, btc_balance, total_value) VALUES ($1, $2, $3)"
                )
                .bind(sim_bal)
                .bind(btc_bal)
                .bind(total_val)
                .execute(&worker_state.db)
                .await
                .ok();
            } else {
                add_log(&worker_state, "Menunggu harga awal dari WebSocket...").await;
            }

            sleep(Duration::from_secs(60)).await;
        }
    });

    // ==========================================
    // AXUM HTTP WEB SERVER
    // ==========================================
    let app = Router::new()
        .route("/", get(serve_dashboard))
        .route("/paper", get(serve_paper_a))
        .route("/paper.html", get(serve_paper_a))
        .route("/paper_a", get(serve_paper_a))
        .route("/paper_b.html", get(serve_paper_b))
        .route("/paper_statarb", get(serve_paper_statarb))
        .route("/paper_statarb.html", get(serve_paper_statarb))
        .route("/js/dashboard.js", get(serve_js))
        .route("/backtest.html", get(serve_backtest))
        .route("/api/backtest/results", get(get_backtest_results))
        .route("/api/backtest/run", post(run_backtest))
        .route("/api/status", get(get_status))
        .route("/api/history", get(get_history))
        .route("/api/cycles", get(get_cycles))
        .route("/api/balance", get(get_balance))
        .route("/api/logs", get(get_logs))
        .route("/api/manual_sell", post(manual_sell))
        .layer(CorsLayer::permissive())
        .layer(Extension(state));

    let port = 8088;
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    println!("Web Dashboard SmartDCA berjalan di http://localhost:{}", port);
    println!("Buka http://<IP_VPS>:{} di browser Anda.", port);

    axum::serve(listener, app).await?;

    Ok(())
}

// Handlers Axum
async fn serve_dashboard() -> impl IntoResponse {
    let mut html_content = std::fs::read_to_string("../smartdca/src/dashboard.html")
        .or_else(|_| std::fs::read_to_string("smartdca/src/dashboard.html"))
        .unwrap_or_else(|_| "<h1>Dashboard HTML file not found</h1>".to_string());
    
    // Inject shared header & footer templates
    let mut header = std::fs::read_to_string("../includes/header.html")
        .or_else(|_| std::fs::read_to_string("includes/header.html"))
        .unwrap_or_default();
    
    // Ubah nama di header menjadi SmartDCA Bot
    header = header.replace("BitTrade Engine", "SmartDCA Bot");
    header = header.replace("BitTrade Menu", "SmartDCA Menu");

    let footer = std::fs::read_to_string("../includes/footer.html")
        .or_else(|_| std::fs::read_to_string("includes/footer.html"))
        .unwrap_or_default();
        
    html_content = html_content.replace("<!-- INCLUDE HEADER -->", &header);
    html_content = html_content.replace("<!-- INCLUDE FOOTER -->", &footer);
    
    Html(html_content)
}

async fn serve_paper_a() -> impl IntoResponse {
    let mut html_content = std::fs::read_to_string("../paper_a")
        .or_else(|_| std::fs::read_to_string("paper_a"))
        .unwrap_or_else(|_| "<h1>Paper A HTML file not found</h1>".to_string());
        
    let mut header = std::fs::read_to_string("../includes/header.html")
        .or_else(|_| std::fs::read_to_string("includes/header.html"))
        .unwrap_or_default();
    header = header.replace("BitTrade Engine", "SmartDCA Bot");
    header = header.replace("BitTrade Menu", "SmartDCA Menu");

    let footer = std::fs::read_to_string("../includes/footer.html")
        .or_else(|_| std::fs::read_to_string("includes/footer.html"))
        .unwrap_or_default();
        
    html_content = html_content.replace("<!-- INCLUDE HEADER -->", &header);
    html_content = html_content.replace("<!-- INCLUDE FOOTER -->", &footer);
    
    Html(html_content)
}

async fn serve_paper_b() -> impl IntoResponse {
    let mut html_content = std::fs::read_to_string("../paper_b.html")
        .or_else(|_| std::fs::read_to_string("paper_b.html"))
        .unwrap_or_else(|_| "<h1>Paper B HTML file not found</h1>".to_string());
        
    let mut header = std::fs::read_to_string("../includes/header.html")
        .or_else(|_| std::fs::read_to_string("includes/header.html"))
        .unwrap_or_default();
    header = header.replace("BitTrade Engine", "SmartDCA Bot");
    header = header.replace("BitTrade Menu", "SmartDCA Menu");

    let footer = std::fs::read_to_string("../includes/footer.html")
        .or_else(|_| std::fs::read_to_string("includes/footer.html"))
        .unwrap_or_default();
        
    html_content = html_content.replace("<!-- INCLUDE HEADER -->", &header);
    html_content = html_content.replace("<!-- INCLUDE FOOTER -->", &footer);
    
    Html(html_content)
}

async fn serve_paper_statarb() -> impl IntoResponse {
    let mut html_content = std::fs::read_to_string("../paper_statarb.html")
        .or_else(|_| std::fs::read_to_string("paper_statarb.html"))
        .unwrap_or_else(|_| "<h1>Paper statARB HTML file not found</h1>".to_string());
        
    let mut header = std::fs::read_to_string("../includes/header.html")
        .or_else(|_| std::fs::read_to_string("includes/header.html"))
        .unwrap_or_default();
    header = header.replace("BitTrade Engine", "SmartDCA Bot");
    header = header.replace("BitTrade Menu", "SmartDCA Menu");

    let footer = std::fs::read_to_string("../includes/footer.html")
        .or_else(|_| std::fs::read_to_string("includes/footer.html"))
        .unwrap_or_default();
        
    html_content = html_content.replace("<!-- INCLUDE HEADER -->", &header);
    html_content = html_content.replace("<!-- INCLUDE FOOTER -->", &footer);
    
    Html(html_content)
}

async fn serve_js() -> impl IntoResponse {
    let js = std::fs::read_to_string("../smartdca/js/dashboard.js")
        .or_else(|_| std::fs::read_to_string("smartdca/js/dashboard.js"))
        .unwrap_or_else(|_| "".to_string());
    
    axum::response::Response::builder()
        .header("content-type", "application/javascript")
        .body(js)
        .unwrap()
}

async fn get_status(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let price = *state.current_price.read().await;
    let sim_bal = *state.simulated_balance.read().await;
    let btc_bal = *state.btc_balance.read().await;
    let layers = *state.layers_filled.read().await;
    let cycle_id = *state.current_cycle_id.read().await;
    let hwm = *state.cycle_high_water_mark.read().await;

    let total_equity = sim_bal + (btc_bal * price);

    // Calculate real-time average entry price & profit pct
    let active_pos: Option<(f64, f64, f64)> = sqlx::query_as(
        "SELECT 
            COALESCE(SUM(price * amount) / NULLIF(SUM(amount), 0), 0.0), 
            COALESCE(SUM(amount), 0.0), 
            COALESCE(SUM(usdt_spent), 0.0)
         FROM dca_active_positions WHERE cycle_id = $1"
    )
    .bind(cycle_id)
    .fetch_one(&state.db)
    .await
    .ok()
    .map(|r: (f64, f64, f64)| r);

    let (avg_entry_price, total_btc, total_spent) = active_pos.unwrap_or((0.0, 0.0, 0.0));
    let current_pnl_pct = if total_spent > 0.0 && avg_entry_price > 0.0 && total_btc > 0.0 {
        let gross_pnl = (price - avg_entry_price) * total_btc;
        let sell_fee = price * total_btc * 0.001;
        let net_pnl = gross_pnl - sell_fee;
        (net_pnl / total_spent) * 100.0
    } else {
        0.0
    };

    // WebSocket activity checker
    let ws_active = (chrono::Utc::now() - *state.last_ws_activity.read().await).num_seconds() <= 30;
    let conclude_active = (chrono::Utc::now() - *state.last_conclude_activity.read().await).num_seconds() <= 120;
    let validate_active = (chrono::Utc::now() - *state.last_validate_activity.read().await).num_seconds() <= 120;
    let executor_active = (chrono::Utc::now() - *state.last_executor_activity.read().await).num_seconds() <= 120;

    // Calculate winrate from cycle summary
    let summary: Option<(i64, i64)> = sqlx::query_as(
        "SELECT COUNT(*), SUM(CASE WHEN status = 'WIN' THEN 1 ELSE 0 END) FROM dca_cycle_summary"
    )
    .fetch_one(&state.db)
    .await
    .ok()
    .map(|r: (i64, i64)| r);

    let (total_cycles, wins) = summary.unwrap_or((0, 0));
    let winrate = if total_cycles > 0 {
        (wins as f64 / total_cycles as f64) * 100.0
    } else {
        0.0
    };

    // CPU/RAM usage info
    let (sys_cpu_pct, sys_mem_mb) = {
        let mut sys = state.sys.write().await;
        sys.refresh_all();
        let cpu = sys.global_cpu_info().cpu_usage() as f64;
        let mem_mb = sys.used_memory() as f64 / 1024.0 / 1024.0;
        (cpu, mem_mb)
    };

    Json(StatusResponse {
        current_price: price,
        simulated_balance: sim_bal,
        btc_balance: btc_bal,
        total_equity,
        layers_filled: layers,
        current_cycle_id: cycle_id,
        avg_entry_price,
        current_pnl_pct,
        cycle_high_water_mark: hwm,
        ws_active,
        conclude_active,
        validate_active,
        executor_active,
        sys_cpu_pct,
        sys_mem_mb,
        start_time: state.start_time,
        winrate,
    })
}

async fn get_history(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, DcaTradeHistory>(
        "SELECT h.id, h.cycle_id, h.action, h.layer, h.price, h.amount, h.usdt_spent, \
                h.timestamp, h.status, h.notes, \
                c.net_pnl, c.pnl_pct \
         FROM dca_trading_history h \
         LEFT JOIN dca_cycle_summary c ON h.cycle_id = c.cycle_id AND h.action = 'SELL' \
         ORDER BY h.id DESC"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn get_cycles(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, DcaCycleSummary>(
        "SELECT id, cycle_id, start_time, end_time, layers_used, avg_entry_price, exit_price, total_spent, net_pnl, pnl_pct, exit_reason, status 
         FROM dca_cycle_summary ORDER BY cycle_id DESC LIMIT 20"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    Json(rows)
}

#[derive(serde::Deserialize)]
struct HistoryQuery {
    all: Option<bool>,
}

async fn get_balance(
    Query(params): Query<HistoryQuery>,
    Extension(state): Extension<AppState>
) -> impl IntoResponse {
    let query_str = if params.all.unwrap_or(false) {
        "SELECT id, simulated_balance, btc_balance, total_value, timestamp FROM ( \
           SELECT id, simulated_balance, btc_balance, total_value, timestamp, \
                  LAG(total_value) OVER (ORDER BY id ASC) as prev_value, \
                  LEAD(total_value) OVER (ORDER BY id ASC) as next_value \
           FROM dca_balance_history \
         ) sub \
         WHERE prev_value IS NULL OR next_value IS NULL OR total_value != prev_value \
         ORDER BY id ASC"
    } else {
        "SELECT id, simulated_balance, btc_balance, total_value, timestamp FROM ( \
           SELECT id, simulated_balance, btc_balance, total_value, timestamp, \
                  LAG(total_value) OVER (ORDER BY id ASC) as prev_value, \
                  LEAD(total_value) OVER (ORDER BY id ASC) as next_value \
           FROM dca_balance_history \
         ) sub \
         WHERE prev_value IS NULL OR next_value IS NULL OR total_value != prev_value \
         ORDER BY id DESC LIMIT 150"
    };

    let mut rows = sqlx::query_as::<_, DcaBalanceHistory>(&query_str)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    if !params.all.unwrap_or(false) {
        rows.reverse();
    } else if rows.len() > 300 {
        let step = rows.len() / 300;
        let mut downsampled = Vec::new();
        for i in (0..rows.len()).step_by(step) {
            downsampled.push(rows[i].clone());
        }
        if let Some(last) = rows.last() {
            if downsampled.last().map(|r| r.id) != Some(last.id) {
                downsampled.push(last.clone());
            }
        }
        return Json(downsampled);
    }

    Json(rows)
}

async fn manual_sell(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let price = *state.current_price.read().await;
    if price <= 0.0 {
        return (axum::http::StatusCode::BAD_REQUEST, "Harga BTC tidak valid").into_response();
    }

    let decision = conclude::Decision::Sell {
        reason: "[Emergency] Manual Sell via Dashboard".to_string(),
    };

    let is_valid = validate::validate_decision(&decision, price, &state).await;
    if is_valid {
        *state.last_executor_activity.write().await = chrono::Utc::now();
        match executor::execute_trade(&decision, price, &state).await {
            Ok(_) => (axum::http::StatusCode::OK, "Simulasi manual sell sukses").into_response(),
            Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("Gagal eksekusi: {}", e)).into_response(),
        }
    } else {
        (axum::http::StatusCode::BAD_REQUEST, "Validasi manual sell gagal (tidak ada BTC atau posisi aktif)").into_response()
    }
}

async fn get_logs(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let logs = state.logs.read().await;
    Json(logs.clone())
}

async fn serve_backtest() -> impl IntoResponse {
    let mut html_content = std::fs::read_to_string("../backtest.html")
        .or_else(|_| std::fs::read_to_string("backtest.html"))
        .unwrap_or_else(|_| "<h1>Backtest HTML file not found</h1>".to_string());
        
    let mut header = std::fs::read_to_string("../includes/header.html")
        .or_else(|_| std::fs::read_to_string("includes/header.html"))
        .unwrap_or_default();
    header = header.replace("BitTrade Engine", "SmartDCA Bot");
    header = header.replace("BitTrade Menu", "SmartDCA Menu");

    let footer = std::fs::read_to_string("../includes/footer.html")
        .or_else(|_| std::fs::read_to_string("includes/footer.html"))
        .unwrap_or_default();
        
    html_content = html_content.replace("<!-- INCLUDE HEADER -->", &header);
    html_content = html_content.replace("<!-- INCLUDE FOOTER -->", &footer);
    
    Html(html_content)
}

async fn get_backtest_results() -> Result<impl IntoResponse, axum::http::StatusCode> {
    let json_content = std::fs::read_to_string("../backtest/backtest_results.json")
        .or_else(|_| std::fs::read_to_string("backtest/backtest_results.json"))
        .or_else(|_| std::fs::read_to_string("backtest_results.json"))
        .map_err(|_| axum::http::StatusCode::NOT_FOUND)?;
        
    Ok(axum::response::Response::builder()
        .header("content-type", "application/json")
        .body(json_content)
        .unwrap())
}

#[derive(serde::Deserialize)]
struct BacktestRunRequest {
    starting_balance: f64,
    tp_hard: f64,
    stop_loss: f64,
}

async fn run_backtest(
    axum::Json(payload): axum::Json<BacktestRunRequest>
) -> Result<impl IntoResponse, axum::http::StatusCode> {
    let python_path = "/root/bittrade-v2-strategi/backtest/venv/bin/python3";
    let script_path = "/root/bittrade-v2-strategi/backtest/run_backtest.py";
    
    let status = tokio::process::Command::new(python_path)
        .arg(script_path)
        .arg(payload.starting_balance.to_string())
        .arg(payload.tp_hard.to_string())
        .arg(payload.stop_loss.to_string())
        .current_dir("/root/bittrade-v2-strategi/backtest")
        .status()
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        
    if status.success() {
        Ok(axum::http::StatusCode::OK)
    } else {
        Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    }
}
