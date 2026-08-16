'use client'

import { useState, useEffect } from 'react'
import { useParams } from 'next/navigation'
import Link from 'next/link'
import { Copy, Check, Coins, TrendingUp, TrendingDown, Users, ArrowRight, FileText } from 'lucide-react'
import { AreaChart, Area, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts'
import api from '@/lib/api'
import { formatNumber, formatCurrency, formatPercentage, copyToClipboard } from '@/lib/utils'

export default function TokenPage() {
  const params = useParams()
  const address = params.address as string
  
  const [data, setData] = useState<any>(null)
  const [holders, setHolders] = useState<any[]>([])
  const [transfers, setTransfers] = useState<any[]>([])
  const [priceHistory, setPriceHistory] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<'overview' | 'holders' | 'transfers' | 'price'>('overview')
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (address) fetchData()
  }, [address])

  const fetchData = async () => {
    setLoading(true)
    setError(null)
    try {
      const [tokenData, holderList, transferList, priceHist] = await Promise.all([
        api.getToken(address),
        api.getTokenHolders(address),
        api.getTokenTransfers(address),
        api.getTokenPriceHistory(address)
      ])
      setData(tokenData)
      setHolders(holderList.items || [])
      setTransfers(transferList.items || [])
      setPriceHistory(priceHist)
    } catch (error) {
      setData(null)
      setError('Failed to load data. Please try again later.')
    } finally {
      setLoading(false)
    }
  }

  const handleCopy = async () => {
    await copyToClipboard(address)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  if (loading) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-dark-900 flex items-center justify-center">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-primary-500"></div>
      </div>
    )
  }

  if (error && !data) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-dark-900 flex items-center justify-center">
        <div className="text-center">
          <p className="text-red-500 mb-4">{error}</p>
          <button onClick={fetchData} className="px-4 py-2 rounded-lg bg-primary-500 text-white hover:bg-primary-600">Retry</button>
        </div>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="mb-8">
          <div className="flex items-center space-x-4 mb-4">
            <div className="w-16 h-16 rounded-full bg-gray-200 dark:bg-dark-700 flex items-center justify-center">
              <Coins className="w-8 h-8 text-gray-400" />
            </div>
            <div>
              <h1 className="text-3xl font-bold text-gray-900 dark:text-white flex items-center space-x-2">
                <span>{data?.symbol}</span>
                {data?.isVerified && <span className="text-primary-500">✓</span>}
              </h1>
              <p className="text-gray-500">{data?.name}</p>
            </div>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 p-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center space-x-2">
                <code className="text-lg font-mono text-gray-900 dark:text-white break-all">{address}</code>
              </div>
              <button onClick={handleCopy} className="p-2 hover:bg-gray-100 dark:hover:bg-dark-700 rounded-lg">
                {copied ? <Check className="w-5 h-5 text-green-500" /> : <Copy className="w-5 h-5 text-gray-400" />}
              </button>
            </div>
          </div>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <p className="text-sm text-gray-500 mb-1">Price</p>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{formatCurrency(data?.price || 0)}</p>
            <p className={`text-sm ${(data?.priceChange24h || 0) >= 0 ? 'text-green-500' : 'text-red-500'}`}>
              {formatPercentage(data?.priceChange24h || 0)}
            </p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <p className="text-sm text-gray-500 mb-1">Market Cap</p>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{formatCurrency(data?.marketCap || 0)}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <p className="text-sm text-gray-500 mb-1">Volume (24h)</p>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{formatCurrency(data?.volume24h || 0)}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <p className="text-sm text-gray-500 mb-1">Holders</p>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{formatNumber(data?.holdersCount || 0)}</p>
          </div>
        </div>

        <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 mb-8">
          <div className="p-6 border-b border-gray-200 dark:border-dark-700">
            <h2 className="text-lg font-semibold text-gray-900 dark:text-white">Price Chart</h2>
          </div>
          <div className="h-80 p-6">
            {priceHistory.length > 0 ? (
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={priceHistory}>
                <defs>
                  <linearGradient id="colorPrice" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="#14b8a6" stopOpacity={0.3} />
                    <stop offset="95%" stopColor="#14b8a6" stopOpacity={0} />
                  </linearGradient>
                </defs>
                <XAxis dataKey="timestamp" stroke="#6b7280" fontSize={12} tickFormatter={(v) => new Date(v * 1000).toLocaleDateString()} />
                <YAxis stroke="#6b7280" fontSize={12} tickFormatter={(v) => `$${v}`} />
                <Tooltip formatter={(v: number) => [`$${v.toFixed(2)}`, 'Price']} />
                <Area type="monotone" dataKey="price" stroke="#14b8a6" strokeWidth={2} fillOpacity={1} fill="url(#colorPrice)" />
              </AreaChart>
            </ResponsiveContainer>
            ) : (
              <div className="h-full flex items-center justify-center text-gray-500">No price history data available</div>
            )}
          </div>
        </div>

        <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200">
          <div className="border-b border-gray-200 dark:border-dark-700">
            <div className="flex space-x-8 px-6">
              {(['overview', 'holders', 'transfers', 'price'] as const).map((tab) => (
                <button key={tab} onClick={() => setActiveTab(tab)} className={`py-4 border-b-2 font-medium capitalize ${activeTab === tab ? 'border-primary-500 text-primary-500' : 'border-transparent text-gray-500'}`}>
                  {tab}
                </button>
              ))}
            </div>
          </div>
          <div className="p-6">
            {activeTab === 'overview' && (
              <div className="grid grid-cols-2 md:grid-cols-4 gap-6">
                <div><p className="text-sm text-gray-500">Total Supply</p><p className="font-mono">{formatNumber(parseInt(data?.totalSupply || '0') / Math.pow(10, data?.decimals || 18))}</p></div>
                <div><p className="text-sm text-gray-500">Decimals</p><p className="font-mono">{data?.decimals}</p></div>
                <div><p className="text-sm text-gray-500">Type</p><p className="font-mono">{data?.type}</p></div>
                <div><p className="text-sm text-gray-500">Transfers</p><p className="font-mono">{formatNumber(data?.transfersCount || 0)}</p></div>
              </div>
            )}
            {activeTab === 'holders' && (
              <div className="space-y-2">
                {holders.length > 0 ? holders.map((h: any, i: number) => (
                  <div key={i} className="flex items-center justify-between p-4 hover:bg-gray-50 dark:hover:bg-dark-700 rounded-lg">
                    <Link href={`/address/${h.address}`} className="font-mono text-primary-500 hover:underline">{h.address?.slice(0, 10)}...</Link>
                    <div className="text-right">
                      <p className="font-medium">{(parseFloat(h.balance) / Math.pow(10, data?.decimals || 18)).toFixed(4)} {data?.symbol}</p>
                      <p className="text-sm text-gray-500">{h.percentage?.toFixed(2)}%</p>
                    </div>
                  </div>
                )) : <div className="text-center py-12 text-gray-500">No holders data</div>}
              </div>
            )}
            {activeTab === 'transfers' && (
              <div className="space-y-2">
                {transfers.length > 0 ? transfers.map((t: any, i: number) => (
                  <div key={i} className="flex items-center justify-between p-4 hover:bg-gray-50 dark:hover:bg-dark-700 rounded-lg">
                    <div>
                      <p className="text-sm"><span className="text-primary-500">{t.from?.slice(0, 8)}...</span> → <span className="text-primary-500">{t.to?.slice(0, 8)}...</span></p>
                    </div>
                    <div className="text-right">
                      <p className="font-medium">{(parseFloat(t.value) / Math.pow(10, data?.decimals || 18)).toFixed(4)} {data?.symbol}</p>
                      <p className="text-xs text-gray-500">{new Date(t.timestamp * 1000).toLocaleString()}</p>
                    </div>
                  </div>
                )) : <div className="text-center py-12 text-gray-500">No transfers</div>}
              </div>
            )}
            {activeTab === 'price' && <div className="text-center py-12 text-gray-500">Price history chart above</div>}
          </div>
        </div>
      </div>
    </div>
  )
}

