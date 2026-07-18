// INI ADALAH FILE main.rs
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

mod corrector;
mod executor;
mod conclude;
mod validate;
mod get;
pub mod risk;

pub const CURRENT_STRATEGY_VERSION: &str = "v5.0_qps";

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
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
    
    // FIX #3: Timestamp terakhir kali SELL profit (untuk cooldown anti-FOMO)
    pub last_profitable_sell_at: Arc<RwLock<Option<chrono::DateTime<chrono::Utc>>>>,

    // FASE 5.0a: Order Book Imbalance (OBI) untuk mendeteksi dinding jual (Sell Wall)
    pub order_book_imbalance: Arc<RwLock<f64>>,
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

#[derive(serde::Serialize, sqlx::FromRow, Clone)]
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
    pub eth_balance: f64,
    pub current_eth_price: f64,
    pub bnb_balance: f64,
    pub current_bnb_price: f64,
    pub sol_balance: f64,
    pub current_sol_price: f64,
    pub xrp_balance: f64,
    pub current_xrp_price: f64,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub ws_active: bool,
    pub conclude_active: bool,
    pub validate_active: bool,
    pub executor_active: bool,
    pub corrector_active: bool,
    pub market_regime: String,
    pub market_regime_eth: String,
    pub market_regime_bnb: String,
    pub market_regime_sol: String,
    pub market_regime_xrp: String,
    pub whale_detected: bool,
    pub winrate: f64,
    pub total_sells: i64,
    pub win_sells: i64,
    pub market_volatility: f64,
    pub order_book_imbalance: f64,
    pub sys_cpu_pct: f64,
    pub sys_mem_mb: f64,
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

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS bot_active_positions (
            id SERIAL PRIMARY KEY,
            buy_price DOUBLE PRECISION NOT NULL,
            high_water_mark DOUBLE PRECISION NOT NULL,
            amount DOUBLE PRECISION NOT NULL,
            opened_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        );"
    ).execute(&pool).await?;

    // Sync 100 data KLine pertama saat startup
    println!("Melakukan sinkronisasi data historis awal dari Binance...");
    if let Err(e) = get::sync_klines(&pool, "BTCUSDT", 100).await {
        eprintln!("Warning: Gagal sync data kline awal: {}", e);
    }

    let (initial_sim, initial_btc) = reconstruct_balance(&pool).await;
    println!("Rekonstruksi saldo dari riwayat transaksi: USDT: ${:.2}, BTC: {:.6}", initial_sim, initial_btc);

    let initial_hwm: f64 = sqlx::query_scalar("SELECT high_water_mark FROM bot_active_positions ORDER BY id DESC LIMIT 1")
        .fetch_optional(&pool)
        .await
        .unwrap_or(None)
        .unwrap_or(0.0);
    println!("[CRASH RECOVERY] Memulihkan High Water Mark posisi aktif dari tabel bot_active_positions: ${:.2}", initial_hwm);

    let now = chrono::Utc::now();
    let state = AppState {
        db: pool,
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
        last_profitable_sell_at: Arc::new(RwLock::new(None)),
        order_book_imbalance: Arc::new(RwLock::new(0.5)),
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
            if let Err(e) = get::sync_klines(&worker_state.db, "BTCUSDT", 5).await {
                eprintln!("Gagal sync 5 kline terbaru: {}", e);
            }

            // Dapatkan harga dari memori
            let mut price = { *worker_state.current_btc_price.read().await };
            
            // Fallback REST API jika WebSocket tidak aktif atau data stale (>30 detik)
            let last_ws = *worker_state.last_ws_activity.read().await;
            let ws_stale = (chrono::Utc::now() - last_ws).num_seconds() > 30;

            if price <= 0.0 || ws_stale {
                if let Ok(rest_p) = get::get_rest_price().await {
                    let mut price_lock = worker_state.current_btc_price.write().await;
                    *price_lock = rest_p;
                    price = rest_p;
                    if ws_stale {
                        add_log(&worker_state, &format!("[FALLBACK REST] Aliran WebSocket stale (>30 dtk). Harga di-sync via REST API: ${:.2}", price)).await;
                    }
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
        .route("/", get(serve_dashboard_main))
        .route("/dashboard_main.html", get(serve_dashboard_main))
        .route("/dashboard.html", get(serve_dashboard))
        .route("/paper", get(serve_paper_a))
        .route("/paper.html", get(serve_paper_a))
        .route("/paper_a", get(serve_paper_a))
        .route("/paper_b.html", get(serve_paper_b))
        .route("/paper_altcoin", get(serve_paper_altcoin))
        .route("/paper_altcoin.html", get(serve_paper_altcoin))
        .route("/paper_statarb", get(serve_paper_statarb))
        .route("/paper_statarb.html", get(serve_paper_statarb))
        .route("/includes/header.html", get(serve_header))
        .route("/includes/footer.html", get(serve_footer))
        .route("/js/dashboard.js", get(serve_js))
        .route("/js/dashboard_main.js", get(serve_js_main))
        .route("/favicon.ico", get(serve_favicon))
        .route("/favicon.png", get(serve_favicon))
        .route("/backtest.html", get(serve_backtest))
        .route("/api/backtest/results", get(get_backtest_results))
        .route("/api/backtest/run", post(run_backtest))
        .route("/api/status", get(get_status))
        .route("/api/history", get(get_history))
        .route("/api/corrections", get(get_corrections))
        .route("/api/logs", get(get_logs))
        .route("/api/balance_history", get(get_balance_history))
        .route("/api/journal", get(get_journal))
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
    let mut html_content = std::fs::read_to_string("../dashboard.html")
        .or_else(|_| std::fs::read_to_string("dashboard.html"))
        .unwrap_or_else(|_| "<h1>Dashboard HTML file not found</h1>".to_string());
    
    let mut header = std::fs::read_to_string("../includes/header.html")
        .or_else(|_| std::fs::read_to_string("includes/header.html"))
        .unwrap_or_default();
    header = header.replace("BitTrade Engine", "BitTrade Bot A");
    header = header.replace("BitTrade Menu", "BitTrade Menu A");

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
    header = header.replace("BitTrade Engine", "BitTrade Bot A");
    header = header.replace("BitTrade Menu", "BitTrade Menu A");

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
    header = header.replace("BitTrade Engine", "BitTrade Bot A");
    header = header.replace("BitTrade Menu", "BitTrade Menu A");

    let footer = std::fs::read_to_string("../includes/footer.html")
        .or_else(|_| std::fs::read_to_string("includes/footer.html"))
        .unwrap_or_default();
        
    html_content = html_content.replace("<!-- INCLUDE HEADER -->", &header);
    html_content = html_content.replace("<!-- INCLUDE FOOTER -->", &footer);
    Html(html_content)
}

async fn serve_paper_altcoin() -> impl IntoResponse {
    let mut html_content = std::fs::read_to_string("../paper_altcoin.html")
        .or_else(|_| std::fs::read_to_string("paper_altcoin.html"))
        .unwrap_or_else(|_| "<h1>Paper Altcoin HTML file not found</h1>".to_string());
        
    let mut header = std::fs::read_to_string("../includes/header.html")
        .or_else(|_| std::fs::read_to_string("includes/header.html"))
        .unwrap_or_default();
    header = header.replace("BitTrade Engine", "BitTrade Bot A");
    header = header.replace("BitTrade Menu", "BitTrade Menu A");

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
    header = header.replace("BitTrade Engine", "BitTrade Bot A");
    header = header.replace("BitTrade Menu", "BitTrade Menu A");

    let footer = std::fs::read_to_string("../includes/footer.html")
        .or_else(|_| std::fs::read_to_string("includes/footer.html"))
        .unwrap_or_default();
        
    html_content = html_content.replace("<!-- INCLUDE HEADER -->", &header);
    html_content = html_content.replace("<!-- INCLUDE FOOTER -->", &footer);
    
    Html(html_content)
}

async fn serve_header() -> impl IntoResponse {
    let mut html_content = std::fs::read_to_string("../includes/header.html")
        .or_else(|_| std::fs::read_to_string("includes/header.html"))
        .unwrap_or_default();
    html_content = html_content.replace("BitTrade Engine", "BitTrade Bot A");
    html_content = html_content.replace("BitTrade Menu", "BitTrade Menu A");
    Html(html_content)
}

async fn serve_footer() -> impl IntoResponse {
    let html_content = std::fs::read_to_string("../includes/footer.html")
        .or_else(|_| std::fs::read_to_string("includes/footer.html"))
        .unwrap_or_default();
    Html(html_content)
}async fn serve_js() -> impl IntoResponse {
    let js = std::fs::read_to_string("../js/dashboard.js")
        .or_else(|_| std::fs::read_to_string("js/dashboard.js"))
        .unwrap_or_else(|_| "".to_string());
    
    axum::response::Response::builder()
        .header("content-type", "application/javascript")
        .header("cache-control", "no-store, no-cache, must-revalidate")
        .body(js)
        .unwrap()
}

async fn serve_favicon() -> impl IntoResponse {
    let bytes = std::fs::read("../favicon.png")
        .or_else(|_| std::fs::read("favicon.png"))
        .unwrap_or_default();
    ([(axum::http::header::CONTENT_TYPE, "image/png")], bytes)
}

async fn get_status(Extension(state): Extension<AppState>) -> impl IntoResponse {

    let sim_bal = state.simulated_balance.read().await;
    let btc_bal = state.btc_balance.read().await;
    let price = state.current_btc_price.read().await;
    let eth_bal = state.eth_balance.read().await;
    let eth_price = state.current_eth_price.read().await;
    let bnb_bal = state.bnb_balance.read().await;
    let bnb_price = state.current_bnb_price.read().await;
    let sol_bal = state.sol_balance.read().await;
    let sol_price = state.current_sol_price.read().await;
    let xrp_bal = state.xrp_balance.read().await;
    let xrp_price = state.current_xrp_price.read().await;
    let regime = state.market_regime.read().await;
    let regime_eth = state.market_regime_eth.read().await;
    let regime_bnb = state.market_regime_bnb.read().await;
    let regime_sol = state.market_regime_sol.read().await;
    let regime_xrp = state.market_regime_xrp.read().await;
    let whale = *state.whale_detected.read().await;
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
        "SELECT COUNT(*) FROM bot_trading_history WHERE action = 'SELL' AND status = 'SUCCESS' AND (notes LIKE '%P&L: $+%' OR notes LIKE '%P&L: +%')"
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let winrate = if total_sells > 0 {
        (win_sells as f64 / total_sells as f64) * 100.0
    } else {
        0.0
    };

    let (sys_cpu_pct, sys_mem_mb) = {
        let mut sys = state.sys.write().await;
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        let cpu = sys.global_cpu_usage() as f64;
        let mem_mb = sys.used_memory() as f64 / 1024.0 / 1024.0;
        (cpu, mem_mb)
    };

    Json(StatusResponse {
        simulated_balance: *sim_bal,
        btc_balance: *btc_bal,
        current_btc_price: *price,
        eth_balance: *eth_bal,
        current_eth_price: *eth_price,
        bnb_balance: *bnb_bal,
        current_bnb_price: *bnb_price,
        sol_balance: *sol_bal,
        current_sol_price: *sol_price,
        xrp_balance: *xrp_bal,
        current_xrp_price: *xrp_price,
        start_time: state.start_time,
        ws_active,
        conclude_active,
        validate_active,
        executor_active,
        corrector_active,
        market_regime: regime.clone(),
        market_regime_eth: regime_eth.clone(),
        market_regime_bnb: regime_bnb.clone(),
        market_regime_sol: regime_sol.clone(),
        market_regime_xrp: regime_xrp.clone(),
        whale_detected: whale,
        winrate,
        total_sells,
        win_sells,
        market_volatility: *vol,
        order_book_imbalance: *state.order_book_imbalance.read().await,
        sys_cpu_pct,
        sys_mem_mb,
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

#[derive(serde::Deserialize)]
struct HistoryQuery {
    all: Option<bool>,
}

async fn get_balance_history(
    Query(params): Query<HistoryQuery>,
    Extension(state): Extension<AppState>
) -> impl IntoResponse {
    let query_str = if params.all.unwrap_or(false) {
        "SELECT id, simulated_balance, btc_balance, btc_value, total_value, timestamp FROM ( \
           SELECT id, simulated_balance, btc_balance, btc_value, total_value, timestamp, \
                  LAG(total_value) OVER (ORDER BY id ASC) as prev_value, \
                  LEAD(total_value) OVER (ORDER BY id ASC) as next_value \
           FROM bot_balance_history \
         ) sub \
         WHERE prev_value IS NULL OR next_value IS NULL OR total_value != prev_value \
         ORDER BY id ASC"
    } else {
        "SELECT id, simulated_balance, btc_balance, btc_value, total_value, timestamp FROM ( \
           SELECT id, simulated_balance, btc_balance, btc_value, total_value, timestamp, \
                  LAG(total_value) OVER (ORDER BY id ASC) as prev_value, \
                  LEAD(total_value) OVER (ORDER BY id ASC) as next_value \
           FROM bot_balance_history \
         ) sub \
         WHERE prev_value IS NULL OR next_value IS NULL OR total_value != prev_value \
         ORDER BY id DESC LIMIT 150"
    };

    let mut rows = sqlx::query_as::<_, BalanceHistory>(&query_str)
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

async fn get_journal() -> impl IntoResponse {
    let content = std::fs::read_to_string("../trading_journal/JURNAL_HARIAN_FASE4.md")
        .or_else(|_| std::fs::read_to_string("trading_journal/JURNAL_HARIAN_FASE4.md"))
        .unwrap_or_else(|_| "Belum ada catatan jurnal.".to_string());
    content
}

async fn serve_backtest() -> impl IntoResponse {
    let mut html_content = std::fs::read_to_string("../backtest.html")
        .or_else(|_| std::fs::read_to_string("backtest.html"))
        .unwrap_or_else(|_| "<h1>Backtest HTML file not found</h1>".to_string());
        
    let mut header = std::fs::read_to_string("../includes/header.html")
        .or_else(|_| std::fs::read_to_string("includes/header.html"))
        .unwrap_or_default();
    header = header.replace("BitTrade Engine", "BitTrade Bot A");
    header = header.replace("BitTrade Menu", "BitTrade Menu A");

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

async fn serve_dashboard_main() -> impl IntoResponse {
    let mut html_content = std::fs::read_to_string("../dashboard_main.html")
        .or_else(|_| std::fs::read_to_string("dashboard_main.html"))
        .unwrap_or_else(|_| "<h1>Main Dashboard HTML file not found</h1>".to_string());
        
    let mut header = std::fs::read_to_string("../includes/header.html")
        .or_else(|_| std::fs::read_to_string("includes/header.html"))
        .unwrap_or_default();
    header = header.replace("BitTrade Engine", "BitTrade Main Dashboard");
    header = header.replace("BitTrade Menu", "BitTrade Main Menu");

    let footer = std::fs::read_to_string("../includes/footer.html")
        .or_else(|_| std::fs::read_to_string("includes/footer.html"))
        .unwrap_or_default();
        
    html_content = html_content.replace("<!-- INCLUDE HEADER -->", &header);
    html_content = html_content.replace("<!-- INCLUDE FOOTER -->", &footer);
    
    Html(html_content)
}

async fn serve_js_main() -> impl IntoResponse {
    let js = std::fs::read_to_string("../js/dashboard_main.js")
        .or_else(|_| std::fs::read_to_string("js/dashboard_main.js"))
        .unwrap_or_else(|_| "".to_string());
    
    axum::response::Response::builder()
        .header("content-type", "application/javascript")
        .header("cache-control", "no-store, no-cache, must-revalidate")
        .body(js)
        .unwrap()
}

