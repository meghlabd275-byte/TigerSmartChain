/**
 * TigerSmartChain TypeScript SDK Types
 */

import { ethers } from 'ethers';

/**
 * Provider configuration
 */
export interface ProviderConfig {
  rpcUrl: string;
  chainId?: number;
  wsUrl?: string;
  gasPrice?: bigint;
}

/**
 * Wallet configuration
 */
export interface WalletConfig {
  privateKey?: string;
  mnemonic?: string;
}

/**
 * Contract deployment options
 */
export interface ContractOptions {
  gasLimit?: bigint;
  gasPrice?: bigint;
  value?: bigint;
}

/**
 * Transaction receipt
 */
export interface TransactionReceipt {
  hash: string;
  blockNumber: number;
  blockHash: string;
  status: number;
  logs: Log[];
  logsBloom: string;
  cumulativeGasUsed: bigint;
  gasUsed: bigint;
}

/**
 * Block
 */
export interface Block {
  number: number;
  hash: string;
  parentHash: string;
  timestamp: number;
  transactions: string[];
  transactionsRoot: string;
  stateRoot: string;
  receiptsRoot: string;
  miner: string;
  difficulty: bigint;
  totalDifficulty: bigint;
  size: bigint;
  gasLimit: bigint;
  gasUsed: bigint;
  nonce: string;
  extraData: string;
}

/**
 * Transaction
 */
export interface Transaction {
  hash: string;
  blockNumber: number;
  blockHash: string;
  from: string;
  to: string;
  value: bigint;
  data: string;
  gasLimit: bigint;
  gasPrice: bigint;
  nonce: number;
  v: number;
  r: string;
  s: string;
}

/**
 * Event log
 */
export interface Log {
  address: string;
  topics: string[];
  data: string;
  logIndex: number;
  blockNumber: number;
  transactionIndex: number;
  transactionHash: string;
  blockHash: string;
}

/**
 * Contract call
 */
export interface Call {
  to: string;
  data: string;
}

/**
 * Signer interface
 */
export type Signer = ethers.Signer;

/**
 * Contract instance
 */
export type ContractInstance = ethers.Contract;

/**
 * Filter
 */
export interface Filter {
  fromBlock?: number | string;
  toBlock?: number | string;
  address?: string;
  topics?: (string | string[] | null)[];
}

/**
 * Event filter
 */
export interface EventFilter extends Filter {
  event?: string;
  args?: any[];
}

/**
 * Wallet signer
 */
export interface WalletSigner extends Signer {
  getAddress(): Promise<string>;
  signMessage(message: string | Uint8Array): Promise<string>;
  signTransaction(transaction: any): Promise<string>;
  sendTransaction(transaction: any): Promise<any>;
}

/**
 * Token info
 */
export interface TokenInfo {
  address: string;
  name: string;
  symbol: string;
  decimals: number;
  totalSupply: bigint;
}

/**
 * NFT info
 */
export interface NFTInfo {
  address: string;
  name: string;
  symbol: string;
  baseURI: string;
}

/**
 * NFT metadata
 */
export interface NFTMetadata {
  tokenId: bigint;
  owner: string;
  tokenURI: string;
  metadata?: {
    name?: string;
    description?: string;
    image?: string;
    attributes?: Record<string, any>;
  };
}

/**
 * Staking info
 */
export interface StakingInfo {
  staked: bigint;
  rewards: bigint;
  rewardPerTokenPaid: bigint;
  stakeTime: number;
  unlockTime: number;
  canUnstake: boolean;
}

/**
 * Governance info
 */
export interface GovernanceInfo {
  proposalId: bigint;
  proposer: string;
  description: string;
  targets: string[];
  values: bigint[];
  signatures: string[];
  calldatas: string[];
  startBlock: number;
  endBlock: number;
  forVotes: bigint;
  againstVotes: bigint;
  abstainVotes: bigint;
  executed: boolean;
  canceled: boolean;
}