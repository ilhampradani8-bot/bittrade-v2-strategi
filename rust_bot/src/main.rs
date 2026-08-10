// INI ADALAH FILE main.rs
use sqlx::postgres::PgPoolOptions;
use reqwest;
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
pub mod conclude;
mod validate;
mod get;
pub mod risk;
pub mod uptrend;
pub mod sideways;
pub mod downtrend;
pub mod breakout;
pub mod classifier;
pub mod calibration;

pub const CURRENT_STRATEGY_VERSION: &str = "v5.0_qps";

#[derive(Clone, Debug, serde::Serialize)]
pub struct CoinState {
    pub symbol: String,
    pub price: f64,
    pub change_24h: f64,
    pub order_book_imbalance: f64,
    pub market_regime: String,
    pub volatility_category: String,
    pub trend_status: String,
    pub quote_volume: f64,
    pub daily_volatility: f64,
}

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub coin_states: Arc<RwLock<std::collections::HashMap<String, CoinState>>>,
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
    pub active_symbol: String,
    pub volatility_category: String,
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

#[derive(serde::Serialize, Clone, Debug)]
pub struct StatusPositionInfo {
    pub buy_price: f64,
    pub high_water_mark: f64,
    pub amount: f64,
    pub opened_at: String,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct CoinStateResponse {
    pub symbol: String,
    pub price: f64,
    pub change_24h: f64,
    pub order_book_imbalance: f64,
    pub market_regime: String,
    pub volatility_category: String,
    pub trend_status: String,
    pub quote_volume: f64,
    pub daily_volatility: f64,
    pub position: Option<StatusPositionInfo>,
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
    pub active_symbol: String,
    pub volatility_category: String,
    pub coin_states: Vec<CoinStateResponse>,
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
    let mut usdt = 200.0;
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
    // Load config dari parent dir .env
    dotenvy::from_filename("../.env").ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let symbol = env::var("SYMBOL").unwrap_or_else(|_| "BTCUSDT".to_string()).to_uppercase();
    
    println!("Memulai Bot Trading {}...", symbol);
    
    // Setup DB
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    let category = classifier::classify_coin_rust(&symbol).await;
    println!("[CVC] Aset aktif: {} | Kategori volatilitas: {}", symbol, category);



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

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS bot_a_parameters (
            category VARCHAR(20) PRIMARY KEY,
            stop_loss_limit DOUBLE PRECISION NOT NULL,
            uptrend_tp_trail_trigger DOUBLE PRECISION NOT NULL,
            uptrend_tp_trail_pullback DOUBLE PRECISION NOT NULL
        );"
    ).execute(&pool).await?;

    // Sync 100 data KLine pertama saat startup
    println!("Melakukan sinkronisasi data historis awal dari Binance...");
    if let Err(e) = get::sync_klines(&pool, &symbol, 100).await {
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

    let initial_states = std::collections::HashMap::new();

    let now = chrono::Utc::now();
    let state = AppState {
        db: pool,
        coin_states: Arc::new(RwLock::new(initial_states)),
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
        active_symbol: symbol.clone(),
        volatility_category: category.clone(),
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

    // Synchronize parameters at startup
    if let Err(e) = calibration::sync_strategy_parameters(&state.db).await {
        eprintln!("[Sync] Gagal memuat parameter dinamis saat startup: {}", e);
    } else {
        add_log(&state, "[Sync] Sinkronisasi parameter strategi awal dari database bot_a_parameters berhasil.").await;
    }

    // Spawn parameters background sync task (runs every 5 minutes)
    let sync_pool = state.db.clone();
    let sync_state = state.clone();
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(300)).await;
            if let Err(e) = calibration::sync_strategy_parameters(&sync_pool).await {
                eprintln!("[Sync Error] Gagal menyinkronkan parameter dari DB: {}", e);
            } else {
                add_log(&sync_state, "[Sync] Auto-Sync parameter strategi dari database bot_a_parameters berhasil.").await;
            }
        }
    });

    // 1. Spawn WebSocket Price Listener di background (Real-time)
    let listener_state = state.clone();
    tokio::spawn(get::start_price_listener(listener_state));

    // 2. Jalankan Background Loop Trader (Eksekusi / Analisa tiap menit)
    let worker_state = state.clone();
    tokio::spawn(async move {
        // Beri waktu 3 detik agar websocket mendapatkan harga awal
        sleep(Duration::from_secs(3)).await;

        loop {
            // Proteksi Delisting: cek dan bersihkan koin yang dihapus dari Binance
            if let Err(e) = check_and_purge_delisted_coins(&worker_state).await {
                eprintln!("[Delist Protection] Gagal melakukan pengecekan koin delist: {}", e);
            }

            // Kita kumpulkan koin yang harganya valid (> 0.0)
            let coins_to_analyze: Vec<(String, f64, String)> = {
                let map = worker_state.coin_states.read().await;
                let mut list: Vec<CoinState> = map.values().cloned().collect();
                // Filter volume >= 1,000,000 USDT
                list.retain(|c| c.quote_volume >= 1_000_000.0);
                // Sort by quote volume descending
                list.sort_by(|a, b| b.quote_volume.partial_cmp(&a.quote_volume).unwrap_or(std::cmp::Ordering::Equal));
                // Limit to top 100
                list.truncate(100);
                list.into_iter()
                    .map(|c| (c.symbol, c.price, c.volatility_category))
                    .collect()
            };

            *worker_state.last_conclude_activity.write().await = chrono::Utc::now();
            *worker_state.last_validate_activity.write().await = chrono::Utc::now();
            *worker_state.last_executor_activity.write().await = chrono::Utc::now();

            // Jalankan analisa untuk tiap koin secara paralel menggunakan tokio::spawn
            for (symbol, price, _category) in coins_to_analyze {
                if price <= 0.0 {
                    continue;
                }
                
                let coin_state = worker_state.clone();
                tokio::spawn(async move {
                    // Cek jumlah kline di DB untuk symbol ini
                    let db_klines_cnt: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM crypto_klines WHERE symbol = $1")
                        .bind(&symbol)
                        .fetch_one(&coin_state.db)
                        .await
                        .unwrap_or(0);
                    let limit = if db_klines_cnt < 35 { 50 } else { 5 };
                    if let Err(e) = get::sync_klines(&coin_state.db, &symbol, limit).await {
                        println!("[{}] ERR SYNC: {}", symbol, e);
                    }

                    // Conclude: Analisa dan buat keputusan
                    let decision = conclude::analyze_market_for_symbol(&symbol, price, &coin_state).await;
                    
                    // Update trend status di coin_states berdasarkan regime koin
                    let regime = match &decision {
                        conclude::Decision::Buy(_, _) => "BULLISH".to_string(),
                        conclude::Decision::Sell(_, _) => "BEARISH".to_string(),
                        _ => "SIDEWAYS".to_string(),
                    };
                    {
                        let mut map = coin_state.coin_states.write().await;
                        if let Some(c) = map.get_mut(&symbol) {
                            c.trend_status = regime.clone();
                            c.market_regime = regime;
                        }
                    }

                    // Validate: Validasi apakah aman untuk transaksi
                    let is_valid = validate::validate_decision(&decision, &symbol, price, &coin_state).await;

                    // Executor: Lakukan eksekusi (simulasi)
                    if is_valid {
                        if let Err(e) = executor::execute_trade(&decision, &symbol, price, &coin_state).await {
                            eprintln!("[{}] Eksekusi Gagal: {}", symbol, e);
                        }
                    }
                });
            }

            // Simpan perkembangan total modal (simulated balance + value of all coin holdings) ke database
            let sim_bal = *worker_state.simulated_balance.read().await;
            
            // Hitung nilai seluruh aset koin yang dipegang dari tabel bot_active_positions
            let active_positions: Vec<(String, f64)> = sqlx::query_as::<_, (String, f64)>(
                "SELECT symbol, amount FROM bot_active_positions"
            )
            .fetch_all(&worker_state.db)
            .await
            .unwrap_or_default();

            let mut total_holdings_value = 0.0;
            {
                let map = worker_state.coin_states.read().await;
                for (sym, amount) in active_positions {
                    if let Some(c) = map.get(&sym) {
                        total_holdings_value += amount * c.price;
                    }
                }
            }

            let total_val = sim_bal + total_holdings_value;
            sqlx::query(
                "INSERT INTO bot_balance_history (simulated_balance, btc_balance, btc_value, total_value) VALUES ($1, $2, $3, $4)"
            )
            .bind(sim_bal)
            .bind(0.0)
            .bind(total_holdings_value)
            .bind(total_val)
            .execute(&worker_state.db)
            .await
            .ok();

            // Tunggu 60 detik sebelum siklus menit berikutnya
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
        .route("/api/parameters", get(calibration::get_parameters))
        .route("/api/parameters/update", post(calibration::update_parameters))
        .route("/api/parameters/calibrate", post(calibration::run_calibration))
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

    // Ambil data posisi aktif untuk seluruh koin
    let positions: Vec<(String, f64, f64, f64, chrono::DateTime<chrono::Utc>)> = sqlx::query_as::<_, (String, f64, f64, f64, chrono::DateTime<chrono::Utc>)>(
        "SELECT symbol, buy_price, high_water_mark, amount, opened_at FROM bot_active_positions"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut pos_map: std::collections::HashMap<String, StatusPositionInfo> = std::collections::HashMap::new();
    for pos in positions {
        let symbol = pos.0.clone();
        let buy_price = pos.1;
        let hwm = pos.2;
        let amount = pos.3;
        let opened_at = pos.4;
        
        pos_map.entry(symbol)
            .and_modify(|existing| {
                let total_cost = (existing.buy_price * existing.amount) + (buy_price * amount);
                existing.amount += amount;
                if existing.amount > 0.0 {
                    existing.buy_price = total_cost / existing.amount;
                }
                if hwm > existing.high_water_mark {
                    existing.high_water_mark = hwm;
                }
                if let Ok(existing_time) = chrono::DateTime::parse_from_rfc3339(&existing.opened_at) {
                    if opened_at < existing_time {
                        existing.opened_at = opened_at.to_rfc3339();
                    }
                }
            })
            .or_insert(StatusPositionInfo {
                buy_price,
                high_water_mark: hwm,
                amount,
                opened_at: opened_at.to_rfc3339(),
            });
    }

    let coin_states_list: Vec<CoinStateResponse> = {
        let map = state.coin_states.read().await;
        map.values()
            .map(|c| {
                let pos = pos_map.get(&c.symbol).cloned();
                CoinStateResponse {
                    symbol: c.symbol.clone(),
                    price: c.price,
                    change_24h: c.change_24h,
                    order_book_imbalance: c.order_book_imbalance,
                    market_regime: c.market_regime.clone(),
                    volatility_category: c.volatility_category.clone(),
                    trend_status: c.trend_status.clone(),
                    quote_volume: c.quote_volume,
                    daily_volatility: c.daily_volatility,
                    position: pos,
                }
            }).collect()
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
        active_symbol: state.active_symbol.clone(),
        volatility_category: state.volatility_category.clone(),
        coin_states: coin_states_list,
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

async fn check_and_purge_delisted_coins(state: &AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Ambil seluruh koin unik yang sedang di-hold
    let held_symbols: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT symbol FROM bot_active_positions"
    )
    .fetch_all(&state.db)
    .await?;

    for symbol in held_symbols {
        // 2. Cek apakah koin masih aktif di Binance dengan memanggil ticker/price
        let url = format!("https://api.binance.com/api/v3/ticker/price?symbol={}", symbol);
        let resp = reqwest::get(&url).await;
        
        let is_delisted = match resp {
            Ok(res) => {
                if res.status() == reqwest::StatusCode::BAD_REQUEST {
                    true
                } else if let Ok(json) = res.json::<serde_json::Value>().await {
                    if let Some(code) = json.get("code").and_then(|c| c.as_i64()) {
                        code == -1121 // -1121 is "Invalid symbol"
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Err(_) => false, // Jangan anggap delisted jika isu koneksi
        };

        if is_delisted {
            let msg = format!(
                "[{}] Proteksi Delisting: Koin telah dihapus/delist dari Binance! Membersihkan posisi aktif.",
                symbol
            );
            println!("{}", msg);
            corrector::log_error(state, "COIN_DELISTED_ALERT", &msg).await;

            // Ambil jumlah koin yang sedang di-hold untuk mencatat riwayat jual darurat (delisted)
            let held_amount: f64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount), 0.0) FROM bot_active_positions WHERE symbol = $1"
            )
            .bind(&symbol)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0.0);

            if held_amount > 0.0 {
                // Hapus posisi dari bot_active_positions
                sqlx::query("DELETE FROM bot_active_positions WHERE symbol = $1")
                    .bind(&symbol)
                    .execute(&state.db)
                    .await?;

                // Catat transaksi SELL darurat karena delisting dengan harga 0.0
                let notes = format!("PROTEKSI DELISTING: Posisi otomatis dihapus karena koin dihapus dari Binance. Jumlah: {:.6}", held_amount);
                sqlx::query(
                    "INSERT INTO bot_trading_history (action, price, amount, status, notes, symbol, strategy_version) VALUES ($1, $2, $3, $4, $5, $6, $7)"
                )
                .bind("SELL")
                .bind(0.0)
                .bind(held_amount)
                .bind("SUCCESS")
                .bind(notes)
                .bind(&symbol)
                .bind(CURRENT_STRATEGY_VERSION)
                .execute(&state.db)
                .await?;
            }
        }
    }
    Ok(())
}

