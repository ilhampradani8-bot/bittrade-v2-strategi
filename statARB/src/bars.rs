use std::collections::VecDeque;
use reqwest::Client;
use serde_json::Value;
use chrono::Utc;

#[derive(Debug, Clone)]
pub struct Bar {
    pub open_time: i64,
    pub close_time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

pub struct BarAggregator {
    pub interval_secs: i64,
    pub max_history: usize,
    pub current_bar: Option<Bar>,
    pub closes: VecDeque<f64>,
}

impl BarAggregator {
    pub fn new(interval_secs: i64, max_history: usize) -> Self {
        Self {
            interval_secs,
            max_history,
            current_bar: None,
            closes: VecDeque::with_capacity(max_history + 1),
        }
    }

    /// Adds a tick to the aggregator. Returns `true` if a new bar was just closed and pushed to `closes`.
    pub fn add_tick(&mut self, timestamp_ms: i64, price: f64, volume: f64) -> bool {
        let interval_ms = self.interval_secs * 1000;
        let bar_open_time = (timestamp_ms / interval_ms) * interval_ms;
        
        let mut closed_bar = false;

        if let Some(ref mut current) = self.current_bar {
            if bar_open_time > current.open_time {
                // Current bar is complete
                self.closes.push_back(current.close);
                if self.closes.len() > self.max_history {
                    self.closes.pop_front();
                }
                closed_bar = true;
                
                // Start new bar
                self.current_bar = Some(Bar {
                    open_time: bar_open_time,
                    close_time: bar_open_time + interval_ms - 1,
                    open: price,
                    high: price,
                    low: price,
                    close: price,
                    volume,
                });
            } else {
                // Update current bar
                if price > current.high { current.high = price; }
                if price < current.low { current.low = price; }
                current.close = price;
                current.volume += volume;
            }
        } else {
            // First tick ever
            self.current_bar = Some(Bar {
                open_time: bar_open_time,
                close_time: bar_open_time + interval_ms - 1,
                open: price,
                high: price,
                low: price,
                close: price,
                volume,
            });
        }
        
        closed_bar
    }

    /// Helper to get the latest close (either the last closed bar, or current unclosed bar)
    pub fn latest_close(&self) -> Option<f64> {
        self.current_bar.as_ref().map(|b| b.close).or_else(|| self.closes.back().copied())
    }
}

/// Fetches historical bars from Binance Klines API to prepopulate the aggregator.
pub async fn fetch_historical_klines(
    symbol: &str,
    interval_str: &str,
    limit: usize,
) -> Result<VecDeque<f64>, Box<dyn std::error::Error + Send + Sync>> {
    let client = Client::new();
    let url = format!(
        "https://api.binance.com/api/v3/klines?symbol={}&interval={}&limit={}",
        symbol, interval_str, limit
    );

    let response = client.get(&url).send().await?;
    if !response.status().is_success() {
        let err_text = response.text().await?;
        return Err(format!("Binance API error: {}", err_text).into());
    }

    let data: Vec<Value> = response.json().await?;
    let mut closes = VecDeque::with_capacity(limit);

    for kline in data {
        if let Some(arr) = kline.as_array() {
            if arr.len() >= 5 {
                if let Some(close_str) = arr[4].as_str() {
                    if let Ok(close_price) = close_str.parse::<f64>() {
                        closes.push_back(close_price);
                    }
                }
            }
        }
    }

    Ok(closes)
}
