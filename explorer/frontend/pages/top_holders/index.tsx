// TigerScan - Top Holders (Rich List) Page with Full Features
// Complete holder rankings with distribution analysis

import { useState, useEffect, useMemo, useCallback } from 'react'
import Head from 'next/head'
import Link from 'next/link'

const API_BASE = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:12000'

interface Holder {
  address: string
  name?: string
  balance: string
  balance_usd: number
  percentage: number
  rank: number
  first_seen: number
  last_active: number
  tx_count: number
  token_count: number
}

interface TokenStats {
  total_holders: number
  total_supply: string
  circulating_supply: string
  holder_distribution: {
    '0-1K': number
    '1K-10K': number
    '10K-100K': number
    '100K-1M': number
    '1M+': number
  }
  whale_count: number
  avg_holdings: number
}

interface FilterState {
  search: string
  minBalance: string
  maxBalance: string
  sortBy: 'rank' | 'balance' | 'tx_count' | 'last_active'
}

// Compute distribution and percentage stats from real holder data
const computeStats = (holders: Holder[]): TokenStats => {
  const distribution = {
    '0-1K': 0,
    '1K-10K': 0,
    '10K-100K': 0,
    '100K-1M': 0,
    '1M+': 0,
  }
  
  let totalBalance = 0
  let whaleCount = 0
  
  holders.forEach(h => {
    const balance = parseFloat(h.balance) / 1e18
    totalBalance += balance
    
    if (balance < 1000) distribution['0-1K']++
    else if (balance < 10000) distribution['1K-10K']++
    else if (balance < 100000) distribution['10K-100K']++
    else if (balance < 1000000) distribution['100K-1M']++
    else {
      distribution['1M+']++
      whaleCount++
    }
  })
  
  const totalSupply = holders.reduce((sum, h) => sum + parseFloat(h.balance), 0)
  
  // Calculate percentages
  holders.forEach((h, i) => {
    h.percentage = (parseFloat(h.balance) / totalSupply) * 100
  })
  
  return {
    total_holders: holders.length,
    total_supply: String(totalSupply * 1e18),
    circulating_supply: String(totalSupply * 0.85 * 1e18),
    holder_distribution: distribution,
    whale_count: whaleCount,
    avg_holdings: holders.length ? totalBalance / holders.length : 0,
  }
}

export default function TopHolders() {
  const [holders, setHolders] = useState<Holder[]>([])
  const [stats, setStats] = useState<TokenStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [filter, setFilter] = useState<FilterState>({
    search: '',
    minBalance: '',
    maxBalance: '',
    sortBy: 'rank',
  })
  const [page, setPage] = useState(1)
  const [selectedToken, setSelectedToken] = useState('TGR')

  const fetchHolders = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const res = await fetch(`${API_BASE}/api/v1/top-holders`)
      if (!res.ok) throw new Error(`Failed to load top holders (${res.status})`)
      const data = await res.json()
      const list: Holder[] = Array.isArray(data) ? data : (data.holders ?? data.data ?? [])
      setHolders(list)
      setStats(list.length ? computeStats(list) : null)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load top holders')
      setHolders([])
      setStats(null)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    fetchHolders()
  }, [fetchHolders])

  // Filter and sort
  const filteredHolders = useMemo(() => {
    let filtered = [...holders]
    
    if (filter.search) {
      const search = filter.search.toLowerCase()
      filtered = filtered.filter(h =>
        h.address.toLowerCase().includes(search) ||
        h.name?.toLowerCase().includes(search)
      )
    }
    
    if (filter.minBalance) {
      const min = parseFloat(filter.minBalance) * 1e18
      filtered = filtered.filter(h => parseFloat(h.balance) >= min)
    }
    
    if (filter.maxBalance) {
      const max = parseFloat(filter.maxBalance) * 1e18
      filtered = filtered.filter(h => parseFloat(h.balance) <= max)
    }
    
    // Sort
    filtered.sort((a, b) => {
      switch (filter.sortBy) {
        case 'balance':
          return parseFloat(b.balance) - parseFloat(a.balance)
        case 'tx_count':
          return b.tx_count - a.tx_count
        case 'last_active':
          return b.last_active - a.last_active
        default:
          return a.rank - b.rank
      }
    })
    
    return filtered
  }, [holders, filter])

  // Paginate
  const paginatedHolders = useMemo(() => {
    const start = (page - 1) * 20
    return filteredHolders.slice(start, start + 20)
  }, [filteredHolders, page])

  const totalPages = Math.ceil(filteredHolders.length / 20)

  const formatAddress = (addr: string) => {
    if (!addr || addr.length < 10) return addr
    return `${addr.substring(0, 6)}...${addr.substring(38)}`
  }

  const formatBalance = (balance: string) => {
    const num = parseFloat(balance) / 1e18
    if (num >= 1000000) return `${(num / 1000000).toFixed(2)}M`
    if (num >= 1000) return `${(num / 1000).toFixed(2)}K`
    return num.toFixed(2)
  }

  const formatTime = (timestamp: number) => {
    const diff = Date.now() - timestamp
    const days = Math.floor(diff / 86400000)
    if (days === 0) return 'Today'
    if (days === 1) return 'Yesterday'
    if (days < 30) return `${days}d ago`
    if (days < 365) return `${Math.floor(days / 30)}mo ago`
    return `${Math.floor(days / 365)}y ago`
  }

  const getRankColor = (rank: number) => {
    if (rank === 1) return 'text-yellow-400'
    if (rank === 2) return 'text-gray-400'
    if (rank === 3) return 'text-orange-400'
    return 'text-gray-400'
  }

  if (loading) {
    return (
      <div className="min-h-screen bg-gray-900 flex items-center justify-center">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-orange-500 mx-auto mb-4"></div>
          <p className="text-gray-400">Loading Top Holders...</p>
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="min-h-screen bg-gray-900 flex items-center justify-center">
        <div className="text-center">
          <p className="text-red-400 mb-4">{error}</p>
          <button
            onClick={fetchHolders}
            className="px-4 py-2 bg-orange-500 hover:bg-orange-600 text-white rounded-lg"
          >
            Retry
          </button>
        </div>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-gray-900">
      <Head><title>Top Holders - TigerScan</title></Head>
      
      {/* Header */}
      <div className="bg-gray-800 border-b border-gray-700">
        <div className="container mx-auto px-4 py-4">
          <div className="flex items-center justify-between">
            <Link href="/" className="text-2xl font-bold text-orange-500">🐯 TigerScan</Link>
            <nav className="flex gap-6">
              <Link href="/tokens" className="text-gray-300 hover:text-white">Tokens</Link>
              <Link href="/top_holders" className="text-orange-400 hover:text-orange-300">Top Holders</Link>
            </nav>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="container mx-auto px-4 py-8">
        <h1 className="text-3xl font-bold text-white mb-8">Top Holders</h1>

        {/* Token Selector */}
        <div className="mb-6">
          <select
            value={selectedToken}
            onChange={(e) => setSelectedToken(e.target.value)}
            className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white"
          >
            <option value="TGR">TGR - TigerToken</option>
            <option value="WETH">WETH - Wrapped Ether</option>
            <option value="USDT">USDT - Tether</option>
            <option value="USDC">USDC - USD Coin</option>
          </select>
        </div>

        {/* Stats */}
        {stats && (
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
            <div className="bg-gray-800 rounded-lg p-4 border border-gray-700">
              <p className="text-gray-400 text-sm">Total Holders</p>
              <p className="text-2xl font-bold text-white">{stats.total_holders.toLocaleString()}</p>
            </div>
            <div className="bg-gray-800 rounded-lg p-4 border border-gray-700">
              <p className="text-gray-400 text-sm">Total Supply</p>
              <p className="text-2xl font-bold text-white">{formatBalance(stats.total_supply)} TGR</p>
            </div>
            <div className="bg-gray-800 rounded-lg p-4 border border-gray-700">
              <p className="text-gray-400 text-sm">Whale Accounts</p>
              <p className="text-2xl font-bold text-orange-400">{stats.whale_count}</p>
            </div>
            <div className="bg-gray-800 rounded-lg p-4 border border-gray-700">
              <p className="text-gray-400 text-sm">Avg Holdings</p>
              <p className="text-2xl font-bold text-green-400">{formatBalance(String(Math.floor(stats.avg_holdings * 1e18)))}</p>
            </div>
          </div>
        )}

        {/* Distribution Chart */}
        {stats && (
          <div className="bg-gray-800 rounded-lg p-6 border border-gray-700 mb-8">
            <h3 className="text-lg font-semibold text-white mb-4">Holder Distribution</h3>
            <div className="grid grid-cols-5 gap-4">
              {Object.entries(stats.holder_distribution).map(([range, count]) => (
                <div key={range} className="text-center">
                  <div className="h-32 bg-gray-700 rounded-lg relative overflow-hidden mb-2">
                    <div 
                      className="absolute bottom-0 w-full bg-gradient-to-t from-orange-500 to-orange-400 rounded-lg transition-all"
                      style={{ height: `${(count / stats.total_holders) * 100}%` }}
                    />
                  </div>
                  <p className="text-xs text-gray-400">{range}</p>
                  <p className="text-lg font-bold text-white">{count}</p>
                  <p className="text-xs text-gray-500">{((count / stats.total_holders) * 100).toFixed(1)}%</p>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Filters */}
        <div className="flex flex-wrap gap-4 mb-6">
          <input
            type="text"
            placeholder="Search address or name..."
            value={filter.search}
            onChange={(e) => setFilter({ ...filter, search: e.target.value })}
            className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white flex-1 min-w-[200px]"
          />
          <input
            type="number"
            placeholder="Min balance (TGR)..."
            value={filter.minBalance}
            onChange={(e) => setFilter({ ...filter, minBalance: e.target.value })}
            className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white w-40"
          />
          <input
            type="number"
            placeholder="Max balance (TGR)..."
            value={filter.maxBalance}
            onChange={(e) => setFilter({ ...filter, maxBalance: e.target.value })}
            className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white w-40"
          />
          <select
            value={filter.sortBy}
            onChange={(e) => setFilter({ ...filter, sortBy: e.target.value as any })}
            className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white"
          >
            <option value="rank">Sort by Rank</option>
            <option value="balance">Sort by Balance</option>
            <option value="tx_count">Sort by Tx Count</option>
            <option value="last_active">Sort by Last Active</option>
          </select>
        </div>

        {/* Holders Table */}
        <div className="bg-gray-800 rounded-lg border border-gray-700 overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-gray-700">
                <tr>
                  <th className="px-4 py-3 text-left text-sm font-medium text-gray-300">Rank</th>
                  <th className="px-4 py-3 text-left text-sm font-medium text-gray-300">Address</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">Balance</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">%</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">Value (USD)</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">Tx Count</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">Last Active</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-700">
                {paginatedHolders.length === 0 ? (
                  <tr>
                    <td colSpan={7} className="px-4 py-12 text-center text-gray-500">
                      No data available
                    </td>
                  </tr>
                ) : (
                paginatedHolders.map((holder) => (
                  <tr key={holder.address} className="hover:bg-gray-750">
                    <td className="px-4 py-3">
                      <span className={`font-bold ${getRankColor(holder.rank)}`}>
                        {holder.rank <= 3 && '🥇'} #{holder.rank}
                      </span>
                    </td>
                    <td className="px-4 py-3">
                      <div>
                        <a href={`/address/${holder.address}`} className="text-orange-400 hover:underline font-mono">
                          {formatAddress(holder.address)}
                        </a>
                        {holder.name && (
                          <span className="ml-2 text-gray-500 text-sm">({holder.name})</span>
                        )}
                      </div>
                    </td>
                    <td className="px-4 py-3 text-right font-medium text-white">
                      {formatBalance(holder.balance)} TGR
                    </td>
                    <td className="px-4 py-3 text-right text-gray-400">
                      {holder.percentage.toFixed(2)}%
                    </td>
                    <td className="px-4 py-3 text-right text-green-400 font-medium">
                      ${holder.balance_usd.toLocaleString(undefined, { maximumFractionDigits: 0 })}
                    </td>
                    <td className="px-4 py-3 text-right text-gray-400">
                      {holder.tx_count.toLocaleString()}
                    </td>
                    <td className="px-4 py-3 text-right text-gray-500">
                      {formatTime(holder.last_active)}
                    </td>
                  </tr>
                ))
                )}
              </tbody>
            </table>
          </div>
          
          {/* Pagination */}
          <div className="px-4 py-3 bg-gray-700 border-t border-gray-600 flex items-center justify-between">
            <button
              onClick={() => setPage(Math.max(1, page - 1))}
              disabled={page === 1}
              className="px-4 py-2 bg-gray-800 border border-gray-600 rounded-lg text-white disabled:opacity-50"
            >
              Previous
            </button>
            <span className="text-gray-400">
              Page {page} of {totalPages} ({filteredHolders.length} holders)
            </span>
            <button
              onClick={() => setPage(Math.min(totalPages, page + 1))}
              disabled={page === totalPages}
              className="px-4 py-2 bg-gray-800 border border-gray-600 rounded-lg text-white disabled:opacity-50"
            >
              Next
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}