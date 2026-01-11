use crate::error::Result;
use crate::events::{BatchInfo, EventData, EventType, StampEvent, StorageIncentivesEvent};
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
    // Address Tracking Methods (Phase 2+3)
    // ========================================================================

    /// Store transaction details in the cache
    pub async fn store_transaction_details(
        &self,
        tx_hash: &str,
        from_address: &str,
        to_address: Option<&str>,
        value: &str,
        gas_price: Option<&str>,
        gas_used: Option<u64>,
        block_number: u64,
        block_timestamp: i64,
        input_data: Option<&str>,
        is_contract_creation: bool,
    ) -> Result<()> {
        let fetched_at = Utc::now().timestamp();

        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    INSERT OR REPLACE INTO transaction_details
                    (transaction_hash, from_address, to_address, value, gas_price, gas_used,
                     block_number, block_timestamp, input_data, is_contract_creation, fetched_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(tx_hash)
                .bind(from_address)
                .bind(to_address)
                .bind(value)
                .bind(gas_price)
                .bind(gas_used.map(|g| g as i64))
                .bind(block_number as i64)
                .bind(block_timestamp)
                .bind(input_data)
                .bind(if is_contract_creation { 1 } else { 0 })
                .bind(fetched_at)
                .execute(pool)
                .await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO transaction_details
                    (transaction_hash, from_address, to_address, value, gas_price, gas_used,
                     block_number, block_timestamp, input_data, is_contract_creation, fetched_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                    ON CONFLICT (transaction_hash) DO UPDATE SET
                        from_address = EXCLUDED.from_address,
                        to_address = EXCLUDED.to_address,
                        value = EXCLUDED.value,
                        gas_price = EXCLUDED.gas_price,
                        gas_used = EXCLUDED.gas_used,
                        block_number = EXCLUDED.block_number,
                        block_timestamp = EXCLUDED.block_timestamp,
                        input_data = EXCLUDED.input_data,
                        is_contract_creation = EXCLUDED.is_contract_creation,
                        fetched_at = EXCLUDED.fetched_at
                    "#,
                )
                .bind(tx_hash)
                .bind(from_address)
                .bind(to_address)
                .bind(value)
                .bind(gas_price)
                .bind(gas_used.map(|g| g as i64))
                .bind(block_number as i64)
                .bind(block_timestamp)
                .bind(input_data)
                .bind(is_contract_creation)
                .bind(fetched_at)
                .execute(pool)
                .await?;
            }
        }

        Ok(())
    }

    /// Get transaction details from cache (returns None if not cached)
    pub async fn get_transaction_details(&self, tx_hash: &str) -> Result<Option<TransactionDetails>> {
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let row = sqlx::query(
                    r#"
                    SELECT transaction_hash, from_address, to_address, value, gas_price, gas_used,
                           block_number, block_timestamp, input_data, is_contract_creation, fetched_at
                    FROM transaction_details
                    WHERE transaction_hash = ?
                    "#,
                )
                .bind(tx_hash)
                .fetch_optional(pool)
                .await?;

                if let Some(row) = row {
                    let is_contract: i64 = row.get("is_contract_creation");
                    let gas_used: Option<i64> = row.get("gas_used");

                    Ok(Some(TransactionDetails {
                        transaction_hash: row.get("transaction_hash"),
                        from_address: row.get("from_address"),
                        to_address: row.get("to_address"),
                        value: row.get("value"),
                        gas_price: row.get("gas_price"),
                        gas_used: gas_used.map(|g| g as u64),
                        block_number: row.get::<i64, _>("block_number") as u64,
                        block_timestamp: row.get("block_timestamp"),
                        input_data: row.get("input_data"),
                        is_contract_creation: is_contract != 0,
                        fetched_at: row.get("fetched_at"),
                    }))
                } else {
                    Ok(None)
                }
            }
            DatabasePool::Postgres(pool) => {
                let row = sqlx::query(
                    r#"
                    SELECT transaction_hash, from_address, to_address, value, gas_price, gas_used,
                           block_number, block_timestamp, input_data, is_contract_creation, fetched_at
                    FROM transaction_details
                    WHERE transaction_hash = $1
                    "#,
                )
                .bind(tx_hash)
                .fetch_optional(pool)
                .await?;

                if let Some(row) = row {
                    let gas_used: Option<i64> = row.get("gas_used");

                    Ok(Some(TransactionDetails {
                        transaction_hash: row.get("transaction_hash"),
                        from_address: row.get("from_address"),
                        to_address: row.get("to_address"),
                        value: row.get("value"),
                        gas_price: row.get("gas_price"),
                        gas_used: gas_used.map(|g| g as u64),
                        block_number: row.get::<i64, _>("block_number") as u64,
                        block_timestamp: row.get("block_timestamp"),
                        input_data: row.get("input_data"),
                        is_contract_creation: row.get("is_contract_creation"),
                        fetched_at: row.get("fetched_at"),
                    }))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Create or update an address record
    /// Uses UPSERT logic to handle both new addresses and updates to existing ones
    pub async fn upsert_address(
        &self,
        address: &str,
        batch_id: Option<&str>,
        amount_spent: Option<&str>,
        block_number: u64,
        block_timestamp: i64,
        is_contract: bool,
    ) -> Result<()> {
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                // First, try to get existing address to update stamp_ids array
                let existing = sqlx::query(
                    "SELECT stamp_ids, total_stamps_purchased, total_amount_spent, first_seen, first_block FROM addresses WHERE address = ?",
                )
                .bind(address)
                .fetch_optional(pool)
                .await?;

                if let Some(existing_row) = existing {
                    // Update existing address
                    let mut stamp_ids: Vec<String> = {
                        let ids_str: String = existing_row.get("stamp_ids");
                        serde_json::from_str(&ids_str).unwrap_or_default()
                    };

                    let mut total_stamps: i64 = existing_row.get("total_stamps_purchased");
                    let mut total_spent: String = existing_row.get("total_amount_spent");

                    // Add new batch_id if provided and not already in the list
                    if let Some(bid) = batch_id {
                        if !stamp_ids.contains(&bid.to_string()) {
                            stamp_ids.push(bid.to_string());
                            total_stamps += 1;
                        }
                    }

                    // Add to total spent
                    if let Some(amount) = amount_spent {
                        let current: u128 = total_spent.parse().unwrap_or(0);
                        let add: u128 = amount.parse().unwrap_or(0);
                        total_spent = (current + add).to_string();
                    }

                    let stamp_ids_json = serde_json::to_string(&stamp_ids)?;

                    sqlx::query(
                        r#"
                        UPDATE addresses SET
                            stamp_ids = ?,
                            total_stamps_purchased = ?,
                            total_amount_spent = ?,
                            last_seen = ?,
                            last_block = ?,
                            transaction_count = transaction_count + 1,
                            is_contract = ?
                        WHERE address = ?
                        "#,
                    )
                    .bind(&stamp_ids_json)
                    .bind(total_stamps)
                    .bind(&total_spent)
                    .bind(block_timestamp)
                    .bind(block_number as i64)
                    .bind(if is_contract { 1 } else { 0 })
                    .bind(address)
                    .execute(pool)
                    .await?;
                } else {
                    // Insert new address
                    let stamp_ids = if let Some(bid) = batch_id {
                        serde_json::to_string(&vec![bid])?
                    } else {
                        "[]".to_string()
                    };

                    let total_stamps = if batch_id.is_some() { 1 } else { 0 };
                    let total_spent = amount_spent.unwrap_or("0");

                    sqlx::query(
                        r#"
                        INSERT INTO addresses
                        (address, stamp_ids, total_stamps_purchased, total_amount_spent,
                         first_seen, last_seen, first_block, last_block, transaction_count, is_contract)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?)
                        "#,
                    )
                    .bind(address)
                    .bind(&stamp_ids)
                    .bind(total_stamps)
                    .bind(total_spent)
                    .bind(block_timestamp)
                    .bind(block_timestamp)
                    .bind(block_number as i64)
                    .bind(block_number as i64)
                    .bind(if is_contract { 1 } else { 0 })
                    .execute(pool)
                    .await?;
                }
            }
            DatabasePool::Postgres(pool) => {
                // PostgreSQL version with proper UPSERT
                let stamp_ids_array = if let Some(bid) = batch_id {
                    vec![bid]
                } else {
                    vec![]
                };

                let total_stamps = if batch_id.is_some() { 1 } else { 0 };
                let total_spent = amount_spent.unwrap_or("0");

                sqlx::query(
                    r#"
                    INSERT INTO addresses
                    (address, stamp_ids, total_stamps_purchased, total_amount_spent,
                     first_seen, last_seen, first_block, last_block, transaction_count, is_contract)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 1, $9)
                    ON CONFLICT (address) DO UPDATE SET
                        stamp_ids = CASE
                            WHEN $2 = '{}' THEN addresses.stamp_ids
                            WHEN addresses.stamp_ids @> $2 THEN addresses.stamp_ids
                            ELSE addresses.stamp_ids || $2
                        END,
                        total_stamps_purchased = CASE
                            WHEN $2 = '{}' THEN addresses.total_stamps_purchased
                            WHEN addresses.stamp_ids @> $2 THEN addresses.total_stamps_purchased
                            ELSE addresses.total_stamps_purchased + 1
                        END,
                        total_amount_spent = (CAST(addresses.total_amount_spent AS NUMERIC) + CAST($4 AS NUMERIC))::TEXT,
                        last_seen = $6,
                        last_block = $8,
                        transaction_count = addresses.transaction_count + 1,
                        is_contract = $9
                    "#,
                )
                .bind(address)
                .bind(&stamp_ids_array)
                .bind(total_stamps)
                .bind(total_spent)
                .bind(block_timestamp)
                .bind(block_timestamp)
                .bind(block_number as i64)
                .bind(block_number as i64)
                .bind(is_contract)
                .execute(pool)
                .await?;
            }
        }

        Ok(())
    }

    /// Store an address interaction (funding relationship)
    pub async fn store_address_interaction(
        &self,
        from_address: &str,
        to_address: &str,
        transaction_hash: &str,
        amount: &str,
        block_number: u64,
        block_timestamp: i64,
        related_to_stamp: bool,
        stamp_batch_id: Option<&str>,
    ) -> Result<()> {
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    INSERT OR IGNORE INTO address_interactions
                    (from_address, to_address, transaction_hash, amount, block_number,
                     block_timestamp, related_to_stamp, stamp_batch_id)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(from_address)
                .bind(to_address)
                .bind(transaction_hash)
                .bind(amount)
                .bind(block_number as i64)
                .bind(block_timestamp)
                .bind(if related_to_stamp { 1 } else { 0 })
                .bind(stamp_batch_id)
                .execute(pool)
                .await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO address_interactions
                    (from_address, to_address, transaction_hash, amount, block_number,
                     block_timestamp, related_to_stamp, stamp_batch_id)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                    ON CONFLICT (transaction_hash, from_address, to_address) DO NOTHING
                    "#,
                )
                .bind(from_address)
                .bind(to_address)
                .bind(transaction_hash)
                .bind(amount)
                .bind(block_number as i64)
                .bind(block_timestamp)
                .bind(related_to_stamp)
                .bind(stamp_batch_id)
                .execute(pool)
                .await?;
            }
        }

        Ok(())
    }
}

/// Transaction details stored in cache
#[derive(Debug, Clone)]
pub struct TransactionDetails {
    pub transaction_hash: String,
    pub from_address: String,
    pub to_address: Option<String>,
    pub value: String,
    pub gas_price: Option<String>,
    pub gas_used: Option<u64>,
    pub block_number: u64,
    pub block_timestamp: i64,
    pub input_data: Option<String>,
    pub is_contract_creation: bool,
    pub fetched_at: i64,
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
