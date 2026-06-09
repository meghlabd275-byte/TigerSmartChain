# TigerSmartChain Web Wallet

A full-featured web wallet for TigerSmartChain blockchain.

## Features

- 🔐 Secure wallet creation and import
- 💸 Send/receive TSC and tokens
- 🖼️ NFT management
- 🔗 Connect with hardware wallets (Ledger, Trezor)
- 🌐 Multi-chain support (BSC, Ethereum)
- 📱 Mobile responsive

## Getting Started

### Installation

```bash
# Install dependencies
npm install

# Start development server
npm run dev

# Build for production
npm run build
```

### Environment Variables

```
NEXT_PUBLIC_RPC_URL=https://rpc.tigersmartchain.com
NEXT_PUBLIC_CHAIN_ID=1
NEXT_PUBLIC_NETWORK_NAME=TigerSmartChain
```

## Tech Stack

- Next.js 14
- React 18
- TypeScript
- ethers.js
- Tailwind CSS

## Wallet Features

### Security
- Client-side key derivation (BIP-39)
- Hardware wallet support
- Transaction signing
- Multi-sig support

### Token Management
- TEP20 token transfers
- TEP721 NFT transfers
- Token approval management

### Transaction History
- Complete transaction history
- Token transfer history
- NFT transfer history

## API

The wallet connects to the following RPC endpoints:

- `eth_blockNumber` - Get current block
- `eth_getBalance` - Get account balance
- `eth_sendTransaction` - Send transactions
- `eth_call` - Contract calls
- `eth_getLogs` - Event logs

## License

MIT