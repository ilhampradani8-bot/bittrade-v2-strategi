use sqlx::postgres::PgPoolOptions;
use std::env;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};
use axum::{
    routing::get,
    response::{Html, Json, IntoResponse},
    Extension, Router,
};
use tower_http::cors::CorsLayer;
use serde::Serialize;

mod corrector;
mod executor;
mod conclude;
mod validate;
mod get;
mod head_allusdt;
mod proses_altcoin;
mod trader_cek;

// ============================================================
// TIPE DATA UTAMA
// ============================================================

/// Data funding rate real-time dari Binance Futures fstream WebSocket
#[derive(Clone, Serialize)]
pub struct FundingData {
    pub symbol: String,
    pub mark_price: f64,       // Harga Mark Futures
    pub index_price: f64,      // Harga Index (≈ Spot)
    pub funding_rate: f64,     // Funding rate periode ini (desimal, contoh: 0.0001 = 0.01%)
    pub next_funding_time_ms: i64,
    pub last_update: DateTime<Utc>,
}

/// Informasi posisi arb yang sedang aktif (in-memory + DB)
#[derive(Clone, Serialize)]
pub struct ArbPositionInfo {
    pub db_id: i32,
    pub symbol: String,
    pub spot_entry_price: f64,
    pub futures_entry_price: f64,
    pub position_size_usdt: f64,
    pub initial_funding_rate: f64,
    pub total_funding_collected: f64,
    pub funding_payments_count: u32,
    pub opened_at: DateTime<Utc>,
    pub last_funding_payment_at: Option<DateTime<Utc>>,
    pub current_mark_price: f64,
    pub current_spot_price: f64,
    pub current_funding_rate: f64,
    pub annualized_yield: f64,          // APR dalam % (contoh: 32.5 = 32.5% per tahun)
    pub consecutive_negative_fr: u8,    // Counter FR negatif berturut
}

// ============================================================
// APP STATE
// ============================================================

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,

    // Saldo simulasi USDT yang tersedia (tidak termasuk yang sedang dideploy ke posisi)
    pub simulated_balance: Arc<RwLock<f64>>,

    // Map semua funding data yang diterima dari fstream WS
    pub funding_data: Arc<RwLock<HashMap<String, FundingData>>>,

    // Map posisi arb aktif saat ini (symbol → ArbPositionInfo)
    pub arb_positions: Arc<RwLock<HashMap<String, ArbPositionInfo>>>,

    // Statistik kumulatif
    pub total_funding_collected: Arc<RwLock<f64>>,
    pub total_positions_opened: Arc<RwLock<u32>>,
    pub total_positions_closed: Arc<RwLock<u32>>,

    // Log aktivitas in-memory (max 100 baris)
    pub logs: Arc<RwLock<Vec<String>>>,
    pub start_time: DateTime<Utc>,

    // Heartbeat process indicators
    pub last_ws_activity: Arc<RwLock<DateTime<Utc>>>,
    pub last_engine_activity: Arc<RwLock<DateTime<Utc>>>,
    pub last_corrector_activity: Arc<RwLock<DateTime<Utc>>>,

    // System metrics
    pub sys: Arc<RwLock<sysinfo::System>>,

    // Konfigurasi engine (di-load dari env atau default)
    pub min_funding_rate: f64,    // Default: 0.0001 (0.01% per periode)
    pub max_positions: usize,     // Default: 10
    pub position_size_usdt: f64,  // Default: 1000.0 per koin
}

// ============================================================
// RESPONSE STRUCTS untuk API
// ============================================================

#[derive(Serialize)]
pub struct StatusResponse {
    pub simulated_balance: f64,
    pub total_deployed_usdt: f64,
    pub total_equity: f64,
    pub total_funding_collected: f64,
    pub total_positions_opened: u32,
    pub total_positions_closed: u32,
    pub active_positions_count: usize,
    pub avg_annualized_yield: f64,
    pub symbols_monitored: usize,
    pub start_time: DateTime<Utc>,
    pub ws_active: bool,
    pub engine_active: bool,
    pub min_funding_rate_pct: f64,
    pub max_positions: usize,
    pub position_size_usdt: f64,
    pub sys_cpu_pct: f64,
    pub sys_mem_mb: f64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct TradeHistory {
    pub id: i32,
    pub action: String,
    pub price: f64,
    pub amount: f64,
    pub timestamp: DateTime<Utc>,
    pub status: Option<String>,
    pub notes: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct CorrectionLog {
    pub id: i32,
    pub error_type: String,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct BalanceHistory {
    pub id: i32,
    pub simulated_balance: f64,
    pub btc_balance: f64,   // Dipakai untuk "deployed capital" di engine baru
    pub btc_value: f64,
    pub total_value: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct FundingLog {
    pub id: i32,
    pub symbol: String,
    pub funding_rate: f64,
    pub payment_amount: f64,
    pub annualized_yield: f64,
    pub position_size_usdt: f64,
    pub timestamp: DateTime<Utc>,
}

// ============================================================
// HELPER
// ============================================================

pub async fn add_log(state: &AppState, msg: &str) {
    let ts = Utc::now()
        .with_timezone(&chrono::FixedOffset::east_opt(7 * 3600).unwrap())
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let formatted = format!("[{}] {}", ts, msg);
    println!("{}", formatted);
    let mut logs = state.logs.write().await;
    logs.push(formatted);
    if logs.len() > 100 {
        logs.remove(0);
    }
}

// ============================================================
// MAIN
// ============================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Inisialisasi provider kriptografi untuk rustls (WSS)
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install default rustls CryptoProvider");

    println!("╔══════════════════════════════════════════════════════╗");
    println!("║   Bot D — Funding Rate Arbitrage Engine (Rust/Tokio) ║");
    println!("╚══════════════════════════════════════════════════════╝");

    dotenvy::from_filename("../.env").ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    // ────────── Inisialisasi Skema DB ──────────
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS alt_trading_history (
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
        "CREATE TABLE IF NOT EXISTS alt_corrections (
            id SERIAL PRIMARY KEY,
            error_type VARCHAR(255) NOT NULL,
            reason TEXT NOT NULL,
            timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        );"
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS alt_balance_history (
            id SERIAL PRIMARY KEY,
            simulated_balance FLOAT NOT NULL,
            btc_balance FLOAT NOT NULL,
            btc_value FLOAT NOT NULL,
            total_value FLOAT NOT NULL,
            timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        );"
    ).execute(&pool).await?;

    // Tabel baru: posisi arb aktif/closed
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS alt_arb_positions (
            id SERIAL PRIMARY KEY,
            symbol VARCHAR(30) NOT NULL,
            spot_entry_price DOUBLE PRECISION NOT NULL,
            futures_entry_price DOUBLE PRECISION NOT NULL,
            position_size_usdt DOUBLE PRECISION NOT NULL,
            initial_funding_rate DOUBLE PRECISION NOT NULL,
            total_funding_collected DOUBLE PRECISION DEFAULT 0.0,
            funding_payments_count INT DEFAULT 0,
            last_funding_payment_at TIMESTAMPTZ,
            opened_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
            closed_at TIMESTAMPTZ,
            close_reason TEXT,
            close_spot_price DOUBLE PRECISION,
            close_futures_price DOUBLE PRECISION,
            net_pnl DOUBLE PRECISION DEFAULT 0.0,
            status VARCHAR(20) DEFAULT 'OPEN'
        );"
    ).execute(&pool).await?;

    // Tabel baru: log setiap pembayaran funding
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS alt_funding_log (
            id SERIAL PRIMARY KEY,
            symbol VARCHAR(30) NOT NULL,
            funding_rate DOUBLE PRECISION NOT NULL,
            payment_amount DOUBLE PRECISION NOT NULL,
            annualized_yield DOUBLE PRECISION NOT NULL,
            position_size_usdt DOUBLE PRECISION NOT NULL,
            timestamp TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        );"
    ).execute(&pool).await?;

    // ────────── Hitung saldo awal ──────────
    // Prioritas: ambil dari balance history terbaru, atau default $10,000
    let initial_balance: f64 = sqlx::query_scalar(
        "SELECT total_value FROM alt_balance_history ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(&pool)
    .await
    .unwrap_or(None)
    .unwrap_or(10_000.0);

    // Kurangi kapital yang masih terkunci di posisi OPEN (agar saldo akurat)
    let deployed_capital: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(position_size_usdt), 0.0) FROM alt_arb_positions WHERE status = 'OPEN'"
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0.0);

    let available_balance = (initial_balance - deployed_capital).max(0.0);
    println!("[INIT] Saldo tersedia: ${:.2} (Total: ${:.2}, Deployed: ${:.2})",
        available_balance, initial_balance, deployed_capital);

    // ────────── Baca konfigurasi dari env ──────────
    let min_fr: f64 = env::var("MIN_FUNDING_RATE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0001);   // 0.01% per periode (default)

    let max_pos: usize = env::var("MAX_ARB_POSITIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    let pos_size: f64 = env::var("POSITION_SIZE_USDT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000.0);

    let now = Utc::now();

    let state = AppState {
        db: pool,
        simulated_balance: Arc::new(RwLock::new(available_balance)),
        funding_data: Arc::new(RwLock::new(HashMap::new())),
        arb_positions: Arc::new(RwLock::new(HashMap::new())),
        total_funding_collected: Arc::new(RwLock::new(0.0)),
        total_positions_opened: Arc::new(RwLock::new(0)),
        total_positions_closed: Arc::new(RwLock::new(0)),
        logs: Arc::new(RwLock::new(Vec::new())),
        start_time: now,
        last_ws_activity: Arc::new(RwLock::new(now)),
        last_engine_activity: Arc::new(RwLock::new(now)),
        last_corrector_activity: Arc::new(RwLock::new(now)),
        sys: Arc::new(RwLock::new(sysinfo::System::new_all())),
        min_funding_rate: min_fr,
        max_positions: max_pos,
        position_size_usdt: pos_size,
    };

    // ────────── Load posisi aktif dari DB (crash recovery) ──────────
    if let Err(e) = head_allusdt::load_arb_positions_from_db(&state).await {
        eprintln!("[WARN] Gagal load posisi arb dari DB: {}", e);
    }

    // ────────── Insert saldo awal ke balance history jika baru ──────────
    {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alt_balance_history")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);
        if count == 0 {
            let _ = sqlx::query(
                "INSERT INTO alt_balance_history (simulated_balance, btc_balance, btc_value, total_value) VALUES ($1, 0, 0, $1)"
            )
            .bind(available_balance)
            .execute(&state.db)
            .await;
        }
    }

    // ────────── Spawn Background Tasks ──────────

    // Task 1: Binance Futures fstream WebSocket listener
    let ws_state = state.clone();
    tokio::spawn(get::start_funding_rate_listener(ws_state));

    // Task 2: Main Arb Engine Processing Loop
    let engine_state = state.clone();
    tokio::spawn(proses_altcoin::start_arb_engine(engine_state));

    // ────────── HTTP Server (Axum Dashboard) ──────────
    let app = Router::new()
        // Dashboard HTML pages
        .route("/", get(serve_dashboard_arbitrage))
        .route("/dashboard_arbitrage.html", get(serve_dashboard_arbitrage))
        .route("/dashboard_main.html", get(serve_dashboard_main))
        .route("/dashboard.html", get(serve_dashboard))
        .route("/paper_arbitrage", get(serve_paper_arbitrage))
        .route("/paper_arbitrage.html", get(serve_paper_arbitrage))
        .route("/paper_statarb", get(serve_paper_statarb))
        .route("/paper_statarb.html", get(serve_paper_statarb))
        .route("/favicon.ico", get(serve_favicon))
        .route("/favicon.png", get(serve_favicon))
        .route("/includes/header.html", get(serve_header))
        .route("/includes/footer.html", get(serve_footer))
        .route("/js/dashboard_arbitrage.js", get(serve_js_arbitrage))
        .route("/js/dashboard_main.js", get(serve_js_main))
        // API Endpoints
        .route("/api/status", get(get_status))
        .route("/api/arb_positions", get(get_arb_positions))
        .route("/api/alt_coins", get(get_arb_positions))   // backward compat untuk dashboard utama
        .route("/api/funding_rates", get(get_top_funding_rates))
        .route("/api/funding_log", get(get_funding_log))
        .route("/api/history", get(get_history))
        .route("/api/history_legacy", get(get_history_legacy))
        .route("/api/corrections", get(get_corrections))
        .route("/api/logs", get(get_logs))
        .route("/api/balance_history", get(get_balance_history))
        .layer(CorsLayer::permissive())
        .layer(Extension(state));

    let port = 8092;
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    println!("[HTTP] Dashboard berjalan di http://localhost:{}", port);
    println!("[HTTP] Config: min_FR={:.4}% | max_pos={} | size_per_coin=${:.0}",
        min_fr * 100.0, max_pos, pos_size);

    axum::serve(listener, app).await?;
    Ok(())
}

// ============================================================
// HTTP HANDLERS — Dashboard Pages
// ============================================================

async fn serve_dashboard_arbitrage() -> impl IntoResponse {
    let mut html = read_file("../dashboard_arbitrage.html", "dashboard_arbitrage.html");
    html = inject_includes(html, "BitTrade Bot D (FR Arb)", "BitTrade Menu D");
    Html(html)
}

async fn serve_dashboard() -> impl IntoResponse {
    let mut html = read_file("../dashboard.html", "dashboard.html");
    html = inject_includes(html, "BitTrade Bot D", "BitTrade Menu D");
    Html(html)
}

async fn serve_dashboard_main() -> impl IntoResponse {
    let mut html = read_file("../dashboard_main.html", "dashboard_main.html");
    html = inject_includes(html, "BitTrade Main Dashboard", "BitTrade Main Menu");
    Html(html)
}

async fn serve_paper_arbitrage() -> impl IntoResponse {
    let mut html = read_file("../paper_arbitrage.html", "paper_arbitrage.html");
    html = inject_includes(html, "BitTrade Bot D", "BitTrade Menu D");
    Html(html)
}

async fn serve_paper_statarb() -> impl IntoResponse {
    let mut html = read_file("../paper_statarb.html", "paper_statarb.html");
    html = inject_includes(html, "BitTrade Bot D", "BitTrade Menu D");
    Html(html)
}

async fn serve_header() -> impl IntoResponse {
    let mut h = read_file("../includes/header.html", "includes/header.html");
    h = h.replace("BitTrade Engine", "BitTrade Bot D (FR Arb)");
    Html(h)
}

async fn serve_footer() -> impl IntoResponse {
    Html(read_file("../includes/footer.html", "includes/footer.html"))
}

async fn serve_favicon() -> impl IntoResponse {
    let bytes = std::fs::read("../favicon.png")
        .or_else(|_| std::fs::read("favicon.png"))
        .unwrap_or_default();
    ([(axum::http::header::CONTENT_TYPE, "image/png")], bytes)
}

async fn serve_js_arbitrage() -> impl IntoResponse {
    let js = read_file("../js/dashboard_arbitrage.js", "js/dashboard_arbitrage.js");
    axum::response::Response::builder()
        .header("content-type", "application/javascript")
        .body(js)
        .unwrap()
}

async fn serve_js_main() -> impl IntoResponse {
    let js = read_file("../js/dashboard_main.js", "js/dashboard_main.js");
    axum::response::Response::builder()
        .header("content-type", "application/javascript")
        .body(js)
        .unwrap()
}

// ============================================================
// HTTP HANDLERS — API Endpoints
// ============================================================

async fn get_status(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let bal = *state.simulated_balance.read().await;
    let positions = state.arb_positions.read().await;
    let fd_map = state.funding_data.read().await;
    let total_collected = *state.total_funding_collected.read().await;
    let total_opened = *state.total_positions_opened.read().await;
    let total_closed = *state.total_positions_closed.read().await;

    let deployed: f64 = positions.values().map(|p| p.position_size_usdt).sum();
    let total_equity = bal + deployed;

    let avg_apr = if positions.is_empty() {
        0.0
    } else {
        positions.values().map(|p| p.annualized_yield).sum::<f64>() / positions.len() as f64
    };

    let now = Utc::now();
    let ws_active = now.signed_duration_since(*state.last_ws_activity.read().await).num_seconds() < 5;
    let engine_active = now.signed_duration_since(*state.last_engine_activity.read().await).num_seconds() < 90;

    let (sys_cpu_pct, sys_mem_mb) = {
        let mut sys = state.sys.write().await;
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        (sys.global_cpu_usage() as f64, sys.used_memory() as f64 / 1024.0 / 1024.0)
    };

    Json(StatusResponse {
        simulated_balance: bal,
        total_deployed_usdt: deployed,
        total_equity,
        total_funding_collected: total_collected,
        total_positions_opened: total_opened,
        total_positions_closed: total_closed,
        active_positions_count: positions.len(),
        avg_annualized_yield: avg_apr,
        symbols_monitored: fd_map.len(),
        start_time: state.start_time,
        ws_active,
        engine_active,
        min_funding_rate_pct: state.min_funding_rate * 100.0,
        max_positions: state.max_positions,
        position_size_usdt: state.position_size_usdt,
        sys_cpu_pct,
        sys_mem_mb,
    })
}

async fn get_arb_positions(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let positions = state.arb_positions.read().await;
    let mut list: Vec<&ArbPositionInfo> = positions.values().collect();
    list.sort_by(|a, b| b.annualized_yield.partial_cmp(&a.annualized_yield).unwrap_or(std::cmp::Ordering::Equal));
    Json(list.into_iter().cloned().collect::<Vec<_>>())
}

async fn get_top_funding_rates(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let fd_map = state.funding_data.read().await;
    let mut list: Vec<&FundingData> = fd_map.values()
        .filter(|f| f.funding_rate > 0.0)
        .collect();
    list.sort_by(|a, b| b.funding_rate.partial_cmp(&a.funding_rate).unwrap_or(std::cmp::Ordering::Equal));
    list.truncate(50); // Top 50
    Json(list.into_iter().cloned().collect::<Vec<_>>())
}

async fn get_funding_log(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, FundingLog>(
        "SELECT id, symbol, funding_rate, payment_amount, annualized_yield, position_size_usdt, timestamp
         FROM alt_funding_log ORDER BY id DESC LIMIT 100"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn get_history(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, TradeHistory>(
        "SELECT id, action, price, amount, timestamp, status, notes
         FROM alt_trading_history 
         WHERE action IN ('OPEN_ARB', 'CLOSE_ARB') 
         ORDER BY id DESC LIMIT 100"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn get_history_legacy(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, TradeHistory>(
        "SELECT id, action, price, amount, timestamp, status, notes
         FROM alt_trading_history 
         WHERE action IN ('BUY', 'SELL') 
         ORDER BY id DESC LIMIT 100"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn get_corrections(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, CorrectionLog>(
        "SELECT id, error_type, reason, timestamp FROM alt_corrections ORDER BY id DESC LIMIT 50"
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
        "SELECT id, simulated_balance, btc_balance, btc_value, total_value, timestamp
         FROM (SELECT id, simulated_balance, btc_balance, btc_value, total_value, timestamp
               FROM alt_balance_history ORDER BY id DESC LIMIT 5000) sub
         ORDER BY id ASC"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    Json(rows)
}

// ============================================================
// UTILITIES
// ============================================================

fn read_file(primary: &str, fallback: &str) -> String {
    std::fs::read_to_string(primary)
        .or_else(|_| std::fs::read_to_string(fallback))
        .unwrap_or_else(|_| format!("<h1>File not found: {}</h1>", primary))
}

fn inject_includes(mut html: String, title: &str, menu: &str) -> String {
    let header = {
        let mut h = read_file("../includes/header.html", "includes/header.html");
        h = h.replace("BitTrade Engine", title);
        h = h.replace("BitTrade Menu", menu);
        h
    };
    let footer = read_file("../includes/footer.html", "includes/footer.html");
    html = html.replace("<!-- INCLUDE HEADER -->", &header);
    html = html.replace("<!-- INCLUDE FOOTER -->", &footer);
    html
}
