//! Shared error types for the Rust reimplementation.

use thiserror::Error;

/// Common error type used by SDK modules.
#[derive(Debug, Error)]
pub enum GridError {
    /// Input is invalid or violates workload/deployment invariants.
    #[error("invalid input: {0}")]
    Validation(String),

    /// External/network dependency failed.
    #[error("backend error: {0}")]
    Backend(String),

    /// Data could not be parsed or serialized.
    #[error("serde error: {0}")]
    Codec(String),

    /// Requested resource or name was not found.
    #[error("not found: {0}")]
    NotFound(String),

    #[error("contract is deleted")]
    ContractDeleted,
}

impl GridError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn backend(msg: impl Into<String>) -> Self {
        Self::Backend(msg.into())
    }
}

impl From<serde_json::Error> for GridError {
    fn from(value: serde_json::Error) -> Self {
        Self::Codec(value.to_string())
    }
}

impl From<std::io::Error> for GridError {
    fn from(value: std::io::Error) -> Self {
        Self::Backend(value.to_string())
    }
}
