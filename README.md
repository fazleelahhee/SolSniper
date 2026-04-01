# SolSniper

> Lightning-fast direct on-chain swaps for PumpSwap AMM on Solana. No middleware, no APIs, pure on-chain execution.

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![Solana](https://img.shields.io/badge/solana-mainnet-purple.svg)](https://solana.com/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Why SolSniper?

Most Solana trading tools route through Jupiter, PumpPortal, or other APIs — adding **3-5 seconds** of latency. SolSniper builds swap transactions directly against PumpSwap AMM pools, achieving **~400ms** execution time.

| Method | Latency | Dependency |
|--------|---------|------------|
| Jupiter API | ~4-5s | External API |
| PumpPortal API | ~3-4s | External API |
| **SolSniper** | **~400ms** | Direct on-chain |

## Features

- **Direct PumpSwap AMM swaps** — no middleware, no external APIs
- **Buy & Sell** — full swap lifecycle with WSOL wrapping/unwrapping
- **On-chain price reading** — read vault balances for real-time pricing
- **Pool discovery** — find PumpSwap pools by token mint
- **Slippage protection** — constant-product formula for accurate min output
- **Priority fees** — configurable compute budget for faster inclusion
- **CLI & Library** — use as command-line tool or import as Rust crate

## Installation

### From source

```bash
git clone https://github.com/fazleelahhee/SolSniper.git
cd SolSniper
cargo build --release
```

The binary will be at `target/release/solsniper`.

### As a Rust dependency

```toml
[dependencies]
solsniper = { git = "https://github.com/fazleelahhee/SolSniper.git" }
```

## Quick Start

### Set environment variables

```bash
export SOLANA_RPC="https://api.mainnet-beta.solana.com"
export SOLANA_WALLET_PATH="/path/to/wallet.json"
```

### Check token price

```bash
solsniper price <BASE_VAULT> <QUOTE_VAULT>
```

```json
{
  "price_sol_per_token": 7.955e-8,
  "quote_sol": 37.89,
  "base_reserve_raw": 476309846172999,
  "quote_reserve_raw": 37890919484
}
```

### Find pool for a token

```bash
solsniper find-pool <TOKEN_MINT>
```

### Buy tokens

```bash
# Buy 0.05 SOL worth of tokens with 5% slippage
solsniper buy <POOL> <BASE_VAULT> <QUOTE_VAULT> 0.05 --slippage 5
```

### Sell tokens

```bash
# Sell specific amount
solsniper sell <POOL> <BASE_VAULT> <QUOTE_VAULT> <MINT> <TOKEN_AMOUNT>

# Sell ALL tokens of a mint
solsniper sell-all <POOL> <BASE_VAULT> <QUOTE_VAULT> <MINT>
```

## CLI Reference

```
solsniper [OPTIONS] <COMMAND>

Commands:
  buy        Buy tokens by spending SOL
  sell       Sell a specific amount of tokens for SOL
  sell-all   Sell ALL tokens of a given mint
  price      Read pool price from vault balances
  find-pool  Discover PumpSwap pool for a token mint

Options:
  --rpc <RPC>        RPC endpoint URL [env: SOLANA_RPC]
  --wallet <WALLET>  Wallet keypair JSON path [env: SOLANA_WALLET_PATH]
  -h, --help         Print help
```

## Library Usage

```rust
use solsniper::{buy, sell, sell_all, pool};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let rpc_url = "https://api.mainnet-beta.solana.com";
    let keypair = solana_sdk::signature::read_keypair_file("wallet.json")?;

    // Find pool for a token
    let pool_info = pool::find_pool(rpc_url, "TOKEN_MINT").await?;
    println!("Pool: {}", pool_info.pool_address);
    println!("Price: {} SOL/token", pool_info.price);

    // Buy 0.05 SOL worth of tokens
    let sig = buy(
        rpc_url, &keypair,
        &pool_info.pool_address,
        &pool_info.base_vault,
        &pool_info.quote_vault,
        0.05,       // SOL amount
        3.0,        // slippage %
        50_000,     // priority fee
        None,       // creator (auto-detect)
    ).await?;
    println!("Buy tx: {}", sig);

    Ok(())
}
```

## Architecture

```
SolSniper
├── src/
│   ├── lib.rs          — Public API exports
│   ├── constants.rs    — PumpSwap program IDs, discriminators
│   ├── error.rs        — Error types (thiserror)
│   ├── instruction.rs  — Swap instruction builders (buy/sell)
│   ├── pool.rs         — Pool discovery, vault price reading
│   ├── rpc.rs          — Transaction sending, confirmation
│   ├── swap.rs         — High-level buy/sell with WSOL handling
│   └── main.rs         — CLI binary
└── Cargo.toml
```

### How it works

1. **Pool Discovery** — Finds PumpSwap pool via `getProgramAccounts` filtered by mint
2. **Price Reading** — Reads base/quote vault SPL token balances (u64 LE at offset 64)
3. **Swap Instruction** — Builds PumpSwap `buy_exact_quote_in` or `sell` with 19 accounts
4. **WSOL Handling** — Wraps SOL before buy, unwraps after sell
5. **Transaction** — Signs and sends with `skipPreflight` + priority fee

### PumpSwap AMM Details

| Detail | Value |
|--------|-------|
| Program ID | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` |
| Buy discriminator | `[198, 46, 21, 82, 180, 217, 232, 112]` |
| Sell discriminator | `[51, 230, 133, 164, 1, 127, 131, 173]` |
| Pool layout | base_vault at offset 139, quote_vault at offset 171 |
| Price formula | `(quote_reserve / 1e9) / (base_reserve / 1e6)` |

## Supported RPC Providers

Works with any Solana RPC:

- [Helius](https://helius.dev) — recommended for `getProgramAccounts`
- [Alchemy](https://alchemy.com)
- [QuickNode](https://quicknode.com)
- [Triton](https://triton.one)
- Public RPC (rate limited)

## Requirements

- Rust 1.75+
- Solana wallet keypair (JSON format)
- Solana RPC endpoint

## Contributing

Contributions welcome! Please open an issue or PR.

1. Fork the repo
2. Create your feature branch (`git checkout -b feature/amazing`)
3. Commit changes (`git commit -m 'Add amazing feature'`)
4. Push (`git push origin feature/amazing`)
5. Open a Pull Request

## License

MIT License — see [LICENSE](LICENSE) for details.

## Disclaimer

This software is for educational and research purposes. Trading cryptocurrency involves significant risk. Use at your own risk.

---

Built with Rust and Solana.
