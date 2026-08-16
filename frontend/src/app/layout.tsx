import type { Metadata } from 'next'
import { Inter } from 'next/font/google'
import './globals.css'
import { Providers } from './providers'
import { Navbar } from '@/components/Navbar'
import { Footer } from '@/components/Footer'
import { Toaster } from 'react-hot-toast'

const inter = Inter({ subsets: ['latin'] })

export const metadata: Metadata = {
  title: 'TigerSmartChain | BNB Smart Chain Explorer',
  description: 'A comprehensive blockchain explorer for BNB Smart Chain with real-time data, analytics, and advanced search capabilities.',
  keywords: ['BNB', 'BSC', 'Blockchain', 'Explorer', 'Crypto', 'Web3', 'DeFi'],
}

export default function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <script
          dangerouslySetInnerHTML={{
            __html: `(function(){try{var t=localStorage.getItem('tigersmartchain-theme');var theme=t?JSON.parse(t).state?.theme:'dark';var root=document.documentElement;root.classList.remove('light','dark');root.classList.add(theme||'dark');}catch(e){document.documentElement.classList.add('dark');}})();`,
          }}
        />
      </head>
      <body className={inter.className}>
        <Providers>
          <div className="min-h-screen flex flex-col bg-gray-50 dark:bg-dark-900">
            <Navbar />
            <main className="flex-1">
              {children}
            </main>
            <Footer />
          </div>
          <Toaster 
            position="bottom-right"
            toastOptions={{
              duration: 4000,
              style: {
                background: '#0f172a',
                color: '#fff',
              },
            }}
          />
        </Providers>
      </body>
    </html>
  )
}
