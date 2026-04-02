//! RPC helpers -- sending transactions, reading accounts, getting blockhash.

use base64::Engine;
use serde_json::json;
use solana_sdk::{
    hash::Hash,
    pubkey::Pubkey,
    signature::Signature,
    transaction::VersionedTransaction,
};
use std::{str::FromStr, time::Duration};

use crate::error::SwapError;

#[derive(Debug)]
struct RpcSendOutcome {
    rpc_url: String,
    result: Result<Signature, String>,
}

fn broadcast_rpc_urls(primary_rpc_url: &str) -> Vec<String> {
    let mut urls = vec![primary_rpc_url.to_string()];

    if let Ok(extra) = std::env::var("SOLSNIPER_BROADCAST_RPCS") {
        for rpc in extra.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if !urls.iter().any(|existing| existing == rpc) {
                urls.push(rpc.to_string());
            }
        }
    }

    urls
}

fn is_duplicate_or_already_processed(error: &str) -> bool {
    let lowered = error.to_ascii_lowercase();
    lowered.contains("already processed")
        || lowered.contains("already been processed")
        || lowered.contains("duplicate")
}

async fn send_transaction_to_rpc(
    http: reqwest::Client,
    rpc_url: String,
    tx_base64: String,
    expected_signature: Signature,
) -> RpcSendOutcome {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sendTransaction",
        "params": [
            tx_base64,
            {
                "encoding": "base64",
                "skipPreflight": true,
                "maxRetries": 0
            }
        ]
    });

    let result = async {
        let resp = http
            .post(&rpc_url)
            .json(&body)
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .map_err(|e| format!("HTTP send failed: {e}"))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse sendTransaction response: {e}"))?;

        if let Some(err) = data.get("error") {
            let err_str = err.to_string();
            if is_duplicate_or_already_processed(&err_str) {
                return Ok(expected_signature);
            }
            return Err(format!("sendTransaction error: {err_str}"));
        }

        let sig_str = data["result"]
            .as_str()
            .ok_or_else(|| "No signature in sendTransaction response".to_string())?;

        let sig =
            Signature::from_str(sig_str).map_err(|e| format!("Bad signature from RPC: {e}"))?;

        if sig != expected_signature {
            return Err(format!(
                "RPC returned unexpected signature {sig}, expected {expected_signature}"
            ));
        }

        Ok(sig)
    }
    .await;

    RpcSendOutcome { rpc_url, result }
}

async fn get_signature_status_from_rpc(
    http: reqwest::Client,
    rpc_url: String,
    signature: Signature,
) -> Result<Option<serde_json::Value>, String> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getSignatureStatuses",
        "params": [[signature.to_string()]]
    });

    let resp = http
        .post(&rpc_url)
        .json(&body)
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .map_err(|e| format!("{rpc_url} => HTTP send failed: {e}"))?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("{rpc_url} => Failed to parse getSignatureStatuses response: {e}"))?;

    let maybe_status = data["result"]["value"]
        .as_array()
        .and_then(|statuses| statuses.first())
        .filter(|status| !status.is_null())
        .cloned();

    Ok(maybe_status)
}

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
    let expected_signature = signed_tx
        .signatures
        .first()
        .copied()
        .ok_or_else(|| SwapError::SigningFailed("Signed transaction has no signatures".into()))?;
    let rpc_urls = broadcast_rpc_urls(rpc_url);

    let mut join_set = tokio::task::JoinSet::new();
    for rpc in rpc_urls.iter().cloned() {
        join_set.spawn(send_transaction_to_rpc(
            http.clone(),
            rpc,
            tx_base64.clone(),
            expected_signature,
        ));
    }

    let mut failures = Vec::new();
    while let Some(joined) = join_set.join_next().await {
        match joined {
            Ok(RpcSendOutcome {
                result: Ok(signature), ..
            }) => return Ok(signature),
            Ok(RpcSendOutcome {
                rpc_url,
                result: Err(err),
            }) => failures.push(format!("{rpc_url} => {err}")),
            Err(err) => failures.push(format!("tokio task join error => {err}")),
        }
    }

    Err(SwapError::Rpc(format!(
        "all RPC broadcasts failed for {expected_signature}: {}",
        failures.join(" | ")
    )))
}

/// Confirm a transaction by polling signature status.
/// Returns Ok(()) on confirmed/finalized, Err on timeout or on-chain failure.
pub async fn confirm_transaction(
    http: &reqwest::Client,
    rpc_url: &str,
    signature: &Signature,
    timeout_secs: u64,
) -> Result<(), SwapError> {
    let rpc_urls = broadcast_rpc_urls(rpc_url);

    for i in 0..timeout_secs {
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        let mut join_set = tokio::task::JoinSet::new();
        for rpc in rpc_urls.iter().cloned() {
            join_set.spawn(get_signature_status_from_rpc(
                http.clone(),
                rpc,
                *signature,
            ));
        }

        while let Some(joined) = join_set.join_next().await {
            let maybe_status = match joined {
                Ok(Ok(status)) => status,
                _ => continue,
            };

            let Some(status) = maybe_status else {
                continue;
            };

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

    // GlobalConfig layout: protocol_fee_recipient at offset 217 (verified on-chain)
    if bytes.len() < 249 {
        return Err(SwapError::GlobalConfigRead(format!(
            "GlobalConfig too short: {} bytes",
            bytes.len()
        )));
    }

    let fee_recipient_bytes: [u8; 32] = bytes[217..249]
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
