'use client'

import { useState, useEffect } from 'react'
import { useParams } from 'next/navigation'
import Link from 'next/link'
import { Copy, Check, Image, TrendingUp, Users, Flame, Hash, ExternalLink } from 'lucide-react'
import { AreaChart, Area, XAxis, YAxis, Tooltip, ResponsiveContainer, BarChart, Bar } from 'recharts'
import api from '@/lib/api'
import { formatNumber, formatCurrency, copyToClipboard, formatTimestamp } from '@/lib/utils'

export default function NFTCollectionPage() {
  const params = useParams()
  const address = params.address as string
  
  const [collection, setCollection] = useState<any>(null)
  const [floorHistory, setFloorHistory] = useState<any[]>([])
  const [volumeHistory, setVolumeHistory] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<'overview' | 'holders' | 'transfers'>('overview')
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (address) fetchData()
  }, [address])

  const fetchData = async () => {
    setLoading(true)
    setError(null)
    try {
      const [col, floor] = await Promise.all([
        api.getNFTCollection(address),
        api.getNFTFloorPrice(address)
      ])
      setCollection(col)
      setFloorHistory([])
      setVolumeHistory([])
    } catch (error) {
      setCollection(null)
      setFloorHistory([])
      setVolumeHistory([])
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

  if (error || !collection) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-dark-900 flex flex-col items-center justify-center gap-4">
        <p className="text-red-500">{error || 'Collection not found'}</p>
        <button onClick={fetchData} className="px-4 py-2 rounded-lg bg-primary-500 text-white hover:bg-primary-600">Retry</button>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {/* Header */}
        <div className="mb-8">
          <div className="flex items-center space-x-4 mb-4">
            <div className="w-20 h-20 rounded-xl bg-gray-200 dark:bg-dark-700 flex items-center justify-center overflow-hidden">
              {collection?.imageUrl ? (
                <img src={collection.imageUrl} alt={collection.name} className="w-full h-full object-cover" />
              ) : (
                <Image className="w-10 h-10 text-gray-400" />
              )}
            </div>
            <div>
              <h1 className="text-3xl font-bold text-gray-900 dark:text-white">{collection?.name}</h1>
              <p className="text-gray-500">{collection?.symbol}</p>
            </div>
          </div>
          
          <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 p-4">
            <div className="flex items-center justify-between">
              <code className="text-lg font-mono text-gray-900 dark:text-white break-all">{address}</code>
              <button onClick={handleCopy} className="p-2 hover:bg-gray-100 dark:hover:bg-dark-700 rounded-lg">
                {copied ? <Check className="w-5 h-5 text-green-500" /> : <Copy className="w-5 h-5 text-gray-400" />}
              </button>
            </div>
          </div>
        </div>

        {/* Stats Grid */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <TrendingUp className="w-4 h-4" />
              <span className="text-sm">Floor Price</span>
            </div>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{formatCurrency(collection?.floorPrice || 0)} BNB</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <Flame className="w-4 h-4" />
              <span className="text-sm">Volume (24h)</span>
            </div>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{formatCurrency(collection?.volume24h || 0)}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <Users className="w-4 h-4" />
              <span className="text-sm">Owners</span>
            </div>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{formatNumber(collection?.ownerCount || 0)}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <Image className="w-4 h-4" />
              <span className="text-sm">Items</span>
            </div>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{formatNumber(collection?.totalSupply || 0)}</p>
          </div>
        </div>

        {/* Charts */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-8 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <h2 className="text-lg font-semibold text-gray-900 dark:text-white mb-4">Floor Price History</h2>
            <div className="h-64 flex items-center justify-center text-gray-500 dark:text-gray-400">
              {floorHistory.length === 0 ? 'No data available' : (
                <ResponsiveContainer width="100%" height="100%">
                  <AreaChart data={floorHistory}>
                    <defs>
                      <linearGradient id="colorFloor" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="5%" stopColor="#14b8a6" stopOpacity={0.3} />
                        <stop offset="95%" stopColor="#14b8a6" stopOpacity={0} />
                      </linearGradient>
                    </defs>
                    <XAxis dataKey="date" stroke="#6b7280" fontSize={12} />
                    <YAxis stroke="#6b7280" fontSize={12} tickFormatter={(v) => `${v} BNB`} />
                    <Tooltip formatter={(v: number) => [`${v} BNB`, 'Floor']} />
                    <Area type="monotone" dataKey="floor" stroke="#14b8a6" strokeWidth={2} fillOpacity={1} fill="url(#colorFloor)" />
                  </AreaChart>
                </ResponsiveContainer>
              )}
            </div>
          </div>

          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <h2 className="text-lg font-semibold text-gray-900 dark:text-white mb-4">Volume History</h2>
            <div className="h-64 flex items-center justify-center text-gray-500 dark:text-gray-400">
              {volumeHistory.length === 0 ? 'No data available' : (
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={volumeHistory}>
                    <XAxis dataKey="date" stroke="#6b7280" fontSize={12} />
                    <YAxis stroke="#6b7280" fontSize={12} />
                    <Tooltip formatter={(v: number) => [`$${formatNumber(v)}`, 'Volume']} />
                    <Bar dataKey="volume" fill="#14b8a6" radius={[4, 4, 0, 0]} />
                  </BarChart>
                </ResponsiveContainer>
              )}
            </div>
          </div>
        </div>

        {/* Tabs */}
        <div className="bg-white dark:bg-dark-800 rounded-xl border">
          <div className="border-b border-gray-200 dark:border-dark-700">
            <div className="flex space-x-8 px-6">
              {(['overview', 'holders', 'transfers'] as const).map((tab) => (
                <button
                  key={tab}
                  onClick={() => setActiveTab(tab)}
                  className={`py-4 border-b-2 font-medium capitalize ${activeTab === tab ? 'border-primary-500 text-primary-500' : 'border-transparent text-gray-500'}`}
                >
                  {tab}
                </button>
              ))}
            </div>
          </div>
          <div className="p-6">
            {activeTab === 'overview' && (
              <div className="grid grid-cols-2 md:grid-cols-4 gap-6">
                <div><p className="text-sm text-gray-500">Type</p><p className="font-medium">{collection?.type}</p></div>
                <div><p className="text-sm text-gray-500">Minted</p><p className="font-medium">{formatNumber(collection?.mintedCount || 0)}</p></div>
                <div><p className="text-sm text-gray-500">Volume (7d)</p><p className="font-medium">{formatCurrency(collection?.volume7d || 0)}</p></div>
                <div><p className="text-sm text-gray-500">Volume (30d)</p><p className="font-medium">{formatCurrency(collection?.volume30d || 0)}</p></div>
              </div>
            )}
            {activeTab === 'holders' && (
              <div className="text-center py-12 text-gray-500">Owner data loading...</div>
            )}
            {activeTab === 'transfers' && (
              <div className="text-center py-12 text-gray-500">Transfer history loading...</div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
