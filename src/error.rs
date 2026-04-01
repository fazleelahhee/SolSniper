//! Error types for direct-swap.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SwapError {
    #[error("Invalid pubkey: {0}")]
    InvalidPubkey(String),

    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    #[error("Transaction not confirmed after {0} seconds")]
    NotConfirmed(u64),

    #[error("No token balance to sell for {0}")]
    NoTokenBalance(String),

    #[error("Failed to read GlobalConfig: {0}")]
    GlobalConfigRead(String),

    #[error("Failed to deserialize: {0}")]
    Deserialize(String),

    #[error("Signing failed: {0}")]
    SigningFailed(String),

    #[error("Pool not found for mint {0}")]
    PoolNotFound(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("{0}")]
    Other(String),
}

impl From<solana_sdk::pubkey::ParsePubkeyError> for SwapError {
    fn from(e: solana_sdk::pubkey::ParsePubkeyError) -> Self {
        SwapError::InvalidPubkey(e.to_string())
    }
}
