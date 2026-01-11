-- Add addresses table for comprehensive address tracking
-- Part of Phase 2: Address Tracking Implementation

CREATE TABLE IF NOT EXISTS addresses (
    address TEXT PRIMARY KEY,                      -- Ethereum address (checksummed)

    -- Stamp activity
    stamp_ids TEXT NOT NULL DEFAULT '[]',          -- JSON array of batch IDs owned/purchased
    total_stamps_purchased INTEGER NOT NULL DEFAULT 0,
    total_amount_spent TEXT NOT NULL DEFAULT '0',  -- Total spent in wei

    -- Funding relationships
    top_funders TEXT,                              -- JSON array: [{address, amount, tx_count}]
    is_funder INTEGER NOT NULL DEFAULT 0,          -- 1 if funds other stamp buyers, 0 otherwise
    funded_addresses TEXT DEFAULT '[]',            -- JSON array of addresses this address has funded

    -- Activity metadata
    first_seen INTEGER NOT NULL,                   -- Block timestamp
    last_seen INTEGER NOT NULL,                    -- Block timestamp
    first_block INTEGER NOT NULL,
    last_block INTEGER NOT NULL,
    transaction_count INTEGER NOT NULL DEFAULT 0,

    -- Classification
    address_type TEXT,                             -- 'buyer', 'funder', 'both', 'contract'
    is_contract INTEGER NOT NULL DEFAULT 0,        -- 1 for contract, 0 for EOA

    -- Optional metadata
    label TEXT,                                    -- User-defined label
    notes TEXT                                     -- User-defined notes
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_addresses_is_funder ON addresses(is_funder);
CREATE INDEX IF NOT EXISTS idx_addresses_stamp_count ON addresses(total_stamps_purchased);
CREATE INDEX IF NOT EXISTS idx_addresses_type ON addresses(address_type);
CREATE INDEX IF NOT EXISTS idx_addresses_first_seen ON addresses(first_seen);
CREATE INDEX IF NOT EXISTS idx_addresses_last_seen ON addresses(last_seen);
CREATE INDEX IF NOT EXISTS idx_addresses_is_contract ON addresses(is_contract);
