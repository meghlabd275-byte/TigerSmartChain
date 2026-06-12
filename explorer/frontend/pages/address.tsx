// TigerScan - Address Detail Page
// Production-ready address explorer with balance, transactions, tokens, and NFTs

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'
import { useRouter } from 'next/router'

interface Address {
  address: string
  balance: string
  nonce: number
  isContract: boolean
  isVerified: boolean
  contractName?: string
  tokenStandard?: string
  totalTxCount: number
  tokenCount: number
  nftCount: number
  firstTxBlock?: number
  lastTxBlock?: number
}

interface Transaction {
  hash: string
  blockNumber: number
  from: string
  to: string
  value: string
  gasPrice: number
  status: boolean
  timestamp: number
}

interface Token {
  address: string
  name: string
  symbol: string
  balance: string
  decimals: number
}

interface NFT {
  collectionAddress: string
  tokenId: string
  name: string
  imageUrl: string
}

interface BalanceHistory {
  blockNumber: number
  balance: string
  timestamp: number
}

export default function AddressPage({ address: initialAddr }: { address: Address | null }) {
  const router = useRouter()
  const [address, setAddress] = useState<Address | null>(initialAddr)
  const [loading, setLoading] = useState(!initialAddr)
  const [error, setError] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<'transactions' | 'tokens' | 'nfts' | 'analytics'>('transactions')
  const [txs, setTxs] = useState<Transaction[]>([])
  const [tokens, setTokens] = useState<Token[]>([])
  const [nfts, setNfts] = useState<NFT[]>([])
  const [balanceHistory, setBalanceHistory] = useState<BalanceHistory[]>([])

  useEffect(() => {
    if (!initialAddr && router.query.address) {
      fetchAddress()
    }
  }, [router.query.address])

  async function fetchAddress() {
    setLoading(true)
    setError(null)
    
    try {
      const [addrRes, txsRes, tokensRes, nftsRes, historyRes] = await Promise.all([
        fetch(`/api/v1/accounts/${router.query.address}`),
        fetch(`/api/v1/transactions?address=${router.query.address}&limit=50`),
        fetch(`/api/v1/tokens?holder=${router.query.address}&limit=20`),
        fetch(`/api/v1/nfts?owner=${router.query.address}&limit=20`),
        fetch(`/api/v1/accounts/${router.query.address}/balance_history?limit=30`)
      ])
      
      if (!addrRes.ok) throw new Error('Address not found')
      
      const [addrData, txsData, tokensData, nftsData, historyData] = await Promise.all([
        addrRes.json(),
        txsRes.json(),
        tokensRes.json(),
        nftsRes.json(),
        historyRes.json()
      ])
      
      setAddress(addrData)
      setTxs(txsData)
      setTokens(tokensData)
      setNfts(nftsData)
      setBalanceHistory(historyData)
    } catch (err: any) {
      setError(err.message || 'Failed to load address')
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

  function formatTokenBalance(balance: string, decimals: number): string {
    const num = parseFloat(balance)
    return (num / Math.pow(10, decimals)).toFixed(6)
  }

  if (loading) return <div style={styles.container}><div style={styles.loading}>Loading address...</div></div>
  if (error || !address) return <div style={styles.container}><div style={styles.error}><h2>Address Not Found</h2><p>{error}</p><Link href="/">Go Home</Link></div></div>

  return (
    <div style={styles.container}>
      <Head>
        <title>{formatAddress(address.address)} | TigerScan.io</title>
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
        <Link href="/">Home</Link> / <Link href="/transactions">Transactions</Link> / {formatAddress(address.address)}
      </div>

      {/* Title */}
      <div style={styles.titleSection}>
        <div style={styles.addressInfo}>
          <h1 style={styles.title}>
            {address.isContract ? (
              <span style={styles.contractBadge}>Contract</span>
            ) : (
              <span style={styles.walletBadge}>Wallet</span>
            )}
            {formatAddress(address.address)}
          </h1>
          {address.isVerified && address.contractName && (
            <p style={styles.contractName}>{address.contractName}</p>
          )}
        </div>
      </div>

      {/* Balance Card */}
      <div style={styles.balanceCard}>
        <div style={styles.balanceLabel}>Balance</div>
        <div style={styles.balanceValue}>{formatValue(address.balance)} TGR</div>
        <div style={styles.balanceUsd}>${(parseFloat(address.balance) / 1e18 * 0.001).toFixed(2)} USD</div>
        <div style={styles.txCounts}>
          {address.totalTxCount} transactions | {address.firstTxBlock ? `#${address.firstTxBlock}` : 'N/A'} - {address.lastTxBlock ? `#${address.lastTxBlock}` : 'N/A'}
        </div>
      </div>

      {/* Tabs */}
      <div style={styles.tabs}>
        <button style={activeTab === 'transactions' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('transactions')}>
          Transactions {address.totalTxCount ? `(${address.totalTxCount})` : ''}
        </button>
        <button style={activeTab === 'tokens' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('tokens')}>
          Tokens {address.tokenCount ? `(${address.tokenCount})` : ''}
        </button>
        <button style={activeTab === 'nfts' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('nfts')}>
          NFTs {address.nftCount ? `(${address.nftCount})` : ''}
        </button>
        <button style={activeTab === 'analytics' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('analytics')}>
          Analytics
        </button>
      </div>

      {/* Content */}
      {activeTab === 'transactions' && (
        <section style={styles.section}>
          {txs.length === 0 ? (
            <p style={styles.empty}>No transactions found</p>
          ) : (
            <table style={styles.table}>
              <thead>
                <tr>
                  <th style={styles.th}>Tx Hash</th>
                  <th style={styles.th}>Block</th>
                  <th style={styles.th}>From</th>
                  <th style={styles.th}>To</th>
                  <th style={styles.th}>Value</th>
                  <th style={styles.th}>Status</th>
                </tr>
              </thead>
              <tbody>
                {txs.map((tx) => (
                  <tr key={tx.hash}>
                    <td style={styles.td}><Link href={`/tx/${tx.hash}`} style={styles.txHash}>{formatAddress(tx.hash)}</Link></td>
                    <td style={styles.td}><Link href={`/blocks/${tx.blockNumber}`} style={styles.link}>#{tx.blockNumber}</Link></td>
                    <td style={styles.td}>
                      {tx.from === address.address ? (
                        <span style={styles.outgoing}>↓ {formatAddress(tx.to)}</span>
                      ) : (
                        <span style={styles.incoming}>↑ {formatAddress(tx.from)}</span>
                      )}
                    </td>
                    <td style={styles.td}>{formatValue(tx.value)} TGR</td>
                    <td style={styles.td}><span style={tx.status ? styles.success : styles.failure}>{tx.status ? '✓' : '✗'}</span></td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      )}

      {activeTab === 'tokens' && (
        <section style={styles.section}>
          {tokens.length === 0 ? (
            <p style={styles.empty}>No token holdings</p>
          ) : (
            <table style={styles.table}>
              <thead>
                <tr>
                  <th style={styles.th}>Token</th>
                  <th style={styles.th}>Symbol</th>
                  <th style={styles.th}>Balance</th>
                  <th style={styles.th}>Value</th>
                </tr>
              </thead>
              <tbody>
                {tokens.map((token) => (
                  <tr key={token.address}>
                    <td style={styles.td}>
                      <Link href={`/token/${token.address}`} style={styles.tokenLink}>
                        {token.name}
                      </Link>
                    </td>
                    <td style={styles.td}>{token.symbol}</td>
                    <td style={styles.td}>{formatTokenBalance(token.balance, token.decimals)}</td>
                    <td style={styles.td}>-</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      )}

      {activeTab === 'nfts' && (
        <section style={styles.section}>
          {nfts.length === 0 ? (
            <p style={styles.empty}>No NFT holdings</p>
          ) : (
            <div style={styles.nftGrid}>
              {nfts.map((nft) => (
                <div key={`${nft.collectionAddress}-${nft.tokenId}`} style={styles.nftCard}>
                  {nft.imageUrl && <img src={nft.imageUrl} alt={nft.name} style={styles.nftImage} />}
                  <div style={styles.nftName}>{nft.name}</div>
                  <div style={styles.nftId}>#{nft.tokenId}</div>
                </div>
              ))}
            </div>
          )}
        </section>
      )}

      {activeTab === 'analytics' && (
        <section style={styles.section}>
          <div style={styles.analyticsGrid}>
            <div style={styles.card}>
              <label style={styles.label}>Total Transactions</label>
              <span style={styles.value}>{address.totalTxCount}</span>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>Tokens Held</label>
              <span style={styles.value}>{address.tokenCount}</span>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>NFTs Held</label>
              <span style={styles.value}>{address.nftCount}</span>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>First Transaction</label>
              <span style={styles.value}>#{address.firstTxBlock || 'N/A'}</span>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>Last Transaction</label>
              <span style={styles.value}>#{address.lastTxBlock || 'N/A'}</span>
            </div>
            <div style={styles.card}>
              <label style={styles.label}>Nonce</label>
              <span style={styles.value}>{address.nonce}</span>
            </div>
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
  titleSection: { maxWidth: '1200px', margin: '0 auto 1rem', padding: '0 1rem' },
  addressInfo: { display: 'flex', flexDirection: 'column' as const, gap: '0.25rem' },
  title: { display: 'flex', alignItems: 'center', gap: '0.5rem', fontSize: '1.25rem', fontFamily: 'monospace' },
  contractBadge: { padding: '0.25rem 0.5rem', background: '#6366f1', color: 'white', borderRadius: '4px', fontSize: '0.75rem', fontFamily: 'system-ui' },
  walletBadge: { padding: '0.25rem 0.5rem', background: '#22c55e', color: 'white', borderRadius: '4px', fontSize: '0.75rem', fontFamily: 'system-ui' },
  contractName: { fontSize: '0.875rem', color: '#666' },
  balanceCard: { maxWidth: '1200px', margin: '0 auto 1rem', padding: '1.5rem', background: 'linear-gradient(135deg, #1a1a2e 0%, #16213e 100%)', borderRadius: '12px', color: 'white' },
  balanceLabel: { fontSize: '0.75rem', color: '#9ca3af', marginBottom: '0.25rem' },
  balanceValue: { fontSize: '2rem', fontWeight: 'bold', marginBottom: '0.25rem' },
  balanceUsd: { fontSize: '0.875rem', color: '#9ca3af', marginBottom: '0.5rem' },
  txCounts: { fontSize: '0.75rem', color: '#9ca3af' },
  tabs: { maxWidth: '1200px', margin: '0 auto', padding: '0 1rem', display: 'flex', gap: '0.5rem', borderBottom: '1px solid #e5e5e5' },
  tab: { padding: '0.75rem 1rem', background: 'none', border: 'none', borderBottom: '2px solid transparent', cursor: 'pointer', fontSize: '0.875rem', color: '#666' },
  activeTab: { padding: '0.75rem 1rem', background: 'none', border: 'none', borderBottom: '2px solid #ff6b35', cursor: 'pointer', fontSize: '0.875rem', color: '#ff6b35', fontWeight: '600' },
  section: { maxWidth: '1200px', margin: '1rem auto', padding: '0 1rem' },
  grid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: '1rem' },
  card: { background: 'white', padding: '1rem', borderRadius: '8px', boxShadow: '0 1px 3px rgba(0,0,0,0.1)' },
  label: { display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: '0.25rem', textTransform: 'uppercase' },
  value: { fontSize: '1.25rem', color: '#1a1a2e' },
  table: { width: '100%', background: 'white', borderRadius: '8px', overflow: 'hidden', borderCollapse: 'collapse' },
  th: { padding: '0.75rem', textAlign: 'left', background: '#f5f6fa', fontSize: '0.75rem', color: '#666', fontWeight: '600' },
  td: { padding: '0.75rem', borderTop: '1px solid #eee', fontSize: '0.875rem' },
  txHash: { color: '#ff6b35', textDecoration: 'none', fontFamily: 'monospace' },
  link: { color: '#ff6b35', textDecoration: 'none' },
  tokenLink: { color: '#ff6b35', textDecoration: 'none', fontWeight: '500' },
  outgoing: { color: '#ef4444' },
  incoming: { color: '#22c55e' },
  success: { color: '#22c55e' },
  failure: { color: '#ef4444' },
  nftGrid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))', gap: '1rem' },
  nftCard: { background: 'white', padding: '1rem', borderRadius: '8px', boxShadow: '0 1px 3px rgba(0,0,0,0.1)' },
  nftImage: { width: '100%', aspectRatio: '1', objectFit: 'cover', borderRadius: '8px', marginBottom: '0.5rem' },
  nftName: { fontSize: '0.875rem', fontWeight: '500', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' },
  nftId: { fontSize: '0.75rem', color: '#666' },
  analyticsGrid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(150px, 1fr))', gap: '1rem' },
  loading: { maxWidth: '1200px', margin: '2rem auto', padding: '2rem', textAlign: 'center', background: 'white', borderRadius: '8px' },
  error: { maxWidth: '1200px', margin: '2rem auto', padding: '2rem', textAlign: 'center', background: 'white', borderRadius: '8px' },
  empty: { padding: '2rem', textAlign: 'center', color: '#666', background: 'white', borderRadius: '8px' },
  footer: { textAlign: 'center', padding: '2rem', color: '#666', marginTop: '3rem' }
}