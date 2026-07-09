'use client'

import { createContext, useContext, useState, useEffect, ReactNode } from 'react'

interface Web3ContextType {
  address: string | null
  chainId: number | null
  balance: string | null
  connect: () => Promise<void>
  disconnect: () => void
  isConnecting: boolean
}

const Web3Context = createContext<Web3ContextType>({
  address: null,
  chainId: null,
  balance: null,
  connect: async () => {},
  disconnect: () => {},
  isConnecting: false,
})

export function useWeb3() {
  return useContext(Web3Context)
}

export function Web3Provider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<string | null>(null)
  const [chainId, setChainId] = useState<number | null>(null)
  const [balance, setBalance] = useState<string | null>(null)
  const [isConnecting, setIsConnecting] = useState(false)

  const connect = async () => {
    setIsConnecting(true)
    try {
      // In production, use actual wallet connection
      // For now, simulate connection
      await new Promise(resolve => setTimeout(resolve, 1000))
      setAddress('0x742d35Cc6634C0532925a3b844Bc9e7595f0fEb1')
      setChainId(56)
      setBalance('1.5')
    } catch (error) {
      console.error('Failed to connect:', error)
    } finally {
      setIsConnecting(false)
    }
  }

  const disconnect = () => {
    setAddress(null)
    setChainId(null)
    setBalance(null)
  }

  return (
    <Web3Context.Provider value={{ address, chainId, balance, connect, disconnect, isConnecting }}>
      {children}
    </Web3Context.Provider>
  )
}

export function WalletConnect() {
  const { address, connect, disconnect, isConnecting } = useWeb3()

  if (address) {
    return (
      <div className="flex items-center space-x-4">
        <div className="px-4 py-2 bg-primary-100 dark:bg-primary-900/30 rounded-lg">
          <span className="text-sm text-primary-600 dark:text-primary-400">
            {address.slice(0, 6)}...{address.slice(-4)}
          </span>
        </div>
        <button onClick={disconnect} className="text-sm text-gray-500 hover:text-gray-700">
          Disconnect
        </button>
      </div>
    )
  }

  return (
    <button
      onClick={connect}
      disabled={isConnecting}
      className="px-4 py-2 bg-primary-500 text-white rounded-lg hover:bg-primary-600 disabled:opacity-50"
    >
      {isConnecting ? 'Connecting...' : 'Connect Wallet'}
    </button>
  )
}
