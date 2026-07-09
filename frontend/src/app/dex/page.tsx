'use client'

import { useState, useEffect } from 'react'
import Link from 'next/link'
import { TrendingUp, Coins, Flame, ChevronLeft, ChevronRight, RefreshCw } from 'lucide-react'
import api from '@/lib/api'
import { formatNumber, formatCurrency } from '@/lib/utils'

interface DexPair {
  address: string
  token0_symbol: string
  token1_symbol: string
  liquidity: number
  volume_24h: number
  volume_7d: number
}

export default function DexPage() {
  const [pairs, setPairs] = useState<DexPair[]>([])
  const [loading, setLoading] = useState(true)
  const [page, setPage] = useState(1)
  const [totalPages, setTotalPages] = useState(1)
  const limit = 25

  useEffect(() => {
    fetchPairs()
  }, [page])

  const fetchPairs = async () => {
    setLoading(true)
    try {
      const response = await api.getDexPairs({ page, limit })
      setPairs(response.items || [])
      setTotalPages(Math.ceil(response.total / limit))
    } catch (error) {
      // Mock data
      setPairs([
        { address: '0x58f876857a02d676309589037300a5d53ba1cbcb', token0_symbol: 'WBNB', token1_symbol: 'USDT', liquidity: 50000000, volume_24h: 10000000, volume_7d: 70000000 },
        { address: '0x16b9a82891338fd9e241f1fa9afd6a7c4a4de52', token0_symbol: 'WBNB', token1_symbol: 'BUSD', liquidity: 30000000, volume_24h: 5000000, volume_7d: 35000000 },
        { address: '0xa6cc3c2531fdaa6ae1c3d403ab9e7bfe7cf5e78', token0_symbol: 'CAKE', token1_symbol: 'WBNB', liquidity: 20000000, volume_24h: 3000000, volume_7d: 21000000 },
      ])
      setTotalPages(10)
    } finally {
      setLoading(false)
    }
  }

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

        {/* Stats */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <TrendingUp className="w-4 h-4" />
              <span className="text-sm">Total Pairs</span>
            </div>
            <p className="text-2xl font-bold">{formatNumber(1234)}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <Coins className="w-4 h-4" />
              <span className="text-sm">Total Liquidity</span>
            </div>
            <p className="text-2xl font-bold">{formatCurrency(500000000)}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <Flame className="w-4 h-4" />
              <span className="text-sm">Volume (24h)</span>
            </div>
            <p className="text-2xl font-bold">{formatCurrency(100000000)}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <TrendingUp className="w-4 h-4" />
              <span className="text-sm">Top Pair</span>
            </div>
            <p className="text-2xl font-bold">WBNB/USDT</p>
          </div>
        </div>

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
                ) : (
                  pairs.map((pair) => (
                    <tr key={pair.address} className="hover:bg-gray-50 dark:hover:bg-dark-700">
                      <td className="px-6 py-4">
                        <Link href={`/dex/${pair.address}`} className="flex items-center space-x-2">
                          <Coins className="w-5 h-5 text-primary-500" />
                          <span className="font-medium text-gray-900 dark:text-white">
                            {pair.token0_symbol}/{pair.token1_symbol}
                          </span>
                        </Link>
                      </td>
                      <td className="px-6 py-4 text-right text-gray-900 dark:text-white">
                        {formatCurrency(pair.liquidity)}
                      </td>
                      <td className="px-6 py-4 text-right text-gray-900 dark:text-white">
                        {formatCurrency(pair.volume_24h)}
                      </td>
                      <td className="px-6 py-4 text-right text-gray-900 dark:text-white">
                        {formatCurrency(pair.volume_7d)}
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
