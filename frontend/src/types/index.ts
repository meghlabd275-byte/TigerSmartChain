// Block Types
export interface Block {
  number: number
  hash: string
  parentHash: string
  timestamp: number
  transactions: string[]
  gasUsed: string
  gasLimit: string
  miner: string
  difficulty: string
  totalDifficulty: string
  size: number
  nonce: string
  extraData: string
  baseFeePerGas?: string
  blobGasUsed?: string
  transactionsCount: number
  unclesCount: number
}

export interface BlockListItem {
  number: number
  hash: string
  timestamp: number
  txCount: number
  gasUsed: string
  miner: string
}

// Transaction Types
export interface Transaction {
  hash: string
  blockNumber: number
  blockHash: string
  timestamp: number
  from: string
  to: string | null
  value: string
  gasPrice: string
  gasUsed: string
  gasLimit: string
  nonce: number
  transactionIndex: number
  input: string
  status: 'success' | 'failure' | 'pending'
  logs: Log[]
  tokenTransfers: TokenTransfer[]
  internalTransactions: InternalTransaction[]
}

export interface InternalTransaction {
  hash: string
  blockNumber: number
  from: string
  to: string
  value: string
  callType: string
  gas: string
  input: string
  output: string
  error?: string
  depth: number
}

// Token Types
export interface Token {
  address: string
  name: string
  symbol: string
  decimals: number
  totalSupply: string
  type: 'BEP20' | 'BEP721' | 'BEP1155'
  price?: number
  priceChange24h?: number
  marketCap?: number
  volume24h?: number
  holdersCount: number
  transfersCount: number
  isVerified: boolean
  isSpam: boolean
  logoUrl?: string
}

export interface TokenHolder {
  address: string
  balance: string
  percentage: number
}

export interface TokenTransfer {
  hash: string
  tokenAddress: string
  from: string
  to: string
  value: string
  timestamp: number
  blockNumber: number
  logIndex: number
}

export interface TokenPriceHistory {
  timestamp: number
  price: number
  volume: number
}

// NFT Types
export interface NFTCollection {
  address: string
  name: string
  symbol: string
  type: 'BEP721' | 'BEP1155'
  totalSupply: number
  mintedCount: number
  ownerCount: number
  floorPrice?: number
  averagePrice?: number
  volume24h?: number
  volume7d?: number
  volume30d?: number
  imageUrl?: string
  bannerUrl?: string
  description?: string
  socialLinks?: SocialLinks
}

export interface NFTToken {
  tokenId: string
  collectionAddress: string
  owner: string
  uri?: string
  metadata?: NFTMetadata
  imageUrl?: string
  animationUrl?: string
}

export interface NFTMetadata {
  name?: string
  description?: string
  image?: string
  external_url?: string
  attributes?: NFTAttribute[]
}

export interface NFTAttribute {
  trait_type: string
  value: string | number
  display_type?: string
}

export interface NFTTransfer {
  hash: string
  collectionAddress: string
  tokenId: string
  from: string
  to: string
  value?: string
  timestamp: number
  blockNumber: number
  price?: string
}

export interface SocialLinks {
  website?: string
  twitter?: string
  discord?: string
  telegram?: string
}

// Contract Types
export interface Contract {
  address: string
  bytecode: string
  bytecodeHash: string
  isVerified: boolean
  verificationDate?: string
  contractType?: string
  sourceCode?: string
  abi?: string
  compilerVersion?: string
  optimizationEnabled?: boolean
  optimizationRuns?: number
  license?: string
  isProxy?: boolean
  implementationAddress?: string
}

// Analytics Types
export interface NetworkStats {
  totalBlocks: number
  totalTransactions: number
  totalAddresses: number
  totalContracts: number
  totalTokens: number
  avgBlockTime: number
  avgGasPrice: string
  tps: number
}

export interface ChartData {
  timestamp: number
  value: number
}

export interface GasOracle {
  slow: string
  standard: string
  fast: string
  baseFee: string
}

// Address Types
export interface Address {
  address: string
  balance: string
  isContract: boolean
  isMultisig?: boolean
  name?: string
  ensName?: string
  txCount: number
  firstSeenBlock: number
  lastSeenBlock: number
  totalReceived: string
  totalSent: string
  tokens?: Token[]
  nfts?: NFTToken[]
}

// Search Types
export interface SearchResult {
  type: 'address' | 'transaction' | 'block' | 'token' | 'nft' | 'ens'
  address?: string
  hash?: string
  number?: number
  name?: string
  symbol?: string
}

// DEX Types
export interface DexPair {
  pairAddress: string
  token0: Token
  token1: Token
  reserve0: string
  reserve1: string
  totalSupply: string
  volume24h: number
  volume7d: number
  liquidity: number
  price0: number
  price1: number
}

// Governance Types
export interface GovernanceProposal {
  id: string
  title: string
  description: string
  status: 'pending' | 'active' | 'passed' | 'failed' | 'executed'
  proposer: string
  voteCount: number
  forVotes: string
  againstVotes: string
  abstainVotes: string
  startBlock: number
  endBlock: number
  createdAt: number
}

// API Response Types
export interface PaginatedResponse<T> {
  items: T[]
  total: number
  page: number
  limit: number
  hasMore: boolean
}

export interface APIResponse<T> {
  data: T
  success: boolean
  message?: string
}
