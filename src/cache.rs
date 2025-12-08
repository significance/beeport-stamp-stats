use crate::error::Result;
use crate::events::{BatchInfo, EventData, EventType, StampEvent};
use chrono::{DateTime, Duration, Utc};
use sqlx::{Row, sqlite::SqlitePool};
use std::path::Path;

#[derive(Clone)]
pub struct Cache {
    pool: SqlitePool,
}

impl Cache {
    /// Create a new cache instance and initialize the database
    pub async fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let db_url = format!("sqlite:{}", db_path.as_ref().display());

        let pool = SqlitePool::connect(&db_url).await?;

        let cache = Self { pool };
        cache.init_schema().await?;

        Ok(cache)
    }

    /// Initialize database schema
    async fn init_schema(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                batch_id TEXT NOT NULL,
                block_number INTEGER NOT NULL,
                block_timestamp INTEGER NOT NULL,
                transaction_hash TEXT NOT NULL,
                log_index INTEGER NOT NULL,
                contract_source TEXT NOT NULL DEFAULT 'PostageStamp',
                data TEXT NOT NULL,
                UNIQUE(transaction_hash, log_index)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS rpc_cache (
                chunk_hash TEXT PRIMARY KEY,
                contract_address TEXT NOT NULL,
                from_block INTEGER NOT NULL,
                to_block INTEGER NOT NULL,
                processed_at INTEGER NOT NULL,
                event_count INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS batches (
                batch_id TEXT PRIMARY KEY,
                owner TEXT NOT NULL,
                depth INTEGER NOT NULL,
                bucket_depth INTEGER NOT NULL,
                immutable INTEGER NOT NULL,
                normalised_balance TEXT NOT NULL,
                created_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create indexes for better query performance
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_events_block ON events(block_number)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(block_timestamp)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_events_batch ON events(batch_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_events_contract ON events(contract_source)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_rpc_cache_blocks ON rpc_cache(contract_address, from_block, to_block)",
        )
        .execute(&self.pool)
        .await?;

        // Create batch balance cache table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS batch_balances (
                batch_id TEXT PRIMARY KEY,
                remaining_balance TEXT NOT NULL,
                fetched_at INTEGER NOT NULL,
                fetched_block INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_batch_balances_fetched ON batch_balances(fetched_at)",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Store events in the database
    pub async fn store_events(&self, events: &[StampEvent]) -> Result<()> {
        for event in events {
            let event_type = event.event_type.to_string();
            let data = serde_json::to_string(&event.data)?;
            let timestamp = event.block_timestamp.timestamp();

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
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Store batch information in the database
    pub async fn store_batches(&self, batches: &[BatchInfo]) -> Result<()> {
        for batch in batches {
            let created_at = batch.created_at.timestamp();
            let immutable = if batch.immutable { 1 } else { 0 };

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO batches
                (batch_id, owner, depth, bucket_depth, immutable, normalised_balance, created_at)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&batch.batch_id)
            .bind(&batch.owner)
            .bind(batch.depth as i64)
            .bind(batch.bucket_depth as i64)
            .bind(immutable)
            .bind(&batch.normalised_balance)
            .bind(created_at)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Get the last block number stored in the database
    pub async fn get_last_block(&self) -> Result<Option<u64>> {
        let row = sqlx::query("SELECT MAX(block_number) as max_block FROM events")
            .fetch_one(&self.pool)
            .await?;

        let max_block: Option<i64> = row.get("max_block");
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
        .fetch_all(&self.pool)
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
                DateTime::from_timestamp(timestamp, 0).unwrap_or_else(|| Utc::now());

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

        let rows = sqlx::query(
            r#"
            SELECT batch_id, owner, depth, bucket_depth, immutable,
                   normalised_balance, created_at
            FROM batches
            WHERE created_at >= ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await?;

        let mut batches = Vec::new();
        for row in rows {
            let immutable: i64 = row.get("immutable");
            let created_at: i64 = row.get("created_at");

            batches.push(BatchInfo {
                batch_id: row.get("batch_id"),
                owner: row.get("owner"),
                depth: row.get::<i64, _>("depth") as u8,
                bucket_depth: row.get::<i64, _>("bucket_depth") as u8,
                immutable: immutable != 0,
                normalised_balance: row.get("normalised_balance"),
                created_at: DateTime::from_timestamp(created_at, 0).unwrap_or_else(|| Utc::now()),
            });
        }

        Ok(batches)
    }

    /// Get total number of events in the database
    #[allow(dead_code)]
    pub async fn count_events(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM events")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get("count"))
    }

    /// Get total number of batches in the database
    #[allow(dead_code)]
    pub async fn count_batches(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM batches")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get("count"))
    }

    /// Check if an RPC chunk has been cached
    pub async fn is_chunk_cached(&self, chunk_hash: &str) -> Result<bool> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM rpc_cache WHERE chunk_hash = ?")
            .bind(chunk_hash)
            .fetch_one(&self.pool)
            .await?;

        let count: i64 = row.get("count");
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

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO rpc_cache
            (chunk_hash, contract_address, from_block, to_block, processed_at, event_count)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(chunk_hash)
        .bind(contract_address)
        .bind(from_block as i64)
        .bind(to_block as i64)
        .bind(now)
        .bind(event_count as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get statistics about RPC cache
    #[allow(dead_code)]
    pub async fn get_cache_stats(&self) -> Result<(i64, i64)> {
        let row = sqlx::query(
            "SELECT COUNT(*) as chunk_count, COALESCE(SUM(event_count), 0) as total_events FROM rpc_cache",
        )
        .fetch_one(&self.pool)
        .await?;

        let chunk_count: i64 = row.get("chunk_count");
        let total_events: i64 = row.get("total_events");

        Ok((chunk_count, total_events))
    }

    /// Get cached batch balance if available and not too old (within 100 blocks)
    pub async fn get_cached_balance(&self, batch_id: &str, current_block: u64) -> Result<Option<String>> {
        let row = sqlx::query(
            "SELECT remaining_balance, fetched_block FROM batch_balances WHERE batch_id = ?",
        )
        .bind(batch_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let fetched_block: i64 = row.get("fetched_block");
            // Consider cache valid if fetched within last 100 blocks (~8 minutes at 5s/block)
            if current_block.saturating_sub(fetched_block as u64) < 100 {
                return Ok(Some(row.get("remaining_balance")));
            }
        }

        Ok(None)
    }

    /// Cache a batch balance
    pub async fn cache_balance(&self, batch_id: &str, balance: &str, current_block: u64) -> Result<()> {
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO batch_balances
            (batch_id, remaining_balance, fetched_at, fetched_block)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(batch_id)
        .bind(balance)
        .bind(now)
        .bind(current_block as i64)
        .execute(&self.pool)
        .await?;

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
