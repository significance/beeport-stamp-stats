use thiserror::Error;

#[derive(Error, Debug)]
pub enum StampError {
    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = StampError::Rpc("connection timeout".to_string());
        assert_eq!(err.to_string(), "RPC error: connection timeout");

        let err = StampError::Contract("invalid signature".to_string());
        assert_eq!(err.to_string(), "Contract error: invalid signature");

        let err = StampError::Parse("invalid number".to_string());
        assert_eq!(err.to_string(), "Parse error: invalid number");
    }

    #[test]
    fn test_error_conversion() {
        // Test that std::io::Error converts to StampError::Io
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let stamp_err: StampError = io_err.into();
        assert!(matches!(stamp_err, StampError::Io(_)));

        // Test that serde_json::Error converts to StampError::Serialization
        let json_err = serde_json::from_str::<i32>("not a number").unwrap_err();
        let stamp_err: StampError = json_err.into();
        assert!(matches!(stamp_err, StampError::Serialization(_)));
    }
}
