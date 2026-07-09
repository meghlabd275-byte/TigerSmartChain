'use client'

import { useState, useEffect, useRef } from 'react'
import Link from 'next/link'
import { usePathname } from 'next/navigation'
import { 
  Search, 
  Menu, 
  X, 
  Sun, 
  Moon, 
  ChevronDown,
  Box,
  FileText,
  Coins,
  Image,
  Activity,
  Zap,
  Database,
  Globe
} from 'lucide-react'
import { useThemeStore, useSearchStore } from '@/lib/store'
import { formatAddress, isValidAddress, isValidHash } from '@/lib/utils'
import { SearchAutocomplete } from './SearchAutocomplete'

const navItems = [
  { name: 'Home', href: '/' },
  { name: 'Blockchain', href: '/blocks', icon: Database },
  { name: 'Transactions', href: '/txs', icon: FileText },
  { name: 'Tokens', href: '/tokens', icon: Coins },
  { name: 'NFTs', href: '/nfts', icon: Image },
  { name: 'Analytics', href: '/charts', icon: Activity },
]

export function Navbar() {
  const [isOpen, setIsOpen] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [showSearch, setShowSearch] = useState(false)
  const { theme, toggleTheme } = useThemeStore()
  const pathname = usePathname()
  const searchRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (searchRef.current && !searchRef.current.contains(event.target as Node)) {
        setShowSearch(false)
      }
    }
    document.addEventListener('mousedown', handleClickOutside)
    return () => document.removeEventListener('mousedown', handleClickOutside)
  }, [])

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault()
    if (searchQuery.trim()) {
      window.location.href = `/search?q=${encodeURIComponent(searchQuery.trim())}`
    }
  }

  const handleSearchSelect = (result: { type: string; value: string }) => {
    setShowSearch(false)
    setSearchQuery('')
    switch (result.type) {
      case 'address':
      case 'token':
      case 'nft':
        window.location.href = `/address/${result.value}`
        break
      case 'transaction':
        window.location.href = `/tx/${result.value}`
        break
      case 'block':
        window.location.href = `/block/${result.value}`
        break
      default:
        window.location.href = `/search?q=${encodeURIComponent(result.value)}`
    }
  }

  return (
    <nav className="sticky top-0 z-50 bg-white/80 dark:bg-dark-900/80 backdrop-blur-xl border-b border-gray-200 dark:border-dark-800">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="flex justify-between items-center h-16">
          {/* Logo */}
          <div className="flex items-center space-x-4">
            <Link href="/" className="flex items-center space-x-2">
              <div className="w-10 h-10 bg-gradient-to-br from-primary-500 to-primary-700 rounded-xl flex items-center justify-center">
                <Zap className="w-6 h-6 text-white" />
              </div>
              <span className="text-xl font-bold gradient-text hidden sm:block">
                TigerSmartChain
              </span>
            </Link>
          </div>

          {/* Desktop Navigation */}
          <div className="hidden md:flex items-center space-x-1">
            {navItems.map((item) => {
              const Icon = item.icon
              const isActive = pathname === item.href || pathname.startsWith(item.href + '/')
              return (
                <Link
                  key={item.name}
                  href={item.href}
                  className={`flex items-center px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                    isActive
                      ? 'bg-primary-100 dark:bg-primary-900/30 text-primary-700 dark:text-primary-300'
                      : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-dark-800'
                  }`}
                >
                  <Icon className="w-4 h-4 mr-2" />
                  {item.name}
                </Link>
              )
            })}
          </div>

          {/* Search & Actions */}
          <div className="flex items-center space-x-3">
            {/* Search Bar */}
            <div ref={searchRef} className="relative hidden sm:block">
              <form onSubmit={handleSearch}>
                <div className="relative">
                  <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
                  <input
                    type="text"
                    value={searchQuery}
                    onChange={(e) => {
                      setSearchQuery(e.target.value)
                      setShowSearch(e.target.value.length > 0)
                    }}
                    onFocus={() => searchQuery.length > 0 && setShowSearch(true)}
                    placeholder="Search by address, txn hash, block..."
                    className="w-64 lg:w-80 pl-10 pr-4 py-2 bg-gray-100 dark:bg-dark-800 border-0 rounded-lg text-sm placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-primary-500 transition-all"
                  />
                </div>
              </form>
              {showSearch && (
                <SearchAutocomplete
                  query={searchQuery}
                  onSelect={handleSearchSelect}
                  onClose={() => setShowSearch(false)}
                />
              )}
            </div>

            {/* Theme Toggle */}
            <button
              onClick={toggleTheme}
              className="p-2 rounded-lg hover:bg-gray-100 dark:hover:bg-dark-800 transition-colors"
              aria-label="Toggle theme"
            >
              {theme === 'dark' ? (
                <Sun className="w-5 h-5 text-gray-400" />
              ) : (
                <Moon className="w-5 h-5 text-gray-600" />
              )}
            </button>

            {/* Mobile Menu Button */}
            <button
              onClick={() => setIsOpen(!isOpen)}
              className="md:hidden p-2 rounded-lg hover:bg-gray-100 dark:hover:bg-dark-800"
            >
              {isOpen ? (
                <X className="w-6 h-6 text-gray-600 dark:text-gray-300" />
              ) : (
                <Menu className="w-6 h-6 text-gray-600 dark:text-gray-300" />
              )}
            </button>
          </div>
        </div>
      </div>

      {/* Mobile Navigation */}
      {isOpen && (
        <div className="md:hidden border-t border-gray-200 dark:border-dark-800 animate-slide-down">
          <div className="px-4 py-4 space-y-2">
            {/* Mobile Search */}
            <form onSubmit={handleSearch} className="mb-4">
              <div className="relative">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
                <input
                  type="text"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  placeholder="Search..."
                  className="w-full pl-10 pr-4 py-2 bg-gray-100 dark:bg-dark-800 border-0 rounded-lg text-sm"
                />
              </div>
            </form>

            {navItems.map((item) => {
              const Icon = item.icon
              const isActive = pathname === item.href
              return (
                <Link
                  key={item.name}
                  href={item.href}
                  onClick={() => setIsOpen(false)}
                  className={`flex items-center px-4 py-3 rounded-lg text-sm font-medium ${
                    isActive
                      ? 'bg-primary-100 dark:bg-primary-900/30 text-primary-700'
                      : 'text-gray-600 dark:text-gray-300'
                  }`}
                >
                  <Icon className="w-5 h-5 mr-3" />
                  {item.name}
                </Link>
              )
            })}
          </div>
        </div>
      )}
    </nav>
  )
}
