# TigerScan Frontend

Next.js frontend for TigerScan blockchain explorer.

## Getting Started

```bash
# Install dependencies
npm install

# Run development server
npm run dev

# Build for production
npm run build
```

## Environment Variables

```
NEXT_PUBLIC_API_URL=http://localhost:8080
NEXT_PUBLIC_CHAIN_ID=9001
NEXT_PUBLIC_CHAIN_NAME=TigerSmartChain
NEXT_PUBLIC_SYMBOL=TGR
```

## Pages

- `/` - Home (Latest blocks & transactions)
- `/blocks` - Block list
- `/blocks/[number]` - Block details
- `/transactions` - Transaction list
- `/transactions/[hash]` - Transaction details
- `/accounts/[address]` - Account details
- `/tokens` - Token list
- `/tokens/[address]` - Token details
- `/validators` - Validator list
- `/nfts` - NFT collections
- `/contracts/[address]` - Contract details
- `/search` - Search results

## API Endpoints

- `/api/v1/blocks` - Get blocks
- `/api/v1/transactions` - Get transactions
- `/api/v1/accounts/[address]` - Get account
- `/api/v1/tokens` - Get tokens
- `/api/v1/validators` - Get validators
- `/api/v1/nfts/collections` - Get NFT collections
- `/api/v1/analytics/stats` - Get stats
- `/api/v1/search?q=` - Search