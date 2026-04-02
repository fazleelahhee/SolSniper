//! # direct-swap
//!
//! Direct on-chain buy/sell for PumpSwap AMM pools on Solana.
//! Programme C -- completely independent from any existing trading system.
//!
//! Builds raw swap instructions against the PumpSwap AMM program
//! (`pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA`), signs locally, and sends
//! via RPC with `skipPreflight` for maximum speed.

pub mod constants;
pub mod error;
pub mod instruction;
pub mod pool;
pub mod pumpportal;
pub mod swap;
pub mod rpc;

pub use error::SwapError;
pub use pool::PoolInfo;
pub use swap::{buy, sell, sell_all};

/// Create a shared HTTP client with connection pooling for maximum speed.
/// Reuse this client across all buy/sell/price calls.
pub fn create_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(5)
        .tcp_keepalive(Some(std::time::Duration::from_secs(30)))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}
