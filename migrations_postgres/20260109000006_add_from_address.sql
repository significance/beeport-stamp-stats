-- Add from_address column to track transaction sender (PostgreSQL)
-- Created: 2026-01-09

-- Add from_address column to events table
ALTER TABLE events ADD COLUMN IF NOT EXISTS from_address TEXT;

-- Add index for from_address queries
CREATE INDEX IF NOT EXISTS idx_events_from_address ON events(from_address);
