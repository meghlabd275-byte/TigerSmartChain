# TigerSmartChain (TSC) 🐯

A fully EVM-compatible blockchain similar to BinanceSmartChain (BNB Chain), built with a multi-language architecture for maximum security and performance.

## Quick Specs

| Property | Value |
|----------|-------|
| **Chain ID** | 6666 |
| **Native Token** | Tiger Coin (TGR) |
| **Ticker** | Tiger |
| **Block Time** | 3 Seconds |
| **Consensus** | Proof of Staked Authority (PoSA) |
| **Max Gas** | 30M |
| **Validators** | 21 |

## Architecture Overview

TigerSmartChain uses a hybrid architecture combining Go for the blockchain node, Rust for security-critical components, and TypeScript for wallets and explorers.

### Go Components (Core Blockchain)
```
node/                     # Blockchain node
cmd/                      # CLI tools
internal/                 # Core implementation
  ├── blockchain/         # Block, transaction, receipt, chain, genesis
  ├── consensus/          # PoSA, validator, election, rewards, slashing
  ├── evm/               # Interpreter, precompiles, opcodes, gas-meter
  ├── state/              # Account, trie, state-db
  ├── storage/            # LevelDB storage
  ├── network/           # P2P, peer, discovery, sync
  ├── rpc/               # JSON-RPC, WebSocket
  └── metrics/           # Prometheus
```

### Rust Components (Security-Critical)
```
security/crypto/          # Cryptography engine (ECDSA, hashing)
security-engine/         # Security analysis (phishing, scam detection)
bridge-engine/          # Cross-chain bridge
analytics-engine/        # TVL, whale detection, rankings
```

### Solidity Contracts
```
contracts/
  ├── TEP20/            # Tiger token standard (BEP20 equivalent)
  ├── TEP721/           # NFT standard
  ├── TEP1155/          # Multi-token standard
  ├── staking/           # Staking pool
  ├── governance/        # DAO governance
  ├── treasury/          # Treasury + Vesting
  └── bridge/            # Cross-chain bridge
```

### Explorer & Wallet
```
explorer/
  ├── apps/              # Indexer, API server
  ├── services/         # Token, NFT, analytics services
  ├── databases/        # PostgreSQL schema
  └── frontend/        # React components, hooks
wallet/
  └── web/             # Web wallet
sdk/
  └── typescript/      # TypeScript SDK
```

### Docker & Deployment
```
docker/                  # Docker configurations
deployment/             # Kubernetes manifests
```

## TEP Token Standards

| Standard | Description |
|----------|-------------|
| TEP-20 | Fungible Token |
| TEP-721 | NFT |
| TEP-1155 | Multi Token |
| TEP-165 | Interface Detection |
| TEP-2612 | Permit |
| TEP-2981 | Royalty |
| TEP-4337 | Account Abstraction |

## Features

### ✅ EVM Compatible
- MetaMask ✅
- Remix ✅
- Hardhat ✅
- Foundry ✅
- Web3.js ✅

### ✅ Validator System
- Validator Registration
- Delegation
- Rewards
- Slashing
- Validator Rotation

### ✅ Staking
- Stake TGR
- Unstake
- Lock Period
- Auto Compound

### ✅ Governance
- DAO
- Voting
- Treasury
- Proposal System

### ✅ Cross Chain Bridge
- Ethereum
- BNB Chain
- Polygon
- Arbitrum
- Base
- Avalanche

### ✅ Security
- Multi-Sig
- Anti-DDOS
- MEV Protection
- Slashing
- Rate Limiting
- Validator Monitoring

## Database Stack

| Database | Use Case |
|----------|---------|
| LevelDB | Chain Storage |
| RocksDB | Archive Nodes |
| Redis | Cache |
| PostgreSQL | Explorer |
| Elasticsearch | Analytics |

## Infrastructure

- Validator Nodes
- RPC Nodes
- Archive Nodes
- Indexer Nodes
- Explorer Nodes
- Bridge Nodes
- Monitoring Nodes

## Build

```bash
# Build main binary
go build -o tigersmartchaind ./cmd/tigersmartchaind/

# Initialize chain
./tigersmartchaind init

# Start node
./tigersmartchaind start

# Run explorer
cd backend && ./tigerscan-api
```

## Token Support

### Native Token
- **Tiger Coin (TGR)** - Native utility token

### Stablecoins
- **Royal Tiger USD (RUSD)** - 1:1 with USD

### Popular EVM Tokens (Same Addresses!)
| Token | Symbol |
|-------|--------|
| USDT | USDT |
| USDC | USDC |
| BNB | BNB |
| ETH | ETH |
| BTCB | BTCB |
| CAKE | CAKE |

---

**TigerSmartChain**: BSC-like EVM blockchain with Tiger Coin 🐯