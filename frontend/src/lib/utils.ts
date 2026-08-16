import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function formatAddress(address: string, startChars = 6, endChars = 4): string {
  if (!address) return ''
  if (address.length <= startChars + endChars) return address
  return `${address.slice(0, startChars)}...${address.slice(-endChars)}`
}

export function formatHash(hash: string, startChars = 10, endChars = 8): string {
  return formatAddress(hash, startChars, endChars)
}

export function formatNumber(num: number | string, decimals = 2): string {
  const n = typeof num === 'string' ? parseFloat(num) : num
  if (isNaN(n)) return '0'
  
  if (n >= 1e12) {
    return (n / 1e12).toFixed(decimals) + 'T'
  }
  if (n >= 1e9) {
    return (n / 1e9).toFixed(decimals) + 'B'
  }
  if (n >= 1e6) {
    return (n / 1e6).toFixed(decimals) + 'M'
  }
  if (n >= 1e3) {
    return (n / 1e3).toFixed(decimals) + 'K'
  }
  return n.toFixed(decimals)
}

export function formatCurrency(value: number | string, currency = 'USD'): string {
  const n = typeof value === 'string' ? parseFloat(value) : value
  if (isNaN(n)) return '$0.00'
  
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency,
    minimumFractionDigits: 2,
    maximumFractionDigits: n < 1 ? 6 : 2,
  }).format(n)
}

export function formatPercentage(value: number | string): string {
  const n = typeof value === 'string' ? parseFloat(value) : value
  if (isNaN(n)) return '0%'
  
  const sign = n >= 0 ? '+' : ''
  return `${sign}${n.toFixed(2)}%`
}

export function formatWei(wei: string | number, decimals = 18): string {
  const w = typeof wei === 'string' ? wei : wei.toString()
  const value = parseFloat(w) / Math.pow(10, decimals)
  return value.toFixed(6)
}

export function formatTimestamp(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

export function formatTimeAgo(timestamp: number): string {
  const now = Math.floor(Date.now() / 1000)
  const diff = now - timestamp
  
  if (diff < 60) return `${diff}s ago`
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  if (diff < 2592000) return `${Math.floor(diff / 86400)}d ago`
  if (diff < 31536000) return `${Math.floor(diff / 2592000)}mo ago`
  return `${Math.floor(diff / 31536000)}y ago`
}

export function shortenTokenId(tokenId: string): string {
  if (tokenId.length > 10) {
    return `${tokenId.slice(0, 6)}...${tokenId.slice(-4)}`
  }
  return tokenId
}

export function isValidAddress(address: string): boolean {
  return /^0x[a-fA-F0-9]{40}$/.test(address)
}

export function isValidHash(hash: string): boolean {
  return /^0x[a-fA-F0-9]{64}$/.test(hash)
}

export function copyToClipboard(text: string): Promise<void> {
  return navigator.clipboard.writeText(text)
}

export function getStatusColor(status: string): string {
  switch (status.toLowerCase()) {
    case 'success':
      return 'text-green-500'
    case 'failure':
    case 'failed':
      return 'text-red-500'
    case 'pending':
      return 'text-yellow-500'
    default:
      return 'text-gray-500'
  }
}

export function getTokenTypeIcon(type: string): string {
  switch (type.toUpperCase()) {
    case 'BEP20':
      return '💰'
    case 'BEP721':
      return '🎨'
    case 'BEP1155':
      return '🃏'
    default:
      return '📄'
  }
}
