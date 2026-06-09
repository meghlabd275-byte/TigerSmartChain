// TigerSmartChain Wallet Types

export interface WalletState {
  account: string | null;
  balance: string;
  isLocked: boolean;
  isConnecting: boolean;
  error: string | null;
}

export interface Transaction {
  hash: string;
  from: string;
  to: string;
  value: string;
  data: string;
  gasLimit: number;
  gasPrice: string;
  nonce: number;
  chainId: number;
  status: TransactionStatus;
  receipt?: TransactionReceipt;
  timestamp: number;
  blockNumber?: number;
  transactionIndex?: number;
}

export type TransactionStatus = 
  | 'pending'
  | 'submitted'
  | 'confirmed'
  | 'failed'
  | 'dropped';

export interface TransactionReceipt {
  transactionHash: string;
  blockHash: string;
  blockNumber: number;
  cumulativeGasUsed: number;
  gasUsed: number;
  status: boolean;
  logs: Log[];
}

export interface Log {
  address: string;
  topics: string[];
  data: string;
  logIndex: number;
  transactionIndex: number;
  transactionHash: string;
  blockHash: string;
  blockNumber: number;
}

export interface Token {
  address: string;
  name: string;
  symbol: string;
  decimals: number;
  totalSupply: string;
  balance?: string;
  logoUrl?: string;
  price?: string;
  marketCap?: string;
  volume24h?: string;
}

export interface Network {
  id: number;
  name: string;
  rpcUrl: string;
  explorerUrl: string;
  chainId: number;
  currency: {
    name: string;
    symbol: string;
    decimals: number;
  };
}

export interface Block {
  number: number;
  hash: string;
  parentHash: string;
  nonce: string;
  sha3Uncles: string;
  logsBloom: string;
  transactionsRoot: string;
  stateRoot: string;
  receiptsRoot: string;
  miner: string;
  difficulty: number;
  totalDifficulty: number;
  size: number;
  gasLimit: number;
  gasUsed: number;
  timestamp: number;
  transactions: string[] | Transaction[];
  uncles: string[];
}

export interface Account {
  address: string;
  balance: string;
  code: string;
  codeHash: string;
  nonce: number;
  storageRoot: string;
}

export interface CallRequest {
  to: string;
  data?: string;
  value?: string;
  gas?: number;
  gasPrice?: string;
}

export interface CallResponse {
  data: string;
}

export interface EstimateGasRequest {
  from?: string;
  to?: string;
  value?: string;
  data?: string;
  gas?: number;
  gasPrice?: string;
}

export interface EstimateGasResponse {
  gasUsed: string;
}

export interface FilterOptions {
  fromBlock?: number | string;
  toBlock?: number | string;
  address?: string;
  topics?: string[];
}

export interface FilterLogsResponse extends Log {
  id: string;
  removed: boolean;
}

// Wallet connection types
export interface ConnectOptions {
  networkId?: number;
  autoUnlock?: boolean;
}

export interface UnlockOptions {
  password: string;
  duration?: number;
}

// Transaction options
export interface TransactionRequest {
  to: string;
  value?: string;
  data?: string;
  gas?: number;
  gasPrice?: string;
  nonce?: number;
  chainId?: number;
}

export interface TransactionResponse {
  hash: string;
  wait: () => Promise<TransactionReceipt>;
}

// Event types
export interface WalletEvent {
  type: 'connect' | 'disconnect' | 'accountChange' | 'networkChange';
  account?: string;
  network?: Network;
}

export interface EventCallback {
  (event: WalletEvent): void;
}

// Signing types
export interface SignMessageRequest {
  message: string;
}

export interface SignTypedDataRequest {
  domain: any;
  types: any;
  value: any;
  primaryType: string;
}

export interface SignatureResponse {
  signature: string;
}

// Token types
export interface TokenBalance {
  address: string;
  balance: string;
  token: Token;
}

export interface TokenTransfer {
  hash: string;
  from: string;
  to: string;
  value: string;
  token: Token;
  timestamp: number;
  blockNumber: number;
  confirmations: number;
}

export interface TokenAllowance {
  owner: string;
  spender: string;
  allowance: string;
  token: Token;
}

// Contract types
export interface Contract {
  address: string;
  abi: any[];
  provider: any;
}

export interface ContractMethod {
  name: string;
  inputs: any[];
  outputs: any[];
  stateMutability: 'pure' | 'view' | 'nonpayable' | 'payable';
}

export interface ContractEvent {
  name: string;
  inputs: any[];
}