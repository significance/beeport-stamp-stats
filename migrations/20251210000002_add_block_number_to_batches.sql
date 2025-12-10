-- Add block_number column to batches table
-- This stores the block number when the batch was created
-- Created: 2025-12-10

-- Add the new column (defaults to 0 for existing rows)
ALTER TABLE batches ADD COLUMN block_number INTEGER NOT NULL DEFAULT 0;

-- Create index for block_number queries
CREATE INDEX IF NOT EXISTS idx_batches_block ON batches(block_number);

-- Note: Existing batches will have block_number = 0
-- They will be updated when the batch creation event is re-fetched
