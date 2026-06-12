// TigerScan - Token Detail Page
// Production-ready token explorer with holders, transfers, and analytics

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'
import { useRouter } from 'next/router'

interface Token {
  address: string
  name: string
  symbol: string
  decimals: number
  totalSupply: string
  type: string
  isVerified: boolean
  holderCount: number
  transferCount: number
  priceUsd: number
  priceChange24h: number
  volume24h: number
  marketCap: number
}

interface Holder {
  address: string
  balance: string
  rank: number
}

interface Transfer {
  hash: string
  from: string
  to: string
  value: string
  blockNumber: number
  timestamp: number
}

export default function TokenPage({ token: initialToken }: { token: Token | null }) {
  const router = useRouter()
  const [token, setToken] = useState<Token | null>(initialToken)
  const [loading, setLoading] = useState(!initialToken)
  const [error, setError] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<'overview' | 'holders' | 'transfers' | 'analytics'>('overview')
  const [holders, setHolders] = useState<Holder[]>([])
  const [transfers, setTransfers] = useState<Transfer[]>([])

  useEffect(() => {
    if (!initialToken && router.query.address) {
      fetchToken()
    }
  }, [router.query.address])

  async function fetchToken() {
    setLoading(true)
    try {
      const [tokenRes, holdersRes, transfersRes] = await Promise.all([
        fetch(`/api/v1/tokens/${router.query.address}`),
        fetch(`/api/v1/tokens/${router.query.address}/holders?limit=50`),
        fetch(`/api/v1/tokens/${router.query.address}/transfers?limit=50`)
      ])
      
      if (!tokenRes.ok) throw new Error('Token not found')
      
      const [tokenData, holdersData, transfersData] = await Promise.all([
        tokenRes.json(),
        holdersRes.json(),
        transfersRes.json()
      ])
      
      setToken(tokenData)
      setHolders(holdersData)
      setTransfers(transfersData)
    } catch (err: any) {
      setError(err.message || 'Failed to load token')
    } finally {
      setLoading(false)
    }
  }

  function formatAddress(addr: string): string {
    if (!addr) return ''
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`
  }

  function formatSupply(supply: string, decimals: number): string {
    const num = parseFloat(supply)
    return (num / Math.pow(10, decimals)).toLocaleString()
  }

  function formatBalance(balance: string): string {
    const num = parseFloat(balance)
    if (num >= 1e9) return `${(num / 1e9).toFixed(2)}B`
    if (num >= 1e6) return `${(num / 1e6).toFixed(2)}M`
    if (num >= 1e3) return `${(num / 1e3).toFixed(2)}K`
    return num.toFixed(2)
  }

  if (loading) return <div style={styles.container}><div style={styles.loading}>Loading token...</div></div>
  if (error || !token) return <div style={styles.container}><div style={styles.error}><h2>Token Not Found</h2><p>{error}</p><Link href="/">Go Home</Link></div></div>

  return (
    <div style={styles.container}>
      <Head>
        <title>{token.name} ({token.symbol}) | TigerScan.io</title>
      </Head>

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

      <div style={styles.breadcrumb}>
        <Link href="/">Home</Link> / <Link href="/tokens">Tokens</Link> / {token.symbol}
      </div>

      {/* Token Header */}
      <div style={styles.tokenHeader}>
        <div style={styles.tokenIcon}>{token.symbol.slice(0, 2)}</div>
        <div style={styles.tokenInfo}>
          <h1 style={styles.title}>
            {token.name}
            {token.isVerified && <span style={styles.verifiedBadge}>✓</span>}
          </h1>
          <p style={styles.symbol}>{token.symbol} • {token.type} • {token.decimals} decimals</p>
        </div>
      </div>

      {/* Price Card */}
      <div style={styles.priceCard}>
        <div style={styles.priceLabel}>Price</div>
        <div style={styles.priceValue}>${token.priceUsd.toFixed(6)}</div>
        <div style={token.priceChange24h >= 0 ? styles.priceUp : styles.priceDown}>
          {token.priceChange24h >= 0 ? '↑' : '↓'} {Math.abs(token.priceChange24h).toFixed(2)}% (24h)
        </div>
      </div>

      {/* Stats */}
      <div style={styles.statsGrid}>
        <div style={styles.statCard}>
          <label style={styles.label}>Market Cap</label>
          <span style={styles.value}>${(token.marketCap / 1e6).toFixed(2)}M</span>
        </div>
        <div style={styles.statCard}>
          <label style={styles.label}>24h Volume</label>
          <span style={styles.value}>${(token.volume24h / 1e6).toFixed(2)}M</span>
        </div>
        <div style={styles.statCard}>
          <label style={styles.label}>Holders</label>
          <span style={styles.value}>{token.holderCount.toLocaleString()}</span>
        </div>
        <div style={styles.statCard}>
          <label style={styles.label}>Transfers</label>
          <span style={styles.value}>{token.transferCount.toLocaleString()}</span>
        </div>
        <div style={styles.statCard}>
          <label style={styles.label}>Total Supply</label>
          <span style={styles.value}>{formatSupply(token.totalSupply, token.decimals)}</span>
        </div>
        <div style={styles.statCard}>
          <label style={styles.label}>Contract</label>
          <Link href={`/address/${token.address}`} style={styles.link}>{formatAddress(token.address)}</Link>
        </div>
      </div>

      {/* Tabs */}
      <div style={styles.tabs}>
        <button style={activeTab === 'overview' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('overview')}>Overview</button>
        <button style={activeTab === 'holders' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('holders')}>
          Holders {token.holderCount ? `(${token.holderCount})` : ''}
        </button>
        <button style={activeTab === 'transfers' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('transfers')}>
          Transfers {token.transferCount ? `(${token.transferCount})` : ''}
        </button>
        <button style={activeTab === 'analytics' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('analytics')}>Analytics</button>
      </div>

      {/* Content */}
      {activeTab === 'overview' && (
        <section style={styles.section}>
          <div style={styles.grid}>
            <div style={styles.card}><label style={styles.label}>Contract Address</label><code style={styles.value}>{token.address}</code></div>
            <div style={styles.card}><label style={styles.label}>Token Type</label><span style={styles.value}>{token.type}</span></div>
            <div style={styles.card}><label style={styles.label}>Decimals</label><span style={styles.value}>{token.decimals}</span></div>
            <div style={styles.card}><label style={styles.label}>Total Supply</label><span style={styles.value}>{formatSupply(token.totalSupply, token.decimals)}</span></div>
          </div>
        </section>
      )}

      {activeTab === 'holders' && (
        <section style={styles.section}>
          {holders.length === 0 ? (
            <p style={styles.empty}>No holders</p>
          ) : (
            <table style={styles.table}>
              <thead><tr><th style={styles.th}>#</th><th style={styles.th}>Address</th><th style={styles.th}>Balance</th><th style={styles.th}>%</th></tr></thead>
              <tbody>
                {holders.map((h) => (
                  <tr key={h.address}>
                    <td style={styles.td}>{h.rank}</td>
                    <td style={styles.td}><Link href={`/address/${h.address}`} style={styles.address}>{formatAddress(h.address)}</Link></td>
                    <td style={styles.td}>{formatBalance(h.balance)}</td>
                    <td style={styles.td}>-</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      )}

      {activeTab === 'transfers' && (
        <section style={styles.section}>
          {transfers.length === 0 ? (
            <p style={styles.empty}>No transfers</p>
          ) : (
            <table style={styles.table}>
              <thead><tr><th style={styles.th}>Tx Hash</th><th style={styles.th}>From</th><th style={styles.th}>To</th><th style={styles.th}>Value</th><th style={styles.th}>Block</th></tr></thead>
              <tbody>
                {transfers.map((t) => (
                  <tr key={t.hash}>
                    <td style={styles.td}><Link href={`/tx/${t.hash}`} style={styles.txHash}>{formatAddress(t.hash)}</Link></td>
                    <td style={styles.td}><Link href={`/address/${t.from}`} style={styles.address}>{formatAddress(t.from)}</Link></td>
                    <td style={styles.td}><Link href={`/address/${t.to}`} style={styles.address}>{formatAddress(t.to)}</Link></td>
                    <td style={styles.td}>{formatBalance(t.value)}</td>
                    <td style={styles.td}><Link href={`/blocks/${t.blockNumber}`} style={styles.link}>#{t.blockNumber}</Link></td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      )}

      {activeTab === 'analytics' && (
        <section style={styles.section}>
          <div style={styles.analyticsGrid}>
            <div style={styles.card}><label style={styles.label}>Market Cap</label><span style={styles.value}>${(token.marketCap / 1e6).toFixed(2)}M</span></div>
            <div style={styles.card}><label style={styles.label}>24h Volume</label><span style={styles.value}>${(token.volume24h / 1e6).toFixed(2)}M</span></div>
            <div style={styles.card}><label style={styles.label}>24h Change</label><span style={token.priceChange24h >= 0 ? styles.success : styles.failure}>{token.priceChange24h.toFixed(2)}%</span></div>
          </div>
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
  tokenHeader: { maxWidth: '1200px', margin: '0 auto 1rem', padding: '0 1rem', display: 'flex', alignItems: 'center', gap: '1rem' },
  tokenIcon: { width: '48px', height: '48px', borderRadius: '50%', background: '#ff6b35', color: 'white', display: 'flex', alignItems: 'center', justifyContent: 'center', fontWeight: 'bold', fontSize: '1.25rem' },
  tokenInfo: { flex: 1 },
  title: { display: 'flex', alignItems: 'center', gap: '0.5rem', fontSize: '1.5rem', marginBottom: '0.25rem' },
  verifiedBadge: { display: 'inline-flex', alignItems: 'center', justifyContent: 'center', width: '20px', height: '20px', background: '#22c55e', color: 'white', borderRadius: '50%', fontSize: '0.75rem' },
  symbol: { color: '#666' },
  priceCard: { maxWidth: '1200px', margin: '0 auto 1rem', padding: '1.5rem', background: 'linear-gradient(135deg, #1a1a2e 0%, #16213e 100%)', borderRadius: '12px', color: 'white' },
  priceLabel: { fontSize: '0.75rem', color: '#9ca3af', marginBottom: '0.25rem' },
  priceValue: { fontSize: '2rem', fontWeight: 'bold', marginBottom: '0.25rem' },
  priceUp: { color: '#22c55e', fontSize: '0.875rem' },
  priceDown: { color: '#ef4444', fontSize: '0.875rem' },
  statsGrid: { maxWidth: '1200px', margin: '0 auto 1rem', padding: '0 1rem', display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(150px, 1fr))', gap: '1rem' },
  statCard: { background: 'white', padding: '1rem', borderRadius: '8px', boxShadow: '0 1px 3px rgba(0,0,0,0.1)' },
  label: { display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: '0.25rem', textTransform: 'uppercase' },
  value: { fontSize: '0.875rem', color: '#1a1a2e' },
  link: { color: '#ff6b35', textDecoration: 'none' },
  tabs: { maxWidth: '1200px', margin: '0 auto', padding: '0 1rem', display: 'flex', gap: '0.5rem', borderBottom: '1px solid #e5e5e5' },
  tab: { padding: '0.75rem 1rem', background: 'none', border: 'none', borderBottom: '2px solid transparent', cursor: 'pointer', fontSize: '0.875rem', color: '#666' },
  activeTab: { padding: '0.75rem 1rem', background: 'none', border: 'none', borderBottom: '2px solid #ff6b35', cursor: 'pointer', fontSize: '0.875rem', color: '#ff6b35', fontWeight: '600' },
  section: { maxWidth: '1200px', margin: '1rem auto', padding: '0 1rem' },
  grid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: '1rem' },
  card: { background: 'white', padding: '1rem', borderRadius: '8px', boxShadow: '0 1px 3px rgba(0,0,0,0.1)' },
  table: { width: '100%', background: 'white', borderRadius: '8px', overflow: 'hidden', borderCollapse: 'collapse' },
  th: { padding: '0.75rem', textAlign: 'left', background: '#f5f6fa', fontSize: '0.75rem', color: '#666', fontWeight: '600' },
  td: { padding: '0.75rem', borderTop: '1px solid #eee', fontSize: '0.875rem' },
  address: { color: '#ff6b35', textDecoration: 'none' },
  txHash: { color: '#ff6b35', textDecoration: 'none', fontFamily: 'monospace' },
  success: { color: '#22c55e' },
  failure: { color: '#ef4444' },
  analyticsGrid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(150px, 1fr))', gap: '1rem' },
  loading: { maxWidth: '1200px', margin: '2rem auto', padding: '2rem', textAlign: 'center', background: 'white', borderRadius: '8px' },
  error: { maxWidth: '1200px', margin: '2rem auto', padding: '2rem', textAlign: 'center', background: 'white', borderRadius: '8px' },
  empty: { padding: '2rem', textAlign: 'center', color: '#666', background: 'white', borderRadius: '8px' },
  footer: { textAlign: 'center', padding: '2rem', color: '#666', marginTop: '3rem' }
}