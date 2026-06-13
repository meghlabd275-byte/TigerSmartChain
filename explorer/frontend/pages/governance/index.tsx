// TigerScan - Governance DAO Page

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'

interface Proposal {
  id: number
  title: string
  description: string
  status: string
  for_votes: string
  against_votes: string
  start_block: number
  end_block: number
}

interface GovernanceStats {
  total_proposals: number
  active_proposals: number
  passed_proposals: number
}

export default function Governance() {
  const [proposals, setProposals] = useState<Proposal[]>([])
  const [stats, setStats] = useState<GovernanceStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [filter, setFilter] = useState<string>('all')

  useEffect(() => {
    fetchData()
  }, [])

  const fetchData = async () => {
    try {
      const [proposalsRes, statsRes] = await Promise.all([
        fetch('https://api.tigerscan.io/v1/governance/proposals'),
        fetch('https://api.tigerscan.io/v1/governance/stats')
      ])
      
      const proposalsData = await proposalsRes.json()
      const statsData = await statsRes.json()
      
      setProposals(proposalsData.proposals || [])
      setStats(statsData)
    } catch (err) {
      console.error(err)
    } finally {
      setLoading(false)
    }
  }

  const filteredProposals = proposals.filter(p => {
    if (filter === 'all') return true
    if (filter === 'active') return p.status === 'Active'
    if (filter === 'passed') return p.status === 'Executed'
    if (filter === 'failed') return p.status === 'Defeated'
    return true
  })

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'Active': return '#00cc88'
      case 'Executed': return '#6bff6b'
      case 'Defeated': return '#ff6b6b'
      case 'Queued': return '#ffaa00'
      default: return '#888'
    }
  }

  const formatVotes = (votes: string) => {
    const num = parseFloat(votes) / 1e18
    return num.toLocaleString(undefined, { maximumFractionDigits: 2 })
  }

  return (
    <div style={styles.container}>
      <Head><title>Governance - TigerScan.io</title></Head>
      
      <header style={styles.header}>
        <Link href="/" style={styles.logo}>🐯 TigerScan</Link>
        <nav style={styles.nav}>
          <Link href="/blocks">Blocks</Link>
          <Link href="/governance">Governance</Link>
        </nav>
      </header>

      <main style={styles.main}>
        <h1 style={styles.title}>Governance Dashboard</h1>

        {!loading && stats && (
          <div style={styles.statsGrid}>
            <div style={styles.statCard}>
              <div style={styles.statLabel}>Total Proposals</div>
              <div style={styles.statValue}>{stats.total_proposals}</div>
            </div>
            <div style={styles.statCard}>
              <div style={styles.statLabel}>Active</div>
              <div style={{...styles.statValue, color: '#00cc88'}}>{stats.active_proposals}</div>
            </div>
            <div style={styles.statCard}>
              <div style={styles.statLabel}>Passed</div>
              <div style={{...styles.statValue, color: '#6bff6b'}}>{stats.passed_proposals}</div>
            </div>
          </div>
        )}

        <div style={styles.toolbar}>
          <span style={styles.label}>Filter:</span>
          {['all', 'active', 'passed', 'failed'].map(f => (
            <button key={f} onClick={() => setFilter(f)} style={filter === f ? styles.activeFilter : styles.filter}>
              {f.charAt(0).toUpperCase() + f.slice(1)}
            </button>
          ))}
        </div>

        <div style={styles.proposalsList}>
          {filteredProposals.map((proposal, i) => (
            <div key={i} style={styles.proposalCard}>
              <div style={styles.proposalHeader}>
                <span style={styles.proposalId}>#{proposal.id}</span>
                <span style={{...styles.status, color: getStatusColor(proposal.status), borderColor: getStatusColor(proposal.status)}}>
                  {proposal.status}
                </span>
              </div>
              
              <h3 style={styles.proposalTitle}>{proposal.title}</h3>
              <p style={styles.proposalDesc}>{proposal.description.slice(0, 200)}...</p>
              
              <div style={styles.votesSection}>
                <div style={styles.voteBar}>
                  <div style={{...styles.voteFor, width: `${(parseFloat(proposal.for_votes) / (parseFloat(proposal.for_votes) + parseFloat(proposal.against_votes))) * 100 || 0}%`}} />
                </div>
                <div style={styles.votesInfo}>
                  <span style={styles.votesFor}>✅ {formatVotes(proposal.for_votes)} TGR</span>
                  <span style={styles.votesAgainst}>❌ {formatVotes(proposal.against_votes)} TGR</span>
                </div>
              </div>
              
              <div style={styles.proposalMeta}>
                <span>Blocks: {proposal.start_block} - {proposal.end_block}</span>
              </div>
            </div>
          ))}
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
  main: { maxWidth: '1000px', margin: '0 auto', padding: '2rem' },
  title: { color: '#fff', fontSize: '2rem', marginBottom: '1.5rem' },
  statsGrid: { display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '1rem', marginBottom: '2rem' },
  statCard: { background: '#12121a', padding: '1.5rem', borderRadius: '12px', border: '1px solid #2a2a3a' },
  statLabel: { color: '#888', fontSize: '0.9rem' },
  statValue: { color: '#ff6b35', fontSize: '1.8rem', fontWeight: 'bold', marginTop: '0.5rem' },
  toolbar: { display: 'flex', gap: '0.5rem', marginBottom: '1.5rem' },
  label: { color: '#888', marginRight: '0.5rem' },
  filter: { padding: '0.5rem 1rem', background: '#1a1a24', border: '1px solid #333', borderRadius: '6px', color: '#888', cursor: 'pointer' },
  activeFilter: { padding: '0.5rem 1rem', background: '#ff6b35', border: 'none', borderRadius: '6px', color: '#fff', cursor: 'pointer' },
  proposalsList: { display: 'flex', flexDirection: 'column', gap: '1rem' },
  proposalCard: { background: '#12121a', padding: '1.5rem', borderRadius: '12px', border: '1px solid #2a2a3a' },
  proposalHeader: { display: 'flex', justifyContent: 'space-between', marginBottom: '0.75rem' },
  proposalId: { color: '#666', fontSize: '0.9rem' },
  status: { padding: '0.25rem 0.75rem', border: '1px solid', borderRadius: '20px', fontSize: '0.8rem', fontWeight: 'bold' },
  proposalTitle: { color: '#fff', margin: '0 0 0.5rem', fontSize: '1.2rem' },
  proposalDesc: { color: '#888', fontSize: '0.9rem', margin: '0 0 1rem', lineHeight: 1.5 },
  votesSection: { marginBottom: '1rem' },
  voteBar: { height: '8px', background: '#2a2a3a', borderRadius: '4px', overflow: 'hidden', marginBottom: '0.5rem' },
  voteFor: { height: '100%', background: 'linear-gradient(90deg, #00cc88, #6bff6b)', borderRadius: '4px' },
  votesInfo: { display: 'flex', justifyContent: 'space-between', fontSize: '0.9rem' },
  votesFor: { color: '#6bff6b' },
  votesAgainst: { color: '#ff6b6b' },
  proposalMeta: { color: '#666', fontSize: '0.85rem' }
}