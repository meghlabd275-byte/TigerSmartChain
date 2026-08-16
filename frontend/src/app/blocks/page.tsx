'use client'

import { useState, useEffect } from 'react'
import Link from 'next/link'
import { 
  ArrowLeft, 
  ArrowRight, 
  RefreshCw, 
  Hash,
  Clock,
  Box,
  FileText,
  ChevronLeft,
  ChevronRight
} from 'lucide-react'
import api from '@/lib/api'
import { formatNumber, formatTimeAgo, formatAddress } from '@/lib/utils'
import type { BlockListItem } from '@/types'

export default function BlocksPage() {
  const [blocks, setBlocks] = useState<BlockListItem[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [page, setPage] = useState(1)
  const [totalPages, setTotalPages] = useState(1)
  const limit = 25

  const fetchBlocks = async () => {
    setLoading(true)
    setError(null)
    try {
      const response = await api.getBlocks({ page, limit })
      setBlocks(response.items)
      setTotalPages(Math.ceil(response.total / limit))
    } catch (error) {
      console.error('Error fetching blocks:', error)
      setBlocks([])
      setTotalPages(1)
      setError('Failed to load data. Please try again later.')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    fetchBlocks()
  }, [page])

  const handlePrevPage = () => {
    if (page > 1) setPage(page - 1)
  }

  const handleNextPage = () => {
    if (page < totalPages) setPage(page + 1)
  }

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {/* Header */}
        <div className="flex items-center justify-between mb-8">
          <div>
            <h1 className="text-3xl font-bold text-gray-900 dark:text-white">Blocks</h1>
            <p className="text-gray-500 dark:text-gray-400 mt-1">
              Latest blocks on BNB Smart Chain
            </p>
          </div>
          <button
            onClick={fetchBlocks}
            className="flex items-center px-4 py-2 bg-primary-500 text-white rounded-lg hover:bg-primary-600 transition-colors"
          >
            <RefreshCw className="w-4 h-4 mr-2" />
            Refresh
          </button>
        </div>

        {/* Blocks Table */}
        <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-gray-50 dark:bg-dark-700">
                <tr>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Block
                  </th>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Timestamp
                  </th>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Transactions
                  </th>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Gas Used
                  </th>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Miner
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200 dark:divide-dark-700">
                {loading ? (
                  Array.from({ length: 10 }).map((_, i) => (
                    <tr key={i}>
                      <td colSpan={5} className="px-6 py-4">
                        <div className="skeleton h-6 w-full rounded"></div>
                      </td>
                    </tr>
                  ))
                ) : error ? (
                  <tr>
                    <td colSpan={5} className="px-6 py-8 text-center text-red-500">
                      {error}
                      <button onClick={fetchBlocks} className="block mx-auto mt-3 px-4 py-2 rounded-lg bg-primary-500 text-white hover:bg-primary-600">Retry</button>
                    </td>
                  </tr>
                ) : blocks.length === 0 ? (
                  <tr>
                    <td colSpan={5} className="px-6 py-8 text-center text-gray-500">No blocks found</td>
                  </tr>
                ) : (
                  blocks.map((block) => (
                    <tr key={block.number} className="hover:bg-gray-50 dark:hover:bg-dark-700 transition-colors">
                      <td className="px-6 py-4">
                        <Link href={`/block/${block.number}`} className="flex items-center space-x-2 group">
                          <Hash className="w-4 h-4 text-primary-500" />
                          <span className="font-medium text-primary-500 group-hover:underline">
                            {formatNumber(block.number)}
                          </span>
                        </Link>
                      </td>
                      <td className="px-6 py-4">
                        <div className="flex items-center space-x-2 text-gray-500 dark:text-gray-400">
                          <Clock className="w-4 h-4" />
                          <span>{formatTimeAgo(block.timestamp)}</span>
                        </div>
                      </td>
                      <td className="px-6 py-4">
                        <div className="flex items-center space-x-2">
                          <FileText className="w-4 h-4 text-gray-400" />
                          <span className="text-gray-900 dark:text-white">{block.txCount} txns</span>
                        </div>
                      </td>
                      <td className="px-6 py-4">
                        <div className="text-gray-900 dark:text-white">
                          {formatNumber(parseInt(block.gasUsed))}
                          <span className="text-gray-500 ml-1">({Math.round(parseInt(block.gasUsed) / 30000000 * 100)}%)</span>
                        </div>
                      </td>
                      <td className="px-6 py-4">
                        <Link href={`/address/${block.miner}`} className="text-primary-500 hover:underline hash-truncate">
                          {formatAddress(block.miner)}
                        </Link>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>

          {/* Pagination */}
          <div className="px-6 py-4 border-t border-gray-200 dark:border-dark-700 flex items-center justify-between">
            <div className="text-sm text-gray-500 dark:text-gray-400">
              Page {page} of {totalPages}
            </div>
            <div className="flex space-x-2">
              <button
                onClick={handlePrevPage}
                disabled={page === 1}
                className="p-2 rounded-lg border border-gray-300 dark:border-dark-600 disabled:opacity-50 disabled:cursor-not-allowed hover:bg-gray-100 dark:hover:bg-dark-700 transition-colors"
              >
                <ChevronLeft className="w-5 h-5" />
              </button>
              <button
                onClick={handleNextPage}
                disabled={page === totalPages}
                className="p-2 rounded-lg border border-gray-300 dark:border-dark-600 disabled:opacity-50 disabled:cursor-not-allowed hover:bg-gray-100 dark:hover:bg-dark-700 transition-colors"
              >
                <ChevronRight className="w-5 h-5" />
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
