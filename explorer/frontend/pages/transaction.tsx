// TigerScan - Transaction Detail Page
// Production-ready transaction explorer with full details, logs, and state changes

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'
import { useRouter } from 'next/router'

interface Transaction {
  hash: string
  blockNumber: number
  blockHash: string
  from: string
  to: string
  value: string
  gasPrice: number
  gasLimit: number
  gasUsed: number
  nonce: number
  input: string
  signature: string
  v: number
  r: string
  s: string
  status: boolean
  txFee: string
  timestamp: number
  logs: Log[]
  internalTransactions: InternalTx[]
}

interface Log {
  address: string
  topics: string[]
  data: string
  logIndex: number
}

interface InternalTx {
  from: string
  to: string
  value: string
  callType: string
  depth: number
}

export default function TransactionPage({ tx: initialTx }: { tx: Transaction | null }) {
  const router = useRouter()
  const [tx, setTx] = useState<Transaction | null>(initialTx)
  const [loading, setLoading] = useState(!initialTx)
  const [error, setError] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<'overview' | 'logs' | 'internal'>('overview')

  useEffect(() => {
    if (!initialTx && router.query.hash) {
      fetchTransaction()
    }
  }, [router.query.hash])

  async function fetchTransaction() {
    setLoading(true)
    setError(null)
    
    try {
      const res = await fetch(`/api/v1/transactions/${router.query.hash}`)
      if (!res.ok) throw new Error('Transaction not found')
      const data = await res.json()
      setTx(data)
    } catch (err: any) {
      setError(err.message || 'Failed to load transaction')
    } finally {
      setLoading(false)
    }
  }

  function formatAddress(addr: string): string {
    if (!addr) return ''
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`
  }

  function formatValue(wei: string | number): string {
    const num = typeof wei === 'string' ? parseFloat(wei) : wei
    const eth = num / 1e18
    if (eth < 0.0001) return `${(num / 1e9).toFixed(2)} Gwei`
    return eth.toFixed(6)
  }

  function formatTime(timestamp: number): string {
    return new Date(timestamp * 1000).toLocaleString()
  }

  function formatData(data: string): string {
    if (!data || data === '0x') return '(None)'
    if (data.length > 64) return `${data.slice(0, 32)}...${data.slice(-32)}`
    return data
  }

  if (loading) return <div style={styles.container}><div style={styles.loading}>Loading transaction...</div></div>
  if (error || !tx) return <div style={styles.container}><div style={styles.error}><h2>Transaction Not Found</h2><p>{error}</p><Link href="/">Go Home</Link></div></div>

  return (
    <div style={styles.container}>
      <Head>
        <title>Tx {tx.hash.slice(0, 10)}... | TigerScan.io</title>
      </Head>

      {/* Header */}
      <header style={styles.header}>
        <div style={styles.headerContent}>
          <Link href="/" style={styles.logo}>🐯 TigerScan.io</Link>
          <nav style={styles.nav}>
            <Link href="/blocks">Blocks</Link>
            <Link href="/transactions">Transactions</Link>
            <Link href="/tokens">Tokens</Link>
          </nav>
        </div>
      </header>

      {/* Breadcrumb */}
      <div style={styles.breadcrumb}>
        <Link href="/">Home</Link> / <Link href="/transactions">Transactions</Link> / {tx.hash.slice(0, 10)}...
      </div>

      {/* Title */}
      <div style={styles.titleSection}>
        <h1 style={styles.title}>
          <span style={tx.status ? styles.successBadge : styles.failureBadge}>
            {tx.status ? '✓' : '✗'}
          </span>
          Transaction {tx.status ? 'Confirmed' : 'Failed'}
        </h1>
        <code style={styles.hash}>{tx.hash}</code>
      </div>

      {/* Tabs */}
      <div style={styles.tabs}>
        <button style={activeTab === 'overview' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('overview')}>
          Overview
        </button>
        <button style={activeTab === 'logs' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('logs')}>
          Logs {tx.logs?.length ? `(${tx.logs.length})` : ''}
        </button>
        <button style={activeTab === 'internal' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('internal')}>
          Internal {tx.internalTransactions?.length ? `(${tx.internalTransactions.length})` : ''}
        </button>
      </div>

      {/* Content */}
      {activeTab === 'overview' && (
        <section style={styles.section}>
          <div style={styles.grid}>
            <div style={styles.card}>
              <label style={styles.label}>Transaction Hash</label>
              <code style={styles.value}>{tx.hash}</code>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>Block</label>
              <Link href={`/blocks/${tx.blockNumber}`} style={styles.link}>#{tx.blockNumber}</Link>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>Timestamp</label>
              <span style={styles.value}>{tx.timestamp ? formatTime(tx.timestamp) : 'Pending'}</span>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>From</label>
              <Link href={`/address/${tx.from}`} style={styles.address}>{formatAddress(tx.from)}</Link>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>To</label>
              <Link href={`/address/${tx.to}`} style={styles.address}>{formatAddress(tx.to)}</Link>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>Value</label>
              <span style={styles.value}>{formatValue(tx.value)} TGR</span>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>Gas Price</label>
              <span style={styles.value}>{formatValue(tx.gasPrice)} Gwei</span>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>Gas Limit / Used</label>
              <span style={styles.value}>{tx.gasUsed.toLocaleString()} / {tx.gasLimit.toLocaleString()}</span>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>Transaction Fee</label>
              <span style={styles.value}>{formatValue(tx.txFee)} TGR</span>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>Nonce</label>
              <span style={styles.value}>{tx.nonce}</span>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>Input Data</label>
              <code style={styles.code}>{formatData(tx.input)}</code>
            </div>
          </div>
        </section>
      )}

      {activeTab === 'logs' && (
        <section style={styles.section}>
          {tx.logs?.length === 0 ? (
            <p style={styles.empty}>No event logs</p>
          ) : (
            <table style={styles.table}>
              <thead>
                <tr>
                  <th style={styles.th}>Address</th>
                  <th style={styles.th}>Topics</th>
                  <th style={styles.th}>Data</th>
                </tr>
              </thead>
              <tbody>
                {tx.logs?.map((log, i) => (
                  <tr key={i}>
                    <td style={styles.td}><Link href={`/address/${log.address}`} style={styles.address}>{formatAddress(log.address)}</Link></td>
                    <td style={styles.td}>{log.topics.map(t => formatAddress(t)).join('\n')}</td>
                    <td style={styles.td}><code style={styles.code}>{formatData(log.data)}</code></td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      )}

      {activeTab === 'internal' && (
        <section style={styles.section}>
          {tx.internalTransactions?.length === 0 ? (
            <p style={styles.empty}>No internal transactions</p>
          ) : (
            <table style={styles.table}>
              <thead>
                <tr>
                  <th style={styles.th}>Depth</th>
                  <th style={styles.th}>From</th>
                  <th style={styles.th}>To</th>
                  <th style={styles.th}>Value</th>
                  <th style={styles.th}>Type</th>
                </tr>
              </thead>
              <tbody>
                {tx.internalTransactions?.map((itx, i) => (
                  <tr key={i}>
                    <td style={styles.td}>{itx.depth}</td>
                    <td style={styles.td}><Link href={`/address/${itx.from}`} style={styles.address}>{formatAddress(itx.from)}</Link></td>
                    <td style={styles.td}><Link href={`/address/${itx.to}`} style={styles.address}>{formatAddress(itx.to)}</Link></td>
                    <td style={styles.td}>{formatValue(itx.value)}</td>
                    <td style={styles.td}>{itx.callType}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      )}

      <footer style={styles.footer}>
        <p>TigerScan.io © 2024 - TigerSmartChain Explorer</p>
      </footer>
    </div>
  )
}

const styles = {
  container: { fontFamily: 'system-ui, sans-serif', minHeight: '100vh', background: '#f5f6fa' },
  header: { background: '#1a1a2e', padding: '1rem' },
  headerContent: { maxWidth: '1200px', margin: '0 auto', display: 'flex', justifyContent: 'space-between', alignItems: 'center' },
  logo: { color: '#ff6b35', fontSize: '1.5rem', fontWeight: 'bold', textDecoration: 'none' },
  nav: { display: 'flex', gap: '1.5rem' },
  breadcrumb: { maxWidth: '1200px', margin: '1rem auto', padding: '0 1rem', color: '#666' },
  titleSection: { maxWidth: '1200px', margin: '0 auto 1rem', padding: '0 1rem' },
  title: { display: 'flex', alignItems: 'center', gap: '0.5rem', fontSize: '1.25rem', marginBottom: '0.5rem' },
  successBadge: { display: 'inline-flex', alignItems: 'center', justifyContent: 'center', width: '24px', height: '24px', background: '#22c55e', color: 'white', borderRadius: '50%', fontSize: '0.875rem' },
  failureBadge: { display: 'inline-flex', alignItems: 'center', justifyContent: 'center', width: '24px', height: '24px', background: '#ef4444', color: 'white', borderRadius: '50%', fontSize: '0.875rem' },
  hash: { fontSize: '0.875rem', color: '#666', wordBreak: 'break-all' },
  tabs: { maxWidth: '1200px', margin: '0 auto', padding: '0 1rem', display: 'flex', gap: '0.5rem', borderBottom: '1px solid #e5e5e5' },
  tab: { padding: '0.75rem 1rem', background: 'none', border: 'none', borderBottom: '2px solid transparent', cursor: 'pointer', fontSize: '0.875rem', color: '#666' },
  activeTab: { padding: '0.75rem 1rem', background: 'none', border: 'none', borderBottom: '2px solid #ff6b35', cursor: 'pointer', fontSize: '0.875rem', color: '#ff6b35', fontWeight: '600' },
  section: { maxWidth: '1200px', margin: '1rem auto', padding: '0 1rem' },
  grid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: '1rem' },
  card: { background: 'white', padding: '1rem', borderRadius: '8px', boxShadow: '0 1px 3px rgba(0,0,0,0.1)' },
  label: { display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: '0.25rem', textTransform: 'uppercase' },
  value: { fontSize: '0.875rem', color: '#1a1a2e', wordBreak: 'break-all' },
  link: { color: '#ff6b35', textDecoration: 'none' },
  address: { color: '#ff6b35', textDecoration: 'none' },
  code: { fontSize: '0.75rem', fontFamily: 'monospace', wordBreak: 'break-all' },
  table: { width: '100%', background: 'white', borderRadius: '8px', overflow: 'hidden', borderCollapse: 'collapse' },
  th: { padding: '0.75rem', textAlign: 'left', background: '#f5f6fa', fontSize: '0.75rem', color: '#666', fontWeight: '600' },
  td: { padding: '0.75rem', borderTop: '1px solid #eee', fontSize: '0.875rem' },
  loading: { maxWidth: '1200px', margin: '2rem auto', padding: '2rem', textAlign: 'center', background: 'white', borderRadius: '8px' },
  error: { maxWidth: '1200px', margin: '2rem auto', padding: '2rem', textAlign: 'center', background: 'white', borderRadius: '8px' },
  empty: { padding: '2rem', textAlign: 'center', color: '#666', background: 'white', borderRadius: '8px' },
  footer: { textAlign: 'center', padding: '2rem', color: '#666', marginTop: '3rem' }
}