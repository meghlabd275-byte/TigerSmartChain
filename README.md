# TigerSmartChain (TSC) 🐯

A fully EVM-compatible blockchain similar to BinanceSmartChain, built on modified go-ethereum architecture.

## Quick Specs

| Property | Value |
|----------|-------|
| **Chain ID** | 9001 |
| **Native Token** | Tiger Coin (TGR) |
| **Ticker** | Tiger |
| **Block Time** | 3 Seconds |
| **Consensus** | Proof of Staked Authority (PoSA) |
| **Max Gas** | 30M |
| **Validators** | 21 |

## Architecture

```
TigerSmartChain/
├── cmd/                    # CLI (tigersmartchaind, validator, wallet)
├── internal/               # Core blockchain
│   ├── blockchain/        # block, transaction, receipt, gas, chain, genesis
│   ├── consensus/         # posa, validator, election, rewards, slashing, governance
│   ├── evm/              # interpreter, precompiles, opcodes, gas-meter, execution-engine
│   ├── state/             # account, trie, snapshot, state-db
│   ├── storage/           # leveldb, rocksdb, cache, archive
│   ├── network/           # p2p, peer, discovery, sync, gossip
│   ├── rpc/              # json-rpc, websocket, graphql, grpc
│   ├── staking/           # validator, delegation, rewards, lockups
│   ├── bridge/           # ethereum, bsc, polygon, arbitrum, base
│   ├── governance/        # proposal, voting, treasury, timelock
│   ├── security/         # cryptography, anti-spam, anti-ddos, anti-mev
│   └── metrics/          # prometheus, grafana, telemetry
├── contracts/            # TEP20, TEP721, TEP1155, staking, governance, bridge
├── explorer/            # backend, indexer, api, frontend
├── wallet/             # mobile, web, browser-extension, sdk
├── sdk/               # javascript, typescript, go, rust, python
├── tests/             # unit, integration, fuzz, load, security
├── scripts/           # deployment scripts
├── docker/            # Docker configurations
└── deployment/        # Kubernetes configs
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