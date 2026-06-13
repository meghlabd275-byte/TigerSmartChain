// Multi-chain Dashboard - Real-time cross-chain view
import { useState, useEffect } from 'react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';

interface ChainData {
  chainId: number;
  chainName: string;
  symbol: string;
  tvl: number;
  txCount: number;
  blocks: number;
  validators: number;
}

interface Stats {
  totalTvl: number;
  totalTx: number;
  chains: ChainData[];
}

export default function MultiChainDashboard() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    async function fetchData() {
      try {
        const response = await fetch('/api/v1/stats');
        if (!response.ok) throw new Error('Failed to fetch stats');
        const data = await response.json();
        
        // Transform to multi-chain view
        setStats({
          totalTvl: data.total_tvl || 0,
          totalTx: data.total_transactions || 0,
          chains: [
            { chainId: 6666, chainName: 'TigerSmartChain', symbol: 'TGR', tvl: data.total_tvl || 0, txCount: data.total_transactions || 0, blocks: data.total_blocks || 0, validators: 21 }
          ]
        });
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Unknown error');
      } finally {
        setLoading(false);
      }
    }
    fetchData();
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, []);

  if (loading) return <div className="p-8">Loading...</div>;
  if (error) return <div className="p-8 text-red-500">Error: {error}</div>;

  return (
    <div className="container mx-auto px-4 py-8">
      <h1 className="text-3xl font-bold mb-8">Multi-Chain Dashboard</h1>
      
      {/* Summary Cards */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-8">
        <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
          <h3 className="text-gray-500 text-sm">Total TVL</h3>
          <p className="text-2xl font-bold">${stats?.totalTvl.toLocaleString() || 0}</p>
        </div>
        <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
          <h3 className="text-gray-500 text-sm">Total Transactions</h3>
          <p className="text-2xl font-bold">{stats?.totalTx.toLocaleString() || 0}</p>
        </div>
        <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
          <h3 className="text-gray-500 text-sm">Connected Chains</h3>
          <p className="text-2xl font-bold">{stats?.chains.length || 0}</p>
        </div>
      </div>

      {/* Chain Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        {stats?.chains.map((chain) => (
          <div key={chain.chainId} className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
            <div className="flex justify-between items-start mb-4">
              <div>
                <h3 className="text-xl font-bold">{chain.chainName}</h3>
                <p className="text-gray-500">Chain ID: {chain.chainId}</p>
              </div>
              <span className="bg-green-100 text-green-800 px-2 py-1 rounded text-sm">Active</span>
            </div>
            <div className="space-y-2">
              <div className="flex justify-between">
                <span className="text-gray-500">Native Token</span>
                <span className="font-medium">{chain.symbol}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-500">TVL</span>
                <span className="font-medium">${chain.tvl.toLocaleString()}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-500">Transactions</span>
                <span className="font-medium">{chain.txCount.toLocaleString()}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-500">Blocks</span>
                <span className="font-medium">{chain.blocks.toLocaleString()}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-500">Validators</span>
                <span className="font-medium">{chain.validators}</span>
              </div>
            </div>
          </div>
        ))}
      </div>

      {/* Add Chain Button */}
      <div className="mt-8 text-center">
        <button className="bg-blue-600 hover:bg-blue-700 text-white px-6 py-2 rounded-lg">
          Add Another Chain
        </button>
      </div>
    </div>
  );
}