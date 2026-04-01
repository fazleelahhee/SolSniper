//! PumpPortal-assisted swap — uses PumpPortal API to build correct tx,
//! then signs locally and sends directly to RPC (faster than PumpPortal's pipeline).
//!
//! This is the pragmatic approach: PumpPortal handles the complex account
//! discovery (PDAs, fee configs, volume accumulators), we handle the speed.

use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
    signer::{keypair::Keypair, Signer},
};
use crate::error::SwapError;

const PUMPPORTAL_API: &str = "https://pumpportal.fun/api/trade-local";

/// Buy tokens via PumpPortal-built transaction.
/// PumpPortal builds the tx with correct accounts, we sign and send to our own RPC.
pub async fn buy(
    rpc_url: &str,
    keypair: &Keypair,
    mint: &str,
    amount_sol: f64,
    slippage_pct: f64,
    cu_price: Option<u64>,
) -> Result<(Signature, u64), SwapError> {
    let http = reqwest::Client::new();
    let payer = keypair.pubkey();

    // Ask PumpPortal to build the tx
    let body = serde_json::json!({
        "publicKey": payer.to_string(),
        "action": "buy",
        "mint": mint,
        "amount": amount_sol,
        "denominatedInSol": "true",
        "slippage": slippage_pct as u32,
        "priorityFee": cu_price.map(|p| p as f64 / 1e9).unwrap_or(0.0005),
        "pool": "pump-amm",
    });

    let resp = http.post(PUMPPORTAL_API)
        .json(&body)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| SwapError::Rpc(format!("PumpPortal request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(SwapError::Rpc(format!("PumpPortal HTTP {}: {}", status, &text[..text.len().min(200)])));
    }

    let tx_bytes = resp.bytes().await
        .map_err(|e| SwapError::Rpc(format!("Failed to read PumpPortal response: {}", e)))?;

    // Sign the tx
    use solana_sdk::transaction::VersionedTransaction;
    let tx = bincode::deserialize::<VersionedTransaction>(&tx_bytes)
        .map_err(|e| SwapError::Rpc(format!("Failed to parse PumpPortal tx: {}", e)))?;
    let signed = VersionedTransaction::try_new(tx.message, &[keypair as &dyn solana_sdk::signer::Signer])
        .map_err(|e| SwapError::Rpc(format!("Failed to sign tx: {}", e)))?;

    // Send directly to our RPC (faster than PumpPortal's default RPC)
    let sig = crate::rpc::send_transaction(&http, rpc_url, &signed).await?;

    Ok((sig, 0)) // token amount unknown until confirmed
}

/// Sell tokens via PumpPortal-built transaction.
pub async fn sell(
    rpc_url: &str,
    keypair: &Keypair,
    mint: &str,
    amount_pct: &str, // "100%" for sell all, or token amount
    slippage_pct: f64,
    cu_price: Option<u64>,
) -> Result<Signature, SwapError> {
    let http = reqwest::Client::new();
    let payer = keypair.pubkey();

    let body = serde_json::json!({
        "publicKey": payer.to_string(),
        "action": "sell",
        "mint": mint,
        "amount": amount_pct,
        "denominatedInSol": "false",
        "slippage": slippage_pct as u32,
        "priorityFee": cu_price.map(|p| p as f64 / 1e9).unwrap_or(0.0005),
        "pool": "pump-amm",
    });

    let resp = http.post(PUMPPORTAL_API)
        .json(&body)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| SwapError::Rpc(format!("PumpPortal request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(SwapError::Rpc(format!("PumpPortal HTTP {}: {}", status, &text[..text.len().min(200)])));
    }

    let tx_bytes = resp.bytes().await
        .map_err(|e| SwapError::Rpc(format!("Failed to read PumpPortal response: {}", e)))?;

    use solana_sdk::transaction::VersionedTransaction;
    let tx = bincode::deserialize::<VersionedTransaction>(&tx_bytes)
        .map_err(|e| SwapError::Rpc(format!("Failed to parse PumpPortal tx: {}", e)))?;
    let signed = VersionedTransaction::try_new(tx.message, &[keypair as &dyn solana_sdk::signer::Signer])
        .map_err(|e| SwapError::Rpc(format!("Failed to sign tx: {}", e)))?;

    let sig = crate::rpc::send_transaction(&http, rpc_url, &signed).await?;

    Ok(sig)
}
