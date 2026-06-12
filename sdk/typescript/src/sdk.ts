/**
 * TigerScan TypeScript SDK
 * 
 * Comprehensive TypeScript SDK for TigerScan API and TigerSmartChain blockchain
 */

import axios, { AxiosInstance, AxiosRequestConfig } from 'axios';
import { ethers, BigNumber, Contract, Wallet } from 'ethers';

// Types
export enum ChainId {
  TIGER_MAINNET = 1,
  TIGER_TESTNET = 5,
  ETHEREUM_MAINNET = 1,
  ETHEREUM_SEPOLIA = 11155111,
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
  transactionCount: number;
  transactions: string[];
  uncles: string[];
}

export interface Transaction {
  hash: string;
  nonce: number;
  blockHash: string;
  blockNumber: number;
  transactionIndex: number;
  from: string;
  to: string;
  value: string;
  gasPrice: string;
  gasLimit: number;
  gasUsed: number;
  input: string;
  status: number;
  timestamp: number;
}

export interface TransactionReceipt {
  transactionHash: string;
  blockHash: string;
  blockNumber: number;
  cumulativeGasUsed: number;
  gasUsed: number;
  contractAddress: string | null;
  status: number;
  logs: Log[];
}

export interface Log {
  address: string;
  topics: string[];
  data: string;
  blockNumber: number;
  logIndex: number;
  transactionIndex: number;
}

export interface Token {
  address: string;
  name: string;
  symbol: string;
  decimals: number;
  totalSupply: string;
  price: string;
  marketCap: string;
  volume24h: string;
  holders: number;
  transfers: number;
}

export interface TokenHolder {
  address: string;
  balance: string;
  percent: number;
}

export interface NFT {
  address: string;
  name: string;
  symbol: string;
  totalSupply: number;
  floorPrice: string;
  holders: number;
  transfers: number;
  royaltyBps: number;
}

export interface Account {
  address: string;
  balance: string;
  transactionCount: number;
}

export interface NetworkStats {
  blockNumber: number;
  totalAddresses: number;
  totalTransactions: number;
  totalValueLocked: string;
  marketCap: string;
  tps: number;
  gasPrice: string;
}

export interface GasPrice {
  slow: string;
  standard: string;
  fast: string;
}

export interface ChartDataPoint {
  timestamp: number;
  value: number;
}

// SDK Configuration
export interface SDKConfig {
  apiKey?: string;
  baseURL?: string;
  chainId?: ChainId;
  timeout?: number;
}

// SDK Client
export class TigerScanSDK {
  private apiKey: string;
  private baseURL: string;
  private chainId: ChainId;
  private timeout: number;
  private http: AxiosInstance;

  constructor(config: SDKConfig = {}) {
    this.apiKey = config.apiKey || process.env.TIGERSCAN_API_KEY || '';
    this.baseURL = config.baseURL || 'https://api.tigerscan.io';
    this.chainId = config.chainId || ChainId.TIGER_MAINNET;
    this.timeout = config.timeout || 30000;

    this.http = axios.create({
      baseURL: this.baseURL,
      timeout: this.timeout,
      headers: {
        'Content-Type': 'application/json',
        'User-Agent': 'TigerScan-SDK-TypeScript/1.0.0',
        ...(this.apiKey && { 'X-API-Key': this.apiKey }),
      },
    });
  }

  // Block Operations

  async getBlock(blockNumber?: number, blockHash?: string): Promise<Block> {
    const endpoint = blockNumber !== undefined 
      ? `/api/v2/block/${blockNumber}`
      : `/api/v2/block/hash/${blockHash}`;
    
    const response = await this.http.get(endpoint);
    const result = response.data.result;
    
    return {
      number: parseInt(result.blockNumber),
      hash: result.hash,
      parentHash: result.parentHash,
      nonce: result.nonce,
      sha3Uncles: result.sha3Uncles,
      logsBloom: result.logsBloom,
      transactionsRoot: result.transactionsRoot,
      stateRoot: result.stateRoot,
      receiptsRoot: result.receiptsRoot,
      miner: result.miner,
      difficulty: parseInt(result.difficulty),
      totalDifficulty: parseInt(result.totalDifficulty),
      size: parseInt(result.size),
      gasLimit: parseInt(result.gasLimit),
      gasUsed: parseInt(result.gasUsed),
      timestamp: parseInt(result.timestamp),
      transactionCount: result.transactions?.length || 0,
      transactions: result.transactions || [],
      uncles: result.uncles || [],
    };
  }

  async getLatestBlock(): Promise<Block> {
    const response = await this.http.get('/api/v2/block/latest');
    return this.getBlock(parseInt(response.data.result.blockNumber));
  }

  async getBlockRewards(blockNumber: number): Promise<{ minerReward: string; uncleReward: string }> {
    const response = await this.http.get(`/api/v2/block/${blockNumber}/rewards`);
    return response.data.result;
  }

  // Transaction Operations

  async getTransaction(txHash: string): Promise<Transaction> {
    const response = await this.http.get(`/api/v2/tx/${txHash}`);
    const result = response.data.result;
    
    return {
      hash: result.hash,
      nonce: parseInt(result.nonce),
      blockHash: result.blockHash,
      blockNumber: parseInt(result.blockNumber),
      transactionIndex: parseInt(result.transactionIndex),
      from: result.from,
      to: result.to,
      value: result.value,
      gasPrice: result.gasPrice,
      gasLimit: parseInt(result.gasLimit),
      gasUsed: parseInt(result.gasUsed),
      input: result.input,
      status: parseInt(result.status),
      timestamp: parseInt(result.timestamp || 0),
    };
  }

  async getTransactionReceipt(txHash: string): Promise<TransactionReceipt> {
    const response = await this.http.get(`/api/v2/tx/${txHash}/receipt`);
    const result = response.data.result;
    
    return {
      transactionHash: result.transactionHash,
      blockHash: result.blockHash,
      blockNumber: parseInt(result.blockNumber),
      cumulativeGasUsed: parseInt(result.cumulativeGasUsed),
      gasUsed: parseInt(result.gasUsed),
      contractAddress: result.contractAddress,
      status: parseInt(result.status),
      logs: result.logs || [],
    };
  }

  async getInternalTransactions(txHash: string): Promise<any[]> {
    const response = await this.http.get(`/api/v2/tx/${txHash}/internal`);
    return response.data.result || [];
  }

  // Account Operations

  async getAccount(address: string): Promise<Account> {
    const response = await this.http.get(`/api/v2/account/${address}`);
    const result = response.data.result;
    
    return {
      address: result.address,
      balance: result.balance,
      transactionCount: parseInt(result.transactionCount),
    };
  }

  async getAccountTransactions(
    address: string, 
    page: number = 1, 
    limit: number = 50
  ): Promise<Transaction[]> {
    const response = await this.http.get(`/api/v2/account/${address}/transactions`, {
      params: { page, limit },
    });
    
    return (response.data.result || []).map((tx: any) => ({
      hash: tx.hash,
      nonce: parseInt(tx.nonce),
      blockHash: tx.blockHash,
      blockNumber: parseInt(tx.blockNumber),
      transactionIndex: parseInt(tx.transactionIndex),
      from: tx.from,
      to: tx.to,
      value: tx.value,
      gasPrice: tx.gasPrice,
      gasLimit: parseInt(tx.gasLimit),
      gasUsed: parseInt(tx.gasUsed),
      input: tx.input,
      status: parseInt(tx.status),
      timestamp: parseInt(tx.timestamp || 0),
    }));
  }

  async getAccountTokens(address: string): Promise<Token[]> {
    const response = await this.http.get(`/api/v2/account/${address}/tokens`);
    return (response.data.result || []).map((t: any) => ({
      address: t.address,
      name: t.name,
      symbol: t.symbol,
      decimals: parseInt(t.decimals),
      totalSupply: t.totalSupply,
      price: t.price,
      marketCap: t.marketCap,
      volume24h: t.volume24h,
      holders: parseInt(t.holders),
      transfers: parseInt(t.transfers),
    }));
  }

  async getAccountNFTs(address: string): Promise<NFT[]> {
    const response = await this.http.get(`/api/v2/account/${address}/nfts`);
    return (response.data.result || []).map((n: any) => ({
      address: n.address,
      name: n.name,
      symbol: n.symbol,
      totalSupply: parseInt(n.totalSupply),
      floorPrice: n.floorPrice,
      holders: parseInt(n.holders),
      transfers: parseInt(n.transfers),
      royaltyBps: parseInt(n.royaltyBps),
    })));
  }

  // Token Operations

  async getTokens(page: number = 1, limit: number = 50): Promise<Token[]> {
    const response = await this.http.get('/api/v2/tokens', {
      params: { page, offset: limit },
    });
    
    return (response.data.result || []).map((t: any) => ({
      address: t.address,
      name: t.name,
      symbol: t.symbol,
      decimals: parseInt(t.decimals),
      totalSupply: t.totalSupply,
      price: t.price,
      marketCap: t.marketCap,
      volume24h: t.volume24h,
      holders: parseInt(t.holders),
      transfers: parseInt(t.transfers),
    }));
  }

  async getToken(address: string): Promise<Token> {
    const response = await this.http.get(`/api/v2/token/${address}`);
    const t = response.data.result;
    
    return {
      address: t.address,
      name: t.name,
      symbol: t.symbol,
      decimals: parseInt(t.decimals),
      totalSupply: t.totalSupply,
      price: t.price,
      marketCap: t.marketCap,
      volume24h: t.volume24h,
      holders: parseInt(t.holders),
      transfers: parseInt(t.transfers),
    };
  }

  async getTokenHolders(
    address: string, 
    page: number = 1, 
    limit: number = 50
  ): Promise<TokenHolder[]> {
    const response = await this.http.get(`/api/v2/token/${address}/holders`, {
      params: { page, limit },
    });
    
    return (response.data.result || []).map((h: any) => ({
      address: h.address,
      balance: h.balance,
      percent: parseFloat(h.percent),
    }));
  }

  async getTokenTransfers(
    address: string, 
    page: number = 1, 
    limit: number = 50
  ): Promise<any[]> {
    const response = await this.http.get(`/api/v2/token/${address}/transfers`, {
      params: { page, limit },
    });
    return response.data.result || [];
  }

  async getTokenPriceHistory(address: string, days: number = 30): Promise<ChartDataPoint[]> {
    const response = await this.http.get(`/api/v2/token/${address}/history`, {
      params: { days },
    });
    
    return (response.data.result || []).map((p: any) => ({
      timestamp: p.timestamp,
      value: parseFloat(p.price),
    }));
  }

  async getHolderDistribution(address: string): Promise<any[]> {
    const response = await this.http.get(`/api/v2/token/${address}/holderdistribution`);
    return response.data.result || [];
  }

  // NFT Operations

  async getNFTs(page: number = 1, filter: string = 'erc721'): Promise<NFT[]> {
    const response = await this.http.get('/api/v2/nfts', {
      params: { page, filter },
    });
    
    return (response.data.result || []).map((n: any) => ({
      address: n.address,
      name: n.name,
      symbol: n.symbol,
      totalSupply: parseInt(n.totalSupply),
      floorPrice: n.floorPrice,
      holders: parseInt(n.holders),
      transfers: parseInt(n.transfers),
      royaltyBps: parseInt(n.royaltyBps),
    }));
  }

  async getNFT(address: string): Promise<NFT> {
    const response = await this.http.get(`/api/v2/nft/${address}`);
    const n = response.data.result;
    
    return {
      address: n.address,
      name: n.name,
      symbol: n.symbol,
      totalSupply: parseInt(n.totalSupply),
      floorPrice: n.floorPrice,
      holders: parseInt(n.holders),
      transfers: parseInt(n.transfers),
      royaltyBps: parseInt(n.royaltyBps),
    };
  }

  async getNFTHolders(address: string, page: number = 1): Promise<any[]> {
    const response = await this.http.get(`/api/v2/nft/${address}/holders`, {
      params: { page },
    });
    return response.data.result || [];
  }

  async getNFTTransfers(address: string, page: number = 1): Promise<any[]> {
    const response = await this.http.get(`/api/v2/nft/${address}/transfers`, {
      params: { page },
    });
    return response.data.result || [];
  }

  async getNFTFloorPrice(address: string): Promise<string> {
    const response = await this.http.get(`/api/v2/nft/${address}/floor`);
    return response.data.result?.floorPrice || '0';
  }

  async getNFTMetadata(address: string, tokenId: string): Promise<any> {
    const response = await this.http.get(`/api/v2/nft/${address}/metadata`, {
      params: { tokenId },
    });
    return response.data.result;
  }

  async getNFTTraits(address: string): Promise<any[]> {
    const response = await this.http.get(`/api/v2/nft/${address}/traits`);
    return response.data.result || [];
  }

  // Analytics

  async getNetworkStats(): Promise<NetworkStats> {
    const response = await this.http.get('/api/v2/stats');
    const s = response.data.result;
    
    return {
      blockNumber: parseInt(s.blockNumber),
      totalAddresses: parseInt(s.totalAddresses),
      totalTransactions: parseInt(s.totalTransactions),
      totalValueLocked: s.totalValueLocked,
      marketCap: s.marketCap,
      tps: parseFloat(s.tps),
      gasPrice: s.gasPrice,
    };
  }

  async getTPS(interval: string = '24h'): Promise<number> {
    const response = await this.http.get('/api/v2/stats/tps', {
      params: { interval },
    });
    return parseFloat(response.data.result?.tps || '0');
  }

  async getGasPrice(): Promise<GasPrice> {
    const response = await this.http.get('/api/v2/stats/gas');
    const g = response.data.result;
    
    return {
      slow: g.slow,
      standard: g.standard,
      fast: g.fast,
    };
  }

  async getTVL(): Promise<string> {
    const response = await this.http.get('/api/v2/stats/tvl');
    return response.data.result?.tvl || '0';
  }

  async getMarketCap(): Promise<string> {
    const response = await this.http.get('/api/v2/stats/marketcap');
    return response.data.result?.marketcap || '0';
  }

  async getRichList(page: number = 1): Promise<any[]> {
    const response = await this.http.get('/api/v2/stats/richlist', {
      params: { page },
    });
    return response.data.result || [];
  }

  async getTopTokens(page: number = 1): Promise<Token[]> {
    const response = await this.http.get('/api/v2/stats/toptokens', {
      params: { page },
    });
    
    return (response.data.result || []).map((t: any) => ({
      address: t.address,
      name: t.name,
      symbol: t.symbol,
      decimals: parseInt(t.decimals),
      totalSupply: t.totalSupply,
      price: t.price,
      marketCap: t.marketCap,
      volume24h: t.volume24h,
      holders: parseInt(t.holders),
      transfers: parseInt(t.transfers),
    }));
  }

  async getTopNFTs(page: number = 1): Promise<NFT[]> {
    const response = await this.http.get('/api/v2/stats/topnfts', {
      params: { page },
    });
    
    return (response.data.result || []).map((n: any) => ({
      address: n.address,
      name: n.name,
      symbol: n.symbol,
      totalSupply: parseInt(n.totalSupply),
      floorPrice: n.floorPrice,
      holders: parseInt(n.holders),
      transfers: parseInt(n.transfers),
      royaltyBps: parseInt(n.royaltyBps),
    }));
  }

  // Chart Data

  async getTPSChart(interval: string = '24h'): Promise<ChartDataPoint[]> {
    const response = await this.http.get('/charts/tps', {
      params: { interval },
    });
    
    const series = response.data.series?.[0];
    return (series?.data || []).map((d: any) => ({
      timestamp: new Date(d.timestamp).getTime(),
      value: d.value,
    }));
  }

  async getGasChart(interval: string = '24h'): Promise<any> {
    const response = await this.http.get('/charts/gas', {
      params: { interval },
    });
    return response.data;
  }

  async getTVLChart(interval: string = '30d'): Promise<ChartDataPoint[]> {
    const response = await this.http.get('/charts/tvl', {
      params: { interval },
    });
    
    const series = response.data.series?.[0];
    return (series?.data || []).map((d: any) => ({
      timestamp: new Date(d.timestamp).getTime(),
      value: d.value,
    }));
  }

  async getTokenPriceChart(
    address: string, 
    interval: string = '30d'
  ): Promise<ChartDataPoint[]> {
    const response = await this.http.get(`/charts/token/${address}`, {
      params: { interval },
    });
    
    const series = response.data.series?.[0];
    return (series?.data || []).map((d: any) => ({
      timestamp: new Date(d.timestamp).getTime(),
      value: d.value,
    }));
  }

  async getGasHeatmap(days: number = 7): Promise<any[]> {
    const response = await this.http.get('/charts/heatmap', {
      params: { days },
    });
    return response.data.heatmap || [];
  }

  // Search

  async search(query: string): Promise<any> {
    const response = await this.http.get('/api/v2/search', {
      params: { q: query },
    });
    return response.data.result;
  }

  // Contract Interaction

  async readContract(
    address: string, 
    method: string, 
    params: string[] = []
  ): Promise<string> {
    const response = await this.http.get(`/api/v2/contract/${address}/read`, {
      params: { method, params },
    });
    return response.data.result;
  }

  async writeContract(
    address: string,
    method: string,
    params: string[],
    fromAddress: string
  ): Promise<string> {
    const response = await this.http.post(`/api/v2/contract/${address}/write`, {
      method,
      params,
      from: fromAddress,
    });
    return response.data.result;
  }

  // Export

  async exportData(
    exportType: string,
    options: {
      address?: string;
      format?: string;
      startBlock?: number;
      endBlock?: number;
      limit?: number;
    } = {}
  ): Promise<any> {
    const response = await this.http.get('/export', {
      params: {
        type: exportType,
        ...options,
      },
    });
    
    if (options.format === 'csv') {
      return response.data;
    }
    return response.data.results || [];
  }
}

// Wallet Class
export class TigerScanWallet {
  private privateKey: string;
  private wallet: Wallet;
  private sdk: TigerScanSDK;

  constructor(privateKey: string, sdk?: TigerScanSDK) {
    this.privateKey = privateKey;
    this.wallet = new Wallet(privateKey);
    this.sdk = sdk || new TigerScanSDK();
  }

  get address(): string {
    return this.wallet.address;
  }

  async getNonce(): Promise<number> {
    const account = await this.sdk.getAccount(this.wallet.address);
    return account.transactionCount;
  }

  async sendTransaction(
    to: string,
    value: string = '0',
    gasLimit: number = 21000
  ): Promise<string> {
    const gasData = await this.sdk.getGasPrice();
    const gasPrice = BigNumber.from(gasData.standard);

    const tx = {
      to,
      value: BigNumber.from(value),
      gasLimit,
      gasPrice,
    };

    const signedTx = await this.wallet.signTransaction(tx);
    // In production, broadcast via RPC
    return signedTx.hash;
  }

  async transferToken(
    tokenAddress: string,
    to: string,
    amount: string
  ): Promise<string> {
    const token = new Contract(
      tokenAddress,
      ['function transfer(address to, uint256 amount)'],
      this.wallet
    );

    const tx = await token.transfer(to, BigNumber.from(amount));
    return tx.hash;
  }
}

// Utility Functions
export function createSDK(config?: SDKConfig): TigerScanSDK {
  return new TigerScanSDK(config);
}

export function createWallet(
  privateKey: string, 
  sdk?: TigerScanSDK
): TigerScanWallet {
  return new TigerScanWallet(privateKey, sdk);
}

// Export types
export type {
  Block,
  Transaction,
  TransactionReceipt,
  Log,
  Token,
  TokenHolder,
  NFT,
  Account,
  NetworkStats,
  GasPrice,
  ChartDataPoint,
};

// Export default
export default TigerScanSDK;