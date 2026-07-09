'use client'

import Link from 'next/link'
import { Github, Twitter, Discord, Telegram, Mail } from 'lucide-react'

const footerLinks = {
  Blockchain: [
    { name: 'Blocks', href: '/blocks' },
    { name: 'Transactions', href: '/txs' },
    { name: 'Pending Transactions', href: '/txs/pending' },
    { name: 'Uncle Blocks', href: '/uncles' },
  ],
  Tokens: [
    { name: 'BEP20 Tokens', href: '/tokens' },
    { name: 'Top Tokens', href: '/tokens/top' },
    { name: 'New Tokens', href: '/tokens/new' },
    { name: 'Token Approvals', href: '/approvals' },
  ],
  NFTs: [
    { name: 'Collections', href: '/nfts' },
    { name: 'Marketplace', href: '/nfts/market' },
    { name: 'Mint Tracker', href: '/nfts/mints' },
    { name: 'Floor Prices', href: '/nfts/floor' },
  ],
  Developers: [
    { name: 'API Documentation', href: '/docs/api' },
    { name: 'Verify Contract', href: '/contracts/verify' },
    { name: 'API Keys', href: '/developers' },
    { name: 'Rate Limits', href: '/docs/rate-limits' },
  ],
}

export function Footer() {
  return (
    <footer className="bg-white dark:bg-dark-900 border-t border-gray-200 dark:border-dark-800">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-12">
        <div className="grid grid-cols-2 md:grid-cols-5 gap-8">
          {/* Brand */}
          <div className="col-span-2">
            <div className="flex items-center space-x-2 mb-4">
              <div className="w-10 h-10 bg-gradient-to-br from-primary-500 to-primary-700 rounded-xl flex items-center justify-center">
                <svg className="w-6 h-6 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
              </div>
              <span className="text-xl font-bold gradient-text">TigerSmartChain</span>
            </div>
            <p className="text-gray-500 dark:text-gray-400 text-sm mb-4 max-w-xs">
              The most advanced BNB Smart Chain explorer with real-time data, analytics, and comprehensive blockchain insights.
            </p>
            <div className="flex space-x-4">
              <a href="#" className="text-gray-400 hover:text-primary-500 transition-colors">
                <Twitter className="w-5 h-5" />
              </a>
              <a href="#" className="text-gray-400 hover:text-primary-500 transition-colors">
                <Discord className="w-5 h-5" />
              </a>
              <a href="#" className="text-gray-400 hover:text-primary-500 transition-colors">
                <Telegram className="w-5 h-5" />
              </a>
              <a href="#" className="text-gray-400 hover:text-primary-500 transition-colors">
                <Github className="w-5 h-5" />
              </a>
              <a href="#" className="text-gray-400 hover:text-primary-500 transition-colors">
                <Mail className="w-5 h-5" />
              </a>
            </div>
          </div>

          {/* Links */}
          {Object.entries(footerLinks).map(([category, links]) => (
            <div key={category}>
              <h3 className="text-sm font-semibold text-gray-900 dark:text-white uppercase tracking-wider mb-4">
                {category}
              </h3>
              <ul className="space-y-3">
                {links.map((link) => (
                  <li key={link.name}>
                    <Link
                      href={link.href}
                      className="text-sm text-gray-500 dark:text-gray-400 hover:text-primary-500 transition-colors"
                    >
                      {link.name}
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>

        <div className="mt-12 pt-8 border-t border-gray-200 dark:border-dark-800">
          <div className="flex flex-col md:flex-row justify-between items-center">
            <p className="text-sm text-gray-500 dark:text-gray-400">
              © {new Date().getFullYear()} TigerSmartChain. All rights reserved.
            </p>
            <div className="flex space-x-6 mt-4 md:mt-0">
              <Link href="/terms" className="text-sm text-gray-500 dark:text-gray-400 hover:text-primary-500">
                Terms of Service
              </Link>
              <Link href="/privacy" className="text-sm text-gray-500 dark:text-gray-400 hover:text-primary-500">
                Privacy Policy
              </Link>
              <Link href="/docs" className="text-sm text-gray-500 dark:text-gray-400 hover:text-primary-500">
                Documentation
              </Link>
            </div>
          </div>
        </div>
      </div>
    </footer>
  )
}
