//! Buy and sell functions -- the main public API.
//!
//! Each function builds a complete transaction with:
//! 1. Compute budget (priority fee)
//! 2. WSOL ATA creation (idempotent)
//! 3. SOL transfer + SyncNative (buy only)
//! 4. Base token ATA creation (idempotent, buy only)
//! 5. PumpSwap swap instruction
//! 6. Close WSOL ATA (unwrap remaining/received SOL)

use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
    signer::{keypair::Keypair, Signer},
    transaction::Transaction,
};
use std::str::FromStr;

use crate::constants::*;
use crate::error::SwapError;
use crate::instruction;
use crate::rpc;

/// Result of a swap operation.
#[derive(Debug)]
pub struct SwapResult {
    pub signature: Signature,
    pub confirmed: bool,
}

/// Buy tokens on a PumpSwap AMM pool.
///
/// Wraps SOL into WSOL, swaps for base tokens via the PumpSwap AMM,
/// then unwraps any remaining WSOL.
#[allow(clippy::too_many_arguments)]
pub async fn buy(
    http: &reqwest::Client,
    rpc_url: &str,
    pool_address: &str,
    base_vault: &str,
    quote_vault: &str,
    coin_creator: &str,
    amount_sol: f64,
    slippage_pct: f64,
    keypair: &Keypair,
    base_mint: &str,
    cu_price: Option<u64>,
) -> Result<SwapResult, SwapError> {
    let payer = keypair.pubkey();
    let lamports = (amount_sol * 1e9) as u64;

    // Parse all pubkeys
    let pool_pk = Pubkey::from_str(pool_address)?;
    let base_mint_pk = Pubkey::from_str(base_mint)?;
    let quote_mint_pk = pubkey(WSOL_MINT);
    let base_vault_pk = Pubkey::from_str(base_vault)?;
    let quote_vault_pk = Pubkey::from_str(quote_vault)?;
    let coin_creator_pk = Pubkey::from_str(coin_creator)?;
    let pumpswap_program = pubkey(PUMPSWAP_PROGRAM);
    let spl_token_program = pubkey(SPL_TOKEN_PROGRAM);
    let assoc_token_program = pubkey(ASSOC_TOKEN_PROGRAM);
    let system_program = pubkey(SYSTEM_PROGRAM);
    let global_config = pubkey(PUMPSWAP_GLOBAL_CONFIG);

    // Derive ATAs — base token uses Token-2022 for pump.fun tokens
    let token_2022 = pubkey(TOKEN_2022_PROGRAM);
    let user_base_ata = instruction::derive_ata(&payer, &base_mint_pk, &token_2022);
    let user_quote_ata = instruction::derive_ata(&payer, &quote_mint_pk, &spl_token_program);

    // Derive PDAs
    let creator_vault_authority = instruction::derive_creator_vault_authority(&coin_creator_pk);
    let creator_vault_ata =
        instruction::derive_ata(&creator_vault_authority, &quote_mint_pk, &spl_token_program);
    let event_authority = instruction::derive_event_authority();

    // Use known protocol fee recipient (skip slow GlobalConfig RPC read)
    let protocol_fee_recipient = pubkey(DEFAULT_PROTOCOL_FEE_RECIPIENT);

    // PARALLEL: Read vault balances AND get blockhash simultaneously
    let (vault_result, blockhash_result) = tokio::join!(
        rpc::read_vault_balances(http, rpc_url, &base_vault_pk, &quote_vault_pk),
        rpc::get_latest_blockhash(http, rpc_url)
    );

    let blockhash = blockhash_result?;

    let min_base_amount_out = match vault_result {
        Ok((base_reserve, quote_reserve)) if base_reserve > 0 && quote_reserve > 0 => {
            let expected =
                (base_reserve as u128 * lamports as u128) / (quote_reserve as u128 + lamports as u128);
            let slippage_factor = (100.0 - slippage_pct) / 100.0;
            (expected as f64 * slippage_factor) as u64
        }
        _ => 1u64,
    };

    // Build swap instruction
    let swap_ix = instruction::build_buy_instruction(
        &pumpswap_program,
        &global_config,
        &pool_pk,
        &payer,
        &base_mint_pk,
        &quote_mint_pk,
        &user_base_ata,
        &user_quote_ata,
        &base_vault_pk,
        &quote_vault_pk,
        &protocol_fee_recipient,
        &creator_vault_authority,
        &creator_vault_ata,
        &event_authority,
        &coin_creator_pk,
        &spl_token_program,
        &system_program,
        &assoc_token_program,
        lamports,
        min_base_amount_out,
    );

    // Assemble full transaction
    let priority = cu_price.unwrap_or(DEFAULT_CU_PRICE);
    let instructions = vec![
        instruction::set_compute_unit_limit(DEFAULT_CU_LIMIT),
        instruction::set_compute_unit_price(priority),
        instruction::create_ata_idempotent(&payer, &payer, &quote_mint_pk, &spl_token_program),
        instruction::transfer_sol(&payer, &user_quote_ata, lamports),
        instruction::sync_native(&user_quote_ata, &spl_token_program),
        instruction::create_ata_idempotent(&payer, &payer, &base_mint_pk, &pubkey(TOKEN_2022_PROGRAM)),
        swap_ix,
        instruction::close_account(&user_quote_ata, &payer, &payer, &spl_token_program),
    ];

    let tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer),
        &[keypair],
        blockhash,
    );

    let versioned = solana_sdk::transaction::VersionedTransaction::from(tx);
    let signature = rpc::send_transaction(http, rpc_url, &versioned).await?;

    // Try to confirm (non-blocking failure -- tx may still land)
    let confirmed = rpc::confirm_transaction(http, rpc_url, &signature, 30)
        .await
        .is_ok();

    Ok(SwapResult {
        signature,
        confirmed,
    })
}

/// Sell tokens on a PumpSwap AMM pool.
#[allow(clippy::too_many_arguments)]
pub async fn sell(
    http: &reqwest::Client,
    rpc_url: &str,
    pool_address: &str,
    base_vault: &str,
    quote_vault: &str,
    coin_creator: &str,
    token_amount: u64,
    slippage_pct: f64,
    keypair: &Keypair,
    base_mint: &str,
    cu_price: Option<u64>,
) -> Result<SwapResult, SwapError> {
    let payer = keypair.pubkey();

    if token_amount == 0 {
        return Err(SwapError::NoTokenBalance(base_mint.to_string()));
    }

    // Parse all pubkeys
    let pool_pk = Pubkey::from_str(pool_address)?;
    let base_mint_pk = Pubkey::from_str(base_mint)?;
    let quote_mint_pk = pubkey(WSOL_MINT);
    let base_vault_pk = Pubkey::from_str(base_vault)?;
    let quote_vault_pk = Pubkey::from_str(quote_vault)?;
    let coin_creator_pk = Pubkey::from_str(coin_creator)?;
    let pumpswap_program = pubkey(PUMPSWAP_PROGRAM);
    let spl_token_program = pubkey(SPL_TOKEN_PROGRAM);
    let assoc_token_program = pubkey(ASSOC_TOKEN_PROGRAM);
    let system_program = pubkey(SYSTEM_PROGRAM);
    let global_config = pubkey(PUMPSWAP_GLOBAL_CONFIG);

    // Derive ATAs
    let token_2022 = pubkey(TOKEN_2022_PROGRAM);
    let user_base_ata = instruction::derive_ata(&payer, &base_mint_pk, &token_2022);
    let user_quote_ata = instruction::derive_ata(&payer, &quote_mint_pk, &spl_token_program);

    // Derive PDAs
    let creator_vault_authority = instruction::derive_creator_vault_authority(&coin_creator_pk);
    let creator_vault_ata =
        instruction::derive_ata(&creator_vault_authority, &quote_mint_pk, &spl_token_program);
    let event_authority = instruction::derive_event_authority();

    // PARALLEL: Read protocol fee, vault balances, and blockhash simultaneously
    let (fee_result, vault_result, blockhash_result) = tokio::join!(
        rpc::read_protocol_fee_recipient(http, rpc_url, &global_config),
        rpc::read_vault_balances(http, rpc_url, &base_vault_pk, &quote_vault_pk),
        rpc::get_latest_blockhash(http, rpc_url)
    );

    let blockhash = blockhash_result?;
    let protocol_fee_recipient = fee_result.unwrap_or_else(|_| pubkey(DEFAULT_PROTOCOL_FEE_RECIPIENT));

    let min_quote_amount_out = match vault_result {
        Ok((base_reserve, quote_reserve)) if base_reserve > 0 && quote_reserve > 0 => {
            let expected = (quote_reserve as u128 * token_amount as u128)
                / (base_reserve as u128 + token_amount as u128);
            let slippage_factor = (100.0 - slippage_pct) / 100.0;
            (expected as f64 * slippage_factor) as u64
        }
        _ => 0u64,
    };

    // Build sell instruction
    let swap_ix = instruction::build_sell_instruction(
        &pumpswap_program,
        &global_config,
        &pool_pk,
        &payer,
        &base_mint_pk,
        &quote_mint_pk,
        &user_base_ata,
        &user_quote_ata,
        &base_vault_pk,
        &quote_vault_pk,
        &protocol_fee_recipient,
        &creator_vault_authority,
        &creator_vault_ata,
        &event_authority,
        &coin_creator_pk,
        &spl_token_program,
        &system_program,
        &assoc_token_program,
        token_amount,
        min_quote_amount_out,
    );

    // Assemble transaction
    let priority = cu_price.unwrap_or(DEFAULT_CU_PRICE);
    let instructions = vec![
        instruction::set_compute_unit_limit(DEFAULT_CU_LIMIT),
        instruction::set_compute_unit_price(priority),
        instruction::create_ata_idempotent(&payer, &payer, &quote_mint_pk, &spl_token_program),
        swap_ix,
        instruction::close_account(&user_quote_ata, &payer, &payer, &spl_token_program),
    ];

    let tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer),
        &[keypair],
        blockhash,
    );

    let versioned = solana_sdk::transaction::VersionedTransaction::from(tx);
    let signature = rpc::send_transaction(http, rpc_url, &versioned).await?;

    let confirmed = rpc::confirm_transaction(http, rpc_url, &signature, 30)
        .await
        .is_ok();

    Ok(SwapResult {
        signature,
        confirmed,
    })
}

/// Sell ALL tokens of a given mint from the wallet.
#[allow(clippy::too_many_arguments)]
pub async fn sell_all(
    http: &reqwest::Client,
    rpc_url: &str,
    pool_address: &str,
    base_vault: &str,
    quote_vault: &str,
    coin_creator: &str,
    slippage_pct: f64,
    keypair: &Keypair,
    base_mint: &str,
    cu_price: Option<u64>,
) -> Result<SwapResult, SwapError> {
    let payer = keypair.pubkey();
    let spl_token_program = pubkey(SPL_TOKEN_PROGRAM);
    let base_mint_pk = Pubkey::from_str(base_mint)?;
    let user_ata = instruction::derive_ata(&payer, &base_mint_pk, &spl_token_program);

    let balance = rpc::get_token_balance(http, rpc_url, &user_ata).await?;
    if balance == 0 {
        return Err(SwapError::NoTokenBalance(base_mint.to_string()));
    }

    sell(
        http,
        rpc_url,
        pool_address,
        base_vault,
        quote_vault,
        coin_creator,
        balance,
        slippage_pct,
        keypair,
        base_mint,
        cu_price,
    )
    .await
}
