// TigerScan - Blockchain Explorer for TigerSmartChain
// Next.js frontend

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'
import styles from '../styles/Home.module.css'

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
  blockNumber: number
}

interface Stats {
  totalBlocks: number
  totalTransactions: number
  tps: number
  avgGasPrice: number
}

export default function Home() {
  const [latestBlocks, setLatestBlocks] = useState<Block[]>([])
  const [latestTxs, setLatestTxs] = useState<Transaction[]>([])
  const [stats, setStats] = useState<Stats>({
    totalBlocks: 0,
    totalTransactions: 0,
    tps: 0,
    avgGasPrice: 0
  })
  const [searchQuery, setSearchQuery] = useState('')
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    fetchData()
  }, [])

  async function fetchData() {
    try {
      const [blocksRes, txsRes, statsRes] = await Promise.all([
        fetch('/api/v1/blocks?limit=10'),
        fetch('/api/v1/transactions?limit=10'),
        fetch('/api/v1/analytics/stats')
      ])

      const blocks = await blocksRes.json()
      const txs = await txsRes.json()
      const statsData = await statsRes.json()

      setLatestBlocks(blocks.slice(0, 10) || [])
      setLatestTxs(txs.slice(0, 10) || [])
      setStats(statsData)
    } catch (error) {
      console.error('Failed to fetch data:', error)
    } finally {
      setLoading(false)
    }
  }

  function handleSearch(e: React.FormEvent) {
    e.preventDefault()
    if (searchQuery.trim()) {
      window.location.href = `/search?q=${encodeURIComponent(searchQuery)}`
    }
  }

  function formatAddress(addr: string): string {
    if (!addr || addr.length < 16) return addr
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`
  }

  function formatValue(value: string): string {
    if (!value) return '0'
    const num = parseFloat(value)
    if (isNaN(num)) return '0'
    return (num / 1e18).toFixed(4)
  }

  function formatNumber(num: number): string {
    return new Intl.NumberFormat().format(num)
  }

  function formatTime(timestamp: number): string {
    return new Date(timestamp * 1000).toLocaleString()
  }

  return (
    <div className={styles.container}>
      <Head>
        <title>TigerScan - TigerSmartChain Explorer</title>
        <meta name="description" content="TigerScan - Blockchain Explorer for TigerSmartChain" />
        <link rel="icon" href="/favicon.ico" />
      </Head>

      {/* Header */}
      <header className={styles.header}>
        <div className={styles.headerContent}>
          <Link href="/" className={styles.logo}>
            <span className={styles.logoIcon}>🐯</span>
            <span className={styles.logoText}>TigerScan</span>
          </Link>
          <nav className={styles.nav}>
            <Link href="/blocks">Blocks</Link>
            <Link href="/transactions">Transactions</Link>
            <Link href="/tokens">Tokens</Link>
            <Link href="/validators">Validators</Link>
            <Link href="/nfts">NFTs</Link>
          </nav>
        </div>
      </header>

      {/* Search Bar */}
      <section className={styles.searchSection}>
        <form onSubmit={handleSearch} className={styles.searchForm}>
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search by Address, Transaction Hash, Block Number"
            className={styles.searchInput}
          />
          <button type="submit" className={styles.searchButton}>
            Search
          </button>
        </form>
      </section>

      {/* Stats */}
      <section className={styles.statsSection}>
        <div className={styles.statsGrid}>
          <div className={styles.statCard}>
            <div className={styles.statLabel}>Total Blocks</div>
            <div className={styles.statValue}>{formatNumber(stats.totalBlocks)}</div>
          </div>
          <div className={styles.statCard}>
            <div className={styles.statLabel}>Total Transactions</div>
            <div className={styles.statValue}>{formatNumber(stats.totalTransactions)}</div>
          </div>
          <div className={styles.statCard}>
            <div className={styles.statLabel}>TPS</div>
            <div className={styles.statValue}>{stats.tps.toFixed(1)}</div>
          </div>
          <div className={styles.statCard}>
            <div className={styles.statLabel}>Avg Gas Price</div>
            <div className={styles.statValue}>{(stats.avgGasPrice / 1e9).toFixed(2)} Gwei</div>
          </div>
        </div>
      </section>

      {/* Latest Blocks */}
      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>Latest Blocks</h2>
        <div className={styles.tableContainer}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th>Block</th>
                <th>Hash</th>
                <th>Timestamp</th>
                <th>Transactions</th>
                <th>Gas Used</th>
                <th>Miner</th>
              </tr>
            </thead>
            <tbody>
              {loading ? (
                <tr><td colSpan={6}>Loading...</td></tr>
              ) : latestBlocks.length === 0 ? (
                <tr><td colSpan={6}>No blocks yet</td></tr>
              ) : (
                latestBlocks.map((block) => (
                  <tr key={block.number}>
                    <td><Link href={`/blocks/${block.number}`}>{block.number}</Link></td>
                    <td><code>{formatAddress(block.hash)}</code></td>
                    <td>{formatTime(block.timestamp)}</td>
                    <td>{block.transactions}</td>
                    <td>{(block.gasUsed / 1e6).toFixed(2)}M</td>
                    <td><code>{formatAddress(block.miner)}</code></td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      {/* Latest Transactions */}
      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>Latest Transactions</h2>
        <div className={styles.tableContainer}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th>Hash</th>
                <th>From</th>
                <th>To</th>
                <th>Value</th>
                <th>Gas Price</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {loading ? (
                <tr><td colSpan={6}>Loading...</td></tr>
              ) : latestTxs.length === 0 ? (
                <tr><td colSpan={6}>No transactions yet</td></tr>
              ) : (
                latestTxs.map((tx) => (
                  <tr key={tx.hash}>
                    <td><code>{formatAddress(tx.hash)}</code></td>
                    <td><code>{formatAddress(tx.from)}</code></td>
                    <td><code>{formatAddress(tx.to)}</code></td>
                    <td>{formatValue(tx.value)} TGR</td>
                    <td>{(parseFloat(tx.gasPrice) / 1e9).toFixed(2)} Gwei</td>
                    <td>
                      <span className={tx.status ? styles.statusSuccess : styles.statusFailed}>
                        {tx.status ? 'Success' : 'Failed'}
                      </span>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      {/* Footer */}
      <footer className={styles.footer}>
        <p>TigerScan © 2024 - TigerSmartChain Explorer</p>
        <p>Chain ID: 9001 | TGR: Tiger Coin</p>
      </footer>
    </div>
  )
}