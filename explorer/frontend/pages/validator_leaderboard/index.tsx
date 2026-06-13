// Validator Leaderboard - Real-time validator rankings
import { useState, useEffect } from 'react';

interface Validator {
  address: string;
  moniker: string;
  self_delegation: string;
  delegation: string;
  total_stake: string;
  uptime: number;
  blocks_count: number;
  misses_count: number;
  is_active: boolean;
}

export default function ValidatorLeaderboard() {
  const [validators, setValidators] = useState<Validator[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [sortBy, setSortBy] = useState<'stake' | 'uptime' | 'blocks'>('stake');

  useEffect(() => {
    async function fetchValidators() {
      try {
        const response = await fetch('/api/v1/validators');
        if (!response.ok) throw new Error('Failed to fetch validators');
        const data = await response.json();
        setValidators(data.validators || []);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Unknown error');
      } finally {
        setLoading(false);
      }
    }
    fetchValidators();
    const interval = setInterval(fetchValidators, 60000);
    return () => clearInterval(interval);
  }, []);

  const sortedValidators = [...validators].sort((a, b) => {
    if (sortBy === 'stake') return Number(b.total_stake) - Number(a.total_stake);
    if (sortBy === 'uptime') return b.uptime - a.uptime;
    return b.blocks_count - a.blocks_count;
  });

  if (loading) return <div className="p-8">Loading...</div>;
  if (error) return <div className="p-8 text-red-500">Error: {error}</div>;

  return (
    <div className="container mx-auto px-4 py-8">
      <h1 className="text-3xl font-bold mb-8">Validator Leaderboard</h1>

      {/* Sort Options */}
      <div className="flex gap-4 mb-6">
        <button
          onClick={() => setSortBy('stake')}
          className={`px-4 py-2 rounded ${sortBy === 'stake' ? 'bg-blue-600 text-white' : 'bg-gray-200'}`}
        >
          By Stake
        </button>
        <button
          onClick={() => setSortBy('uptime')}
          className={`px-4 py-2 rounded ${sortBy === 'uptime' ? 'bg-blue-600 text-white' : 'bg-gray-200'}`}
        >
          By Uptime
        </button>
        <button
          onClick={() => setSortBy('blocks')}
          className={`px-4 py-2 rounded ${sortBy === 'blocks' ? 'bg-blue-600 text-white' : 'bg-gray-200'}`}
        >
          By Blocks
        </button>
      </div>

      {/* Table */}
      <div className="overflow-x-auto">
        <table className="min-w-full bg-white dark:bg-gray-800 rounded-lg shadow">
          <thead className="bg-gray-100 dark:bg-gray-700">
            <tr>
              <th className="px-4 py-3 text-left">Rank</th>
              <th className="px-4 py-3 text-left">Validator</th>
              <th className="px-4 py-3 text-right">Self Stake</th>
              <th className="px-4 py-3 text-right">Total Stake</th>
              <th className="px-4 py-3 text-right">Uptime</th>
              <th className="px-4 py-3 text-right">Blocks</th>
              <th className="px-4 py-3 text-center">Status</th>
            </tr>
          </thead>
          <tbody>
            {sortedValidators.map((validator, index) => (
              <tr key={validator.address} className="border-t border-gray-200 dark:border-gray-700">
                <td className="px-4 py-3">
                  {index < 3 ? (
                    <span className={`text-lg font-bold ${
                      index === 0 ? 'text-yellow-500' : 
                      index === 1 ? 'text-gray-400' : 
                      'text-orange-500'
                    }`}>
                      {index + 1}
                    </span>
                  ) : (
                    <span className="text-gray-500">{index + 1}</span>
                  )}
                </td>
                <td className="px-4 py-3">
                  <div>
                    <p className="font-medium">{validator.moniker || validator.address}</p>
                    <p className="text-xs text-gray-500">{validator.address}</p>
                  </div>
                </td>
                <td className="px-4 py-3 text-right">
                  {Number(validator.self_delegation / 1e18).toFixed(2)} TGR
                </td>
                <td className="px-4 py-3 text-right">
                  {Number(validator.total_stake / 1e18).toFixed(2)} TGR
                </td>
                <td className="px-4 py-3 text-right">
                  <span className={validator.uptime > 99 ? 'text-green-600' : validator.uptime > 95 ? 'text-yellow-600' : 'text-red-600'}>
                    {validator.uptime.toFixed(2)}%
                  </span>
                </td>
                <td className="px-4 py-3 text-right">{validator.blocks_count.toLocaleString()}</td>
                <td className="px-4 py-3 text-center">
                  <span className={`px-2 py-1 rounded text-xs ${
                    validator.is_active 
                      ? 'bg-green-100 text-green-800' 
                      : 'bg-red-100 text-red-800'
                  }`}>
                    {validator.is_active ? 'Active' : 'Inactive'}
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}