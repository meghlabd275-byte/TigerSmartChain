'use client'

import { useState, useEffect } from 'react'
import Link from 'next/link'
import { TrendingUp, Coins, Flame, ChevronLeft, ChevronRight, RefreshCw } from 'lucide-react'
import api from '@/lib/api'
import { formatNumber, formatCurrency } from '@/lib/utils'

interface DexPair {
  address: string
  token0Symbol: string
  token1Symbol: string
  liquidity: number
  volume24h: number
  volume7d: number
}

export default function DexPage() {
  const [pairs, setPairs] = useState<DexPair[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [page, setPage] = useState(1)
  const [totalPages, setTotalPages] = useState(1)
  const [total, setTotal] = useState(0)
  const [stats, setStats] = useState<{
    totalPairs: number
    totalLiquidity: string
    totalVolume: string
  } | null>(null)
  const limit = 25

  useEffect(() => {
    fetchPairs()
  }, [page])

  const fetchPairs = async () => {
    setLoading(true)
    setError(null)
    try {
      const [response, dexStats] = await Promise.all([
        api.getDexPairs({ page, limit }),
        api.getDexStats(),
      ])
      setPairs((response.items || []) as unknown as DexPair[])
      setTotal(response.total || 0)
      setTotalPages(Math.max(1, Math.ceil(response.total / limit)))
      setStats(dexStats)
    } catch (err) {
      // Show the failure rather than substituting invented pairs, which would
      // present fabricated liquidity and volume as if they were on chain.
      setPairs([])
      setTotal(0)
      setTotalPages(1)
      setStats(null)
      setError(err instanceof Error ? err.message : 'Failed to load DEX pairs')
    } finally {
      setLoading(false)
    }
  }

  const topPair = pairs.reduce<DexPair | null>(
    (best, p) => (!best || p.volume24h > best.volume24h ? p : best),
    null,
  )

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="flex items-center justify-between mb-8">
          <div>
            <h1 className="text-3xl font-bold text-gray-900 dark:text-white">DEX Pairs</h1>
            <p className="text-gray-500 dark:text-gray-400 mt-1">
              Track liquidity and volume across DEX pairs
            </p>
          </div>
          <button onClick={fetchPairs} className="flex items-center px-4 py-2 bg-primary-500 text-white rounded-lg hover:bg-primary-600">
            <RefreshCw className="w-4 h-4 mr-2" />
            Refresh
          </button>
        </div>

        {/* Stats — protocol totals summed in SQL, not page-level sums. */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <TrendingUp className="w-4 h-4" />
              <span className="text-sm">Total Pairs</span>
            </div>
            <p className="text-2xl font-bold">{formatNumber(stats?.totalPairs ?? total)}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <Coins className="w-4 h-4" />
              <span className="text-sm">Total Liquidity</span>
            </div>
            <p className="text-2xl font-bold">
              {stats ? formatCurrency(Number(stats.totalLiquidity)) : '—'}
            </p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <Flame className="w-4 h-4" />
              <span className="text-sm">Volume 24h</span>
            </div>
            <p className="text-2xl font-bold">
              {stats ? formatCurrency(Number(stats.totalVolume)) : '—'}
            </p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <TrendingUp className="w-4 h-4" />
              <span className="text-sm">Top Pair</span>
            </div>
            <p className="text-2xl font-bold truncate">
              {topPair ? `${topPair.token0Symbol}/${topPair.token1Symbol}` : '—'}
            </p>
          </div>
        </div>

        {error && (
          <div className="mb-6 rounded-lg border border-red-300 bg-red-50 dark:border-red-800 dark:bg-red-950 px-4 py-3 text-sm text-red-700 dark:text-red-300">
            {error}
          </div>
        )}

        {/* Pairs Table */}
        <div className="bg-white dark:bg-dark-800 rounded-xl border overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-gray-50 dark:bg-dark-700">
                <tr>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 uppercase">Pair</th>
                  <th className="px-6 py-4 text-right text-xs font-semibold text-gray-500 uppercase">Liquidity</th>
                  <th className="px-6 py-4 text-right text-xs font-semibold text-gray-500 uppercase">Volume (24h)</th>
                  <th className="px-6 py-4 text-right text-xs font-semibold text-gray-500 uppercase">Volume (7d)</th>
                  <th className="px-6 py-4 text-right text-xs font-semibold text-gray-500 uppercase">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200 dark:divide-dark-700">
                {loading ? (
                  Array.from({ length: 10 }).map((_, i) => (
                    <tr key={i}>
                      <td colSpan={5} className="px-6 py-4"><div className="skeleton h-6 w-full rounded"></div></td>
                    </tr>
                  ))
                ) : pairs.length === 0 ? (
                  <tr>
                    <td colSpan={5} className="px-6 py-12 text-center text-gray-500 dark:text-gray-400">
                      No DEX pairs have been indexed yet.
                    </td>
                  </tr>
                ) : (
                  pairs.map((pair) => (
                    <tr key={pair.address} className="hover:bg-gray-50 dark:hover:bg-dark-700">
                      <td className="px-6 py-4">
                        <Link href={`/dex/${pair.address}`} className="flex items-center space-x-2">
                          <Coins className="w-5 h-5 text-primary-500" />
                          <span className="font-medium text-gray-900 dark:text-white">
                            {pair.token0Symbol}/{pair.token1Symbol}
                          </span>
                        </Link>
                      </td>
                      <td className="px-6 py-4 text-right text-gray-900 dark:text-white">
                        {formatCurrency(pair.liquidity)}
                      </td>
                      <td className="px-6 py-4 text-right text-gray-900 dark:text-white">
                        {formatCurrency(pair.volume24h)}
                      </td>
                      <td className="px-6 py-4 text-right text-gray-900 dark:text-white">
                        {formatCurrency(pair.volume7d)}
                      </td>
                      <td className="px-6 py-4 text-right">
                        <Link href={`/dex/${pair.address}`} className="text-primary-500 hover:underline">
                          View
                        </Link>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>

          <div className="px-6 py-4 border-t flex items-center justify-between">
            <div className="text-sm text-gray-500">Page {page} of {totalPages}</div>
            <div className="flex space-x-2">
              <button onClick={() => setPage(Math.max(1, page - 1))} disabled={page === 1} className="p-2 rounded-lg border disabled:opacity-50">
                <ChevronLeft className="w-5 h-5" />
              </button>
              <button onClick={() => setPage(Math.min(totalPages, page + 1))} disabled={page === totalPages} className="p-2 rounded-lg border disabled:opacity-50">
                <ChevronRight className="w-5 h-5" />
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
