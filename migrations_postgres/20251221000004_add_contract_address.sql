-- Add contract_address column to events and storage_incentives_events tables
-- Created: 2025-12-21
-- Purpose: Track exact contract address that emitted each event for multi-version support
-- PostgreSQL version

-- ============================================================================
-- Step 1: Add contract_address columns
-- ============================================================================

-- Add contract_address to events table (PostageStamp and StampsRegistry events)
ALTER TABLE events ADD COLUMN IF NOT EXISTS contract_address TEXT;

-- Add contract_address to storage_incentives_events table (PriceOracle, StakeRegistry, Redistribution)
ALTER TABLE storage_incentives_events ADD COLUMN IF NOT EXISTS contract_address TEXT;

-- ============================================================================
-- Step 2: Create indexes for performance
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_events_contract_address ON events(contract_address);
CREATE INDEX IF NOT EXISTS idx_si_contract_address ON storage_incentives_events(contract_address);

-- ============================================================================
-- Step 3: Backfill contract_address using block number inference
-- ============================================================================

-- Backfill PostageStamp events
-- Uses deployment blocks to determine which contract version emitted each event
UPDATE events
SET contract_address = CASE
    -- Current version (v0.8.6) deployed at block 31305656
    WHEN block_number >= 31305656 THEN '0x45a1502382541cd610cc9068e88727426b696293'
    -- Previous version (Phase 4) deployed at block 25527076
    ELSE '0x30d155478ef27ab32a1d578be7b84bc5988af381'
END
WHERE contract_source = 'PostageStamp' AND contract_address IS NULL;

-- Backfill StampsRegistry events (only one version exists)
UPDATE events
SET contract_address = '0x5ebfbefb1e88391efb022d5d33302f50a46bf4f3'
WHERE contract_source = 'StampsRegistry' AND contract_address IS NULL;

-- Backfill Redistribution events (6 versions with most frequent redeployments)
UPDATE storage_incentives_events
SET contract_address = CASE
    -- v0.9.4 @ Block 41105199 (current)
    WHEN block_number >= 41105199 THEN '0x5069cdfb3d9e56d23b1caee83ce6109a7e4fd62d'
    -- v0.9.3 @ Block 40430261
    WHEN block_number >= 40430261 THEN '0x9f9a8da5a0db2611f9802ba1a0b99cc4a1c3b6a2'
    -- v0.9.2 @ Block 37339181
    WHEN block_number >= 37339181 THEN '0x69c62cacd68c2cbbf3d0c7502ef556db3ac7889b'
    -- v0.9.1 @ Block 35961755
    WHEN block_number >= 35961755 THEN '0xfff73fd14537277b3f3807e1ab0f85e17c0abea5'
    -- v0.8.6 @ Block 34159666
    WHEN block_number >= 34159666 THEN '0xd9dfe7b0ddc7cca41304fe9507ed823fad3bdbab'
    -- Phase 4 @ Block 31305409 (earliest)
    ELSE '0x1f9a1fde5c6350e949c5e4aa163b4c97011199b4'
END
WHERE contract_source = 'Redistribution' AND contract_address IS NULL;

-- Backfill StakeRegistry events (4 versions)
UPDATE storage_incentives_events
SET contract_address = CASE
    -- v0.9.3 @ Block 40430237 (current)
    WHEN block_number >= 40430237 THEN '0xda2a16ee889e7f04980a8d597b48c8d51b9518f4'
    -- v0.9.2 @ Block 37339175
    WHEN block_number >= 37339175 THEN '0x445b848e16730988f871c4a09ab74526d27c2ce8'
    -- v0.9.1 @ Block 35961749
    WHEN block_number >= 35961749 THEN '0xbe212ea1a4978a64e8f7636ae18305c38ca092bd'
    -- v0.4.0 @ Block 25527075 (earliest)
    ELSE '0x781c6d1f0eae6f1da1f604c6cdccdb8b76428ba7'
END
WHERE contract_source = 'StakeRegistry' AND contract_address IS NULL;

-- Backfill PriceOracle events (3 versions)
UPDATE storage_incentives_events
SET contract_address = CASE
    -- v0.9.2 @ Block 37339168 (current)
    WHEN block_number >= 37339168 THEN '0x47eef336e7fe5bed98499a4696bce8f28c1b0a8b'
    -- v0.9.1 @ Block 31305665
    WHEN block_number >= 31305665 THEN '0x86de783bf23bc13daef5a55ec531c198da8f10cf'
    -- Phase 4 @ Block 25527079 (earliest)
    ELSE '0x344a2cc7304b32a87efdc5407cd4bec7cf98f035'
END
WHERE contract_source = 'PriceOracle' AND contract_address IS NULL;

-- ============================================================================
-- Step 4: Verification queries (commented out - run manually if needed)
-- ============================================================================

-- Verify all events have contract_address
-- SELECT
--     contract_source,
--     COUNT(*) as total_events,
--     COUNT(contract_address) as events_with_address,
--     COUNT(*) - COUNT(contract_address) as missing_address
-- FROM events
-- GROUP BY contract_source;

-- SELECT
--     contract_source,
--     COUNT(*) as total_events,
--     COUNT(contract_address) as events_with_address,
--     COUNT(*) - COUNT(contract_address) as missing_address
-- FROM storage_incentives_events
-- GROUP BY contract_source;

-- Verify address distribution
-- SELECT contract_source, contract_address, COUNT(*) as event_count
-- FROM events
-- GROUP BY contract_source, contract_address
-- ORDER BY contract_source, event_count DESC;

-- SELECT contract_source, contract_address, COUNT(*) as event_count
-- FROM storage_incentives_events
-- GROUP BY contract_source, contract_address
-- ORDER BY contract_source, event_count DESC;
