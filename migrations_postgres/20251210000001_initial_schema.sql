-- Initial schema for beeport-stamp-stats database (PostgreSQL)
-- Created: 2025-12-10

-- Events table: stores all blockchain events
CREATE TABLE IF NOT EXISTS events (
    id BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    batch_id TEXT NOT NULL,
    block_number BIGINT NOT NULL,
    block_timestamp BIGINT NOT NULL,
    transaction_hash TEXT NOT NULL,
    log_index BIGINT NOT NULL,
    contract_source TEXT NOT NULL DEFAULT 'PostageStamp',
    data TEXT NOT NULL,
    UNIQUE(transaction_hash, log_index)
);

-- RPC cache table: caches chunk fetch results
CREATE TABLE IF NOT EXISTS rpc_cache (
    chunk_hash TEXT PRIMARY KEY,
    contract_address TEXT NOT NULL,
    from_block BIGINT NOT NULL,
    to_block BIGINT NOT NULL,
    processed_at BIGINT NOT NULL,
    event_count BIGINT NOT NULL
);

-- Batches table: stores batch creation information
CREATE TABLE IF NOT EXISTS batches (
    batch_id TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    depth BIGINT NOT NULL,
    bucket_depth BIGINT NOT NULL,
    immutable BIGINT NOT NULL,
    normalised_balance TEXT NOT NULL,
    created_at BIGINT NOT NULL
);

-- Batch balances table: caches balance lookups
CREATE TABLE IF NOT EXISTS batch_balances (
    batch_id TEXT PRIMARY KEY,
    remaining_balance TEXT NOT NULL,
    fetched_at BIGINT NOT NULL,
    fetched_block BIGINT NOT NULL
);

-- Cache metadata table: stores last cached price and block
CREATE TABLE IF NOT EXISTS cache_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at BIGINT NOT NULL
);

-- Indexes for better query performance
CREATE INDEX IF NOT EXISTS idx_events_block ON events(block_number);
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(block_timestamp);
CREATE INDEX IF NOT EXISTS idx_events_batch ON events(batch_id);
CREATE INDEX IF NOT EXISTS idx_events_contract ON events(contract_source);
CREATE INDEX IF NOT EXISTS idx_rpc_cache_blocks ON rpc_cache(contract_address, from_block, to_block);
CREATE INDEX IF NOT EXISTS idx_batch_balances_fetched ON batch_balances(fetched_at);
