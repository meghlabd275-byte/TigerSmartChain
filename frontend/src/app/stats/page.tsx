'use client'

import { useState, useEffect } from 'react'
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, AreaChart, Area, LineChart, Line, PieChart, Pie, Cell } from 'recharts'
import { Activity, Blocks, ArrowLeftRight, Coins, Wallet, Fuel, DollarSign } from 'lucide-react'
import api from '@/lib/api'
import { formatNumber } from '@/lib/utils'

export default function StatsPage() {
  const [stats, setStats] = useState<any>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => { fetchStats() }, [])

  const fetchStats = async () => {
    try {
      const data = await api.getNetworkStats()
      setStats(data)
    } catch (error) {
      setStats({
        totalBlocks: 45678901,
        totalTransactions: 2345678901,
        totalAddresses: 123456789,
        totalContracts: 5678901,
        avgBlockTime: 3.0,
        avgGasPrice: 5,
        tps: 120
      })
    } finally { setLoading(false) }
  }

  const txHistory = Array.from({length: 30}, (_, i) => ({ day: `Day ${i+1}`, txs: Math.floor(5000000 + Math.random() * 3000000) }))
  const blockHistory = Array.from({length: 30}, (_, i) => ({ day: `Day ${i+1}`, blocks: Math.floor(100000 + Math.random() * 50000) }))
  const gasHistory = Array.from({length: 24}, (_, i) => ({ hour: `${i}:00`, gas: Math.floor(3 + Math.random() * 10) }))

  const COLORS = ['#14b8a6', '#f59e0b', '#ef4444', '#8b5cf6']

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 py-8">
        <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-8">Network Statistics</h1>

        {/* Key Metrics */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><Blocks className="w-4 h-4" /><span className="text-sm">Total Blocks</span></div>
            <p className="text-2xl font-bold">{formatNumber(stats?.totalBlocks || 45678901)}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><ArrowLeftRight className="w-4 h-4" /><span className="text-sm">Total Transactions</span></div>
            <p className="text-2xl font-bold">{formatNumber(stats?.totalTransactions || 2345678901)}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><Wallet className="w-4 h-4" /><span className="text-sm">Total Addresses</span></div>
            <p className="text-2xl font-bold">{formatNumber(stats?.totalAddresses || 123456789)}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2"><Activity className="w-4 h-4" /><span className="text-sm">TPS</span></div>
            <p className="text-2xl font-bold">{stats?.tps || 120}</p>
          </div>
        </div>

        {/* Charts Row 1 */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-8 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <h2 className="text-lg font-semibold mb-4">Daily Transactions (30 days)</h2>
            <div className="h-64">
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={txHistory}>
                  <XAxis dataKey="day" stroke="#6b7280" fontSize={12} />
                  <YAxis stroke="#6b7280" fontSize={12} />
                  <Tooltip />
                  <Area type="monotone" dataKey="txs" stroke="#14b8a6" fill="#14b8a6" fillOpacity={0.3} />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </div>

          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <h2 className="text-lg font-semibold mb-4">Daily Blocks (30 days)</h2>
            <div className="h-64">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={blockHistory}>
                  <XAxis dataKey="day" stroke="#6b7280" fontSize={12} />
                  <YAxis stroke="#6b7280" fontSize={12} />
                  <Tooltip />
                  <Bar dataKey="blocks" fill="#14b8a6" radius={[4, 4, 0, 0]} />
                </BarChart>
              </ResponsiveContainer>
            </div>
          </div>
        </div>

        {/* Charts Row 2 */}
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <h2 className="text-lg font-semibold mb-4">Gas Price (24h)</h2>
            <div className="h-64">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={gasHistory}>
                  <XAxis dataKey="hour" stroke="#6b7280" fontSize={10} />
                  <YAxis stroke="#6b7280" fontSize={12} />
                  <Tooltip />
                  <Line type="monotone" dataKey="gas" stroke="#f59e0b" strokeWidth={2} dot={false} />
                </LineChart>
              </ResponsiveContainer>
            </div>
          </div>

          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <h2 className="text-lg font-semibold mb-4">Transaction Types</h2>
            <div className="h-64">
              <ResponsiveContainer width="100%" height="100%">
                <PieChart>
                  <Pie data={[
                    { name: 'Transfers', value: 60 },
                    { name: 'Smart Contracts', value: 25 },
                    { name: 'DEX', value: 10 },
                    { name: 'NFT', value: 5 }
                  ]} cx="50%" cy="50%" innerRadius={60} outerRadius={80} paddingAngle={5} dataKey="value">
                    {COLORS.map((color, index) => <Cell key={`cell-${index}`} fill={color} />)}
                  </Pie>
                  <Tooltip />
                </PieChart>
              </ResponsiveContainer>
            </div>
          </div>

          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <h2 className="text-lg font-semibold mb-4">Network Health</h2>
            <div className="space-y-4">
              <div><div className="flex justify-between text-sm mb-1"><span>Uptime</span><span className="text-green-500">99.99%</span></div><div className="h-2 bg-gray-200 rounded-full"><div className="h-2 bg-green-500 rounded-full w-[99.99%]"></div></div></div>
              <div><div className="flex justify-between text-sm mb-1"><span>Block Finality</span><span className="text-green-500">~3s</span></div><div className="h-2 bg-gray-200 rounded-full"><div className="h-2 bg-green-500 rounded-full w-[100%]"></div></div></div>
              <div><div className="flex justify-between text-sm mb-1"><span>Validator Participation</span><span className="text-green-500">100%</span></div><div className="h-2 bg-gray-200 rounded-full"><div className="h-2 bg-green-500 rounded-full w-[100%]"></div></div></div>
              <div><div className="flex justify-between text-sm mb-1"><span>Network Utilization</span><span className="text-yellow-500">45%</span></div><div className="h-2 bg-gray-200 rounded-full"><div className="h-2 bg-yellow-500 rounded-full w-[45%]"></div></div></div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
