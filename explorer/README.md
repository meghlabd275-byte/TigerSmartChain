# TigerScan.io - TigerSmartChain Blockchain Explorer 🐯

A production-grade blockchain explorer for TigerSmartChain, equivalent to BscScan.

## Features

### Blockchain
- ✅ Latest Blocks
- ✅ Latest Transactions
- ✅ Transaction Receipts
- ✅ Internal Transactions
- ✅ Pending Transactions
- ✅ Gas Analytics
- ✅ Chain Statistics

### Accounts
- ✅ Account Overview
- ✅ Balance History
- ✅ Transaction History
- ✅ Token Holdings
- ✅ NFT Holdings

### Token Explorer (TEP20)
- ✅ Token List
- ✅ Token Details
- ✅ Token Holders
- ✅ Transfers

### NFT Explorer (TEP721/TEP1155)
- ✅ Collections
- ✅ NFT Details
- ✅ Owners
- ✅ Transfers

### Smart Contract Hub
- ✅ Contract Verification
- ✅ Source Code Viewer
- ✅ Read/Write Contract

### Validator Explorer
- ✅ Validator List
- ✅ Performance Metrics
- ✅ Rewards

### Staking
- ✅ Staking Pools
- ✅ Delegations

### Governance
- ✅ Proposals
- ✅ Voting

### Bridge Explorer
- ✅ Cross Chain Transfers
- ✅ Bridge Status

### Analytics
- ✅ TPS
- ✅ Gas Analytics
- ✅ Network Stats

## Architecture

```
TigerScan/
├── apps/
│   ├── explorer-web/      # Frontend
│   ├── admin-web/       # Admin Panel
│   ├── api-server/     # REST API
│   ├── indexer/       # Blockchain Indexer
│   ├── analytics/     # Analytics Engine
│   ├── search/       # Search Engine
│   └── verifier/     # Contract Verifier
├── packages/
│   ├── blockchain/
│   ├── contracts/
│   ├── tokens/
│   ├── nft/
│   ├── staking/
│   ├── validators/
│   └── bridge/
├── services/
│   ├── block-sync/
│   ├── tx-sync/
│   ├── token-sync/
│   └── mempool-sync/
└── databases/
    ├── postgres/
    ├── redis/
    └── elasticsearch/
```

## Tech Stack

| Component | Technology |
|-----------|------------|
| Frontend | Next.js + TypeScript |
| Backend API | Go + Gin |
| Indexer | Go |
| Search | Elasticsearch |
| Analytics | Rust |
| Database | PostgreSQL |
| Cache | Redis |

## API Endpoints

| Endpoint | Description |
|---------|-------------|
| `GET /api/v1/blocks` | List blocks |
| `GET /api/v1/blocks/:number` | Get block |
| `GET /api/v1/blocks/latest` | Latest block |
| `GET /api/v1/transactions` | List transactions |
| `GET /api/v1/transactions/:hash` | Get transaction |
| `GET /api/v1/accounts/:address` | Get account |
| `GET /api/v1/tokens` | List tokens |
| `GET /api/v1/validators` | List validators |
| `GET /api/v1/nfts/collections` | NFT collections |
| `GET /api/v1/analytics/stats` | Network stats |
| `GET /api/v1/search?q=` | Search |
| `POST /api/v1/contracts/verify` | Verify contract |

## Admin Panel Features

- User Management
- API Key Management
- Validator Management
- Token Management
- Contract Management
- Security Center
- Monitoring
- Audit Logs

## Build

```bash
# Run API server
cd explorer/apps/api-server
go run main.go

# Build frontend
cd explorer/frontend
npm install
npm run dev
```

## Chain Information

- **Chain ID**: 9001
- **Explorer**: TigerScan.io
- **Token**: TGR (Tiger Coin)

---

**TigerScan.io**: Your gateway to TigerSmartChain 🐯