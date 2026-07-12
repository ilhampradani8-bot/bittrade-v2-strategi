#!/usr/bin/env python3
"""
Download 90 hari data historis BTC/USDT 1m dari Binance
dan simpan ke PostgreSQL tabel btc_klines.

Jalankan: python3 download_history.py
Dependency: pip install requests psycopg2-binary python-dotenv
"""

import requests
import psycopg2
import time
import os
from datetime import datetime, timezone, timedelta
from urllib.parse import urlparse, unquote
from dotenv import load_dotenv

# --- KONFIGURASI ---
SYMBOL      = "BTCUSDT"
INTERVAL    = "1m"
DAYS_BACK   = 90          # Berapa hari ke belakang yang mau didownload
BATCH_SIZE  = 1000        # Maksimum Binance per request
DELAY_SEC   = 0.5         # Jeda antar request (hindari rate limit)
BINANCE_URL = "https://api.binance.com/api/v3/klines"

# Load DATABASE_URL dari .env (di folder utama, satu level di atas folder backtest)
load_dotenv(dotenv_path="../.env")
DATABASE_URL = os.getenv("DATABASE_URL")

def parse_db_url(url):
    """Parse DATABASE_URL menggunakan urllib.parse untuk keandalan maksimum"""
    if not url:
        raise ValueError("DATABASE_URL is not set in environment or .env file.")
    
    parsed = urlparse(url)
    return {
        "host": parsed.hostname,
        "port": parsed.port or 5432,
        "dbname": parsed.path.lstrip('/'),
        "user": unquote(parsed.username) if parsed.username else None,
        "password": unquote(parsed.password) if parsed.password else None
    }

def fetch_klines(start_ms, end_ms):
    """Fetch satu batch klines dari Binance"""
    params = {
        "symbol":    SYMBOL,
        "interval":  INTERVAL,
        "startTime": start_ms,
        "endTime":   end_ms,
        "limit":     BATCH_SIZE,
    }
    resp = requests.get(BINANCE_URL, params=params, timeout=10)
    resp.raise_for_status()
    return resp.json()

def insert_batch(cursor, klines):
    """Insert batch ke PostgreSQL, skip duplikat"""
    sql = """
        INSERT INTO btc_klines (open_time, open_price, high_price, low_price, close_price, volume)
        VALUES (%s, %s, %s, %s, %s, %s)
        ON CONFLICT (open_time) DO NOTHING
    """
    rows = [
        (
            datetime.fromtimestamp(int(k[0]) / 1000, tz=timezone.utc), # open_time (timestamp with time zone)
            float(k[1]),  # open_price
            float(k[2]),  # high_price
            float(k[3]),  # low_price
            float(k[4]),  # close_price
            float(k[5]),  # volume
        )
        for k in klines
    ]
    cursor.executemany(sql, rows)
    return len(rows)

def main():
    print(f"[SmartBacktest] Mulai download {DAYS_BACK} hari data {SYMBOL} {INTERVAL}")
    print(f"[SmartBacktest] Estimasi: ~{DAYS_BACK * 1440 // BATCH_SIZE + 1} requests")

    # Hitung rentang waktu
    end_dt   = datetime.now(timezone.utc)
    start_dt = end_dt - timedelta(days=DAYS_BACK)
    start_ms = int(start_dt.timestamp() * 1000)
    end_ms   = int(end_dt.timestamp() * 1000)

    # Koneksi database
    try:
        db_params = parse_db_url(DATABASE_URL)
        conn = psycopg2.connect(**db_params)
        conn.autocommit = False
        cursor = conn.cursor()
    except Exception as e:
        print(f"[ERROR] Gagal terhubung ke database: {e}")
        return

    # Verifikasi/buat tabel jika belum ada
    try:
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS btc_klines (
                open_time   TIMESTAMP WITH TIME ZONE PRIMARY KEY,
                open_price  DOUBLE PRECISION NOT null,
                high_price  DOUBLE PRECISION NOT null,
                low_price   DOUBLE PRECISION NOT null,
                close_price DOUBLE PRECISION NOT null,
                volume      DOUBLE PRECISION NOT null
            );
        """)
        conn.commit()
    except Exception as e:
        print(f"[ERROR] Gagal memverifikasi/membuat tabel btc_klines: {e}")
        conn.rollback()
        cursor.close()
        conn.close()
        return

    total_inserted = 0
    total_skipped  = 0
    current_ms     = start_ms
    batch_num      = 0

    while current_ms < end_ms:
        batch_num += 1
        batch_end = min(current_ms + BATCH_SIZE * 60 * 1000, end_ms)

        try:
            klines = fetch_klines(current_ms, batch_end)
            if not klines:
                break

            inserted = insert_batch(cursor, klines)
            skipped  = len(klines) - inserted
            total_inserted += inserted
            total_skipped  += skipped

            # Progress log
            first_dt = datetime.fromtimestamp(klines[0][0] / 1000, tz=timezone.utc)
            last_dt  = datetime.fromtimestamp(klines[-1][0] / 1000, tz=timezone.utc)
            pct = (current_ms - start_ms) / (end_ms - start_ms) * 100
            print(f"[Batch {batch_num:03d}] {first_dt.strftime('%Y-%m-%d %H:%M')} → "
                  f"{last_dt.strftime('%Y-%m-%d %H:%M')} | "
                  f"+{inserted} baru, skip {skipped} | Progress: {pct:.1f}%")

            conn.commit()

            # Lanjut ke batch berikutnya
            current_ms = int(klines[-1][0]) + 60000  # +1 menit dari candle terakhir

        except requests.RequestException as e:
            print(f"[ERROR] Request Binance gagal: {e}. Mengulangi dalam 5 detik...")
            time.sleep(5)
            continue
        except Exception as e:
            print(f"[ERROR] Database error: {e}")
            conn.rollback()
            raise

        time.sleep(DELAY_SEC)

    cursor.close()
    conn.close()

    print(f"\n[SELESAI] Total inserted: {total_inserted:,} candle")
    print(f"[SELESAI] Total skipped (duplikat): {total_skipped:,} candle")
    print(f"[SELESAI] Database siap untuk backtest!")

if __name__ == "__main__":
    main()
