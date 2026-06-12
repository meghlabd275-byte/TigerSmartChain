// TigerScan - NFT Collection & Detail Page
// Production-ready NFT explorer with collections, metadata, and analytics

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'
import { useRouter } from 'next/router'

interface NFTCollection {
  address: string
  name: string
  symbol: string
  type: string
  totalSupply: number
  holderCount: number
  transferCount: number
  floorPrice: number
  floorPriceUsd: number
  volume24h: number
  volume24hUsd: number
  sales24h: number
  isVerified: boolean
  description: string
  imageUrl: string
  bannerUrl: string
  externalUrl: string
  socialLinks: {
    twitter?: string
    discord?: string
    website?: string
  }
}

interface NFT {
  collectionAddress: string
  tokenId: string
  owner: string
  name: string
  description: string
  imageUrl: string
  animationUrl: string
  externalUrl: string
  metadata: Record<string, any>
  attributes: NFTAttribute[]
  transferCount: number
  lastSalePrice: number
}

interface NFTAttribute {
  trait_type: string
  value: string
  rarity?: number
}

interface Transfer {
  hash: string
  from: string
  to: string
  tokenId: string
  amount: number
  blockNumber: number
  timestamp: number
  price?: number
}

export default function NFTPage() {
  const router = useRouter()
  const { address, tokenId } = router.query
  const [collection, setCollection] = useState<NFTCollection | null>(null)
  const [nft, setNft] = useState<NFT | null>(null)
  const [nfts, setNfts] = useState<NFT[]>([])
  const [transfers, setTransfers] = useState<Transfer[]>([])
  const [loading, setLoading] = useState(true)
  const [activeTab, setActiveTab] = useState<'items' | 'holders' | 'transfers'>('items')

  useEffect(() => {
    if (address) {
      fetchCollection()
      if (tokenId) {
        fetchNFT()
      }
    }
  }, [address, tokenId])

  async function fetchCollection() {
    try {
      const [colRes, nftsRes, transfersRes] = await Promise.all([
        fetch(`/api/v1/nfts/collections/${address}`),
        fetch(`/api/v1/nfts/collections/${address}/items?limit=20`),
        fetch(`/api/v1/nfts/collections/${address}/transfers?limit=20`)
      ])
      
      const [colData, nftsData, transfersData] = await Promise.all([
        colRes.json(),
        nftsRes.json(),
        transfersRes.json()
      ])
      
      setCollection(colData.result || colData)
      setNfts(nftsData.result || nftsData)
      setTransfers(transfersData.result || transfersData)
    } catch (err) {
      console.error(err)
    } finally {
      setLoading(false)
    }
  }

  async function fetchNFT() {
    try {
      const [nftRes, transfersRes] = await Promise.all([
        fetch(`/api/v1/nfts/collections/${address}/${tokenId}`),
        fetch(`/api/v1/nfts/collections/${address}/${tokenId}/transfers?limit=10`)
      ])
      
      const [nftData, transfersData] = await Promise.all([
        nftRes.json(),
        transfersRes.json()
      ])
      
      setNft(nftData.result || nftData)
      setTransfers(transfersData.result || transfersData)
    } catch (err) {
      console.error(err)
    }
  }

  function formatAddress(addr: string): string {
    if (!addr) return ''
    return `${addr.slice(0, 6)}...${addr.slice(-4)}`
  }

  function formatPrice(usd: number): string {
    if (!usd) return 'N/A'
    if (usd >= 1000) return `$${usd.toLocaleString()}`
    return `$${usd.toFixed(2)}`
  }

  // Collection View
  if (address && !tokenId) {
    if (loading) {
      return <div style={styles.container}><div style={styles.loading}>Loading collection...</div></div>
    }

    return (
      <div style={styles.container}>
        <Head><title>{collection?.name || 'NFT Collection'} | TigerScan.io</title></Head>
        
        {/* Header */}
        <header style={styles.header}>
          <div style={styles.headerContent}>
            <Link href="/" style={styles.logo}>🐯 TigerScan.io</Link>
            <nav style={styles.nav}>
              <Link href="/blocks">Blocks</Link>
              <Link href="/tokens">Tokens</Link>
              <Link href="/nfts">NFTs</Link>
            </nav>
          </div>
        </header>

        {/* Banner */}
        {collection?.bannerUrl && (
          <div style={styles.banner}><img src={collection.bannerUrl} alt="" style={styles.bannerImg} /></div>
        )}

        {/* Collection Info */}
        <div style={styles.collectionHeader}>
          {collection?.imageUrl && (
            <img src={collection.imageUrl} alt={collection.name} style={styles.collectionIcon} />
          )}
          <div style={styles.collectionInfo}>
            <h1 style={styles.title}>
              {collection?.name}
              {collection?.isVerified && <span style={styles.verified}>✓</span>}
            </h1>
            <p style={styles.symbol}>{collection?.symbol} • {collection?.type}</p>
          </div>
        </div>

        {/* Stats */}
        <div style={styles.statsGrid}>
          <div style={styles.statCard}>
            <label style={styles.label}>Floor Price</label>
            <span style={styles.value}>{formatPrice(collection?.floorPriceUsd || 0)}</span>
          </div>
          <div style={styles.statCard}>
            <label style={styles.label}>24h Volume</label>
            <span style={styles.value}>{formatPrice(collection?.volume24hUsd || 0)}</span>
          </div>
          <div style={styles.statCard}>
            <label style={styles.label}>24h Sales</label>
            <span style={styles.value}>{collection?.sales24h || 0}</span>
          </div>
          <div style={styles.statCard}>
            <label style={styles.label}>Total Supply</label>
            <span style={styles.value}>{collection?.totalSupply?.toLocaleString() || 0}</span>
          </div>
          <div style={styles.statCard}>
            <label style={styles.label}>Holders</label>
            <span style={styles.value}>{collection?.holderCount?.toLocaleString() || 0}</span>
          </div>
          <div style={styles.statCard}>
            <label style={styles.label}>Transfers</label>
            <span style={styles.value}>{collection?.transferCount?.toLocaleString() || 0}</span>
          </div>
        </div>

        {/* Tabs */}
        <div style={styles.tabs}>
          <button style={activeTab === 'items' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('items')}>
            Items ({collection?.totalSupply?.toLocaleString() || 0})
          </button>
          <button style={activeTab === 'holders' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('holders')}>
            Holders
          </button>
          <button style={activeTab === 'transfers' ? styles.activeTab : styles.tab} onClick={() => setActiveTab('transfers')}>
            Transfers
          </button>
        </div>

        {/* Content */}
        {activeTab === 'items' && (
          <section style={styles.section}>
            {nfts.length === 0 ? (
              <p style={styles.empty}>No NFTs found</p>
            ) : (
              <div style={styles.nftGrid}>
                {nfts.map((nft) => (
                  <Link key={`${nft.collectionAddress}-${nft.tokenId}`} 
                    href={`/nft/${nft.collectionAddress}/${nft.tokenId}`} 
                    style={styles.nftCardLink}>
                    <div style={styles.nftCard}>
                      {nft.imageUrl && <img src={nft.imageUrl} alt={nft.name} style={styles.nftImage} />}
                      <div style={styles.nftInfo}>
                        <div style={styles.nftName}>{nft.name || `#${nft.tokenId}`}</div>
                      </div>
                    </div>
                  </Link>
                ))}
              </div>
            )}
          </section>
        )}

        {activeTab === 'transfers' && (
          <section style={styles.section}>
            {transfers.length === 0 ? (
              <p style={styles.empty}>No transfers</p>
            ) : (
              <table style={styles.table}>
                <thead>
                  <tr>
                    <th style={styles.th}>Tx Hash</th>
                    <th style={styles.th}>From</th>
                    <th style={styles.th}>To</th>
                    <th style={styles.th}>Token ID</th>
                    <th style={styles.th}>Block</th>
                  </tr>
                </thead>
                <tbody>
                  {transfers.map((t) => (
                    <tr key={t.hash}>
                      <td style={styles.td}>
                        <Link href={`/tx/${t.hash}`} style={styles.link}>{formatAddress(t.hash)}</Link>
                      </td>
                      <td style={styles.td}>
                        <Link href={`/address/${t.from}`} style={styles.link}>{formatAddress(t.from)}</Link>
                      </td>
                      <td style={styles.td}>
                        <Link href={`/address/${t.to}`} style={styles.link}>{formatAddress(t.to)}</Link>
                      </td>
                      <td style={styles.td}>{t.tokenId}</td>
                      <td style={styles.td}>
                        <Link href={`/blocks/${t.blockNumber}`} style={styles.link}>#{t.blockNumber}</Link>
                      </td>
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

  // Single NFT View
  if (tokenId && nft) {
    return (
      <div style={styles.container}>
        <Head><title>{nft.name || `#${tokenId}`} | TigerScan.io</title></Head>
        
        <header style={styles.header}>
          <div style={styles.headerContent}>
            <Link href="/" style={styles.logo}>🐯 TigerScan.io</Link>
            <nav style={styles.nav}>
              <Link href={`/nft/${address}`}>← Back to Collection</Link>
            </nav>
          </div>
        </header>

        <div style={styles.nftDetailGrid}>
          <div style={styles.nftMedia}>
            {nft.imageUrl && <img src={nft.imageUrl} alt={nft.name} style={styles.nftDetailImage} />}
            {nft.animationUrl && (
              <video src={nft.animationUrl} controls style={styles.nftVideo} />
            )}
          </div>
          
          <div style={styles.nftDetailInfo}>
            <h1 style={styles.title}>{nft.name || `#${tokenId}`}</h1>
            <p style={styles.tokenId}>Token ID: {tokenId}</p>
            
            <div style={styles.detailCard}>
              <label style={styles.label}>Owner</label>
              <Link href={`/address/${nft.owner}`} style={styles.link}>{formatAddress(nft.owner)}</Link>
            </div>

            {nft.attributes && nft.attributes.length > 0 && (
              <div style={styles.attributes}>
                <label style={styles.label}>Attributes</label>
                <div style={styles.attributeGrid}>
                  {nft.attributes.map((attr, i) => (
                    <div key={i} style={styles.attribute}>
                      <span style={styles.attrType}>{attr.trait_type}</span>
                      <span style={styles.attrValue}>{attr.value}</span>
                      {attr.rarity && <span style={styles.attrRarity}>{attr.rarity}%</span>}
                    </div>
                  ))}
                </div>
              </div>
            )}

            {nft.description && (
              <div style={styles.detailCard}>
                <label style={styles.label}>Description</label>
                <p style={styles.description}>{nft.description}</p>
              </div>
            )}
          </div>
        </div>

        <footer style={styles.footer}>
          <p>TigerScan.io © 2024</p>
        </footer>
      </div>
    )
  }

  return <div style={styles.container}><div style={styles.loading}>Loading...</div></div>
}

const styles = {
  container: { fontFamily: 'system-ui, sans-serif', minHeight: '100vh', background: '#f5f6fa' },
  header: { background: '#1a1a2e', padding: '1rem' },
  headerContent: { maxWidth: '1200px', margin: '0 auto', display: 'flex', justifyContent: 'space-between', alignItems: 'center' },
  logo: { color: '#ff6b35', fontSize: '1.5rem', fontWeight: 'bold', textDecoration: 'none' },
  nav: { display: 'flex', gap: '1.5rem' },
  banner: { width: '100%', height: '200px', overflow: 'hidden' },
  bannerImg: { width: '100%', height: '100%', objectFit: 'cover' },
  collectionHeader: { maxWidth: '1200px', margin: '0 auto', padding: '1rem', display: 'flex', gap: '1rem', alignItems: 'center' },
  collectionIcon: { width: '80px', height: '80px', borderRadius: '12px', objectFit: 'cover' },
  collectionInfo: { flex: 1 },
  title: { display: 'flex', alignItems: 'center', gap: '0.5rem', fontSize: '1.5rem', marginBottom: '0.25rem' },
  verified: { display: 'inline-flex', alignItems: 'center', justifyContent: 'center', width: '20px', height: '20px', background: '#22c55e', color: 'white', borderRadius: '50%', fontSize: '0.75rem' },
  symbol: { color: '#666' },
  statsGrid: { maxWidth: '1200px', margin: '0 auto', padding: '0 1rem', display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(150px, 1fr))', gap: '1rem' },
  statCard: { background: 'white', padding: '1rem', borderRadius: '8px', boxShadow: '0 1px 3px rgba(0,0,0,0.1)' },
  label: { display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: '0.25rem', textTransform: 'uppercase' as const },
  value: { fontSize: '1.25rem', fontWeight: '600', color: '#1a1a2e' },
  tabs: { maxWidth: '1200px', margin: '1rem auto', padding: '0 1rem', display: 'flex', gap: '0.5rem', borderBottom: '1px solid #e5e5e5' },
  tab: { padding: '0.75rem 1rem', background: 'none', border: 'none', borderBottom: '2px solid transparent', cursor: 'pointer', fontSize: '0.875rem', color: '#666' },
  activeTab: { padding: '0.75rem 1rem', background: 'none', border: 'none', borderBottom: '2px solid #ff6b35', cursor: 'pointer', fontSize: '0.875rem', color: '#ff6b35', fontWeight: '600' as const },
  section: { maxWidth: '1200px', margin: '1rem auto', padding: '0 1rem' },
  nftGrid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))', gap: '1rem' },
  nftCardLink: { textDecoration: 'none' },
  nftCard: { background: 'white', borderRadius: '8px', overflow: 'hidden', boxShadow: '0 1px 3px rgba(0,0,0,0.1)', transition: 'transform 0.2s', cursor: 'pointer' },
  nftImage: { width: '100%', aspectRatio: '1', objectFit: 'cover' },
  nftInfo: { padding: '0.75rem' },
  nftName: { fontSize: '0.875rem', fontWeight: '500', color: '#1a1a2e', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' as const },
  table: { width: '100%', background: 'white', borderRadius: '8px', overflow: 'hidden', borderCollapse: 'collapse' },
  th: { padding: '0.75rem', textAlign: 'left', background: '#f5f6fa', fontSize: '0.75rem', color: '#666', fontWeight: '600' as const },
  td: { padding: '0.75rem', borderTop: '1px solid #eee', fontSize: '0.875rem' },
  link: { color: '#ff6b35', textDecoration: 'none' },
  empty: { padding: '2rem', textAlign: 'center', color: '#666', background: 'white', borderRadius: '8px' },
  loading: { maxWidth: '1200px', margin: '2rem auto', padding: '2rem', textAlign: 'center', background: 'white', borderRadius: '8px' },
  nftDetailGrid: { maxWidth: '1200px', margin: '1rem auto', padding: '1rem', display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '2rem' },
  nftMedia: { background: 'white', borderRadius: '12px', padding: '1rem' },
  nftDetailImage: { width: '100%', borderRadius: '8px' },
  nftVideo: { width: '100%', marginTop: '1rem', borderRadius: '8px' },
  nftDetailInfo: { background: 'white', borderRadius: '12px', padding: '1.5rem' },
  tokenId: { color: '#666', marginBottom: '1rem' },
  detailCard: { marginBottom: '1rem' },
  description: { color: '#666', lineHeight: 1.6 },
  attributes: { marginBottom: '1rem' },
  attributeGrid: { display: 'flex', flexWrap: 'wrap', gap: '0.5rem' },
  attribute: { padding: '0.5rem', background: '#f5f6fa', borderRadius: '8px', display: 'flex', flexDirection: 'column' as const },
  attrType: { fontSize: '0.75rem', color: '#666' },
  attrValue: { fontSize: '0.875rem', fontWeight: '500', color: '#1a1a2e' },
  attrRarity: { fontSize: '0.75rem', color: '#ff6b35' },
  footer: { textAlign: 'center', padding: '2rem', color: '#666', marginTop: '3rem' }
}