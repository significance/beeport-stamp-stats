-- Payment Channel Support (Bandwidth Incentives)
-- Three tables: factories, deployments, events

-- Table 1: Payment channel factory contracts
CREATE TABLE IF NOT EXISTS payment_channel_factories (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL,
    factory_address VARCHAR NOT NULL UNIQUE,
    deployment_block BIGINT NOT NULL,
    network VARCHAR NOT NULL,
    active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_factories_address ON payment_channel_factories(factory_address);

-- Table 2: Discovered chequebook deployments
CREATE TABLE IF NOT EXISTS payment_channel_deployments (
    id SERIAL PRIMARY KEY,
    chequebook_address VARCHAR NOT NULL UNIQUE,
    factory_address VARCHAR NOT NULL,
    deployed_at_block BIGINT NOT NULL,
    deployed_at_timestamp BIGINT NOT NULL,
    transaction_hash VARCHAR NOT NULL,
    issuer_address VARCHAR,
    overlay_address VARCHAR,
    discovered_at BIGINT NOT NULL,
    FOREIGN KEY (factory_address) REFERENCES payment_channel_factories(factory_address)
);

CREATE INDEX IF NOT EXISTS idx_deployments_address ON payment_channel_deployments(chequebook_address);
CREATE INDEX IF NOT EXISTS idx_deployments_factory ON payment_channel_deployments(factory_address);
CREATE INDEX IF NOT EXISTS idx_deployments_block ON payment_channel_deployments(deployed_at_block);
CREATE INDEX IF NOT EXISTS idx_deployments_issuer ON payment_channel_deployments(issuer_address);

-- Table 3: Payment channel events (from individual chequebooks)
CREATE TABLE IF NOT EXISTS payment_channel_events (
    id SERIAL PRIMARY KEY,
    event_type VARCHAR NOT NULL,
    chequebook_address VARCHAR NOT NULL,
    block_number BIGINT NOT NULL,
    block_timestamp BIGINT NOT NULL,
    transaction_hash VARCHAR NOT NULL,
    log_index BIGINT NOT NULL,

    -- ChequeCashed fields
    beneficiary VARCHAR,
    recipient VARCHAR,
    caller VARCHAR,
    total_payout VARCHAR,
    cumulative_payout VARCHAR,
    caller_payout VARCHAR,

    -- HardDeposit fields
    deposit_beneficiary VARCHAR,
    deposit_amount VARCHAR,
    deposit_decrease_amount VARCHAR,
    deposit_timeout BIGINT,

    -- Withdraw fields
    withdraw_amount VARCHAR,

    -- Raw data for debugging
    data JSONB,

    UNIQUE (transaction_hash, log_index),
    FOREIGN KEY (chequebook_address) REFERENCES payment_channel_deployments(chequebook_address)
);

CREATE INDEX IF NOT EXISTS idx_pc_events_chequebook ON payment_channel_events(chequebook_address);
CREATE INDEX IF NOT EXISTS idx_pc_events_block ON payment_channel_events(block_number);
CREATE INDEX IF NOT EXISTS idx_pc_events_type ON payment_channel_events(event_type);
CREATE INDEX IF NOT EXISTS idx_pc_events_timestamp ON payment_channel_events(block_timestamp);
CREATE INDEX IF NOT EXISTS idx_pc_events_beneficiary ON payment_channel_events(beneficiary);
