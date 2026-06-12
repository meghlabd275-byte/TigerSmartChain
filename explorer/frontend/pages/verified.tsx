// Verified Contracts Hub - Browse and search verified smart contracts
import { useState, useEffect } from 'react';
import Header from '../components/Header';

interface VerifiedContract {
  address: string;
  name: string;
  compiler: string;
  version: string;
  license: string;
  optimizations: boolean;
  proxy: boolean;
  matches: number;
  txns: number;
  holders: number;
}

interface SearchFilters {
  query: string;
  compiler: string;
  license: string;
  optimized: string;
  proxy: string;
}

export default function VerifiedContractsPage() {
  const [contracts, setContracts] = useState<VerifiedContract[]>([]);
  const [loading, setLoading] = useState(true);
  const [filters, setFilters] = useState<SearchFilters>({
    query: '',
    compiler: '',
    license: '',
    optimized: '',
    proxy: '',
  });
  const [sortBy, setSortBy] = useState('txns');
  const [page, setPage] = useState(1);
  const [totalPages, setTotalPages] = useState(1);

  useEffect(() => {
    loadContracts();
  }, [filters, sortBy, page]);

  const loadContracts = async () => {
    setLoading(true);
    try {
      const params = new URLSearchParams();
      if (filters.query) params.append('q', filters.query);
      if (filters.compiler) params.append('compiler', filters.compiler);
      if (filters.license) params.append('license', filters.license);
      if (filters.optimized) params.append('optimized', filters.optimized);
      if (filters.proxy) params.append('proxy', filters.proxy);
      params.append('sort', sortBy);
      params.append('page', page.toString());
      params.append('limit', '50');

      const response = await fetch(
        `${process.env.NEXT_PUBLIC_API_URL || ''}/contracts/verified?${params}`
      );
      const data = await response.json();

      if (data.result) {
        setContracts(data.result.contracts || []);
        setTotalPages(data.result.totalPages || 1);
      }
    } catch (error) {
      console.error('Failed to load contracts:', error);
    } finally {
      setLoading(false);
    }
  };

  const handleFilterChange = (key: keyof SearchFilters, value: string) => {
    setFilters(prev => ({ ...prev, [key]: value }));
    setPage(1);
  };

  const compilers = ['Solidity', 'Vyper', 'Yul'];
  const licenses = ['MIT', 'GPL-3.0', 'BSD-3-Clause', 'Apache-2.0', 'UNLICENSED'];

  return (
    <div className="min-h-screen bg-gray-50">
      <Header />
      
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900">Verified Contracts</h1>
          <p className="mt-2 text-gray-600">
            Browse and search verified smart contracts on TigerSmartChain
          </p>
        </div>

        {/* Search Bar */}
        <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-4 mb-6">
          <div className="flex gap-4">
            <div className="flex-1">
              <input
                type="text"
                placeholder="Search by contract name, address, or tx hash..."
                value={filters.query}
                onChange={(e) => handleFilterChange('query', e.target.value)}
                className="w-full px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500"
              />
            </div>
          </div>

          {/* Filters */}
          <div className="flex flex-wrap gap-4 mt-4">
            <select
              value={filters.compiler}
              onChange={(e) => handleFilterChange('compiler', e.target.value)}
              className="px-3 py-2 border border-gray-300 rounded-lg"
            >
              <option value="">All Compilers</option>
              {compilers.map(c => (
                <option key={c} value={c}>{c}</option>
              ))}
            </select>

            <select
              value={filters.license}
              onChange={(e) => handleFilterChange('license', e.target.value)}
              className="px-3 py-2 border border-gray-300 rounded-lg"
            >
              <option value="">All Licenses</option>
              {licenses.map(l => (
                <option key={l} value={l}>{l}</option>
              ))}
            </select>

            <select
              value={filters.optimized}
              onChange={(e) => handleFilterChange('optimized', e.target.value)}
              className="px-3 py-2 border border-gray-300 rounded-lg"
            >
              <option value="">Optimization</option>
              <option value="true">Optimized</option>
              <option value="false">Not Optimized</option>
            </select>

            <select
              value={filters.proxy}
              onChange={(e) => handleFilterChange('proxy', e.target.value)}
              className="px-3 py-2 border border-gray-300 rounded-lg"
            >
              <option value="">Proxy Type</option>
              <option value="upgradeable">Upgradeable</option>
              <option value="immutable">Immutable</option>
            </select>

            <select
              value={sortBy}
              onChange={(e) => setSortBy(e.target.value)}
              className="px-3 py-2 border border-gray-300 rounded-lg"
            >
              <option value="txns">Most Transactions</option>
              <option value="holders">Most Holders</option>
              <option value="name">Name</option>
              <option value="date">Date Verified</option>
            </select>
          </div>
        </div>

        {/* Results */}
        <div className="bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden">
          {loading ? (
            <div className="p-12 flex justify-center">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600"></div>
            </div>
          ) : contracts.length > 0 ? (
            <>
              <table className="w-full">
                <thead className="bg-gray-50">
                  <tr>
                    <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Contract</th>
                    <th className="px-6 py-3 text-left text-xs font-medium text-gray-500">Compiler</th>
                    <th className="px-6 py-3 text-left text-xs font-medium text-gray-500">License</th>
                    <th className="px-6 py-3 text-center text-xs font-medium text-gray-500">TXs</th>
                    <th className="px-6 py-3 text-center text-xs font-medium text-gray-500">Holders</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-gray-200">
                  {contracts.map((contract) => (
                    <tr key={contract.address} className="hover:bg-gray-50">
                      <td className="px-6 py-4">
                        <div className="flex items-center">
                          <div>
                            <a 
                              href={`/address/${contract.address}`}
                              className="font-medium text-gray-900 hover:text-blue-600"
                            >
                              {contract.name}
                            </a>
                            <div className="text-sm text-gray-500 font-mono">
                              {contract.address.slice(0, 6)}...{contract.address.slice(-4)}
                            </div>
                          </div>
                          {contract.proxy && (
                            <span className="ml-2 px-2 py-1 bg-purple-100 text-purple-800 text-xs rounded">
                              Proxy
                            </span>
                          )}
                          {contract.optimizations && (
                            <span className="ml-2 px-2 py-1 bg-green-100 text-green-800 text-xs rounded">
                              ✓
                            </span>
                          )}
                        </div>
                      </td>
                      <td className="px-6 py-4 text-sm text-gray-700">
                        {contract.compiler} v{contract.version}
                      </td>
                      <td className="px-6 py-4 text-sm text-gray-700">
                        {contract.license}
                      </td>
                      <td className="px-6 py-4 text-center text-sm text-gray-700">
                        {contract.txns.toLocaleString()}
                      </td>
                      <td className="px-6 py-4 text-center text-sm text-gray-700">
                        {contract.holders.toLocaleString()}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>

              {/* Pagination */}
              <div className="px-6 py-4 border-t border-gray-200 flex items-center justify-between">
                <button
                  onClick={() => setPage(p => Math.max(1, p - 1))}
                  disabled={page === 1}
                  className="px-4 py-2 border border-gray-300 rounded-lg disabled:opacity-50"
                >
                  Previous
                </button>
                <span className="text-gray-600">
                  Page {page} of {totalPages}
                </span>
                <button
                  onClick={() => setPage(p => Math.min(totalPages, p + 1))}
                  disabled={page === totalPages}
                  className="px-4 py-2 border border-gray-300 rounded-lg disabled:opacity-50"
                >
                  Next
                </button>
              </div>
            </>
          ) : (
            <div className="p-12 text-center text-gray-500">
              No verified contracts found
            </div>
          )}
        </div>

        {/* Stats */}
        <div className="grid grid-cols-4 gap-4 mt-8">
          <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
            <div className="text-sm text-gray-500">Total Verified</div>
            <div className="text-2xl font-bold text-gray-900">12,543</div>
          </div>
          <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
            <div className="text-sm text-gray-500">With Proxy</div>
            <div className="text-2xl font-bold text-gray-900">2,341</div>
          </div>
          <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
            <div className="text-sm text-gray-500">Optimized</div>
            <div className="text-2xl font-bold text-gray-900">9,842</div>
          </div>
          <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
            <div className="text-sm text-gray-500">24h Verifications</div>
            <div className="text-2xl font-bold text-gray-900">156</div>
          </div>
        </div>
      </main>
    </div>
  );
}