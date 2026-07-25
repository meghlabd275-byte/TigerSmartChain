/**
 * NFT Rarity Page
 * Complete frontend for NFT rarity scoring and analysis
 */

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'

interface RarityScore {
  token_id: string
  total_score: number
  rank: number
  percentile: number
  trait_rarity: number
  statistical_rarity: number
}

interface CollectionStats {
  total_supply: number
  floor_price: number
  average_price: number
  volume_24h: number
  holders: number
}

interface TraitDistribution {
  trait_type: string
  value: string
  count: number
  percentage: number
  rarity_score: number
}

export default function NFTRarityPage() {
  const [collectionAddress, setCollectionAddress] = useState('')
  const [rarityScores, setRarityScores] = useState<RarityScore[]>([])
  const [collectionStats, setCollectionStats] = useState<CollectionStats | null>(null)
  const [traitDistributions, setTraitDistributions] = useState<TraitDistribution[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<'overview' | 'traits' | 'rankings'>('overview')

  useEffect(() => {
    if (collectionAddress) {
      fetchRarityData()
    }
  }, [collectionAddress])

  async function fetchRarityData() {
    setLoading(true)
    setError(null)
    
    try {
      // Fetch collection stats
      const statsRes = await fetch(`/api/v1/nfts/${collectionAddress}/stats`)
      if (statsRes.ok) {
        const statsData = await statsRes.json()
        setCollectionStats(statsData)
      }
      
      // Fetch rarity scores
      const rarityRes = await fetch(`/api/v1/nfts/${collectionAddress}/rarity`)
      if (rarityRes.ok) {
        const rarityData = await rarityRes.json()
        setRarityScores(rarityData.rarity_scores || [])
      }
      
      // Fetch trait distributions
      const traitsRes = await fetch(`/api/v1/nfts/${collectionAddress}/traits`)
      if (traitsRes.ok) {
        const traitsData = await traitsRes.json()
        setTraitDistributions(traitsData.traits || [])
      }
    } catch (err: any) {
      setError(err.message || 'Failed to load rarity data')
    } finally {
      setLoading(false)
    }
  }

  function getScoreColor(score: number): string {
    if (score >= 90) return 'text-purple-600'
    if (score >= 70) return 'text-blue-600'
    if (score >= 50) return 'text-green-600'
    if (score >= 30) return 'text-yellow-600'
    return 'text-red-600'
  }

  function getRankBadge(rank: number, total: number): string {
    const percentile = (rank / total) * 100
    if (percentile <= 1) return 'bg-purple-100 text-purple-800'
    if (percentile <= 5) return 'bg-blue-100 text-blue-800'
    if (percentile <= 10) return 'bg-green-100 text-green-800'
    return 'bg-gray-100 text-gray-800'
  }

  return (
    <>
      <Head>
        <title>NFT Rarity | TigerScan</title>
      </Head>
      
      <div className="min-h-screen bg-gray-50">
        {/* Header */}
        <header className="bg-white shadow">
          <div className="max-w-7xl mx-auto py-6 px-4 sm:px-6 lg:px-8">
            <div className="flex items-center justify-between">
              <h1 className="text-3xl font-bold text-gray-900">
                NFT Rarity Analyzer
              </h1>
              <Link href="/" className="text-blue-600 hover:text-blue-800">
                ← Back to Home
              </Link>
            </div>
          </div>
        </header>

        <main className="max-w-7xl mx-auto py-6 px-4 sm:px-6 lg:px-8">
          {/* Search */}
          <div className="mb-8">
            <label className="block text-sm font-medium text-gray-700 mb-2">
              Collection Address
            </label>
            <div className="flex gap-4">
              <input
                type="text"
                value={collectionAddress}
                onChange={(e) => setCollectionAddress(e.target.value)}
                placeholder="0x..."
                className="flex-1 px-4 py-2 border border-gray-300 rounded-md focus:ring-2 focus:ring-blue-500"
              />
              <button
                onClick={fetchRarityData}
                disabled={loading || !collectionAddress}
                className="px-6 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50"
              >
                {loading ? 'Loading...' : 'Analyze'}
              </button>
            </div>
          </div>

          {error && (
            <div className="mb-4 p-4 bg-red-100 border border-red-400 text-red-700 rounded">
              {error}
            </div>
          )}

          {collectionStats && (
            <>
              {/* Stats Cards */}
              <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-8">
                <div className="bg-white p-6 rounded-lg shadow">
                  <div className="text-sm text-gray-500">Total Supply</div>
                  <div className="text-2xl font-bold">{collectionStats.total_supply.toLocaleString()}</div>
                </div>
                <div className="bg-white p-6 rounded-lg shadow">
                  <div className="text-sm text-gray-500">Floor Price</div>
                  <div className="text-2xl font-bold">${collectionStats.floor_price.toFixed(2)}</div>
                </div>
                <div className="bg-white p-6 rounded-lg shadow">
                  <div className="text-sm text-gray-500">Avg Price</div>
                  <div className="text-2xl font-bold">${collectionStats.average_price.toFixed(2)}</div>
                </div>
                <div className="bg-white p-6 rounded-lg shadow">
                  <div className="text-sm text-gray-500">24h Volume</div>
                  <div className="text-2xl font-bold">${collectionStats.volume_24h.toFixed(2)}</div>
                </div>
              </div>

              {/* Tabs */}
              <div className="border-b border-gray-200 mb-6">
                <nav className="-mb-px flex space-x-8">
                  {['overview', 'traits', 'rankings'].map((tab) => (
                    <button
                      key={tab}
                      onClick={() => setActiveTab(tab as any)}
                      className={`py-4 px-1 border-b-2 font-medium text-sm ${
                        activeTab === tab
                          ? 'border-blue-500 text-blue-600'
                          : 'border-transparent text-gray-500 hover:text-gray-700'
                      }`}
                    >
                      {tab.charAt(0).toUpperCase() + tab.slice(1)}
                    </button>
                  ))}
                </nav>
              </div>

              {/* Overview Tab */}
              {activeTab === 'overview' && (
                <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                  {/* Top Rarest */}
                  <div className="bg-white p-6 rounded-lg shadow">
                    <h3 className="text-lg font-medium mb-4">Top 10 Rarest NFTs</h3>
                    <div className="space-y-3">
                      {rarityScores.slice(0, 10).map((score, idx) => (
                        <div key={score.token_id} className="flex items-center justify-between p-3 bg-gray-50 rounded">
                          <div className="flex items-center gap-3">
                            <span className="text-sm text-gray-500">#{idx + 1}</span>
                            <Link href={`/nft/${collectionAddress}/${score.token_id}`} className="text-blue-600 hover:underline">
                              #{score.token_id}
                            </Link>
                          </div>
                          <span className={`font-bold ${getScoreColor(score.total_score)}`}>
                            {score.total_score.toFixed(2)}
                          </span>
                        </div>
                      ))}
                    </div>
                  </div>

                  {/* Rarity Distribution */}
                  <div className="bg-white p-6 rounded-lg shadow">
                    <h3 className="text-lg font-medium mb-4">Score Distribution</h3>
                    <div className="space-y-2">
                      {[
                        { range: '90-100', count: rarityScores.filter(s => s.total_score >= 90).length, color: 'bg-purple-500' },
                        { range: '70-89', count: rarityScores.filter(s => s.total_score >= 70 && s.total_score < 90).length, color: 'bg-blue-500' },
                        { range: '50-69', count: rarityScores.filter(s => s.total_score >= 50 && s.total_score < 70).length, color: 'bg-green-500' },
                        { range: '30-49', count: rarityScores.filter(s => s.total_score >= 30 && s.total_score < 50).length, color: 'bg-yellow-500' },
                        { range: '0-29', count: rarityScores.filter(s => s.total_score < 30).length, color: 'bg-red-500' },
                      ].map((item) => (
                        <div key={item.range} className="flex items-center gap-3">
                          <span className="w-16 text-sm text-gray-600">{item.range}</span>
                          <div className="flex-1 h-4 bg-gray-200 rounded overflow-hidden">
                            <div
                              className={`h-full ${item.color}`}
                              style={{ width: `${(item.count / rarityScores.length) * 100}%` }}
                            />
                          </div>
                          <span className="text-sm text-gray-600 w-12 text-right">{item.count}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              )}

              {/* Traits Tab */}
              {activeTab === 'traits' && (
                <div className="bg-white rounded-lg shadow overflow-hidden">
                  <table className="min-w-full divide-y divide-gray-200">
                    <thead className="bg-gray-50">
                      <tr>
                        <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Trait Type</th>
                        <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Value</th>
                        <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase">Count</th>
                        <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase">Percentage</th>
                        <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase">Rarity</th>
                      </tr>
                    </thead>
                    <tbody className="bg-white divide-y divide-gray-200">
                      {traitDistributions.map((trait, idx) => (
                        <tr key={idx}>
                          <td className="px-6 py-4 whitespace-nowrap text-sm font-medium text-gray-900">
                            {trait.trait_type}
                          </td>
                          <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                            {trait.value}
                          </td>
                          <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500 text-right">
                            {trait.count}
                          </td>
                          <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500 text-right">
                            {trait.percentage.toFixed(2)}%
                          </td>
                          <td className="px-6 py-4 whitespace-nowrap text-sm text-right">
                            <span className={getScoreColor(trait.rarity_score)}>
                              {trait.rarity_score.toFixed(2)}
                            </span>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}

              {/* Rankings Tab */}
              {activeTab === 'rankings' && (
                <div className="bg-white rounded-lg shadow overflow-hidden">
                  <table className="min-w-full divide-y divide-gray-200">
                    <thead className="bg-gray-50">
                      <tr>
                        <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Rank</th>
                        <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Token ID</th>
                        <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase">Score</th>
                        <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase">Percentile</th>
                        <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase">Trait Rarity</th>
                      </tr>
                    </thead>
                    <tbody className="bg-white divide-y divide-gray-200">
                      {rarityScores.map((score) => (
                        <tr key={score.token_id}>
                          <td className="px-6 py-4 whitespace-nowrap">
                            <span className={`px-2 py-1 text-xs font-medium rounded ${getRankBadge(score.rank, rarityScores.length)}`}>
                              #{score.rank}
                            </span>
                          </td>
                          <td className="px-6 py-4 whitespace-nowrap text-sm text-blue-600">
                            <Link href={`/nft/${collectionAddress}/${score.token_id}`}>
                              #{score.token_id}
                            </Link>
                          </td>
                          <td className="px-6 py-4 whitespace-nowrap text-sm text-right font-bold">
                            <span className={getScoreColor(score.total_score)}>
                              {score.total_score.toFixed(2)}
                            </span>
                          </td>
                          <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500 text-right">
                            {score.percentile.toFixed(1)}%
                          </td>
                          <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500 text-right">
                            {score.trait_rarity.toFixed(2)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </>
          )}

          {!collectionStats && !loading && (
            <div className="text-center py-12 text-gray-500">
              Enter a collection address to view rarity analysis
            </div>
          )}
        </main>
      </div>
    </>
  )
}
