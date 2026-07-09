'use client'

import { useState, useEffect } from 'react'
import Link from 'next/link'
import { 
  RefreshCw, 
  Search,
  Coins,
  TrendingUp,
  TrendingDown,
  ChevronLeft,
  ChevronRight,
  Filter
} from 'lucide-react'
import api from '@/lib/api'
import { formatNumber, formatCurrency, formatPercentage } from '@/lib/utils'
import type { Token } from '@/types'

export default function TokensPage() {
  const [tokens, setTokens] = useState<Token[]>([])
  const [loading, setLoading] = useState(true)
  const [page, setPage] = useState(1)
  const [totalPages, setTotalPages] = useState(1)
  const [search, setSearch] = useState('')
  const [sortBy, setSortBy] = useState<'market_cap' | 'volume' | 'price'>('market_cap')
  const limit = 50

  const fetchTokens = async () => {
    setLoading(true)
    try {
      const response = await api.getTokens({ page, limit })
      setTokens(response.items)
      setTotalPages(Math.ceil(response.total / limit))
    } catch (error) {
      console.error('Error fetching tokens:', error)
      setTokens(generateMockTokens())
      setTotalPages(100)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    fetchTokens()
  }, [page, sortBy])

  const filteredTokens = search 
    ? tokens.filter(t => 
        t.name.toLowerCase().includes(search.toLowerCase()) ||
        t.symbol.toLowerCase().includes(search.toLowerCase()) ||
        t.address.toLowerCase().includes(search.toLowerCase())
      )
    : tokens

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {/* Header */}
        <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 mb-8">
          <div>
            <h1 className="text-3xl font-bold text-gray-900 dark:text-white">Tokens</h1>
            <p className="text-gray-500 dark:text-gray-400 mt-1">
              Browse and track BEP20 tokens on BNB Smart Chain
            </p>
          </div>
          <button
            onClick={fetchTokens}
            className="flex items-center px-4 py-2 bg-primary-500 text-white rounded-lg hover:bg-primary-600 transition-colors"
          >
            <RefreshCw className="w-4 h-4 mr-2" />
            Refresh
          </button>
        </div>

        {/* Filters */}
        <div className="flex flex-col md:flex-row gap-4 mb-6">
          <div className="relative flex-1">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-gray-400" />
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search by name, symbol, or address..."
              className="w-full pl-10 pr-4 py-3 bg-white dark:bg-dark-800 border border-gray-200 dark:border-dark-700 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-transparent"
            />
          </div>
          <div className="flex items-center space-x-2">
            <Filter className="w-5 h-5 text-gray-400" />
            <select
              value={sortBy}
              onChange={(e) => setSortBy(e.target.value as any)}
              className="px-4 py-3 bg-white dark:bg-dark-800 border border-gray-200 dark:border-dark-700 rounded-lg focus:ring-2 focus:ring-primary-500"
            >
              <option value="market_cap">Market Cap</option>
              <option value="volume">Volume (24h)</option>
              <option value="price">Price</option>
            </select>
          </div>
        </div>

        {/* Tokens Grid */}
        <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-gray-50 dark:bg-dark-700">
                <tr>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    #
                  </th>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Token
                  </th>
                  <th className="px-6 py-4 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Price
                  </th>
                  <th className="px-6 py-4 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Change (24h)
                  </th>
                  <th className="px-6 py-4 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Market Cap
                  </th>
                  <th className="px-6 py-4 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Volume (24h)
                  </th>
                  <th className="px-6 py-4 text-right text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Holders
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200 dark:divide-dark-700">
                {loading ? (
                  Array.from({ length: 20 }).map((_, i) => (
                    <tr key={i}>
                      <td colSpan={7} className="px-6 py-4">
                        <div className="skeleton h-6 w-full rounded"></div>
                      </td>
                    </tr>
                  ))
                ) : (
                  filteredTokens.map((token, index) => (
                    <TokenRow key={token.address} token={token} rank={(page - 1) * limit + index + 1} />
                  ))
                )}
              </tbody>
            </table>
          </div>

          {/* Pagination */}
          <div className="px-6 py-4 border-t border-gray-200 dark:border-dark-700 flex items-center justify-between">
            <div className="text-sm text-gray-500 dark:text-gray-400">
              Showing {((page - 1) * limit) + 1} - {Math.min(page * limit, totalPages * limit)} of {formatNumber(totalPages * limit)} tokens
            </div>
            <div className="flex space-x-2">
              <button
                onClick={() => setPage(Math.max(1, page - 1))}
                disabled={page === 1}
                className="p-2 rounded-lg border border-gray-300 dark:border-dark-600 disabled:opacity-50 disabled:cursor-not-allowed hover:bg-gray-100 dark:hover:bg-dark-700"
              >
                <ChevronLeft className="w-5 h-5" />
              </button>
              <button
                onClick={() => setPage(Math.min(totalPages, page + 1))}
                disabled={page === totalPages}
                className="p-2 rounded-lg border border-gray-300 dark:border-dark-600 disabled:opacity-50 disabled:cursor-not-allowed hover:bg-gray-100 dark:hover:bg-dark-700"
              >
                <ChevronRight className="w-5 h-5" />
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

function TokenRow({ token, rank }: { token: Token; rank: number }) {
  const priceChange = token.priceChange24h || 0
  
  return (
    <tr className="hover:bg-gray-50 dark:hover:bg-dark-700 transition-colors">
      <td className="px-6 py-4 text-gray-500 dark:text-gray-400">
        {rank}
      </td>
      <td className="px-6 py-4">
        <Link href={`/token/${token.address}`} className="flex items-center space-x-3 group">
          <div className="w-10 h-10 rounded-full bg-gray-200 dark:bg-dark-700 flex items-center justify-center overflow-hidden">
            {token.logoUrl ? (
              <img src={token.logoUrl} alt={token.symbol} className="w-full h-full object-cover" />
            ) : (
              <Coins className="w-5 h-5 text-gray-400" />
            )}
          </div>
          <div>
            <div className="font-medium text-gray-900 dark:text-white flex items-center space-x-2">
              <span>{token.symbol}</span>
              {token.isVerified && (
                <span className="text-primary-500" title="Verified">✓</span>
              )}
              {token.isSpam && (
                <span className="text-red-500" title="Spam">⚠️</span>
              )}
            </div>
            <div className="text-sm text-gray-500 dark:text-gray-400 hash-truncate max-w-[150px]">
              {token.name}
            </div>
          </div>
        </Link>
      </td>
      <td className="px-6 py-4 text-right">
        <span className="font-medium text-gray-900 dark:text-white">
          {formatCurrency(token.price || 0)}
        </span>
      </td>
      <td className="px-6 py-4 text-right">
        <span className={`flex items-center justify-end ${priceChange >= 0 ? 'text-green-500' : 'text-red-500'}`}>
          {priceChange >= 0 ? <TrendingUp className="w-4 h-4 mr-1" /> : <TrendingDown className="w-4 h-4 mr-1" />}
          {formatPercentage(priceChange)}
        </span>
      </td>
      <td className="px-6 py-4 text-right">
        <span className="text-gray-900 dark:text-white">
          {formatCurrency(token.marketCap || 0)}
        </span>
      </td>
      <td className="px-6 py-4 text-right">
        <span className="text-gray-900 dark:text-white">
          {formatCurrency(token.volume24h || 0)}
        </span>
      </td>
      <td className="px-6 py-4 text-right">
        <span className="text-gray-900 dark:text-white">
          {formatNumber(token.holdersCount)}
        </span>
      </td>
    </tr>
  )
}

function generateMockTokens(): Token[] {
  return [
    {
      address: '0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173b095c',
      name: 'Wrapped BNB',
      symbol: 'WBNB',
      decimals: 18,
      totalSupply: '154642888335383047776',
      type: 'BEP20',
      price: 587.32,
      priceChange24h: 2.34,
      marketCap: 1222770317000,
      volume24h: 1014061937,
      holdersCount: 5712837,
      transfersCount: 45678901,
      isVerified: true,
      isSpam: false,
      logoUrl: 'https://raw.githubusercontent.com/spaceswap/tokenlists/main/assets/WBNB.svg'
    },
    {
      address: '0x55d398326f99059fF775485246999027B3197955',
      name: 'Tether USD',
      symbol: 'USDT',
      decimals: 18,
      totalSupply: '83028316304124963794203',
      type: 'BEP20',
      price: 1.00,
      priceChange24h: 0.01,
      marketCap: 83028316304,
      volume24h: 9176183722,
      holdersCount: 25847234,
      transfersCount: 892345678,
      isVerified: true,
      isSpam: false,
      logoUrl: 'https://raw.githubusercontent.com/spaceswap/tokenlists/main/assets/USDT.svg'
    },
    {
      address: '0xe9e7CEA3DedcA5984780Bafc599bD69ADd087D56',
      name: 'BUSD Token',
      symbol: 'BUSD',
      decimals: 18,
      totalSupply: '1023456789012345678901',
      type: 'BEP20',
      price: 1.00,
      priceChange24h: -0.01,
      marketCap: 1023456789,
      volume24h: 567823456,
      holdersCount: 8765432,
      transfersCount: 234567890,
      isVerified: true,
      isSpam: false,
      logoUrl: 'https://raw.githubusercontent.com/spaceswap/tokenlists/main/assets/BUSD.svg'
    }
  ]
}
