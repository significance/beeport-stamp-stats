-- Payment Channel Support (Bandwidth Incentives)
-- Three tables: factories, deployments, events

-- Table 1: Payment channel factory contracts
CREATE TABLE IF NOT EXISTS payment_channel_factories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    factory_address TEXT NOT NULL UNIQUE,
    deployment_block INTEGER NOT NULL,
    network TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_factories_address ON payment_channel_factories(factory_address);

-- Table 2: Discovered chequebook deployments
CREATE TABLE IF NOT EXISTS payment_channel_deployments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chequebook_address TEXT NOT NULL UNIQUE,
    factory_address TEXT NOT NULL,
    deployed_at_block INTEGER NOT NULL,
    deployed_at_timestamp TIMESTAMP NOT NULL,
    transaction_hash TEXT NOT NULL,
    issuer_address TEXT,
    overlay_address TEXT,
    discovered_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (factory_address) REFERENCES payment_channel_factories(factory_address)
);

CREATE INDEX IF NOT EXISTS idx_deployments_address ON payment_channel_deployments(chequebook_address);
CREATE INDEX IF NOT EXISTS idx_deployments_factory ON payment_channel_deployments(factory_address);
CREATE INDEX IF NOT EXISTS idx_deployments_block ON payment_channel_deployments(deployed_at_block);
CREATE INDEX IF NOT EXISTS idx_deployments_issuer ON payment_channel_deployments(issuer_address);

-- Table 3: Payment channel events (from individual chequebooks)
CREATE TABLE IF NOT EXISTS payment_channel_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    chequebook_address TEXT NOT NULL,
    block_number INTEGER NOT NULL,
    block_timestamp TIMESTAMP NOT NULL,
    transaction_hash TEXT NOT NULL,
    log_index INTEGER NOT NULL,

    -- ChequeCashed fields
    beneficiary TEXT,
    recipient TEXT,
    caller TEXT,
    total_payout TEXT,
    cumulative_payout TEXT,
    caller_payout TEXT,

    -- HardDeposit fields
    deposit_beneficiary TEXT,
    deposit_amount TEXT,
    deposit_decrease_amount TEXT,
    deposit_timeout INTEGER,

    -- Withdraw fields
    withdraw_amount TEXT,

    -- Raw data for debugging
    data TEXT,

    UNIQUE (transaction_hash, log_index),
    FOREIGN KEY (chequebook_address) REFERENCES payment_channel_deployments(chequebook_address)
);

CREATE INDEX IF NOT EXISTS idx_pc_events_chequebook ON payment_channel_events(chequebook_address);
CREATE INDEX IF NOT EXISTS idx_pc_events_block ON payment_channel_events(block_number);
CREATE INDEX IF NOT EXISTS idx_pc_events_type ON payment_channel_events(event_type);
CREATE INDEX IF NOT EXISTS idx_pc_events_timestamp ON payment_channel_events(block_timestamp);
CREATE INDEX IF NOT EXISTS idx_pc_events_beneficiary ON payment_channel_events(beneficiary);
