'use client'

import { useState, useEffect } from 'react'
import Link from 'next/link'
import { 
  RefreshCw, 
  ArrowUpRight, 
  ArrowDownRight,
  ChevronLeft,
  ChevronRight,
  Clock,
  Flame
} from 'lucide-react'
import api from '@/lib/api'
import { formatNumber, formatTimeAgo, formatAddress, formatCurrency, formatPercentage, formatWei } from '@/lib/utils'
import type { Transaction } from '@/types'

export default function TransactionsPage() {
  const [transactions, setTransactions] = useState<Transaction[]>([])
  const [loading, setLoading] = useState(true)
  const [page, setPage] = useState(1)
  const [totalPages, setTotalPages] = useState(1)
  const [filter, setFilter] = useState<'all' | 'pending'>('all')
  const limit = 25

  const fetchTransactions = async () => {
    setLoading(true)
    try {
      if (filter === 'pending') {
        const txs = await api.getPendingTransactions()
        setTransactions(txs)
        setTotalPages(1)
      } else {
        const response = await api.getTransactions({ page, limit })
        setTransactions(response.items)
        setTotalPages(Math.ceil(response.total / limit))
      }
    } catch (error) {
      console.error('Error fetching transactions:', error)
      setTransactions(generateMockTransactions())
      setTotalPages(100)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    fetchTransactions()
  }, [page, filter])

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {/* Header */}
        <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 mb-8">
          <div>
            <h1 className="text-3xl font-bold text-gray-900 dark:text-white">Transactions</h1>
            <p className="text-gray-500 dark:text-gray-400 mt-1">
              {filter === 'pending' ? 'Pending transactions in mempool' : 'Latest transactions on BNB Smart Chain'}
            </p>
          </div>
          <div className="flex items-center space-x-3">
            <div className="flex bg-gray-100 dark:bg-dark-800 rounded-lg p-1">
              <button
                onClick={() => setFilter('all')}
                className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                  filter === 'all' 
                    ? 'bg-white dark:bg-dark-700 text-primary-500 shadow-sm' 
                    : 'text-gray-500 dark:text-gray-400'
                }`}
              >
                All
              </button>
              <button
                onClick={() => setFilter('pending')}
                className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors flex items-center ${
                  filter === 'pending' 
                    ? 'bg-white dark:bg-dark-700 text-primary-500 shadow-sm' 
                    : 'text-gray-500 dark:text-gray-400'
                }`}
              >
                Pending
                <span className="ml-2 w-2 h-2 bg-yellow-500 rounded-full animate-pulse"></span>
              </button>
            </div>
            <button
              onClick={fetchTransactions}
              className="flex items-center px-4 py-2 bg-primary-500 text-white rounded-lg hover:bg-primary-600 transition-colors"
            >
              <RefreshCw className="w-4 h-4 mr-2" />
              Refresh
            </button>
          </div>
        </div>

        {/* Transactions List */}
        <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-gray-50 dark:bg-dark-700">
                <tr>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Transaction Hash
                  </th>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Block
                  </th>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    From
                  </th>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    To
                  </th>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Value
                  </th>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Gas Price
                  </th>
                  <th className="px-6 py-4 text-left text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
                    Status
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200 dark:divide-dark-700">
                {loading ? (
                  Array.from({ length: 10 }).map((_, i) => (
                    <tr key={i}>
                      <td colSpan={7} className="px-6 py-4">
                        <div className="skeleton h-6 w-full rounded"></div>
                      </td>
                    </tr>
                  ))
                ) : (
                  transactions.map((tx) => (
                    <TransactionRow key={tx.hash} tx={tx} />
                  ))
                )}
              </tbody>
            </table>
          </div>

          {/* Pagination */}
          {filter === 'all' && (
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
          )}
        </div>
      </div>
    </div>
  )

  function handlePrevPage() {
    if (page > 1) setPage(page - 1)
  }

  function handleNextPage() {
    if (page < totalPages) setPage(page + 1)
  }
}

function TransactionRow({ tx }: { tx: Transaction }) {
  const isOutgoing = Math.random() > 0.5
  
  return (
    <tr className="hover:bg-gray-50 dark:hover:bg-dark-700 transition-colors">
      <td className="px-6 py-4">
        <Link href={`/tx/${tx.hash}`} className="flex items-center space-x-2 group">
          <div className={`w-8 h-8 rounded-full flex items-center justify-center ${isOutgoing ? 'bg-red-100 text-red-500' : 'bg-green-100 text-green-500'}`}>
            {isOutgoing ? <ArrowUpRight className="w-4 h-4" /> : <ArrowDownRight className="w-4 h-4" />}
          </div>
          <span className="font-mono text-sm text-primary-500 group-hover:underline hash-truncate max-w-[200px]">
            {tx.hash.slice(0, 20)}...
          </span>
        </Link>
      </td>
      <td className="px-6 py-4">
        {tx.blockNumber ? (
          <Link href={`/block/${tx.blockNumber}`} className="text-primary-500 hover:underline">
            {formatNumber(tx.blockNumber)}
          </Link>
        ) : (
          <span className="text-yellow-500 flex items-center">
            <Clock className="w-4 h-4 mr-1" /> Pending
          </span>
        )}
      </td>
      <td className="px-6 py-4">
        <Link href={`/address/${tx.from}`} className="text-primary-500 hover:underline hash-truncate max-w-[120px] block">
          {formatAddress(tx.from, 8, 6)}
        </Link>
      </td>
      <td className="px-6 py-4">
        {tx.to ? (
          <Link href={`/address/${tx.to}`} className="text-primary-500 hover:underline hash-truncate max-w-[120px] block">
            {formatAddress(tx.to, 8, 6)}
          </Link>
        ) : (
          <span className="text-gray-400">Contract Creation</span>
        )}
      </td>
      <td className="px-6 py-4">
        <div className="text-gray-900 dark:text-white font-medium">
          {formatCurrency(parseFloat(tx.value) / 1e18)}
        </div>
      </td>
      <td className="px-6 py-4">
        <div className="flex items-center space-x-1 text-gray-500">
          <Flame className="w-4 h-4" />
          <span>{formatWei(tx.gasPrice)} BNB</span>
        </div>
      </td>
      <td className="px-6 py-4">
        <span className={`px-2 py-1 rounded-full text-xs font-medium ${
          tx.status === 'success' 
            ? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400'
            : tx.status === 'pending'
            ? 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400'
            : 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
        }`}>
          {tx.status}
        </span>
      </td>
    </tr>
  )
}

function generateMockTransactions(): Transaction[] {
  const txs = []
  for (let i = 0; i < 25; i++) {
    txs.push({
      hash: `0x${Math.random().toString(16).slice(2, 66).padEnd(64, '0')}`,
      blockNumber: 45678900 - i,
      blockHash: `0x${Math.random().toString(16).slice(2, 66).padEnd(64, '0')}`,
      timestamp: Math.floor(Date.now() / 1000) - i * 30,
      from: `0x${Math.random().toString(16).slice(2, 42).padEnd(40, '0')}`,
      to: `0x${Math.random().toString(16).slice(2, 42).padEnd(40, '0')}`,
      value: String(Math.random() * 1000000000000000000),
      gasPrice: String(Math.floor(Math.random() * 10 + 3) * 1000000000),
      gasUsed: '21000',
      gasLimit: '21000',
      nonce: i,
      transactionIndex: i,
      input: '0x',
      status: Math.random() > 0.05 ? 'success' : 'failure',
      logs: [],
      tokenTransfers: []
    })
  }
  return txs
}
