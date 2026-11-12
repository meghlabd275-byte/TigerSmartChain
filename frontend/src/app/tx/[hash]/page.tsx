'use client'

import { useState, useEffect } from 'react'
import { useParams } from 'next/navigation'
import Link from 'next/link'
import { Copy, Check, ArrowUpRight, ArrowDownRight, Clock, Flame, Hash, FileText, Activity } from 'lucide-react'
import api from '@/lib/api'
import { formatNumber, formatCurrency, formatAddress, formatTimestamp, formatWei, copyToClipboard } from '@/lib/utils'

export default function TransactionPage() {
  const params = useParams()
  const hash = params.hash as string
  
  const [data, setData] = useState<any>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<'overview' | 'logs' | 'internal'>('overview')
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (hash) fetchData()
  }, [hash])

  const fetchData = async () => {
    setLoading(true)
    setError(null)
    try {
      const txData = await api.getTransaction(hash)
      setData(txData)
    } catch (err) {
      // An unreachable indexer must surface as an error. Inventing a "successful"
      // transfer with plausible hashes and amounts misreports chain state.
      setData(null)
      setError(err instanceof Error ? err.message : 'Failed to load transaction')
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

  if (error || !data) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-dark-900 flex items-center justify-center px-4">
        <div className="max-w-md text-center">
          <h1 className="text-xl font-semibold text-gray-900 dark:text-white mb-2">
            Transaction unavailable
          </h1>
          <p className="text-gray-500 dark:text-gray-400">
            {error ?? 'Transaction not found.'}
          </p>
        </div>
      </div>
    )
  }

  const gasFee = (parseFloat(data.gasUsed) * parseFloat(data.gasPrice)) / 1e18

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="mb-8">
          <div className="flex items-center space-x-3 mb-4">
            <div className={`p-3 rounded-xl ${data?.status === 'success' ? 'bg-green-100' : 'bg-red-100'}`}>
              <FileText className={`w-8 h-8 ${data?.status === 'success' ? 'text-green-500' : 'text-red-500'}`} />
            </div>
            <div>
              <h1 className="text-2xl font-bold text-gray-900 dark:text-white">Transaction {data?.status === 'success' ? 'Successful' : 'Failed'}</h1>
              <p className="text-gray-500">Transaction Details</p>
            </div>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 p-4">
            <div className="flex items-center justify-between">
              <code className="text-lg font-mono text-gray-900 dark:text-white break-all">{hash}</code>
              <button onClick={() => handleCopy(hash)} className="p-2 hover:bg-gray-100 rounded-lg">
                {copied ? <Check className="w-5 h-5 text-green-500" /> : <Copy className="w-5 h-5 text-gray-400" />}
              </button>
            </div>
          </div>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <p className="text-sm text-gray-500 mb-1">Block</p>
            <Link href={`/block/${data?.blockNumber}`} className="text-2xl font-bold text-primary-500 hover:underline">{formatNumber(data?.blockNumber || 0)}</Link>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <p className="text-sm text-gray-500 mb-1">Time</p>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{data ? formatTimestamp(data.timestamp) : '-'}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <p className="text-sm text-gray-500 mb-1">Gas Used</p>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{formatNumber(parseInt(data?.gasUsed || '0'))}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border p-6">
            <p className="text-sm text-gray-500 mb-1">Gas Fee</p>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{formatCurrency(gasFee)} BNB</p>
          </div>
        </div>

        <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 mb-8">
          <div className="p-6 border-b border-gray-200 dark:border-dark-700">
            <div className="flex items-center justify-between">
              <Link href={`/address/${data?.from}`} className="flex items-center space-x-3">
                <div className="w-10 h-10 rounded-full bg-red-100 flex items-center justify-center">
                  <ArrowUpRight className="w-5 h-5 text-red-500" />
                </div>
                <div>
                  <p className="text-sm text-gray-500">From</p>
                  <p className="font-mono text-primary-500">{formatAddress(data?.from || '')}</p>
                </div>
              </Link>
              <div className="px-4 py-2 bg-gray-100 dark:bg-dark-700 rounded-lg">
                <p className="text-lg font-bold text-gray-900 dark:text-white">{formatCurrency(parseFloat(data?.value || '0') / 1e18)} BNB</p>
              </div>
              <Link href={`/address/${data?.to}`} className="flex items-center space-x-3">
                <div className="text-right">
                  <p className="text-sm text-gray-500">To</p>
                  <p className="font-mono text-primary-500">{formatAddress(data?.to || '')}</p>
                </div>
                <div className="w-10 h-10 rounded-full bg-green-100 flex items-center justify-center">
                  <ArrowDownRight className="w-5 h-5 text-green-500" />
                </div>
              </Link>
            </div>
          </div>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-6 p-6">
            <div><p className="text-sm text-gray-500">Gas Price</p><p className="font-mono">{formatWei(data?.gasPrice || '0')} BNB</p></div>
            <div><p className="text-sm text-gray-500">Gas Limit</p><p className="font-mono">{formatNumber(parseInt(data?.gasLimit || '0'))}</p></div>
            <div><p className="text-sm text-gray-500">Nonce</p><p className="font-mono">{data?.nonce}</p></div>
            <div><p className="text-sm text-gray-500">Tx Index</p><p className="font-mono">{data?.transactionIndex}</p></div>
          </div>
        </div>

        <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200">
          <div className="border-b border-gray-200">
            <div className="flex space-x-8 px-6">
              {(['overview', 'logs', 'internal'] as const).map((tab) => (
                <button key={tab} onClick={() => setActiveTab(tab)} className={`py-4 border-b-2 font-medium capitalize ${activeTab === tab ? 'border-primary-500 text-primary-500' : 'border-transparent text-gray-500'}`}>
                  {tab}
                </button>
              ))}
            </div>
          </div>
          <div className="p-6">
            {activeTab === 'overview' && (
              <div className="space-y-4">
                <div><p className="text-sm text-gray-500 mb-1">Input Data</p><code className="block p-4 bg-gray-100 rounded-lg font-mono text-sm break-all">{data?.input || '0x'}</code></div>
                <div><p className="text-sm text-gray-500 mb-1">Block Hash</p><code className="font-mono text-sm">{data?.blockHash}</code></div>
              </div>
            )}
            {activeTab === 'logs' && <div className="text-center py-12 text-gray-500">{data?.logs?.length || 0} logs</div>}
            {activeTab === 'internal' && <div className="text-center py-12 text-gray-500">No internal transactions</div>}
          </div>
        </div>
      </div>
    </div>
  )
}
