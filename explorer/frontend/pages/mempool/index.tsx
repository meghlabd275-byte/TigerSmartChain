// TigerScan - Mempool (Pending Transactions) Page with Full Features
// Real-time pending transaction monitoring with gas analysis

import { useState, useEffect, useMemo } from 'react'
import Head from 'next/head'
import Link from 'next/link'

interface PendingTx {
  hash: string
  from: string
  to: string
  value: string
  gas_price: string
  gas_limit: number
  gas_used?: number
  nonce: number
  data: string
  timestamp: number
  tx_type: 'legacy' | 'eip1559' | 'eip2930'
  max_fee_per_gas?: string
  max_priority_fee_per_gas?: string
  block_number?: number
}

interface PoolStats {
  total_pending: number
  avg_gas_price: number
  median_gas_price: number
  avg_tx_value: number
  gas_distribution: Record<string, number>
  top_senders: Array<{ address: string; tx_count: number; total_value: string }>
  top_receivers: Array<{ address: string; tx_count: number; total_value: string }>
  pending_value: number
  tx_types: Record<string, number>
}

interface FilterState {
  minGasPrice: string
  maxGasPrice: string
  search: string
  txType: string
}

// Generate sample pending transactions
const generateSampleTxs = (): PendingTx[] => {
  const txTypes: PendingTx['tx_type'][] = ['legacy', 'eip1559', 'eip2930']
  const now = Date.now()
  
  return Array.from({ length: 50 }, (_, i) => {
    const txType = txTypes[Math.floor(Math.random() * txTypes.length)]
    const gasPrice = Math.floor(Math.random() * 100 + 10) // 10-110 gwei
    const value = Math.floor(Math.random() * 10 + 0.1) // 0.1-10 TGR
    
    return {
      hash: '0x' + Math.random().toString(16).substring(2, 66).padStart(64, '0').substring(0, 64),
      from: '0x' + Math.random().toString(16).substring(2, 42).padStart(40, '0').substring(0, 40),
      to: '0x' + Math.random().toString(16).substring(2, 42).padStart(40, '0').substring(0, 40),
      value: String(value * 1e18),
      gas_price: String(gasPrice * 1e9),
      gas_limit: Math.floor(Math.random() * 100000 + 21000),
      nonce: Math.floor(Math.random() * 100),
      data: '0x' + Math.random().toString(16).substring(2, 138).padStart(136, '0').substring(0, 136),
      timestamp: now - Math.floor(Math.random() * 300000), // 0-5 min ago
      tx_type: txType,
      max_fee_per_gas: txType === 'eip1559' ? String((gasPrice + 20) * 1e9) : undefined,
      max_priority_fee_per_gas: txType === 'eip1559' ? String(Math.floor(Math.random() * 5 + 1) * 1e9) : undefined,
    }
  }).sort((a, b) => parseInt(b.gas_price) - parseInt(a.gas_price))
}

// Generate sample stats
const generateSampleStats = (txs: PendingTx[]): PoolStats => {
  const gasPrices = txs.map(t => parseInt(t.gas_price))
  const values = txs.map(t => parseFloat(t.value))
  
  const gasDistribution = {
    '0-20': 0,
    '20-50': 0,
    '50-100': 0,
    '100+': 0,
  }
  
  gasPrices.forEach(gp => {
    const gwei = gp / 1e9
    if (gwei < 20) gasDistribution['0-20']++
    else if (gwei < 50) gasDistribution['20-50']++
    else if (gwei < 100) gasDistribution['50-100']++
    else gasDistribution['100+']++
  })
  
  const txTypes = txs.reduce((acc, t) => {
    acc[t.tx_type] = (acc[t.tx_type] || 0) + 1
    return acc
  }, {} as Record<string, number>)
  
  // Top senders
  const senders = txs.reduce((acc, t) => {
    if (!acc[t.from]) {
      acc[t.from] = { address: t.from, tx_count: 0, total_value: 0 }
    }
    acc[t.from].tx_count++
    acc[t.from].total_value += parseFloat(t.value)
    return acc
  }, {} as Record<string, { address: string; tx_count: number; total_value: number }>)
  
  // Top receivers
  const receivers = txs.reduce((acc, t) => {
    if (!acc[t.to]) {
      acc[t.to] = { address: t.to, tx_count: 0, total_value: 0 }
    }
    acc[t.to].tx_count++
    acc[t.to].total_value += parseFloat(t.value)
    return acc
  }, {} as Record<string, { address: string; tx_count: number; total_value: number }>)
  
  return {
    total_pending: txs.length,
    avg_gas_price: gasPrices.reduce((a, b) => a + b, 0) / gasPrices.length,
    median_gas_price: gasPrices.sort((a, b) => a - b)[Math.floor(gasPrices.length / 2)],
    avg_tx_value: values.reduce((a, b) => a + b, 0) / values.length,
    gas_distribution: gasDistribution,
    top_senders: Object.values(senders).sort((a, b) => b.tx_count - a.tx_count).slice(0, 5),
    top_receivers: Object.values(receivers).sort((a, b) => b.tx_count - a.tx_count).slice(0, 5),
    pending_value: values.reduce((a, b) => a + b, 0),
    tx_types: txTypes,
  }
}

export default function Mempool() {
  const [txs, setTxs] = useState<PendingTx[]>([])
  const [stats, setStats] = useState<PoolStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [filter, setFilter] = useState<FilterState>({
    minGasPrice: '',
    maxGasPrice: '',
    search: '',
    txType: '',
  })
  const [sortBy, setSortBy] = useState<'gas_price' | 'nonce' | 'value' | 'time'>('gas_price')
  const [autoRefresh, setAutoRefresh] = useState(true)
  const [lastUpdate, setLastUpdate] = useState<Date | null>(null)

  useEffect(() => {
    const sampleTxs = generateSampleTxs()
    const sampleStats = generateSampleStats(sampleTxs)
    
    setTimeout(() => {
      setTxs(sampleTxs)
      setStats(sampleStats)
      setLastUpdate(new Date())
      setLoading(false)
    }, 300)
    
    // Auto refresh every 10 seconds
    const interval = setInterval(() => {
      if (autoRefresh) {
        const newTxs = generateSampleTxs()
        const newStats = generateSampleStats(newTxs)
        setTxs(newTxs)
        setStats(newStats)
        setLastUpdate(new Date())
      }
    }, 10000)
    
    return () => clearInterval(interval)
  }, [autoRefresh])

  // Filter transactions
  const filteredTxs = useMemo(() => {
    let filtered = [...txs]
    
    if (filter.minGasPrice) {
      const min = parseFloat(filter.minGasPrice) * 1e9
      filtered = filtered.filter(t => parseInt(t.gas_price) >= min)
    }
    
    if (filter.maxGasPrice) {
      const max = parseFloat(filter.maxGasPrice) * 1e9
      filtered = filtered.filter(t => parseInt(t.gas_price) <= max)
    }
    
    if (filter.search) {
      const search = filter.search.toLowerCase()
      filtered = filtered.filter(t =>
        t.hash.toLowerCase().includes(search) ||
        t.from.toLowerCase().includes(search) ||
        t.to.toLowerCase().includes(search)
      )
    }
    
    if (filter.txType) {
      filtered = filtered.filter(t => t.tx_type === filter.txType)
    }
    
    // Sort
    filtered.sort((a, b) => {
      switch (sortBy) {
        case 'gas_price':
          return parseInt(b.gas_price) - parseInt(a.gas_price)
        case 'nonce':
          return b.nonce - a.nonce
        case 'value':
          return parseFloat(b.value) - parseFloat(a.value)
        case 'time':
          return b.timestamp - a.timestamp
        default:
          return 0
      }
    })
    
    return filtered
  }, [txs, filter, sortBy])

  const formatAddress = (addr: string) => {
    if (!addr || addr.length < 10) return addr
    return `${addr.substring(0, 6)}...${addr.substring(38)}`
  }

  const formatValue = (value: string) => {
    const num = parseFloat(value) / 1e18
    return num.toLocaleString(undefined, { maximumFractionDigits: 4 })
  }

  const formatGasPrice = (price: string) => {
    const gwei = parseInt(price) / 1e9
    return gwei.toFixed(2)
  }

  const formatTime = (timestamp: number) => {
    const diff = Date.now() - timestamp
    const seconds = Math.floor(diff / 1000)
    const minutes = Math.floor(diff / 60000)
    
    if (seconds < 60) return `${seconds}s ago`
    return `${minutes}m ago`
  }

  const getGasPriceColor = (price: string) => {
    const gwei = parseInt(price) / 1e9
    if (gwei < 20) return 'text-green-400'
    if (gwei < 50) return 'text-yellow-400'
    if (gwei < 100) return 'text-orange-400'
    return 'text-red-400'
  }

  if (loading) {
    return (
      <div className="min-h-screen bg-gray-900 flex items-center justify-center">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-orange-500 mx-auto mb-4"></div>
          <p className="text-gray-400">Loading Mempool...</p>
        </div>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-gray-900">
      <Head><title>Mempool - TigerScan</title></Head>
      
      {/* Header */}
      <div className="bg-gray-800 border-b border-gray-700">
        <div className="container mx-auto px-4 py-4">
          <div className="flex items-center justify-between">
            <Link href="/" className="text-2xl font-bold text-orange-500">🐯 TigerScan</Link>
            <nav className="flex gap-6">
              <Link href="/blocks" className="text-gray-300 hover:text-white">Blocks</Link>
              <Link href="/transactions" className="text-gray-300 hover:text-white">Transactions</Link>
              <Link href="/mempool" className="text-orange-400 hover:text-orange-300">Mempool</Link>
            </nav>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="container mx-auto px-4 py-8">
        <div className="flex items-center justify-between mb-8">
          <h1 className="text-3xl font-bold text-white">Mempool</h1>
          <div className="flex items-center gap-4">
            <button
              onClick={() => setAutoRefresh(!autoRefresh)}
              className={`px-4 py-2 rounded-lg font-medium transition-colors ${
                autoRefresh 
                  ? 'bg-green-100 text-green-800' 
                  : 'bg-gray-700 text-gray-300'
              }`}
            >
              {autoRefresh ? '🔴 Live' : '⏸ Paused'}
            </button>
            {lastUpdate && (
              <span className="text-sm text-gray-500">
                Updated {formatTime(lastUpdate.getTime())}
              </span>
            )}
          </div>
        </div>

        {/* Stats Grid */}
        {stats && (
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
            <div className="bg-gray-800 rounded-lg p-4 border border-gray-700">
              <p className="text-gray-400 text-sm">Pending Transactions</p>
              <p className="text-2xl font-bold text-white">{stats.total_pending}</p>
            </div>
            <div className="bg-gray-800 rounded-lg p-4 border border-gray-700">
              <p className="text-gray-400 text-sm">Avg Gas Price</p>
              <p className="text-2xl font-bold text-orange-400">{formatGasPrice(String(Math.floor(stats.avg_gas_price)))} Gwei</p>
            </div>
            <div className="bg-gray-800 rounded-lg p-4 border border-gray-700">
              <p className="text-gray-400 text-sm">Median Gas Price</p>
              <p className="text-2xl font-bold text-white">{formatGasPrice(String(stats.median_gas_price))} Gwei</p>
            </div>
            <div className="bg-gray-800 rounded-lg p-4 border border-gray-700">
              <p className="text-gray-400 text-sm">Pending Value</p>
              <p className="text-2xl font-bold text-green-400">{stats.pending_value.toFixed(2)} TGR</p>
            </div>
          </div>
        )}

        {/* Gas Distribution & Tx Types */}
        {stats && (
          <div className="grid md:grid-cols-2 gap-6 mb-8">
            {/* Gas Distribution */}
            <div className="bg-gray-800 rounded-lg p-6 border border-gray-700">
              <h3 className="text-lg font-semibold text-white mb-4">Gas Distribution</h3>
              <div className="space-y-2">
                {Object.entries(stats.gas_distribution).map(([range, count]) => (
                  <div key={range} className="flex items-center">
                    <span className="w-20 text-gray-400 text-sm">{range} Gwei</span>
                    <div className="flex-1 h-4 bg-gray-700 rounded-full overflow-hidden mx-2">
                      <div 
                        className="h-full bg-orange-500 rounded-full"
                        style={{ width: `${(count / stats.total_pending) * 100}%` }}
                      />
                    </div>
                    <span className="text-white text-sm w-12 text-right">{count}</span>
                  </div>
                ))}
              </div>
            </div>

            {/* Transaction Types */}
            <div className="bg-gray-800 rounded-lg p-6 border border-gray-700">
              <h3 className="text-lg font-semibold text-white mb-4">Transaction Types</h3>
              <div className="space-y-2">
                {Object.entries(stats.tx_types).map(([type_, count]) => (
                  <div key={type_} className="flex items-center justify-between">
                    <span className="text-gray-400 capitalize">{type_.replace('eip', 'EIP ')}</span>
                    <div className="flex items-center">
                      <div className="w-32 h-4 bg-gray-700 rounded-full overflow-hidden mx-2">
                        <div 
                          className="h-full bg-blue-500 rounded-full"
                          style={{ width: `${(count / stats.total_pending) * 100}%` }}
                        />
                      </div>
                      <span className="text-white text-sm">{count}</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}

        {/* Top Senders & Receivers */}
        {stats && (
          <div className="grid md:grid-cols-2 gap-6 mb-8">
            <div className="bg-gray-800 rounded-lg p-6 border border-gray-700">
              <h3 className="text-lg font-semibold text-white mb-4">Top Senders</h3>
              <div className="space-y-2">
                {stats.top_senders.map((sender, i) => (
                  <div key={i} className="flex items-center justify-between">
                    <a href={`/address/${sender.address}`} className="text-orange-400 hover:underline font-mono text-sm">
                      {formatAddress(sender.address)}
                    </a>
                    <div className="text-right">
                      <span className="text-white">{sender.tx_count} tx</span>
                      <span className="text-gray-500 ml-2">({formatValue(String(sender.total_value * 1e18))} TGR)</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
            <div className="bg-gray-800 rounded-lg p-6 border border-gray-700">
              <h3 className="text-lg font-semibold text-white mb-4">Top Receivers</h3>
              <div className="space-y-2">
                {stats.top_receivers.map((receiver, i) => (
                  <div key={i} className="flex items-center justify-between">
                    <a href={`/address/${receiver.address}`} className="text-orange-400 hover:underline font-mono text-sm">
                      {formatAddress(receiver.address)}
                    </a>
                    <div className="text-right">
                      <span className="text-white">{receiver.tx_count} tx</span>
                      <span className="text-gray-500 ml-2">({formatValue(String(receiver.total_value * 1e18))} TGR)</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}

        {/* Filters */}
        <div className="flex flex-wrap gap-4 mb-6">
          <input
            type="number"
            placeholder="Min gas (Gwei)..."
            value={filter.minGasPrice}
            onChange={(e) => setFilter({ ...filter, minGasPrice: e.target.value })}
            className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white"
          />
          <input
            type="number"
            placeholder="Max gas (Gwei)..."
            value={filter.maxGasPrice}
            onChange={(e) => setFilter({ ...filter, maxGasPrice: e.target.value })}
            className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white"
          />
          <input
            type="text"
            placeholder="Search hash, address..."
            value={filter.search}
            onChange={(e) => setFilter({ ...filter, search: e.target.value })}
            className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white flex-1 min-w-[200px]"
          />
          <select
            value={filter.txType}
            onChange={(e) => setFilter({ ...filter, txType: e.target.value })}
            className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white"
          >
            <option value="">All Types</option>
            <option value="legacy">Legacy</option>
            <option value="eip1559">EIP-1559</option>
            <option value="eip2930">EIP-2930</option>
          </select>
          <select
            value={sortBy}
            onChange={(e) => setSortBy(e.target.value as any)}
            className="px-4 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white"
          >
            <option value="gas_price">Sort by Gas</option>
            <option value="value">Sort by Value</option>
            <option value="nonce">Sort by Nonce</option>
            <option value="time">Sort by Time</option>
          </select>
        </div>

        {/* Transactions Table */}
        <div className="bg-gray-800 rounded-lg border border-gray-700 overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-gray-700">
                <tr>
                  <th className="px-4 py-3 text-left text-sm font-medium text-gray-300">Hash</th>
                  <th className="px-4 py-3 text-left text-sm font-medium text-gray-300">From</th>
                  <th className="px-4 py-3 text-left text-sm font-medium text-gray-300">To</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">Value</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">Gas Price</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">Gas Limit</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">Nonce</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">Type</th>
                  <th className="px-4 py-3 text-right text-sm font-medium text-gray-300">Time</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-700">
                {filteredTxs.map((tx) => (
                  <tr key={tx.hash} className="hover:bg-gray-750">
                    <td className="px-4 py-3">
                      <a href={`/transaction/${tx.hash}`} className="text-orange-400 hover:underline font-mono text-sm">
                        {formatAddress(tx.hash)}
                      </a>
                    </td>
                    <td className="px-4 py-3">
                      <a href={`/address/${tx.from}`} className="text-orange-400 hover:underline font-mono text-sm">
                        {formatAddress(tx.from)}
                      </a>
                    </td>
                    <td className="px-4 py-3">
                      <a href={`/address/${tx.to}`} className="text-orange-400 hover:underline font-mono text-sm">
                        {formatAddress(tx.to)}
                      </a>
                    </td>
                    <td className="px-4 py-3 text-right text-white font-medium">
                      {formatValue(tx.value)} TGR
                    </td>
                    <td className={`px-4 py-3 text-right font-medium ${getGasPriceColor(tx.gas_price)}`}>
                      {formatGasPrice(tx.gas_price)} Gwei
                    </td>
                    <td className="px-4 py-3 text-right text-gray-400">
                      {tx.gas_limit.toLocaleString()}
                    </td>
                    <td className="px-4 py-3 text-right text-gray-400">
                      {tx.nonce}
                    </td>
                    <td className="px-4 py-3 text-right">
                      <span className="px-2 py-1 rounded text-xs font-medium bg-blue-900 text-blue-300">
                        {tx.tx_type.replace('eip', 'EIP ')}
                      </span>
                    </td>
                    <td className="px-4 py-3 text-right text-gray-400 text-sm">
                      {formatTime(tx.timestamp)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  )
}