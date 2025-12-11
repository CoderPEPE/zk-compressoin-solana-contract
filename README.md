# 🚀 Solana Launchpad

A gas-efficient token launchpad on Solana that enables creators to launch tokens for sale with configurable pricing, supply limits, and mint restrictions. Built with Anchor framework for secure and efficient smart contracts.

## ✨ Features

- **Token Launching**: Launch tokens with custom supply, pricing, and mint limits
- **Flexible Pricing**: Support for both paid and free token mints
- **Platform Fees**: Configurable platform fee collection (default 5%)
- **Mint Limits**: Per-transaction mint limits to prevent whale accumulation
- **Auto-closing**: Sales automatically close when fully sold
- **Manual Closing**: Creators can close sales and reclaim unsold tokens
- **Gas-efficient**: Optimized for minimal Solana transaction costs
- **Comprehensive Testing**: Full test coverage with gas cost monitoring

## 🏗️ Architecture

The launchpad consists of several key components:

- **App State**: Global platform configuration (owner, USDC mint, platform fee)
- **Token Sale**: Individual sale configuration and state tracking
- **Program Authority**: PDA for secure fund transfers
- **Token Vaults**: Secure token storage during sales

## 📋 Prerequisites

Before running this project, ensure you have:

- **Node.js** (v16 or higher)
- **Yarn** package manager
- **Rust** (latest stable version)
- **Solana CLI** (v1.18 or higher)
- **Anchor CLI** (v0.30.1 or higher)

### Installation

```bash
# Install Solana CLI
sh -c "$(curl -sSfL https://release.solana.com/v1.18.4/install)"

# Install Anchor CLI
cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
avm install latest
avm use latest

# Install Node.js dependencies
yarn install
```

## 🧪 Testing

The project includes comprehensive tests covering all functionality with gas cost monitoring.

### Running Tests

```bash
# Run all tests
anchor test

# Run tests with verbose output
anchor test -- --verbose

# Run specific test file
anchor test -- --grep "launch token"

# Run tests on different cluster
anchor test --provider.cluster devnet
```

### Test Coverage

The test suite covers:

- ✅ Token launching (paid and free mints)
- ✅ Token purchasing with USDC payments
- ✅ Platform fee distribution (95% to creator, 5% to platform)
- ✅ Mint limit enforcement
- ✅ Sale auto-closing when fully sold
- ✅ Manual sale closing by creators
- ✅ Access control and validation
- ✅ Gas cost monitoring and reporting

### Test Structure

```
tests/
├── solana-launchpad.ts    # Main test suite
└── z-security-tests.ts    # Security-focused tests
```

## 🚀 Deployment

### Local Development

1. **Start local Solana validator:**
```bash
solana-test-validator
```

2. **Build and deploy locally:**
```bash
anchor build
anchor deploy
```

3. **Run tests:**
```bash
anchor test
```

### Devnet Deployment

1. **Configure for devnet:**
```bash
solana config set --url https://api.devnet.solana.com
```

2. **Airdrop SOL for deployment:**
```bash
solana airdrop 2
```

3. **Build and deploy:**
```bash
anchor build
anchor deploy --provider.cluster devnet
```

4. **Update program ID in Anchor.toml:**
```toml
[programs.devnet]
solana_launchpad = "YOUR_PROGRAM_ID_HERE"
```

### Mainnet Deployment

1. **Configure for mainnet:**
```bash
solana config set --url https://api.mainnet.solana.com
```

2. **Ensure sufficient SOL for deployment and rent:**
```bash
solana balance
# Need ~2-3 SOL for deployment
```

3. **Deploy to mainnet:**
```bash
anchor build
anchor deploy --provider.cluster mainnet
```

## 📖 Usage

### Program Initialization

First, initialize the launchpad with platform settings:

```typescript
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { GaslessLaunchpad } from "./target/types/gasless_launchpad";

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);
const program = anchor.workspace.GaslessLaunchpad as Program<GaslessLaunchpad>;

// Initialize with USDC mint and 5% platform fee
const usdcMint = new anchor.web3.PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
await program.methods
  .initialize(usdcMint, 500) // 500 basis points = 5%
  .accounts({
    owner: platformOwner.publicKey,
    appState: appStatePDA,
  })
  .signers([platformOwner])
  .rpc();
```

### Launching a Token

```typescript
// Launch a paid token
const tokenMint = await createMint(...); // Create token mint
const tokenSalePDA = await anchor.web3.PublicKey.findProgramAddressSync(
  [Buffer.from("token_sale"), tokenMint.toBuffer()],
  program.programId
);

await program.methods
  .launchToken(
    "My Token",        // name
    "MTK",            // symbol
    new BN(1000000),  // supply (1M tokens)
    new BN(1000000),  // price per token (1 USDC)
    new BN(100000),   // max per mint (100k tokens)
    "metadata123"     // metadata ID
  )
  .accounts({
    creator: creator.publicKey,
    tokenMint,
    tokenSale: tokenSalePDA,
    saleTokenAccount,
  })
  .signers([creator])
  .rpc();
```

### Buying Tokens

```typescript
// Buy tokens with USDC
const usdcAmount = new BN(10000000); // 10 USDC

await program.methods
  .buyTokens(usdcAmount)
  .accounts({
    buyer: buyer.publicKey,
    tokenSale: tokenSalePDA,
    tokenMint,
    saleTokenAccount,
    buyerTokenAccount,
    buyerUsdcAccount,
    programAuthority,
    programUsdcAccount,
    ownerUsdcAccount: platformOwnerUsdcAccount,
    creatorUsdcAccount,
    appState,
  })
  .signers([buyer])
  .rpc();
```

### Closing a Sale

```typescript
// Close sale and reclaim unsold tokens
await program.methods
  .closeSale()
  .accounts({
    creator: creator.publicKey,
    tokenSale: tokenSalePDA,
    tokenMint,
    saleTokenAccount,
    creatorTokenAccount,
  })
  .signers([creator])
  .rpc();
```

## 🔧 API Reference

### Instructions

#### `initialize(usdc_mint, platform_fee_bps)`
Initialize the launchpad platform.
- `usdc_mint`: USDC token mint address
- `platform_fee_bps`: Platform fee in basis points (max 1000 = 10%)

#### `launch_token(name, symbol, supply, price_per_token, limit_per_mint, metadata_id)`
Launch a new token sale.
- `name`: Token name (1-32 chars)
- `symbol`: Token symbol (1-10 chars)
- `supply`: Total tokens for sale
- `price_per_token`: Price in USDC (0 for free mints)
- `limit_per_mint`: Max tokens per purchase (required for free mints)
- `metadata_id`: Metadata identifier (≤100 chars)

#### `buy_tokens(usdc_amount)`
Purchase tokens from an active sale.
- `usdc_amount`: USDC amount to spend (0 for free mints)

#### `close_sale()`
Close an active sale and reclaim unsold tokens.

#### `update_fee(new_fee_bps)`
Update platform fee (owner only).

### Account Structures

#### AppState
```rust
pub struct AppState {
    pub owner: Pubkey,
    pub usdc_mint: Pubkey,
    pub platform_fee_bps: u16,
}
```

#### TokenSale
```rust
pub struct TokenSale {
    pub creator: Pubkey,
    pub token_mint: Pubkey,
    pub price_per_token: u64,
    pub supply_for_sale: u64,
    pub tokens_sold: u64,
    pub active: bool,
    pub metadata_id: String,
    pub limit_per_mint: u64,
    pub decimals: u8,
    pub bump: u8,
}
```

## 🛠️ Development

### Project Structure

```
├── programs/
│   └── solana-launchpad/
│       ├── src/lib.rs          # Main program logic
│       └── Cargo.toml          # Rust dependencies
├── tests/
│   ├── solana-launchpad.ts     # Main test suite
│   └── z-security-tests.ts     # Security tests
├── migrations/
│   └── deploy.ts               # Deployment script
├── app/                        # Frontend (if applicable)
├── Anchor.toml                 # Anchor configuration
├── Cargo.toml                  # Workspace configuration
└── package.json                # Node.js dependencies
```

### Building

```bash
# Build the program
anchor build

# Build with verbose output
anchor build --verbose

# Clean and rebuild
anchor clean && anchor build
```

### Code Quality

```bash
# Run linter
yarn lint

# Fix linting issues
yarn lint:fix

# Format code
anchor fmt
```

### Local Development Setup

1. **Clone the repository:**
```bash
git clone <repository-url>
cd solana-launchpad
```

2. **Install dependencies:**
```bash
yarn install
```

3. **Start local validator:**
```bash
solana-test-validator
```

4. **Build and test:**
```bash
anchor build
anchor test
```

---

Built with ❤️ using [Anchor Framework](https://www.anchor-lang.com/)