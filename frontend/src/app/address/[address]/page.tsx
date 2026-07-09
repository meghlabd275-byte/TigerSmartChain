'use client'

import { useState, useEffect } from 'react'
import { useParams } from 'next/navigation'
import Link from 'next/link'
import { 
  Copy, 
  Check, 
  FileText, 
  Coins, 
  Image, 
  ArrowUpRight, 
  ArrowDownRight,
  Wallet
} from 'lucide-react'
import api from '@/lib/api'
import { formatNumber, formatCurrency, copyToClipboard, formatTimestamp } from '@/lib/utils'
import type { Address as AddressType, Transaction, Token, NFTToken } from '@/types'

export default function AddressPage() {
  const params = useParams()
  const address = params.address as string
  
  const [data, setData] = useState<AddressType | null>(null)
  const [transactions, setTransactions] = useState<Transaction[]>([])
  const [tokens, setTokens] = useState<Token[]>([])
  const [nfts, setNfts] = useState<NFTToken[]>([])
  const [loading, setLoading] = useState(true)
  const [activeTab, setActiveTab] = useState<'transactions' | 'tokens' | 'nfts'>('transactions')
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (address) fetchData()
  }, [address])

  const fetchData = async () => {
    try {
      const [addrData, txs, tokenList, nftList] = await Promise.all([
        api.getAddress(address),
        api.getTransactions({ address, limit: 20 }),
        api.getAddressTokens(address),
        api.getAddressNFTs(address)
      ])
      setData(addrData)
      setTransactions(txs.items || [])
      setTokens(tokenList)
      setNfts(nftList)
    } catch (error) {
      setData({
        address,
        balance: '1000000000000000000',
        isContract: false,
        txCount: 1234,
        firstSeenBlock: 10000000,
        lastSeenBlock: 45678901,
        totalReceived: '5000000000000000000',
        totalSent: '4000000000000000000'
      })
    } finally {
      setLoading(false)
    }
  }

  const handleCopy = async () => {
    await copyToClipboard(address)
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

  const balance = data ? parseFloat(data.balance) / 1e18 : 0

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="mb-8">
          <div className="flex items-center space-x-3 mb-4">
            <div className="p-3 bg-primary-100 dark:bg-primary-900/30 rounded-xl">
              {data?.isContract ? (
                <FileText className="w-8 h-8 text-primary-500" />
              ) : (
                <Wallet className="w-8 h-8 text-primary-500" />
              )}
            </div>
            <div>
              <h1 className="text-2xl font-bold text-gray-900 dark:text-white">
                {data?.isContract ? 'Contract Address' : 'Wallet Address'}
              </h1>
              <p className="text-gray-500 dark:text-gray-400">
                {data?.isContract ? 'Smart Contract' : 'Externally Owned Account'}
              </p>
            </div>
          </div>

          <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 p-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center space-x-2">
                <code className="text-lg font-mono text-gray-900 dark:text-white break-all">{address}</code>
                <button onClick={handleCopy} className="p-2 hover:bg-gray-100 dark:hover:bg-dark-700 rounded-lg">
                  {copied ? <Check className="w-5 h-5 text-green-500" /> : <Copy className="w-5 h-5 text-gray-400" />}
                </button>
              </div>
            </div>
          </div>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-8">
          <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <Wallet className="w-4 h-4" />
              <span className="text-sm">Balance</span>
            </div>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{formatCurrency(balance)} BNB</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <FileText className="w-4 h-4" />
              <span className="text-sm">Transactions</span>
            </div>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{formatNumber(data?.txCount || 0)}</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <ArrowUpRight className="w-4 h-4" />
              <span className="text-sm">Total Received</span>
            </div>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{formatCurrency(parseFloat(data?.totalReceived || '0') / 1e18)} BNB</p>
          </div>
          <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 p-6">
            <div className="flex items-center space-x-2 text-gray-500 mb-2">
              <ArrowDownRight className="w-4 h-4" />
              <span className="text-sm">Total Sent</span>
            </div>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{formatCurrency(parseFloat(data?.totalSent || '0') / 1e18)} BNB</p>
          </div>
        </div>

        <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700">
          <div className="border-b border-gray-200 dark:border-dark-700">
            <div className="flex space-x-8 px-6">
              {(['transactions', 'tokens', 'nfts'] as const).map((tab) => (
                <button
                  key={tab}
                  onClick={() => setActiveTab(tab)}
                  className={`py-4 border-b-2 font-medium capitalize transition-colors ${
                    activeTab === tab
                      ? 'border-primary-500 text-primary-500'
                      : 'border-transparent text-gray-500 hover:text-gray-700'
                  }`}
                >
                  {tab} {tab === 'transactions' ? `(${transactions.length})` : tab === 'tokens' ? `(${tokens.length})` : `(${nfts.length})`}
                </button>
              ))}
            </div>
          </div>
          <div className="p-6">
            {activeTab === 'transactions' && <TransactionList transactions={transactions} />}
            {activeTab === 'tokens' && <TokenList tokens={tokens} />}
            {activeTab === 'nfts' && <NFTList nfts={nfts} />}
          </div>
        </div>
      </div>
    </div>
  )
}

function TransactionList({ transactions }: { transactions: Transaction[] }) {
  if (transactions.length === 0) return <div className="text-center py-12 text-gray-500">No transactions found</div>
  return (
    <div className="space-y-2">
      {transactions.map((tx) => (
        <Link key={tx.hash} href={`/tx/${tx.hash}`} className="flex items-center justify-between p-4 hover:bg-gray-50 dark:hover:bg-dark-700 rounded-lg">
          <div className="flex items-center space-x-4">
            <div className={`w-10 h-10 rounded-full flex items-center justify-center bg-green-100 text-green-500`}>
              <ArrowUpRight className="w-5 h-5" />
            </div>
            <div>
              <p className="font-mono text-sm text-primary-500 hash-truncate max-w-[200px]">{tx.hash.slice(0, 20)}...</p>
              <p className="text-xs text-gray-500">Block {tx.blockNumber} • {formatTimestamp(tx.timestamp)}</p>
            </div>
          </div>
          <div className="text-right">
            <p className="font-medium text-gray-900 dark:text-white">{formatCurrency(parseFloat(tx.value) / 1e18)} BNB</p>
            <span className={`text-xs ${tx.status === 'success' ? 'text-green-500' : 'text-red-500'}`}>{tx.status}</span>
          </div>
        </Link>
      ))}
    </div>
  )
}

function TokenList({ tokens }: { tokens: Token[] }) {
  if (tokens.length === 0) return <div className="text-center py-12 text-gray-500">No tokens found</div>
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      {tokens.map((token) => (
        <Link key={token.address} href={`/token/${token.address}`} className="flex items-center space-x-4 p-4 hover:bg-gray-50 dark:hover:bg-dark-700 rounded-lg">
          <div className="w-10 h-10 rounded-full bg-gray-200 dark:bg-dark-700 flex items-center justify-center">
            <Coins className="w-5 h-5 text-gray-400" />
          </div>
          <div>
            <p className="font-medium text-gray-900 dark:text-white">{token.symbol}</p>
            <p className="text-sm text-gray-500">{token.name}</p>
          </div>
        </Link>
      ))}
    </div>
  )
}

function NFTList({ nfts }: { nfts: NFTToken[] }) {
  if (nfts.length === 0) return <div className="text-center py-12 text-gray-500">No NFTs found</div>
  return (
    <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-4">
      {nfts.map((nft) => (
        <Link key={`${nft.collectionAddress}-${nft.tokenId}`} href={`/nft/${nft.collectionAddress}/${nft.tokenId}`} className="aspect-square bg-gray-100 dark:bg-dark-700 rounded-lg overflow-hidden">
          <div className="w-full h-full flex items-center justify-center">
            <Image className="w-8 h-8 text-gray-400" />
          </div>
        </Link>
      ))}
    </div>
  )
}
