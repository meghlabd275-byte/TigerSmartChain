'use client'

import { useState, useEffect } from 'react'
import Link from 'next/link'
import { 
  RefreshCw, 
  Search,
  Image,
  TrendingUp,
  ChevronLeft,
  ChevronRight,
  Filter,
  Flame
} from 'lucide-react'
import api from '@/lib/api'
import { formatNumber, formatCurrency } from '@/lib/utils'
import type { NFTCollection } from '@/types'

export default function NFTsPage() {
  const [collections, setCollections] = useState<NFTCollection[]>([])
  const [loading, setLoading] = useState(true)
  const [page, setPage] = useState(1)
  const [totalPages, setTotalPages] = useState(1)
  const [search, setSearch] = useState('')
  const limit = 20

  const fetchCollections = async () => {
    setLoading(true)
    try {
      const response = await api.getNFTCollections({ page, limit })
      setCollections(response.items)
      setTotalPages(Math.ceil(response.total / limit))
    } catch (error) {
      console.error('Error fetching collections:', error)
      setCollections(generateMockCollections())
      setTotalPages(100)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    fetchCollections()
  }, [page])

  const filteredCollections = search
    ? collections.filter(c => 
        c.name.toLowerCase().includes(search.toLowerCase()) ||
        c.symbol?.toLowerCase().includes(search.toLowerCase())
      )
    : collections

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {/* Header */}
        <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 mb-8">
          <div>
            <h1 className="text-3xl font-bold text-gray-900 dark:text-white">NFT Collections</h1>
            <p className="text-gray-500 dark:text-gray-400 mt-1">
              Explore BEP721 and BEP1155 NFT collections on BNB Smart Chain
            </p>
          </div>
          <button
            onClick={fetchCollections}
            className="flex items-center px-4 py-2 bg-primary-500 text-white rounded-lg hover:bg-primary-600 transition-colors"
          >
            <RefreshCw className="w-4 h-4 mr-2" />
            Refresh
          </button>
        </div>

        {/* Search */}
        <div className="relative mb-6">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-gray-400" />
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search collections by name or symbol..."
            className="w-full pl-10 pr-4 py-3 bg-white dark:bg-dark-800 border border-gray-200 dark:border-dark-700 rounded-lg focus:ring-2 focus:ring-primary-500"
          />
        </div>

        {/* Collections Grid */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-6">
          {loading ? (
            Array.from({ length: 8 }).map((_, i) => (
              <div key={i} className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 p-4">
                <div className="skeleton h-48 w-full rounded-lg mb-4"></div>
                <div className="skeleton h-6 w-2/3 rounded mb-2"></div>
                <div className="skeleton h-4 w-1/2 rounded"></div>
              </div>
            ))
          ) : (
            filteredCollections.map((collection) => (
              <CollectionCard key={collection.address} collection={collection} />
            ))
          )}
        </div>

        {/* Pagination */}
        <div className="mt-8 flex items-center justify-between">
          <div className="text-sm text-gray-500 dark:text-gray-400">
            Page {page} of {totalPages}
          </div>
          <div className="flex space-x-2">
            <button
              onClick={() => setPage(Math.max(1, page - 1))}
              disabled={page === 1}
              className="p-2 rounded-lg border border-gray-300 dark:border-dark-600 disabled:opacity-50 disabled:cursor-not-allowed hover:bg-gray-100 dark:hover:bg-dark-700"
            >
              <ChevronLeft className="w-5 h-5" />
            </button>
            <button
              onClick={() => setPage(Math.min(totalPages, page + 1))}
              disabled={page === totalPages}
              className="p-2 rounded-lg border border-gray-300 dark:border-dark-600 disabled:opacity-50 disabled:cursor-not-allowed hover:bg-gray-100 dark:hover:bg-dark-700"
            >
              <ChevronRight className="w-5 h-5" />
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

function CollectionCard({ collection }: { collection: NFTCollection }) {
  return (
    <Link href={`/nft/${collection.address}`}>
      <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 overflow-hidden hover:border-primary-500 transition-all card-hover">
        {/* Collection Image */}
        <div className="aspect-square relative bg-gray-100 dark:bg-dark-700">
          {collection.imageUrl ? (
            <img 
              src={collection.imageUrl} 
              alt={collection.name}
              className="w-full h-full object-cover"
            />
          ) : (
            <div className="w-full h-full flex items-center justify-center">
              <Image className="w-16 h-16 text-gray-300 dark:text-gray-600" />
            </div>
          )}
          <div className="absolute top-2 right-2 px-2 py-1 bg-black/50 backdrop-blur-sm rounded-full text-xs text-white">
            {collection.type}
          </div>
        </div>

        {/* Collection Info */}
        <div className="p-4">
          <h3 className="font-semibold text-gray-900 dark:text-white truncate">
            {collection.name}
          </h3>
          <p className="text-sm text-gray-500 dark:text-gray-400 truncate">
            {collection.symbol || 'No symbol'}
          </p>

          {/* Stats */}
          <div className="mt-4 grid grid-cols-2 gap-2 text-sm">
            <div>
              <p className="text-gray-500 dark:text-gray-400">Floor</p>
              <p className="font-medium text-gray-900 dark:text-white">
                {collection.floorPrice ? formatCurrency(collection.floorPrice) : 'N/A'}
              </p>
            </div>
            <div>
              <p className="text-gray-500 dark:text-gray-400">Volume (24h)</p>
              <p className="font-medium text-gray-900 dark:text-white">
                {collection.volume24h ? formatCurrency(collection.volume24h) : 'N/A'}
              </p>
            </div>
            <div>
              <p className="text-gray-500 dark:text-gray-400">Owners</p>
              <p className="font-medium text-gray-900 dark:text-white">
                {formatNumber(collection.ownerCount)}
              </p>
            </div>
            <div>
              <p className="text-gray-500 dark:text-gray-400">Items</p>
              <p className="font-medium text-gray-900 dark:text-white">
                {formatNumber(collection.totalSupply)}
              </p>
            </div>
          </div>
        </div>
      </div>
    </Link>
  )
}

function generateMockCollections(): NFTCollection[] {
  return [
    {
      address: '0x1234567890abcdef1234567890abcdef12345678',
      name: 'Bored Ape NFT',
      symbol: 'BAYC',
      type: 'BEP721',
      totalSupply: 10000,
      mintedCount: 10000,
      ownerCount: 6500,
      floorPrice: 35.5,
      averagePrice: 42.3,
      volume24h: 125000,
      volume7d: 890000,
      volume30d: 3200000,
      imageUrl: 'https://picsum.photos/400/400?random=1'
    },
    {
      address: '0x2345678901abcdef2345678901abcdef23456789',
      name: 'CryptoPunks Clone',
      symbol: 'PUNK',
      type: 'BEP721',
      totalSupply: 10000,
      mintedCount: 10000,
      ownerCount: 4500,
      floorPrice: 28.9,
      averagePrice: 35.1,
      volume24h: 89000,
      volume7d: 567000,
      volume30d: 2100000,
      imageUrl: 'https://picsum.photos/400/400?random=2'
    }
  ]
}
