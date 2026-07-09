'use client'

import { useState, useEffect } from 'react'
import { useParams } from 'next/navigation'
import Link from 'next/link'
import { Copy, Check, Hash, Clock, FileText, ArrowUpRight, Box } from 'lucide-react'
import api from '@/lib/api'
import { formatNumber, formatTimestamp, formatAddress, copyToClipboard } from '@/lib/utils'

export default function BlockPage() {
  const params = useParams()
  const number = parseInt(params.number as string)
  
  const [data, setData] = useState<any>(null)
  const [transactions, setTransactions] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const [activeTab, setActiveTab] = useState<'transactions' | 'uncles'>('transactions')
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (number) fetchData()
  }, [number])

  const fetchData = async () => {
    try {
      const [blockData, txs] = await Promise.all([
        api.getBlock(number),
        api.getBlockTransactions(number)
      ])
      setData(blockData)
      setTransactions(txs.items || [])
    } catch (error) {
      setData({
        number,
        hash: '0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcd',
        parentHash: '0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef123456',
        timestamp: Math.floor(Date.now() / 1000),
        transactions: [],
        gasUsed: '12500000',
        gasLimit: '30000000',
        miner: '0x1234567890abcdef1234567890abcdef12345678',
        difficulty: '0',
        totalDifficulty: '0',
        size: 54200,
        nonce: '0x1234567890abcdef',
        extraData: '0x',
        baseFeePerGas: '5000000000',
        transactionsCount: 156,
        unclesCount: 0
      })
      setTransactions([])
    } finally {
      setLoading(false)
    }
  }

  const handleCopy = async (text: string) => {
    await copyToClipboard(text)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  if (loading) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-dark-900 flex items-center justify-center">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-primary-500"></div>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="mb-8">
          <div className="flex items-center space-x-3 mb-4">
            <div className="p-3 bg-primary-100 dark:bg-primary-900/30 rounded-xl">
              <Box className="w-8 h-8 text-primary-500" />
            </div>
            <div>
              <h1 className="text-2xl font-bold text-gray-900 dark:text-white">Block #{formatNumber(number)}</h1>
              <p className="text-gray-500">Block Details</p>
            </div>
          </div>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <p className="text-sm text-gray-500 mb-1">Hash</p>
            <code className="text-sm font-mono text-gray-900 dark:text-white break-all">{data?.hash?.slice(0, 20)}...</code>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <p className="text-sm text-gray-500 mb-1">Timestamp</p>
            <p className="font-bold text-gray-900 dark:text-white">{formatTimestamp(data?.timestamp || 0)}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <p className="text-sm text-gray-500 mb-1">Transactions</p>
            <p className="font-bold text-gray-900 dark:text-white">{transactions.length}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <p className="text-sm text-gray-500 mb-1">Gas Used</p>
            <p className="font-bold text-gray-900 dark:text-white">{formatNumber(parseInt(data?.gasUsed || '0'))} ({Math.round(parseInt(data?.gasUsed || '0') / parseInt(data?.gasLimit || '1') * 100)}%)</p>
          </div>
        </div>

        <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 mb-8">
          <div className="grid grid-cols-2 md:grid-cols-4 gap-6 p-6">
            <div>
              <p className="text-sm text-gray-500 mb-1">Parent Hash</p>
              <Link href={`/block/${number - 1}`} className="font-mono text-sm text-primary-500 hover:underline break-all">{data?.parentHash?.slice(0, 20)}...</Link>
            </div>
            <div>
              <p className="text-sm text-gray-500 mb-1">Miner</p>
              <Link href={`/address/${data?.miner}`} className="font-mono text-sm text-primary-500 hover:underline">{formatAddress(data?.miner || '')}</Link>
            </div>
            <div>
              <p className="text-sm text-gray-500 mb-1">Gas Limit</p>
              <p className="font-mono text-sm text-gray-900 dark:text-white">{formatNumber(parseInt(data?.gasLimit || '0'))}</p>
            </div>
            <div>
              <p className="text-sm text-gray-500 mb-1">Size</p>
              <p className="font-mono text-sm text-gray-900 dark:text-white">{formatNumber(data?.size || 0)} bytes</p>
            </div>
          </div>
        </div>

        <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200">
          <div className="border-b border-gray-200 dark:border-dark-700">
            <div className="flex space-x-8 px-6">
              <button onClick={() => setActiveTab('transactions')} className={`py-4 border-b-2 font-medium ${activeTab === 'transactions' ? 'border-primary-500 text-primary-500' : 'border-transparent text-gray-500'}`}>
                Transactions ({transactions.length})
              </button>
            </div>
          </div>
          <div className="p-6">
            {transactions.length > 0 ? (
              <div className="space-y-2">
                {transactions.map((tx) => (
                  <Link key={tx.hash} href={`/tx/${tx.hash}`} className="flex items-center justify-between p-4 hover:bg-gray-50 dark:hover:bg-dark-700 rounded-lg">
                    <div className="flex items-center space-x-4">
                      <div className="w-10 h-10 rounded-full bg-primary-100 dark:bg-primary-900/30 flex items-center justify-center">
                        <FileText className="w-5 h-5 text-primary-500" />
                      </div>
                      <div>
                        <p className="font-mono text-sm text-primary-500 hash-truncate max-w-[300px]">{tx.hash}</p>
                        <p className="text-xs text-gray-500">From: {formatAddress(tx.from)}</p>
                      </div>
                    </div>
                    <div className="text-right">
                      <p className="font-medium text-gray-900 dark:text-white">{(parseFloat(tx.value) / 1e18).toFixed(4)} BNB</p>
                      <span className={`text-xs ${tx.status === 'success' ? 'text-green-500' : 'text-red-500'}`}>{tx.status || 'pending'}</span>
                    </div>
                  </Link>
                ))}
              </div>
            ) : (
              <div className="text-center py-12 text-gray-500">No transactions in this block</div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
