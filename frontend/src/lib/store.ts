'use client'

import { create } from 'zustand'
import { persist } from 'zustand/middleware'

interface ThemeState {
  theme: 'light' | 'dark'
  toggleTheme: () => void
}

export const useThemeStore = create<ThemeState>()(
  persist(
    (set) => ({
      theme: 'dark',
      toggleTheme: () => set((state) => ({ 
        theme: state.theme === 'light' ? 'dark' : 'light' 
      })),
    }),
    {
      name: 'tigersmartchain-theme',
    }
  )
)

interface SearchState {
  recentSearches: string[]
  addRecentSearch: (query: string) => void
  clearRecentSearches: () => void
}

export const useSearchStore = create<SearchState>()(
  persist(
    (set) => ({
      recentSearches: [],
      addRecentSearch: (query: string) => set((state) => ({
        recentSearches: [query, ...state.recentSearches.filter(s => s !== query)].slice(0, 10)
      })),
      clearRecentSearches: () => set({ recentSearches: [] }),
    }),
    {
      name: 'tigersmartchain-searches',
    }
  )
)

interface WalletState {
  watchedAddresses: string[]
  addWatchedAddress: (address: string) => void
  removeWatchedAddress: (address: string) => void
}

export const useWalletStore = create<WalletState>()(
  persist(
    (set) => ({
      watchedAddresses: [],
      addWatchedAddress: (address: string) => set((state) => ({
        watchedAddresses: [...new Set([address, ...state.watchedAddresses])]
      })),
      removeWatchedAddress: (address: string) => set((state) => ({
        watchedAddresses: state.watchedAddresses.filter(a => a !== address)
      })),
    }),
    {
      name: 'tigersmartchain-wallets',
    }
  )
)
