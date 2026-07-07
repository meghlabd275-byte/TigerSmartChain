// TigerScan - Blockchain Explorer for TigerSmartChain
// Next.js frontend page with Tailwind CSS

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'

// Types
interface Block {
  number: number
  hash: string
  timestamp: number
  transactions: number
  gasUsed: number
  miner: string
}

interface Transaction {
  hash: string
  from: string
  to: string
  value: string
  gasPrice: string
  status: boolean
}

interface Stats {
  totalBlocks: number
  totalTransactions: number
  tps: number
  avgGasPrice: number
}

export default function Home() {
  const [blocks, setBlocks] = useState<Block[]>([])
  const [txs, setTxs] = useState<Transaction[]>([])
  const [stats, setStats] = useState<Stats>({ totalBlocks: 0, totalTransactions: 0, tps: 0, avgGasPrice: 0 })
  const [search, setSearch] = useState('')
  const [loading, setLoading] = useState(true)

  useEffect(() => { fetchData() }, [])

  async function fetchData() {
    try {
      // In production these would be real API calls
      const [b, t, s] = await Promise.all([
        fetch('/api/v1/blocks?limit=10').then(r => r.ok ? r.json() : []),
        fetch('/api/v1/transactions?limit=10').then(r => r.ok ? r.json() : []),
        fetch('/api/v1/analytics/stats').then(r => r.ok ? r.json() : { totalBlocks: 1234567, totalTransactions: 9876543, tps: 15.5, avgGasPrice: 1000000000 })
      ])
      setBlocks(Array.isArray(b) ? b.slice(0, 10) : [])
      setTxs(Array.isArray(t) ? t.slice(0, 10) : [])
      setStats(s)
    } catch (e) {
      console.error(e)
      // Mock data for display if API fails
      setStats({ totalBlocks: 1234567, totalTransactions: 9876543, tps: 15.5, avgGasPrice: 1000000000 })
    }
    finally { setLoading(false) }
  }

  function handleSearch(e: React.FormEvent) {
    e.preventDefault()
    if (search.trim()) {
      window.location.href = `/search?q=${encodeURIComponent(search)}`
    }
  }

  function fmtAddr(addr: string): string {
    if (!addr || addr.length < 16) return addr
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`
  }

  function fmtVal(v: string): string {
    const num = parseFloat(v || '0')
    return (num / 1e18).toFixed(4)
  }

  function fmtTime(ts: number): string {
    return new Date(ts * 1000).toLocaleString()
  }

  return (
    <div className="min-h-screen bg-gray-50 font-sans text-gray-900">
      <Head>
        <title>TigerScan.io - TigerSmartChain Explorer</title>
        <meta name="description" content="TigerScan.io - Blockchain Explorer for TigerSmartChain" />
      </Head>

      {/* Header */}
      <header className="bg-[#1a1a2e] text-white py-4 px-6 sticky top-0 z-50 shadow-md">
        <div className="max-w-7xl mx-auto flex justify-between items-center">
          <Link href="/" className="text-[#ff6b35] text-2xl font-bold flex items-center gap-2">
            <span>🐯 TigerScan.io</span>
          </Link>
          <nav className="hidden md:flex gap-6 text-sm font-medium">
            <Link href="/blocks" className="hover:text-[#ff6b35] transition">Blocks</Link>
            <Link href="/transactions" className="hover:text-[#ff6b35] transition">Transactions</Link>
            <Link href="/tokens" className="hover:text-[#ff6b35] transition">Tokens</Link>
            <Link href="/validators" className="hover:text-[#ff6b35] transition">Validators</Link>
            <Link href="/nfts" className="hover:text-[#ff6b35] transition">NFTs</Link>
          </nav>
        </div>
      </header>

      {/* Search Hero Section */}
      <section className="bg-gradient-to-br from-[#1a1a2e] to-[#16213e] py-16 px-6 text-center">
        <div className="max-w-4xl mx-auto">
          <h1 className="text-3xl md:text-4xl font-bold text-white mb-8">TigerSmartChain Explorer</h1>
          <form onSubmit={handleSearch} className="flex flex-col md:flex-row gap-2 max-w-2xl mx-auto">
            <input
              type="text"
              value={search}
              onChange={e => setSearch(e.target.value)}
              placeholder="Search by Address, Txn Hash, Block"
              className="flex-1 p-4 rounded-lg focus:outline-none focus:ring-2 focus:ring-[#ff6b35] text-gray-800"
            />
            <button type="submit" className="bg-[#ff6b35] hover:bg-[#e85a24] text-white px-8 py-4 rounded-lg font-bold transition duration-200">
              Search
            </button>
          </form>
        </div>
      </section>

      {/* Stats Cards */}
      <section className="max-w-7xl mx-auto -mt-8 px-6">
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          {[
            { label: 'Total Blocks', value: stats.totalBlocks.toLocaleString() },
            { label: 'Total Transactions', value: stats.totalTransactions.toLocaleString() },
            { label: 'Network TPS', value: stats.tps },
            { label: 'Avg Gas Price', value: `${(stats.avgGasPrice / 1e9).toFixed(2)} Gwei` },
          ].map((stat, i) => (
            <div key={i} className="bg-white p-6 rounded-xl shadow-sm border border-gray-100">
              <p className="text-gray-500 text-xs uppercase tracking-wider font-semibold mb-1">{stat.label}</p>
              <p className="text-2xl font-bold text-[#1a1a2e]">{stat.value}</p>
            </div>
          ))}
        </div>
      </section>

      <main className="max-w-7xl mx-auto py-12 px-6 grid grid-cols-1 lg:grid-cols-2 gap-8">
        {/* Latest Blocks */}
        <section className="bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden">
          <div className="p-5 border-b border-gray-100 flex justify-between items-center">
            <h2 className="font-bold text-gray-800">Latest Blocks</h2>
            <Link href="/blocks" className="text-xs font-bold text-[#ff6b35] hover:underline uppercase">View All</Link>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead className="bg-gray-50 text-gray-500 uppercase text-[10px] font-bold">
                <tr>
                  <th className="px-5 py-3">Block</th>
                  <th className="px-5 py-3">Time</th>
                  <th className="px-5 py-3">Txns</th>
                  <th className="px-5 py-3 text-right">Miner</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-100">
                {loading ? (
                  <tr><td colSpan={4} className="p-5 text-center text-gray-400">Loading blocks...</td></tr>
                ) : blocks.length === 0 ? (
                  <tr><td colSpan={4} className="p-5 text-center text-gray-400">No recent blocks found</td></tr>
                ) : blocks.map(b => (
                  <tr key={b.number} className="hover:bg-gray-50 transition">
                    <td className="px-5 py-4">
                      <Link href={`/block/${b.number}`} className="text-[#ff6b35] font-medium">{b.number}</Link>
                    </td>
                    <td className="px-5 py-4 text-gray-500">{fmtTime(b.timestamp)}</td>
                    <td className="px-5 py-4">{b.transactions}</td>
                    <td className="px-5 py-4 text-right font-mono text-[#ff6b35]">{fmtAddr(b.miner)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        {/* Latest Transactions */}
        <section className="bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden">
          <div className="p-5 border-b border-gray-100 flex justify-between items-center">
            <h2 className="font-bold text-gray-800">Latest Transactions</h2>
            <Link href="/transactions" className="text-xs font-bold text-[#ff6b35] hover:underline uppercase">View All</Link>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead className="bg-gray-50 text-gray-500 uppercase text-[10px] font-bold">
                <tr>
                  <th className="px-5 py-3">Hash</th>
                  <th className="px-5 py-3">From / To</th>
                  <th className="px-5 py-3 text-right">Value</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-100">
                {loading ? (
                  <tr><td colSpan={3} className="p-5 text-center text-gray-400">Loading transactions...</td></tr>
                ) : txs.length === 0 ? (
                  <tr><td colSpan={3} className="p-5 text-center text-gray-400">No recent transactions found</td></tr>
                ) : txs.map(tx => (
                  <tr key={tx.hash} className="hover:bg-gray-50 transition">
                    <td className="px-5 py-4">
                      <Link href={`/tx/${tx.hash}`} className="text-[#ff6b35] font-mono block truncate w-32">{fmtAddr(tx.hash)}</Link>
                    </td>
                    <td className="px-5 py-4 space-y-1">
                      <p className="text-[11px] text-gray-400">From <span className="text-[#ff6b35] font-mono">{fmtAddr(tx.from)}</span></p>
                      <p className="text-[11px] text-gray-400">To <span className="text-[#ff6b35] font-mono">{fmtAddr(tx.to)}</span></p>
                    </td>
                    <td className="px-5 py-4 text-right font-bold text-gray-700">{fmtVal(tx.value)} TGR</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      </main>

      {/* Footer */}
      <footer className="bg-white border-t border-gray-100 py-12 mt-12">
        <div className="max-w-7xl mx-auto px-6 flex flex-col md:flex-row justify-between items-center gap-6">
          <div className="text-[#ff6b35] text-xl font-bold italic">🐯 TigerScan.io</div>
          <div className="text-gray-400 text-sm">
            © 2024 TigerSmartChain Explorer | Chain ID: 9001 | Powered by Tiger Engine
          </div>
          <div className="flex gap-4">
            <Link href="/docs" className="text-gray-500 hover:text-[#ff6b35] transition text-sm">API Docs</Link>
            <Link href="/terms" className="text-gray-500 hover:text-[#ff6b35] transition text-sm">Terms</Link>
            <Link href="/privacy" className="text-gray-500 hover:text-[#ff6b35] transition text-sm">Privacy</Link>
          </div>
        </div>
      </footer>
    </div>
  )
}
