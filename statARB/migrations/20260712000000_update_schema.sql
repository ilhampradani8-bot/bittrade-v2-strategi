-- Alter starb_pair_stats to add beta and r2 columns
ALTER TABLE starb_pair_stats ADD COLUMN IF NOT EXISTS beta DOUBLE PRECISION;
ALTER TABLE starb_pair_stats ADD COLUMN IF NOT EXISTS r2 DOUBLE PRECISION;

-- Alter starb_corrections to add severity column
ALTER TABLE starb_corrections ADD COLUMN IF NOT EXISTS severity VARCHAR(50) DEFAULT 'INFO';

-- Alter starb_trading_history to add beta and r2 columns
ALTER TABLE starb_trading_history ADD COLUMN IF NOT EXISTS beta DOUBLE PRECISION;
ALTER TABLE starb_trading_history ADD COLUMN IF NOT EXISTS r2 DOUBLE PRECISION;

-- Alter starb_active_positions to add entry_beta and entry_r2 columns
ALTER TABLE starb_active_positions ADD COLUMN IF NOT EXISTS entry_beta DOUBLE PRECISION;
ALTER TABLE starb_active_positions ADD COLUMN IF NOT EXISTS entry_r2 DOUBLE PRECISION;

-- Create starb_cooldowns table
CREATE TABLE IF NOT EXISTS starb_cooldowns (
    id SERIAL PRIMARY KEY,
    pair_name VARCHAR(50) NOT NULL UNIQUE,
    cooldown_until TIMESTAMPTZ NOT NULL
);
