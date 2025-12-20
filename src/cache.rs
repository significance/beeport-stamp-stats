use crate::error::Result;
use crate::events::{BatchInfo, EventData, EventType, StampEvent};
use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use std::path::Path;

#[derive(Clone)]
enum DatabasePool {
    Sqlite(sqlx::SqlitePool),
    Postgres(sqlx::PgPool),
}

#[derive(Clone)]
pub struct Cache {
    pool: DatabasePool,
}

impl Cache {
    /// Create a new cache instance and initialize the database
    /// Supports both SQLite (file path or sqlite://) and PostgreSQL (postgres://)
    pub async fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let path_str = db_path.as_ref().to_string_lossy();

        // Detect database type and connect with appropriate driver
        let pool = if path_str.starts_with("postgres://") || path_str.starts_with("postgresql://") {
            // PostgreSQL connection string
            tracing::info!("Connecting to PostgreSQL database");

            // Try to connect, and create database if it doesn't exist
            let pg_pool = match sqlx::PgPool::connect(&path_str).await {
                Ok(pool) => pool,
                Err(e) => {
                    let err_msg = e.to_string();
                    if err_msg.contains("does not exist") || err_msg.contains("database") && err_msg.contains("does not exist") {
                        tracing::info!("Database does not exist, creating it...");

                        // Extract database name from connection string
                        // Format: postgres://user:pass@host:port/database or postgres://user@host/database
                        let db_name = path_str
                            .split('/')
                            .next_back()
                            .and_then(|s| s.split('?').next())
                            .unwrap_or("beeport_stamps");

                        // Connect to default postgres database to create the target database
                        let base_url = path_str.rsplit_once('/').map(|x| x.0).unwrap_or(&path_str);
                        let postgres_url = format!("{base_url}/postgres");

                        tracing::debug!("Connecting to postgres database to create '{}'", db_name);
                        let admin_pool = sqlx::PgPool::connect(&postgres_url).await?;

                        // Create database (ignore error if it already exists)
                        let create_query = format!("CREATE DATABASE {db_name}");
                        match sqlx::query(&create_query).execute(&admin_pool).await {
                            Ok(_) => tracing::info!("Database '{}' created successfully", db_name),
                            Err(e) if e.to_string().contains("already exists") => {
                                tracing::debug!("Database '{}' already exists", db_name);
                            }
                            Err(e) => return Err(e.into()),
                        }

                        // Now connect to the newly created database
                        sqlx::PgPool::connect(&path_str).await?
                    } else {
                        return Err(e.into());
                    }
                }
            };

            DatabasePool::Postgres(pg_pool)
        } else {
            // SQLite (either with sqlite:// prefix or as file path)
            let db_url = if path_str.starts_with("sqlite://") {
                tracing::info!("Connecting to SQLite database");
                path_str.to_string()
            } else {
                tracing::info!("Connecting to SQLite database: {}", path_str);
                let path = db_path.as_ref();

                // Ensure parent directory exists for SQLite
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                format!("sqlite:{path_str}")
            };

            // Use SqliteConnectOptions to auto-create database file
            use sqlx::sqlite::SqliteConnectOptions;
            use std::str::FromStr;
            let options = SqliteConnectOptions::from_str(&db_url)?
                .create_if_missing(true);
            let sqlite_pool = sqlx::SqlitePool::connect_with(options).await?;
            DatabasePool::Sqlite(sqlite_pool)
        };

        let cache = Self { pool };
        cache.run_migrations().await?;

        Ok(cache)
    }

    /// Run database migrations
    async fn run_migrations(&self) -> Result<()> {
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                tracing::debug!("Running SQLite migrations from ./migrations");
                sqlx::migrate!("./migrations")
                    .run(pool)
                    .await?;
            }
            DatabasePool::Postgres(pool) => {
                tracing::debug!("Running PostgreSQL migrations from ./migrations_postgres");
                sqlx::migrate!("./migrations_postgres")
                    .run(pool)
                    .await?;
            }
        }
        Ok(())
    }


    /// Store events in the database
    pub async fn store_events(&self, events: &[StampEvent]) -> Result<()> {
        for event in events {
            let event_type = event.event_type.to_string();
            let data = serde_json::to_string(&event.data)?;
            let timestamp = event.block_timestamp.timestamp();

            match &self.pool {
                DatabasePool::Sqlite(pool) => {
                    sqlx::query(
                        r#"
                        INSERT OR REPLACE INTO events
                        (event_type, batch_id, block_number, block_timestamp, transaction_hash, log_index, contract_source, data)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                        "#,
                    )
                    .bind(&event_type)
                    .bind(&event.batch_id)
                    .bind(event.block_number as i64)
                    .bind(timestamp)
                    .bind(&event.transaction_hash)
                    .bind(event.log_index as i64)
                    .bind(&event.contract_source)
                    .bind(&data)
                    .execute(pool)
                    .await?;
                }
                DatabasePool::Postgres(pool) => {
                    sqlx::query(
                        r#"
                        INSERT INTO events
                        (event_type, batch_id, block_number, block_timestamp, transaction_hash, log_index, contract_source, data)
                        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                        ON CONFLICT (transaction_hash, log_index) DO UPDATE SET
                            event_type = EXCLUDED.event_type,
                            batch_id = EXCLUDED.batch_id,
                            block_number = EXCLUDED.block_number,
                            block_timestamp = EXCLUDED.block_timestamp,
                            contract_source = EXCLUDED.contract_source,
                            data = EXCLUDED.data
                        "#,
                    )
                    .bind(&event_type)
                    .bind(&event.batch_id)
                    .bind(event.block_number as i64)
                    .bind(timestamp)
                    .bind(&event.transaction_hash)
                    .bind(event.log_index as i64)
                    .bind(&event.contract_source)
                    .bind(&data)
                    .execute(pool)
                    .await?;
                }
            }
        }

        Ok(())
    }

    /// Store batch information in the database
    pub async fn store_batches(&self, batches: &[BatchInfo]) -> Result<()> {
        for batch in batches {
            let created_at = batch.created_at.timestamp();
            let immutable = if batch.immutable { 1 } else { 0 };

            // Use database-specific UPSERT syntax
            match &self.pool {
                DatabasePool::Sqlite(pool) => {
                    sqlx::query(
                        r#"
                        INSERT OR REPLACE INTO batches
                        (batch_id, owner, depth, bucket_depth, immutable, normalised_balance, created_at, block_number)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                        "#
                    )
                    .bind(&batch.batch_id)
                    .bind(&batch.owner)
                    .bind(batch.depth as i64)
                    .bind(batch.bucket_depth as i64)
                    .bind(immutable)
                    .bind(&batch.normalised_balance)
                    .bind(created_at)
                    .bind(batch.block_number as i64)
                    .execute(pool)
                    .await?;
                }
                DatabasePool::Postgres(pool) => {
                    sqlx::query(
                        r#"
                        INSERT INTO batches
                        (batch_id, owner, depth, bucket_depth, immutable, normalised_balance, created_at, block_number)
                        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                        ON CONFLICT (batch_id) DO UPDATE SET
                            owner = EXCLUDED.owner,
                            depth = EXCLUDED.depth,
                            bucket_depth = EXCLUDED.bucket_depth,
                            immutable = EXCLUDED.immutable,
                            normalised_balance = EXCLUDED.normalised_balance,
                            created_at = EXCLUDED.created_at,
                            block_number = EXCLUDED.block_number
                        "#
                    )
                    .bind(&batch.batch_id)
                    .bind(&batch.owner)
                    .bind(batch.depth as i64)
                    .bind(batch.bucket_depth as i64)
                    .bind(immutable)
                    .bind(&batch.normalised_balance)
                    .bind(created_at)
                    .bind(batch.block_number as i64)
                    .execute(pool)
                    .await?;
                }
            }
        }

        Ok(())
    }

    /// Get the last block number stored in the database
    pub async fn get_last_block(&self) -> Result<Option<u64>> {
        let max_block: Option<i64> = match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let row = sqlx::query("SELECT MAX(block_number) as max_block FROM events")
                    .fetch_one(pool)
                    .await?;
                row.get("max_block")
            }
            DatabasePool::Postgres(pool) => {
                let row = sqlx::query("SELECT MAX(block_number) as max_block FROM events")
                    .fetch_one(pool)
                    .await?;
                row.get("max_block")
            }
        };
        Ok(max_block.map(|b| b as u64))
    }

    /// Retrieve events from the last N months
    pub async fn get_events(&self, months: u32) -> Result<Vec<StampEvent>> {
        let cutoff = if months == 0 {
            0
        } else {
            let cutoff_date = Utc::now() - Duration::days((months * 30) as i64);
            cutoff_date.timestamp()
        };

        let events = match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let rows = sqlx::query(
                    r#"
                    SELECT event_type, batch_id, block_number, block_timestamp,
                           transaction_hash, log_index, contract_source, data
                    FROM events
                    WHERE block_timestamp >= ?
                    ORDER BY block_number ASC, log_index ASC
                    "#,
                )
                .bind(cutoff)
                .fetch_all(pool)
                .await?;

                let mut events = Vec::new();
                for row in rows {
                    let event_type_str: String = row.get("event_type");
                    let event_type = match event_type_str.as_str() {
                        "BatchCreated" => EventType::BatchCreated,
                        "BatchTopUp" => EventType::BatchTopUp,
                        "BatchDepthIncrease" => EventType::BatchDepthIncrease,
                        _ => continue,
                    };

                    let data_str: String = row.get("data");
                    let data: EventData = serde_json::from_str(&data_str)?;

                    let timestamp: i64 = row.get("block_timestamp");
                    let block_timestamp =
                        DateTime::from_timestamp(timestamp, 0).unwrap_or_else(Utc::now);

                    events.push(StampEvent {
                        event_type,
                        batch_id: row.get("batch_id"),
                        block_number: row.get::<i64, _>("block_number") as u64,
                        block_timestamp,
                        transaction_hash: row.get("transaction_hash"),
                        log_index: row.get::<i64, _>("log_index") as u64,
                        contract_source: row.get("contract_source"),
                        data,
                    });
                }
                events
            }
            DatabasePool::Postgres(pool) => {
                let rows = sqlx::query(
                    r#"
                    SELECT event_type, batch_id, block_number, block_timestamp,
                           transaction_hash, log_index, contract_source, data
                    FROM events
                    WHERE block_timestamp >= $1
                    ORDER BY block_number ASC, log_index ASC
                    "#,
                )
                .bind(cutoff)
                .fetch_all(pool)
                .await?;

                let mut events = Vec::new();
                for row in rows {
                    let event_type_str: String = row.get("event_type");
                    let event_type = match event_type_str.as_str() {
                        "BatchCreated" => EventType::BatchCreated,
                        "BatchTopUp" => EventType::BatchTopUp,
                        "BatchDepthIncrease" => EventType::BatchDepthIncrease,
                        _ => continue,
                    };

                    let data_str: String = row.get("data");
                    let data: EventData = serde_json::from_str(&data_str)?;

                    let timestamp: i64 = row.get("block_timestamp");
                    let block_timestamp =
                        DateTime::from_timestamp(timestamp, 0).unwrap_or_else(Utc::now);

                    events.push(StampEvent {
                        event_type,
                        batch_id: row.get("batch_id"),
                        block_number: row.get::<i64, _>("block_number") as u64,
                        block_timestamp,
                        transaction_hash: row.get("transaction_hash"),
                        log_index: row.get::<i64, _>("log_index") as u64,
                        contract_source: row.get("contract_source"),
                        data,
                    });
                }
                events
            }
        };

        Ok(events)
    }

    /// Retrieve batches from the last N months
    pub async fn get_batches(&self, months: u32) -> Result<Vec<BatchInfo>> {
        let cutoff = if months == 0 {
            0
        } else {
            let cutoff_date = Utc::now() - Duration::days((months * 30) as i64);
            cutoff_date.timestamp()
        };

        let batches = match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let rows = sqlx::query(
                    r#"
                    SELECT batch_id, owner, depth, bucket_depth, immutable,
                           normalised_balance, created_at, block_number
                    FROM batches
                    WHERE created_at >= ?
                    ORDER BY created_at ASC
                    "#,
                )
                .bind(cutoff)
                .fetch_all(pool)
                .await?;

                let mut batches = Vec::new();
                for row in rows {
                    let immutable: i64 = row.get("immutable");
                    let created_at: i64 = row.get("created_at");
                    let block_number: i64 = row.get("block_number");

                    batches.push(BatchInfo {
                        batch_id: row.get("batch_id"),
                        owner: row.get("owner"),
                        depth: row.get::<i64, _>("depth") as u8,
                        bucket_depth: row.get::<i64, _>("bucket_depth") as u8,
                        immutable: immutable != 0,
                        normalised_balance: row.get("normalised_balance"),
                        created_at: DateTime::from_timestamp(created_at, 0).unwrap_or_else(Utc::now),
                        block_number: block_number as u64,
                    });
                }
                batches
            }
            DatabasePool::Postgres(pool) => {
                let rows = sqlx::query(
                    r#"
                    SELECT batch_id, owner, depth, bucket_depth, immutable,
                           normalised_balance, created_at, block_number
                    FROM batches
                    WHERE created_at >= $1
                    ORDER BY created_at ASC
                    "#,
                )
                .bind(cutoff)
                .fetch_all(pool)
                .await?;

                let mut batches = Vec::new();
                for row in rows {
                    let immutable: i64 = row.get("immutable");
                    let created_at: i64 = row.get("created_at");
                    let block_number: i64 = row.get("block_number");

                    batches.push(BatchInfo {
                        batch_id: row.get("batch_id"),
                        owner: row.get("owner"),
                        depth: row.get::<i64, _>("depth") as u8,
                        bucket_depth: row.get::<i64, _>("bucket_depth") as u8,
                        immutable: immutable != 0,
                        normalised_balance: row.get("normalised_balance"),
                        created_at: DateTime::from_timestamp(created_at, 0).unwrap_or_else(Utc::now),
                        block_number: block_number as u64,
                    });
                }
                batches
            }
        };

        Ok(batches)
    }

    /// Get total number of events in the database
    #[allow(dead_code)]
    pub async fn count_events(&self) -> Result<i64> {
        let count: i64 = match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let row = sqlx::query("SELECT COUNT(*) as count FROM events")
                    .fetch_one(pool)
                    .await?;
                row.get("count")
            }
            DatabasePool::Postgres(pool) => {
                let row = sqlx::query("SELECT COUNT(*) as count FROM events")
                    .fetch_one(pool)
                    .await?;
                row.get("count")
            }
        };
        Ok(count)
    }

    /// Get total number of batches in the database
    #[allow(dead_code)]
    pub async fn count_batches(&self) -> Result<i64> {
        let count: i64 = match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let row = sqlx::query("SELECT COUNT(*) as count FROM batches")
                    .fetch_one(pool)
                    .await?;
                row.get("count")
            }
            DatabasePool::Postgres(pool) => {
                let row = sqlx::query("SELECT COUNT(*) as count FROM batches")
                    .fetch_one(pool)
                    .await?;
                row.get("count")
            }
        };
        Ok(count)
    }

    /// Check if an RPC chunk has been cached
    pub async fn is_chunk_cached(&self, chunk_hash: &str) -> Result<bool> {
        let count: i64 = match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let row = sqlx::query("SELECT COUNT(*) as count FROM rpc_cache WHERE chunk_hash = ?")
                    .bind(chunk_hash)
                    .fetch_one(pool)
                    .await?;
                row.get("count")
            }
            DatabasePool::Postgres(pool) => {
                let row = sqlx::query("SELECT COUNT(*) as count FROM rpc_cache WHERE chunk_hash = $1")
                    .bind(chunk_hash)
                    .fetch_one(pool)
                    .await?;
                row.get("count")
            }
        };

        Ok(count > 0)
    }

    /// Store RPC chunk metadata in cache
    pub async fn cache_chunk(
        &self,
        chunk_hash: &str,
        contract_address: &str,
        from_block: u64,
        to_block: u64,
        event_count: usize,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp();

        // Use database-specific UPSERT syntax
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    INSERT OR REPLACE INTO rpc_cache
                    (chunk_hash, contract_address, from_block, to_block, processed_at, event_count)
                    VALUES (?, ?, ?, ?, ?, ?)
                    "#
                )
                .bind(chunk_hash)
                .bind(contract_address)
                .bind(from_block as i64)
                .bind(to_block as i64)
                .bind(now)
                .bind(event_count as i64)
                .execute(pool)
                .await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO rpc_cache
                    (chunk_hash, contract_address, from_block, to_block, processed_at, event_count)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    ON CONFLICT (chunk_hash) DO UPDATE SET
                        contract_address = EXCLUDED.contract_address,
                        from_block = EXCLUDED.from_block,
                        to_block = EXCLUDED.to_block,
                        processed_at = EXCLUDED.processed_at,
                        event_count = EXCLUDED.event_count
                    "#
                )
                .bind(chunk_hash)
                .bind(contract_address)
                .bind(from_block as i64)
                .bind(to_block as i64)
                .bind(now)
                .bind(event_count as i64)
                .execute(pool)
                .await?;
            }
        }

        Ok(())
    }

    /// Get statistics about RPC cache
    #[allow(dead_code)]
    pub async fn get_cache_stats(&self) -> Result<(i64, i64)> {
        let (chunk_count, total_events) = match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let row = sqlx::query(
                    "SELECT COUNT(*) as chunk_count, COALESCE(SUM(event_count), 0) as total_events FROM rpc_cache",
                )
                .fetch_one(pool)
                .await?;

                let chunk_count: i64 = row.get("chunk_count");
                let total_events: i64 = row.get("total_events");
                (chunk_count, total_events)
            }
            DatabasePool::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT COUNT(*) as chunk_count, COALESCE(SUM(event_count), 0) as total_events FROM rpc_cache",
                )
                .fetch_one(pool)
                .await?;

                let chunk_count: i64 = row.get("chunk_count");
                let total_events: i64 = row.get("total_events");
                (chunk_count, total_events)
            }
        };

        Ok((chunk_count, total_events))
    }

    /// Get cached batch balance if available and not too old
    pub async fn get_cached_balance(&self, batch_id: &str, current_block: u64, validity_blocks: u64) -> Result<Option<String>> {
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let row = sqlx::query(
                    "SELECT remaining_balance, fetched_block FROM batch_balances WHERE batch_id = ?",
                )
                .bind(batch_id)
                .fetch_optional(pool)
                .await?;

                if let Some(row) = row {
                    let fetched_block: i64 = row.get("fetched_block");
                    // Consider cache valid if fetched within the specified validity period
                    if current_block.saturating_sub(fetched_block as u64) < validity_blocks {
                        return Ok(Some(row.get("remaining_balance")));
                    }
                }

                Ok(None)
            }
            DatabasePool::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT remaining_balance, fetched_block FROM batch_balances WHERE batch_id = $1",
                )
                .bind(batch_id)
                .fetch_optional(pool)
                .await?;

                if let Some(row) = row {
                    let fetched_block: i64 = row.get("fetched_block");
                    // Consider cache valid if fetched within the specified validity period
                    if current_block.saturating_sub(fetched_block as u64) < validity_blocks {
                        return Ok(Some(row.get("remaining_balance")));
                    }
                }

                Ok(None)
            }
        }
    }

    /// Cache a batch balance
    pub async fn cache_balance(&self, batch_id: &str, balance: &str, current_block: u64) -> Result<()> {
        let now = chrono::Utc::now().timestamp();

        // Use database-specific UPSERT syntax
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    INSERT OR REPLACE INTO batch_balances
                    (batch_id, remaining_balance, fetched_at, fetched_block)
                    VALUES (?, ?, ?, ?)
                    "#
                )
                .bind(batch_id)
                .bind(balance)
                .bind(now)
                .bind(current_block as i64)
                .execute(pool)
                .await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO batch_balances
                    (batch_id, remaining_balance, fetched_at, fetched_block)
                    VALUES ($1, $2, $3, $4)
                    ON CONFLICT (batch_id) DO UPDATE SET
                        remaining_balance = EXCLUDED.remaining_balance,
                        fetched_at = EXCLUDED.fetched_at,
                        fetched_block = EXCLUDED.fetched_block
                    "#
                )
                .bind(batch_id)
                .bind(balance)
                .bind(now)
                .bind(current_block as i64)
                .execute(pool)
                .await?;
            }
        }

        Ok(())
    }

    /// Get the last cached price
    pub async fn get_cached_price(&self) -> Result<Option<u128>> {
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let row = sqlx::query(
                    "SELECT value FROM cache_metadata WHERE key = 'last_price'",
                )
                .fetch_optional(pool)
                .await?;

                if let Some(row) = row {
                    let value: String = row.get("value");
                    let price = value.parse::<u128>()
                        .map_err(|_| crate::error::StampError::Parse("Invalid cached price".to_string()))?;
                    Ok(Some(price))
                } else {
                    Ok(None)
                }
            }
            DatabasePool::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT value FROM cache_metadata WHERE key = 'last_price'",
                )
                .fetch_optional(pool)
                .await?;

                if let Some(row) = row {
                    let value: String = row.get("value");
                    let price = value.parse::<u128>()
                        .map_err(|_| crate::error::StampError::Parse("Invalid cached price".to_string()))?;
                    Ok(Some(price))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Cache the current price
    pub async fn cache_price(&self, price: u128) -> Result<()> {
        let now = chrono::Utc::now().timestamp();

        // Use database-specific UPSERT syntax
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    INSERT OR REPLACE INTO cache_metadata
                    (key, value, updated_at)
                    VALUES ('last_price', ?, ?)
                    "#
                )
                .bind(price.to_string())
                .bind(now)
                .execute(pool)
                .await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO cache_metadata
                    (key, value, updated_at)
                    VALUES ('last_price', $1, $2)
                    ON CONFLICT (key) DO UPDATE SET
                        value = EXCLUDED.value,
                        updated_at = EXCLUDED.updated_at
                    "#
                )
                .bind(price.to_string())
                .bind(now)
                .execute(pool)
                .await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    async fn create_test_cache() -> (Cache, NamedTempFile) {
        let temp_file = NamedTempFile::new().unwrap();
        let cache = Cache::new(temp_file.path()).await.unwrap();
        (cache, temp_file)
    }

    #[tokio::test]
    async fn test_cache_creation() {
        let (cache, _temp_file) = create_test_cache().await;
        assert_eq!(cache.count_events().await.unwrap(), 0);
        assert_eq!(cache.count_batches().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_store_and_retrieve_events() {
        let (cache, _temp_file) = create_test_cache().await;

        let events = vec![StampEvent {
            event_type: EventType::BatchCreated,
            batch_id: "0x1234".to_string(),
            block_number: 1000,
            block_timestamp: Utc::now(),
            transaction_hash: "0xabcd".to_string(),
            log_index: 0,
            contract_source: "PostageStamp".to_string(),
            data: EventData::BatchCreated {
                total_amount: "1000000000000000000".to_string(),
                normalised_balance: "500000000000000000".to_string(),
                owner: "0x5678".to_string(),
                depth: 20,
                bucket_depth: 16,
                immutable_flag: false,
                payer: None,
            },
        }];

        cache.store_events(&events).await.unwrap();
        assert_eq!(cache.count_events().await.unwrap(), 1);

        let retrieved = cache.get_events(0).await.unwrap();
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].batch_id, "0x1234");
    }

    #[tokio::test]
    async fn test_store_and_retrieve_batches() {
        let (cache, _temp_file) = create_test_cache().await;

        let batches = vec![BatchInfo {
            batch_id: "0x1234".to_string(),
            owner: "0x5678".to_string(),
            depth: 20,
            bucket_depth: 16,
            immutable: false,
            normalised_balance: "500000000000000000".to_string(),
            created_at: Utc::now(),
            block_number: 1000,
        }];

        cache.store_batches(&batches).await.unwrap();
        assert_eq!(cache.count_batches().await.unwrap(), 1);

        let retrieved = cache.get_batches(0).await.unwrap();
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].batch_id, "0x1234");
    }

    #[tokio::test]
    async fn test_get_last_block() {
        let (cache, _temp_file) = create_test_cache().await;

        assert_eq!(cache.get_last_block().await.unwrap(), None);

        let events = vec![
            StampEvent {
                event_type: EventType::BatchCreated,
                batch_id: "0x1234".to_string(),
                block_number: 1000,
                block_timestamp: Utc::now(),
                transaction_hash: "0xabcd1".to_string(),
                log_index: 0,
                contract_source: "PostageStamp".to_string(),
                data: EventData::BatchCreated {
                    total_amount: "1000000000000000000".to_string(),
                    normalised_balance: "500000000000000000".to_string(),
                    owner: "0x5678".to_string(),
                    depth: 20,
                    bucket_depth: 16,
                    immutable_flag: false,
                    payer: None,
                },
            },
            StampEvent {
                event_type: EventType::BatchTopUp,
                batch_id: "0x1234".to_string(),
                block_number: 2000,
                block_timestamp: Utc::now(),
                transaction_hash: "0xabcd2".to_string(),
                log_index: 0,
                contract_source: "PostageStamp".to_string(),
                data: EventData::BatchTopUp {
                    topup_amount: "100000000000000000".to_string(),
                    normalised_balance: "600000000000000000".to_string(),
                    payer: None,
                },
            },
        ];

        cache.store_events(&events).await.unwrap();
        assert_eq!(cache.get_last_block().await.unwrap(), Some(2000));
    }
}
