# Database Migrations

This directory contains SQL migrations for the beeport-stamp-stats database.

## Overview

The project uses [sqlx](https://github.com/launchbadge/sqlx) for database migrations. Migrations are automatically applied when the application starts.

## Migration Files

Migrations are named with the following format:
```
<timestamp>_<description>.sql
```

For example:
- `20251210000001_initial_schema.sql` - Initial database schema
- `20251210000002_add_block_number_to_batches.sql` - Adds block_number column to batches table
- `20251220000003_add_storage_incentives_events.sql` - Adds storage_incentives_events table
- `20251221000004_add_contract_address.sql` - Adds contract_address column for multi-version support

## How Migrations Work

1. **Automatic Application**: Migrations run automatically when the Cache is initialized
2. **Tracking**: sqlx creates a `_sqlx_migrations` table to track which migrations have been applied
3. **Idempotent**: Migrations are only applied once, even if the application restarts
4. **Order**: Migrations are applied in lexicographical order (by filename)

## Creating New Migrations

To create a new migration:

1. Create a new file in this directory with format: `YYYYMMDDHHMMSS_description.sql`
2. Write your SQL migration (use `IF NOT EXISTS` for safety where possible)
3. The migration will be applied automatically on next application start

### Example Migration

```sql
-- Add new_column to some_table
-- Created: 2025-12-10

ALTER TABLE some_table ADD COLUMN new_column TEXT;
CREATE INDEX IF NOT EXISTS idx_some_table_new_column ON some_table(new_column);
```

## Migration Best Practices

1. **Use timestamps**: Prefix migrations with a timestamp to ensure ordering
2. **Be descriptive**: Use clear, descriptive names for migration files
3. **Add comments**: Include comments explaining what the migration does
4. **Test locally**: Test migrations on a copy of production data before deploying
5. **Handle defaults**: When adding NOT NULL columns, provide DEFAULT values
6. **Create indexes**: Create indexes for frequently queried columns
7. **No data loss**: Ensure migrations preserve existing data

## Handling Existing Databases

When a migration adds a new column with `NOT NULL`, existing rows will get the DEFAULT value:

```sql
ALTER TABLE batches ADD COLUMN block_number INTEGER NOT NULL DEFAULT 0;
```

Existing batches will have `block_number = 0`. These can be updated by re-fetching the BatchCreated events from the blockchain.

## Verifying Migrations

To check which migrations have been applied:

```bash
sqlite3 stamp-cache.db "SELECT version, description, installed_on FROM _sqlx_migrations"
```

To view the current schema:

```bash
sqlite3 stamp-cache.db ".schema"
```

## Troubleshooting

If migrations fail:

1. Check the error message in the application logs
2. Verify the SQL syntax in the migration file
3. Ensure the database file has proper permissions
4. Check that previous migrations completed successfully

To reset and reapply migrations (⚠️ **WARNING: This deletes all data**):

```bash
rm stamp-cache.db
# Run any command to recreate the database with migrations
./target/release/beeport-stamp-stats summary
```
