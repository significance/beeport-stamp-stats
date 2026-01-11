-- Add address_interactions table for tracking funding relationships
-- Part of Phase 2: Address Tracking Implementation

CREATE TABLE IF NOT EXISTS address_interactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_address TEXT NOT NULL,                    -- Sender (funder)
    to_address TEXT NOT NULL,                      -- Recipient
    transaction_hash TEXT NOT NULL,
    amount TEXT NOT NULL,                          -- Transfer amount in wei
    block_number INTEGER NOT NULL,
    block_timestamp INTEGER NOT NULL,

    -- Context: was this interaction related to stamp activity?
    related_to_stamp INTEGER NOT NULL DEFAULT 0,   -- 1 if related, 0 otherwise
    stamp_batch_id TEXT,                           -- If related, which batch

    UNIQUE(transaction_hash, from_address, to_address)
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_interactions_from ON address_interactions(from_address);
CREATE INDEX IF NOT EXISTS idx_interactions_to ON address_interactions(to_address);
CREATE INDEX IF NOT EXISTS idx_interactions_stamp_related ON address_interactions(related_to_stamp);
CREATE INDEX IF NOT EXISTS idx_interactions_block ON address_interactions(block_number);
CREATE INDEX IF NOT EXISTS idx_interactions_timestamp ON address_interactions(block_timestamp);
