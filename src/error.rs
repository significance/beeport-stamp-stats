use thiserror::Error;

#[derive(Error, Debug)]
pub enum StampError {
    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Contract error: {0}")]
    Contract(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
}

pub type Result<T> = std::result::Result<T, StampError>;
