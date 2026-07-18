#![allow(clippy::collapsible_if)]

use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
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

mod get;
mod conclude;
mod validate;
mod executor;
mod corrector;
pub mod bars;

// ============================================================
// STATE & DATA STRUCTURES
// ============================================================

/// Real-time price data for assets
#[derive(Clone, Debug, Serialize)]
pub struct PriceData {
    pub symbol: String,
    pub price: f64,
    pub last_update: DateTime<Utc>,
}

/// Statistics for a trading pair
#[derive(Clone, Debug, Serialize)]
pub struct PairStats {
    pub symbol_a: String,
    pub symbol_b: String,
    pub price_a: f64,
    pub price_b: f64,
    pub current_ratio: f64,
    pub rolling_mean: f64,
    pub rolling_std: f64,
    pub z_score: f64,
    pub last_update: DateTime<Utc>,
    // UPGRADE: Added beta and r2 to live PairStats tracking
    pub beta: f64,
    pub r2: f64,
    pub ols_alpha: f64,
}

/// Spread Position Info
#[derive(Clone, Debug, Serialize)]
pub struct SpreadPosition {
    pub id: i32,
    pub pair_name: String,
    pub direction: String, // "BUY_SPREAD" or "SELL_SPREAD"
    pub entry_z_score: f64,
    pub entry_ratio: f64,
    pub entry_price_a: f64,
    pub entry_price_b: f64,
    pub qty_a: f64,
    pub qty_b: f64,
    pub deployed_usdt: f64,
    pub status: String, // "OPEN", "CLOSED"
    pub opened_at: DateTime<Utc>,
    pub exit_price_a: Option<f64>,
    pub exit_price_b: Option<f64>,
    pub exit_ratio: Option<f64>,
    pub exit_z_score: Option<f64>,
    pub net_pnl: f64,
    pub closed_at: Option<DateTime<Utc>>,
    // UPGRADE: Added entry beta and entry r2 for tracking spread position OLS regression details
    pub entry_beta: Option<f64>,
    pub entry_r2: Option<f64>,
}

// UPGRADE: Added PairCircuitBreaker to track stop losses and pause state per pair
#[derive(Clone, Debug, Serialize)]
pub struct PairCircuitBreaker {
    pub consecutive_sl: u32,
    pub paused_until: Option<DateTime<Utc>>,
}

// UPGRADE: Log Level structure for observability
#[derive(Clone, Copy, Debug, Serialize)]
pub enum LogLevel {
    INFO,
    WARN,
    ERROR,
    CRITICAL,
}

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,

    // Simulated cash balance (USDT)
    pub simulated_balance: Arc<RwLock<f64>>,

    // Live prices map: symbol -> PriceData
    pub prices: Arc<RwLock<HashMap<String, PriceData>>>,

    // Current pair statistics: PairName -> PairStats
    pub pair_stats: Arc<RwLock<HashMap<String, PairStats>>>,

    // Active spread positions: PairName -> SpreadPosition
    pub active_positions: Arc<RwLock<HashMap<String, SpreadPosition>>>,

    // Cumulative stats
    pub total_pnl: Arc<RwLock<f64>>,
    pub total_trades: Arc<RwLock<u32>>,
    pub logs: Arc<RwLock<Vec<String>>>,
    pub start_time: DateTime<Utc>,

    // Heartbeats
    pub last_ws_activity: Arc<RwLock<DateTime<Utc>>>,
    pub last_engine_activity: Arc<RwLock<DateTime<Utc>>>,
    pub last_corrector_activity: Arc<RwLock<DateTime<Utc>>>,

    // System info for monitoring
    pub sys: Arc<RwLock<sysinfo::System>>,

    // Configurations
    pub z_entry_threshold: f64, // e.g. 2.0
    pub z_exit_threshold: f64,  // e.g. 0.2
    pub position_size_usdt: f64, // e.g. 100.0
    pub max_positions: usize,
    pub scanner_pairs: Arc<RwLock<Vec<get::ScannerPair>>>,

    // UPGRADE: Added new configuration options loaded from environment
    pub min_samples_for_signal: usize,
    pub interval_secs: i64,
    pub min_r2: f64,
    pub max_consecutive_sl: u32,
    pub consecutive_sl_window_mins: i64,
    pub pause_duration_mins: i64,
    pub max_drawdown_pct: f64,
    pub expected_value_buffer_multiplier: f64,
    pub fee_rate: f64,
    pub cooldown_seconds: i64,
    pub mode: String,

    // UPGRADE: Added safety trackers
    pub cooldowns: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    pub circuit_breakers: Arc<RwLock<HashMap<String, PairCircuitBreaker>>>,
    pub data_feed_healthy: Arc<RwLock<bool>>,
    pub current_samples: Arc<RwLock<usize>>,

    // UPGRADE: Dynamic exchange limits
    pub btc_step_size: Arc<RwLock<f64>>,
    pub eth_step_size: Arc<RwLock<f64>>,
    pub btc_min_notional: Arc<RwLock<f64>>,
    pub eth_min_notional: Arc<RwLock<f64>>,
}

// ============================================================
// API RESPONSE STRUCTS
// ============================================================

#[derive(Serialize)]
pub struct StatusResponse {
    pub simulated_balance: f64,
    pub total_deployed_usdt: f64,
    pub total_equity: f64,
    pub total_pnl: f64,
    pub total_trades: u32,
    pub active_positions_count: usize,
    pub start_time: DateTime<Utc>,
    pub ws_active: bool,
    pub engine_active: bool,
    pub corrector_active: bool,
    pub z_entry_threshold: f64,
    pub z_exit_threshold: f64,
    pub interval_secs: i64,
    pub position_size_usdt: f64,
    pub sys_cpu_pct: f64,
    pub sys_mem_mb: f64,
    // UPGRADE: Added diagnostic stats for frontend
    pub mode: String,
    pub data_feed_healthy: bool,
    pub warmup_progress: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct DBTradeHistory {
    pub id: i32,
    pub pair_name: String,
    pub action: String,
    pub z_score: f64,
    pub ratio: f64,
    pub price_a: f64,
    pub price_b: f64,
    pub amount_a: f64,
    pub amount_b: f64,
    pub net_pnl: Option<f64>,
    pub timestamp: DateTime<Utc>,
    pub notes: Option<String>,
    // UPGRADE: added beta and r2 columns to trade history
    pub beta: Option<f64>,
    pub r2: Option<f64>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct DBCorrection {
    pub id: i32,
    pub error_type: String,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
    // UPGRADE: Added severity level to corrections log
    pub severity: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct DBBalanceHistory {
    pub id: i32,
    pub simulated_balance: f64,
    pub deployed_balance: f64,
    pub total_equity: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct DBPairStatLog {
    pub id: i32,
    pub pair_name: String,
    pub price_a: f64,
    pub price_b: f64,
    pub current_ratio: f64,
    pub rolling_mean: f64,
    pub rolling_std: f64,
    pub z_score: f64,
    pub timestamp: DateTime<Utc>,
    // UPGRADE: Added beta and r2 column logs for statistics
    pub beta: Option<f64>,
    pub r2: Option<f64>,
    pub ols_alpha: Option<f64>,
}

// UPGRADE: Metrics response structure
#[derive(Serialize)]
pub struct MetricsResponse {
    pub total_trades: u32,
    pub win_rate: f64,
    pub avg_fee_drag_pct: f64,
    pub avg_gross_capture_usd: f64,
    pub avg_fee_usd: f64,
    pub profit_factor: f64,
}

// ============================================================
// LOGGER & ALERTS HELPER
// ============================================================

// UPGRADE: Implementation of structure logger with severity levels
pub async fn add_log(state: &AppState, msg: &str) {
    add_log_with_level(state, LogLevel::INFO, msg).await;
}

pub async fn add_log_with_level(state: &AppState, level: LogLevel, msg: &str) {
    let level_str = match level {
        LogLevel::INFO => "INFO",
        LogLevel::WARN => "WARN",
        LogLevel::ERROR => "ERROR",
        LogLevel::CRITICAL => "CRITICAL",
    };
    let ts = Utc::now()
        .with_timezone(&chrono::FixedOffset::east_opt(7 * 3600).unwrap())
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let formatted = format!("[{}] [{}] {}", ts, level_str, msg);
    println!("{}", formatted);

    // Write logs to in-memory list
    {
        let mut logs = state.logs.write().await;
        logs.push(formatted);
        if logs.len() > 100 {
            logs.remove(0);
        }
    }

    // Trigger Telegram alerts for critical failures
    if let LogLevel::CRITICAL = level {
        let _ = trigger_telegram_alert(msg).await;
    }
}

// UPGRADE: Telegram Alert Hook
async fn trigger_telegram_alert(msg: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let token = env::var("TELEGRAM_BOT_TOKEN");
    let chat_id = env::var("TELEGRAM_CHAT_ID");

    if let (Ok(tok), Ok(cid)) = (token, chat_id) {
        let client = reqwest::Client::new();
        let formatted_text = format!("🚨 *statARB CRITICAL ALERT* 🚨\n\n{}", msg);
        let url = format!("https://api.telegram.org/bot{}/sendMessage", tok);
        let _ = client.post(&url)
            .json(&serde_json::json!({
                "chat_id": cid,
                "text": formatted_text,
                "parse_mode": "Markdown"
            }))
            .send()
            .await;
    }
    Ok(())
}

fn get_env_or_default<T>(key: &str, default: T) -> T
where
    T: std::str::FromStr,
{
    match env::var(key) {
        Ok(val) => val.parse::<T>().unwrap_or(default),
        Err(_) => default,
    }
}

// ============================================================
// MAIN ENTRY
// ============================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize cryptography provider for WSS
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install default rustls CryptoProvider");

    println!("╔══════════════════════════════════════════════════════╗");
    println!("║   statARB — Statistical Arbitrage Engine (Rust)      ║");
    println!("╚══════════════════════════════════════════════════════╝");

    dotenvy::from_filename("../.env").ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    // UPGRADE: Run DB migrations to alter/create tables cleanly
    println!("[INIT] Executing SQL migrations...");
    if let Err(e) = sqlx::migrate!().run(&pool).await {
        eprintln!("[WARN] Migration warning/failure (could be due to existing columns): {:?}", e);
    } else {
        println!("[INIT] Database migrations completed successfully!");
    }

    // UPGRADE: Dynamic configurations loaded from environment
    let z_entry_threshold = get_env_or_default("STAT_ARB_Z_ENTRY_THRESHOLD", 2.0);
    let z_exit_threshold = get_env_or_default("STAT_ARB_Z_EXIT_THRESHOLD", 0.2);
    let position_size_usdt = get_env_or_default("STAT_ARB_POSITION_SIZE_USDT", 130.0);
    let max_positions = get_env_or_default("STAT_ARB_MAX_POSITIONS", 1);
    let interval_secs = get_env_or_default("STAT_ARB_INTERVAL_SECS", 300);
    let fee_rate = get_env_or_default("STAT_ARB_FEE_RATE", 0.0016);
    let cooldown_seconds = get_env_or_default("STAT_ARB_COOLDOWN_SECONDS", 60);
    let min_samples_for_signal = get_env_or_default("STAT_ARB_MIN_SAMPLES_FOR_SIGNAL", 96);
    let min_r2 = get_env_or_default("STAT_ARB_MIN_R2", 0.85);
    let max_consecutive_sl = get_env_or_default("STAT_ARB_MAX_CONSECUTIVE_SL", 3);
    let consecutive_sl_window_mins = get_env_or_default("STAT_ARB_CONSECUTIVE_SL_WINDOW_MINS", 5);
    let pause_duration_mins = get_env_or_default("STAT_ARB_PAUSE_DURATION_MINS", 15);
    let max_drawdown_pct = get_env_or_default("STAT_ARB_MAX_DRAWDOWN_PCT", 15.0);
    let expected_value_buffer_multiplier = get_env_or_default("STAT_ARB_EXPECTED_VALUE_BUFFER_MULTIPLIER", 2.5);
    // UPGRADE: Mode is hardcoded to SUSPENDED by default due to empirical research findings.
    let mode = env::var("STAT_ARB_MODE").unwrap_or_else(|_| "SUSPENDED".to_string());

    // UPGRADE: Validation checks on configurations at load time
    if z_entry_threshold <= 0.0 || z_exit_threshold < 0.0 || z_exit_threshold >= z_entry_threshold {
        panic!("Invalid configuration: Z-score boundaries are invalid");
    }
    if position_size_usdt <= 0.0 {
        panic!("Invalid configuration: position size must be greater than zero");
    }
    if max_positions == 0 {
        panic!("Invalid configuration: max positions must be greater than zero");
    }
    if fee_rate < 0.0 {
        panic!("Invalid configuration: fee rate must be positive");
    }
    if min_samples_for_signal < 10 {
        panic!("Invalid configuration: min samples must be at least 10");
    }
    if !(0.0..=1.0).contains(&min_r2) {
        panic!("Invalid configuration: min R2 must be between 0.0 and 1.0");
    }

    // Calculate initial balance
    let initial_balance: f64 = sqlx::query_scalar(
        "SELECT total_equity FROM starb_balance_history ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(&pool)
    .await
    .unwrap_or(None)
    .unwrap_or(10_000.0);

    let deployed_capital: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(deployed_usdt), 0.0) FROM starb_active_positions WHERE status = 'OPEN'"
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0.0);

    let available_balance = (initial_balance - deployed_capital).max(0.0);
    println!("[INIT] Balance: available=${:.2}, total=${:.2}, deployed=${:.2}",
        available_balance, initial_balance, deployed_capital);

    let now = Utc::now();
    let state = AppState {
        db: pool,
        simulated_balance: Arc::new(RwLock::new(available_balance)),
        prices: Arc::new(RwLock::new(HashMap::new())),
        pair_stats: Arc::new(RwLock::new(HashMap::new())),
        active_positions: Arc::new(RwLock::new(HashMap::new())),
        total_pnl: Arc::new(RwLock::new(0.0)),
        total_trades: Arc::new(RwLock::new(0)),
        logs: Arc::new(RwLock::new(Vec::new())),
        start_time: now,
        last_ws_activity: Arc::new(RwLock::new(now)),
        last_engine_activity: Arc::new(RwLock::new(now)),
        last_corrector_activity: Arc::new(RwLock::new(now)),
        sys: Arc::new(RwLock::new(sysinfo::System::new_all())),
        z_entry_threshold,
        z_exit_threshold,
        position_size_usdt,
        max_positions,
        interval_secs,
        scanner_pairs: Arc::new(RwLock::new(Vec::new())),
        // UPGRADE: Config maps & safety variables initialization
        min_samples_for_signal,
        min_r2,
        max_consecutive_sl,
        consecutive_sl_window_mins,
        pause_duration_mins,
        max_drawdown_pct,
        expected_value_buffer_multiplier,
        fee_rate,
        cooldown_seconds,
        mode,
        cooldowns: Arc::new(RwLock::new(HashMap::new())),
        circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
        data_feed_healthy: Arc::new(RwLock::new(true)),
        current_samples: Arc::new(RwLock::new(0)),
        btc_step_size: Arc::new(RwLock::new(0.001)),
        eth_step_size: Arc::new(RwLock::new(0.001)),
        btc_min_notional: Arc::new(RwLock::new(50.0)),
        eth_min_notional: Arc::new(RwLock::new(20.0)),
    };

    // Load active positions from database for recovery
    if let Err(e) = executor::recover_positions(&state).await {
        eprintln!("[WARN] Failed to recover active positions: {}", e);
    }

    // Populate initial balance history if empty
    {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM starb_balance_history")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);
        if count == 0 {
            let _ = sqlx::query(
                "INSERT INTO starb_balance_history (simulated_balance, deployed_balance, total_equity) VALUES ($1, $2, $3)"
            )
            .bind(available_balance)
            .bind(deployed_capital)
            .bind(initial_balance)
            .execute(&state.db)
            .await;
        }
    }

    // ────────── Start Background Services ──────────
    
    // 1. WebSocket stream ingestor for BTCUSDT and ETHUSDT
    let ws_state = state.clone();
    tokio::spawn(get::start_price_listener(ws_state));

    // 2. Statistical Analysis & Trade Arbitrage loop
    let engine_state = state.clone();
    if engine_state.mode != "SUSPENDED" {
        tokio::spawn(conclude::start_analysis_loop(engine_state));
    } else {
        println!("[SUSPENDED] Trade Arbitrage loop dinonaktifkan secara hardcode. Merujuk ke statARB/RESEARCH_FINDINGS.md.");
    }

    // 3. System Corrector & DB Sync Loop
    let corrector_state = state.clone();
    tokio::spawn(corrector::start_corrector_loop(corrector_state));

    // 4. 300+ Coin Co-Integration Spread Scanner Loop
    let scanner_state = state.clone();
    tokio::spawn(get::start_scanner_loop(scanner_state));

    // 5. Exchange Limits Updater
    let limits_state = state.clone();
    tokio::spawn(get::start_exchange_limits_updater(limits_state));

    // ────────── Router Configuration ──────────
    let app = Router::new()
        // Front-end files
        .route("/", get(serve_dashboard))
        .route("/dashboard.html", get(serve_dashboard))
        .route("/dashboard_statarb.html", get(serve_dashboard))
        .route("/statarb", get(serve_dashboard))
        .route("/statarb/", get(serve_dashboard))
        .route("/statarb/dashboard.html", get(serve_dashboard))
        .route("/statarb/dashboard_statarb.html", get(serve_dashboard))
        .route("/paper.html", get(serve_paper))
        .route("/paper_statarb.html", get(serve_paper))
        .route("/paper_statarb", get(serve_paper))
        .route("/paper_a", get(serve_paper_a))
        .route("/paper_b.html", get(serve_paper_b))
        .route("/paper_altcoin.html", get(serve_paper_altcoin))
        .route("/paper_arbitrage.html", get(serve_paper_arbitrage))
        .route("/includes/header.html", get(serve_header))
        .route("/includes/footer.html", get(serve_footer))
        .route("/favicon.ico", get(serve_favicon))
        .route("/favicon.png", get(serve_favicon))
        .route("/js/dashboard.js", get(serve_js))
        // API routes
        .route("/api/status", get(get_status))
        .route("/api/positions", get(get_positions))
        .route("/api/history", get(get_history))
        .route("/api/corrections", get(get_corrections))
        .route("/api/balance_history", get(get_balance_history))
        .route("/api/pair_stats", get(get_pair_stats))
        .route("/api/logs", get(get_logs))
        .route("/api/coin_scanner", get(get_coin_scanner))
        // UPGRADE: Added metrics API
        .route("/api/metrics", get(get_metrics))
        .layer(CorsLayer::permissive())
        .layer(Extension(state));

    let port = 8093;
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    println!("[HTTP] Statistical Arbitrage Engine server running on port {}", port);

    // UPGRADE: Axum server with graceful shutdown handling SIGINT/SIGTERM
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

// UPGRADE: Shutdown signal capture helper
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("[HTTP] Graceful shutdown received. Draining connections and resources...");
}

// ============================================================
// ROUTE HANDLERS
// ============================================================

fn read_file(path1: &str, path2: &str) -> String {
    std::fs::read_to_string(path1)
        .or_else(|_| std::fs::read_to_string(path2))
        .unwrap_or_else(|_| format!("<h1>File not found: {}</h1>", path1))
}

fn inject_includes(mut html: String, title: &str, menu_name: &str) -> String {
    let mut header = read_file("../includes/header.html", "includes/header.html");
    header = header.replace("BitTrade Engine", title);
    header = header.replace("BitTrade Menu", menu_name);

    let footer = read_file("../includes/footer.html", "includes/footer.html");
    html = html.replace("<!-- INCLUDE HEADER -->", &header);
    html = html.replace("<!-- INCLUDE FOOTER -->", &footer);
    html
}

async fn serve_dashboard() -> impl IntoResponse {
    let mut html = read_file("dashboard_statarb.html", "../dashboard_statarb.html");
    if html.contains("File not found") {
        html = read_file("statARB/dashboard.html", "../statARB/dashboard.html");
    }
    if html.contains("File not found") {
        html = read_file("dashboard.html", "statARB/dashboard.html");
    }
    html = inject_includes(html, "BitTrade Bot E (statARB)", "BitTrade Menu E");
    Html(html)
}

async fn serve_paper() -> impl IntoResponse {
    let mut html = read_file("paper.html", "../paper_statarb.html");
    html = inject_includes(html, "BitTrade Bot E (statARB)", "BitTrade Menu E");
    Html(html)
}

async fn serve_paper_a() -> impl IntoResponse {
    let mut html = read_file("../paper_a", "paper_a");
    html = inject_includes(html, "BitTrade Bot E (statARB)", "BitTrade Menu E");
    Html(html)
}

async fn serve_paper_b() -> impl IntoResponse {
    let mut html = read_file("../paper_b.html", "paper_b.html");
    html = inject_includes(html, "BitTrade Bot E (statARB)", "BitTrade Menu E");
    Html(html)
}

async fn serve_paper_altcoin() -> impl IntoResponse {
    let mut html = read_file("../paper_altcoin.html", "paper_altcoin.html");
    html = inject_includes(html, "BitTrade Bot E (statARB)", "BitTrade Menu E");
    Html(html)
}

async fn serve_paper_arbitrage() -> impl IntoResponse {
    let mut html = read_file("../paper_arbitrage.html", "paper_arbitrage.html");
    html = inject_includes(html, "BitTrade Bot E (statARB)", "BitTrade Menu E");
    Html(html)
}

async fn serve_header() -> impl IntoResponse {
    let mut h = read_file("../includes/header.html", "includes/header.html");
    h = h.replace("BitTrade Engine", "BitTrade Bot E (statARB)");
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

async fn serve_js() -> impl IntoResponse {
    let js = std::fs::read_to_string("js/dashboard.js").unwrap_or_else(|_| "console.error('JS not found')".to_string());
    axum::response::Response::builder()
        .header("content-type", "application/javascript")
        .body(js)
        .unwrap()
}

async fn get_status(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let bal = *state.simulated_balance.read().await;
    let positions = state.active_positions.read().await;
    let total_collected = *state.total_pnl.read().await;
    let total_trades = *state.total_trades.read().await;

    let deployed: f64 = positions.values().map(|p| p.deployed_usdt).sum();
    let total_equity = bal + deployed;

    let now = Utc::now();
    let ws_active = now.signed_duration_since(*state.last_ws_activity.read().await).num_seconds() < 15;
    let engine_active = now.signed_duration_since(*state.last_engine_activity.read().await).num_seconds() < 15;
    let corrector_active = now.signed_duration_since(*state.last_corrector_activity.read().await).num_seconds() < 90;

    let (sys_cpu_pct, sys_mem_mb) = {
        let mut sys = state.sys.write().await;
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        (sys.global_cpu_usage() as f64, sys.used_memory() as f64 / 1024.0 / 1024.0)
    };

    // UPGRADE: Compute warmup progress
    let cur_samples = *state.current_samples.read().await;
    let min_samples = state.min_samples_for_signal;
    let warmup_progress = if cur_samples >= min_samples {
        "READY".to_string()
    } else {
        format!("{}/{}", cur_samples, min_samples)
    };

    Json(StatusResponse {
        simulated_balance: bal,
        total_deployed_usdt: deployed,
        total_equity,
        total_pnl: total_collected,
        total_trades,
        active_positions_count: positions.len(),
        start_time: state.start_time,
        ws_active,
        engine_active,
        corrector_active,
        z_entry_threshold: state.z_entry_threshold,
        z_exit_threshold: state.z_exit_threshold,
        interval_secs: state.interval_secs,
        position_size_usdt: state.position_size_usdt,
        sys_cpu_pct,
        sys_mem_mb,
        mode: state.mode.clone(),
        data_feed_healthy: *state.data_feed_healthy.read().await,
        warmup_progress,
    })
}

async fn get_positions(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let positions = state.active_positions.read().await;
    let list: Vec<SpreadPosition> = positions.values().cloned().collect();
    Json(list)
}

async fn get_history(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, DBTradeHistory>(
        "SELECT id, pair_name, action, z_score, ratio, price_a, price_b, amount_a, amount_b, net_pnl, timestamp, notes, beta, r2
         FROM starb_trading_history
         ORDER BY id DESC LIMIT 100"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn get_corrections(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, DBCorrection>(
        "SELECT id, error_type, reason, timestamp, severity FROM starb_corrections ORDER BY id DESC LIMIT 50"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn get_balance_history(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, DBBalanceHistory>(
        "SELECT id, simulated_balance, deployed_balance, total_equity, timestamp
         FROM (SELECT id, simulated_balance, deployed_balance, total_equity, timestamp
               FROM starb_balance_history ORDER BY id DESC LIMIT 200) sub
         ORDER BY id ASC"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn get_pair_stats(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, DBPairStatLog>(
        "SELECT id, pair_name, price_a, price_b, current_ratio, rolling_mean, rolling_std, z_score, timestamp, beta, r2, ols_alpha
         FROM (SELECT id, pair_name, price_a, price_b, current_ratio, rolling_mean, rolling_std, z_score, timestamp, beta, r2, ols_alpha
               FROM starb_pair_stats ORDER BY id DESC LIMIT 200) sub
         ORDER BY id ASC"
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

async fn get_coin_scanner(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let pairs = state.scanner_pairs.read().await;
    Json(pairs.clone())
}

// UPGRADE: Implementation of real-time metrics API endpoint
async fn get_metrics(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT net_pnl, amount_a, amount_b, price_a, price_b FROM starb_trading_history WHERE action LIKE 'CLOSE_%'"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let total_trades = rows.len() as u32;
    if total_trades == 0 {
        return Json(MetricsResponse {
            total_trades: 0,
            win_rate: 0.0,
            avg_fee_drag_pct: 0.0,
            avg_gross_capture_usd: 0.0,
            avg_fee_usd: 0.0,
            profit_factor: 0.0,
        });
    }

    let mut win_count = 0;
    let mut total_fees = 0.0;
    let mut total_gross_pnl = 0.0;
    let mut gross_wins = 0.0;
    let mut gross_losses = 0.0;

    for row in rows {
        let net_pnl: f64 = row.try_get("net_pnl").unwrap_or(0.0);
        let amount_a: f64 = row.try_get("amount_a").unwrap_or(0.0);
        let amount_b: f64 = row.try_get("amount_b").unwrap_or(0.0);
        let price_a: f64 = row.try_get("price_a").unwrap_or(0.0);
        let price_b: f64 = row.try_get("price_b").unwrap_or(0.0);

        if net_pnl > 0.0 {
            win_count += 1;
            gross_wins += net_pnl;
        } else {
            gross_losses += net_pnl.abs();
        }

        // Calculate deployed_usdt exactly as the sum of leg exposures
        let deployed_usdt = (amount_a * price_a) + (amount_b * price_b);
        let fees = deployed_usdt * 0.0016;
        let gross_pnl = net_pnl + fees;

        total_fees += fees;
        total_gross_pnl += gross_pnl;
    }

    let win_rate = (win_count as f64 / total_trades as f64) * 100.0;
    let avg_fee_usd = total_fees / total_trades as f64;
    let avg_gross_capture_usd = total_gross_pnl / total_trades as f64;

    let avg_fee_drag_pct = if total_gross_pnl.abs() > 0.0001 {
        (total_fees / total_gross_pnl.abs()) * 100.0
    } else {
        0.0
    };

    let profit_factor = if gross_losses > 0.0 {
        gross_wins / gross_losses
    } else {
        gross_wins
    };

    Json(MetricsResponse {
        total_trades,
        win_rate,
        avg_fee_drag_pct,
        avg_gross_capture_usd,
        avg_fee_usd,
        profit_factor,
    })
}
