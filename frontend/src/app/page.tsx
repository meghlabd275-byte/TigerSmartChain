'use client'

import { useState, useEffect } from 'react'
import Link from 'next/link'
import { 
  ArrowUpRight, 
  ArrowDownRight, 
  Box, 
  FileText, 
  Coins, 
  Image,
  Activity,
  Zap,
  TrendingUp,
  Users,
  Clock,
  ArrowRight,
  RefreshCw
} from 'lucide-react'
import { 
  AreaChart, 
  Area, 
  XAxis, 
  YAxis, 
  CartesianGrid, 
  Tooltip, 
  ResponsiveContainer 
} from 'recharts'
import api from '@/lib/api'
import { formatNumber, formatCurrency, formatPercentage, formatTimeAgo, formatAddress } from '@/lib/utils'
import type { Block, Transaction, Token, NetworkStats, GasOracle, ChartData } from '@/types'

export default function HomePage() {
  const [latestBlock, setLatestBlock] = useState<Block | null>(null)
  const [recentTxs, setRecentTxs] = useState<Transaction[]>([])
  const [topTokens, setTopTokens] = useState<Token[]>([])
  const [networkStats, setNetworkStats] = useState<NetworkStats | null>(null)
  const [gasOracle, setGasOracle] = useState<GasOracle | null>(null)
  const [txChartData, setTxChartData] = useState<ChartData[]>([])
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const fetchData = async () => {
    try {
      const [block, txs, tokens, stats, gas, chart] = await Promise.all([
        api.getLatestBlock(),
        api.getTransactions({ limit: 10 }),
        api.getTokens({ limit: 10 }),
        api.getNetworkStats(),
        api.getGasOracle(),
        api.getTransactionHistory({ timeframe: '24h' })
      ])

      setLatestBlock(block)
      setRecentTxs(txs.items)
      setTopTokens(tokens.items)
      setNetworkStats(stats)
      setGasOracle(gas)
      setTxChartData(chart)
      setError(null)
    } catch (error) {
      console.error('Error fetching data:', error)
      setError('Failed to load data. Please try again later.')
    } finally {
      setLoading(false)
      setRefreshing(false)
    }
  }

  useEffect(() => {
    fetchData()
    const interval = setInterval(fetchData, 15000)
    return () => clearInterval(interval)
  }, [])

  const handleRefresh = () => {
    setRefreshing(true)
    fetchData()
  }

  if (loading) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-dark-900 flex items-center justify-center">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-primary-500"></div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-dark-900 flex flex-col items-center justify-center gap-4">
        <p className="text-red-500">{error}</p>
        <button onClick={handleRefresh} className="px-4 py-2 rounded-lg bg-primary-500 text-white hover:bg-primary-600">Retry</button>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      {/* Hero Section */}
      <div className="bg-gradient-to-br from-primary-600 via-primary-700 to-primary-900 text-white">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-16">
          <div className="text-center mb-12">
            <h1 className="text-4xl md:text-5xl font-bold mb-4">
              BNB Smart Chain Explorer
            </h1>
            <p className="text-xl text-primary-100 max-w-2xl mx-auto">
              Real-time blockchain data, analytics, and insights for the BNB ecosystem
            </p>
          </div>

          {/* Stats Grid */}
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
            <StatCard 
              icon={<Box className="w-5 h-5" />}
              label="Block Height"
              value={formatNumber(networkStats?.totalBlocks || 0)}
              subValue={`#${formatNumber(latestBlock?.number || 0)}`}
            />
            <StatCard 
              icon={<FileText className="w-5 h-5" />}
              label="Transactions"
              value={formatNumber(networkStats?.totalTransactions || 0)}
              subValue={`${formatNumber(networkStats?.tps || 0)} TPS`}
            />
            <StatCard 
              icon={<Users className="w-5 h-5" />}
              label="Addresses"
              value={formatNumber(networkStats?.totalAddresses || 0)}
            />
            <StatCard 
              icon={<Coins className="w-5 h-5" />}
              label="Tokens"
              value={formatNumber(networkStats?.totalTokens || 0)}
              subValue={`${formatNumber(networkStats?.totalContracts || 0)} Contracts`}
            />
          </div>

          {/* Gas Prices */}
          {gasOracle && (
            <div className="glass rounded-xl p-4">
              <div className="flex items-center justify-between flex-wrap gap-4">
                <div className="flex items-center space-x-2">
                  <Zap className="w-5 h-5 text-bnb" />
                  <span className="font-semibold">Gas Price</span>
                </div>
                <div className="flex space-x-6">
                  <GasItem label="Slow" value={gasOracle.slow} color="text-green-400" />
                  <GasItem label="Standard" value={gasOracle.standard} color="text-yellow-400" />
                  <GasItem label="Fast" value={gasOracle.fast} color="text-red-400" />
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Main Content */}
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
          {/* Recent Transactions */}
          <div className="lg:col-span-2">
            <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 overflow-hidden">
              <div className="flex items-center justify-between p-4 border-b border-gray-200 dark:border-dark-700">
                <h2 className="text-lg font-semibold text-gray-900 dark:text-white flex items-center">
                  <FileText className="w-5 h-5 mr-2 text-primary-500" />
                  Recent Transactions
                </h2>
                <button 
                  onClick={handleRefresh}
                  className="p-2 rounded-lg hover:bg-gray-100 dark:hover:bg-dark-700 transition-colors"
                >
                  <RefreshCw className={`w-4 h-4 text-gray-500 ${refreshing ? 'animate-spin' : ''}`} />
                </button>
              </div>
              <div className="divide-y divide-gray-200 dark:divide-dark-700">
                {recentTxs.map((tx) => (
                  <TransactionRow key={tx.hash} tx={tx} />
                ))}
              </div>
              <div className="p-4 border-t border-gray-200 dark:border-dark-700">
                <Link href="/txs" className="flex items-center justify-center text-primary-500 hover:text-primary-600 font-medium">
                  View All Transactions <ArrowRight className="w-4 h-4 ml-2" />
                </Link>
              </div>
            </div>
          </div>

          {/* Top Tokens */}
          <div className="lg:col-span-1">
            <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 overflow-hidden">
              <div className="flex items-center justify-between p-4 border-b border-gray-200 dark:border-dark-700">
                <h2 className="text-lg font-semibold text-gray-900 dark:text-white flex items-center">
                  <Coins className="w-5 h-5 mr-2 text-primary-500" />
                  Top Tokens
                </h2>
                <Link href="/tokens" className="text-sm text-primary-500 hover:text-primary-600">
                  View All
                </Link>
              </div>
              <div className="divide-y divide-gray-200 dark:divide-dark-700">
                {topTokens.map((token) => (
                  <TokenRow key={token.address} token={token} />
                ))}
              </div>
            </div>
          </div>
        </div>

        {/* Chart Section */}
        <div className="mt-8 bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 p-6">
          <div className="flex items-center justify-between mb-6">
            <h2 className="text-lg font-semibold text-gray-900 dark:text-white flex items-center">
              <Activity className="w-5 h-5 mr-2 text-primary-500" />
              Transaction History (24h)
            </h2>
            <div className="flex space-x-2">
              {['24h', '7d', '30d', '1y'].map((tf) => (
                <button
                  key={tf}
                  className="px-3 py-1 text-sm rounded-lg bg-gray-100 dark:bg-dark-700 text-gray-600 dark:text-gray-300 hover:bg-primary-100 dark:hover:bg-primary-900/30"
                >
                  {tf}
                </button>
              ))}
            </div>
          </div>
          <div className="h-64 flex items-center justify-center">
            {txChartData.length === 0 ? (
              <span className="text-gray-500 dark:text-gray-400">No chart data available</span>
            ) : (
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={txChartData}>
                <defs>
                  <linearGradient id="colorTx" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="#14b8a6" stopOpacity={0.3} />
                    <stop offset="95%" stopColor="#14b8a6" stopOpacity={0} />
                  </linearGradient>
                </defs>
                <CartesianGrid strokeDasharray="3 3" stroke="#374151" opacity={0.2} />
                <XAxis dataKey="timestamp" stroke="#6b7280" fontSize={12} tickFormatter={(v) => new Date(v * 1000).toLocaleTimeString([], {hour: '2-digit', minute:'2-digit'})} />
                <YAxis stroke="#6b7280" fontSize={12} tickFormatter={(v) => formatNumber(v)} />
                <Tooltip 
                  contentStyle={{ backgroundColor: '#1f2937', border: 'none', borderRadius: '8px' }}
                  labelFormatter={(v) => new Date(v * 1000).toLocaleString()}
                  formatter={(v: number) => [formatNumber(v), 'Transactions']}
                />
                <Area type="monotone" dataKey="value" stroke="#14b8a6" strokeWidth={2} fillOpacity={1} fill="url(#colorTx)" />
              </AreaChart>
            </ResponsiveContainer>
            )}
          </div>
        </div>

        {/* Quick Links */}
        <div className="mt-8 grid grid-cols-2 md:grid-cols-4 gap-4">
          <QuickLinkCard 
            icon={<Box className="w-6 h-6" />}
            title="Blocks"
            description="View latest blocks"
            href="/blocks"
          />
          <QuickLinkCard 
            icon={<Image className="w-6 h-6" />}
            title="NFTs"
            description="Explore NFT collections"
            href="/nfts"
          />
          <QuickLinkCard 
            icon={<TrendingUp className="w-6 h-6" />}
            title="Analytics"
            description="Network statistics"
            href="/charts"
          />
          <QuickLinkCard 
            icon={<Activity className="w-6 h-6" />}
            title="API"
            description="Developer documentation"
            href="/docs/api"
          />
        </div>
      </div>
    </div>
  )
}

function StatCard({ icon, label, value, subValue }: { icon: React.ReactNode; label: string; value: string; subValue?: string }) {
  return (
    <div className="bg-white/10 backdrop-blur-sm rounded-xl p-4">
      <div className="flex items-center justify-between mb-2">
        <span className="text-primary-200">{icon}</span>
        {subValue && <span className="text-xs text-primary-300">{subValue}</span>}
      </div>
      <p className="text-2xl font-bold">{value}</p>
      <p className="text-sm text-primary-200">{label}</p>
    </div>
  )
}

function GasItem({ label, value, color }: { label: string; value: string; color: string }) {
  return (
    <div className="text-center">
      <p className={`text-lg font-bold ${color}`}>{value} Gwei</p>
      <p className="text-xs text-gray-300">{label}</p>
    </div>
  )
}

function TransactionRow({ tx }: { tx: Transaction }) {
  const isOutgoing = tx.from.toLowerCase() === '0x742d35Cc6634C0532925a3b844Bc9e7595f0eB1E'.toLowerCase()
  
  return (
    <Link href={`/tx/${tx.hash}`} className="flex items-center justify-between p-4 hover:bg-gray-50 dark:hover:bg-dark-700 transition-colors">
      <div className="flex items-center space-x-3">
        <div className={`w-10 h-10 rounded-full flex items-center justify-center ${isOutgoing ? 'bg-red-100 text-red-500' : 'bg-green-100 text-green-500'}`}>
          {isOutgoing ? <ArrowUpRight className="w-5 h-5" /> : <ArrowDownRight className="w-5 h-5" />}
        </div>
        <div>
          <p className="font-medium text-gray-900 dark:text-white hash-truncate max-w-xs">
            {formatAddress(tx.hash, 10, 8)}
          </p>
          <p className="text-sm text-gray-500">
            {formatTimeAgo(tx.timestamp)}
          </p>
        </div>
      </div>
      <div className="text-right">
        <p className="font-medium text-gray-900 dark:text-white">
          {formatCurrency(parseFloat(tx.value) / 1e18)}
        </p>
        <p className={`text-sm ${tx.status === 'success' ? 'text-green-500' : 'text-red-500'}`}>
          {tx.status}
        </p>
      </div>
    </Link>
  )
}

function TokenRow({ token }: { token: Token }) {
  return (
    <Link href={`/token/${token.address}`} className="flex items-center justify-between p-4 hover:bg-gray-50 dark:hover:bg-dark-700 transition-colors">
      <div className="flex items-center space-x-3">
        <div className="w-10 h-10 rounded-full bg-gray-200 dark:bg-dark-700 flex items-center justify-center overflow-hidden">
          {token.logoUrl ? (
            <img src={token.logoUrl} alt={token.symbol} className="w-full h-full object-cover" />
          ) : (
            <Coins className="w-5 h-5 text-gray-400" />
          )}
        </div>
        <div>
          <p className="font-medium text-gray-900 dark:text-white">{token.symbol}</p>
          <p className="text-sm text-gray-500">{token.name}</p>
        </div>
      </div>
      <div className="text-right">
        <p className="font-medium text-gray-900 dark:text-white">
          {formatCurrency(token.price || 0)}
        </p>
        <p className={`text-sm ${(token.priceChange24h || 0) >= 0 ? 'text-green-500' : 'text-red-500'}`}>
          {formatPercentage(token.priceChange24h || 0)}
        </p>
      </div>
    </Link>
  )
}

function QuickLinkCard({ icon, title, description, href }: { icon: React.ReactNode; title: string; description: string; href: string }) {
  return (
    <Link href={href} className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 p-6 hover:border-primary-500 transition-colors group">
      <div className="text-primary-500 mb-4 group-hover:scale-110 transition-transform">{icon}</div>
      <h3 className="font-semibold text-gray-900 dark:text-white mb-1">{title}</h3>
      <p className="text-sm text-gray-500">{description}</p>
    </Link>
  )
}
