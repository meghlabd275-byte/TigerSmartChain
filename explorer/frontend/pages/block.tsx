// TigerScan - Block Detail Page
// Production-ready block explorer detail page with full information

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'
import { useRouter } from 'next/router'

// Types
interface Block {
  number: number
  hash: string
  parentHash: string
  timestamp: number
  miner: string
  gasUsed: number
  gasLimit: number
  transactions: Transaction[]
  txCount: number
  reward: string
  baseFeePerGas: number
  size: number
}

interface Transaction {
  hash: string
  from: string
  to: string
  value: string
  gasPrice: number
  gasUsed: number
  status: boolean
  timestamp: number
}

interface PageProps {
  params: { number: string }
}

export async function getServerSideProps({ params }: PageProps) {
  const blockNumber = parseInt(params.number)
  
  try {
    // Fetch block data from API
    const blockRes = await fetch(`${process.env.API_URL}/api/v1/blocks/${blockNumber}`)
    const block = await blockRes.json()
    
    // Fetch transactions
    const txsRes = await fetch(`${process.env.API_URL}/api/v1/transactions?block=${blockNumber}`)
    const transactions = await txsRes.json()
    
    return {
      props: {
        block: block || null,
        transactions: transactions || []
      }
    }
  } catch (error) {
    return { props: { block: null, transactions: [] } }
  }
}

export default function BlockPage({ block: initialBlock, transactions: initialTxs }: { block: Block | null, transactions: Transaction[] }) {
  const router = useRouter()
  const [block, setBlock] = useState<Block | null>(initialBlock)
  const [loading, setLoading] = useState(!initialBlock)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!initialBlock && router.query.number) {
      fetchBlock()
    }
  }, [router.query.number])

  async function fetchBlock() {
    setLoading(true)
    setError(null)
    
    try {
      const [blockRes, txsRes] = await Promise.all([
        fetch(`/api/v1/blocks/${router.query.number}`),
        fetch(`/api/v1/transactions?block=${router.query.number}&limit=100`)
      ])
      
      if (!blockRes.ok) {
        throw new Error('Block not found')
      }
      
      const blockData = await blockRes.json()
      const txsData = await txsRes.json()
      
      setBlock(blockData)
    } catch (err: any) {
      setError(err.message || 'Failed to load block')
    } finally {
      setLoading(false)
    }
  }

  function formatAddress(addr: string): string {
    if (!addr) return ''
    if (addr.length < 16) return addr
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

  function formatHash(hash: string): string {
    if (!hash) return ''
    return `${hash.slice(0, 10)}...${hash.slice(-8)}`
  }

  if (loading) {
    return (
      <div style={styles.container}>
        <div style={styles.loading}>Loading block...</div>
      </div>
    )
  }

  if (error || !block) {
    return (
      <div style={styles.container}>
        <div style={styles.error}>
          <h2>Block Not Found</h2>
          <p>{error || 'This block does not exist'}</p>
          <Link href="/" style={styles.link}>Go to Homepage</Link>
        </div>
      </div>
    )
  }

  return (
    <div style={styles.container}>
      <Head>
        <title>Block #{block.number} | TigerScan.io</title>
        <meta name="description" content={`Block ${block.number} on TigerSmartChain`} />
      </Head>

      {/* Header */}
      <header style={styles.header}>
        <div style={styles.headerContent}>
          <Link href="/" style={styles.logo}>🐯 TigerScan.io</Link>
          <nav style={styles.nav}>
            <Link href="/blocks">Blocks</Link>
            <Link href="/transactions">Transactions</Link>
            <Link href="/tokens">Tokens</Link>
            <Link href="/validators">Validators</Link>
          </nav>
        </div>
      </header>

      {/* Breadcrumb */}
      <div style={styles.breadcrumb}>
        <Link href="/">Home</Link> / <Link href="/blocks">Blocks</Link> / Block #{block.number}
      </div>

      {/* Block Overview */}
      <section style={styles.section}>
        <h1 style={styles.title}>Block #{block.number}</h1>
        
        <div style={styles.grid}>
          <div style={styles.card}>
            <label style={styles.label}>Hash</label>
            <code style={styles.value}>{block.hash}</code>
          </div>
          <div style={styles.card}>
            <label style={styles.label}>Parent Hash</label>
            <Link href={`/blocks/${block.number - 1}`} style={styles.link}>{formatHash(block.parentHash)}</Link>
          </div>
          <div style={styles.card}>
            <label style={styles.label}>Timestamp</label>
            <span style={styles.value}>{formatTime(block.timestamp)}</span>
          </div>
          <div style={styles.card}>
            <label style={styles.label}>Miner</label>
            <Link href={`/address/${block.miner}`} style={styles.value}>{formatAddress(block.miner)}</Link>
          </div>
          <div style={styles.card}>
            <label style={styles.label}>Gas Used</label>
            <span style={styles.value}>{formatGas(block.gasUsed)} ({((block.gasUsed / block.gasLimit) * 100).toFixed(1)}%)</span>
          </div>
          <div style={styles.card}>
            <label style={styles.label}>Gas Limit</label>
            <span style={styles.value}>{formatGas(block.gasLimit)}</span>
          </div>
          <div style={styles.card}>
            <label style={styles.label}>Base Fee Per Gas</label>
            <span style={styles.value}>{formatValue(block.baseFeePerGas)} Gwei</span>
          </div>
          <div style={styles.card}>
            <label style={styles.label}>Block Reward</label>
            <span style={styles.value}>{formatValue(block.reward)} TGR</span>
          </div>
          <div style={styles.card}>
            <label style={styles.label}>Size</label>
            <span style={styles.value}>{block.size} bytes</span>
          </div>
          <div style={styles.card}>
            <label style={styles.label}>Transactions</label>
            <span style={styles.value}>{block.txCount}</span>
          </div>
        </div>
      </section>

      {/* Transactions */}
      <section style={styles.section}>
        <h2 style={styles.sectionTitle}>Transactions ({block.txCount})</h2>
        
        {block.txCount === 0 ? (
          <p style={styles.empty}>No transactions in this block</p>
        ) : (
          <table style={styles.table}>
            <thead>
              <tr>
                <th style={styles.th}>Tx Hash</th>
                <th style={styles.th}>From</th>
                <th style={styles.th}>To</th>
                <th style={styles.th}>Value</th>
                <th style={styles.th}>Gas Price</th>
                <th style={styles.th}>Status</th>
              </tr>
            </thead>
            <tbody>
              {block.transactions?.map((tx: Transaction) => (
                <tr key={tx.hash}>
                  <td style={styles.td}>
                    <Link href={`/tx/${tx.hash}`} style={styles.txHash}>{formatHash(tx.hash)}</Link>
                  </td>
                  <td style={styles.td}>
                    <Link href={`/address/${tx.from}`} style={styles.address}>{formatAddress(tx.from)}</Link>
                  </td>
                  <td style={styles.td}>
                    <Link href={`/address/${tx.to}`} style={styles.address}>{formatAddress(tx.to)}</Link>
                  </td>
                  <td style={styles.td}>{formatValue(tx.value)} TGR</td>
                  <td style={styles.td}>{formatValue(tx.gasPrice)} Gwei</td>
                  <td style={styles.td}>
                    <span style={tx.status ? styles.success : styles.failure}>
                      {tx.status ? '✓ Success' : '✗ Failed'}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      {/* Footer */}
      <footer style={styles.footer}>
        <p>TigerScan.io © 2024 - TigerSmartChain Explorer</p>
        <p>Chain ID: 9001 | Token: TGR (Tiger Coin)</p>
      </footer>
    </div>
  )
}

function formatGas(gas: number): string {
  if (gas >= 1e6) return `${(gas / 1e6).toFixed(2)}M`
  if (gas >= 1e3) return `${(gas / 1e3).toFixed(2)}K`
  return gas.toString()
}

const styles = {
  container: { fontFamily: 'system-ui, sans-serif', minHeight: '100vh', background: '#f5f6fa' },
  header: { background: '#1a1a2e', padding: '1rem' },
  headerContent: { maxWidth: '1200px', margin: '0 auto', display: 'flex', justifyContent: 'space-between', alignItems: 'center' },
  logo: { color: '#ff6b35', fontSize: '1.5rem', fontWeight: 'bold', textDecoration: 'none' },
  nav: { display: 'flex', gap: '1.5rem' },
  breadcrumb: { maxWidth: '1200px', margin: '1rem auto', padding: '0 1rem', color: '#666' },
  section: { maxWidth: '1200px', margin: '2rem auto', padding: '0 1rem' },
  title: { fontSize: '1.5rem', marginBottom: '1rem', color: '#1a1a2e' },
  sectionTitle: { fontSize: '1.25rem', marginBottom: '1rem', color: '#1a1a2e' },
  grid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: '1rem' },
  card: { background: 'white', padding: '1rem', borderRadius: '8px', boxShadow: '0 1px 3px rgba(0,0,0,0.1)' },
  label: { display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: '0.25rem', textTransform: 'uppercase' },
  value: { fontSize: '0.875rem', color: '#1a1a2e', wordBreak: 'break-all' },
  link: { color: '#ff6b35', textDecoration: 'none' },
  table: { width: '100%', background: 'white', borderRadius: '8px', overflow: 'hidden', borderCollapse: 'collapse' },
  th: { padding: '0.75rem', textAlign: 'left', background: '#f5f6fa', fontSize: '0.75rem', color: '#666', fontWeight: '600' },
  td: { padding: '0.75rem', borderTop: '1px solid #eee', fontSize: '0.875rem' },
  txHash: { color: '#ff6b35', textDecoration: 'none', fontFamily: 'monospace' },
  address: { color: '#ff6b35', textDecoration: 'none' },
  success: { color: '#22c55e' },
  failure: { color: '#ef4444' },
  loading: { maxWidth: '1200px', margin: '2rem auto', padding: '2rem', textAlign: 'center', background: 'white', borderRadius: '8px' },
  error: { maxWidth: '1200px', margin: '2rem auto', padding: '2rem', textAlign: 'center', background: 'white', borderRadius: '8px' },
  empty: { padding: '2rem', textAlign: 'center', color: '#666', background: 'white', borderRadius: '8px' },
  footer: { textAlign: 'center', padding: '2rem', color: '#666', marginTop: '3rem' }
}