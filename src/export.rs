use crate::batch::PeriodStats;
use crate::error::Result;
use crate::events::{BatchInfo, StampEvent};
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Export format types
#[derive(Debug, Clone)]
pub enum ExportFormat {
    Csv,
    Json,
}

/// Export events to a file
pub fn export_events<P: AsRef<Path>>(
    events: &[StampEvent],
    path: P,
    format: ExportFormat,
) -> Result<()> {
    match format {
        ExportFormat::Csv => export_events_csv(events, path),
        ExportFormat::Json => export_events_json(events, path),
    }
}

/// Export batches to a file
pub fn export_batches<P: AsRef<Path>>(
    batches: &[BatchInfo],
    path: P,
    format: ExportFormat,
) -> Result<()> {
    match format {
        ExportFormat::Csv => export_batches_csv(batches, path),
        ExportFormat::Json => export_batches_json(batches, path),
    }
}

/// Export period statistics to a file
pub fn export_stats<P: AsRef<Path>>(
    stats: &[PeriodStats],
    path: P,
    format: ExportFormat,
) -> Result<()> {
    match format {
        ExportFormat::Csv => export_stats_csv(stats, path),
        ExportFormat::Json => export_stats_json(stats, path),
    }
}

// CSV export implementations

fn export_events_csv<P: AsRef<Path>>(events: &[StampEvent], path: P) -> Result<()> {
    let mut file = File::create(path)?;

    // Write header
    writeln!(
        file,
        "block_number,timestamp,event_type,batch_id,transaction_hash,log_index,details"
    )?;

    // Write data
    for event in events {
        let details = serde_json::to_string(&event.data)?;
        writeln!(
            file,
            "{},{},{},{},{},{},\"{}\"",
            event.block_number,
            event.block_timestamp.to_rfc3339(),
            event.event_type,
            event.batch_id.as_deref().unwrap_or("N/A"),
            event.transaction_hash,
            event.log_index,
            details.replace("\"", "\"\"")
        )?;
    }

    Ok(())
}

fn export_batches_csv<P: AsRef<Path>>(batches: &[BatchInfo], path: P) -> Result<()> {
    let mut file = File::create(path)?;

    // Write header
    writeln!(
        file,
        "batch_id,owner,depth,bucket_depth,immutable,normalised_balance,created_at"
    )?;

    // Write data
    for batch in batches {
        writeln!(
            file,
            "{},{},{},{},{},{},{}",
            batch.batch_id,
            batch.owner,
            batch.depth,
            batch.bucket_depth,
            batch.immutable,
            batch.normalised_balance,
            batch.created_at.to_rfc3339()
        )?;
    }

    Ok(())
}

fn export_stats_csv<P: AsRef<Path>>(stats: &[PeriodStats], path: P) -> Result<()> {
    let mut file = File::create(path)?;

    // Write header
    writeln!(
        file,
        "period_key,period_label,batch_created,batch_topup,batch_depth_increase,total_events,unique_batches"
    )?;

    // Write data
    for stat in stats {
        writeln!(
            file,
            "{},{},{},{},{},{},{}",
            stat.period_key,
            stat.period_label,
            stat.batch_created_count,
            stat.batch_topup_count,
            stat.batch_depth_increase_count,
            stat.total_events,
            stat.unique_batches
        )?;
    }

    Ok(())
}

// JSON export implementations

fn export_events_json<P: AsRef<Path>>(events: &[StampEvent], path: P) -> Result<()> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, events)?;
    Ok(())
}

fn export_batches_json<P: AsRef<Path>>(batches: &[BatchInfo], path: P) -> Result<()> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, batches)?;
    Ok(())
}

fn export_stats_json<P: AsRef<Path>>(stats: &[PeriodStats], path: P) -> Result<()> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, stats)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventData, EventType};
    use chrono::{TimeZone, Utc};
    use tempfile::NamedTempFile;

    #[test]
    fn test_export_events_json() {
        let events = vec![StampEvent {
            event_type: EventType::BatchCreated,
            batch_id: "0x1234".to_string(),
            block_number: 1000,
            block_timestamp: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            transaction_hash: "0xabcd".to_string(),
            log_index: 0,
            contract_source: "PostageStamp".to_string(),
            contract_address: None,
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

        let temp_file = NamedTempFile::new().unwrap();
        export_events(&events, temp_file.path(), ExportFormat::Json).unwrap();

        let content = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(content.contains("BatchCreated"));
        assert!(content.contains("0x1234"));
    }

    #[test]
    fn test_export_events_csv() {
        let events = vec![StampEvent {
            event_type: EventType::BatchCreated,
            batch_id: "0x1234".to_string(),
            block_number: 1000,
            block_timestamp: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            transaction_hash: "0xabcd".to_string(),
            log_index: 0,
            contract_source: "PostageStamp".to_string(),
            contract_address: None,
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

        let temp_file = NamedTempFile::new().unwrap();
        export_events(&events, temp_file.path(), ExportFormat::Csv).unwrap();

        let content = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(content.contains("block_number"));
        assert!(content.contains("1000"));
        assert!(content.contains("0x1234"));
    }

    #[test]
    fn test_export_batches_json() {
        let batches = vec![BatchInfo {
            batch_id: "0x1234".to_string(),
            owner: "0x5678".to_string(),
            depth: 20,
            bucket_depth: 16,
            immutable: false,
            normalised_balance: "500000000000000000".to_string(),
            created_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            block_number: 1000,
        }];

        let temp_file = NamedTempFile::new().unwrap();
        export_batches(&batches, temp_file.path(), ExportFormat::Json).unwrap();

        let content = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(content.contains("0x1234"));
        assert!(content.contains("0x5678"));
    }

    #[test]
    fn test_export_batches_csv() {
        let batches = vec![BatchInfo {
            batch_id: "0x1234".to_string(),
            owner: "0x5678".to_string(),
            depth: 20,
            bucket_depth: 16,
            immutable: false,
            normalised_balance: "500000000000000000".to_string(),
            created_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            block_number: 1000,
        }];

        let temp_file = NamedTempFile::new().unwrap();
        export_batches(&batches, temp_file.path(), ExportFormat::Csv).unwrap();

        let content = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(content.contains("batch_id"));
        assert!(content.contains("0x1234"));
    }

    #[test]
    fn test_export_stats_json() {
        let stats = vec![PeriodStats {
            period_key: "2025-01".to_string(),
            period_label: "January 2025".to_string(),
            batch_created_count: 5,
            batch_topup_count: 10,
            batch_depth_increase_count: 2,
            total_events: 17,
            unique_batches: 5,
        }];

        let temp_file = NamedTempFile::new().unwrap();
        export_stats(&stats, temp_file.path(), ExportFormat::Json).unwrap();

        let content = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(content.contains("2025-01"));
        assert!(content.contains("January 2025"));
    }

    #[test]
    fn test_export_stats_csv() {
        let stats = vec![PeriodStats {
            period_key: "2025-01".to_string(),
            period_label: "January 2025".to_string(),
            batch_created_count: 5,
            batch_topup_count: 10,
            batch_depth_increase_count: 2,
            total_events: 17,
            unique_batches: 5,
        }];

        let temp_file = NamedTempFile::new().unwrap();
        export_stats(&stats, temp_file.path(), ExportFormat::Csv).unwrap();

        let content = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(content.contains("period_key"));
        assert!(content.contains("2025-01"));
    }
}
