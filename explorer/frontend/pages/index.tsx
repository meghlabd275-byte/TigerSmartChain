// TigerScan - Blockchain Explorer for TigerSmartChain
// Next.js frontend page

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
      const [b, t, s] = await Promise.all([
        fetch('/api/v1/blocks?limit=10').then(r => r.json()),
        fetch('/api/v1/transactions?limit=10').then(r => r.json()),
        fetch('/api/v1/analytics/stats').then(r => r.json())
      ])
      setBlocks(b.slice(0, 10) || [])
      setTxs(t.slice(0, 10) || [])
      setStats(s)
    } catch (e) { console.error(e) }
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
    <div style={styles.container}>
      <Head>
        <title>TigerScan.io - TigerSmartChain Explorer</title>
        <meta name="description" content="TigerScan.io - Blockchain Explorer for TigerSmartChain" />
      </Head>

      {/* Header */}
      <header style={styles.header}>
        <div style={styles.headerContent}>
          <Link href="/" style={styles.logo}>
            <span>🐯 TigerScan.io</span>
          </Link>
          <nav style={styles.nav}>
            <Link href="/blocks">Blocks</Link>
            <Link href="/transactions">Transactions</Link>
            <Link href="/tokens">Tokens</Link>
            <Link href="/validators">Validators</Link>
            <Link href="/nfts">NFTs</Link>
          </nav>
        </div>
      </header>

      {/* Search */}
      <section style={styles.searchSection}>
        <form onSubmit={handleSearch} style={styles.searchForm}>
          <input
            type="text"
            value={search}
            onChange={e => setSearch(e.target.value)}
            placeholder="Search by Address, Transaction Hash, Block Number"
            style={styles.searchInput}
          />
          <button type="submit" style={styles.searchButton}>Search</button>
        </form>
      </section>

      {/* Stats */}
      <section style={styles.statsSection}>
        <div style={styles.statsGrid}>
          <div style={styles.statCard}>
            <div style={styles.statLabel}>Total Blocks</div>
            <div style={styles.statValue}>{stats.totalBlocks}</div>
          </div>
          <div style={styles.statCard}>
            <div style={styles.statLabel}>Total Transactions</div>
            <div style={styles.statValue}>{stats.totalTransactions}</div>
          </div>
          <div style={styles.statCard}>
            <div style={styles.statLabel}>TPS</div>
            <div style={styles.statValue}>{stats.tps}</div>
          </div>
          <div style={styles.statCard}>
            <div style={styles.statLabel}>Avg Gas</div>
            <div style={styles.statValue}>{(stats.avgGasPrice / 1e9).toFixed(2)} Gwei</div>
          </div>
        </div>
      </section>

      {/* Latest Blocks */}
      <section style={styles.section}>
        <h2 style={styles.sectionTitle}>Latest Blocks</h2>
        <table style={styles.table}>
          <thead>
            <tr>
              <th>Block</th><th>Hash</th><th>Time</th><th>Txns</th><th>Gas</th><th>Miner</th>
            </tr>
          </thead>
          <tbody>
            {loading ? <tr><td colSpan={6}>Loading...</td></tr> :
            blocks.map(b => (
              <tr key={b.number}>
                <td><Link href={`/blocks/${b.number}`}>{b.number}</Link></td>
                <td><code>{fmtAddr(b.hash)}</code></td>
                <td>{fmtTime(b.timestamp)}</td>
                <td>{b.transactions}</td>
                <td>{(b.gasUsed / 1e6).toFixed(1)}M</td>
                <td><code>{fmtAddr(b.miner)}</code></td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      {/* Latest Transactions */}
      <section style={styles.section}>
        <h2 style={styles.sectionTitle}>Latest Transactions</h2>
        <table style={styles.table}>
          <thead>
            <tr>
              <th>Hash</th><th>From</th><th>To</th><th>Value</th><th>Gas</th><th>Status</th>
            </tr>
          </thead>
          <tbody>
            {loading ? <tr><td colSpan={6}>Loading...</td></tr> :
            txs.map(tx => (
              <tr key={tx.hash}>
                <td><code>{fmtAddr(tx.hash)}</code></td>
                <td><code>{fmtAddr(tx.from)}</code></td>
                <td><code>{fmtAddr(tx.to)}</code></td>
                <td>{fmtVal(tx.value)} TGR</td>
                <td>{(parseFloat(tx.gasPrice || '0') / 1e9).toFixed(2)} Gwei</td>
                <td style={tx.status ? styles.statusSuccess : styles.statusFailed}>
                  {tx.status ? '✓' : '✗'}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      {/* Footer */}
      <footer style={styles.footer}>
        <p>TigerScan.io © 2024 - TigerSmartChain Explorer</p>
        <p>Chain ID: 9001 | Token: TGR (Tiger Coin)</p>
      </footer>
    </div>
  )
}

const styles = {
  container: { fontFamily: 'system-ui, sans-serif', minHeight: '100vh' },
  header: { background: '#1a1a2e', padding: '1rem' },
  headerContent: { maxWidth: '1200px', margin: '0 auto', display: 'flex', justifyContent: 'space-between', alignItems: 'center' },
  logo: { color: '#ff6b35', fontSize: '1.5rem', fontWeight: 'bold', textDecoration: 'none' },
  nav: { display: 'flex', gap: '1.5rem' },
  searchSection: { background: 'linear-gradient(135deg, #1a1a2e 0%, #16213e 100%)', padding: '3rem 1rem', textAlign: 'center' },
  searchForm: { maxWidth: '600px', margin: '0 auto', display: 'flex', gap: '0.5rem' },
  searchInput: { flex: 1, padding: '1rem', fontSize: '1rem', borderRadius: '8px', border: 'none' },
  searchButton: { padding: '1rem 2rem', fontSize: '1rem', background: '#ff6b35', color: 'white', border: 'none', borderRadius: '8px', cursor: 'pointer' },
  statsSection: { maxWidth: '1200px', margin: '2rem auto', padding: '0 1rem' },
  statsGrid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: '1rem' },
  statCard: { background: 'white', padding: '1.5rem', borderRadius: '12px', boxShadow: '0 2px 8px rgba(0,0,0,0.1)' },
  statLabel: { color: '#666', fontSize: '0.875rem', marginBottom: '0.5rem' },
  statValue: { fontSize: '1.5rem', fontWeight: 'bold', color: '#1a1a2e' },
  section: { maxWidth: '1200px', margin: '2rem auto', padding: '0 1rem' },
  sectionTitle: { fontSize: '1.25rem', marginBottom: '1rem', color: '#1a1a2e' },
  table: { width: '100%', background: 'white', borderRadius: '12px', overflow: 'hidden', borderCollapse: 'collapse' },
  statusSuccess: { color: 'green' },
  statusFailed: { color: 'red' },
  footer: { textAlign: 'center', padding: '2rem', color: '#666', marginTop: '3rem' }
}