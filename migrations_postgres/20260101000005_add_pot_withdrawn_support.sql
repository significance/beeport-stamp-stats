-- Add support for PotWithdrawn, PriceUpdate, and CopyBatchFailed events (PostgreSQL)
-- Created: 2026-01-01

-- Make batch_id nullable to support events like PotWithdrawn that don't have a batch_id
ALTER TABLE events ALTER COLUMN batch_id DROP NOT NULL;

-- Add columns for PotWithdrawn events
ALTER TABLE events ADD COLUMN IF NOT EXISTS pot_recipient TEXT;
ALTER TABLE events ADD COLUMN IF NOT EXISTS pot_total_amount TEXT;

-- Add columns for PriceUpdate events
ALTER TABLE events ADD COLUMN IF NOT EXISTS price TEXT;

-- Add columns for CopyBatchFailed events
ALTER TABLE events ADD COLUMN IF NOT EXISTS copy_index TEXT;
ALTER TABLE events ADD COLUMN IF NOT EXISTS copy_batch_id TEXT;

-- Add indexes for new columns
CREATE INDEX IF NOT EXISTS idx_events_pot_recipient ON events(pot_recipient);
CREATE INDEX IF NOT EXISTS idx_events_price ON events(price);
