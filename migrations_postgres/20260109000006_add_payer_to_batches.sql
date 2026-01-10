-- Add payer column to batches table (PostgreSQL)
-- Created: 2026-01-09

-- Add payer column (nullable since PostageStamp events don't have payer)
ALTER TABLE batches ADD COLUMN IF NOT EXISTS payer TEXT;

-- Add index for payer lookups
CREATE INDEX IF NOT EXISTS idx_batches_payer ON batches(payer);
