//! PumpSwap AMM program constants.

use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// PumpSwap AMM program ID.
pub const PUMPSWAP_PROGRAM: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";

/// Pump.fun bonding curve program (used for creator_vault PDA derivation).
pub const PUMP_PROGRAM: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

/// PumpSwap GlobalConfig account (stores protocol_fee_recipient).
pub const PUMPSWAP_GLOBAL_CONFIG: &str = "ADyA8hBd1ckwkgdC5fC4bKBS32sNeuHuJecF8GWqV7Zb";

/// Wrapped SOL mint address.
pub const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";

/// SPL Token program.
pub const SPL_TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

/// Token-2022 program.
pub const TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

/// Associated Token Program.
pub const ASSOC_TOKEN_PROGRAM: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

/// System program.
pub const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";

/// PumpSwap `buy_exact_quote_in` instruction discriminator.
/// Anchor discriminator for the buy_exact_quote_in method.
pub const PUMPSWAP_BUY_DISC: [u8; 8] = [198, 46, 21, 82, 180, 217, 232, 112];

/// PumpSwap `sell_exact_base_in` (sell) instruction discriminator.
pub const PUMPSWAP_SELL_DISC: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];

/// Default protocol fee recipient (fallback if GlobalConfig read fails).
pub const DEFAULT_PROTOCOL_FEE_RECIPIENT: &str = "CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM";

/// Default compute unit limit for swap transactions.
pub const DEFAULT_CU_LIMIT: u32 = 300_000;

/// Default compute unit price (priority fee in micro-lamports).
pub const DEFAULT_CU_PRICE: u64 = 50_000;

/// Parse a constant string into a Pubkey (panics on invalid -- only for known constants).
pub fn pubkey(s: &str) -> Pubkey {
    Pubkey::from_str(s).expect("invalid hardcoded pubkey constant")
}
