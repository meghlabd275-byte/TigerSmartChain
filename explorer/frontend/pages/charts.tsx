// Charts Dashboard - Advanced analytics with interactive charts
import { useState, useEffect } from 'react';
import { useSearchParams } from 'next/navigation';
import Header from '../components/Header';

interface ChartData {
  date: string;
  value: number;
}

interface DEXPair {
  address: string;
  token0Symbol: string;
  token1Symbol: string;
  liquidityUSD: number;
  volume24h: number;
  price0: number;
  price1: number;
  txCount: number;
}

export default function ChartsPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [activeTab, setActiveTab] = useState('tvl');
  const [chartData, setChartData] = useState<ChartData[]>([]);
  const [dexPairs, setDexPairs] = useState<DEXPair[]>([]);
  const [loading, setLoading] = useState(true);
  const [days, setDays] = useState(30);

  useEffect(() => {
    loadChartData();
    loadDEXPairs();
  }, [activeTab, days]);

  const loadChartData = async () => {
    setLoading(true);
    try {
      const endpoint = activeTab === 'tvl' ? '/charts/tvl' :
                    activeTab === 'transactions' ? '/charts/transactions' :
                    activeTab === 'accounts' ? '/charts/accounts' :
                    activeTab === 'gas' ? '/charts/gas' :
                    activeTab === 'nft' ? '/charts/nft-volume' :
                    activeTab === 'token' ? '/charts/token-volume' : '/charts/dex-volume';
      
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL || ''}${endpoint}?days=${days}`);
      const data = await response.json();
      
      if (data.result) {
        setChartData(data.result);
      }
    } catch (error) {
      console.error('Failed to load chart data:', error);
    } finally {
      setLoading(false);
    }
  };

  const loadDEXPairs = async () => {
    try {
      const response = await fetch(`${process.env.NEXT_PUBLIC_API_URL || ''}/dex/pairs?limit=20`);
      const data = await response.json();
      
      if (data.result) {
        setDexPairs(data.result);
      }
    } catch (error) {
      console.error('Failed to load DEX pairs:', error);
    }
  };

  const tabs = [
    { id: 'tvl', label: 'TVL', description: 'Total Value Locked' },
    { id: 'transactions', label: 'Transactions', description: 'Transaction Volume' },
    { id: 'accounts', label: 'Accounts', description: 'Active Accounts' },
    { id: 'gas', label: 'Gas', description: 'Gas Prices' },
    { id: 'nft', label: 'NFT', description: 'NFT Volume' },
    { id: 'token', label: 'Token', description: 'Token Volume' },
    { id: 'dex', label: 'DEX', description: 'DEX Volume' },
  ];

  const formatValue = (value: number, type: string) => {
    if (type === 'currency') {
      return new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD', maximumFractionDigits: 0 }).format(value);
    }
    if (type === 'compact') {
      return new Intl.NumberFormat('en-US', { notation: 'compact', maximumFractionDigits: 1 }).format(value);
    }
    return value.toLocaleString();
  };

  const maxValue = Math.max(...chartData.map(d => d.value), 1);

  return (
    <div className="min-h-screen bg-gray-50">
      <Header />
      
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {/* Page Header */}
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900">Analytics Dashboard</h1>
          <p className="mt-2 text-gray-600">Explore TigerSmartChain analytics and DeFi metrics</p>
        </div>

        {/* Time Range Selector */}
        <div className="flex gap-2 mb-6">
          {[7, 30, 90, 365].map(d => (
            <button
              key={d}
              onClick={() => setDays(d)}
              className={`px-4 py-2 rounded-lg font-medium transition-colors ${
                days === d
                  ? 'bg-blue-600 text-white'
                  : 'bg-white text-gray-700 border border-gray-200 hover:bg-gray-50'
              }`}
            >
              {d === 7 ? '7D' : d === 30 ? '30D' : d === 90 ? '90D' : '1Y'}
            </button>
          ))}
        </div>

        {/* Tabs */}
        <div className="border-b border-gray-200 mb-6">
          <nav className="flex gap-4">
            {tabs.map(tab => (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`py-3 px-1 border-b-2 font-medium text-sm transition-colors ${
                  activeTab === tab.id
                    ? 'border-blue-600 text-blue-600'
                    : 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300'
                }`}
              >
                {tab.label}
              </button>
            ))}
          </nav>
        </div>

        {/* Chart Area */}
        <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6 mb-8">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-lg font-semibold text-gray-900">
              {tabs.find(t => t.id === activeTab)?.description}
            </h2>
            {chartData.length > 0 && (
              <span className="text-2xl font-bold text-gray-900">
                {formatValue(chartData[chartData.length - 1]?.value || 0, activeTab === 'tvl' ? 'currency' : 'compact')}
              </span>
            )}
          </div>

          {loading ? (
            <div className="h-64 flex items-center justify-center">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600"></div>
            </div>
          ) : (
            <div className="h-64 flex items-end gap-1">
              {chartData.map((point, index) => (
                <div
                  key={index}
                  className="flex-1 bg-blue-600 rounded-t transition-all hover:bg-blue-700 relative group"
                  style={{
                    height: `${(point.value / maxValue) * 100}%`,
                    minHeight: point.value > 0 ? '4px' : '0'
                  }}
                >
                  <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 px-2 py-1 bg-gray-900 text-white text-xs rounded opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap z-10">
                    {point.date}: {formatValue(point.value, activeTab === 'tvl' ? 'currency' : 'compact')}
                  </div>
                </div>
              ))}
            </div>
          )}

          <div className="flex justify-between mt-2 text-xs text-gray-500">
            <span>{chartData[0]?.date}</span>
            <span>{chartData[chartData.length - 1]?.date}</span>
          </div>
        </div>

        {/* DEX Pairs Table */}
        <div className="bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden">
          <div className="px-6 py-4 border-b border-gray-200">
            <h2 className="text-lg font-semibold text-gray-900">Top DEX Pairs</h2>
          </div>
          
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Pair</th>
                  <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase tracking-wider">Liquidity</th>
                  <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase tracking-wider">Volume 24h</th>
                  <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase tracking-wider">Price</th>
                  <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase tracking-wider">TXs</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200">
                {dexPairs.map((pair, index) => (
                  <tr key={pair.address} className="hover:bg-gray-50">
                    <td className="px-6 py-4 whitespace-nowrap">
                      <div className="flex items-center">
                        <span className="text-sm text-gray-500 mr-2">{index + 1}</span>
                        <span className="font-medium text-gray-900">
                          {pair.token0Symbol}/{pair.token1Symbol}
                        </span>
                      </div>
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-right text-gray-900">
                      {formatValue(pair.liquidityUSD, 'currency')}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-right text-gray-900">
                      {formatValue(pair.volume24h, 'currency')}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-right text-gray-900">
                      ${pair.price1.toFixed(6)}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-right text-gray-900">
                      {pair.txCount.toLocaleString()}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          
          {dexPairs.length === 0 && !loading && (
            <div className="px-6 py-12 text-center text-gray-500">
              No DEX pairs found
            </div>
          )}
        </div>

        {/* Network Stats */}
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mt-8">
          <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
            <div className="text-sm text-gray-500 mb-1">Total Value Locked</div>
            <div className="text-2xl font-bold text-gray-900">$1.2B</div>
            <div className="text-sm text-green-600 mt-1">+5.2%</div>
          </div>
          <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
            <div className="text-sm text-gray-500 mb-1">24h Volume</div>
            <div className="text-2xl font-bold text-gray-900">$850M</div>
            <div className="text-sm text-green-600 mt-1">+12.5%</div>
          </div>
          <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
            <div className="text-sm text-gray-500 mb-1">Active Addresses</div>
            <div className="text-2xl font-bold text-gray-900">2.5M</div>
            <div className="text-sm text-green-600 mt-1">+8.3%</div>
          </div>
          <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
            <div className="text-sm text-gray-500 mb-1">Transactions Today</div>
            <div className="text-2xl font-bold text-gray-900">4.2M</div>
            <div className="text-sm text-green-600 mt-1">+15.2%</div>
          </div>
        </div>
      </main>
    </div>
  );
}