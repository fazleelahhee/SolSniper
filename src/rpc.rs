//! RPC helpers -- sending transactions, reading accounts, getting blockhash.

use base64::Engine;
use serde_json::json;
use solana_sdk::{
    hash::Hash,
    pubkey::Pubkey,
    signature::Signature,
    transaction::VersionedTransaction,
};
use std::str::FromStr;

use crate::error::SwapError;

/// Send a signed VersionedTransaction via RPC with `skipPreflight` and `maxRetries`.
/// Returns the transaction signature.
pub async fn send_transaction(
    http: &reqwest::Client,
    rpc_url: &str,
    signed_tx: &VersionedTransaction,
) -> Result<Signature, SwapError> {
    let tx_bytes =
        bincode::serialize(signed_tx).map_err(|e| SwapError::Deserialize(e.to_string()))?;
    let tx_base64 = base64::engine::general_purpose::STANDARD.encode(&tx_bytes);

    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sendTransaction",
        "params": [
            tx_base64,
            {
                "encoding": "base64",
                "skipPreflight": true,
                "maxRetries": 3
            }
        ]
    });

    let resp = http
        .post(rpc_url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| SwapError::Rpc(format!("Failed to parse sendTransaction response: {e}")))?;

    if let Some(err) = data.get("error") {
        return Err(SwapError::Rpc(format!("sendTransaction error: {err}")));
    }

    let sig_str = data["result"]
        .as_str()
        .ok_or_else(|| SwapError::Rpc("No signature in sendTransaction response".into()))?;

    Signature::from_str(sig_str).map_err(|e| SwapError::Rpc(format!("Bad signature: {e}")))
}

/// Confirm a transaction by polling signature status.
/// Returns Ok(()) on confirmed/finalized, Err on timeout or on-chain failure.
pub async fn confirm_transaction(
    http: &reqwest::Client,
    rpc_url: &str,
    signature: &Signature,
    timeout_secs: u64,
) -> Result<(), SwapError> {
    for i in 0..timeout_secs {
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignatureStatuses",
            "params": [[signature.to_string()]]
        });

        let resp = http
            .post(rpc_url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await;

        let data: serde_json::Value = match resp {
            Ok(r) => match r.json().await {
                Ok(d) => d,
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        if let Some(statuses) = data["result"]["value"].as_array() {
            if let Some(Some(status)) = statuses.first().map(|s| {
                if s.is_null() {
                    None
                } else {
                    Some(s)
                }
            }) {
                // Check for on-chain error
                if status.get("err").is_some() && !status["err"].is_null() {
                    return Err(SwapError::TransactionFailed(format!(
                        "On-chain error: {}",
                        status["err"]
                    )));
                }

                let conf_status = status["confirmationStatus"].as_str().unwrap_or("");
                if conf_status == "confirmed" || conf_status == "finalized" {
                    return Ok(());
                }
            }
        }
    }

    Err(SwapError::NotConfirmed(timeout_secs))
}

/// Get the latest blockhash from RPC.
pub async fn get_latest_blockhash(
    http: &reqwest::Client,
    rpc_url: &str,
) -> Result<Hash, SwapError> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLatestBlockhash",
        "params": [{"commitment": "confirmed"}]
    });

    let resp = http
        .post(rpc_url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    let data: serde_json::Value = resp.json().await.map_err(|e| {
        SwapError::Rpc(format!("Failed to parse getLatestBlockhash response: {e}"))
    })?;

    let hash_str = data["result"]["value"]["blockhash"]
        .as_str()
        .ok_or_else(|| SwapError::Rpc("No blockhash in response".into()))?;

    Hash::from_str(hash_str).map_err(|e| SwapError::Rpc(format!("Bad blockhash: {e}")))
}

/// Read the protocol_fee_recipient from the PumpSwap GlobalConfig account.
/// Layout: Discriminator (8 bytes) + admin (32 bytes) + protocol_fee_recipient (32 bytes).
pub async fn read_protocol_fee_recipient(
    http: &reqwest::Client,
    rpc_url: &str,
    global_config: &Pubkey,
) -> Result<Pubkey, SwapError> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getAccountInfo",
        "params": [global_config.to_string(), {"encoding": "base64"}]
    });

    let resp = http
        .post(rpc_url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    let data: serde_json::Value = resp.json().await.map_err(|e| {
        SwapError::GlobalConfigRead(format!("Failed to parse getAccountInfo: {e}"))
    })?;

    let account_data = data["result"]["value"]["data"][0]
        .as_str()
        .ok_or_else(|| SwapError::GlobalConfigRead("No GlobalConfig account data".into()))?;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(account_data)
        .map_err(|e| SwapError::GlobalConfigRead(format!("Base64 decode failed: {e}")))?;

    // GlobalConfig layout: discriminator (8) + admin (32) = offset 40 for protocol_fee_recipient
    if bytes.len() < 72 {
        return Err(SwapError::GlobalConfigRead(format!(
            "GlobalConfig too short: {} bytes",
            bytes.len()
        )));
    }

    let fee_recipient_bytes: [u8; 32] = bytes[40..72]
        .try_into()
        .map_err(|_| SwapError::GlobalConfigRead("Slice conversion failed".into()))?;

    Ok(Pubkey::from(fee_recipient_bytes))
}

/// Read token balance from a user's ATA via RPC.
/// Returns raw token amount (no decimals applied).
pub async fn get_token_balance(
    http: &reqwest::Client,
    rpc_url: &str,
    ata: &Pubkey,
) -> Result<u64, SwapError> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getTokenAccountBalance",
        "params": [ata.to_string()]
    });

    let resp = http
        .post(rpc_url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| SwapError::Rpc(format!("Failed to parse getTokenAccountBalance: {e}")))?;

    // If account does not exist, return 0
    if data.get("error").is_some() {
        return Ok(0);
    }

    let amount_str = data["result"]["value"]["amount"]
        .as_str()
        .unwrap_or("0");

    amount_str
        .parse::<u64>()
        .map_err(|e| SwapError::Rpc(format!("Bad token amount: {e}")))
}

/// Read vault balances to compute pool price.
/// Returns (base_amount_raw, quote_amount_raw).
pub async fn read_vault_balances(
    http: &reqwest::Client,
    rpc_url: &str,
    base_vault: &Pubkey,
    quote_vault: &Pubkey,
) -> Result<(u64, u64), SwapError> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getMultipleAccounts",
        "params": [
            [base_vault.to_string(), quote_vault.to_string()],
            {"encoding": "base64"}
        ]
    });

    let resp = http
        .post(rpc_url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| SwapError::Rpc(format!("Failed to parse getMultipleAccounts: {e}")))?;

    let accounts = data["result"]["value"]
        .as_array()
        .ok_or_else(|| SwapError::Rpc("No accounts in getMultipleAccounts response".into()))?;

    if accounts.len() < 2 || accounts[0].is_null() || accounts[1].is_null() {
        return Err(SwapError::Rpc("Vault accounts not found".into()));
    }

    let base_amount = parse_spl_amount(&accounts[0])?;
    let quote_amount = parse_spl_amount(&accounts[1])?;

    Ok((base_amount, quote_amount))
}

/// Parse SPL token amount from a getMultipleAccounts result entry.
/// Token account data: amount is u64 LE at offset 64.
fn parse_spl_amount(account: &serde_json::Value) -> Result<u64, SwapError> {
    let data_b64 = account["data"][0]
        .as_str()
        .ok_or_else(|| SwapError::Rpc("No data in account".into()))?;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .map_err(|e| SwapError::Rpc(format!("Base64 decode failed: {e}")))?;

    if bytes.len() < 72 {
        return Err(SwapError::Rpc(format!(
            "Token account data too short: {} bytes",
            bytes.len()
        )));
    }

    // SPL token account layout: amount is u64 at offset 64
    let amount = u64::from_le_bytes(
        bytes[64..72]
            .try_into()
            .map_err(|_| SwapError::Rpc("Failed to parse token amount bytes".into()))?,
    );

    Ok(amount)
}
