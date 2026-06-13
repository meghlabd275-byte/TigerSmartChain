// TigerScan - Transaction Trace Debugger Page
// State diff, call stack, gas profiler visualization

import { useState } from 'react'
import Head from 'next/head'
import Link from 'next/link'

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

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setLoading(true)
    setError('')
    setResult(null)

    try {
      const response = await fetch('https://api.tigerscan.io/v1/trace', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          tx_hash: txHash,
          enable_state_diff: true,
          enable_gas_profiling: true
        })
      })

      const data = await response.json()
      
      if (data.result) {
        setResult(data.result)
      } else {
        setError(data.error || 'Trace failed')
      }
    } catch (err) {
      setError('Network error')
    } finally {
      setLoading(false)
    }
  }

  const formatAddress = (addr: string) => {
    if (!addr || addr.length < 16) return addr
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`
  }

  return (
    <div style={styles.container}>
      <Head>
        <title>Transaction Trace - TigerScan.io</title>
      </Head>

      <header style={styles.header}>
        <Link href="/" style={styles.logo}>🐯 TigerScan</Link>
        <nav style={styles.nav}>
          <Link href="/blocks">Blocks</Link>
          <Link href="/transactions">Transactions</Link>
          <Link href="/verify">Verify</Link>
        </nav>
      </header>

      <main style={styles.main}>
        <h1 style={styles.title}>Transaction Trace Debugger</h1>
        
        <form onSubmit={handleSubmit} style={styles.form}>
          <div style={styles.field}>
            <label style={styles.label}>Transaction Hash</label>
            <input
              type="text"
              value={txHash}
              onChange={(e) => setTxHash(e.target.value)}
              placeholder="0x..."
              style={styles.input}
              required
            />
          </div>
          <button type="submit" disabled={loading} style={styles.submit}>
            {loading ? 'Tracing...' : 'Analyze Transaction'}
          </button>
        </form>

        {error && <div style={styles.error}>{error}</div>}

        {result && (
          <div style={styles.result}>
            {/* Transaction Info */}
            <div style={styles.txInfo}>
              <h3>Transaction Details</h3>
              <div style={styles.infoGrid}>
                <div><strong>Hash:</strong> <code>{result.transaction_hash}</code></div>
                <div><strong>Block:</strong> {result.block_number}</div>
                <div><strong>From:</strong> <code>{formatAddress(result.from)}</code></div>
                <div><strong>To:</strong> <code>{formatAddress(result.to)}</code></div>
                <div><strong>Value:</strong> {parseFloat(result.value) / 1e18} TGR</div>
                <div><strong>Gas Used:</strong> {result.gas_used.toLocaleString()}</div>
                <div><strong>Status:</strong> <span style={result.status ? styles.success : styles.failure}>{result.status ? 'Success' : 'Failed'}</span></div>
              </div>
            </div>

            {/* Tabs */}
            <div style={styles.tabs}>
              <button onClick={() => setActiveTab('calls')} style={activeTab === 'calls' ? styles.activeTab : styles.tab}>Call Tree</button>
              <button onClick={() => setActiveTab('state')} style={activeTab === 'state' ? styles.activeTab : styles.tab}>State Diff</button>
              <button onClick={() => setActiveTab('gas')} style={activeTab === 'gas' ? styles.activeTab : styles.tab}>Gas Profile</button>
            </div>

            {/* Call Tree */}
            {activeTab === 'calls' && (
              <div style={styles.callsTree}>
                <h4>Call Tree</h4>
                {result.traces.map((call, i) => (
                  <div key={i} style={{...styles.callItem, paddingLeft: `${call.depth * 20}px`}}>
                    <span style={styles.callType}>{call.call_type}</span>
                    <span style={styles.callFrom}>{formatAddress(call.from)}</span>
                    <span style={styles.callArrow}>→</span>
                    <span style={styles.callTo}>{formatAddress(call.to)}</span>
                    <span style={styles.callGas}>{call.gas_used.toLocaleString()}</span>
                    {call.revert && <span style={styles.revert}>REVERT</span>}
                    {call.error && <span style={styles.errorText}>{call.error}</span>}
                  </div>
                ))}
              </div>
            )}

            {/* State Diff */}
            {activeTab === 'state' && result.state_diff && (
              <div style={styles.stateDiff}>
                <h4>Storage Changes</h4>
                {result.state_diff.changes.length === 0 ? (
                  <p>No storage changes</p>
                ) : (
                  result.state_diff.changes.map((change, i) => (
                    <div key={i} style={styles.diffItem}>
                      <div style={styles.diffAddr}>{change.address}</div>
                      <div style={styles.diffSlot}>Slot: {change.slot}</div>
                      <div style={styles.diffPre}>- {change.pre_value}</div>
                      <div style={styles.diffPost}>+ {change.post_value}</div>
                    </div>
                  ))
                )}
              </div>
            )}

            {/* Gas Profile */}
            {activeTab === 'gas' && result.gas_profiling && (
              <div style={styles.gasProfile}>
                <h4>Gas Usage Analysis</h4>
                <div style={styles.totalGas}>Total Gas: {result.gas_profiling.total_gas.toLocaleString()}</div>
                
                <h5>Gas by Call</h5>
                {result.gas_profiling.gas_per_call.slice(0, 10).map((call, i) => (
                  <div key={i} style={styles.gasItem}>
                    <span>#{call.call_index} {call.call_type}</span>
                    <span>{call.gas_used.toLocaleString()}</span>
                    <span>{call.percentage.toFixed(1)}%</span>
                    <div style={{...styles.gasBar, width: `${call.percentage}%`}} />
                  </div>
                ))}

                {result.gas_profiling.optimization_suggestions.length > 0 && (
                  <>
                    <h5>Optimization Suggestions</h5>
                    {result.gas_profiling.optimization_suggestions.map((s, i) => (
                      <div key={i} style={styles.suggestion}>
                        <strong>Call #{s.call_index}:</strong> {s.suggestion}
                        <span>Savings: ~{s.estimated_savings.toLocaleString()} gas</span>
                      </div>
                    ))}
                  </>
                )}
              </div>
            )}
          </div>
        )}
      </main>
    </div>
  )
}

const styles: Record<string, React.CSSProperties> = {
  container: { fontFamily: 'system-ui, sans-serif', minHeight: '100vh', background: '#0a0a0f' },
  header: { display: 'flex', justifyContent: 'space-between', padding: '1rem 2rem', background: '#12121a', borderBottom: '1px solid #2a2a3a' },
  logo: { color: '#ff6b35', fontSize: '1.5rem', fontWeight: 'bold', textDecoration: 'none' },
  nav: { display: 'flex', gap: '1.5rem' },
  main: { maxWidth: '1200px', margin: '0 auto', padding: '2rem' },
  title: { color: '#fff', fontSize: '2rem', marginBottom: '1.5rem' },
  form: { background: '#12121a', padding: '2rem', borderRadius: '12px', display: 'flex', gap: '1rem', alignItems: 'flex-end' },
  field: { flex: 1 },
  label: { display: 'block', color: '#aaa', marginBottom: '0.5rem' },
  input: { width: '100%', padding: '0.75rem', borderRadius: '8px', border: '1px solid #333', background: '#1a1a24', color: '#fff', fontSize: '1rem' },
  submit: { padding: '0.75rem 2rem', borderRadius: '8px', border: 'none', background: 'linear-gradient(135deg, #ff6b35, #ff8f5a)', color: '#fff', fontWeight: 'bold', cursor: 'pointer' },
  error: { marginTop: '1rem', padding: '1rem', background: '#3a1a1a', color: '#ff6b6b', borderRadius: '8px' },
  result: { marginTop: '2rem', background: '#12121a', borderRadius: '12px', padding: '1.5rem' },
  txInfo: { marginBottom: '1.5rem' },
  infoGrid: { display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '1rem', marginTop: '1rem', fontSize: '0.9rem' },
  success: { color: '#6bff6b' },
  failure: { color: '#ff6b6b' },
  tabs: { display: 'flex', gap: '0.5rem', marginBottom: '1rem' },
  tab: { padding: '0.5rem 1rem', border: 'none', background: '#1a1a24', color: '#888', borderRadius: '6px', cursor: 'pointer' },
  activeTab: { padding: '0.5rem 1rem', border: 'none', background: '#ff6b35', color: '#fff', borderRadius: '6px', cursor: 'pointer' },
  callsTree: { background: '#0d0d14', padding: '1rem', borderRadius: '8px', fontFamily: 'monospace', fontSize: '0.85rem' },
  callItem: { display: 'flex', gap: '0.75rem', alignItems: 'center', padding: '0.5rem', borderBottom: '1px solid #222' },
  callType: { background: '#2a2a4a', padding: '2px 8px', borderRadius: '4px', fontSize: '0.75rem', color: '#88f' },
  callFrom: { color: '#f88' },
  callArrow: { color: '#888' },
  callTo: { color: '#8f8' },
  callGas: { marginLeft: 'auto', color: '#888' },
  revert: { background: '#ff3333', color: '#fff', padding: '2px 6px', borderRadius: '4px', fontSize: '0.7rem' },
  errorText: { color: '#ff6b6b', fontSize: '0.8rem' },
  stateDiff: { background: '#0d0d14', padding: '1rem', borderRadius: '8px' },
  diffItem: { padding: '0.75rem', borderBottom: '1px solid #222' },
  diffAddr: { color: '#ff6b35', fontSize: '0.9rem' },
  diffSlot: { color: '#888', fontSize: '0.8rem' },
  diffPre: { color: '#ff6b6b', fontFamily: 'monospace', fontSize: '0.85rem', marginTop: '0.25rem' },
  diffPost: { color: '#6bff6b', fontFamily: 'monospace', fontSize: '0.85rem' },
  gasProfile: { background: '#0d0d14', padding: '1rem', borderRadius: '8px' },
  totalGas: { fontSize: '1.2rem', fontWeight: 'bold', color: '#ff6b35', marginBottom: '1rem' },
  gasItem: { display: 'grid', gridTemplateColumns: '1fr 100px 60px', gap: '1rem', padding: '0.5rem', alignItems: 'center', position: 'relative' },
  gasBar: { position: 'absolute', bottom: 0, left: 0, height: '2px', background: '#ff6b35', opacity: 0.3 },
  suggestion: { padding: '0.75rem', background: '#1a2a1a', borderRadius: '6px', marginTop: '0.5rem', fontSize: '0.9rem', display: 'flex', justifyContent: 'space-between' }
}