-- Add storage_incentives_events table for PriceOracle, StakeRegistry, and Redistribution events
-- Created: 2025-12-20
-- Covers 17 event types across 3 contracts

-- Storage Incentives Events table: unified table for all storage incentives contract events
CREATE TABLE IF NOT EXISTS storage_incentives_events (
    -- Core event metadata (always present)
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    block_number INTEGER NOT NULL,
    block_timestamp INTEGER NOT NULL,
    transaction_hash TEXT NOT NULL,
    log_index INTEGER NOT NULL,
    contract_source TEXT NOT NULL,  -- 'PriceOracle', 'StakeRegistry', 'Redistribution'
    event_type TEXT NOT NULL,

    -- Calculated/derived fields
    round_number INTEGER,           -- block_number / 152 (for redistribution/price oracle)
    phase TEXT,                     -- 'commit', 'reveal', 'claim' (for redistribution events)

    -- Common identity fields
    owner_address TEXT,             -- Ethereum address (staking, redistribution)
    overlay TEXT,                   -- bytes32 as hex string (staking, redistribution)

    -- PriceOracle specific fields
    price TEXT,                     -- uint256 (stored as string to avoid overflow)

    -- StakeRegistry specific fields
    committed_stake TEXT,           -- uint256
    potential_stake TEXT,           -- uint256
    height INTEGER,                 -- uint8
    slash_amount TEXT,              -- uint256
    freeze_time TEXT,               -- uint256 (blocks)
    withdraw_amount TEXT,           -- uint256

    -- Redistribution specific - Commit/Reveal data
    stake TEXT,                     -- uint256
    stake_density TEXT,             -- uint256
    reserve_commitment TEXT,        -- bytes32
    depth INTEGER,                  -- uint8

    -- Redistribution specific - Claim phase data
    anchor TEXT,                    -- bytes32
    truth_hash TEXT,                -- bytes32
    truth_depth INTEGER,            -- uint8

    -- Redistribution specific - Winner data (from Reveal struct in WinnerSelected event)
    winner_overlay TEXT,            -- bytes32
    winner_owner TEXT,              -- address
    winner_depth INTEGER,           -- uint8
    winner_stake TEXT,              -- uint256
    winner_stake_density TEXT,      -- uint256
    winner_hash TEXT,               -- bytes32

    -- Redistribution specific - Statistics
    commit_count INTEGER,
    reveal_count INTEGER,
    chunk_count INTEGER,
    redundancy_count INTEGER,

    -- Redistribution specific - Chunk proofs
    chunk_index_in_rc INTEGER,
    chunk_address TEXT,             -- bytes32

    UNIQUE(transaction_hash, log_index)
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_si_contract_source ON storage_incentives_events(contract_source);
CREATE INDEX IF NOT EXISTS idx_si_event_type ON storage_incentives_events(event_type);
CREATE INDEX IF NOT EXISTS idx_si_block_number ON storage_incentives_events(block_number);
CREATE INDEX IF NOT EXISTS idx_si_round_number ON storage_incentives_events(round_number);
CREATE INDEX IF NOT EXISTS idx_si_phase ON storage_incentives_events(phase);
CREATE INDEX IF NOT EXISTS idx_si_overlay ON storage_incentives_events(overlay);
CREATE INDEX IF NOT EXISTS idx_si_owner_address ON storage_incentives_events(owner_address);
CREATE INDEX IF NOT EXISTS idx_si_block_timestamp ON storage_incentives_events(block_timestamp);
