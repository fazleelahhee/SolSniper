//! CLI binary for testing direct-swap buy/sell on PumpSwap AMM pools.
//!
//! Usage:
//!   solsniper buy  <pool> <base_vault> <quote_vault> <creator> <mint> <sol_amount> [--slippage 3] [--cu-price 50000]
//!   solsniper sell <pool> <base_vault> <quote_vault> <creator> <mint> <token_amount> [--slippage 3] [--cu-price 50000]
//!   solsniper sell-all <pool> <base_vault> <quote_vault> <creator> <mint> [--slippage 3]
//!   solsniper price <base_vault> <quote_vault>
//!   solsniper find-pool <mint>
//!   solsniper fast-buy <mint> <sol_amount> [--slippage 10] [--cu-price 50000]
//!   solsniper fast-sell <mint> [amount] [--slippage 10] [--cu-price 50000]

use clap::{Parser, Subcommand};
use solana_sdk::signer::{keypair::Keypair, Signer};
use std::str::FromStr;

#[derive(Parser)]
#[command(name = "solsniper", about = "Direct PumpSwap AMM buy/sell on Solana")]
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
        pool: String,
        base_vault: String,
        quote_vault: String,
        creator: String,
        mint: String,
        sol_amount: f64,
        #[arg(long, default_value = "3.0")]
        slippage: f64,
        #[arg(long)]
        cu_price: Option<u64>,
    },

    /// Sell a specific amount of tokens for SOL
    Sell {
        pool: String,
        base_vault: String,
        quote_vault: String,
        creator: String,
        mint: String,
        token_amount: u64,
        #[arg(long, default_value = "3.0")]
        slippage: f64,
        #[arg(long)]
        cu_price: Option<u64>,
    },

    /// Sell ALL tokens of a given mint
    SellAll {
        pool: String,
        base_vault: String,
        quote_vault: String,
        creator: String,
        mint: String,
        #[arg(long, default_value = "5.0")]
        slippage: f64,
        #[arg(long)]
        cu_price: Option<u64>,
    },

    /// Read pool price from vault balances
    Price {
        base_vault: String,
        quote_vault: String,
    },

    /// Discover PumpSwap pool for a token mint
    FindPool {
        mint: String,
    },

    /// Fast buy via PumpPortal (correct accounts, our RPC)
    FastBuy {
        mint: String,
        sol_amount: f64,
        #[arg(long, default_value = "10.0")]
        slippage: f64,
        #[arg(long)]
        cu_price: Option<u64>,
    },

    /// Fast sell via PumpPortal (correct accounts, our RPC)
    FastSell {
        mint: String,
        #[arg(default_value = "100%")]
        amount: String,
        #[arg(long, default_value = "10.0")]
        slippage: f64,
        #[arg(long)]
        cu_price: Option<u64>,
    },
}

/// Get SOL received from a sell transaction by checking pre/post balances on-chain.
/// Polls getTransaction up to 5 times (4s apart) until confirmed.
async fn get_sell_sol_amount(
    http: &reqwest::Client,
    rpc_url: &str,
    signature: &str,
) -> Option<f64> {
    for _ in 0..5 {
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;

        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "getTransaction",
            "params": [signature, {"encoding": "jsonParsed", "maxSupportedTransactionVersion": 0}]
        });

        // Use continue (not ?) on transient errors — keep polling all 5 attempts
        let resp = match http.post(rpc_url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(10))
            .send().await {
                Ok(r) => r,
                Err(_) => continue,
            };

        let data: serde_json::Value = match resp.json().await {
            Ok(d) => d,
            Err(_) => continue,
        };

        let result = match data.get("result") {
            Some(r) if !r.is_null() => r,
            _ => continue, // not confirmed yet
        };

        // Check for on-chain error
        if let Some(err) = result.get("meta").and_then(|m| m.get("err")) {
            if !err.is_null() { return Some(0.0); }
        }

        let pre = result.pointer("/meta/preBalances/0").and_then(|v| v.as_u64());
        let post = result.pointer("/meta/postBalances/0").and_then(|v| v.as_u64());
        let fee = result.pointer("/meta/fee").and_then(|v| v.as_u64()).unwrap_or(0);

        if let (Some(pre), Some(post)) = (pre, post) {
            let sol = (post + fee).saturating_sub(pre) as f64 / 1e9;
            if sol > 0.0001 {
                return Some(sol);
            }
        }
    }
    None
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

    // Single HTTP client — reused across all operations (connection pooling)
    let http = solsniper::create_http_client();

    match cli.command {
        Commands::Buy {
            pool, base_vault, quote_vault, creator, mint, sol_amount, slippage, cu_price,
        } => {
            let wp = wallet_path.ok_or_else(|| anyhow::anyhow!("Wallet path required: --wallet or SOLANA_WALLET_PATH"))?;
            let keypair = load_keypair(&wp)?;
            println!("Wallet: {}", keypair.pubkey());
            println!("BUY: {} SOL on pool {} (slippage {}%)", sol_amount, &pool[..12.min(pool.len())], slippage);

            let result = solsniper::buy(
                &http, &rpc_url, &pool, &base_vault, &quote_vault, &creator,
                sol_amount, slippage, &keypair, &mint, cu_price,
            ).await?;

            let output = serde_json::json!({
                "success": true, "action": "buy",
                "signature": result.signature.to_string(), "confirmed": result.confirmed,
                "sol_amount": sol_amount, "pool": pool, "mint": mint,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::Sell {
            pool, base_vault, quote_vault, creator, mint, token_amount, slippage, cu_price,
        } => {
            let wp = wallet_path.ok_or_else(|| anyhow::anyhow!("Wallet path required: --wallet or SOLANA_WALLET_PATH"))?;
            let keypair = load_keypair(&wp)?;
            println!("Wallet: {}", keypair.pubkey());
            println!("SELL: {} tokens on pool {} (slippage {}%)", token_amount, &pool[..12.min(pool.len())], slippage);

            let result = solsniper::sell(
                &http, &rpc_url, &pool, &base_vault, &quote_vault, &creator,
                token_amount, slippage, &keypair, &mint, cu_price,
            ).await?;

            let output = serde_json::json!({
                "success": true, "action": "sell",
                "signature": result.signature.to_string(), "confirmed": result.confirmed,
                "token_amount": token_amount, "pool": pool, "mint": mint,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::SellAll {
            pool, base_vault, quote_vault, creator, mint, slippage, cu_price,
        } => {
            let wp = wallet_path.ok_or_else(|| anyhow::anyhow!("Wallet path required: --wallet or SOLANA_WALLET_PATH"))?;
            let keypair = load_keypair(&wp)?;
            println!("Wallet: {}", keypair.pubkey());
            println!("SELL ALL: mint {} on pool {} (slippage {}%)", &mint[..12.min(mint.len())], &pool[..12.min(pool.len())], slippage);

            let result = solsniper::sell_all(
                &http, &rpc_url, &pool, &base_vault, &quote_vault, &creator,
                slippage, &keypair, &mint, cu_price,
            ).await?;

            let output = serde_json::json!({
                "success": true, "action": "sell_all",
                "signature": result.signature.to_string(), "confirmed": result.confirmed,
                "pool": pool, "mint": mint,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::Price { base_vault, quote_vault } => {
            let bv = solana_sdk::pubkey::Pubkey::from_str(&base_vault)?;
            let qv = solana_sdk::pubkey::Pubkey::from_str(&quote_vault)?;

            let (base_raw, quote_raw) =
                solsniper::rpc::read_vault_balances(&http, &rpc_url, &bv, &qv).await?;

            let quote_sol = quote_raw as f64 / 1e9;
            let price_sol = quote_sol / (base_raw as f64 / 1e6);

            let output = serde_json::json!({
                "base_vault": base_vault, "quote_vault": quote_vault,
                "base_reserve_raw": base_raw, "quote_reserve_raw": quote_raw,
                "quote_sol": quote_sol, "price_sol_per_token": price_sol,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::FindPool { mint } => {
            println!("Searching for PumpSwap pool for mint {}...", &mint[..12.min(mint.len())]);
            let pool = solsniper::pool::find_pool(&http, &rpc_url, &mint).await?;

            let output = serde_json::json!({
                "pool_address": pool.pool_address, "base_vault": pool.base_vault,
                "quote_vault": pool.quote_vault, "coin_creator": pool.coin_creator,
                "base_mint": pool.base_mint, "price_sol": pool.price_sol,
                "quote_sol_in_pool": pool.quote_sol_in_pool,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::FastBuy { mint, sol_amount, slippage, cu_price } => {
            let wp = wallet_path.as_ref().ok_or_else(|| anyhow::anyhow!("Wallet path required"))?;
            let keypair = load_keypair(wp)?;
            println!("Wallet: {}", keypair.pubkey());
            println!("FAST BUY: {} SOL of {} (slippage {}%)", sol_amount, &mint[..12.min(mint.len())], slippage);

            let (sig, _tokens) = solsniper::pumpportal::buy(
                &http, &rpc_url, &keypair, &mint, sol_amount, slippage, cu_price,
            ).await?;

            let output = serde_json::json!({
                "action": "fast_buy", "mint": mint,
                "sol_amount": sol_amount, "signature": sig.to_string(), "success": true,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Commands::FastSell { mint, amount, slippage, cu_price } => {
            let wp = wallet_path.as_ref().ok_or_else(|| anyhow::anyhow!("Wallet path required"))?;
            let keypair = load_keypair(wp)?;
            let pubkey = keypair.pubkey();
            println!("Wallet: {}", pubkey);
            println!("FAST SELL: {} of {} (slippage {}%)", amount, &mint[..12.min(mint.len())], slippage);

            let sig = solsniper::pumpportal::sell(
                &http, &rpc_url, &keypair, &mint, &amount, slippage, cu_price,
            ).await?;

            // Get SOL amount received by checking on-chain balance change
            let sol_amount = get_sell_sol_amount(&http, &rpc_url, &sig.to_string()).await
                .unwrap_or(0.0);

            let output = serde_json::json!({
                "action": "fast_sell", "mint": mint,
                "signature": sig.to_string(), "success": true,
                "sol_amount": sol_amount,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }

    Ok(())
}
