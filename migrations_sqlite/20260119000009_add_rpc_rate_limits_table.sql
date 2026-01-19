-- Add table to persist discovered RPC rate limits across sessions
--
-- This table enables adaptive rate limiting to persist learned rate limits
-- across sessions, avoiding re-discovery on every restart.
--
-- Example queries:
--   SELECT * FROM rpc_rate_limits WHERE is_active = 1 ORDER BY last_updated_at DESC;
--   SELECT rpc_url, discovered_rate_limit, measured_rps, success_rate FROM rpc_rate_limits;
CREATE TABLE IF NOT EXISTS rpc_rate_limits (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Endpoint identification
    rpc_url TEXT NOT NULL UNIQUE,

    -- Rate limit information
    discovered_rate_limit REAL NOT NULL,
    rate_limit_strategy TEXT NOT NULL, -- 'manual', 'adaptive', 'aggressive'

    -- Statistics
    total_requests INTEGER NOT NULL DEFAULT 0,
    rate_limit_errors INTEGER NOT NULL DEFAULT 0,
    success_rate REAL NOT NULL DEFAULT 1.0,

    -- Measurements
    measured_rps REAL,
    avg_response_time_ms REAL,

    -- Timestamps (stored as TEXT in ISO 8601 format)
    first_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_rate_limit_at TEXT,

    -- Status
    is_active INTEGER NOT NULL DEFAULT 1, -- SQLite uses INTEGER for boolean
    consecutive_successes INTEGER NOT NULL DEFAULT 0,
    consecutive_failures INTEGER NOT NULL DEFAULT 0
);

-- Index for fast lookups by URL
CREATE INDEX IF NOT EXISTS idx_rpc_rate_limits_url ON rpc_rate_limits(rpc_url);

-- Index for active endpoints
CREATE INDEX IF NOT EXISTS idx_rpc_rate_limits_active ON rpc_rate_limits(is_active);

-- Index for recent updates
CREATE INDEX IF NOT EXISTS idx_rpc_rate_limits_updated ON rpc_rate_limits(last_updated_at DESC);
