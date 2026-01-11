-- Add transaction_details table for caching full transaction information
-- Part of Phase 2: Address Tracking Implementation

CREATE TABLE IF NOT EXISTS transaction_details (
    transaction_hash TEXT PRIMARY KEY,
    from_address TEXT NOT NULL,
    to_address TEXT,                               -- NULL for contract creation
    value TEXT NOT NULL,                           -- ETH value in wei
    gas_price TEXT,
    gas_used BIGINT,
    block_number BIGINT NOT NULL,
    block_timestamp BIGINT NOT NULL,
    input_data TEXT,                               -- Contract call data
    is_contract_creation BOOLEAN NOT NULL DEFAULT false,
    fetched_at BIGINT NOT NULL                     -- When we fetched this data
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_tx_details_from ON transaction_details(from_address);
CREATE INDEX IF NOT EXISTS idx_tx_details_to ON transaction_details(to_address);
CREATE INDEX IF NOT EXISTS idx_tx_details_block ON transaction_details(block_number);
CREATE INDEX IF NOT EXISTS idx_tx_details_timestamp ON transaction_details(block_timestamp);
