'use client'

import Link from 'next/link'
import { usePathname } from 'next/navigation'
import { Search, Menu, X, Sun, Moon, Wallet } from 'lucide-react'
import { useState } from 'react'

const navItems = [
  { name: 'Home', href: '/' },
  { name: 'Blocks', href: '/blocks' },
  { name: 'Transactions', href: '/txs' },
  { name: 'Tokens', href: '/tokens' },
  { name: 'NFTs', href: '/nfts' },
  { name: 'DEX', href: '/dex' },
  { name: 'Gas', href: '/gas' },
]

export default function Navbar() {
  const pathname = usePathname()
  const [isOpen, setIsOpen] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault()
    if (searchQuery) {
      window.location.href = `/search?q=${encodeURIComponent(searchQuery)}`
    }
  }

  return (
    <nav className="bg-white dark:bg-dark-800 border-b border-gray-200 dark:border-dark-700 sticky top-0 z-50">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="flex justify-between h-16">
          <div className="flex items-center">
            <Link href="/" className="flex-shrink-0 flex items-center">
              <div className="w-8 h-8 bg-primary-500 rounded-lg flex items-center justify-center">
                <span className="text-white font-bold">T</span>
              </div>
              <span className="ml-2 text-xl font-bold">TigerScan</span>
            </Link>
            <div className="hidden md:ml-8 md:flex md:space-x-4">
              {navItems.map((item) => (
                <Link key={item.name} href={item.href} className={`px-3 py-2 rounded-md text-sm font-medium ${pathname === item.href ? 'text-primary-500' : 'text-gray-600'}`}>
                  {item.name}
                </Link>
              ))}
            </div>
          </div>
          <div className="flex items-center space-x-4">
            <form onSubmit={handleSearch} className="hidden md:block">
              <div className="relative">
                <input type="text" value={searchQuery} onChange={(e) => setSearchQuery(e.target.value)} placeholder="Search..." className="w-64 px-4 py-2 pl-10 bg-gray-100 rounded-lg text-sm focus:ring-2 focus:ring-primary-500" />
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
              </div>
            </form>
            <button className="flex items-center px-4 py-2 bg-primary-500 text-white rounded-lg"><Wallet className="w-4 h-4 mr-2" />Connect</button>
            <button onClick={() => setIsOpen(!isOpen)} className="md:hidden"><Menu className="w-6 h-6" /></button>
          </div>
        </div>
      </div>
      {isOpen && (
        <div className="md:hidden border-t p-4">
          {navItems.map((item) => <Link key={item.name} href={item.href} className="block py-2">{item.name}</Link>)}
        </div>
      )}
    </nav>
  )
}
