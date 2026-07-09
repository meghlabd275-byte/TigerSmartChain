'use client'

import { useState, useEffect } from 'react'
import { Zap, Clock, Gauge, Activity } from 'lucide-react'
import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts'
import api from '@/lib/api'
import { formatCurrency } from '@/lib/utils'

export default function GasPage() {
  const [gasData, setGasData] = useState<any>(null)
  const [history, setHistory] = useState<any[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => { fetchGasData() }, [])

  const fetchGasData = async () => {
    try {
      const [oracle, historyData] = await Promise.all([api.getGasOracle(), api.getGasHistory({ timeframe: '24h' })])
      setGasData(oracle)
      setHistory(historyData || generateMockHistory())
    } catch (error) {
      setGasData({ slow: 3, standard: 5, fast: 8, baseFee: 5, networkUtilization: 0.45 })
      setHistory(generateMockHistory())
    } finally { setLoading(false) }
  }

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 py-8">
        <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-8">Gas Tracker</h1>

        <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><Clock className="w-4 h-4" /><span className="text-sm">Slow</span></div>
            <p className="text-2xl font-bold">{gasData?.slow || 3} Gwei</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><Zap className="w-4 h-4" /><span className="text-sm">Standard</span></div>
            <p className="text-2xl font-bold">{gasData?.standard || 5} Gwei</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><Gauge className="w-4 h-4" /><span className="text-sm">Fast</span></div>
            <p className="text-2xl font-bold">{gasData?.fast || 8} Gwei</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><Activity className="w-4 h-4" /><span className="text-sm">Base Fee</span></div>
            <p className="text-2xl font-bold">{gasData?.baseFee || 5} Gwei</p>
          </div>
        </div>

        <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
          <h2 className="text-lg font-semibold mb-4">Gas Price History (24h)</h2>
          <div className="h-80">
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
          </div>
        </div>
      </div>
    </div>
  )
}

function generateMockHistory() {
  const data = []
  const now = Math.floor(Date.now() / 1000)
  for (let i = 24; i >= 0; i--) {
    data.push({ timestamp: now - i * 3600, slow: 2 + Math.random() * 2, standard: 4 + Math.random() * 3, fast: 7 + Math.random() * 5 })
  }
  return data
}
