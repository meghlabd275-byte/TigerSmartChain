'use client'

import { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react'

interface Web3ContextType {
  address: string | null
  chainId: number | null
  balance: string | null
  connect: () => Promise<void>
  disconnect: () => void
  isConnecting: boolean
  error: string | null
}

const Web3Context = createContext<Web3ContextType>({
  address: null,
  chainId: null,
  balance: null,
  connect: async () => {},
  disconnect: () => {},
  isConnecting: false,
  error: null,
})

export function useWeb3() {
  return useContext(Web3Context)
}

interface Eip1193Provider {
  request(args: { method: string; params?: unknown[] }): Promise<unknown>
  on?(event: string, handler: (...args: never[]) => void): void
  removeListener?(event: string, handler: (...args: never[]) => void): void
}

function getInjectedProvider(): Eip1193Provider | null {
  if (typeof window === 'undefined') return null
  return (window as unknown as { ethereum?: Eip1193Provider }).ethereum ?? null
}

export function Web3Provider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<string | null>(null)
  const [chainId, setChainId] = useState<number | null>(null)
  const [balance, setBalance] = useState<string | null>(null)
  const [isConnecting, setIsConnecting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async (account: string) => {
    const provider = getInjectedProvider()
    if (!provider) return
    const hexChainId = (await provider.request({ method: 'eth_chainId' })) as string
    const hexBalance = (await provider.request({
      method: 'eth_getBalance',
      params: [account, 'latest'],
    })) as string
    setChainId(parseInt(hexChainId, 16))
    setBalance(BigInt(hexBalance).toString())
  }, [])

  const connect = useCallback(async () => {
    const provider = getInjectedProvider()
    if (!provider) {
      setError('No EVM wallet detected. Install MetaMask or another injected wallet.')
      return
    }
    setIsConnecting(true)
    setError(null)
    try {
      const accounts = (await provider.request({ method: 'eth_requestAccounts' })) as string[]
      const account = accounts[0]
      if (!account) {
        setError('Wallet returned no accounts.')
        return
      }
      setAddress(account)
      await refresh(account)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to connect wallet')
    } finally {
      setIsConnecting(false)
    }
  }, [refresh])

  const disconnect = useCallback(() => {
    setAddress(null)
    setChainId(null)
    setBalance(null)
    setError(null)
  }, [])

  // Re-attach to an already-authorised wallet and follow account/chain changes.
  useEffect(() => {
    const provider = getInjectedProvider()
    if (!provider) return

    let cancelled = false
    provider
      .request({ method: 'eth_accounts' })
      .then(async (accounts) => {
        const account = (accounts as string[])[0]
        if (cancelled || !account) return
        setAddress(account)
        await refresh(account)
      })
      .catch(() => undefined)

    const onAccountsChanged = (...args: never[]) => {
      const accounts = args[0] as unknown as string[]
      if (accounts.length === 0) {
        disconnect()
      } else {
        setAddress(accounts[0])
        refresh(accounts[0]).catch(() => undefined)
      }
    }
    const onChainChanged = (...args: never[]) => {
      setChainId(parseInt(args[0] as unknown as string, 16))
    }

    provider.on?.('accountsChanged', onAccountsChanged)
    provider.on?.('chainChanged', onChainChanged)
    return () => {
      cancelled = true
      provider.removeListener?.('accountsChanged', onAccountsChanged)
      provider.removeListener?.('chainChanged', onChainChanged)
    }
  }, [refresh, disconnect])

  return (
    <Web3Context.Provider
      value={{ address, chainId, balance, connect, disconnect, isConnecting, error }}
    >
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
