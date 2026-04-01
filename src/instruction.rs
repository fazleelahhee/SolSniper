//! Instruction builders for PumpSwap AMM swap, ATA creation, WSOL wrapping.

#[allow(deprecated)]
use solana_sdk::system_instruction;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    compute_budget::ComputeBudgetInstruction,
};

use crate::constants::*;

// ---------------------------------------------------------------------------
// PumpSwap swap instruction builders
// ---------------------------------------------------------------------------

/// Build the PumpSwap `buy_exact_quote_in` instruction.
///
/// Spends `lamports` of SOL (via wrapped SOL) to buy base tokens.
/// `min_base_amount_out` is the minimum tokens to receive (slippage protection).
#[allow(clippy::too_many_arguments)]
pub fn build_buy_instruction(
    pumpswap_program: &Pubkey,
    global_config: &Pubkey,
    pool: &Pubkey,
    user: &Pubkey,
    base_mint: &Pubkey,
    quote_mint: &Pubkey,
    user_base_ata: &Pubkey,
    user_quote_ata: &Pubkey,
    base_vault: &Pubkey,
    quote_vault: &Pubkey,
    protocol_fee_recipient: &Pubkey,
    creator_vault_authority: &Pubkey,
    creator_vault_ata: &Pubkey,
    event_authority: &Pubkey,
    coin_creator: &Pubkey,
    spl_token_program: &Pubkey,
    system_prog: &Pubkey,
    assoc_token_prog: &Pubkey,
    lamports: u64,
    min_base_amount_out: u64,
) -> Instruction {
    // [8 bytes disc] [8 bytes spendable_quote_in] [8 bytes min_base_amount_out] [1 byte track_volume]
    let mut ix_data = Vec::with_capacity(25);
    ix_data.extend_from_slice(&PUMPSWAP_BUY_DISC);
    ix_data.extend_from_slice(&lamports.to_le_bytes());
    ix_data.extend_from_slice(&min_base_amount_out.to_le_bytes());
    ix_data.push(0x00); // track_volume = None

    // Account ordering from real PumpSwap tx (verified on-chain)
    let accounts = vec![
        AccountMeta::new(*pool, false),                          // 0: pool
        AccountMeta::new(*user, true),                           // 1: user (signer)
        AccountMeta::new_readonly(*global_config, false),        // 2: global_config
        AccountMeta::new_readonly(*base_mint, false),            // 3: base_mint
        AccountMeta::new_readonly(*quote_mint, false),           // 4: quote_mint
        AccountMeta::new(*user_base_ata, false),                 // 5: user_base_token_account
        AccountMeta::new(*user_quote_ata, false),                // 6: user_quote_token_account
        AccountMeta::new(*base_vault, false),                    // 7: pool_base_token_account
        AccountMeta::new(*quote_vault, false),                   // 8: pool_quote_token_account
        AccountMeta::new(*protocol_fee_recipient, false),        // 9: protocol_fee_recipient
        AccountMeta::new(*creator_vault_authority, false),       // 10: coin_creator_vault_authority
        AccountMeta::new_readonly(pubkey(TOKEN_2022_PROGRAM), false),  // 11: token_program_2022
        AccountMeta::new_readonly(*spl_token_program, false),    // 12: token_program
        AccountMeta::new_readonly(*system_prog, false),          // 13: system_program
        AccountMeta::new_readonly(*assoc_token_prog, false),     // 14: associated_token_program
        AccountMeta::new_readonly(*event_authority, false),      // 15: event_authority
        AccountMeta::new_readonly(*pumpswap_program, false),     // 16: program (self-reference)
        AccountMeta::new_readonly(*coin_creator, false),         // 17: coin_creator
        AccountMeta::new(*creator_vault_ata, false),             // 18: creator_vault_ata
    ];

    Instruction {
        program_id: *pumpswap_program,
        accounts,
        data: ix_data,
    }
}

/// Build the PumpSwap sell instruction.
///
/// Sells `base_amount_in` raw tokens for SOL.
/// `min_quote_amount_out` is the minimum lamports to receive (slippage protection).
#[allow(clippy::too_many_arguments)]
pub fn build_sell_instruction(
    pumpswap_program: &Pubkey,
    global_config: &Pubkey,
    pool: &Pubkey,
    user: &Pubkey,
    base_mint: &Pubkey,
    quote_mint: &Pubkey,
    user_base_ata: &Pubkey,
    user_quote_ata: &Pubkey,
    base_vault: &Pubkey,
    quote_vault: &Pubkey,
    protocol_fee_recipient: &Pubkey,
    creator_vault_authority: &Pubkey,
    creator_vault_ata: &Pubkey,
    event_authority: &Pubkey,
    coin_creator: &Pubkey,
    spl_token_program: &Pubkey,
    system_prog: &Pubkey,
    assoc_token_prog: &Pubkey,
    base_amount_in: u64,
    min_quote_amount_out: u64,
) -> Instruction {
    // [8 bytes disc] [8 bytes base_amount_in] [8 bytes min_quote_amount_out]
    let mut ix_data = Vec::with_capacity(24);
    ix_data.extend_from_slice(&PUMPSWAP_SELL_DISC);
    ix_data.extend_from_slice(&base_amount_in.to_le_bytes());
    ix_data.extend_from_slice(&min_quote_amount_out.to_le_bytes());

    // Account ordering from real PumpSwap tx (same as buy)
    let accounts = vec![
        AccountMeta::new(*pool, false),                          // 0: pool
        AccountMeta::new(*user, true),                           // 1: user (signer)
        AccountMeta::new_readonly(*global_config, false),        // 2: global_config
        AccountMeta::new_readonly(*base_mint, false),            // 3: base_mint
        AccountMeta::new_readonly(*quote_mint, false),           // 4: quote_mint
        AccountMeta::new(*user_base_ata, false),                 // 5: user_base_token_account
        AccountMeta::new(*user_quote_ata, false),                // 6: user_quote_token_account
        AccountMeta::new(*base_vault, false),                    // 7: pool_base_token_account
        AccountMeta::new(*quote_vault, false),                   // 8: pool_quote_token_account
        AccountMeta::new(*protocol_fee_recipient, false),        // 9: protocol_fee_recipient
        AccountMeta::new(*creator_vault_authority, false),       // 10: coin_creator_vault_authority
        AccountMeta::new_readonly(pubkey(TOKEN_2022_PROGRAM), false),  // 11: token_program_2022
        AccountMeta::new_readonly(*spl_token_program, false),    // 12: token_program
        AccountMeta::new_readonly(*system_prog, false),          // 13: system_program
        AccountMeta::new_readonly(*assoc_token_prog, false),     // 14: associated_token_program
        AccountMeta::new_readonly(*event_authority, false),      // 15: event_authority
        AccountMeta::new_readonly(*pumpswap_program, false),     // 16: program (self-reference)
        AccountMeta::new_readonly(*coin_creator, false),         // 17: coin_creator
        AccountMeta::new(*creator_vault_ata, false),             // 18: creator_vault_ata
    ];

    Instruction {
        program_id: *pumpswap_program,
        accounts,
        data: ix_data,
    }
}

// ---------------------------------------------------------------------------
// SPL / system helper instructions
// ---------------------------------------------------------------------------

/// Create Associated Token Account (idempotent -- CreateIdempotent variant).
/// Will not fail if the ATA already exists.
pub fn create_ata_idempotent(
    payer: &Pubkey,
    owner: &Pubkey,
    mint: &Pubkey,
    token_program: &Pubkey,
) -> Instruction {
    let assoc_program = pubkey(ASSOC_TOKEN_PROGRAM);
    let system_prog = pubkey(SYSTEM_PROGRAM);
    let ata = derive_ata(owner, mint, token_program);

    Instruction {
        program_id: assoc_program,
        accounts: vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(*owner, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new_readonly(system_prog, false),
            AccountMeta::new_readonly(*token_program, false),
        ],
        data: vec![1], // CreateIdempotent = instruction index 1
    }
}

/// SyncNative instruction -- syncs native SOL balance for a WSOL account.
pub fn sync_native(account: &Pubkey, token_program: &Pubkey) -> Instruction {
    Instruction {
        program_id: *token_program,
        accounts: vec![AccountMeta::new(*account, false)],
        data: vec![17], // SyncNative = instruction index 17
    }
}

/// CloseAccount instruction -- closes a token account, returning rent + balance.
pub fn close_account(
    account: &Pubkey,
    destination: &Pubkey,
    owner: &Pubkey,
    token_program: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: *token_program,
        accounts: vec![
            AccountMeta::new(*account, false),
            AccountMeta::new(*destination, false),
            AccountMeta::new_readonly(*owner, true),
        ],
        data: vec![9], // CloseAccount = instruction index 9
    }
}

/// Transfer SOL (system_instruction::transfer wrapper, re-exported for convenience).
pub fn transfer_sol(from: &Pubkey, to: &Pubkey, lamports: u64) -> Instruction {
    system_instruction::transfer(from, to, lamports)
}

/// Compute budget: set CU limit.
pub fn set_compute_unit_limit(units: u32) -> Instruction {
    ComputeBudgetInstruction::set_compute_unit_limit(units)
}

/// Compute budget: set CU price (priority fee).
pub fn set_compute_unit_price(micro_lamports: u64) -> Instruction {
    ComputeBudgetInstruction::set_compute_unit_price(micro_lamports)
}

// ---------------------------------------------------------------------------
// PDA / ATA derivation
// ---------------------------------------------------------------------------

/// Derive an Associated Token Address.
pub fn derive_ata(wallet: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
    let assoc_program = pubkey(ASSOC_TOKEN_PROGRAM);
    let (ata, _) = Pubkey::find_program_address(
        &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
        &assoc_program,
    );
    ata
}

/// Derive creator_vault_authority PDA (under pump.fun program, NOT PumpSwap).
pub fn derive_creator_vault_authority(coin_creator: &Pubkey) -> Pubkey {
    let pump_program = pubkey(PUMP_PROGRAM);
    let (pda, _) = Pubkey::find_program_address(
        &[b"creator_vault", coin_creator.as_ref()],
        &pump_program,
    );
    pda
}

/// Derive event_authority PDA (under pump.fun program).
pub fn derive_event_authority() -> Pubkey {
    let pumpswap_program = pubkey(PUMPSWAP_PROGRAM);
    let (pda, _) = Pubkey::find_program_address(
        &[b"__event_authority"],
        &pumpswap_program,
    );
    pda
}
