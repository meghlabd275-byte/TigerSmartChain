'use client'

import { useState, useEffect } from 'react'
import { Zap, Clock, Gauge, Activity } from 'lucide-react'
import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts'
import api from '@/lib/api'

export default function GasPage() {
  const [gasData, setGasData] = useState<any>(null)
  const [history, setHistory] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => { fetchGasData() }, [])

  const fetchGasData = async () => {
    setLoading(true)
    setError(null)
    try {
      const [oracle, historyData] = await Promise.all([api.getGasOracle(), api.getGasHistory({ timeframe: '24h' })])
      setGasData(oracle)
      setHistory(historyData || [])
    } catch (error) {
      setGasData(null)
      setHistory([])
      setError('Failed to load data. Please try again later.')
    } finally { setLoading(false) }
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
        <button onClick={fetchGasData} className="px-4 py-2 rounded-lg bg-primary-500 text-white hover:bg-primary-600">Retry</button>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 py-8">
        <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-8">Gas Tracker</h1>

        <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><Clock className="w-4 h-4" /><span className="text-sm">Slow</span></div>
            <p className="text-2xl font-bold">{gasData?.slow || 0} Gwei</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><Zap className="w-4 h-4" /><span className="text-sm">Standard</span></div>
            <p className="text-2xl font-bold">{gasData?.standard || 0} Gwei</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><Gauge className="w-4 h-4" /><span className="text-sm">Fast</span></div>
            <p className="text-2xl font-bold">{gasData?.fast || 0} Gwei</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><Activity className="w-4 h-4" /><span className="text-sm">Base Fee</span></div>
            <p className="text-2xl font-bold">{gasData?.baseFee || 0} Gwei</p>
          </div>
        </div>

        <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
          <h2 className="text-lg font-semibold mb-4">Gas Price History (24h)</h2>
          <div className="h-80 flex items-center justify-center">
            {history.length === 0 ? (
              <span className="text-gray-500 dark:text-gray-400">No history data available</span>
            ) : (
            <ResponsiveContainer width="100%" height="100%">
              <LineChart data={history}>
                <XAxis dataKey="timestamp" stroke="#6b7280" fontSize={12} tickFormatter={(v) => new Date(v * 1000).toLocaleTimeString()} />
                <YAxis stroke="#6b7280" fontSize={12} tickFormatter={(v) => `${v} Gwei`} />
                <Tooltip formatter={(v: number) => [`${v} Gwei`, 'Gas Price']} />
                <Line type="monotone" dataKey="slow" stroke="#ef4444" strokeWidth={2} dot={false} name="Slow" />
                <Line type="monotone" dataKey="standard" stroke="#eab308" strokeWidth={2} dot={false} name="Standard" />
                <Line type="monotone" dataKey="fast" stroke="#22c55e" strokeWidth={2} dot={false} name="Fast" />
              </LineChart>
            </ResponsiveContainer>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
