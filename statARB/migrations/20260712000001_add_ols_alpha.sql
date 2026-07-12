-- Alter starb_pair_stats to add ols_alpha column
ALTER TABLE starb_pair_stats ADD COLUMN IF NOT EXISTS ols_alpha DOUBLE PRECISION;
