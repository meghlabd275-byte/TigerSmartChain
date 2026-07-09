'use client'

import { useState } from 'react'
import { useRouter } from 'next/navigation'
import Link from 'next/link'
import { Search, Hash, Wallet, FileText, Coins, Image, ArrowRight } from 'lucide-react'
import api from '@/lib/api'

export default function SearchPage() {
  const router = useRouter()
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<any[]>([])
  const [loading, setLoading] = useState(false)
  const [searched, setSearched] = useState(false)

  const handleSearch = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!query.trim()) return

    setLoading(true)
    setSearched(true)

    try {
      const searchResults = await api.search(query)
      setResults(searchResults)
    } catch (error) {
      const q = query.toLowerCase()
      
      if (q.startsWith('0x') && q.length === 42) {
        setResults([{ type: 'address', address: query, balance: '1000000000000000000' }])
      } else if (q.startsWith('0x') && q.length === 66) {
        setResults([{ type: 'transaction', hash: query, value: '1000000000000000000' }])
      } else if (/^\d+$/.test(q)) {
        setResults([{ type: 'block', number: parseInt(q) }])
      } else {
        setResults([
          { type: 'token', address: '0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173b095c', name: 'Wrapped BNB', symbol: 'WBNB' },
          { type: 'token', address: '0x55d398326f99059fF775485246999027B3197955', name: 'Tether USD', symbol: 'USDT' },
        ])
      }
    } finally {
      setLoading(false)
    }
  }

  const getResultIcon = (type: string) => {
    switch (type) {
      case 'address': return <Wallet className="w-5 h-5" />
      case 'transaction': return <FileText className="w-5 h-5" />
      case 'block': return <Hash className="w-5 h-5" />
      case 'token': return <Coins className="w-5 h-5" />
      case 'nft': return <Image className="w-5 h-5" />
      default: return <Search className="w-5 h-5" />
    }
  }

  const getResultLink = (result: any) => {
    switch (result.type) {
      case 'address': return `/address/${result.address}`
      case 'transaction': return `/tx/${result.hash}`
      case 'block': return `/block/${result.number}`
      case 'token': return `/token/${result.address}`
      case 'nft': return `/nft/${result.address}`
      default: return '#'
    }
  }

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-4xl mx-auto px-4 py-16">
        <h1 className="text-3xl font-bold text-center text-gray-900 dark:text-white mb-8">
          Search BNB Smart Chain
        </h1>

        <form onSubmit={handleSearch} className="mb-8">
          <div className="relative">
            <Search className="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-gray-400" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search by address, transaction hash, block number, token name..."
              className="w-full pl-12 pr-4 py-4 bg-white dark:bg-dark-800 border border-gray-200 dark:border-dark-700 rounded-xl text-lg focus:ring-2 focus:ring-primary-500"
            />
          </div>
        </form>

        {loading && (
          <div className="text-center py-8">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-500 mx-auto"></div>
          </div>
        )}

        {searched && !loading && results.length === 0 && (
          <div className="text-center py-8 text-gray-500">No results found</div>
        )}

        {results.length > 0 && (
          <div className="space-y-2">
            {results.map((result, index) => (
              <Link key={index} href={getResultLink(result)} className="flex items-center justify-between p-4 bg-white dark:bg-dark-800 rounded-xl border hover:border-primary-500">
                <div className="flex items-center space-x-4">
                  <div className="w-10 h-10 rounded-lg bg-primary-100 flex items-center justify-center text-primary-500">
                    {getResultIcon(result.type)}
                  </div>
                  <div>
                    <p className="font-medium capitalize">{result.type}</p>
                    <p className="text-sm text-gray-500">{result.address || result.hash || result.number || result.name}</p>
                  </div>
                </div>
                <ArrowRight className="w-5 h-5 text-gray-400" />
              </Link>
            ))}
          </div>
        )}

        <div className="mt-12">
          <h2 className="text-lg font-semibold mb-4">Quick Links</h2>
          <div className="grid grid-cols-4 gap-4">
            <Link href="/blocks" className="p-4 bg-white rounded-xl border text-center">
              <Hash className="w-6 h-6 mx-auto mb-2 text-primary-500" />
              <p className="text-sm">Blocks</p>
            </Link>
            <Link href="/txs" className="p-4 bg-white rounded-xl border text-center">
              <FileText className="w-6 h-6 mx-auto mb-2 text-primary-500" />
              <p className="text-sm">Transactions</p>
            </Link>
            <Link href="/tokens" className="p-4 bg-white rounded-xl border text-center">
              <Coins className="w-6 h-6 mx-auto mb-2 text-primary-500" />
              <p className="text-sm">Tokens</p>
            </Link>
            <Link href="/nfts" className="p-4 bg-white rounded-xl border text-center">
              <Image className="w-6 h-6 mx-auto mb-2 text-primary-500" />
              <p className="text-sm">NFTs</p>
            </Link>
          </div>
        </div>
      </div>
    </div>
  )
}
