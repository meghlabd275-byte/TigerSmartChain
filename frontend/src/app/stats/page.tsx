'use client'

import { useState, useEffect } from 'react'
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  BarChart,
  Bar,
  LineChart,
  Line,
} from 'recharts'
import { Activity, Blocks, ArrowLeftRight, Wallet, Fuel } from 'lucide-react'
import api from '@/lib/api'
import { formatNumber } from '@/lib/utils'
import type { NetworkStats, ChartData } from '@/types'

const EMPTY = '—'

export default function StatsPage() {
  const [stats, setStats] = useState<NetworkStats | null>(null)
  const [txHistory, setTxHistory] = useState<ChartData[]>([])
  const [addressHistory, setAddressHistory] = useState<ChartData[]>([])
  const [gasHistory, setGasHistory] = useState<ChartData[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false

    const fetchData = async () => {
      setLoading(true)
      setError(null)
      try {
        const [networkStats, txData, addrData, gasData] = await Promise.all([
          api.getNetworkStats(),
          api.getTransactionHistory({ timeframe: '30d' }),
          api.getAddressHistory({ timeframe: '30d' }),
          api.getGasHistory({ timeframe: '24h' }),
        ])
        if (cancelled) return
        setStats(networkStats)
        setTxHistory(txData)
        setAddressHistory(addrData)
        setGasHistory(gasData)
      } catch (err) {
        if (cancelled) return
        // Showing invented totals or randomised series here would be
        // indistinguishable from live chain metrics, so report the failure.
        setStats(null)
        setTxHistory([])
        setAddressHistory([])
        setGasHistory([])
        setError(err instanceof Error ? err.message : 'Failed to load network statistics')
      } finally {
        if (!cancelled) setLoading(false)
      }
    }

    fetchData()
    return () => {
      cancelled = true
    }
  }, [])

  const series = (data: ChartData[]) =>
    data.map((point) => ({
      time: new Date(point.timestamp * 1000).toLocaleDateString(),
      value: point.value,
    }))

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 py-8">
        <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-8">Network Statistics</h1>

        {error && (
          <div className="mb-6 rounded-xl border border-red-200 bg-red-50 dark:border-red-900/40 dark:bg-red-900/20 p-4 text-sm text-red-700 dark:text-red-300">
            {error}
          </div>
        )}

        {/* Key Metrics */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <Blocks className="w-4 h-4" />
              <span className="text-sm">Latest Block</span>
            </div>
            <p className="text-2xl font-bold">
              {loading ? EMPTY : formatNumber(stats?.totalBlocks)}
            </p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <ArrowLeftRight className="w-4 h-4" />
              <span className="text-sm">Total Transactions</span>
            </div>
            <p className="text-2xl font-bold">
              {loading ? EMPTY : formatNumber(stats?.totalTransactions)}
            </p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <Wallet className="w-4 h-4" />
              <span className="text-sm">Total Addresses</span>
            </div>
            <p className="text-2xl font-bold">
              {loading ? EMPTY : formatNumber(stats?.totalAddresses)}
            </p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <Activity className="w-4 h-4" />
              <span className="text-sm">TPS</span>
            </div>
            <p className="text-2xl font-bold">
              {loading ? EMPTY : formatNumber(stats?.tps)}
            </p>
          </div>
        </div>

        {/* Secondary Metrics */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <Fuel className="w-4 h-4" />
              <span className="text-sm">Average Gas Price</span>
            </div>
            <p className="text-2xl font-bold">
              {loading ? EMPTY : stats ? `${stats.avgGasPrice} Gwei` : EMPTY}
            </p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <p className="text-sm text-gray-500 mb-2">Average Block Time</p>
            <p className="text-2xl font-bold">
              {loading ? EMPTY : stats ? `${stats.avgBlockTime.toFixed(2)}s` : EMPTY}
            </p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <p className="text-sm text-gray-500 mb-2">Verified Contracts</p>
            <p className="text-2xl font-bold">
              {loading ? EMPTY : formatNumber(stats?.totalContracts)}
            </p>
          </div>
        </div>

        {/* Charts Row 1 */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-8 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <h2 className="text-lg font-semibold mb-4">Daily Transactions (30 days)</h2>
            <div className="h-64">
              {series(txHistory).length > 0 ? (
                <ResponsiveContainer width="100%" height="100%">
                  <AreaChart data={series(txHistory)}>
                    <XAxis dataKey="time" stroke="#6b7280" fontSize={12} />
                    <YAxis stroke="#6b7280" fontSize={12} />
                    <Tooltip />
                    <Area
                      type="monotone"
                      dataKey="value"
                      stroke="#14b8a6"
                      fill="#14b8a6"
                      fillOpacity={0.3}
                    />
                  </AreaChart>
                </ResponsiveContainer>
              ) : (
                <EmptyChart loading={loading} />
              )}
            </div>
          </div>

          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <h2 className="text-lg font-semibold mb-4">Daily Active Addresses (30 days)</h2>
            <div className="h-64">
              {series(addressHistory).length > 0 ? (
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={series(addressHistory)}>
                    <XAxis dataKey="time" stroke="#6b7280" fontSize={12} />
                    <YAxis stroke="#6b7280" fontSize={12} />
                    <Tooltip />
                    <Bar dataKey="value" fill="#14b8a6" radius={[4, 4, 0, 0]} />
                  </BarChart>
                </ResponsiveContainer>
              ) : (
                <EmptyChart loading={loading} />
              )}
            </div>
          </div>
        </div>

        {/* Charts Row 2 */}
        <div className="grid grid-cols-1 gap-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <h2 className="text-lg font-semibold mb-4">Gas Price (24h)</h2>
            <div className="h-64">
              {series(gasHistory).length > 0 ? (
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart data={series(gasHistory)}>
                    <XAxis dataKey="time" stroke="#6b7280" fontSize={10} />
                    <YAxis stroke="#6b7280" fontSize={12} />
                    <Tooltip />
                    <Line
                      type="monotone"
                      dataKey="value"
                      stroke="#f59e0b"
                      strokeWidth={2}
                      dot={false}
                    />
                  </LineChart>
                </ResponsiveContainer>
              ) : (
                <EmptyChart loading={loading} />
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

function EmptyChart({ loading }: { loading: boolean }) {
  return (
    <div className="h-full flex items-center justify-center text-sm text-gray-500 dark:text-gray-400">
      {loading ? 'Loading…' : 'No data available'}
    </div>
  )
}
