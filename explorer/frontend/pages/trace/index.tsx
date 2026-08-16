// TigerScan - Transaction Trace Debugger Page
// State diff, call stack, gas profiler visualization with Tailwind CSS

import { useState } from 'react'
import Head from 'next/head'
import Link from 'next/link'

const API_BASE = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:12000'

interface CallTrace {
  call_type: string
  from: string
  to: string
  value: string
  gas: number
  gas_used: number
  depth: number
  index: number
  parent_index: number | null
  revert: boolean
  error?: string
}

interface StateDiff {
  changes: Array<{
    address: string
    slot: string
    pre_value: string
    post_value: string
    diff_type: string
  }>
}

interface GasProfile {
  total_gas: number
  gas_per_call: Array<{
    call_index: number
    call_type: string
    gas_used: number
    percentage: number
  }>
  optimization_suggestions: Array<{
    call_index: number
    suggestion: string
    estimated_savings: number
  }>
}

interface TraceResult {
  transaction_hash: string
  block_number: number
  from: string
  to: string
  value: string
  gas_used: number
  status: boolean
  traces: CallTrace[]
  state_diff?: StateDiff
  gas_profiling?: GasProfile
}

export default function TraceDebugger() {
  const [txHash, setTxHash] = useState('')
  const [loading, setLoading] = useState(false)
  const [result, setResult] = useState<TraceResult | null>(null)
  const [error, setError] = useState('')
  const [activeTab, setActiveTab] = useState<'calls' | 'state' | 'gas'>('calls')

  const fetchTrace = async () => {
    setLoading(true)
    setError('')
    setResult(null)

    try {
      const response = await fetch(`${API_BASE}/api/v1/trace/${encodeURIComponent(txHash)}`)

      if (!response.ok) {
        const errBody = await response.json().catch(() => null)
        throw new Error(errBody?.error || `Trace request failed (${response.status})`)
      }

      const data = await response.json()
      const result = data.result ?? data

      if (!result || !result.traces) {
        setError(data.error || 'No trace data available for this transaction.')
      } else {
        setResult(result)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Network error')
    } finally {
      setLoading(false)
    }
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    fetchTrace()
  }

  const formatAddress = (addr: string) => {
    if (!addr || addr.length < 16) return addr
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`
  }

  return (
    <div className="min-h-screen bg-[#0a0a0f] text-gray-200 font-sans">
      <Head>
        <title>Transaction Trace - TigerScan.io</title>
      </Head>

      <header className="bg-[#12121a] border-b border-[#2a2a3a] py-4 px-8 flex justify-between items-center sticky top-0 z-50">
        <Link href="/" className="text-[#ff6b35] text-2xl font-bold">🐯 TigerScan</Link>
        <nav className="flex gap-6 text-sm font-medium text-gray-400">
          <Link href="/blocks" className="hover:text-white transition">Blocks</Link>
          <Link href="/transactions" className="hover:text-white transition">Transactions</Link>
          <Link href="/verify" className="hover:text-white transition">Verify</Link>
        </nav>
      </header>

      <main className="max-w-6xl mx-auto py-12 px-6">
        <h1 className="text-3xl font-bold text-white mb-2">Advanced Transaction Trace</h1>
        <p className="text-gray-500 mb-8">Deep inspection of internal calls, state changes, and gas profile.</p>
        
        <form onSubmit={handleSubmit} className="bg-[#12121a] p-8 rounded-2xl shadow-xl border border-[#1f1f2e] flex flex-col md:flex-row gap-4 items-end">
          <div className="flex-1 w-full">
            <label className="block text-gray-400 text-xs font-bold uppercase tracking-widest mb-2">Transaction Hash</label>
            <input
              type="text"
              value={txHash}
              onChange={(e) => setTxHash(e.target.value)}
              placeholder="0x..."
              className="w-full bg-[#1a1a24] border border-[#2a2a3a] rounded-xl px-4 py-3 text-white focus:outline-none focus:ring-2 focus:ring-[#ff6b35] transition"
              required
            />
          </div>
          <button type="submit" disabled={loading} className="w-full md:w-auto bg-gradient-to-r from-[#ff6b35] to-[#ff8f5a] hover:from-[#ff8f5a] hover:to-[#ff6b35] text-white font-bold px-10 py-3 rounded-xl shadow-lg shadow-[#ff6b35]/20 disabled:opacity-50 transition duration-300">
            {loading ? 'Analyzing...' : 'Analyze Trace'}
          </button>
        </form>

        {loading && (
          <div className="mt-6 flex items-center justify-center py-12">
            <div className="animate-spin rounded-full h-10 w-10 border-b-2 border-[#ff6b35]"></div>
          </div>
        )}

        {error && (
          <div className="mt-6 p-4 bg-red-900/20 border border-red-900/50 text-red-400 rounded-xl text-sm flex items-center justify-between">
            <span>{error}</span>
            <button
              onClick={fetchTrace}
              className="ml-4 px-3 py-1 bg-red-900/40 hover:bg-red-900/60 text-red-300 rounded-lg text-xs"
            >
              Retry
            </button>
          </div>
        )}

        {!loading && !error && !result && txHash && (
          <div className="mt-6 p-4 bg-[#12121a] border border-[#1f1f2e] text-gray-500 rounded-xl text-sm text-center">
            No data available
          </div>
        )}

        {result && (
          <div className="mt-12 space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
            {/* Transaction Stats Header */}
            <div className="bg-[#12121a] rounded-2xl p-8 border border-[#1f1f2e]">
              <div className="flex justify-between items-start mb-8">
                <div>
                  <h3 className="text-gray-500 text-xs font-bold uppercase tracking-widest mb-1">Transaction Identity</h3>
                  <code className="text-white text-lg break-all">{result.transaction_hash}</code>
                </div>
                <div className={`px-4 py-1 rounded-full text-xs font-bold ${result.status ? 'bg-green-900/20 text-green-400 border border-green-900/50' : 'bg-red-900/20 text-red-400 border border-red-900/50'}`}>
                    {result.status ? 'SUCCESS' : 'FAILED'}
                </div>
              </div>

              <div className="grid grid-cols-2 md:grid-cols-4 gap-8">
                <div>
                  <p className="text-gray-500 text-xs font-bold uppercase mb-1">Block</p>
                  <p className="text-white font-bold">{result.block_number}</p>
                </div>
                <div>
                  <p className="text-gray-500 text-xs font-bold uppercase mb-1">Gas Used</p>
                  <p className="text-white font-bold">{result.gas_used.toLocaleString()}</p>
                </div>
                <div>
                  <p className="text-gray-500 text-xs font-bold uppercase mb-1">From</p>
                  <code className="text-[#ff6b35] text-sm">{formatAddress(result.from)}</code>
                </div>
                <div>
                  <p className="text-gray-500 text-xs font-bold uppercase mb-1">To</p>
                  <code className="text-[#ff6b35] text-sm">{formatAddress(result.to)}</code>
                </div>
              </div>
            </div>

            {/* Navigation Tabs */}
            <div className="flex border-b border-[#2a2a3a]">
              {[
                { id: 'calls', label: 'Internal Calls' },
                { id: 'state', label: 'State Diffs' },
                { id: 'gas', label: 'Gas Profiling' }
              ].map((tab) => (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id as any)}
                  className={`px-8 py-4 text-sm font-bold transition-all duration-200 border-b-2 ${
                    activeTab === tab.id
                    ? 'border-[#ff6b35] text-[#ff6b35]'
                    : 'border-transparent text-gray-500 hover:text-gray-300'
                  }`}
                >
                  {tab.label}
                </button>
              ))}
            </div>

            {/* Tab Content: Calls */}
            {activeTab === 'calls' && (
              <div className="bg-[#0d0d14] rounded-2xl overflow-hidden border border-[#1f1f2e]">
                <div className="overflow-x-auto">
                  <table className="w-full text-left text-sm">
                    <thead className="bg-[#12121a] text-gray-500 uppercase text-[10px] font-bold">
                      <tr>
                        <th className="px-6 py-4">Type</th>
                        <th className="px-6 py-4">Call Detail</th>
                        <th className="px-6 py-4 text-right">Gas Consumed</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-[#1f1f2e] font-mono">
                      {result.traces.map((call, i) => (
                        <tr key={i} className="hover:bg-white/[0.02] transition">
                          <td className="px-6 py-4">
                            <span className={`px-2 py-0.5 rounded text-[10px] font-bold ${
                              call.call_type === 'CALL' ? 'bg-blue-900/20 text-blue-400' :
                              call.call_type === 'STATICCALL' ? 'bg-purple-900/20 text-purple-400' :
                              'bg-orange-900/20 text-orange-400'
                            }`}>
                              {call.call_type}
                            </span>
                          </td>
                          <td className="px-6 py-4" style={{ paddingLeft: `${call.depth * 24 + 24}px` }}>
                            <div className="flex items-center gap-2">
                                <span className="text-gray-500">{formatAddress(call.from)}</span>
                                <span className="text-[#ff6b35] opacity-50">→</span>
                                <span className="text-white font-bold">{formatAddress(call.to)}</span>
                                {call.revert && <span className="ml-2 bg-red-900/40 text-red-500 text-[10px] px-1 rounded">REVERTED</span>}
                            </div>
                          </td>
                          <td className="px-6 py-4 text-right text-gray-400">
                            {call.gas_used.toLocaleString()}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}

            {/* Tab Content: State */}
            {activeTab === 'state' && (
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {result.state_diff?.changes.map((change, i) => (
                  <div key={i} className="bg-[#12121a] p-6 rounded-2xl border border-[#1f1f2e] space-y-4">
                    <div className="flex justify-between items-center">
                      <span className="text-[#ff6b35] font-bold text-xs">{formatAddress(change.address)}</span>
                      <span className="bg-gray-800 text-gray-400 text-[10px] px-2 py-0.5 rounded">SLOT {change.slot}</span>
                    </div>
                    <div className="space-y-2 font-mono text-xs">
                        <div className="flex items-start gap-2 bg-red-900/10 p-2 rounded">
                            <span className="text-red-500">-</span>
                            <span className="text-red-900/80 break-all">{change.pre_value}</span>
                        </div>
                        <div className="flex items-start gap-2 bg-green-900/10 p-2 rounded">
                            <span className="text-green-500">+</span>
                            <span className="text-green-400 break-all">{change.post_value}</span>
                        </div>
                    </div>
                  </div>
                ))}
                {(!result.state_diff || result.state_diff.changes.length === 0) && (
                    <div className="col-span-full py-12 text-center text-gray-500 italic">No state changes detected in this transaction.</div>
                )}
              </div>
            )}

            {/* Tab Content: Gas */}
            {activeTab === 'gas' && result.gas_profiling && (
              <div className="space-y-6">
                <div className="bg-[#12121a] p-8 rounded-2xl border border-[#1f1f2e]">
                    <h4 className="text-white font-bold mb-6 flex items-center gap-2">
                        <span className="text-[#ff6b35]">⚡</span> Gas Consumption Heatmap
                    </h4>
                    <div className="space-y-4">
                        {result.gas_profiling.gas_per_call.map((call, i) => (
                            <div key={i} className="space-y-1">
                                <div className="flex justify-between text-xs mb-1">
                                    <span className="text-gray-400">#{call.call_index} {call.call_type}</span>
                                    <span className="text-white font-bold">{call.percentage}%</span>
                                </div>
                                <div className="w-full bg-[#1a1a24] h-2 rounded-full overflow-hidden">
                                    <div className="bg-[#ff6b35] h-full rounded-full transition-all duration-1000" style={{ width: `${call.percentage}%` }}></div>
                                </div>
                            </div>
                        ))}
                    </div>
                </div>

                {result.gas_profiling.optimization_suggestions.length > 0 && (
                    <div className="bg-[#ff6b35]/10 border border-[#ff6b35]/20 p-8 rounded-2xl">
                        <h4 className="text-[#ff6b35] font-bold mb-4 uppercase tracking-widest text-xs">AI Optimization Insights</h4>
                        <div className="space-y-4">
                            {result.gas_profiling.optimization_suggestions.map((s, i) => (
                                <div key={i} className="flex gap-4 items-start bg-[#12121a]/50 p-4 rounded-xl">
                                    <div className="bg-[#ff6b35] text-white p-2 rounded-lg text-xs font-bold leading-none">#{s.call_index}</div>
                                    <div className="flex-1">
                                        <p className="text-sm text-gray-200">{s.suggestion}</p>
                                        <p className="text-[10px] text-[#ff6b35] font-bold mt-1">ESTIMATED SAVINGS: ~{s.estimated_savings.toLocaleString()} GAS</p>
                                    </div>
                                </div>
                            ))}
                        </div>
                    </div>
                )}
              </div>
            )}
          </div>
        )}
      </main>

      <footer className="py-12 border-t border-[#1f1f2e] text-center text-gray-600 text-sm">
          TigerSmartChain Advanced Trace Debugger &copy; 2024
      </footer>
    </div>
  )
}
