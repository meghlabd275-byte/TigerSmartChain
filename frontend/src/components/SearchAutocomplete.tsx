'use client'

import { useState, useEffect } from 'react'
import Link from 'next/link'
import { Search, FileText, Box, Coins, Image, Hash } from 'lucide-react'
import { isValidAddress, isValidHash } from '@/lib/utils'

interface SearchResult {
  type: 'address' | 'transaction' | 'block' | 'token' | 'nft' | 'ens'
  value: string
  label?: string
}

interface SearchAutocompleteProps {
  query: string
  onSelect: (result: { type: string; value: string }) => void
  onClose: () => void
}

export function SearchAutocomplete({ query, onSelect, onClose }: SearchAutocompleteProps) {
  const [results, setResults] = useState<SearchResult[]>([])
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    const searchQuery = async () => {
      if (query.length < 2) {
        setResults(getQuickSuggestions(query))
        return
      }

      setLoading(true)
      
      // Quick suggestions based on input format
      const suggestions: SearchResult[] = []

      if (isValidAddress(query)) {
        suggestions.push({
          type: 'address',
          value: query,
          label: 'Wallet Address'
        })
      }

      if (isValidHash(query)) {
        suggestions.push({
          type: 'transaction',
          value: query,
          label: 'Transaction Hash'
        })
      }

      // Check if it's a block number
      if (/^\d+$/.test(query)) {
        suggestions.push({
          type: 'block',
          value: query,
          label: `Block #${query}`
        })
      }

      // Add search suggestions
      suggestions.push({
        type: 'search',
        value: query,
        label: `Search for "${query}"`
      })

      setResults(suggestions)
      setLoading(false)
    }

    const debounce = setTimeout(searchQuery, 300)
    return () => clearTimeout(debounce)
  }, [query])

  const getQuickSuggestions = (q: string): SearchResult[] => {
    const suggestions: SearchResult[] = []
    
    if (q.length === 0) {
      suggestions.push(
        { type: 'block', value: 'latest', label: 'Latest Block' },
        { type: 'transaction', value: 'pending', label: 'Pending Transactions' },
        { type: 'token', value: 'top', label: 'Top Tokens' }
      )
    }
    
    return suggestions
  }

  const getIcon = (type: string) => {
    switch (type) {
      case 'address':
        return <Box className="w-4 h-4" />
      case 'transaction':
        return <FileText className="w-4 h-4" />
      case 'block':
        return <Hash className="w-4 h-4" />
      case 'token':
        return <Coins className="w-4 h-4" />
      case 'nft':
        return <Image className="w-4 h-4" />
      default:
        return <Search className="w-4 h-4" />
    }
  }

  const getHref = (result: SearchResult): string => {
    switch (result.type) {
      case 'address':
      case 'token':
      case 'nft':
        return `/address/${result.value}`
      case 'transaction':
        return `/tx/${result.value}`
      case 'block':
        return `/block/${result.value}`
      case 'search':
        return `/search?q=${encodeURIComponent(result.value)}`
      default:
        return `/search?q=${encodeURIComponent(result.value)}`
    }
  }

  if (results.length === 0 && !loading) {
    return (
      <div className="absolute top-full left-0 right-0 mt-2 bg-white dark:bg-dark-800 rounded-lg shadow-xl border border-gray-200 dark:border-dark-700 overflow-hidden z-50">
        <div className="p-4 text-center text-gray-500 text-sm">
          No results found
        </div>
      </div>
    )
  }

  return (
    <div className="absolute top-full left-0 right-0 mt-2 bg-white dark:bg-dark-800 rounded-lg shadow-xl border border-gray-200 dark:border-dark-700 overflow-hidden z-50 animate-fade-in">
      {loading ? (
        <div className="p-4 text-center text-gray-500 text-sm">
          Searching...
        </div>
      ) : (
        <ul className="max-h-80 overflow-y-auto">
          {results.map((result, index) => (
            <li key={index}>
              <Link
                href={getHref(result)}
                onClick={() => onClose()}
                className="flex items-center px-4 py-3 hover:bg-gray-50 dark:hover:bg-dark-700 transition-colors"
              >
                <span className="w-8 h-8 rounded-lg bg-gray-100 dark:bg-dark-700 flex items-center justify-center text-gray-500 mr-3">
                  {getIcon(result.type)}
                </span>
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium text-gray-900 dark:text-white truncate">
                    {result.value}
                  </p>
                  {result.label && (
                    <p className="text-xs text-gray-500 dark:text-gray-400">
                      {result.label}
                    </p>
                  )}
                </div>
              </Link>
            </li>
          ))}
        </ul>
      )}
      
      {/* Powered by */}
      <div className="px-4 py-2 bg-gray-50 dark:bg-dark-700 border-t border-gray-200 dark:border-dark-600">
        <p className="text-xs text-gray-400 text-center">
          Press Enter to search
        </p>
      </div>
    </div>
  )
}
