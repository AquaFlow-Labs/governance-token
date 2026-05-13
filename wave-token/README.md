# wave-token

A Rust client for interacting with the **WaveToken** ERC-20 smart contract — the governance token powering the Drips Wave streaming protocol. Built with [`alloy`](https://github.com/alloy-rs/alloy), the modern Rust Ethereum library.

> **WAVE** is the value unit streamed through the Drips Network. This crate gives you typed, compile-time-safe access to every function on the deployed contract — reads, writes, and future Drips Hub integration.

---

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [How It Works](#how-it-works)
- [Prerequisites](#prerequisites)
- [Setup](#setup)
- [Run](#run)
- [Code Walkthrough](#code-walkthrough)
- [The Solidity Contract](#the-solidity-contract-reference)
- [ABI Reference](#abi-reference)
- [Token Economics](#token-economics)
- [Testing Locally with Anvil](#testing-locally-with-anvil)
- [Extending the Client](#extending-the-client)
- [Drips Integration Context](#drips-integration-context)
- [Security Considerations](#security-considerations)
- [Dependencies](#dependencies)
- [Contributing](#contributing)
- [License](#license)

---

## Overview

WaveToken (`WAVE`) is an ERC-20 governance token deployed on EVM-compatible chains. It serves as the `tokenAddress` in Drips Network `setStreams` calls, enabling continuous, programmable value streaming between addresses at a per-second rate.

This Rust crate provides:
- Typed, compile-time-safe ABI bindings via `alloy::sol!`
- Read operations: `name`, `symbol`, `totalSupply`, `balanceOf`
- Write operations: `mint` (owner-only, waits for on-chain confirmation)
- Wallet-backed provider with nonce, gas, and chain-ID management built in
- A clean foundation to extend with Drips Hub `setStreams` calls

The crate is intentionally minimal — it does one thing well: give you a safe, ergonomic Rust interface to the `WaveToken` contract without boilerplate.

---

## Architecture

### Project Structure

```
wave-token/
├── Cargo.toml              # Dependencies: alloy, tokio, dotenvy, eyre
├── .env.example            # Environment variable template
├── abi/
│   └── WaveToken.json      # Contract ABI (ERC-20 + mint + owner)
└── src/
    ├── main.rs             # Entry point — loads env, builds provider, prints token info
    └── token.rs            # ABI bindings + all contract interaction functions
```

### Component Responsibilities

| File | Responsibility |
|---|---|
| `Cargo.toml` | Declares all dependencies with pinned versions |
| `.env.example` | Documents required environment variables |
| `abi/WaveToken.json` | Source of truth for the contract interface; consumed by `sol!` at compile time |
| `src/main.rs` | Wires together env loading, provider construction, and example calls |
| `src/token.rs` | All contract logic — bindings, provider factory, typed call wrappers |

### Data Flow

```
.env
 └─ RPC_URL + PRIVATE_KEY + CONTRACT_ADDRESS
        │
        ▼
  PrivateKeySigner          (parses hex private key → signing key)
        │
        ▼
  EthereumWallet            (wraps signer for transaction signing)
        │
        ▼
  ProviderBuilder
  ├── NonceFiller           (auto-increments nonce per tx)
  ├── GasFiller             (estimates gas, sets EIP-1559 fields)
  └── ChainIdFiller         (fetches chain ID once, attaches to all txs)
        │
        ▼
  WaveToken::new(addr, provider)   ◄──── abi/WaveToken.json
        │                                (sol! macro at compile time)
   ┌────┴──────────────────────────┐
   │                               │
 view calls (.call())          send calls (.send() → .watch())
 ─────────────────              ──────────────────────────────
 name()                         mint(to, amount)
 symbol()                       transfer(to, amount)
 decimals()
 totalSupply()
 balanceOf(account)
 owner()
```

### Layer Diagram

```
┌─────────────────────────────────────────────┐
│                  main.rs                    │  ← application layer
│  (env loading, provider wiring, CLI output) │
└────────────────────┬────────────────────────┘
                     │ calls
┌────────────────────▼────────────────────────┐
│                  token.rs                   │  ← contract interface layer
│  (sol! bindings, typed wrappers, provider   │
│   factory, address parsing)                 │
└────────────────────┬────────────────────────┘
                     │ uses
┌────────────────────▼────────────────────────┐
│              alloy provider stack           │  ← transport layer
│  (HTTP/WS RPC, fillers, wallet, signing)    │
└────────────────────┬────────────────────────┘
                     │ JSON-RPC
┌────────────────────▼────────────────────────┐
│           EVM Node (Infura / Anvil)         │  ← network layer
│       WaveToken contract on-chain           │
└─────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Reason |
|---|---|
| `alloy::sol!` macro | Generates fully typed Rust structs from the ABI at compile time — no runtime ABI parsing, no stringly-typed calls, compiler catches interface mismatches |
| `with_recommended_fillers()` | Automatically handles nonce management, gas estimation, and chain ID — no manual setup required |
| `impl Provider + Clone` return type | Keeps the provider generic without boxing; avoids `dyn` overhead and preserves zero-cost abstraction |
| `eyre` for errors | Ergonomic error propagation with context; compatible with `?` throughout async code; easy to attach `.wrap_err()` messages |
| Pinned dependency versions | Reproducible builds; avoids surprise breakage from upstream semver bumps |
| `dotenvy` over `std::env` | Loads `.env` files automatically in development without changing production behavior |

---

## How It Works

### The `sol!` Macro

`alloy::sol!` is the core of this crate. It reads `abi/WaveToken.json` at **compile time** and generates a Rust module called `WaveToken` containing:

- A `WaveTokenInstance<P>` struct bound to a provider
- One method per ABI function, with Rust-native argument and return types
- `Call` and `SolCall` trait implementations for each function
- Proper handling of `view` vs `nonpayable` — view functions use `.call()`, state-changing functions use `.send()`

This means if the ABI changes and you update the JSON, the compiler will immediately flag every call site that no longer matches. No runtime surprises.

### Transaction Lifecycle

When you call `mint(to, amount)`:

1. `alloy` encodes the calldata using the ABI (function selector + ABI-encoded arguments)
2. `NonceFiller` fetches the current nonce for your address from the node
3. `GasFiller` calls `eth_estimateGas` and `eth_feeHistory` to set `maxFeePerGas` and `maxPriorityFeePerGas`
4. `ChainIdFiller` attaches the chain ID (fetched once on first use)
5. `EthereumWallet` signs the fully-formed transaction with your private key
6. The signed transaction is submitted via `eth_sendRawTransaction`
7. `.watch()` polls `eth_getTransactionReceipt` until the transaction is included in a block
8. The function returns `Ok(())` once confirmed

### View Call Lifecycle

When you call `total_supply()`:

1. `alloy` encodes the calldata (just the 4-byte function selector for `totalSupply()`)
2. The provider sends an `eth_call` — no signing, no gas cost, no state change
3. The return value is ABI-decoded into a Rust `U256`
4. The `._0` field accesses the first (and only) return value from the tuple

---

## Prerequisites

- **Rust 1.75+** (stable) — install via [rustup](https://rustup.rs)
- **A funded wallet** — the private key of an account that owns the deployed contract (for `mint`) or any account (for reads)
- **An RPC endpoint** — Infura, Alchemy, QuickNode, or a local Anvil node
- **A deployed `WaveToken` contract** — the address goes in `CONTRACT_ADDRESS`

To check your Rust version:

```bash
rustc --version
```

---

## Setup

```bash
git clone <repo>
cd wave-token

cp .env.example .env
```

Edit `.env` with your values:

```env
RPC_URL=https://mainnet.infura.io/v3/YOUR_KEY
CONTRACT_ADDRESS=0xYourWaveTokenAddress
PRIVATE_KEY=0xYourPrivateKey
```

> **Never commit `.env` to version control.** It is already in `.gitignore` by convention. Only commit `.env.example`.

### RPC URL options

| Provider | Free tier | URL format |
|---|---|---|
| Infura | 100k req/day | `https://mainnet.infura.io/v3/<KEY>` |
| Alchemy | 300M compute units/month | `https://eth-mainnet.g.alchemy.com/v2/<KEY>` |
| Anvil (local) | unlimited | `http://127.0.0.1:8545` |

---

## Run

```bash
cargo run
```

Expected output:

```
name:         Wave Governance
symbol:       WAVE
total supply: 1000000000000000000000000
```

To build a release binary:

```bash
cargo build --release
./target/release/wave-token
```

---

## Code Walkthrough

### Entry Point — `src/main.rs`

`main.rs` is intentionally thin. It owns three responsibilities: load environment variables, construct the provider, and demonstrate the public API.

```rust
mod token;

use eyre::Result;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();  // loads .env if present; silently skips if missing

    let rpc_url = std::env::var("RPC_URL")?;
    let contract_addr = std::env::var("CONTRACT_ADDRESS")?;
    let private_key = std::env::var("PRIVATE_KEY")?;

    let client = token::build_client(&rpc_url, &private_key).await?;

    println!("name:         {}", token::name(&client, &contract_addr).await?);
    println!("symbol:       {}", token::symbol(&client, &contract_addr).await?);
    println!("total supply: {}", token::total_supply(&client, &contract_addr).await?);

    Ok(())
}
```

`dotenvy::dotenv().ok()` — the `.ok()` intentionally discards the error. In production (where env vars are injected by the runtime), there is no `.env` file and that is fine.

### ABI Bindings — `src/token.rs`

```rust
sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    WaveToken,
    "abi/WaveToken.json"
);
```

This single macro call generates the entire typed interface. The `#[sol(rpc)]` attribute is what enables `.call()` and `.send()` on the generated methods — without it you only get encoding/decoding helpers.

### Building the Provider

```rust
pub async fn build_client(rpc_url: &str, private_key: &str) -> Result<impl Provider + Clone> {
    let signer: PrivateKeySigner = private_key.parse()?;
    let wallet = EthereumWallet::from(signer);
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .wallet(wallet)
        .on_builtin(rpc_url)
        .await?;
    Ok(provider)
}
```

`on_builtin` detects the URL scheme and picks the right transport automatically:
- `http://` or `https://` → HTTP transport
- `ws://` or `wss://` → WebSocket transport (needed for subscriptions)

`with_recommended_fillers()` stacks three middleware layers:
- **NonceFiller** — tracks and increments nonce per transaction, handles concurrent tx ordering
- **GasFiller** — calls `eth_estimateGas` and `eth_feeHistory`, sets EIP-1559 `maxFeePerGas` / `maxPriorityFeePerGas`
- **ChainIdFiller** — fetches chain ID once on first use, attaches to every transaction to prevent replay attacks

### Internal Contract Helper

```rust
fn contract<P: Provider + Clone>(
    provider: P,
    address: &str,
) -> Result<WaveToken::WaveTokenInstance<P>> {
    let addr = Address::from_str(address)?;
    Ok(WaveToken::new(addr, provider))
}
```

This private helper centralises address parsing and instance construction. Every public function calls it rather than duplicating the parse logic.

### Reading Token State

```rust
pub async fn name<P: Provider + Clone>(provider: &P, address: &str) -> Result<String> {
    Ok(contract(provider.clone(), address)?.name().call().await?._0)
}

pub async fn total_supply<P: Provider + Clone>(provider: &P, address: &str) -> Result<U256> {
    Ok(contract(provider.clone(), address)?.totalSupply().call().await?._0)
}

pub async fn balance_of<P: Provider + Clone>(provider: &P, address: &str, account: &str) -> Result<U256> {
    let acc = Address::from_str(account)?;
    Ok(contract(provider.clone(), address)?.balanceOf(acc).call().await?._0)
}
```

All read functions are `eth_call` — they cost no gas and require no signing. The `._0` accessor is how `alloy` exposes the first field of the ABI-decoded return tuple.

### Minting Tokens (Owner Only)

`mint` sends a signed transaction and blocks until the receipt is confirmed on-chain.

```rust
pub async fn mint<P: Provider + Clone>(provider: &P, address: &str, to: &str, amount: U256) -> Result<()> {
    let to_addr = Address::from_str(to)?;
    contract(provider.clone(), address)?
        .mint(to_addr, amount)
        .send().await?    // submits the transaction, returns a PendingTransaction
        .watch().await?;  // polls for receipt until included in a block
    Ok(())
}
```

To call `mint` from `main.rs`:

```rust
use alloy::primitives::U256;

// 1,000,000 WAVE (18 decimals)
let amount = U256::from(1_000_000u64) * U256::from(10u64).pow(U256::from(18u64));
token::mint(&client, &contract_addr, "0xRecipientAddress", amount).await?;
```

### Checking a Balance

```rust
let balance = token::balance_of(&client, &contract_addr, "0xSomeAddress").await?;
println!("balance: {} WAVE (raw units)", balance);

// Convert to human-readable (divide by 10^18)
let divisor = U256::from(10u64).pow(U256::from(18u64));
println!("balance: {} WAVE", balance / divisor);
```

---

## The Solidity Contract (Reference)

This Rust crate is the off-chain counterpart to the following Solidity contract. The `abi/WaveToken.json` is the ABI extracted from this contract after compilation with `solc` or Hardhat/Foundry.

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

contract WaveToken is ERC20, Ownable {
    constructor() ERC20("Wave Governance", "WAVE") Ownable(msg.sender) {}

    function mint(address to, uint256 amount) external onlyOwner {
        _mint(to, amount);
    }
}
```

**What OpenZeppelin provides:**

- `ERC20` — standard transfer, approve, allowance, balanceOf, totalSupply logic
- `Ownable` — `onlyOwner` modifier, `owner()` getter, `transferOwnership()`, `renounceOwnership()`

**What the contract adds:**

- `mint(address to, uint256 amount)` — owner-only token issuance. There is no cap, so the owner controls total supply entirely.

**To extract the ABI yourself** (using Foundry):

```bash
forge build
cat out/WaveToken.sol/WaveToken.json | jq '.abi' > abi/WaveToken.json
```

---

## ABI Reference

| Function | Mutability | Access | Arguments | Returns | Description |
|---|---|---|---|---|---|
| `name()` | view | public | — | `string` | Returns `"Wave Governance"` |
| `symbol()` | view | public | — | `string` | Returns `"WAVE"` |
| `decimals()` | view | public | — | `uint8` | Returns `18` |
| `totalSupply()` | view | public | — | `uint256` | Total tokens minted minus burned |
| `balanceOf(address)` | view | public | `account: address` | `uint256` | Token balance of an address |
| `transfer(address, uint256)` | nonpayable | public | `to: address`, `amount: uint256` | `bool` | Transfer from caller to `to` |
| `allowance(address, address)` | view | public | `owner`, `spender` | `uint256` | Remaining approved spend |
| `approve(address, uint256)` | nonpayable | public | `spender`, `amount` | `bool` | Approve a spender |
| `transferFrom(address, address, uint256)` | nonpayable | public | `from`, `to`, `amount` | `bool` | Transfer on behalf of `from` |
| `mint(address, uint256)` | nonpayable | onlyOwner | `to: address`, `amount: uint256` | — | Mint new tokens |
| `owner()` | view | public | — | `address` | Current contract owner |
| `transferOwnership(address)` | nonpayable | onlyOwner | `newOwner: address` | — | Transfer contract ownership |
| `renounceOwnership()` | nonpayable | onlyOwner | — | — | Permanently remove owner (irreversible) |

---

## Token Economics

| Property | Value |
|---|---|
| Name | Wave Governance |
| Symbol | WAVE |
| Decimals | 18 |
| Initial supply | 0 (minted on demand) |
| Max supply | Uncapped (owner-controlled) |
| Mintable | Yes — owner only |
| Burnable | No (not in current contract) |

**Decimal handling:** Like ETH (wei), `1 WAVE` is represented on-chain as `1 * 10^18 = 1000000000000000000`. Always work in raw units when calling the contract and convert for display.

```rust
// 1.5 WAVE in raw units
let amount = U256::from(15u64) * U256::from(10u64).pow(U256::from(17u64));
```

---

## Testing Locally with Anvil

[Anvil](https://book.getfoundry.sh/anvil/) is Foundry's local EVM node. It gives you a fully funded test environment with instant block times.

### Start Anvil

```bash
anvil
```

Anvil prints 10 pre-funded accounts with private keys. Copy one for your `.env`.

### Deploy WaveToken to Anvil

```bash
# In your Solidity project
forge create src/WaveToken.sol:WaveToken \
  --rpc-url http://127.0.0.1:8545 \
  --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

Copy the `Deployed to:` address into `CONTRACT_ADDRESS` in your `.env`.

### Update `.env` for local testing

```env
RPC_URL=http://127.0.0.1:8545
CONTRACT_ADDRESS=0x<address from forge create output>
PRIVATE_KEY=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

```bash
cargo run
```

No real funds, no gas costs, instant confirmations.

---

## Extending the Client

### Add `transfer`

```rust
pub async fn transfer<P: Provider + Clone>(
    provider: &P,
    address: &str,
    to: &str,
    amount: U256,
) -> Result<()> {
    let to_addr = Address::from_str(to)?;
    contract(provider.clone(), address)?
        .transfer(to_addr, amount)
        .send().await?
        .watch().await?;
    Ok(())
}
```

### Add `approve` + `transfer_from`

```rust
pub async fn approve<P: Provider + Clone>(
    provider: &P,
    address: &str,
    spender: &str,
    amount: U256,
) -> Result<()> {
    let spender_addr = Address::from_str(spender)?;
    contract(provider.clone(), address)?
        .approve(spender_addr, amount)
        .send().await?
        .watch().await?;
    Ok(())
}
```

### Watch for Transfer Events

To listen for `Transfer` events in real time (requires a WebSocket RPC URL):

```rust
use alloy::rpc::types::Filter;

let filter = Filter::new()
    .address(Address::from_str(contract_addr)?)
    .event("Transfer(address,address,uint256)");

let sub = provider.subscribe_logs(&filter).await?;
let mut stream = sub.into_stream();

while let Some(log) = stream.next().await {
    println!("Transfer log: {:?}", log);
}
```

### Add a CLI with `clap`

Add to `Cargo.toml`:

```toml
clap = { version = "4.5.4", features = ["derive"] }
```

Then in `main.rs`:

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Balance { account: String },
    Mint { to: String, amount: u64 },
    TotalSupply,
}
```

---

## Drips Integration Context

In the Drips Network, `WaveToken` is used as the `tokenAddress` argument in `setStreams` calls. The full flow from minting to streaming:

```
1. Owner mints WAVE to sender address
         │
         ▼
2. Sender approves Drips Hub contract to spend WAVE
   (ERC-20 approve)
         │
         ▼
3. Sender calls DripsHub.setStreams(
       tokenAddress = WAVE_CONTRACT,
       currReceivers = [],
       balanceDelta  = +amount,
       newReceivers  = [{ accountId, config }],
       ...
   )
         │
         ▼
4. Drips Hub holds WAVE in escrow
   and streams it to receivers
   at a per-second rate defined in config
         │
         ▼
5. Receivers call DripsHub.collect()
   to withdraw accumulated WAVE
```

The Rust extension for step 2–3 would add a `drips.rs` module with bindings to the Drips Hub ABI, following the same `sol!` pattern used in `token.rs`.

```rust
// Future drips.rs
sol!(
    #[sol(rpc)]
    DripsHub,
    "abi/DripsHub.json"
);

pub async fn set_streams<P: Provider + Clone>(
    provider: &P,
    drips_hub: &str,
    token: &str,
    balance_delta: i128,
    receivers: Vec<DripsHub::StreamReceiver>,
) -> Result<()> {
    // approve + setStreams
}
```

---

## Security Considerations

**Private key handling**
- Never hardcode private keys in source code
- Never commit `.env` to version control
- In production, use a secrets manager (AWS Secrets Manager, HashiCorp Vault) or hardware wallet integration
- Consider using a dedicated hot wallet with only the minimum required balance

**`mint` is owner-only on-chain**
- The Solidity `onlyOwner` modifier enforces this at the EVM level — even if you call `mint` from a non-owner address in Rust, the transaction will revert
- The Rust client does not add an extra ownership check; it relies on the contract

**`renounceOwnership` is irreversible**
- Calling `renounceOwnership()` on the contract permanently removes the ability to mint
- There is no recovery path — do not call this unless you intend to freeze the supply forever

**RPC endpoint trust**
- Your RPC provider sees all your read requests and submitted transactions
- For sensitive operations, consider running your own node or using a provider with a strong privacy policy

**`U256` arithmetic**
- Rust does not overflow `U256` silently — operations panic in debug mode and wrap in release mode
- Always validate amounts before passing them to contract calls

---

## Dependencies

| Crate | Version | Purpose |
|---|---|---|
| [`alloy`](https://crates.io/crates/alloy) | 0.12.6 | Ethereum provider, ABI bindings, signers, primitives, transport |
| [`tokio`](https://crates.io/crates/tokio) | 1.44.2 | Async runtime — drives all `.await` calls |
| [`dotenvy`](https://crates.io/crates/dotenvy) | 0.15.7 | Loads `.env` files into `std::env` |
| [`eyre`](https://crates.io/crates/eyre) | 0.6.12 | Ergonomic error handling with `?` and context messages |

`alloy` with `features = ["full"]` includes:
- `alloy-provider` — the provider stack and fillers
- `alloy-signer-local` — `PrivateKeySigner`
- `alloy-contract` — the `sol!` macro and contract instance types
- `alloy-network` — `EthereumWallet` and network abstractions
- `alloy-primitives` — `Address`, `U256`, `Bytes`, `B256`
- `alloy-rpc-types` — `TransactionRequest`, `Filter`, `Log`

---

## Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feat/my-feature`
3. Make your changes and ensure `cargo build` passes
4. Open a pull request with a clear description of the change

Code style follows standard `rustfmt` defaults. Run `cargo fmt` before committing.

---

## License

MIT
