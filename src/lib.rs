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
