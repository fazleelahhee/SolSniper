//! CLI binary for testing direct-swap buy/sell on PumpSwap AMM pools.
//!
//! Usage:
//!   direct-swap buy  <pool> <base_vault> <quote_vault> <creator> <mint> <sol_amount> [--slippage 3] [--cu-price 50000]
//!   direct-swap sell <pool> <base_vault> <quote_vault> <creator> <mint> <token_amount> [--slippage 3] [--cu-price 50000]
//!   direct-swap sell-all <pool> <base_vault> <quote_vault> <creator> <mint> [--slippage 3]
//!   direct-swap price <base_vault> <quote_vault>
//!   direct-swap find-pool <mint>
//!
//! Environment variables:
//!   SOLANA_RPC          - RPC endpoint (default: Alchemy)
//!   SOLANA_WALLET_PATH  - Path to wallet keypair JSON file

use clap::{Parser, Subcommand};
use solana_sdk::signer::{keypair::Keypair, Signer};
use std::str::FromStr;

#[derive(Parser)]
#[command(name = "direct-swap", about = "Direct PumpSwap AMM buy/sell on Solana")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// RPC endpoint URL (overrides SOLANA_RPC env var)
    #[arg(long, global = true)]
    rpc: Option<String>,

    /// Wallet keypair JSON file path (overrides SOLANA_WALLET_PATH env var)
    #[arg(long, global = true)]
    wallet: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Buy tokens by spending SOL
    Buy {
        /// PumpSwap pool address
        pool: String,
        /// Pool base token vault
        base_vault: String,
        /// Pool quote (SOL) vault
        quote_vault: String,
        /// Token creator address
        creator: String,
        /// Token mint address
        mint: String,
        /// SOL amount to spend (e.g. 0.005)
        sol_amount: f64,
        /// Slippage tolerance in percent (default: 3.0)
        #[arg(long, default_value = "3.0")]
        slippage: f64,
        /// Compute unit price / priority fee in micro-lamports
        #[arg(long)]
        cu_price: Option<u64>,
    },

    /// Sell a specific amount of tokens for SOL
    Sell {
        /// PumpSwap pool address
        pool: String,
        /// Pool base token vault
        base_vault: String,
        /// Pool quote (SOL) vault
        quote_vault: String,
        /// Token creator address
        creator: String,
        /// Token mint address
        mint: String,
        /// Raw token amount to sell (with decimals, e.g. 1000000 = 1 token for 6-decimal)
        token_amount: u64,
        /// Slippage tolerance in percent (default: 3.0)
        #[arg(long, default_value = "3.0")]
        slippage: f64,
        /// Compute unit price
        #[arg(long)]
        cu_price: Option<u64>,
    },

    /// Sell ALL tokens of a given mint
    SellAll {
        /// PumpSwap pool address
        pool: String,
        /// Pool base token vault
        base_vault: String,
        /// Pool quote (SOL) vault
        quote_vault: String,
        /// Token creator address
        creator: String,
        /// Token mint address
        mint: String,
        /// Slippage tolerance in percent (default: 5.0)
        #[arg(long, default_value = "5.0")]
        slippage: f64,
        /// Compute unit price
        #[arg(long)]
        cu_price: Option<u64>,
    },

    /// Read pool price from vault balances
    Price {
        /// Pool base token vault
        base_vault: String,
        /// Pool quote (SOL) vault
        quote_vault: String,
    },

    /// Discover PumpSwap pool for a token mint
    FindPool {
        /// Token mint address
        mint: String,
    },
}

fn load_keypair(wallet_path: &str) -> anyhow::Result<Keypair> {
    let data = std::fs::read_to_string(wallet_path)?;
    let key_bytes: Vec<u8> = serde_json::from_str(&data)?;
    Keypair::try_from(key_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("Failed to create keypair: {}", e))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let rpc_url = cli
        .rpc
        .or_else(|| std::env::var("SOLANA_RPC").ok())
        .unwrap_or_else(|| {
            "https://solana-mainnet.g.alchemy.com/v2/Kzs0AKNhPE7sQWrwl3Lsx".to_string()
        });

    let wallet_path = cli
        .wallet
        .or_else(|| std::env::var("SOLANA_WALLET_PATH").ok());

    match cli.command {
        Commands::Buy {
            pool,
            base_vault,
            quote_vault,
            creator,
            mint,
            sol_amount,
            slippage,
            cu_price,
        } => {
            let wp = wallet_path.ok_or_else(|| {
                anyhow::anyhow!("Wallet path required: --wallet or SOLANA_WALLET_PATH")
            })?;
            let keypair = load_keypair(&wp)?;
            println!("Wallet: {}", keypair.pubkey());
            println!(
                "BUY: {} SOL on pool {} (slippage {}%)",
                sol_amount,
                &pool[..12.min(pool.len())],
                slippage
            );

            let result = solsniper::buy(
                &rpc_url,
                &pool,
                &base_vault,
                &quote_vault,
                &creator,
                sol_amount,
                slippage,
                &keypair,
                &mint,
                cu_price,
            )
            .await?;

            let output = serde_json::json!({
                "success": true,
                "action": "buy",
                "signature": result.signature.to_string(),
                "confirmed": result.confirmed,
                "sol_amount": sol_amount,
                "pool": pool,
                "mint": mint,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::Sell {
            pool,
            base_vault,
            quote_vault,
            creator,
            mint,
            token_amount,
            slippage,
            cu_price,
        } => {
            let wp = wallet_path.ok_or_else(|| {
                anyhow::anyhow!("Wallet path required: --wallet or SOLANA_WALLET_PATH")
            })?;
            let keypair = load_keypair(&wp)?;
            println!("Wallet: {}", keypair.pubkey());
            println!(
                "SELL: {} tokens on pool {} (slippage {}%)",
                token_amount,
                &pool[..12.min(pool.len())],
                slippage
            );

            let result = solsniper::sell(
                &rpc_url,
                &pool,
                &base_vault,
                &quote_vault,
                &creator,
                token_amount,
                slippage,
                &keypair,
                &mint,
                cu_price,
            )
            .await?;

            let output = serde_json::json!({
                "success": true,
                "action": "sell",
                "signature": result.signature.to_string(),
                "confirmed": result.confirmed,
                "token_amount": token_amount,
                "pool": pool,
                "mint": mint,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::SellAll {
            pool,
            base_vault,
            quote_vault,
            creator,
            mint,
            slippage,
            cu_price,
        } => {
            let wp = wallet_path.ok_or_else(|| {
                anyhow::anyhow!("Wallet path required: --wallet or SOLANA_WALLET_PATH")
            })?;
            let keypair = load_keypair(&wp)?;
            println!("Wallet: {}", keypair.pubkey());
            println!(
                "SELL ALL: mint {} on pool {} (slippage {}%)",
                &mint[..12.min(mint.len())],
                &pool[..12.min(pool.len())],
                slippage
            );

            let result = solsniper::sell_all(
                &rpc_url,
                &pool,
                &base_vault,
                &quote_vault,
                &creator,
                slippage,
                &keypair,
                &mint,
                cu_price,
            )
            .await?;

            let output = serde_json::json!({
                "success": true,
                "action": "sell_all",
                "signature": result.signature.to_string(),
                "confirmed": result.confirmed,
                "pool": pool,
                "mint": mint,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::Price {
            base_vault,
            quote_vault,
        } => {
            let http = reqwest::Client::new();
            let bv = solana_sdk::pubkey::Pubkey::from_str(&base_vault)?;
            let qv = solana_sdk::pubkey::Pubkey::from_str(&quote_vault)?;

            let (base_raw, quote_raw) =
                solsniper::rpc::read_vault_balances(&http, &rpc_url, &bv, &qv).await?;

            let quote_sol = quote_raw as f64 / 1e9;
            let price_sol = quote_sol / (base_raw as f64 / 1e6);

            let output = serde_json::json!({
                "base_vault": base_vault,
                "quote_vault": quote_vault,
                "base_reserve_raw": base_raw,
                "quote_reserve_raw": quote_raw,
                "quote_sol": quote_sol,
                "price_sol_per_token": price_sol,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::FindPool { mint } => {
            let http = reqwest::Client::new();
            println!("Searching for PumpSwap pool for mint {}...", &mint[..12.min(mint.len())]);

            let pool = solsniper::pool::find_pool(&http, &rpc_url, &mint).await?;

            let output = serde_json::json!({
                "pool_address": pool.pool_address,
                "base_vault": pool.base_vault,
                "quote_vault": pool.quote_vault,
                "coin_creator": pool.coin_creator,
                "base_mint": pool.base_mint,
                "price_sol": pool.price_sol,
                "quote_sol_in_pool": pool.quote_sol_in_pool,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }

    Ok(())
}
