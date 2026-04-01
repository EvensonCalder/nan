use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NanError {
    #[error("{0}")]
    Message(String),
    #[error("failed to locate the home directory")]
    HomeDirectoryUnavailable,
    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write {path}: {source}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to create directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse JSON in {path}: {source}")]
    ParseJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to serialize JSON for {path}: {source}")]
    SerializeJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid data: {0}")]
    InvalidData(String),
}

impl NanError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}
