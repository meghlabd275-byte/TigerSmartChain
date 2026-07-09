import axios, { AxiosInstance } from 'axios'
import type {
  Block,
  BlockListItem,
  Transaction,
  Token,
  TokenHolder,
  TokenTransfer,
  TokenPriceHistory,
  NFTCollection,
  NFTToken,
  NFTTransfer,
  Contract,
  NetworkStats,
  GasOracle,
  Address,
  SearchResult,
  DexPair,
  GovernanceProposal,
  PaginatedResponse,
  ChartData,
  InternalTransaction,
} from '@/types'

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080'

class APIClient {
  private client: AxiosInstance

  constructor() {
    this.client = axios.create({
      baseURL: API_URL,
      timeout: 30000,
      headers: {
        'Content-Type': 'application/json',
      },
    })

    this.client.interceptors.response.use(
      (response) => response,
      (error) => {
        console.error('API Error:', error.message)
        return Promise.reject(error)
      }
    )
  }

  // Block APIs
  async getBlock(blockNumber: number): Promise<Block> {
    const { data } = await this.client.get<Block>(`/blocks/${blockNumber}`)
    return data
  }

  async getLatestBlock(): Promise<Block> {
    const { data } = await this.client.get<Block>('/blocks/latest')
    return data
  }

  async getBlocks(params: {
    page?: number
    limit?: number
    startBlock?: number
    endBlock?: number
  }): Promise<PaginatedResponse<BlockListItem>> {
    const { data } = await this.client.get<PaginatedResponse<BlockListItem>>('/blocks', { params })
    return data
  }

  // Transaction APIs
  async getTransaction(txHash: string): Promise<Transaction> {
    const { data } = await this.client.get<Transaction>(`/txs/${txHash}`)
    return data
  }

  async getTransactions(params: {
    page?: number
    limit?: number
    address?: string
    block?: number
  }): Promise<PaginatedResponse<Transaction>> {
    const { data } = await this.client.get<PaginatedResponse<Transaction>>('/txs', { params })
    return data
  }

  async getPendingTransactions(): Promise<Transaction[]> {
    const { data } = await this.client.get<Transaction[]>('/txs/pending')
    return data
  }

  // Internal Transaction APIs
  async getInternalTransactions(txHash: string): Promise<InternalTransaction[]> {
    const { data } = await this.client.get<InternalTransaction[]>(`/internal-txs/${txHash}`)
    return data
  }

  async getInternalTransactionsByAddress(address: string, params?: {
    page?: number
    limit?: number
  }): Promise<PaginatedResponse<InternalTransaction>> {
    const { data } = await this.client.get<PaginatedResponse<InternalTransaction>>(
      `/internal-txs/address/${address}`,
      { params }
    )
    return data
  }

  // Token APIs
  async getToken(tokenAddress: string): Promise<Token> {
    const { data } = await this.client.get<Token>(`/tokens/${tokenAddress}`)
    return data
  }

  async getTokens(params?: {
    page?: number
    limit?: number
    type?: string
    verified?: boolean
  }): Promise<PaginatedResponse<Token>> {
    const { data } = await this.client.get<PaginatedResponse<Token>>('/tokens', { params })
    return data
  }

  async getTokenHolders(tokenAddress: string, params?: {
    page?: number
    limit?: number
  }): Promise<PaginatedResponse<TokenHolder>> {
    const { data } = await this.client.get<PaginatedResponse<TokenHolder>>(
      `/tokens/${tokenAddress}/holders`,
      { params }
    )
    return data
  }

  async getTokenTransfers(tokenAddress: string, params?: {
    page?: number
    limit?: number
  }): Promise<PaginatedResponse<TokenTransfer>> {
    const { data } = await this.client.get<PaginatedResponse<TokenTransfer>>(
      `/tokens/${tokenAddress}/transfers`,
      { params }
    )
    return data
  }

  async getTokenPriceHistory(tokenAddress: string, params?: {
    timeframe?: '1h' | '24h' | '7d' | '30d' | '1y'
  }): Promise<ChartData[]> {
    const { data } = await this.client.get<ChartData[]>(`/tokens/${tokenAddress}/price-history`, { params })
    return data
  }

  // NFT APIs
  async getNFTCollection(collectionAddress: string): Promise<NFTCollection> {
    const { data } = await this.client.get<NFTCollection>(`/nfts/${collectionAddress}`)
    return data
  }

  async getNFTCollections(params?: {
    page?: number
    limit?: number
  }): Promise<PaginatedResponse<NFTCollection>> {
    const { data } = await this.client.get<PaginatedResponse<NFTCollection>>('/nfts', { params })
    return data
  }

  async getNFTToken(collectionAddress: string, tokenId: string): Promise<NFTToken> {
    const { data } = await this.client.get<NFTToken>(`/nfts/${collectionAddress}/tokens/${tokenId}`)
    return data
  }

  async getNFTOwners(collectionAddress: string, tokenId: string): Promise<string[]> {
    const { data } = await this.client.get<string[]>(`/nfts/${collectionAddress}/tokens/${tokenId}/owners`)
    return data
  }

  async getNFTTransfers(collectionAddress: string, params?: {
    page?: number
    limit?: number
  }): Promise<PaginatedResponse<NFTTransfer>> {
    const { data } = await this.client.get<PaginatedResponse<NFTTransfer>>(
      `/nfts/${collectionAddress}/transfers`,
      { params }
    )
    return data
  }

  async getNFTFloorPrice(collectionAddress: string): Promise<{ floor: number; average: number }> {
    const { data } = await this.client.get<{ floor: number; average: number }>(
      `/nfts/${collectionAddress}/floor`
    )
    return data
  }

  // Contract APIs
  async getContract(address: string): Promise<Contract> {
    const { data } = await this.client.get<Contract>(`/contracts/${address}`)
    return data
  }

  async verifyContract(data: {
    address: string
    sourceCode: string
    compilerVersion: string
    contractName: string
    optimization: boolean
    runs: number
  }): Promise<{ success: boolean; message: string }> {
    const { data: response } = await this.client.post<{ success: boolean; message: string }>(
      '/contracts/verify',
      data
    )
    return response
  }

  async getVerifiedContracts(params?: {
    page?: number
    limit?: number
  }): Promise<PaginatedResponse<Contract>> {
    const { data } = await this.client.get<PaginatedResponse<Contract>>('/contracts/verified', { params })
    return data
  }

  // Address APIs
  async getAddress(address: string): Promise<Address> {
    const { data } = await this.client.get<Address>(`/addresses/${address}`)
    return data
  }

  async getAddressTokens(address: string): Promise<Token[]> {
    const { data } = await this.client.get<Token[]>(`/addresses/${address}/tokens`)
    return data
  }

  async getAddressNFTs(address: string): Promise<NFTToken[]> {
    const { data } = await this.client.get<NFTToken[]>(`/addresses/${address}/nfts`)
    return data
  }

  // Search APIs
  async search(query: string): Promise<SearchResult[]> {
    const { data } = await this.client.get<SearchResult[]>('/search', { params: { q: query } })
    return data
  }

  async searchAdvanced(params: {
    q: string
    type?: string
    fromBlock?: number
    toBlock?: number
  }): Promise<SearchResult[]> {
    const { data } = await this.client.get<SearchResult[]>('/search/advanced', { params })
    return data
  }

  // Analytics APIs
  async getNetworkStats(): Promise<NetworkStats> {
    const { data } = await this.client.get<NetworkStats>('/stats/network')
    return data
  }

  async getTransactionHistory(params?: {
    timeframe?: '24h' | '7d' | '30d' | '1y'
  }): Promise<ChartData[]> {
    const { data } = await this.client.get<ChartData[]>('/charts/transactions', { params })
    return data
  }

  async getAddressHistory(params?: {
    timeframe?: '24h' | '7d' | '30d' | '1y'
  }): Promise<ChartData[]> {
    const { data } = await this.client.get<ChartData[]>('/charts/addresses', { params })
    return data
  }

  async getGasOracle(): Promise<GasOracle> {
    const { data } = await this.client.get<GasOracle>('/gas/oracle')
    return data
  }

  // DEX APIs
  async getDexPairs(params?: {
    page?: number
    limit?: number
  }): Promise<PaginatedResponse<DexPair>> {
    const { data } = await this.client.get<PaginatedResponse<DexPair>>('/dex/pairs', { params })
    return data
  }

  async getDexPair(pairAddress: string): Promise<DexPair> {
    const { data } = await this.client.get<DexPair>(`/dex/pairs/${pairAddress}`)
    return data
  }

  // Governance APIs
  async getGovernanceProposals(params?: {
    page?: number
    limit?: number
    status?: string
  }): Promise<PaginatedResponse<GovernanceProposal>> {
    const { data } = await this.client.get<PaginatedResponse<GovernanceProposal>>(
      '/governance/proposals',
      { params }
    )
    return data
  }

  async getGovernanceProposal(proposalId: string): Promise<GovernanceProposal> {
    const { data } = await this.client.get<GovernanceProposal>(`/governance/proposals/${proposalId}`)
    return data
  }

  // Trace APIs
  async getTrace(txHash: string): Promise<any> {
    const { data } = await this.client.get<any>(`/trace/${txHash}`)
    return data
  }

  async getStateDiff(txHash: string): Promise<any> {
    const { data } = await this.client.get<any>(`/state-diff/${txHash}`)
    return data
  }

  // Labels APIs
  async getLabels(): Promise<{ category: string; labels: { address: string; name: string }[] }[]> {
    const { data } = await this.client.get('/labels')
    return data
  }

  async getAddressLabel(address: string): Promise<string | null> {
    const { data } = await this.client.get<string | null>(`/labels/${address}`)
    return data
  }

  // Block Transactions
  async getBlockTransactions(blockNumber: number): Promise<PaginatedResponse<Transaction>> {
    const { data } = await this.client.get<PaginatedResponse<Transaction>>(`/blocks/${blockNumber}/transactions`)
    return data
  }
}

export const api = new APIClient()
export default api
