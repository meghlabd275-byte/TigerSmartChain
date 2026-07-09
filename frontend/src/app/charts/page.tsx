'use client'

import { useState, useEffect } from 'react'
import { 
  AreaChart, 
  Area, 
  XAxis, 
  YAxis, 
  CartesianGrid, 
  Tooltip, 
  ResponsiveContainer,
  BarChart,
  Bar,
  PieChart,
  Pie,
  Cell
} from 'recharts'
import { 
  Activity,
  Users,
  FileText,
  Coins,
  TrendingUp,
  Database,
  Flame
} from 'lucide-react'
import api from '@/lib/api'
import { formatNumber, formatCurrency } from '@/lib/utils'
import type { NetworkStats, ChartData } from '@/types'

const COLORS = ['#14b8a6', '#f59e0b', '#ef4444', '#8b5cf6', '#ec4899']

export default function ChartsPage() {
  const [networkStats, setNetworkStats] = useState<NetworkStats | null>(null)
  const [txHistory, setTxHistory] = useState<ChartData[]>([])
  const [addressHistory, setAddressHistory] = useState<ChartData[]>([])
  const [timeframe, setTimeframe] = useState<'24h' | '7d' | '30d' | '1y'>('7d')
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    const fetchData = async () => {
      setLoading(true)
      try {
        const [stats, txData, addrData] = await Promise.all([
          api.getNetworkStats(),
          api.getTransactionHistory({ timeframe }),
          api.getAddressHistory({ timeframe })
        ])
        setNetworkStats(stats)
        setTxHistory(txData)
        setAddressHistory(addrData)
      } catch (error) {
        console.error('Error fetching chart data:', error)
        setNetworkStats({
          totalBlocks: 45678901,
          totalTransactions: 2345678901,
          totalAddresses: 123456789,
          totalContracts: 5678901,
          totalTokens: 23456,
          avgBlockTime: 3.2,
          avgGasPrice: '5',
          tps: 125
        })
        setTxHistory(generateMockChartData())
        setAddressHistory(generateMockAddressData())
      } finally {
        setLoading(false)
      }
    }
    fetchData()
  }, [timeframe])

  const mockDistribution = [
    { name: 'Ethereum', value: 45 },
    { name: 'Binance', value: 25 },
    { name: 'Others', value: 30 }
  ]

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {/* Header */}
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900 dark:text-white">Analytics</h1>
          <p className="text-gray-500 dark:text-gray-400 mt-1">
            Network statistics and charts for BNB Smart Chain
          </p>
        </div>

        {/* Stats Cards */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
          <StatCard 
            icon={<Database className="w-6 h-6" />}
            label="Total Blocks"
            value={formatNumber(networkStats?.totalBlocks || 0)}
            change="+12.5%"
            positive
          />
          <StatCard 
            icon={<FileText className="w-6 h-6" />}
            label="Total Transactions"
            value={formatNumber(networkStats?.totalTransactions || 0)}
            change="+8.3%"
            positive
          />
          <StatCard 
            icon={<Users className="w-6 h-6" />}
            label="Total Addresses"
            value={formatNumber(networkStats?.totalAddresses || 0)}
            change="+5.2%"
            positive
          />
          <StatCard 
            icon={<Activity className="w-6 h-6" />}
            label="TPS"
            value={networkStats?.tps.toString() || '0'}
            change="+15.7%"
            positive
          />
        </div>

        {/* Timeframe Selector */}
        <div className="flex justify-end mb-6">
          <div className="flex bg-white dark:bg-dark-800 rounded-lg p-1 border border-gray-200 dark:border-dark-700">
            {(['24h', '7d', '30d', '1y'] as const).map((tf) => (
              <button
                key={tf}
                onClick={() => setTimeframe(tf)}
                className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                  timeframe === tf
                    ? 'bg-primary-500 text-white'
                    : 'text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-dark-700'
                }`}
              >
                {tf}
              </button>
            ))}
          </div>
        </div>

        {/* Charts Grid */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
          {/* Transaction Chart */}
          <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 p-6">
            <h2 className="text-lg font-semibold text-gray-900 dark:text-white mb-6 flex items-center">
              <FileText className="w-5 h-5 mr-2 text-primary-500" />
              Transactions Over Time
            </h2>
            <div className="h-72">
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={txHistory.length > 0 ? txHistory : generateMockChartData()}>
                  <defs>
                    <linearGradient id="colorTx" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="5%" stopColor="#14b8a6" stopOpacity={0.3} />
                      <stop offset="95%" stopColor="#14b8a6" stopOpacity={0} />
                    </linearGradient>
                  </defs>
                  <CartesianGrid strokeDasharray="3 3" stroke="#374151" opacity={0.2} />
                  <XAxis dataKey="timestamp" stroke="#6b7280" fontSize={12} tickFormatter={(v) => formatTimeLabel(v)} />
                  <YAxis stroke="#6b7280" fontSize={12} tickFormatter={(v) => formatNumber(v)} />
                  <Tooltip 
                    contentStyle={{ backgroundColor: '#1f2937', border: 'none', borderRadius: '8px', color: '#fff' }}
                    labelFormatter={(v) => new Date(v * 1000).toLocaleDateString()}
                    formatter={(v: number) => [formatNumber(v), 'Transactions']}
                  />
                  <Area type="monotone" dataKey="value" stroke="#14b8a6" strokeWidth={2} fillOpacity={1} fill="url(#colorTx)" />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </div>

          {/* New Addresses Chart */}
          <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 p-6">
            <h2 className="text-lg font-semibold text-gray-900 dark:text-white mb-6 flex items-center">
              <Users className="w-5 h-5 mr-2 text-primary-500" />
              New Addresses
            </h2>
            <div className="h-72">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={addressHistory.length > 0 ? addressHistory : generateMockAddressData()}>
                  <CartesianGrid strokeDasharray="3 3" stroke="#374151" opacity={0.2} />
                  <XAxis dataKey="timestamp" stroke="#6b7280" fontSize={12} tickFormatter={(v) => formatTimeLabel(v)} />
                  <YAxis stroke="#6b7280" fontSize={12} tickFormatter={(v) => formatNumber(v)} />
                  <Tooltip 
                    contentStyle={{ backgroundColor: '#1f2937', border: 'none', borderRadius: '8px', color: '#fff' }}
                    labelFormatter={(v) => new Date(v * 1000).toLocaleDateString()}
                    formatter={(v: number) => [formatNumber(v), 'Addresses']}
                  />
                  <Bar dataKey="value" fill="#14b8a6" radius={[4, 4, 0, 0]} />
                </BarChart>
              </ResponsiveContainer>
            </div>
          </div>

          {/* Gas Price Chart */}
          <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 p-6">
            <h2 className="text-lg font-semibold text-gray-900 dark:text-white mb-6 flex items-center">
              <Flame className="w-5 h-5 mr-2 text-primary-500" />
              Gas Price History
            </h2>
            <div className="h-72">
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={generateMockGasData()}>
                  <defs>
                    <linearGradient id="colorGas" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="5%" stopColor="#f59e0b" stopOpacity={0.3} />
                      <stop offset="95%" stopColor="#f59e0b" stopOpacity={0} />
                    </linearGradient>
                  </defs>
                  <CartesianGrid strokeDasharray="3 3" stroke="#374151" opacity={0.2} />
                  <XAxis dataKey="timestamp" stroke="#6b7280" fontSize={12} tickFormatter={(v) => formatTimeLabel(v)} />
                  <YAxis stroke="#6b7280" fontSize={12} tickFormatter={(v) => `${v} Gwei`} />
                  <Tooltip 
                    contentStyle={{ backgroundColor: '#1f2937', border: 'none', borderRadius: '8px', color: '#fff' }}
                    labelFormatter={(v) => new Date(v * 1000).toLocaleDateString()}
                    formatter={(v: number) => [`${v} Gwei`, 'Gas Price']}
                  />
                  <Area type="monotone" dataKey="value" stroke="#f59e0b" strokeWidth={2} fillOpacity={1} fill="url(#colorGas)" />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </div>

          {/* Token Distribution */}
          <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 p-6">
            <h2 className="text-lg font-semibold text-gray-900 dark:text-white mb-6 flex items-center">
              <Coins className="w-5 h-5 mr-2 text-primary-500" />
              Transaction Types
            </h2>
            <div className="h-72 flex items-center justify-center">
              <ResponsiveContainer width="100%" height="100%">
                <PieChart>
                  <Pie
                    data={mockDistribution}
                    cx="50%"
                    cy="50%"
                    innerRadius={60}
                    outerRadius={100}
                    paddingAngle={5}
                    dataKey="value"
                    label={({ name, percent }) => `${name} ${(percent * 100).toFixed(0)}%`}
                  >
                    {mockDistribution.map((entry, index) => (
                      <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} />
                    ))}
                  </Pie>
                  <Tooltip 
                    contentStyle={{ backgroundColor: '#1f2937', border: 'none', borderRadius: '8px', color: '#fff' }}
                  />
                </PieChart>
              </ResponsiveContainer>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

function StatCard({ icon, label, value, change, positive }: { 
  icon: React.ReactNode
  label: string
  value: string
  change: string
  positive: boolean
}) {
  return (
    <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 p-6">
      <div className="flex items-center justify-between mb-4">
        <div className="p-2 bg-primary-100 dark:bg-primary-900/30 rounded-lg text-primary-500">
          {icon}
        </div>
        <span className={`text-sm font-medium ${positive ? 'text-green-500' : 'text-red-500'}`}>
          {change}
        </span>
      </div>
      <p className="text-2xl font-bold text-gray-900 dark:text-white">{value}</p>
      <p className="text-sm text-gray-500 dark:text-gray-400">{label}</p>
    </div>
  )
}

function formatTimeLabel(timestamp: number): string {
  const date = new Date(timestamp * 1000)
  return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
}

function generateMockChartData(): ChartData[] {
  const data = []
  const now = Math.floor(Date.now() / 1000)
  for (let i = 6; i >= 0; i--) {
    data.push({
      timestamp: now - i * 86400,
      value: Math.floor(Math.random() * 500000) + 200000
    })
  }
  return data
}

function generateMockAddressData(): ChartData[] {
  const data = []
  const now = Math.floor(Date.now() / 1000)
  for (let i = 6; i >= 0; i--) {
    data.push({
      timestamp: now - i * 86400,
      value: Math.floor(Math.random() * 50000) + 10000
    })
  }
  return data
}

function generateMockGasData(): ChartData[] {
  const data = []
  const now = Math.floor(Date.now() / 1000)
  for (let i = 23; i >= 0; i--) {
    data.push({
      timestamp: now - i * 3600,
      value: Math.floor(Math.random() * 15) + 3
    })
  }
  return data
}
