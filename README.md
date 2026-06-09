# TigerSmartChain (TSC) - BSC Alternative

A high-performance BSC-like EVM-compatible blockchain with native **Tiger Coin (TGR)** utility token.

## Native Token: Tiger Coin (TGR) 🐯

- **Name**: Tiger Coin
- **Ticker**: Tiger
- **Symbol**: TGR  
- **Decimals**: 18
- **Total Supply**: 1 billion (1,000,000,000 TGR)
- **Chain ID**: 9001

Tiger Coin is the native utility token for:
- ⛽ Transaction fees (gas)
- 🔒 Validator staking & rewards
- 🗳️ Governance voting
- 🌉 Cross-chain bridge operations
- 💰 DeFi protocols (DEX, lending, staking)

## 🌐 EVM Compatibility - Work on All EVM Chains!

TigerSmartChain is **100% EVM-compatible**. This means:

✅ **Any EVM contract works on TigerSmartChain**
- Deploy Ethereum, BSC, Polygon contracts directly
- No code changes needed
- Same bytecode, same behavior

✅ **Contracts on TigerSmartChain work on ALL EVM chains**
- Deploy once, use everywhere
- Ethereum ↔ TigerSmartChain ↔ BSC ↔ Polygon ↔ Arbitrum ↔ Base
- Universal smart contract deployment

✅ **Token Standards Compatible**
- ERC-20 = TEP-20 = BEP-20 (same!)
- ERC-721 = TEP-721 = BEP-721 (same!)
- ERC-1155 = TEP-1155 (same!)

## Performance - Equal or Better Than BSC

| Feature | TigerSmartChain | BinanceSmartChain | Status |
|---------|-----------------|-------------------|--------|
| Block Time | 3 seconds | 3 seconds | ✅ Match |
| Max Gas Limit | 30M | 30M | ✅ Match |
| TPS (Theoretical) | 100+ | 100+ | ✅ Match |
| Consensus | PoSA | PoSA | ✅ Match |
| Validators | 21 | 21 | ✅ Match |
| EVM Version | Istanbul | Istanbul | ✅ Match |
| Finality | ~3 seconds | ~3 seconds | ✅ Match |

## Key Features

✅ **Full EVM Compatibility**
- MetaMask ✅
- Remix ✅  
- Hardhat/Foundry ✅
- Web3.js/Ethers.js ✅

✅ **PoSA Consensus**
- Validator registration & delegation
- Staking rewards (5% APY)
- Slashing mechanism
- Validator rotation

✅ **Smart Contract Standards**
- TEP20 (Fungible Token - like BEP20)
- TEP721 (NFT - like BEP721)
- TEP1155 (Multi Token)

✅ **RPC Endpoints**
- HTTP JSON-RPC (port 8545)
- WebSocket RPC (port 8546)
- gRPC (port 8547)

✅ **Security**
- Anti-DDOS protection
- Rate limiting
- MEV protection
- Validator monitoring

## Quick Start

```bash
# Initialize new chain
./tigersmartchaind init

# Start validator node
./tigersmartchaind start --validator --key <private-key>

# Connect to console
./tigersmartchaind console

# Stake as validator (100 TGR min)
./tigersmartchaind validator stake --amount 100000TGR
```

## RPC Methods

Standard Ethereum + BSC methods:
- eth_blockNumber, eth_getBlockByNumber, eth_getBalance
- eth_sendTransaction, eth_call, eth_estimateGas
- bsc_gasPrice, bsc_getValidatorInfo
- net_version, web3_clientVersion

## Block Explorer

- Backend API: `internal/explorer/`
- Frontend: Next.js + React
- Real-time indexer

## Bridge Support

Cross-chain bridges:
- Ethereum ↔ TigerSmartChain
- Polygon ↔ TigerSmartChain  
- Arbitrum ↔ TigerSmartChain
- Base ↔ TigerSmartChain

## Governance

- On-chain governance with TGR
- Proposal system
- Voting mechanism
- Treasury

---

**TigerSmartChain**: Matching BSC speed & features with Tiger Coin 🐯** 
