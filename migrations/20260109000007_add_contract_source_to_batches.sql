-- Add contract_source column to batches table (SQLite)
-- Created: 2026-01-09

-- Add contract_source column (NOT NULL with default for existing rows)
ALTER TABLE batches ADD COLUMN contract_source TEXT NOT NULL DEFAULT 'PostageStamp';

-- Add index for contract_source lookups
CREATE INDEX IF NOT EXISTS idx_batches_contract_source ON batches(contract_source);
