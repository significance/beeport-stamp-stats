use crate::cache::Cache;
use crate::cli::OutputFormat;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use tabled::Tabled;

/// Address summary entry showing stamp activity
#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct AddressSummary {
    #[tabled(rename = "Address")]
    pub address: String,

    #[tabled(rename = "Role")]
    pub role: String,

    #[tabled(rename = "Stamps")]
    pub stamp_count: i64,

    #[tabled(rename = "First Activity")]
    pub first_seen: String,

    #[tabled(rename = "Last Activity")]
    pub last_seen: String,

    #[tabled(skip)]
    pub is_owner: bool,

    #[tabled(skip)]
    pub is_payer: bool,

    #[tabled(skip)]
    pub is_sender: bool,
}

/// Delegation case where owner != from_address
#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct DelegationCase {
    #[tabled(rename = "Transaction Hash")]
    pub tx_hash: String,

    #[tabled(rename = "Owner")]
    pub owner: String,

    #[tabled(rename = "Payer")]
    pub payer: String,

    #[tabled(rename = "Sender (from)")]
    pub from_address: String,

    #[tabled(rename = "Block")]
    pub block_number: i64,

    #[tabled(rename = "Batch ID")]
    pub batch_id: String,
}

pub async fn execute(
    cache: Cache,
    output: OutputFormat,
    min_stamps: u32,
    show_delegated_only: bool,
) -> Result<()> {
    if show_delegated_only {
        execute_delegated_analysis(cache, output).await
    } else {
        execute_full_summary(cache, output, min_stamps).await
    }
}

async fn execute_full_summary(cache: Cache, output: OutputFormat, min_stamps: u32) -> Result<()> {
    let addresses = cache.get_address_summary(min_stamps).await?;

    match output {
        OutputFormat::Table => {
            use tabled::Table;
            println!("\n## Address Summary\n");
            println!("{}", Table::new(&addresses));
            println!("\n**Total unique addresses:** {}", addresses.len());
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&addresses)?);
        }
        OutputFormat::Csv => {
            println!("address,role,stamp_count,first_seen,last_seen,is_owner,is_payer,is_sender");
            for addr in &addresses {
                println!(
                    "{},{},{},{},{},{},{},{}",
                    addr.address,
                    addr.role,
                    addr.stamp_count,
                    addr.first_seen,
                    addr.last_seen,
                    addr.is_owner,
                    addr.is_payer,
                    addr.is_sender
                );
            }
        }
    }

    Ok(())
}

async fn execute_delegated_analysis(cache: Cache, output: OutputFormat) -> Result<()> {
    let delegations = cache.get_delegation_cases().await?;

    match output {
        OutputFormat::Table => {
            use tabled::Table;
            println!("\n## Delegation Cases (Owner ≠ Sender)\n");
            println!("{}", Table::new(&delegations));
            println!("\n**Total delegation cases:** {}", delegations.len());
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&delegations)?);
        }
        OutputFormat::Csv => {
            println!("tx_hash,owner,payer,from_address,block_number,batch_id");
            for del in &delegations {
                println!(
                    "{},{},{},{},{},{}",
                    del.tx_hash,
                    del.owner,
                    del.payer,
                    del.from_address,
                    del.block_number,
                    del.batch_id
                );
            }
        }
    }

    Ok(())
}
