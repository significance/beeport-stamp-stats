-- Add table to persist discovered RPC rate limits across sessions
--
-- This table enables adaptive rate limiting to persist learned rate limits
-- across sessions, avoiding re-discovery on every restart.
--
-- Example queries:
--   SELECT * FROM rpc_rate_limits WHERE is_active = TRUE ORDER BY last_updated_at DESC;
--   SELECT rpc_url, discovered_rate_limit, measured_rps, success_rate FROM rpc_rate_limits;
CREATE TABLE IF NOT EXISTS rpc_rate_limits (
    id SERIAL PRIMARY KEY,

    -- Endpoint identification
    rpc_url TEXT NOT NULL UNIQUE,

    -- Rate limit information
    discovered_rate_limit DOUBLE PRECISION NOT NULL,
    rate_limit_strategy TEXT NOT NULL, -- 'manual', 'adaptive', 'aggressive'

    -- Statistics
    total_requests BIGINT NOT NULL DEFAULT 0,
    rate_limit_errors BIGINT NOT NULL DEFAULT 0,
    success_rate DOUBLE PRECISION NOT NULL DEFAULT 1.0,

    -- Measurements
    measured_rps DOUBLE PRECISION,
    avg_response_time_ms DOUBLE PRECISION,

    -- Timestamps
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_rate_limit_at TIMESTAMPTZ,

    -- Status
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    consecutive_successes INTEGER NOT NULL DEFAULT 0,
    consecutive_failures INTEGER NOT NULL DEFAULT 0
);

-- Index for fast lookups by URL
CREATE INDEX IF NOT EXISTS idx_rpc_rate_limits_url ON rpc_rate_limits(rpc_url);

-- Index for active endpoints
CREATE INDEX IF NOT EXISTS idx_rpc_rate_limits_active ON rpc_rate_limits(is_active);

-- Index for recent updates
CREATE INDEX IF NOT EXISTS idx_rpc_rate_limits_updated ON rpc_rate_limits(last_updated_at DESC);

-- Comments for documentation
COMMENT ON TABLE rpc_rate_limits IS 'Persists discovered RPC rate limits across sessions for adaptive throttling';
COMMENT ON COLUMN rpc_rate_limits.discovered_rate_limit IS 'Current effective rate limit in requests per second';
COMMENT ON COLUMN rpc_rate_limits.rate_limit_strategy IS 'Strategy used for this endpoint: manual, adaptive, or aggressive';
COMMENT ON COLUMN rpc_rate_limits.measured_rps IS 'Most recently measured requests per second';
COMMENT ON COLUMN rpc_rate_limits.avg_response_time_ms IS 'Moving average of response time in milliseconds';
COMMENT ON COLUMN rpc_rate_limits.consecutive_successes IS 'Consecutive successful requests since last rate limit';
COMMENT ON COLUMN rpc_rate_limits.consecutive_failures IS 'Consecutive failed requests';
