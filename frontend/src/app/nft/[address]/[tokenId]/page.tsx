'use client'

import { useState, useEffect } from 'react'
import { useParams } from 'next/navigation'
import Link from 'next/link'
import { Copy, Check, Image as ImageIcon, User, ArrowLeft, ArrowRight, Hash } from 'lucide-react'
import api from '@/lib/api'
import { copyToClipboard, formatTimestamp, formatAddress } from '@/lib/utils'

export default function NFTItemPage() {
  const params = useParams<{ address: string; tokenId: string }>()
  const collectionAddress = params?.address ?? ''
  const tokenId = params?.tokenId ?? ''

  const [token, setToken] = useState<any>(null)
  const [collection, setCollection] = useState<any>(null)
  const [history, setHistory] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (!collectionAddress || !tokenId) return

    const load = async () => {
      setLoading(true)
      setError(null)
      try {
        const [item, col, hist] = await Promise.all([
          api.getNFTToken(collectionAddress, tokenId),
          api.getNFTCollection(collectionAddress).catch(() => null),
          api.getNFTTokenHistory(collectionAddress, tokenId),
        ])
        if (!item) throw new Error('Token not found')
        setToken(item)
        setCollection(col)
        setHistory(hist.items)
      } catch (err) {
        setToken(null)
        setError(err instanceof Error ? err.message : 'Failed to load this token')
      } finally {
        setLoading(false)
      }
    }

    load()
  }, [collectionAddress, tokenId])

  const copy = async () => {
    await copyToClipboard(collectionAddress)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  if (loading) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-dark-900 flex items-center justify-center">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-primary-500" />
      </div>
    )
  }

  if (error || !token) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-dark-900 flex flex-col items-center justify-center gap-4">
        <p className="text-red-500">{error || 'Token not found'}</p>
        <Link href={`/nft/${collectionAddress}`} className="px-4 py-2 rounded-lg bg-primary-500 text-white hover:bg-primary-600">
          Back to collection
        </Link>
      </div>
    )
  }

  const metadata = token.metadata || {}
  const image = token.imageUrl || metadata.image_url || metadata.image

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-dark-900">
      <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <Link
          href={`/nft/${collectionAddress}`}
          className="inline-flex items-center gap-2 text-sm text-gray-500 hover:text-primary-500 mb-6"
        >
          <ArrowLeft className="w-4 h-4" />
          {collection?.name || formatAddress(collectionAddress)}
        </Link>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
          <div className="aspect-square bg-gray-200 dark:bg-dark-700 rounded-2xl overflow-hidden flex items-center justify-center">
            {image ? (
              // eslint-disable-next-line @next/next/no-img-element
              <img src={image} alt={metadata.name || `#${tokenId}`} className="w-full h-full object-cover" />
            ) : (
              <ImageIcon className="w-16 h-16 text-gray-400" />
            )}
          </div>

          <div>
            <p className="text-sm text-primary-500 mb-1">{collection?.name}</p>
            <h1 className="text-3xl font-bold text-gray-900 dark:text-white mb-4">
              {metadata.name || `#${tokenId}`}
            </h1>

            {metadata.description && (
              <p className="text-gray-600 dark:text-gray-400 mb-6">{metadata.description}</p>
            )}

            <div className="bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700 divide-y divide-gray-200 dark:divide-dark-700">
              <div className="p-4 flex items-center justify-between">
                <span className="flex items-center gap-2 text-sm text-gray-500">
                  <Hash className="w-4 h-4" /> Token ID
                </span>
                <span className="font-mono text-gray-900 dark:text-white">{tokenId}</span>
              </div>
              <div className="p-4 flex items-center justify-between">
                <span className="flex items-center gap-2 text-sm text-gray-500">
                  <User className="w-4 h-4" /> Owner
                </span>
                <Link href={`/address/${token.owner}`} className="font-mono text-primary-500 hover:underline">
                  {formatAddress(token.owner || '')}
                </Link>
              </div>
              <div className="p-4 flex items-center justify-between">
                <span className="text-sm text-gray-500">Contract</span>
                <span className="flex items-center gap-2">
                  <code className="font-mono text-sm text-gray-900 dark:text-white">
                    {formatAddress(collectionAddress)}
                  </code>
                  <button onClick={copy} className="p-1 hover:bg-gray-100 dark:hover:bg-dark-700 rounded">
                    {copied ? <Check className="w-4 h-4 text-green-500" /> : <Copy className="w-4 h-4 text-gray-400" />}
                  </button>
                </span>
              </div>
              {token.uri && (
                <div className="p-4">
                  <span className="block text-sm text-gray-500 mb-1">Token URI</span>
                  <code className="block font-mono text-xs break-all text-gray-700 dark:text-gray-300">{token.uri}</code>
                </div>
              )}
            </div>

            {Array.isArray(metadata.attributes) && metadata.attributes.length > 0 && (
              <div className="mt-6">
                <h2 className="text-sm font-semibold text-gray-500 mb-3">Attributes</h2>
                <div className="grid grid-cols-2 gap-3">
                  {metadata.attributes.map((attr: any, i: number) => (
                    <div key={i} className="bg-white dark:bg-dark-800 rounded-lg border border-gray-200 dark:border-dark-700 p-3">
                      <p className="text-xs text-gray-500 uppercase">{attr.trait_type}</p>
                      <p className="text-sm font-medium text-gray-900 dark:text-white">{String(attr.value)}</p>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        </div>

        <div className="mt-10 bg-white dark:bg-dark-800 rounded-xl border border-gray-200 dark:border-dark-700">
          <h2 className="px-6 py-4 border-b border-gray-200 dark:border-dark-700 font-semibold text-gray-900 dark:text-white">
            Transfer history
          </h2>
          {history.length === 0 ? (
            <p className="p-6 text-center text-gray-500">No transfers recorded for this token.</p>
          ) : (
            <div className="divide-y divide-gray-200 dark:divide-dark-700">
              {history.map((t: any) => (
                <div key={t.id} className="p-4 flex flex-wrap items-center gap-3 text-sm">
                  <span className="text-gray-500 w-40 shrink-0">{formatTimestamp(new Date(t.createdAt).getTime() / 1000)}</span>
                  <Link href={`/address/${t.fromAddress}`} className="font-mono text-primary-500 hover:underline">
                    {formatAddress(t.fromAddress || '')}
                  </Link>
                  <ArrowRight className="w-4 h-4 text-gray-400 shrink-0" />
                  <Link href={`/address/${t.toAddress}`} className="font-mono text-primary-500 hover:underline">
                    {formatAddress(t.toAddress || '')}
                  </Link>
                  <Link href={`/tx/${t.transactionHash}`} className="ml-auto font-mono text-gray-500 hover:text-primary-500">
                    {formatAddress(t.transactionHash || '')}
                  </Link>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
