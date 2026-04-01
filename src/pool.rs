//! PumpSwap pool discovery and price reading.

use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

use crate::constants::*;
use crate::error::SwapError;
use crate::rpc;

/// Resolved PumpSwap pool information needed for swaps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolInfo {
    pub pool_address: String,
    pub base_vault: String,
    pub quote_vault: String,
    pub coin_creator: String,
    pub base_mint: String,
    pub price_sol: f64,
    pub quote_sol_in_pool: f64,
}

impl PoolInfo {
    /// Create a PoolInfo from known addresses (when pool data is already available).
    pub fn from_known(
        pool_address: &str,
        base_vault: &str,
        quote_vault: &str,
        coin_creator: &str,
        base_mint: &str,
    ) -> Self {
        Self {
            pool_address: pool_address.to_string(),
            base_vault: base_vault.to_string(),
            quote_vault: quote_vault.to_string(),
            coin_creator: coin_creator.to_string(),
            base_mint: base_mint.to_string(),
            price_sol: 0.0,
            quote_sol_in_pool: 0.0,
        }
    }

    /// Fetch current pool price from vault balances.
    /// price_sol = (quote_reserve / 1e9) / (base_reserve / 1e6)
    pub async fn refresh_price(
        &mut self,
        http: &reqwest::Client,
        rpc_url: &str,
    ) -> Result<f64, SwapError> {
        let base_vault_pk = Pubkey::from_str(&self.base_vault)?;
        let quote_vault_pk = Pubkey::from_str(&self.quote_vault)?;

        let (base_amount, quote_amount) =
            rpc::read_vault_balances(http, rpc_url, &base_vault_pk, &quote_vault_pk).await?;

        if base_amount == 0 {
            return Err(SwapError::Rpc("Base vault empty".into()));
        }

        let quote_sol = quote_amount as f64 / 1e9;
        let price_sol = quote_sol / (base_amount as f64 / 1e6);

        self.price_sol = price_sol;
        self.quote_sol_in_pool = quote_sol;

        Ok(price_sol)
    }
}

/// Discover a PumpSwap pool for a given token mint via `getProgramAccounts`.
///
/// Searches for pool accounts where the base_mint matches the given mint.
/// Returns the pool info with vault addresses extracted from the pool account data.
///
/// Pool account layout (relevant offsets):
/// - base_mint at offset 43 (32 bytes)
/// - base_vault at offset 139 (32 bytes)
/// - quote_vault at offset 171 (32 bytes)
pub async fn find_pool(
    http: &reqwest::Client,
    rpc_url: &str,
    mint: &str,
) -> Result<PoolInfo, SwapError> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getProgramAccounts",
        "params": [
            PUMPSWAP_PROGRAM,
            {
                "encoding": "base64",
                "filters": [{"memcmp": {"offset": 43, "bytes": mint}}]
            }
        ]
    });

    let resp = http
        .post(rpc_url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| SwapError::Rpc(format!("getProgramAccounts parse error: {e}")))?;

    let accounts = data["result"]
        .as_array()
        .ok_or_else(|| SwapError::PoolNotFound(mint.to_string()))?;

    if accounts.is_empty() {
        return Err(SwapError::PoolNotFound(mint.to_string()));
    }

    let account = &accounts[0];
    let pool_address = account["pubkey"]
        .as_str()
        .ok_or_else(|| SwapError::Rpc("No pubkey in pool account".into()))?
        .to_string();

    let account_data_b64 = account["account"]["data"][0]
        .as_str()
        .ok_or_else(|| SwapError::Rpc("No data in pool account".into()))?;

    use base64::Engine;
    let pool_bytes = base64::engine::general_purpose::STANDARD
        .decode(account_data_b64)
        .map_err(|e| SwapError::Rpc(format!("Pool data base64 decode: {e}")))?;

    if pool_bytes.len() < 243 {
        return Err(SwapError::Rpc(format!(
            "Pool account too short: {} bytes",
            pool_bytes.len()
        )));
    }

    let base_vault = bs58::encode(&pool_bytes[139..171]).into_string();
    let quote_vault = bs58::encode(&pool_bytes[171..203]).into_string();

    // Extract coin_creator from pool data if available
    // Pool layout: creator at offset 203 (32 bytes) -- may vary
    let coin_creator = if pool_bytes.len() >= 235 {
        bs58::encode(&pool_bytes[203..235]).into_string()
    } else {
        // Fallback: system program (unknown creator)
        SYSTEM_PROGRAM.to_string()
    };

    let mut pool_info = PoolInfo {
        pool_address,
        base_vault,
        quote_vault,
        coin_creator,
        base_mint: mint.to_string(),
        price_sol: 0.0,
        quote_sol_in_pool: 0.0,
    };

    // Try to read current price
    let _ = pool_info.refresh_price(http, rpc_url).await;

    Ok(pool_info)
}
