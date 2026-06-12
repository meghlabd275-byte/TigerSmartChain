// TigerScan - Validator Detail Page
// Production-ready validator explorer with performance metrics and delegations

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'
import { useRouter } from 'next/router'

interface Validator {
  address: string
  name: string
  totalStake: string
  selfStake: string
  delegatorCount: number
  commissionRate: number
  uptime: number
  blocksSigned: number
  blocksMissed: number
  blocksProposed: number
  rewardsAccumulated: string
  isActive: boolean
  isJailed: boolean
}

interface Delegation {
  delegator: string
  amount: string
  rewards: string
}

interface RewardHistory {
  blockNumber: number
  amount: string
  timestamp: number
}

export default function ValidatorPage({ validator: initialValidator }: { validator: Validator | null }) {
  const router = useRouter()
  const [validator, setValidator] = useState<Validator | null>(initialValidator)
  const [loading, setLoading] = useState(!initialValidator)
  const [error, setError] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<'overview' | 'delegations' | 'rewards'>('overview')
  const [delegations, setDelegations] = useState<Delegation[]>([])
  const [rewards, setRewards] = useState<RewardHistory[]>([])

  useEffect(() => {
    if (!initialValidator && router.query.address) {
      fetchValidator()
    }
  }, [router.query.address])

  async function fetchValidator() {
    setLoading(true)
    try {
      const [valRes, delRes, rewRes] = await Promise.all([
        fetch(`/api/v1/validators/${router.query.address}`),
        fetch(`/api/v1/validators/${router.query.address}/delegations?limit=50`),
        fetch(`/api/v1/validators/${router.query.address}/rewards?limit=50`)
      ])
      
      if (!valRes.ok) throw new Error('Validator not found')
      
      const [valData, delData, rewData] = await Promise.all([
        valRes.json(),
        delRes.json(),
        rewRes.json()
      ])
      
      setValidator(valData)
      setDelegations(delData)
      setRewards(rewData)
    } catch (err: any) {
      setError(err.message || 'Failed to load validator')
    } finally {
      setLoading(false)
    }
  }

  function formatAddress(addr: string): string {
    if (!addr) return ''
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`
  }

  function formatStake(stake: string): string {
    const num = parseFloat(stake) / 1e18
    if (num >= 1e6) return `${(num / 1e6).toFixed(2)}M`
    if (num >= 1e3) return `${(num / 1e3).toFixed(2)}K`
    return num.toFixed(2)
  }

  if (loading) return <div style={styles.container}><div style={styles.loading}>Loading validator...</div></div>
  if (error || !validator) return <div style={styles.container}><div style={styles.error}><h2>Validator Not Found</h2><p>{error}</p><Link href="/">Go Home</Link></div></div>

  return (
    <div style={styles.container}>
      <Head>
        <title>{validator.name || formatAddress(validator.address)} | TigerScan.io</title>
      </Head>

      <header style={styles.header}>
        <div style={styles.headerContent}>
          <Link href="/" style={styles.logo}>🐯 TigerScan.io</Link>
          <nav style={styles.nav}>
            <Link href="/blocks">Blocks</Link>
            <Link href="/validators">Validators</Link>
          </nav>
        </div>
      </header>

      <div style={styles.breadcrumb}>
        <Link href="/">Home</Link> / <Link href="/validators">Validators</Link> / {validator.name || formatAddress(validator.address)}
      </div>

      {/* Validator Header */}
      <div style={styles.validatorHeader}>
        <div style={styles.validatorIcon}>⚡</div>
        <div style={styles.validatorInfo}>
          <h1 style={styles.title}>
            {validator.name || formatAddress(validator.address)}
            {validator.isActive ? <span style={styles.activeBadge}>Active</span> : <span style={styles.inactiveBadge}>Inactive</span>}
            {validator.isJailed && <span style={styles.jailedBadge}>Jailed</span>}
          </h1>
          <code style={styles.address}>{validator.address}</code>
        </div>
      </div>

      {/* Stats */}
      <div style={styles.statsGrid}>
        <div style={styles.statCard}>
          <label style={styles.label}>Total Stake</label>
          <span style={styles.value}>{formatStake(validator.totalStake)} TGR</span>
        </div>
        <div style={styles.statCard}>
          <label style={styles.label}>Self Stake</label>
          <span style={styles.value}>{formatStake(validator.selfStake)} TGR</span>
        </div>
        <div style={styles.statCard}>
          <label style={styles.label}>Delegators</label>
          <span style={styles.value}>{validator.delegatorCount.toLocaleString()}</span>
        </div>
        <div style={styles.statCard}>
          <label style={styles.label}>Commission</label>
          <span style={styles.value}>{validator.commissionRate}%</span>
        </div>
        <div style={styles.statCard}>
          <label style={styles.label}>Uptime</label>
          <span style={styles.value}>{validator.uptime.toFixed(2)}%</span>
        </div>
        <div style={styles.statCard}>
          <label style={styles.label}>Blocks Proposed</label>
          <span style={styles.value}>{validator.blocksProposed.toLocaleString()}</span>
        </div>
      </div>

      {/* Tabs */}
      <div style={styles.tabs}>
        <button style={activeTab === 'overview' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('overview')}>Overview</button>
        <button style={activeTab === 'delegations' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('delegations')}>
          Delegations {validator.delegatorCount ? `(${validator.delegatorCount})` : ''}
        </button>
        <button style={activeTab === 'rewards' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('rewards')}>Rewards</button>
      </div>

      {/* Content */}
      {activeTab === 'overview' && (
        <section style={styles.section}>
          <div style={styles.grid}>
            <div style={styles.card}><label style={styles.label}>Address</label><code style={styles.value}>{validator.address}</code></div>
            <div style={styles.card}><label style={styles.label}>Total Stake</label><span style={styles.value}>{formatStake(validator.totalStake)} TGR</span></div>
            <div style={styles.card}><label style={styles.label}>Self Stake</label><span style={styles.value}>{formatStake(validator.selfStake)} TGR</span></div>
            <div style={styles.card}><label style={styles.label}>Delegators</label><span style={styles.value}>{validator.delegatorCount}</span></div>
            <div style={styles.card}><label style={styles.label}>Commission Rate</label><span style={styles.value}>{validator.commissionRate}%</span></div>
            <div style={styles.card}><label style={styles.label}>Uptime</label><span style={styles.value}>{validator.uptime.toFixed(2)}%</span></div>
            <div style={styles.card}><label style={styles.label}>Blocks Signed</label><span style={styles.value}>{validator.blocksSigned.toLocaleString()}</span></div>
            <div style={styles.card}><label style={styles.label}>Blocks Missed</label><span style={styles.value}>{validator.blocksMissed.toLocaleString()}</span></div>
            <div style={styles.card}><label style={styles.label}>Blocks Proposed</label><span style={styles.value}>{validator.blocksProposed.toLocaleString()}</span></div>
            <div style={styles.card}><label style={styles.label}>Total Rewards</label><span style={styles.value}>{formatStake(validator.rewardsAccumulated)} TGR</span></div>
          </div>
        </section>
      )}

      {activeTab === 'delegations' && (
        <section style={styles.section}>
          {delegations.length === 0 ? (
            <p style={styles.empty}>No delegations</p>
          ) : (
            <table style={styles.table}>
              <thead><tr><th style={styles.th}>Delegator</th><th style={styles.th}>Amount</th><th style={styles.th}>Rewards</th></tr></thead>
              <tbody>
                {delegations.map((d) => (
                  <tr key={d.delegator}>
                    <td style={styles.td}><Link href={`/address/${d.delegator}`} style={styles.address}>{formatAddress(d.delegator)}</Link></td>
                    <td style={styles.td}>{formatStake(d.amount)} TGR</td>
                    <td style={styles.td}>{formatStake(d.rewards)} TGR</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      )}

      {activeTab === 'rewards' && (
        <section style={styles.section}>
          {rewards.length === 0 ? (
            <p style={styles.empty}>No rewards history</p>
          ) : (
            <table style={styles.table}>
              <thead><tr><th style={styles.th}>Block</th><th style={styles.th}>Amount</th><th style={styles.th}>Timestamp</th></tr></thead>
              <tbody>
                {rewards.map((r) => (
                  <tr key={r.blockNumber}>
                    <td style={styles.td}><Link href={`/blocks/${r.blockNumber}`} style={styles.link}>#{r.blockNumber}</Link></td>
                    <td style={styles.td}>{formatStake(r.amount)} TGR</td>
                    <td style={styles.td}>{new Date(r.timestamp * 1000).toLocaleString()}</td>
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
  validatorHeader: { maxWidth: '1200px', margin: '0 auto 1rem', padding: '0 1rem', display: 'flex', alignItems: 'center', gap: '1rem' },
  validatorIcon: { width: '64px', height: '64px', borderRadius: '50%', background: '#ff6b35', color: 'white', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '2rem' },
  validatorInfo: { flex: 1 },
  title: { display: 'flex', alignItems: 'center', gap: '0.5rem', fontSize: '1.5rem', marginBottom: '0.25rem' },
  activeBadge: { padding: '0.25rem 0.5rem', background: '#22c55e', color: 'white', borderRadius: '4px', fontSize: '0.75rem', fontFamily: 'system-ui' },
  inactiveBadge: { padding: '0.25rem 0.5rem', background: '#6b7280', color: 'white', borderRadius: '4px', fontSize: '0.75rem', fontFamily: 'system-ui' },
  jailedBadge: { padding: '0.25rem 0.5rem', background: '#ef4444', color: 'white', borderRadius: '4px', fontSize: '0.75rem', fontFamily: 'system-ui' },
  address: { fontSize: '0.875rem', color: '#666', fontFamily: 'monospace' },
  statsGrid: { maxWidth: '1200px', margin: '0 auto 1rem', padding: '0 1rem', display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(150px, 1fr))', gap: '1rem' },
  statCard: { background: 'white', padding: '1rem', borderRadius: '8px', boxShadow: '0 1px 3px rgba(0,0,0,0.1)' },
  label: { display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: '0.25rem', textTransform: 'uppercase' },
  value: { fontSize: '1rem', color: '#1a1a2e', fontWeight: '500' },
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
  link: { color: '#ff6b35', textDecoration: 'none' },
  loading: { maxWidth: '1200px', margin: '2rem auto', padding: '2rem', textAlign: 'center', background: 'white', borderRadius: '8px' },
  error: { maxWidth: '1200px', margin: '2rem auto', padding: '2rem', textAlign: 'center', background: 'white', borderRadius: '8px' },
  empty: { padding: '2rem', textAlign: 'center', color: '#666', background: 'white', borderRadius: '8px' },
  footer: { textAlign: 'center', padding: '2rem', color: '#666', marginTop: '3rem' }
}