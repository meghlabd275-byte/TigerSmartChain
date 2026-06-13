// TigerScan - Pending Transaction Pool (Mempool) Page

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'

interface PendingTx {
  hash: string
  from: string
  to: string
  value: string
  gas_price: string
  gas_limit: number
  nonce: number
  data: string
  timestamp: number
  tx_type: string
}

interface PoolStats {
  total_pending: number
  avg_gas_price: number
  gas_distribution: Record<string, number>
  top_senders: Array<{ address: string; tx_count: number }>
}

export default function Mempool() {
  const [txs, setTxs] = useState<PendingTx[]>([])
  const [stats, setStats] = useState<PoolStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [sortBy, setSortBy] = useState<'gas_price' | 'nonce' | 'value'>('gas_price')

  useEffect(() => {
    fetchData()
    const interval = setInterval(fetchData, 5000)
    return () => clearInterval(interval)
  }, [])

  const fetchData = async () => {
    try {
      const [txRes, statsRes] = await Promise.all([
        fetch('https://api.tigerscan.io/v1/mempool?limit=50'),
        fetch('https://api.tigerscan.io/v1/mempool/stats')
      ])
      
      const txData = await txRes.json()
      const statsData = await statsRes.json()
      
      setTxs(txData.transactions || [])
      setStats(statsData)
    } catch (err) {
      console.error(err)
    } finally {
      setLoading(false)
    }
  }

  const sortedTxs = [...txs].sort((a, b) => {
    if (sortBy === 'gas_price') {
      return parseFloat(b.gas_price) - parseFloat(a.gas_price)
    }
    if (sortBy === 'nonce') {
      return a.nonce - b.nonce
    }
    return parseFloat(b.value) - parseFloat(a.value)
  })

  const formatAddr = (addr: string) => addr ? `${addr.slice(0, 6)}...${addr.slice(-4)}` : ''

  return (
    <div style={styles.container}>
      <Head><title>Pending Transactions - TigerScan.io</title></Head>
      
      <header style={styles.header}>
        <Link href="/" style={styles.logo}>🐯 TigerScan</Link>
        <nav style={styles.nav}>
          <Link href="/blocks">Blocks</Link>
          <Link href="/transactions">Transactions</Link>
          <Link href="/mempool">Mempool</Link>
        </nav>
      </header>

      <main style={styles.main}>
        <h1 style={styles.title}>Pending Transaction Pool</h1>

        {!loading && stats && (
          <div style={styles.statsGrid}>
            <div style={styles.statCard}>
              <div style={styles.statLabel}>Pending Transactions</div>
              <div style={styles.statValue}>{stats.total_pending.toLocaleString()}</div>
            </div>
            <div style={styles.statCard}>
              <div style={styles.statLabel}>Avg Gas Price</div>
              <div style={styles.statValue}>{(parseFloat(String(stats.avg_gas_price)) / 1e9).toFixed(2)} Gwei</div>
            </div>
            <div style={styles.statCard}>
              <div style={styles.statLabel}>Top Sender</div>
              <div style={styles.statValue}>{stats.top_senders[0]?.address ? formatAddr(stats.top_senders[0].address) : '-'}</div>
            </div>
          </div>
        )}

        <div style={styles.toolbar}>
          <span style={styles.label}>Sort by:</span>
          <select value={sortBy} onChange={(e) => setSortBy(e.target.value as any)} style={styles.select}>
            <option value="gas_price">Gas Price</option>
            <option value="nonce">Nonce</option>
            <option value="value">Value</option>
          </select>
          <span style={styles.autoRefresh}>🔄 Auto-refresh every 5s</span>
        </div>

        <div style={styles.tableWrapper}>
          <table style={styles.table}>
            <thead>
              <tr>
                <th>Hash</th>
                <th>From</th>
                <th>To</th>
                <th>Value</th>
                <th>Gas Price</th>
                <th>Gas Limit</th>
                <th>Nonce</th>
              </tr>
            </thead>
            <tbody>
              {sortedTxs.map((tx, i) => (
                <tr key={i}>
                  <td><code style={styles.hash}>{formatAddr(tx.hash)}</code></td>
                  <td><code style={styles.addr}>{formatAddr(tx.from)}</code></td>
                  <td><code style={styles.addr}>{formatAddr(tx.to)}</code></td>
                  <td style={styles.value}>{(parseFloat(tx.value) / 1e18).toFixed(4)}</td>
                  <td style={styles.gasPrice}>{(parseFloat(tx.gas_price) / 1e9).toFixed(2)} Gwei</td>
                  <td>{tx.gas_limit.toLocaleString()}</td>
                  <td>{tx.nonce}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </main>
    </div>
  )
}

const styles: Record<string, React.CSSProperties> = {
  container: { fontFamily: 'system-ui, sans-serif', minHeight: '100vh', background: '#0a0a0f' },
  header: { display: 'flex', justifyContent: 'space-between', padding: '1rem 2rem', background: '#12121a', borderBottom: '1px solid #2a2a3a' },
  logo: { color: '#ff6b35', fontSize: '1.5rem', fontWeight: 'bold', textDecoration: 'none' },
  nav: { display: 'flex', gap: '1.5rem' },
  main: { maxWidth: '1400px', margin: '0 auto', padding: '2rem' },
  title: { color: '#fff', fontSize: '2rem', marginBottom: '1.5rem' },
  statsGrid: { display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '1rem', marginBottom: '1.5rem' },
  statCard: { background: '#12121a', padding: '1.5rem', borderRadius: '12px', border: '1px solid #2a2a3a' },
  statLabel: { color: '#888', fontSize: '0.9rem', marginBottom: '0.5rem' },
  statValue: { color: '#ff6b35', fontSize: '1.5rem', fontWeight: 'bold' },
  toolbar: { display: 'flex', gap: '1rem', alignItems: 'center', marginBottom: '1rem' },
  label: { color: '#888' },
  select: { padding: '0.5rem', borderRadius: '6px', background: '#1a1a24', color: '#fff', border: '1px solid #333' },
  autoRefresh: { marginLeft: 'auto', color: '#666', fontSize: '0.9rem' },
  tableWrapper: { background: '#12121a', borderRadius: '12px', overflow: 'hidden' },
  table: { width: '100%', borderCollapse: 'collapse', fontSize: '0.9rem' },
  hash: { color: '#88f', fontSize: '0.85rem' },
  addr: { color: '#aaa', fontSize: '0.85rem' },
  value: { color: '#6bff6b' },
  gasPrice: { color: '#ff6b35' }
}