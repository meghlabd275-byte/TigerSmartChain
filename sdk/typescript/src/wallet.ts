/**
 * TigerWallet - Wallet implementation for TigerSmartChain
 */

import { ethers, Signer, Wallet as EthersWallet, JsonRpcProvider, Contract } from 'ethers';
import type { ProviderConfig, TransactionReceipt } from './types';
import { TEP20_ABI } from './abis/tep20';
import { TEP721_ABI } from './abis/tep721';

/**
 * TigerWallet class
 */
export class TigerWallet {
  private wallet: EthersWallet;
  private provider: JsonRpcProvider;
  private address: string;

  /**
   * Create or open a wallet
   */
  static create(privateKey: string, config?: ProviderConfig): TigerWallet {
    const provider = new JsonRpcProvider(config?.rpcUrl || 'http://localhost:8545');
    const wallet = new EthersWallet(privateKey, provider);
    
    return new TigerWallet(wallet, provider);
  }

  /**
   * Create a new wallet
   */
  static createRandom(config?: ProviderConfig): TigerWallet {
    const wallet = EthersWallet.createRandom();
    
    return new TigerWallet(
      wallet,
      new JsonRpcProvider(config?.rpcUrl || 'http://localhost:8545')
    );
  }

  /**
   * Create wallet from mnemonic
   */
  static fromMnemonic(mnemonic: string, config?: ProviderConfig): TigerWallet {
    const wallet = EthersWallet.fromPhrase(mnemonic);
    
    return new TigerWallet(
      wallet,
      new JsonRpcProvider(config?.rpcUrl || 'http://localhost:8545')
    );
  }

  /**
   * Constructor
   */
  constructor(wallet: EthersWallet, provider: JsonRpcProvider) {
    this.wallet = wallet;
    this.provider = provider;
    this.address = wallet.address;
  }

  /**
   * Get wallet address
   */
  getAddress(): string {
    return this.address;
  }

  /**
   * Get provider
   */
  getProvider(): JsonRpcProvider {
    return this.provider;
  }

  /**
   * Get signer
   */
  getSigner(): Signer {
    return this.wallet;
  }

  /**
   * Get balance
   */
  async getBalance(): Promise<bigint> {
    return this.provider.getBalance(this.address);
  }

  /**
   * Get transaction count (nonce)
   */
  async getTransactionCount(): Promise<number> {
    return this.provider.getTransactionCount(this.address);
  }

  /**
   * Send transaction
   */
  async sendTransaction(tx: {
    to: string;
    value?: bigint;
    data?: string;
    gasLimit?: bigint;
    gasPrice?: bigint;
  }): Promise<TransactionReceipt> {
    const response = await this.wallet.sendTransaction({
      to: tx.to,
      value: tx.value || 0n,
      data: tx.data || '0x',
      gasLimit: tx.gasLimit,
      gasPrice: tx.gasPrice,
    });
    
    return response.wait();
  }

  /**
   * Transfer tokens (native)
   */
  async transfer(to: string, amount: bigint): Promise<TransactionReceipt> {
    return this.sendTransaction({
      to,
      value: amount,
    });
  }

  /**
   * Get TEP20 token balance
   */
  async getTokenBalance(tokenAddress: string): Promise<bigint> {
    const contract = new Contract(tokenAddress, TEP20_ABI, this.wallet);
    return contract.balanceOf(this.address) as Promise<bigint>;
  }

  /**
   * Transfer TEP20 tokens
   */
  async transferToken(
    tokenAddress: string,
    to: string,
    amount: bigint
  ): Promise<TransactionReceipt> {
    const contract = new Contract(tokenAddress, TEP20_ABI, this.wallet);
    const tx = await contract.transfer(to, amount);
    return tx.wait();
  }

  /**
   * Get TEP721 NFT balance
   */
  async getNFTBalance(nftAddress: string): Promise<number> {
    const contract = new Contract(nftAddress, TEP721_ABI, this.wallet);
    return contract.balanceOf(this.address) as Promise<number>;
  }

  /**
   * Transfer TEP721 NFT
   */
  async transferNFT(
    nftAddress: string,
    to: string,
    tokenId: bigint
  ): Promise<TransactionReceipt> {
    const contract = new Contract(nftAddress, TEP721_ABI, this.wallet);
    const tx = await contract.transferFrom(this.address, to, tokenId);
    return tx.wait();
  }

  /**
   * Sign message
   */
  async signMessage(message: string): Promise<string> {
    return this.wallet.signMessage(message);
  }

  /**
   * Sign transaction (without sending)
   */
  async signTransaction(tx: {
    to: string;
    value?: bigint;
    data?: string;
  }): Promise<string> {
    const populatedTx = await this.wallet.populateTransaction({
      to: tx.to,
      value: tx.value || 0n,
      data: tx.data || '0x',
    });
    
    return this.wallet.signTransaction(populatedTx);
  }

  /**
   * Encrypt wallet to JSON
   */
  async encrypt(password: string): Promise<string> {
    return this.wallet.encrypt(password);
  }

  /**
   * Get private key (be careful!)
   */
  getPrivateKey(): string {
    return this.wallet.privateKey;
  }

  /**
   * Connect to another provider
   */
  connect(provider: JsonRpcProvider): TigerWallet {
    const wallet = new EthersWallet(this.wallet.privateKey, provider);
    return new TigerWallet(wallet, provider);
  }
}