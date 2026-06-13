// TigerScan - Cross-chain Portfolio & Bridge Tracking

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'

interface Chain { id: number; name: string; symbol: string; balance: number; usd_value: number }
interface BridgeTx { id: string; bridge: string; from_chain: string; to_chain: string; status: string; amount: string; time: string }

export default function CrossChain() {
  const [address, setAddress] = useState('')
  const [portfolio, setPortfolio] = useState<Chain[]>([])
  const [bridges, setBridges] = useState<BridgeTx[]>([])
  const [loading, setLoading] = useState(false)
  const [totalUsd, setTotalUsd] = useState(0)

  const chains = [
    { id: 1, name: 'Ethereum', symbol: 'ETH' },
    { id: 56, name: 'BNB Chain', symbol: 'BNB' },
    { id: 137, name: 'Polygon', symbol: 'MATIC' },
    { id: 42161, name: 'Arbitrum', symbol: 'ETH' },
    { id: 10, name: 'Optimism', symbol: 'ETH' },
  ]

  useEffect(() => { if (address) fetchData() }, [address])

  const fetchData = async () => {
    setLoading(true)
    try {
      const [portfolioRes, bridgesRes] = await Promise.all([
        fetch(`https://api.tigerscan.io/v1/crosschain/portfolio?address=${address}`),
        fetch(`https://api.tigerscan.io/v1/crosschain/bridges?address=${address}`)
      ])
      const portfolioData = await portfolioRes.json()
      const bridgesData = await bridgesRes.json()
      setPortfolio(portfolioData.chains || [])
      setBridges(bridgesData.transactions || [])
      setTotalUsd(portfolioData.total_usd || 0)
    } catch (e) { console.error(e) }
    finally { setLoading(false) }
  }

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'completed': return '#00cc88'
      case 'pending': return '#ffaa00'
      case 'failed': return '#ff3333'
      default: return '#888'
    }
  }

  return (
    <div style={styles.container}>
      <Head><title>Cross-chain Portfolio - TigerScan.io</title></Head>
      <header style={styles.header}>
        <Link href="/" style={styles.logo}>🐯 TigerScan</Link>
        <nav style={styles.nav}>
          <Link href="/">Home</Link>
          <Link href="/crosschain">Cross-chain</Link>
        </nav>
      </header>
      <main style={styles.main}>
        <h1 style={styles.title}>Cross-chain Portfolio</h1>
        
        <div style={styles.searchBox}>
          <input value={address} onChange={e => setAddress(e.target.value)} placeholder="Enter address to view cross-chain portfolio" style={styles.input} />
          <button onClick={fetchData} style={styles.button}>Search</button>
        </div>

        {loading ? <p>Loading...</p> : address && (
          <>
            <div style={styles.totalCard}>
              <span style={styles.totalLabel}>Total Portfolio Value</span>
              <span style={styles.totalValue}>${totalUsd.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}</span>
            </div>

            <section style={styles.section}>
              <h2>Chain Balances</h2>
              <div style={styles.chainGrid}>
                {chains.map(chain => {
                  const chainData = portfolio.find(p => p.id === chain.id)
                  return (
                    <div key={chain.id} style={styles.chainCard}>
                      <div style={styles.chainHeader}>
                        <span style={styles.chainName}>{chain.name}</span>
                        <span style={styles.chainSymbol}>{chain.symbol}</span>
                      </div>
                      <div style={styles.chainBalance}>
                        {chainData ? chainData.balance.toFixed(4) : '0.0000'}
                      </div>
                      <div style={styles.chainUsd}>
                        ${chainData ? chainData.usd_value.toFixed(2) : '0.00'}
                      </div>
                    </div>
                  )
                })}
              </div>
            </section>

            <section style={styles.section}>
              <h2>Bridge Transactions</h2>
              <div style={styles.txList}>
                {bridges.length === 0 ? <p style={styles.empty}>No bridge transactions</p> :
                  bridges.map(tx => (
                    <div key={tx.id} style={styles.txCard}>
                      <div style={styles.txHeader}>
                        <span style={styles.txBridge}>{tx.bridge}</span>
                        <span style={{...styles.txStatus, color: getStatusColor(tx.status)}}>{tx.status}</span>
                      </div>
                      <div style={styles.txRoute}>
                        {tx.from_chain} → {tx.to_chain}
                      </div>
                      <div style={styles.txAmount}>{tx.amount}</div>
                      <div style={styles.txTime}>{tx.time}</div>
                    </div>
                  ))
                }
              </div>
            </section>

            <section style={styles.section}>
              <h2>Bridge Routes</h2>
              <div style={styles.routeGrid}>
                <div style={styles.routeCard}>
                  <span>Stargate</span>
                  <span>ETH ↔ BNB</span>
                  <span style={styles.routeFee}>0.3% fee</span>
                </div>
                <div style={styles.routeCard}>
                  <span>Celer</span>
                  <span>ETH ↔ Polygon</span>
                  <span style={styles.routeFee}>0.5% fee</span>
                </div>
                <div style={styles.routeCard}>
                  <span>Across</span>
                  <span>ETH ↔ Arbitrum</span>
                  <span style={styles.routeFee}>0.2% fee</span>
                </div>
              </div>
            </section>
          </>
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
  main: { maxWidth: '1000px', margin: '0 auto', padding: '2rem' },
  title: { color: '#fff', fontSize: '2rem', marginBottom: '1.5rem' },
  searchBox: { display: 'flex', gap: '1rem', marginBottom: '2rem' },
  input: { flex: 1, padding: '0.75rem', borderRadius: '8px', border: '1px solid #333', background: '#1a1a24', color: '#fff' },
  button: { padding: '0.75rem 1.5rem', background: '#ff6b35', color: '#fff', border: 'none', borderRadius: '8px', cursor: 'pointer' },
  totalCard: { background: '#12121a', padding: '2rem', borderRadius: '12px', textAlign: 'center', marginBottom: '2rem' },
  totalLabel: { display: 'block', color: '#888', marginBottom: '0.5rem' },
  totalValue: { fontSize: '2.5rem', fontWeight: 'bold', color: '#ff6b35' },
  section: { background: '#12121a', padding: '1.5rem', borderRadius: '12px', marginBottom: '1.5rem' },
  chainGrid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))', gap: '1rem', marginTop: '1rem' },
  chainCard: { background: '#1a1a24', padding: '1rem', borderRadius: '8px' },
  chainHeader: { display: 'flex', justifyContent: 'space-between', marginBottom: '0.5rem' },
  chainName: { color: '#fff', fontWeight: 'bold' },
  chainSymbol: { color: '#888', fontSize: '0.85rem' },
  chainBalance: { color: '#fff', fontSize: '1.2rem' },
  chainUsd: { color: '#888', fontSize: '0.9rem' },
  txList: { display: 'flex', flexDirection: 'column', gap: '0.5rem' },
  empty: { color: '#666', fontStyle: 'italic' },
  txCard: { background: '#1a1a24', padding: '1rem', borderRadius: '8px' },
  txHeader: { display: 'flex', justifyContent: 'space-between', marginBottom: '0.5rem' },
  txBridge: { color: '#ff6b35', fontWeight: 'bold' },
  txStatus: { fontWeight: 'bold', textTransform: 'capitalize' },
  txRoute: { color: '#888', fontSize: '0.9rem' },
  txAmount: { color: '#fff', marginTop: '0.25rem' },
  txTime: { color: '#666', fontSize: '0.8rem', marginTop: '0.25rem' },
  routeGrid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))', gap: '1rem', marginTop: '1rem' },
  routeCard: { background: '#1a1a24', padding: '1rem', borderRadius: '8px', display: 'flex', flexDirection: 'column', gap: '0.25rem' },
  routeFee: { color: '#00cc88', fontSize: '0.85rem' },
}