-- Add support for PotWithdrawn, PriceUpdate, and CopyBatchFailed events
-- Created: 2026-01-01

-- Make batch_id nullable and add columns for new event types
-- SQLite doesn't support ALTER COLUMN, so we need to recreate the table

-- Step 1: Create new events table with nullable batch_id and new columns
CREATE TABLE IF NOT EXISTS events_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    batch_id TEXT,  -- Now nullable
    block_number INTEGER NOT NULL,
    block_timestamp INTEGER NOT NULL,
    transaction_hash TEXT NOT NULL,
    log_index INTEGER NOT NULL,
    contract_source TEXT NOT NULL DEFAULT 'PostageStamp',
    contract_address TEXT,
    data TEXT NOT NULL,

    -- PotWithdrawn event columns
    pot_recipient TEXT,
    pot_total_amount TEXT,

    -- PriceUpdate event columns
    price TEXT,

    -- CopyBatchFailed event columns
    copy_index TEXT,
    copy_batch_id TEXT,

    UNIQUE(transaction_hash, log_index)
);

-- Step 2: Copy data from old table
INSERT INTO events_new (id, event_type, batch_id, block_number, block_timestamp, transaction_hash, log_index, contract_source, contract_address, data)
SELECT id, event_type, batch_id, block_number, block_timestamp, transaction_hash, log_index, contract_source, contract_address, data
FROM events;

-- Step 3: Drop old table
DROP TABLE events;

-- Step 4: Rename new table
ALTER TABLE events_new RENAME TO events;

-- Step 5: Recreate indexes
CREATE INDEX IF NOT EXISTS idx_events_block ON events(block_number);
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(block_timestamp);
CREATE INDEX IF NOT EXISTS idx_events_batch ON events(batch_id);
CREATE INDEX IF NOT EXISTS idx_events_contract ON events(contract_source);
CREATE INDEX IF NOT EXISTS idx_events_pot_recipient ON events(pot_recipient);
CREATE INDEX IF NOT EXISTS idx_events_price ON events(price);
