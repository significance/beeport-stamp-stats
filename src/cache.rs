use crate::error::Result;
use crate::events::{
    BatchInfo, ChequebookDeployment, EventData, EventType, PaymentChannelEvent,
    PaymentChannelEventData, StampEvent, StorageIncentivesEvent,
};
use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use std::path::Path;

/// Format large numbers with thousand separators
fn format_number(n: u128) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let len = s.len();

    for (i, c) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }

    result
}

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
            let contract_address = event.contract_address.as_ref().map(|addr| addr.as_str());
            let batch_id = event.batch_id.as_deref();

            // Extract event-specific data
            let (pot_recipient, pot_total_amount, price, copy_index, copy_batch_id) = match &event.data {
                EventData::PotWithdrawn { recipient, total_amount } => {
                    (Some(recipient.as_str()), Some(total_amount.as_str()), None, None, None)
                }
                EventData::PriceUpdate { price } => {
                    (None, None, Some(price.as_str()), None, None)
                }
                EventData::CopyBatchFailed { index, batch_id } => {
                    (None, None, None, Some(index.as_str()), Some(batch_id.as_str()))
                }
                _ => (None, None, None, None, None),
            };

            match &self.pool {
                DatabasePool::Sqlite(pool) => {
                    sqlx::query(
                        r#"
                        INSERT OR REPLACE INTO events
                        (event_type, batch_id, block_number, block_timestamp, transaction_hash, log_index, contract_source, contract_address, from_address, data, pot_recipient, pot_total_amount, price, copy_index, copy_batch_id)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                        "#,
                    )
                    .bind(&event_type)
                    .bind(batch_id)
                    .bind(event.block_number as i64)
                    .bind(timestamp)
                    .bind(&event.transaction_hash)
                    .bind(event.log_index as i64)
                    .bind(&event.contract_source)
                    .bind(contract_address)
                    .bind(event.from_address.as_deref())
                    .bind(&data)
                    .bind(pot_recipient)
                    .bind(pot_total_amount)
                    .bind(price)
                    .bind(copy_index)
                    .bind(copy_batch_id)
                    .execute(pool)
                    .await?;
                }
                DatabasePool::Postgres(pool) => {
                    sqlx::query(
                        r#"
                        INSERT INTO events
                        (event_type, batch_id, block_number, block_timestamp, transaction_hash, log_index, contract_source, contract_address, from_address, data, pot_recipient, pot_total_amount, price, copy_index, copy_batch_id)
                        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                        ON CONFLICT (transaction_hash, log_index) DO UPDATE SET
                            event_type = EXCLUDED.event_type,
                            batch_id = EXCLUDED.batch_id,
                            block_number = EXCLUDED.block_number,
                            block_timestamp = EXCLUDED.block_timestamp,
                            contract_source = EXCLUDED.contract_source,
                            contract_address = EXCLUDED.contract_address,
                            from_address = EXCLUDED.from_address,
                            data = EXCLUDED.data,
                            pot_recipient = EXCLUDED.pot_recipient,
                            pot_total_amount = EXCLUDED.pot_total_amount,
                            price = EXCLUDED.price,
                            copy_index = EXCLUDED.copy_index,
                            copy_batch_id = EXCLUDED.copy_batch_id
                        "#,
                    )
                    .bind(&event_type)
                    .bind(batch_id)
                    .bind(event.block_number as i64)
                    .bind(timestamp)
                    .bind(&event.transaction_hash)
                    .bind(event.log_index as i64)
                    .bind(&event.contract_source)
                    .bind(contract_address)
                    .bind(event.from_address.as_deref())
                    .bind(&data)
                    .bind(pot_recipient)
                    .bind(pot_total_amount)
                    .bind(price)
                    .bind(copy_index)
                    .bind(copy_batch_id)
                    .execute(pool)
                    .await?;
                }
            }
        }

        Ok(())
    }

    /// Store storage incentives events in the database
    /// Handles PriceOracle, StakeRegistry, and Redistribution events
    pub async fn store_storage_incentives_events(&self, events: &[StorageIncentivesEvent]) -> Result<()> {
        for event in events {
            let timestamp = event.block_timestamp.timestamp();
            let contract_address = event.contract_address.as_ref().map(|addr| addr.as_str());

            match &self.pool {
                DatabasePool::Sqlite(pool) => {
                    sqlx::query(
                        r#"
                        INSERT OR REPLACE INTO storage_incentives_events
                        (block_number, block_timestamp, transaction_hash, log_index, contract_source, contract_address, event_type,
                         round_number, phase, owner_address, overlay,
                         price, committed_stake, potential_stake, height, slash_amount, freeze_time, withdraw_amount,
                         stake, stake_density, reserve_commitment, depth,
                         anchor, truth_hash, truth_depth,
                         winner_overlay, winner_owner, winner_depth, winner_stake, winner_stake_density, winner_hash,
                         commit_count, reveal_count, chunk_count, redundancy_count,
                         chunk_index_in_rc, chunk_address)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                        "#,
                    )
                    .bind(event.block_number as i64)
                    .bind(timestamp)
                    .bind(&event.transaction_hash)
                    .bind(event.log_index as i64)
                    .bind(&event.contract_source)
                    .bind(contract_address)
                    .bind(&event.event_type)
                    .bind(event.round_number.map(|v| v as i64))
                    .bind(&event.phase)
                    .bind(&event.owner_address)
                    .bind(&event.overlay)
                    .bind(&event.price)
                    .bind(&event.committed_stake)
                    .bind(&event.potential_stake)
                    .bind(event.height.map(|v| v as i64))
                    .bind(&event.slash_amount)
                    .bind(&event.freeze_time)
                    .bind(&event.withdraw_amount)
                    .bind(&event.stake)
                    .bind(&event.stake_density)
                    .bind(&event.reserve_commitment)
                    .bind(event.depth.map(|v| v as i64))
                    .bind(&event.anchor)
                    .bind(&event.truth_hash)
                    .bind(event.truth_depth.map(|v| v as i64))
                    .bind(&event.winner_overlay)
                    .bind(&event.winner_owner)
                    .bind(event.winner_depth.map(|v| v as i64))
                    .bind(&event.winner_stake)
                    .bind(&event.winner_stake_density)
                    .bind(&event.winner_hash)
                    .bind(event.commit_count.map(|v| v as i64))
                    .bind(event.reveal_count.map(|v| v as i64))
                    .bind(event.chunk_count.map(|v| v as i64))
                    .bind(event.redundancy_count.map(|v| v as i64))
                    .bind(event.chunk_index_in_rc.map(|v| v as i64))
                    .bind(&event.chunk_address)
                    .execute(pool)
                    .await?;
                }
                DatabasePool::Postgres(pool) => {
                    sqlx::query(
                        r#"
                        INSERT INTO storage_incentives_events
                        (block_number, block_timestamp, transaction_hash, log_index, contract_source, contract_address, event_type,
                         round_number, phase, owner_address, overlay,
                         price, committed_stake, potential_stake, height, slash_amount, freeze_time, withdraw_amount,
                         stake, stake_density, reserve_commitment, depth,
                         anchor, truth_hash, truth_depth,
                         winner_overlay, winner_owner, winner_depth, winner_stake, winner_stake_density, winner_hash,
                         commit_count, reveal_count, chunk_count, redundancy_count,
                         chunk_index_in_rc, chunk_address)
                        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32, $33, $34, $35, $36, $37)
                        ON CONFLICT (transaction_hash, log_index) DO UPDATE SET
                            block_number = EXCLUDED.block_number,
                            block_timestamp = EXCLUDED.block_timestamp,
                            contract_source = EXCLUDED.contract_source,
                            contract_address = EXCLUDED.contract_address,
                            event_type = EXCLUDED.event_type,
                            round_number = EXCLUDED.round_number,
                            phase = EXCLUDED.phase,
                            owner_address = EXCLUDED.owner_address,
                            overlay = EXCLUDED.overlay,
                            price = EXCLUDED.price,
                            committed_stake = EXCLUDED.committed_stake,
                            potential_stake = EXCLUDED.potential_stake,
                            height = EXCLUDED.height,
                            slash_amount = EXCLUDED.slash_amount,
                            freeze_time = EXCLUDED.freeze_time,
                            withdraw_amount = EXCLUDED.withdraw_amount,
                            stake = EXCLUDED.stake,
                            stake_density = EXCLUDED.stake_density,
                            reserve_commitment = EXCLUDED.reserve_commitment,
                            depth = EXCLUDED.depth,
                            anchor = EXCLUDED.anchor,
                            truth_hash = EXCLUDED.truth_hash,
                            truth_depth = EXCLUDED.truth_depth,
                            winner_overlay = EXCLUDED.winner_overlay,
                            winner_owner = EXCLUDED.winner_owner,
                            winner_depth = EXCLUDED.winner_depth,
                            winner_stake = EXCLUDED.winner_stake,
                            winner_stake_density = EXCLUDED.winner_stake_density,
                            winner_hash = EXCLUDED.winner_hash,
                            commit_count = EXCLUDED.commit_count,
                            reveal_count = EXCLUDED.reveal_count,
                            chunk_count = EXCLUDED.chunk_count,
                            redundancy_count = EXCLUDED.redundancy_count,
                            chunk_index_in_rc = EXCLUDED.chunk_index_in_rc,
                            chunk_address = EXCLUDED.chunk_address
                        "#,
                    )
                    .bind(event.block_number as i64)
                    .bind(timestamp)
                    .bind(&event.transaction_hash)
                    .bind(event.log_index as i64)
                    .bind(&event.contract_source)
                    .bind(contract_address)
                    .bind(&event.event_type)
                    .bind(event.round_number.map(|v| v as i64))
                    .bind(&event.phase)
                    .bind(&event.owner_address)
                    .bind(&event.overlay)
                    .bind(&event.price)
                    .bind(&event.committed_stake)
                    .bind(&event.potential_stake)
                    .bind(event.height.map(|v| v as i64))
                    .bind(&event.slash_amount)
                    .bind(&event.freeze_time)
                    .bind(&event.withdraw_amount)
                    .bind(&event.stake)
                    .bind(&event.stake_density)
                    .bind(&event.reserve_commitment)
                    .bind(event.depth.map(|v| v as i64))
                    .bind(&event.anchor)
                    .bind(&event.truth_hash)
                    .bind(event.truth_depth.map(|v| v as i64))
                    .bind(&event.winner_overlay)
                    .bind(&event.winner_owner)
                    .bind(event.winner_depth.map(|v| v as i64))
                    .bind(&event.winner_stake)
                    .bind(&event.winner_stake_density)
                    .bind(&event.winner_hash)
                    .bind(event.commit_count.map(|v| v as i64))
                    .bind(event.reveal_count.map(|v| v as i64))
                    .bind(event.chunk_count.map(|v| v as i64))
                    .bind(event.redundancy_count.map(|v| v as i64))
                    .bind(event.chunk_index_in_rc.map(|v| v as i64))
                    .bind(&event.chunk_address)
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
                        (batch_id, owner, payer, contract_source, depth, bucket_depth, immutable, normalised_balance, created_at, block_number)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                        "#
                    )
                    .bind(&batch.batch_id)
                    .bind(&batch.owner)
                    .bind(&batch.payer)
                    .bind(&batch.contract_source)
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
                        (batch_id, owner, payer, contract_source, depth, bucket_depth, immutable, normalised_balance, created_at, block_number)
                        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                        ON CONFLICT (batch_id) DO UPDATE SET
                            owner = EXCLUDED.owner,
                            payer = EXCLUDED.payer,
                            contract_source = EXCLUDED.contract_source,
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
                    .bind(&batch.payer)
                    .bind(&batch.contract_source)
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
                           transaction_hash, log_index, contract_source, from_address, data
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
                        contract_address: None, // Will be populated from database after migration
                        from_address: row.get("from_address"),
                        data,
                    });
                }
                events
            }
            DatabasePool::Postgres(pool) => {
                let rows = sqlx::query(
                    r#"
                    SELECT event_type, batch_id, block_number, block_timestamp,
                           transaction_hash, log_index, contract_source, from_address, data
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
                        contract_address: None, // Will be populated from database after migration
                        from_address: row.get("from_address"),
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
                    SELECT batch_id, owner, payer, contract_source, depth, bucket_depth, immutable,
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
                        payer: row.get("payer"),
                        contract_source: row.get("contract_source"),
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
                    SELECT batch_id, owner, payer, contract_source, depth, bucket_depth, immutable,
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
                        payer: row.get("payer"),
                        contract_source: row.get("contract_source"),
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

    /// Get block timestamp from cached event data
    ///
    /// Checks both events and storage_incentives_events tables for any event with this block number.
    /// Returns the timestamp if found, None if the block has never been fetched.
    pub async fn get_block_timestamp(&self, block_number: u64) -> Result<Option<i64>> {
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                // Try events table first
                let row = sqlx::query(
                    "SELECT block_timestamp FROM events WHERE block_number = ? LIMIT 1"
                )
                .bind(block_number as i64)
                .fetch_optional(pool)
                .await?;

                if let Some(row) = row {
                    return Ok(Some(row.get("block_timestamp")));
                }

                // Try storage_incentives_events table
                let row = sqlx::query(
                    "SELECT block_timestamp FROM storage_incentives_events WHERE block_number = ? LIMIT 1"
                )
                .bind(block_number as i64)
                .fetch_optional(pool)
                .await?;

                if let Some(row) = row {
                    return Ok(Some(row.get("block_timestamp")));
                }

                Ok(None)
            }
            DatabasePool::Postgres(pool) => {
                // Try events table first
                let row = sqlx::query(
                    "SELECT block_timestamp FROM events WHERE block_number = $1 LIMIT 1"
                )
                .bind(block_number as i64)
                .fetch_optional(pool)
                .await?;

                if let Some(row) = row {
                    return Ok(Some(row.get("block_timestamp")));
                }

                // Try storage_incentives_events table
                let row = sqlx::query(
                    "SELECT block_timestamp FROM storage_incentives_events WHERE block_number = $1 LIMIT 1"
                )
                .bind(block_number as i64)
                .fetch_optional(pool)
                .await?;

                if let Some(row) = row {
                    return Ok(Some(row.get("block_timestamp")));
                }

                Ok(None)
            }
        }
    }

    /// Get address summary showing unique addresses and their activity
    #[allow(dead_code)]
    pub async fn get_address_summary(
        &self,
        min_stamps: u32,
    ) -> Result<Vec<crate::commands::address_summary::AddressSummary>> {
        use crate::commands::address_summary::AddressSummary;

        let query = format!(
            r#"
            WITH address_roles AS (
                SELECT
                    address,
                    SUM(CASE WHEN role = 'owner' THEN 1 ELSE 0 END) > 0 as is_owner,
                    SUM(CASE WHEN role = 'payer' THEN 1 ELSE 0 END) > 0 as is_payer,
                    SUM(CASE WHEN role = 'sender' THEN 1 ELSE 0 END) > 0 as is_sender,
                    COUNT(*) as stamp_count,
                    MIN(block_timestamp) as first_seen,
                    MAX(block_timestamp) as last_seen
                FROM (
                    SELECT
                        COALESCE(json_extract(data, '$.BatchCreated.owner'), json_extract(data, '$.BatchTopUp.owner')) as address,
                        'owner' as role,
                        block_timestamp
                    FROM events
                    WHERE event_type IN ('BatchCreated', 'BatchTopUp')

                    UNION ALL

                    SELECT
                        COALESCE(json_extract(data, '$.BatchCreated.payer'), json_extract(data, '$.BatchTopUp.payer')) as address,
                        'payer' as role,
                        block_timestamp
                    FROM events
                    WHERE event_type IN ('BatchCreated', 'BatchTopUp')
                      AND COALESCE(json_extract(data, '$.BatchCreated.payer'), json_extract(data, '$.BatchTopUp.payer')) IS NOT NULL

                    UNION ALL

                    SELECT
                        from_address as address,
                        'sender' as role,
                        block_timestamp
                    FROM events
                    WHERE from_address IS NOT NULL
                ) all_addresses
                WHERE address IS NOT NULL
                GROUP BY address
                HAVING stamp_count >= {min_stamps}
            )
            SELECT
                address,
                CASE
                    WHEN is_owner AND is_payer AND is_sender THEN 'Owner+Payer+Sender'
                    WHEN is_owner AND is_sender THEN 'Owner+Sender'
                    WHEN is_payer AND is_sender THEN 'Payer+Sender'
                    WHEN is_owner AND is_payer THEN 'Owner+Payer'
                    WHEN is_owner THEN 'Owner'
                    WHEN is_payer THEN 'Payer'
                    WHEN is_sender THEN 'Sender'
                    ELSE 'Unknown'
                END as role,
                stamp_count,
                datetime(first_seen, 'unixepoch') as first_seen,
                datetime(last_seen, 'unixepoch') as last_seen,
                is_owner,
                is_payer,
                is_sender
            FROM address_roles
            ORDER BY stamp_count DESC, address
            "#
        );

        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let rows = sqlx::query(&query).fetch_all(pool).await?;

                let mut summaries = Vec::new();
                for row in rows {
                    summaries.push(AddressSummary {
                        address: row.get("address"),
                        role: row.get("role"),
                        stamp_count: row.get("stamp_count"),
                        total_capacity: "N/A".to_string(), // Old method doesn't calculate capacity
                        first_seen: row.get("first_seen"),
                        last_seen: row.get("last_seen"),
                        is_owner: row.get::<i64, _>("is_owner") != 0,
                        is_payer: row.get::<i64, _>("is_payer") != 0,
                        is_sender: row.get::<i64, _>("is_sender") != 0,
                    });
                }

                Ok(summaries)
            }
            DatabasePool::Postgres(pool) => {
                let query = format!(
                    r#"
                    WITH address_roles AS (
                        SELECT
                            address,
                            SUM(CASE WHEN role = 'owner' THEN 1 ELSE 0 END) > 0 as is_owner,
                            SUM(CASE WHEN role = 'payer' THEN 1 ELSE 0 END) > 0 as is_payer,
                            SUM(CASE WHEN role = 'sender' THEN 1 ELSE 0 END) > 0 as is_sender,
                            COUNT(*) as stamp_count,
                            MIN(block_timestamp) as first_seen,
                            MAX(block_timestamp) as last_seen
                        FROM (
                            SELECT
                                data::jsonb->>'owner' as address,
                                'owner' as role,
                                block_timestamp
                            FROM events
                            WHERE event_type IN ('BatchCreated', 'BatchTopUp')

                            UNION ALL

                            SELECT
                                data::jsonb->>'payer' as address,
                                'payer' as role,
                                block_timestamp
                            FROM events
                            WHERE event_type IN ('BatchCreated', 'BatchTopUp')
                              AND data::jsonb->>'payer' IS NOT NULL

                            UNION ALL

                            SELECT
                                from_address as address,
                                'sender' as role,
                                block_timestamp
                            FROM events
                            WHERE from_address IS NOT NULL
                        ) all_addresses
                        WHERE address IS NOT NULL
                        GROUP BY address
                        HAVING COUNT(*) >= {min_stamps}
                    )
                    SELECT
                        address,
                        CASE
                            WHEN is_owner AND is_payer AND is_sender THEN 'Owner+Payer+Sender'
                            WHEN is_owner AND is_sender THEN 'Owner+Sender'
                            WHEN is_payer AND is_sender THEN 'Payer+Sender'
                            WHEN is_owner AND is_payer THEN 'Owner+Payer'
                            WHEN is_owner THEN 'Owner'
                            WHEN is_payer THEN 'Payer'
                            WHEN is_sender THEN 'Sender'
                            ELSE 'Unknown'
                        END as role,
                        stamp_count,
                        to_char(to_timestamp(first_seen), 'YYYY-MM-DD HH24:MI:SS') as first_seen,
                        to_char(to_timestamp(last_seen), 'YYYY-MM-DD HH24:MI:SS') as last_seen,
                        is_owner,
                        is_payer,
                        is_sender
                    FROM address_roles
                    ORDER BY stamp_count DESC, address
                    "#
                );

                let rows = sqlx::query(&query).fetch_all(pool).await?;

                let mut summaries = Vec::new();
                for row in rows {
                    summaries.push(AddressSummary {
                        address: row.get("address"),
                        role: row.get("role"),
                        stamp_count: row.get("stamp_count"),
                        total_capacity: "N/A".to_string(), // Old method doesn't calculate capacity
                        first_seen: row.get("first_seen"),
                        last_seen: row.get("last_seen"),
                        is_owner: row.get("is_owner"),
                        is_payer: row.get("is_payer"),
                        is_sender: row.get("is_sender"),
                    });
                }

                Ok(summaries)
            }
        }
    }

    /// Get address summary with advanced filtering and capacity calculations
    #[allow(clippy::too_many_arguments)]
    pub async fn get_address_summary_filtered(
        &self,
        client: &crate::blockchain::BlockchainClient,
        registry: &crate::contracts::ContractRegistry,
        _config: &crate::config::AppConfig,
        min_stamps: u32,
        role: Option<crate::cli::RoleFilter>,
        live_only: bool,
        price: Option<String>,
        _refresh: bool,
        _cache_validity_blocks: u64,
    ) -> Result<Vec<crate::commands::address_summary::AddressSummary>> {
        use crate::commands::address_summary::AddressSummary;

        // Get current block and price for TTL calculations if needed
        let (current_block, current_price) = if live_only {
            let block = client.get_current_block().await?;
            let price_val = if let Some(p) = price {
                p.parse::<u128>()
                    .map_err(|e| crate::error::StampError::Parse(format!("Invalid price: {e}")))?
            } else {
                // Try to get cached price first
                match self.get_cached_price().await {
                    Ok(Some(cached_price)) => cached_price,
                    _ => client.get_current_price(registry).await?,
                }
            };
            (Some(block), Some(price_val))
        } else {
            (None, None)
        };

        // Build role filter SQL (PostgreSQL cannot use aliases in HAVING, need full expressions)
        let role_filter_sqlite = match role {
            Some(crate::cli::RoleFilter::Owner) => "AND is_owner",
            Some(crate::cli::RoleFilter::Sender) => "AND is_sender",
            Some(crate::cli::RoleFilter::OwnerAndSender) => "AND is_owner AND is_sender",
            None => "",
        };

        let role_filter_postgres = match role {
            Some(crate::cli::RoleFilter::Owner) => "AND (SUM(CASE WHEN role = 'owner' THEN 1 ELSE 0 END) > 0)",
            Some(crate::cli::RoleFilter::Sender) => "AND (SUM(CASE WHEN role = 'sender' THEN 1 ELSE 0 END) > 0)",
            Some(crate::cli::RoleFilter::OwnerAndSender) => "AND (SUM(CASE WHEN role = 'owner' THEN 1 ELSE 0 END) > 0) AND (SUM(CASE WHEN role = 'sender' THEN 1 ELSE 0 END) > 0)",
            None => "",
        };

        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                // Build live filter SQL for SQLite
                let live_filter = if live_only {
                    let _block = current_block.unwrap();
                    let price_val = current_price.unwrap();
                    format!(
                        r#"
                        AND batch_id IN (
                            SELECT b.batch_id
                            FROM batches b
                            LEFT JOIN batch_balances bb ON b.batch_id = bb.batch_id
                            WHERE (bb.remaining_balance IS NOT NULL
                                   AND CAST(bb.remaining_balance AS INTEGER) / ({price_val} * (1 << b.depth)) > 0)
                               OR (bb.remaining_balance IS NULL
                                   AND CAST(b.normalised_balance AS INTEGER) / ({price_val} * (1 << b.depth)) > 0)
                        )
                        "#
                    )
                } else {
                    String::new()
                };

                let query = format!(
                    r#"
                    WITH address_batch_info AS (
                        SELECT
                            address,
                            role,
                            batch_id,
                            block_timestamp
                        FROM (
                            SELECT
                                COALESCE(json_extract(data, '$.BatchCreated.owner'), json_extract(data, '$.BatchTopUp.owner')) as address,
                                'owner' as role,
                                batch_id,
                                block_timestamp
                            FROM events
                            WHERE event_type IN ('BatchCreated', 'BatchTopUp')
                              {live_filter}

                            UNION ALL

                            SELECT
                                COALESCE(json_extract(data, '$.BatchCreated.payer'), json_extract(data, '$.BatchTopUp.payer')) as address,
                                'payer' as role,
                                batch_id,
                                block_timestamp
                            FROM events
                            WHERE event_type IN ('BatchCreated', 'BatchTopUp')
                              AND COALESCE(json_extract(data, '$.BatchCreated.payer'), json_extract(data, '$.BatchTopUp.payer')) IS NOT NULL
                              {live_filter}

                            UNION ALL

                            SELECT
                                from_address as address,
                                'sender' as role,
                                batch_id,
                                block_timestamp
                            FROM events
                            WHERE from_address IS NOT NULL
                              {live_filter}
                        ) all_addresses
                        WHERE address IS NOT NULL
                    ),
                    address_roles AS (
                        SELECT
                            address,
                            SUM(CASE WHEN role = 'owner' THEN 1 ELSE 0 END) > 0 as is_owner,
                            SUM(CASE WHEN role = 'payer' THEN 1 ELSE 0 END) > 0 as is_payer,
                            SUM(CASE WHEN role = 'sender' THEN 1 ELSE 0 END) > 0 as is_sender,
                            COUNT(DISTINCT batch_id) as stamp_count,
                            MIN(block_timestamp) as first_seen,
                            MAX(block_timestamp) as last_seen
                        FROM address_batch_info
                        GROUP BY address
                        HAVING stamp_count >= {min_stamps}
                           {role_filter_sqlite}
                    ),
                    address_capacity AS (
                        SELECT
                            abi.address,
                            SUM(1 << b.depth) as total_capacity
                        FROM address_batch_info abi
                        JOIN batches b ON abi.batch_id = b.batch_id
                        WHERE abi.role = 'owner'
                        GROUP BY abi.address
                    )
                    SELECT
                        ar.address,
                        CASE
                            WHEN ar.is_owner AND ar.is_payer AND ar.is_sender THEN 'Owner+Payer+Sender'
                            WHEN ar.is_owner AND ar.is_sender THEN 'Owner+Sender'
                            WHEN ar.is_payer AND ar.is_sender THEN 'Payer+Sender'
                            WHEN ar.is_owner AND ar.is_payer THEN 'Owner+Payer'
                            WHEN ar.is_owner THEN 'Owner'
                            WHEN ar.is_payer THEN 'Payer'
                            WHEN ar.is_sender THEN 'Sender'
                            ELSE 'Unknown'
                        END as role,
                        ar.stamp_count,
                        COALESCE(ac.total_capacity, 0) as total_capacity,
                        datetime(ar.first_seen, 'unixepoch') as first_seen,
                        datetime(ar.last_seen, 'unixepoch') as last_seen,
                        ar.is_owner,
                        ar.is_payer,
                        ar.is_sender
                    FROM address_roles ar
                    LEFT JOIN address_capacity ac ON ar.address = ac.address
                    ORDER BY ar.stamp_count DESC, ar.address
                    "#
                );

                let rows = sqlx::query(&query).fetch_all(pool).await?;

                let mut summaries = Vec::new();
                for row in rows {
                    let capacity: i64 = row.get("total_capacity");
                    summaries.push(AddressSummary {
                        address: row.get("address"),
                        role: row.get("role"),
                        stamp_count: row.get("stamp_count"),
                        total_capacity: format_number(capacity as u128),
                        first_seen: row.get("first_seen"),
                        last_seen: row.get("last_seen"),
                        is_owner: row.get::<i64, _>("is_owner") != 0,
                        is_payer: row.get::<i64, _>("is_payer") != 0,
                        is_sender: row.get::<i64, _>("is_sender") != 0,
                    });
                }

                Ok(summaries)
            }
            DatabasePool::Postgres(pool) => {
                // Build live filter SQL for PostgreSQL
                let live_filter = if live_only {
                    let _block = current_block.unwrap();
                    let price_val = current_price.unwrap();
                    format!(
                        r#"
                        AND batch_id IN (
                            SELECT b.batch_id
                            FROM batches b
                            LEFT JOIN batch_balances bb ON b.batch_id = bb.batch_id
                            WHERE (bb.remaining_balance IS NOT NULL
                                   AND CAST(bb.remaining_balance AS NUMERIC) / ({price_val} * POW(2, b.depth)) > 0)
                               OR (bb.remaining_balance IS NULL
                                   AND CAST(b.normalised_balance AS NUMERIC) / ({price_val} * POW(2, b.depth)) > 0)
                        )
                        "#
                    )
                } else {
                    String::new()
                };

                let query = format!(
                    r#"
                    WITH address_batch_info AS (
                        SELECT
                            address,
                            role,
                            batch_id,
                            block_timestamp
                        FROM (
                            SELECT
                                data::jsonb->>'owner' as address,
                                'owner' as role,
                                batch_id,
                                block_timestamp
                            FROM events
                            WHERE event_type IN ('BatchCreated', 'BatchTopUp')
                              {live_filter}

                            UNION ALL

                            SELECT
                                data::jsonb->>'payer' as address,
                                'payer' as role,
                                batch_id,
                                block_timestamp
                            FROM events
                            WHERE event_type IN ('BatchCreated', 'BatchTopUp')
                              AND data::jsonb->>'payer' IS NOT NULL
                              {live_filter}

                            UNION ALL

                            SELECT
                                from_address as address,
                                'sender' as role,
                                batch_id,
                                block_timestamp
                            FROM events
                            WHERE from_address IS NOT NULL
                              {live_filter}
                        ) all_addresses
                        WHERE address IS NOT NULL
                    ),
                    address_roles AS (
                        SELECT
                            address,
                            SUM(CASE WHEN role = 'owner' THEN 1 ELSE 0 END) > 0 as is_owner,
                            SUM(CASE WHEN role = 'payer' THEN 1 ELSE 0 END) > 0 as is_payer,
                            SUM(CASE WHEN role = 'sender' THEN 1 ELSE 0 END) > 0 as is_sender,
                            COUNT(DISTINCT batch_id) as stamp_count,
                            MIN(block_timestamp) as first_seen,
                            MAX(block_timestamp) as last_seen
                        FROM address_batch_info
                        GROUP BY address
                        HAVING COUNT(DISTINCT batch_id) >= {min_stamps}
                           {role_filter_postgres}
                    ),
                    address_capacity AS (
                        SELECT
                            abi.address,
                            SUM(POW(2, b.depth)::BIGINT) as total_capacity
                        FROM address_batch_info abi
                        JOIN batches b ON abi.batch_id = b.batch_id
                        WHERE abi.role = 'owner'
                        GROUP BY abi.address
                    )
                    SELECT
                        ar.address,
                        CASE
                            WHEN ar.is_owner AND ar.is_payer AND ar.is_sender THEN 'Owner+Payer+Sender'
                            WHEN ar.is_owner AND ar.is_sender THEN 'Owner+Sender'
                            WHEN ar.is_payer AND ar.is_sender THEN 'Payer+Sender'
                            WHEN ar.is_owner AND ar.is_payer THEN 'Owner+Payer'
                            WHEN ar.is_owner THEN 'Owner'
                            WHEN ar.is_payer THEN 'Payer'
                            WHEN ar.is_sender THEN 'Sender'
                            ELSE 'Unknown'
                        END as role,
                        ar.stamp_count,
                        COALESCE(ac.total_capacity, 0)::TEXT as total_capacity,
                        to_char(to_timestamp(ar.first_seen), 'YYYY-MM-DD HH24:MI:SS') as first_seen,
                        to_char(to_timestamp(ar.last_seen), 'YYYY-MM-DD HH24:MI:SS') as last_seen,
                        ar.is_owner,
                        ar.is_payer,
                        ar.is_sender
                    FROM address_roles ar
                    LEFT JOIN address_capacity ac ON ar.address = ac.address
                    ORDER BY ar.stamp_count DESC, ar.address
                    "#
                );

                let rows = sqlx::query(&query).fetch_all(pool).await?;

                let mut summaries = Vec::new();
                for row in rows {
                    // PostgreSQL returns NUMERIC, need to parse as string
                    let capacity_str: String = row.get("total_capacity");
                    let capacity = capacity_str.parse::<u128>().unwrap_or(0);
                    summaries.push(AddressSummary {
                        address: row.get("address"),
                        role: row.get("role"),
                        stamp_count: row.get("stamp_count"),
                        total_capacity: format_number(capacity),
                        first_seen: row.get("first_seen"),
                        last_seen: row.get("last_seen"),
                        is_owner: row.get("is_owner"),
                        is_payer: row.get("is_payer"),
                        is_sender: row.get("is_sender"),
                    });
                }

                Ok(summaries)
            }
        }
    }

    /// Get delegation cases where owner != from_address
    pub async fn get_delegation_cases(
        &self,
    ) -> Result<Vec<crate::commands::address_summary::DelegationCase>> {
        use crate::commands::address_summary::DelegationCase;

        let query = r#"
            SELECT
                transaction_hash,
                COALESCE(json_extract(data, '$.BatchCreated.owner'), '') as owner,
                COALESCE(json_extract(data, '$.BatchCreated.payer'), 'N/A') as payer,
                COALESCE(from_address, 'N/A') as from_address,
                block_number,
                COALESCE(batch_id, 'N/A') as batch_id
            FROM events
            WHERE event_type = 'BatchCreated'
              AND from_address IS NOT NULL
              AND json_extract(data, '$.BatchCreated.owner') != from_address
            ORDER BY block_number DESC
            LIMIT 100
        "#;

        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let rows = sqlx::query(query).fetch_all(pool).await?;

                let mut cases = Vec::new();
                for row in rows {
                    cases.push(DelegationCase {
                        tx_hash: row.get("transaction_hash"),
                        owner: row.get("owner"),
                        payer: row.get("payer"),
                        from_address: row.get("from_address"),
                        block_number: row.get("block_number"),
                        batch_id: row.get("batch_id"),
                    });
                }

                Ok(cases)
            }
            DatabasePool::Postgres(pool) => {
                let query = r#"
                    SELECT
                        transaction_hash,
                        COALESCE(data::jsonb->>'owner', '') as owner,
                        COALESCE(data::jsonb->>'payer', 'N/A') as payer,
                        COALESCE(from_address, 'N/A') as from_address,
                        block_number,
                        COALESCE(batch_id, 'N/A') as batch_id
                    FROM events
                    WHERE event_type = 'BatchCreated'
                      AND from_address IS NOT NULL
                      AND data::jsonb->>'owner' != from_address
                    ORDER BY block_number DESC
                    LIMIT 100
                "#;

                let rows = sqlx::query(query).fetch_all(pool).await?;

                let mut cases = Vec::new();
                for row in rows {
                    cases.push(DelegationCase {
                        tx_hash: row.get("transaction_hash"),
                        owner: row.get("owner"),
                        payer: row.get("payer"),
                        from_address: row.get("from_address"),
                        block_number: row.get("block_number"),
                        batch_id: row.get("batch_id"),
                    });
                }

                Ok(cases)
            }
        }
    }

    /// Get migration status from the database
    pub async fn get_migration_status(&self) -> Result<Vec<MigrationInfo>> {
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let query = r#"
                    SELECT CAST(version AS TEXT) as version, description, installed_on
                    FROM _sqlx_migrations
                    ORDER BY version
                "#;
                let rows = sqlx::query(query).fetch_all(pool).await?;
                let mut migrations = Vec::new();
                for row in rows {
                    migrations.push(MigrationInfo {
                        version: row.get("version"),
                        description: row.get("description"),
                        installed_on: row.get("installed_on"),
                    });
                }
                Ok(migrations)
            }
            DatabasePool::Postgres(pool) => {
                // Use custom query to format timestamp as string
                let query_with_format = r#"
                    SELECT version, description, to_char(installed_on, 'YYYY-MM-DD HH24:MI:SS') as installed_on
                    FROM _sqlx_migrations
                    ORDER BY version
                "#;
                let rows = sqlx::query(query_with_format).fetch_all(pool).await?;
                let mut migrations = Vec::new();
                for row in rows {
                    let version: i64 = row.get("version");
                    migrations.push(MigrationInfo {
                        version: version.to_string(),
                        description: row.get("description"),
                        installed_on: row.get("installed_on"),
                    });
                }
                Ok(migrations)
            }
        }
    }

    // ========================================================================
    // Payment Channel Methods (Bandwidth Incentives)
    // ========================================================================

    /// Store chequebook deployment info
    pub async fn store_chequebook_deployment(&self, deployment: &ChequebookDeployment) -> Result<()> {
        let deployed_timestamp = deployment.deployed_at_timestamp.timestamp();
        let discovered_timestamp = Utc::now().timestamp();

        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    INSERT OR REPLACE INTO payment_channel_deployments
                    (chequebook_address, factory_address, deployed_at_block, deployed_at_timestamp,
                     transaction_hash, issuer_address, overlay_address, discovered_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(&deployment.chequebook_address)
                .bind(&deployment.factory_address)
                .bind(deployment.deployed_at_block as i64)
                .bind(deployed_timestamp)
                .bind(&deployment.transaction_hash)
                .bind(&deployment.issuer_address)
                .bind(&deployment.overlay_address)
                .bind(discovered_timestamp)
                .execute(pool)
                .await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO payment_channel_deployments
                    (chequebook_address, factory_address, deployed_at_block, deployed_at_timestamp,
                     transaction_hash, issuer_address, overlay_address, discovered_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                    ON CONFLICT (chequebook_address) DO UPDATE SET
                        factory_address = EXCLUDED.factory_address,
                        deployed_at_block = EXCLUDED.deployed_at_block,
                        deployed_at_timestamp = EXCLUDED.deployed_at_timestamp,
                        transaction_hash = EXCLUDED.transaction_hash,
                        issuer_address = EXCLUDED.issuer_address,
                        overlay_address = EXCLUDED.overlay_address
                    "#,
                )
                .bind(&deployment.chequebook_address)
                .bind(&deployment.factory_address)
                .bind(deployment.deployed_at_block as i64)
                .bind(deployed_timestamp)
                .bind(&deployment.transaction_hash)
                .bind(&deployment.issuer_address)
                .bind(&deployment.overlay_address)
                .bind(discovered_timestamp)
                .execute(pool)
                .await?;
            }
        }
        Ok(())
    }

    /// Update issuer address for a chequebook
    pub async fn update_chequebook_issuer(
        &self,
        chequebook_address: &str,
        issuer_address: &str,
    ) -> Result<()> {
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    UPDATE payment_channel_deployments
                    SET issuer_address = ?
                    WHERE chequebook_address = ?
                    "#,
                )
                .bind(issuer_address)
                .bind(chequebook_address)
                .execute(pool)
                .await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    UPDATE payment_channel_deployments
                    SET issuer_address = $1
                    WHERE chequebook_address = $2
                    "#,
                )
                .bind(issuer_address)
                .bind(chequebook_address)
                .execute(pool)
                .await?;
            }
        }
        Ok(())
    }

    /// Update overlay address for a chequebook
    #[allow(dead_code)]
    pub async fn update_chequebook_overlay(
        &self,
        chequebook_address: &str,
        overlay_address: &str,
    ) -> Result<()> {
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    UPDATE payment_channel_deployments
                    SET overlay_address = ?
                    WHERE chequebook_address = ?
                    "#,
                )
                .bind(overlay_address)
                .bind(chequebook_address)
                .execute(pool)
                .await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    UPDATE payment_channel_deployments
                    SET overlay_address = $1
                    WHERE chequebook_address = $2
                    "#,
                )
                .bind(overlay_address)
                .bind(chequebook_address)
                .execute(pool)
                .await?;
            }
        }
        Ok(())
    }

    /// Store payment channel events
    pub async fn store_payment_channel_events(&self, events: &[PaymentChannelEvent]) -> Result<()> {
        for event in events {
            // Convert event data to database fields
            let (beneficiary, recipient, caller, total_payout, cumulative_payout, caller_payout,
                 deposit_beneficiary, deposit_amount, deposit_decrease_amount, deposit_timeout,
                 withdraw_amount) = match &event.data {
                PaymentChannelEventData::ChequeCashed {
                    beneficiary,
                    recipient,
                    caller,
                    total_payout,
                    cumulative_payout,
                    caller_payout,
                } => (
                    Some(beneficiary.clone()),
                    Some(recipient.clone()),
                    Some(caller.clone()),
                    Some(total_payout.clone()),
                    Some(cumulative_payout.clone()),
                    Some(caller_payout.clone()),
                    None, None, None, None, None,
                ),
                PaymentChannelEventData::ChequeBounced {} => (
                    None, None, None, None, None, None, None, None, None, None, None,
                ),
                PaymentChannelEventData::HardDepositAmountChanged { beneficiary, amount } => (
                    None, None, None, None, None, None,
                    Some(beneficiary.clone()), Some(amount.clone()), None, None, None,
                ),
                PaymentChannelEventData::HardDepositDecreasePrepared { beneficiary, decrease_amount } => (
                    None, None, None, None, None, None,
                    Some(beneficiary.clone()), None, Some(decrease_amount.clone()), None, None,
                ),
                PaymentChannelEventData::HardDepositTimeoutChanged { beneficiary, timeout } => (
                    None, None, None, None, None, None,
                    Some(beneficiary.clone()), None, None, Some(*timeout as i64), None,
                ),
                PaymentChannelEventData::Withdraw { amount } => (
                    None, None, None, None, None, None, None, None, None, None, Some(amount.clone()),
                ),
            };

            let event_type_str = event.event_type.to_string();
            let block_timestamp = event.block_timestamp.timestamp();

            match &self.pool {
                DatabasePool::Sqlite(pool) => {
                    let data_json = serde_json::to_string(&event.data)?;
                    sqlx::query(
                        r#"
                        INSERT OR REPLACE INTO payment_channel_events
                        (event_type, chequebook_address, block_number, block_timestamp,
                         transaction_hash, log_index, beneficiary, recipient, caller,
                         total_payout, cumulative_payout, caller_payout,
                         deposit_beneficiary, deposit_amount, deposit_decrease_amount, deposit_timeout,
                         withdraw_amount, data)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                        "#,
                    )
                    .bind(&event_type_str)
                    .bind(&event.chequebook_address)
                    .bind(event.block_number as i64)
                    .bind(block_timestamp)
                    .bind(&event.transaction_hash)
                    .bind(event.log_index as i64)
                    .bind(&beneficiary)
                    .bind(&recipient)
                    .bind(&caller)
                    .bind(&total_payout)
                    .bind(&cumulative_payout)
                    .bind(&caller_payout)
                    .bind(&deposit_beneficiary)
                    .bind(&deposit_amount)
                    .bind(&deposit_decrease_amount)
                    .bind(deposit_timeout)
                    .bind(&withdraw_amount)
                    .bind(&data_json)
                    .execute(pool)
                    .await?;
                }
                DatabasePool::Postgres(pool) => {
                    sqlx::query(
                        r#"
                        INSERT INTO payment_channel_events
                        (event_type, chequebook_address, block_number, block_timestamp,
                         transaction_hash, log_index, beneficiary, recipient, caller,
                         total_payout, cumulative_payout, caller_payout,
                         deposit_beneficiary, deposit_amount, deposit_decrease_amount, deposit_timeout,
                         withdraw_amount, data)
                        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
                        ON CONFLICT (transaction_hash, log_index) DO NOTHING
                        "#,
                    )
                    .bind(&event_type_str)
                    .bind(&event.chequebook_address)
                    .bind(event.block_number as i64)
                    .bind(block_timestamp)
                    .bind(&event.transaction_hash)
                    .bind(event.log_index as i64)
                    .bind(&beneficiary)
                    .bind(&recipient)
                    .bind(&caller)
                    .bind(&total_payout)
                    .bind(&cumulative_payout)
                    .bind(&caller_payout)
                    .bind(&deposit_beneficiary)
                    .bind(&deposit_amount)
                    .bind(&deposit_decrease_amount)
                    .bind(deposit_timeout)
                    .bind(&withdraw_amount)
                    .bind(sqlx::types::Json(&event.data))
                    .execute(pool)
                    .await?;
                }
            }
        }
        Ok(())
    }

    /// Get all discovered chequebook addresses
    pub async fn get_discovered_chequebooks(&self) -> Result<Vec<ChequebookDeployment>> {
        let deployments = match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let rows = sqlx::query(
                    r#"
                    SELECT chequebook_address, factory_address, deployed_at_block,
                           deployed_at_timestamp, transaction_hash, issuer_address, overlay_address
                    FROM payment_channel_deployments
                    ORDER BY deployed_at_block ASC
                    "#,
                )
                .fetch_all(pool)
                .await?;

                let mut deployments = Vec::new();
                for row in rows {
                    let deployed_at_block: i64 = row.get("deployed_at_block");
                    let deployed_at_timestamp: i64 = row.get("deployed_at_timestamp");
                    deployments.push(ChequebookDeployment {
                        chequebook_address: row.get("chequebook_address"),
                        factory_address: row.get("factory_address"),
                        deployed_at_block: deployed_at_block as u64,
                        deployed_at_timestamp: DateTime::from_timestamp(deployed_at_timestamp, 0)
                            .unwrap_or_else(Utc::now),
                        transaction_hash: row.get("transaction_hash"),
                        issuer_address: row.get("issuer_address"),
                        overlay_address: row.get("overlay_address"),
                    });
                }
                deployments
            }
            DatabasePool::Postgres(pool) => {
                let rows = sqlx::query(
                    r#"
                    SELECT chequebook_address, factory_address, deployed_at_block,
                           deployed_at_timestamp, transaction_hash, issuer_address, overlay_address
                    FROM payment_channel_deployments
                    ORDER BY deployed_at_block ASC
                    "#,
                )
                .fetch_all(pool)
                .await?;

                let mut deployments = Vec::new();
                for row in rows {
                    let deployed_at_block: i64 = row.get("deployed_at_block");
                    let deployed_at_timestamp: i64 = row.get("deployed_at_timestamp");
                    deployments.push(ChequebookDeployment {
                        chequebook_address: row.get("chequebook_address"),
                        factory_address: row.get("factory_address"),
                        deployed_at_block: deployed_at_block as u64,
                        deployed_at_timestamp: DateTime::from_timestamp(deployed_at_timestamp, 0)
                            .unwrap_or_else(Utc::now),
                        transaction_hash: row.get("transaction_hash"),
                        issuer_address: row.get("issuer_address"),
                        overlay_address: row.get("overlay_address"),
                    });
                }
                deployments
            }
        };

        Ok(deployments)
    }

    /// Get payment channel events from database
    pub async fn get_payment_channel_events(
        &self,
        from_block: Option<u64>,
        to_block: Option<u64>,
        event_type_filter: Option<&str>,
    ) -> Result<Vec<PaymentChannelEvent>> {
        use crate::events::{PaymentChannelEventData, PaymentChannelEventType};

        let from = from_block.unwrap_or(0);
        let to = to_block.unwrap_or(u64::MAX);

        let events = match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let query = if let Some(event_type) = event_type_filter {
                    sqlx::query(
                        r#"
                        SELECT event_type, chequebook_address, block_number, block_timestamp,
                               transaction_hash, log_index, data
                        FROM payment_channel_events
                        WHERE block_number >= ? AND block_number <= ? AND event_type = ?
                        ORDER BY block_number ASC, log_index ASC
                        "#,
                    )
                    .bind(from as i64)
                    .bind(to as i64)
                    .bind(event_type)
                } else {
                    sqlx::query(
                        r#"
                        SELECT event_type, chequebook_address, block_number, block_timestamp,
                               transaction_hash, log_index, data
                        FROM payment_channel_events
                        WHERE block_number >= ? AND block_number <= ?
                        ORDER BY block_number ASC, log_index ASC
                        "#,
                    )
                    .bind(from as i64)
                    .bind(to as i64)
                };

                let rows = query.fetch_all(pool).await?;

                let mut events = Vec::new();
                for row in rows {
                    let event_type_str: String = row.get("event_type");
                    let event_type = match event_type_str.as_str() {
                        "ChequeCashed" => PaymentChannelEventType::ChequeCashed,
                        "ChequeBounced" => PaymentChannelEventType::ChequeBounced,
                        "HardDepositAmountChanged" => {
                            PaymentChannelEventType::HardDepositAmountChanged
                        }
                        "HardDepositDecreasePrepared" => {
                            PaymentChannelEventType::HardDepositDecreasePrepared
                        }
                        "HardDepositTimeoutChanged" => {
                            PaymentChannelEventType::HardDepositTimeoutChanged
                        }
                        "Withdraw" => PaymentChannelEventType::Withdraw,
                        _ => continue,
                    };

                    let data_str: String = row.get("data");
                    let data: PaymentChannelEventData = serde_json::from_str(&data_str)?;

                    let timestamp: i64 = row.get("block_timestamp");
                    let block_timestamp =
                        DateTime::from_timestamp(timestamp, 0).unwrap_or_else(Utc::now);

                    events.push(PaymentChannelEvent {
                        event_type,
                        chequebook_address: row.get("chequebook_address"),
                        block_number: row.get::<i64, _>("block_number") as u64,
                        block_timestamp,
                        transaction_hash: row.get("transaction_hash"),
                        log_index: row.get::<i64, _>("log_index") as u64,
                        data,
                    });
                }
                events
            }
            DatabasePool::Postgres(pool) => {
                let query = if let Some(event_type) = event_type_filter {
                    sqlx::query(
                        r#"
                        SELECT event_type, chequebook_address, block_number, block_timestamp,
                               transaction_hash, log_index, data
                        FROM payment_channel_events
                        WHERE block_number >= $1 AND block_number <= $2 AND event_type = $3
                        ORDER BY block_number ASC, log_index ASC
                        "#,
                    )
                    .bind(from as i64)
                    .bind(to as i64)
                    .bind(event_type)
                } else {
                    sqlx::query(
                        r#"
                        SELECT event_type, chequebook_address, block_number, block_timestamp,
                               transaction_hash, log_index, data
                        FROM payment_channel_events
                        WHERE block_number >= $1 AND block_number <= $2
                        ORDER BY block_number ASC, log_index ASC
                        "#,
                    )
                    .bind(from as i64)
                    .bind(to as i64)
                };

                let rows = query.fetch_all(pool).await?;

                let mut events = Vec::new();
                for row in rows {
                    let event_type_str: String = row.get("event_type");
                    let event_type = match event_type_str.as_str() {
                        "ChequeCashed" => PaymentChannelEventType::ChequeCashed,
                        "ChequeBounced" => PaymentChannelEventType::ChequeBounced,
                        "HardDepositAmountChanged" => {
                            PaymentChannelEventType::HardDepositAmountChanged
                        }
                        "HardDepositDecreasePrepared" => {
                            PaymentChannelEventType::HardDepositDecreasePrepared
                        }
                        "HardDepositTimeoutChanged" => {
                            PaymentChannelEventType::HardDepositTimeoutChanged
                        }
                        "Withdraw" => PaymentChannelEventType::Withdraw,
                        _ => continue,
                    };

                    let data_json: serde_json::Value = row.get("data");
                    let data: PaymentChannelEventData = serde_json::from_value(data_json)?;

                    let timestamp: i64 = row.get("block_timestamp");
                    let block_timestamp =
                        DateTime::from_timestamp(timestamp, 0).unwrap_or_else(Utc::now);

                    events.push(PaymentChannelEvent {
                        event_type,
                        chequebook_address: row.get("chequebook_address"),
                        block_number: row.get::<i64, _>("block_number") as u64,
                        block_timestamp,
                        transaction_hash: row.get("transaction_hash"),
                        log_index: row.get::<i64, _>("log_index") as u64,
                        data,
                    });
                }
                events
            }
        };

        Ok(events)
    }

    /// Get payment channel events for the last N months (0 for all time)
    pub async fn get_payment_channel_events_recent(
        &self,
        months: u32,
    ) -> Result<Vec<PaymentChannelEvent>> {
        use crate::events::{PaymentChannelEventData, PaymentChannelEventType};

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
                    SELECT event_type, chequebook_address, block_number, block_timestamp,
                           transaction_hash, log_index, data
                    FROM payment_channel_events
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
                        "ChequeCashed" => PaymentChannelEventType::ChequeCashed,
                        "ChequeBounced" => PaymentChannelEventType::ChequeBounced,
                        "HardDepositAmountChanged" => PaymentChannelEventType::HardDepositAmountChanged,
                        "HardDepositDecreasePrepared" => PaymentChannelEventType::HardDepositDecreasePrepared,
                        "HardDepositTimeoutChanged" => PaymentChannelEventType::HardDepositTimeoutChanged,
                        "Withdraw" => PaymentChannelEventType::Withdraw,
                        _ => continue,
                    };

                    let data_str: String = row.get("data");
                    let data: PaymentChannelEventData = serde_json::from_str(&data_str)?;

                    let timestamp: i64 = row.get("block_timestamp");
                    let block_timestamp =
                        DateTime::from_timestamp(timestamp, 0).unwrap_or_else(Utc::now);

                    events.push(PaymentChannelEvent {
                        event_type,
                        chequebook_address: row.get("chequebook_address"),
                        block_number: row.get::<i64, _>("block_number") as u64,
                        block_timestamp,
                        transaction_hash: row.get("transaction_hash"),
                        log_index: row.get::<i64, _>("log_index") as u64,
                        data,
                    });
                }
                events
            }
            DatabasePool::Postgres(pool) => {
                let rows = sqlx::query(
                    r#"
                    SELECT event_type, chequebook_address, block_number, block_timestamp,
                           transaction_hash, log_index, data
                    FROM payment_channel_events
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
                        "ChequeCashed" => PaymentChannelEventType::ChequeCashed,
                        "ChequeBounced" => PaymentChannelEventType::ChequeBounced,
                        "HardDepositAmountChanged" => PaymentChannelEventType::HardDepositAmountChanged,
                        "HardDepositDecreasePrepared" => PaymentChannelEventType::HardDepositDecreasePrepared,
                        "HardDepositTimeoutChanged" => PaymentChannelEventType::HardDepositTimeoutChanged,
                        "Withdraw" => PaymentChannelEventType::Withdraw,
                        _ => continue,
                    };

                    let data_json: serde_json::Value = row.get("data");
                    let data: PaymentChannelEventData = serde_json::from_value(data_json)?;

                    let timestamp: i64 = row.get("block_timestamp");
                    let block_timestamp =
                        DateTime::from_timestamp(timestamp, 0).unwrap_or_else(Utc::now);

                    events.push(PaymentChannelEvent {
                        event_type,
                        chequebook_address: row.get("chequebook_address"),
                        block_number: row.get::<i64, _>("block_number") as u64,
                        block_timestamp,
                        transaction_hash: row.get("transaction_hash"),
                        log_index: row.get::<i64, _>("log_index") as u64,
                        data,
                    });
                }
                events
            }
        };

        Ok(events)
    }

    /// Get last scanned block for factory (used for incremental discovery)
    #[allow(dead_code)]
    pub async fn get_last_factory_scan_block(&self, factory_address: &str) -> Result<Option<u64>> {
        let max_block: Option<i64> = match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let row = sqlx::query(
                    "SELECT MAX(deployed_at_block) as max_block FROM payment_channel_deployments WHERE factory_address = ?"
                )
                .bind(factory_address)
                .fetch_one(pool)
                .await?;
                row.get("max_block")
            }
            DatabasePool::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT MAX(deployed_at_block) as max_block FROM payment_channel_deployments WHERE factory_address = $1"
                )
                .bind(factory_address)
                .fetch_one(pool)
                .await?;
                row.get("max_block")
            }
        };
        Ok(max_block.map(|b| b as u64))
    }

    /// Count total payment channel events
    pub async fn count_payment_channel_events(&self) -> Result<u64> {
        let count: i64 = match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let row = sqlx::query("SELECT COUNT(*) as count FROM payment_channel_events")
                    .fetch_one(pool)
                    .await?;
                row.get("count")
            }
            DatabasePool::Postgres(pool) => {
                let row = sqlx::query("SELECT COUNT(*) as count FROM payment_channel_events")
                    .fetch_one(pool)
                    .await?;
                row.get("count")
            }
        };
        Ok(count as u64)
    }

    /// Count total discovered chequebooks
    pub async fn count_chequebooks(&self) -> Result<u64> {
        let count: i64 = match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let row = sqlx::query("SELECT COUNT(*) as count FROM payment_channel_deployments")
                    .fetch_one(pool)
                    .await?;
                row.get("count")
            }
            DatabasePool::Postgres(pool) => {
                let row = sqlx::query("SELECT COUNT(*) as count FROM payment_channel_deployments")
                    .fetch_one(pool)
                    .await?;
                row.get("count")
            }
        };
        Ok(count as u64)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_rpc_rate_limit(
        &self,
        rpc_url: &str,
        discovered_rate_limit: f64,
        strategy: &str,
        total_requests: u64,
        rate_limit_errors: u64,
        measured_rps: f64,
        consecutive_successes: u32,
    ) -> Result<()> {
        let success_rate = if total_requests > 0 {
            1.0 - (rate_limit_errors as f64 / total_requests as f64)
        } else {
            1.0
        };

        match &self.pool {
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO rpc_rate_limits (
                        rpc_url, discovered_rate_limit, rate_limit_strategy,
                        total_requests, rate_limit_errors, success_rate,
                        measured_rps, consecutive_successes, last_updated_at
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
                    ON CONFLICT (rpc_url) DO UPDATE SET
                        discovered_rate_limit = $2,
                        rate_limit_strategy = $3,
                        total_requests = $4,
                        rate_limit_errors = $5,
                        success_rate = $6,
                        measured_rps = $7,
                        consecutive_successes = $8,
                        last_updated_at = NOW()
                    "#,
                )
                .bind(rpc_url)
                .bind(discovered_rate_limit)
                .bind(strategy)
                .bind(total_requests as i64)
                .bind(rate_limit_errors as i64)
                .bind(success_rate)
                .bind(measured_rps)
                .bind(consecutive_successes as i32)
                .execute(pool)
                .await?;
            }
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO rpc_rate_limits (
                        rpc_url, discovered_rate_limit, rate_limit_strategy,
                        total_requests, rate_limit_errors, success_rate,
                        measured_rps, consecutive_successes, last_updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))
                    ON CONFLICT (rpc_url) DO UPDATE SET
                        discovered_rate_limit = ?2,
                        rate_limit_strategy = ?3,
                        total_requests = ?4,
                        rate_limit_errors = ?5,
                        success_rate = ?6,
                        measured_rps = ?7,
                        consecutive_successes = ?8,
                        last_updated_at = datetime('now')
                    "#,
                )
                .bind(rpc_url)
                .bind(discovered_rate_limit)
                .bind(strategy)
                .bind(total_requests as i64)
                .bind(rate_limit_errors as i64)
                .bind(success_rate)
                .bind(measured_rps)
                .bind(consecutive_successes as i32)
                .execute(pool)
                .await?;
            }
        }

        Ok(())
    }

    /// Get previously discovered rate limit for an RPC
    pub async fn get_rpc_rate_limit(&self, rpc_url: &str) -> Result<Option<f64>> {
        let rate_limit = match &self.pool {
            DatabasePool::Postgres(pool) => {
                sqlx::query_scalar::<_, f64>(
                    "SELECT discovered_rate_limit FROM rpc_rate_limits WHERE rpc_url = $1 AND is_active = TRUE"
                )
                .bind(rpc_url)
                .fetch_optional(pool)
                .await?
            }
            DatabasePool::Sqlite(pool) => {
                sqlx::query_scalar::<_, f64>(
                    "SELECT discovered_rate_limit FROM rpc_rate_limits WHERE rpc_url = ?1 AND is_active = 1"
                )
                .bind(rpc_url)
                .fetch_optional(pool)
                .await?
            }
        };

        Ok(rate_limit)
    }

    /// Get all RPC rate limit statistics
    #[allow(dead_code)]
    pub async fn get_all_rpc_stats(&self) -> Result<Vec<RpcRateLimitStats>> {
        let stats = match &self.pool {
            DatabasePool::Postgres(pool) => {
                sqlx::query_as::<_, RpcRateLimitStats>(
                    r#"
                    SELECT
                        rpc_url,
                        discovered_rate_limit,
                        rate_limit_strategy,
                        total_requests,
                        rate_limit_errors,
                        success_rate,
                        measured_rps,
                        EXTRACT(EPOCH FROM (NOW() - last_updated_at)) as seconds_since_update
                    FROM rpc_rate_limits
                    WHERE is_active = TRUE
                    ORDER BY last_updated_at DESC
                    "#,
                )
                .fetch_all(pool)
                .await?
            }
            DatabasePool::Sqlite(pool) => {
                sqlx::query_as::<_, RpcRateLimitStats>(
                    r#"
                    SELECT
                        rpc_url,
                        discovered_rate_limit,
                        rate_limit_strategy,
                        total_requests,
                        rate_limit_errors,
                        success_rate,
                        measured_rps,
                        (julianday('now') - julianday(last_updated_at)) * 86400 as seconds_since_update
                    FROM rpc_rate_limits
                    WHERE is_active = 1
                    ORDER BY last_updated_at DESC
                    "#,
                )
                .fetch_all(pool)
                .await?
            }
        };

        Ok(stats)
    }
}

/// Migration information
#[derive(Debug, Clone)]
pub struct MigrationInfo {
    pub version: String,
    pub description: String,
    pub installed_on: String,
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
            batch_id: Some("0x1234".to_string()),
            block_number: 1000,
            block_timestamp: Utc::now(),
            transaction_hash: "0xabcd".to_string(),
            log_index: 0,
            contract_source: "PostageStamp".to_string(),
            contract_address: None,
            from_address: None,
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
        assert_eq!(retrieved[0].batch_id, Some("0x1234".to_string()));
    }

    #[tokio::test]
    async fn test_store_and_retrieve_batches() {
        let (cache, _temp_file) = create_test_cache().await;

        let batches = vec![BatchInfo {
            batch_id: "0x1234".to_string(),
            owner: "0x5678".to_string(),
            payer: None,
            contract_source: "PostageStamp".to_string(),
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
                batch_id: Some("0x1234".to_string()),
                block_number: 1000,
                block_timestamp: Utc::now(),
                transaction_hash: "0xabcd1".to_string(),
                log_index: 0,
                contract_source: "PostageStamp".to_string(),
                contract_address: None,
                from_address: None,
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
                batch_id: Some("0x1234".to_string()),
                block_number: 2000,
                block_timestamp: Utc::now(),
                transaction_hash: "0xabcd2".to_string(),
                log_index: 0,
                contract_source: "PostageStamp".to_string(),
                contract_address: None,
                from_address: None,
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

/// RPC rate limit statistics
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct RpcRateLimitStats {
    pub rpc_url: String,
    pub discovered_rate_limit: f64,
    pub rate_limit_strategy: String,
    pub total_requests: i64,
    pub rate_limit_errors: i64,
    pub success_rate: f64,
    pub measured_rps: f64,
    pub seconds_since_update: f64,
}
