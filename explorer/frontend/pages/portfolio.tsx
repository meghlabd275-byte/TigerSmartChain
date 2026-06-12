// Multichain Portfolio Page - Cross-chain portfolio view
import { useState, useEffect } from 'react';
import Header from '../components/Header';

interface ChainPortfolio {
  chainId: number;
  chainName: string;
  nativeBalance: string;
  nativeValueUSD: number;
  tokens: TokenBalance[];
  nfts: NFTBalance[];
}

interface TokenBalance {
  chainId: number;
  address: string;
  name: string;
  symbol: string;
  balance: string;
  priceUSD: number;
  valueUSD: number;
}

interface NFTBalance {
  chainId: number;
  collection: string;
  tokenId: string;
  name: string;
  imageUrl: string;
}

interface Portfolio {
  address: string;
  chains: Record<number, ChainPortfolio>;
  totalValueUSD: number;
  tokens: TokenBalance[];
  nfts: NFTBalance[];
}

export default function PortfolioPage() {
  const [address, setAddress] = useState('');
  const [portfolio, setPortfolio] = useState<Portfolio | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [activeChain, setActiveChain] = useState<number | 'all'>('all');

  const supportedChains = [
    { id: 1, name: 'Ethereum', symbol: 'ETH', color: '#627EEA' },
    { id: 56, name: 'BNB Chain', symbol: 'BNB', color: '#F3BA2F' },
    { id: 137, name: 'Polygon', symbol: 'MATIC', color: '#8247E5' },
    { id: 42161, name: 'Arbitrum', symbol: 'ETH', color: '#28A0F0' },
    { id: 10, name: 'Optimism', symbol: 'ETH', color: '#FF0420' },
    { id: 8453, name: 'Base', symbol: 'ETH', color: '#0052FF' },
    { id: 9001, name: 'TigerSmartChain', symbol: 'TGR', color: '#FF6B35' },
  ];

  const loadPortfolio = async (addr: string) => {
    if (!addr) return;
    
    setLoading(true);
    setError('');

    try {
      const response = await fetch(
        `${process.env.NEXT_PUBLIC_API_URL || ''}/portfolio/${addr}`
      );
      const data = await response.json();

      if (data.result) {
        setPortfolio(data.result);
      } else {
        setPortfolio(null);
      }
    } catch (err) {
      setError('Failed to load portfolio');
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    loadPortfolio(address);
  };

  const filteredChains = activeChain === 'all' 
    ? Object.values(portfolio?.chains || {})
    : [portfolio?.chains[activeChain]].filter(Boolean);

  const formatValue = (value: number) => {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD',
      maximumFractionDigits: 2,
    }).format(value);
  };

  const getChainInfo = (chainId: number) => {
    return supportedChains.find(c => c.id === chainId) || { name: `Chain ${chainId}`, symbol: '?', color: '#888' };
  };

  return (
    <div className="min-h-screen bg-gray-50">
      <Header />
      
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900">Cross-Chain Portfolio</h1>
          <p className="mt-2 text-gray-600">View your portfolio across multiple chains</p>
        </div>

        {/* Search Form */}
        <form onSubmit={handleSubmit} className="mb-8">
          <div className="flex gap-4">
            <input
              type="text"
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              placeholder="Enter wallet address..."
              className="flex-1 px-4 py-3 border border-gray-300 rounded-xl focus:ring-2 focus:ring-blue-500"
            />
            <button
              type="submit"
              disabled={loading}
              className="px-6 py-3 bg-blue-600 text-white rounded-xl font-medium hover:bg-blue-700 disabled:opacity-50"
            >
              {loading ? 'Loading...' : 'View Portfolio'}
            </button>
          </div>
        </form>

        {error && (
          <div className="bg-red-50 border border-red-200 rounded-lg p-4 mb-6 text-red-700">
            {error}
          </div>
        )}

        {portfolio && (
          <>
            {/* Portfolio Summary */}
            <div className="bg-gradient-to-r from-blue-600 to-blue-800 rounded-2xl p-8 text-white mb-8">
              <div className="text-blue-100 mb-2">Total Portfolio Value</div>
              <div className="text-4xl font-bold mb-4">{formatValue(portfolio.totalValueUSD)}</div>
              <div className="flex gap-6">
                <div>
                  <div className="text-blue-200 text-sm">Chains</div>
                  <div className="text-2xl font-semibold">{Object.keys(portfolio.chains).length}</div>
                </div>
                <div>
                  <div className="text-blue-200 text-sm">Tokens</div>
                  <div className="text-2xl font-semibold">{portfolio.tokens.length}</div>
                </div>
                <div>
                  <div className="text-blue-200 text-sm">NFTs</div>
                  <div className="text-2xl font-semibold">{portfolio.nfts.length}</div>
                </div>
              </div>
            </div>

            {/* Chain Filter */}
            <div className="flex gap-2 mb-6 overflow-x-auto pb-2">
              <button
                onClick={() => setActiveChain('all')}
                className={`px-4 py-2 rounded-lg whitespace-nowrap ${
                  activeChain === 'all' ? 'bg-blue-600 text-white' : 'bg-white border border-gray-200'
                }`}
              >
                All Chains
              </button>
              {supportedChains.map(chain => (
                <button
                  key={chain.id}
                  onClick={() => setActiveChain(chain.id)}
                  className={`px-4 py-2 rounded-lg whitespace-nowrap ${
                    activeChain === chain.id ? 'bg-blue-600 text-white' : 'bg-white border border-gray-200'
                  }`}
                >
                  {chain.name}
                </button>
              ))}
            </div>

            {/* Chain Cards */}
            <div className="grid gap-6">
              {filteredChains.map(chainPortfolio => {
                const chainInfo = getChainInfo(chainPortfolio.chainId);
                const chainValue = chainPortfolio.nativeValueUSD + 
                  chainPortfolio.tokens.reduce((sum, t) => sum + t.valueUSD, 0);

                return (
                  <div key={chainPortfolio.chainId} className="bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden">
                    <div className="bg-gray-50 px-6 py-4 flex items-center justify-between">
                      <div className="flex items-center gap-3">
                        <div 
                          className="w-10 h-10 rounded-full flex items-center justify-center text-white font-bold"
                          style={{ backgroundColor: chainInfo.color }}
                        >
                          {chainInfo.symbol.charAt(0)}
                        </div>
                        <div>
                          <div className="font-semibold text-gray-900">{chainInfo.name}</div>
                          <div className="text-sm text-gray-500">
                            {chainPortfolio.nativeBalance ? parseFloat(chainPortfolio.nativeBalance).toFixed(4) : '0'} {chainInfo.symbol}
                          </div>
                        </div>
                      </div>
                      <div className="text-right">
                        <div className="text-xl font-bold text-gray-900">{formatValue(chainValue)}</div>
                      </div>
                    </div>

                    {/* Tokens */}
                    {chainPortfolio.tokens.length > 0 && (
                      <div className="p-6">
                        <h3 className="text-sm font-medium text-gray-500 mb-4">Tokens</h3>
                        <div className="space-y-3">
                          {chainPortfolio.tokens.map((token, i) => (
                            <div key={i} className="flex items-center justify-between">
                              <div className="flex items-center gap-3">
                                <div className="w-8 h-8 bg-gray-200 rounded-full"></div>
                                <div>
                                  <div className="font-medium text-gray-900">{token.symbol}</div>
                                  <div className="text-sm text-gray-500">{token.name}</div>
                                </div>
                              </div>
                              <div className="text-right">
                                <div className="font-medium text-gray-900">{formatValue(token.valueUSD)}</div>
                                <div className="text-sm text-gray-500">{parseFloat(token.balance).toFixed(4)}</div>
                              </div>
                            </div>
                          ))}
                        </div>
                      </div>
                    )}

                    {/* NFTs */}
                    {chainPortfolio.nfts.length > 0 && (
                      <div className="p-6 border-t border-gray-200">
                        <h3 className="text-sm font-medium text-gray-500 mb-4">NFTs</h3>
                        <div className="grid grid-cols-4 gap-4">
                          {chainPortfolio.nfts.slice(0, 8).map((nft, i) => (
                            <div key={i} className="aspect-square bg-gray-100 rounded-lg overflow-hidden">
                              {nft.imageUrl ? (
                                <img src={nft.imageUrl} alt={nft.name} className="w-full h-full object-cover" />
                              ) : (
                                <div className="w-full h-full flex items-center justify-center text-gray-400">🖼️</div>
                              )}
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </>
        )}

        {!portfolio && !loading && (
          <div className="text-center py-12 text-gray-500">
            Enter a wallet address to view your cross-chain portfolio
          </div>
        )}
      </main>
    </div>
  );
}