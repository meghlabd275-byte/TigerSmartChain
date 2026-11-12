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

// The gateway registers every route under /api/v1 (see
// go_services/internal/gateway/endpoints.go). Requests have to carry that
// prefix, so it is appended here rather than at each call site. A deployment
// that already includes the prefix in NEXT_PUBLIC_API_URL is left alone.
const API_ROOT = (process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080').replace(/\/+$/, '')
const API_URL = /\/api\/v1$/.test(API_ROOT) ? API_ROOT : `${API_ROOT}/api/v1`

// The gateway returns database columns as-is, so every response key is
// snake_case. The types in @/types are camelCase, so the wire format is
// converted here in one place instead of at each of the ~250 call sites.
// Only keys are rewritten; string values are left untouched.
const snakeToCamel = (key: string): string =>
  key.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase())

const convertKeys = (value: unknown): any => {
  if (Array.isArray(value)) {
    return value.map(convertKeys)
  }
  if (value !== null && typeof value === 'object') {
    const out: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      out[snakeToCamel(k)] = convertKeys(v)
    }
    return out
  }
  return value
}

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
      (response) => {
        response.data = convertKeys(response.data)
        return response
      },
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

  // Chain-wide transfer feed, used by the transfer tracker page.
  async getRecentTransfers(params?: {
    page?: number
    limit?: number
  }): Promise<PaginatedResponse<Record<string, unknown>>> {
    const { data } = await this.client.get<PaginatedResponse<Record<string, unknown>>>(
      '/transfers', { params }
    )
    return data
  }

  async getTransferStats(): Promise<Record<string, any>> {
    const { data } = await this.client.get('/transfers/stats')
    return data
  }

  async getTransferSeries(params?: { hours?: number }): Promise<PaginatedResponse<Record<string, unknown>>> {
    const { data } = await this.client.get<PaginatedResponse<Record<string, unknown>>>(
      '/transfers/series', { params }
    )
    return data
  }

  async getTokenPriceHistory(tokenAddress: string, params?: {
    timeframe?: '1h' | '24h' | '7d' | '30d' | '1y'
  }): Promise<ChartData[]> {
    const { data } = await this.client.get<ChartData[]>(`/tokens/${tokenAddress}/price-history`, { params })
    return data
  }

  // Holder concentration and balance-bucket counts, computed by the indexer.
  async getTokenAnalytics(tokenAddress: string): Promise<{
    holderCount: number
    totalBalance: string
    totalValueUsd: string
    top10Percent: number
    top50Percent: number
    top100Percent: number
    above10m: number
    from1mTo10m: number
    from100kTo1m: number
    from10kTo100k: number
    from1kTo10k: number
    below1k: number
    transferVolume: string
    transferCount: number
  }> {
    const { data } = await this.client.get(`/tokens/${tokenAddress}/analytics`)
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

  async getNFTTokens(collectionAddress: string, params?: {
    page?: number
    limit?: number
  }): Promise<PaginatedResponse<Record<string, unknown>>> {
    const { data } = await this.client.get<PaginatedResponse<Record<string, unknown>>>(
      `/nfts/${collectionAddress}/tokens`,
      { params }
    )
    return data
  }

  // The gateway exposes owners per collection (/nfts/:address/owners), not per
  // token id, and returns rows rather than bare addresses.
  async getNFTOwners(collectionAddress: string, params?: {
    page?: number
    limit?: number
  }): Promise<{ tokenId: string; owner: string }[]> {
    const { data } = await this.client.get<PaginatedResponse<{ tokenId: string; owner: string }>>(
      `/nfts/${collectionAddress}/owners`,
      { params }
    )
    return data.items
  }

  // Marketplace. These read indexed rows; nothing is generated client-side.
  async getNFTMarketplaceCollections(params?: { page?: number; limit?: number }):
    Promise<PaginatedResponse<Record<string, unknown>>> {
    const { data } = await this.client.get<PaginatedResponse<Record<string, unknown>>>(
      '/nfts/marketplace/collections', { params }
    )
    return data
  }

  async getNFTMarketplaceStats(): Promise<Record<string, string>> {
    const { data } = await this.client.get('/nfts/marketplace/stats')
    return data
  }

  async getNFTSales(params?: { page?: number; limit?: number }):
    Promise<PaginatedResponse<Record<string, unknown>>> {
    const { data } = await this.client.get<PaginatedResponse<Record<string, unknown>>>(
      '/nfts/sales', { params }
    )
    return data
  }

  async getNFTCollectionSales(collectionAddress: string, params?: { page?: number; limit?: number }):
    Promise<PaginatedResponse<Record<string, unknown>>> {
    const { data } = await this.client.get<PaginatedResponse<Record<string, unknown>>>(
      `/nfts/${collectionAddress}/sales`, { params }
    )
    return data
  }

  async getNFTCollectionSalesSeries(collectionAddress: string, params?: { limit?: number }):
    Promise<PaginatedResponse<Record<string, unknown>>> {
    const { data } = await this.client.get<PaginatedResponse<Record<string, unknown>>>(
      `/nfts/${collectionAddress}/sales-series`, { params }
    )
    return data
  }

  async getNFTCollectionHoldersSeries(collectionAddress: string, params?: { limit?: number }):
    Promise<PaginatedResponse<Record<string, unknown>>> {
    const { data } = await this.client.get<PaginatedResponse<Record<string, unknown>>>(
      `/nfts/${collectionAddress}/holders-series`, { params }
    )
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

  async getNFTFloorHistory(collectionAddress: string, params?: { limit?: number }): Promise<PaginatedResponse<any>> {
    const { data } = await this.client.get<PaginatedResponse<any>>(
      `/nfts/${collectionAddress}/floor-history`,
      { params }
    )
    return data
  }

  async getNFTVolumeHistory(collectionAddress: string, params?: { limit?: number }): Promise<PaginatedResponse<any>> {
    const { data } = await this.client.get<PaginatedResponse<any>>(
      `/nfts/${collectionAddress}/volume-history`,
      { params }
    )
    return data
  }

  // Transfers of one token, rather than of the whole collection.
  async getNFTTokenHistory(
    collectionAddress: string,
    tokenId: string,
    params?: { limit?: number }
  ): Promise<PaginatedResponse<NFTTransfer>> {
    const { data } = await this.client.get<PaginatedResponse<NFTTransfer>>(
      `/nfts/${collectionAddress}/tokens/${tokenId}/history`,
      { params }
    )
    return data
  }

  async getNFTTokenTransfers(
    collectionAddress: string,
    tokenId: string,
    params?: { limit?: number }
  ): Promise<PaginatedResponse<NFTTransfer>> {
    const { data } = await this.client.get<PaginatedResponse<NFTTransfer>>(
      `/nfts/${collectionAddress}/tokens/${tokenId}/transfers`,
      { params }
    )
    return data
  }

  async getNFTTokenOwner(collectionAddress: string, tokenId: string): Promise<{ owner: string }> {
    const { data } = await this.client.get<{ owner: string }>(
      `/nfts/${collectionAddress}/tokens/${tokenId}/owner`
    )
    return data
  }

  async getNFTRarity(collectionAddress: string, tokenId: string): Promise<any> {
    const { data } = await this.client.get<any>(
      `/nfts/${collectionAddress}/rarity/${tokenId}`
    )
    return data
  }

  // Developer tools. Selectors and event topics are computed server-side from
  // keccak256, so the client never carries a table that can go stale.
  async computeSignature(signature: string, kind: 'function' | 'event' = 'function'):
    Promise<{ signature: string; kind: string; selector?: string; topic?: string }> {
    const { data } = await this.client.post('/tools/signature', { signature, kind })
    return data
  }

  async lookupSignature(selector: string): Promise<{ selector: string; signature: string; source: string }> {
    const { data } = await this.client.get(`/tools/signature/${encodeURIComponent(selector)}`)
    return data
  }

  async encodeCall(signature: string, args: unknown[]):
    Promise<{ signature: string; selector: string; calldata: string }> {
    const { data } = await this.client.post('/tools/encode-call', { signature, args })
    return data
  }

  async encodeEventTopic(signature: string): Promise<{ signature: string; topic0: string }> {
    const { data } = await this.client.post('/tools/encode-event-topic', { signature })
    return data
  }

  async getNativePrice(): Promise<{ symbol: string; name: string; priceUsd: string; priceChange24h: string }> {
    const { data } = await this.client.get('/gas/native-price')
    return data
  }

  // Gas network statistics and the sampled price series behind the tracker.
  async getGasNetworkStats(): Promise<Record<string, any>> {
    const { data } = await this.client.get('/gas/network-stats')
    return data
  }

  async getGasPriceSeries(params?: { limit?: number }): Promise<PaginatedResponse<Record<string, unknown>>> {
    const { data } = await this.client.get<PaginatedResponse<Record<string, unknown>>>(
      '/gas/price-series', { params }
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

  // Contract ABI, as stored by the verifier. Returns [] when the address is
  // not a verified contract.
  async getContractABI(address: string): Promise<{ abi: any[] }> {
    const { data } = await this.client.get<{ abi: any[] }>(`/contracts/${address}/abi`)
    return data
  }

  // Read-only JSON-RPC passthrough. The gateway forwards only methods on its
  // allow-list (eth_call, eth_estimateGas, debug_trace*, trace_*); anything
  // else is refused with 403.
  async rpcCall(method: string, params: unknown[]): Promise<any> {
    const { data } = await this.client.post('/rpc', { method, params })
    return data
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

  async getGasHistory(params?: {
    timeframe?: '24h' | '7d' | '30d' | '1y'
  }): Promise<ChartData[]> {
    const { data } = await this.client.get<ChartData[]>('/charts/gas', { params })
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

  // Protocol-wide DEX totals, summed in SQL over every indexed pair.
  async getDexStats(): Promise<{
    totalPairs: number
    totalLiquidity: string
    totalVolume: string
  }> {
    const { data } = await this.client.get('/stats/dex')
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
    const { data } = await this.client.get<any>(`/txs/${txHash}/state-diff`)
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

  // Whale activity
  async getWhales(params?: { page?: number; limit?: number }): Promise<PaginatedResponse<any>> {
    const { data } = await this.client.get<PaginatedResponse<any>>('/whales', { params })
    return data
  }

  async getWhalesByAddress(address: string, params?: { page?: number; limit?: number }): Promise<PaginatedResponse<any>> {
    const { data } = await this.client.get<PaginatedResponse<any>>(`/whales/${address}`, { params })
    return data
  }

  // Security
  async getSecurityStats(): Promise<{
    malicious_contracts: number
    phishing_urls: number
    scam_tokens: number
  }> {
    const { data } = await this.client.get('/security/stats')
    return data
  }

  async getMaliciousContracts(params?: { page?: number; limit?: number }): Promise<any[]> {
    const { data } = await this.client.get<any[]>('/security/malicious-contracts', { params })
    return data
  }

  async getPhishingURLs(params?: { page?: number; limit?: number }): Promise<any[]> {
    const { data } = await this.client.get<any[]>('/security/phishing-urls', { params })
    return data
  }

  async getScamTokens(params?: { page?: number; limit?: number }): Promise<any[]> {
    const { data } = await this.client.get<any[]>('/security/scam-tokens', { params })
    return data
  }

  async checkAddressSecurity(address: string): Promise<{
    address: string
    is_flagged: boolean
    threat_count: number
    threats: any[]
  }> {
    const { data } = await this.client.get(`/security/check/${address}`)
    return data
  }

  // Staking
  async getStakingDelegations(params?: { page?: number; limit?: number }): Promise<any[]> {
    const { data } = await this.client.get<any[]>('/staking/delegations', { params })
    return data
  }

  async getStakingByDelegator(address: string, params?: { page?: number; limit?: number }): Promise<any[]> {
    const { data } = await this.client.get<any[]>(`/staking/delegator/${address}`, { params })
    return data
  }

  // Flash loans
  async getFlashLoans(params?: { page?: number; limit?: number }): Promise<PaginatedResponse<any>> {
    const { data } = await this.client.get<PaginatedResponse<any>>('/flash-loans', { params })
    return data
  }

  // Bridge
  async getBridgeTransfers(params?: { page?: number; limit?: number }): Promise<any[]> {
    const { data } = await this.client.get<any[]>('/bridge/transfers', { params })
    return data
  }

  async getBridgeTransfer(transferId: string): Promise<any> {
    const { data } = await this.client.get<any>(`/bridge/transfers/${transferId}`)
    return data
  }

  // NFT approvals
  async getNFTApprovals(params?: { page?: number; limit?: number }): Promise<any[]> {
    const { data } = await this.client.get<any[]>('/nft-approvals', { params })
    return data
  }

  async getNFTApprovalsByOwner(address: string, params?: { page?: number; limit?: number }): Promise<any[]> {
    const { data } = await this.client.get<any[]>(`/nft-approvals/${address}`, { params })
    return data
  }

  // Token approvals. The risk label is derived by the gateway from the
  // malicious-contract and scam-token tables.
  async getApprovals(params?: { page?: number; limit?: number }): Promise<PaginatedResponse<any>> {
    const { data } = await this.client.get<PaginatedResponse<any>>('/approvals', { params })
    return data
  }

  async getApprovalStats(): Promise<{
    total: number
    safe: number
    warning: number
    danger: number
  }> {
    const { data } = await this.client.get('/approvals/stats')
    return data
  }

  async getApprovalsByOwner(address: string, params?: { page?: number; limit?: number }): Promise<PaginatedResponse<any>> {
    const { data } = await this.client.get<PaginatedResponse<any>>(`/approvals/owner/${address}`, { params })
    return data
  }

  async getApprovalsBySpender(address: string, params?: { page?: number; limit?: number }): Promise<PaginatedResponse<any>> {
    const { data } = await this.client.get<PaginatedResponse<any>>(`/approvals/spender/${address}`, { params })
    return data
  }

  // 24-hour whale, MEV and flash-loan totals, summed in SQL.
  async getWhaleStats(): Promise<Record<string, string | number>> {
    const { data } = await this.client.get('/whales/stats')
    return data
  }

  async getMEVBundles(params?: { page?: number; limit?: number }): Promise<PaginatedResponse<any>> {
    const { data } = await this.client.get<PaginatedResponse<any>>('/mev/bundles', { params })
    return data
  }
}

export const api = new APIClient()
export default api
