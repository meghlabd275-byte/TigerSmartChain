# TigerSmartChain (TSC) - BSC Alternative with TigerScan Explorer 🐯

A high-performance BSC-like EVM-compatible blockchain with **Tiger Coin (TGR)** and built-in blockchain explorer.

## Native Token: Tiger Coin (TGR)

| Property | Value |
|----------|-------|
| **Name** | Tiger Coin |
| **Ticker** | Tiger |
| **Symbol** | TGR |
| **Chain ID** | 9001 |
| **Decimals** | 18 |
| **Total Supply** | 1 Billion |

### Royal Tiger USD (RUSD) - Stablecoin

| Property | Value |
|----------|-------|
| **Name** | Royal Tiger United State Dollar |
| **Ticker** | RUSD |
| **Symbol** | RUSD |
| **Type** | Stablecoin (1:1 USD) |

## EVM Compatibility - Same Addresses!

All popular EVM tokens work on TigerSmartChain with **identical contract addresses**:

| Token | Works On |
|-------|----------|
| USDT | Ethereum, BSC, Polygon, Arbitrum |
| USDC | Ethereum, BSC, Polygon, Arbitrum |
| BNB | BSC |
| ETH | Ethereum |
| BTCB | BSC |
| CAKE | BSC |
| BUSD | BSC |

**Deploy once, use everywhere!** ERC-20 = TEP-20 = BEP-20

## Performance (Matching BSC)

| Feature | TigerSmartChain | BSC |
|---------|----------------|-----|
| Block Time | 3 sec | 3 sec ✅ |
| Max Gas | 30M | 30M ✅ |
| TPS | 100+ | 100+ ✅ |
| Consensus | PoSA | PoSA ✅ |
| Validators | 21 | 21 ✅ |

## Quick Start

```bash
# Build
go build -o tigersmartchaind ./cmd/tigersmartchaind/

# Initialize
./tigersmartchaind init

# Start node
./tigersmartchaind start
```

## TigerScan Explorer

Built-in blockchain explorer (TigerScan.io equivalent):

```bash
# Run explorer API
cd backend && ./tigersmartchaind-api
```

### API Endpoints
- `GET /api/v1/blocks` - List blocks
- `GET /api/v1/transactions` - List transactions
- `GET /api/v1/accounts/:address` - Get account
- `GET /api/v1/tokens` - List tokens
- `GET /api/v1/validators` - List validators
- `GET /api/v1/analytics/stats` - Network stats

## Project Structure

```
TigerSmartChain/
├── cmd/                    # CLI commands
├── internal/               # Core blockchain
│   ├── blockchain/        # Block, transaction
│   ├── consensus/         # PoSA consensus
│   ├── evm/               # EVM execution
│   ├── state/              # State DB
│   ├── storage/            # LevelDB
│   ├── rpc/               # JSON-RPC
│   └── network/            # P2P networking
├── contracts/              # TEP20, TEP721
├── frontend/              # TigerScan web
├── backend/               # TigerScan API
└── tigersmartchaind       # Main binary
```

---

**TigerSmartChain**: Full EVM blockchain with Tiger Coin + TigerScan 🐯