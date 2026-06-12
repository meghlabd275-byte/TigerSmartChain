// Search Results Page - Dedicated search with filters
import { useState, useEffect } from 'react';
import { useSearchParams } from 'next/navigation';
import Header from '../components/Header';

interface SearchResult {
  type: 'address' | 'block' | 'transaction' | 'token' | 'nft' | 'contract';
  address?: string;
  hash?: string;
  number?: number;
  name?: string;
  symbol?: string;
}

export default function SearchPage() {
  const [searchParams] = useSearchParams();
  const query = searchParams.get('q') || '';
  const [results, setResults] = useState<SearchResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [activeFilter, setActiveFilter] = useState<string>('all');
  const [searchHistory, setSearchHistory] = useState<string[]>([]);

  useEffect(() => {
    if (query) {
      performSearch(query);
    }
    loadSearchHistory();
  }, [query]);

  const performSearch = async (q: string) => {
    setLoading(true);
    try {
      let url = `${process.env.NEXT_PUBLIC_API_URL || ''}/search?q=${encodeURIComponent(q)}`;
      if (activeFilter !== 'all') {
        url += `&type=${activeFilter}`;
      }

      const response = await fetch(url);
      const data = await response.json();

      if (data.result) {
        setResults(data.result);
      }
    } catch (error) {
      console.error('Search failed:', error);
    } finally {
      setLoading(false);
    }
  };

  const loadSearchHistory = () => {
    const history = localStorage.getItem('searchHistory');
    if (history) {
      setSearchHistory(JSON.parse(history));
    }
  };

  const saveToHistory = (q: string) => {
    const newHistory = [q, ...searchHistory.filter(h => h !== q)].slice(0, 10);
    setSearchHistory(newHistory);
    localStorage.setItem('searchHistory', JSON.stringify(newHistory));
  };

  const filters = [
    { id: 'all', label: 'All' },
    { id: 'address', label: 'Addresses' },
    { id: 'block', label: 'Blocks' },
    { id: 'transaction', label: 'Transactions' },
    { id: 'token', label: 'Tokens' },
    { id: 'nft', label: 'NFTs' },
    { id: 'contract', label: 'Contracts' },
  ];

  const getResultIcon = (type: string) => {
    switch (type) {
      case 'address':
        return '📍';
      case 'block':
        return '🧱';
      case 'transaction':
        return '📜';
      case 'token':
        return '🪙';
      case 'nft':
        return '🖼️';
      case 'contract':
        return '📝';
      default:
        return '❓';
    }
  };

  const getResultLink = (result: SearchResult) => {
    switch (result.type) {
      case 'address':
        return `/address/${result.address}`;
      case 'block':
        return `/block/${result.number}`;
      case 'transaction':
        return `/tx/${result.hash}`;
      case 'token':
        return `/token/${result.address}`;
      case 'nft':
        return `/nft/${result.address}`;
      case 'contract':
        return `/address/${result.address}`;
      default:
        return '#';
    }
  };

  const getResultTitle = (result: SearchResult) => {
    switch (result.type) {
      case 'address':
        return result.address;
      case 'block':
        return `Block #${result.number}`;
      case 'transaction':
        return result.hash;
      case 'token':
        return `${result.name} (${result.symbol})`;
      case 'nft':
        return result.name;
      case 'contract':
        return result.name || result.address;
      default:
        return 'Unknown';
    }
  };

  const handleFilterChange = (filter: string) => {
    setActiveFilter(filter);
    if (query) {
      performSearch(query);
    }
  };

  if (!query) {
    return (
      <div className="min-h-screen bg-gray-50">
        <Header />
        <main className="max-w-3xl mx-auto px-4 py-12">
          <h1 className="text-3xl font-bold text-gray-900 mb-8">Search</h1>
          
          {/* Search Input */}
          <form action="/search" method="GET" className="mb-8">
            <input
              type="text"
              name="q"
              placeholder="Search by address, transaction hash, block, token, or ENS..."
              className="w-full px-4 py-3 text-lg border border-gray-300 rounded-xl focus:ring-2 focus:ring-blue-500"
              autoFocus
            />
          </form>

          {/* Recent Searches */}
          {searchHistory.length > 0 && (
            <div>
              <h2 className="text-sm font-medium text-gray-500 mb-3">Recent Searches</h2>
              <div className="flex flex-wrap gap-2">
                {searchHistory.map((q, i) => (
                  <a
                    key={i}
                    href={`/search?q=${encodeURIComponent(q)}`}
                    className="px-3 py-1 bg-gray-100 rounded-full text-sm hover:bg-gray-200"
                  >
                    {q.slice(0, 12)}...
                  </a>
                ))}
              </div>
            </div>
          )}
        </main>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-gray-50">
      <Header />
      
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {/* Search Header */}
        <div className="mb-6">
          <form action="/search" method="GET" className="flex gap-2">
            <input
              type="text"
              name="q"
              defaultValue={query}
              placeholder="Search..."
              className="flex-1 px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500"
            />
            <button
              type="submit"
              className="px-4 py-2 bg-blue-600 text-white rounded-lg"
            >
              Search
            </button>
          </form>
        </div>

        {/* Filters */}
        <div className="flex gap-2 mb-6 overflow-x-auto pb-2">
          {filters.map(filter => (
            <button
              key={filter.id}
              onClick={() => handleFilterChange(filter.id)}
              className={`px-4 py-2 rounded-lg whitespace-nowrap ${
                activeFilter === filter.id
                  ? 'bg-blue-600 text-white'
                  : 'bg-white border border-gray-200 hover:bg-gray-50'
              }`}
            >
              {filter.label}
            </button>
          ))}
        </div>

        {/* Results */}
        <div className="bg-white rounded-xl shadow-sm border border-gray-200">
          {loading ? (
            <div className="p-12 flex justify-center">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600"></div>
            </div>
          ) : results.length > 0 ? (
            <div className="divide-y divide-gray-200">
              {results.map((result, index) => (
                <a
                  key={index}
                  href={getResultLink(result)}
                  className="flex items-center p-4 hover:bg-gray-50"
                >
                  <span className="text-2xl mr-4">{getResultIcon(result.type)}</span>
                  <div className="flex-1 min-w-0">
                    <div className="font-medium text-gray-900 truncate">
                      {getResultTitle(result)}
                    </div>
                    <div className="text-sm text-gray-500 capitalize">
                      {result.type}
                    </div>
                  </div>
                  <svg
                    className="w-5 h-5 text-gray-400"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M9 5l7 7-7 7"
                    />
                  </svg>
                </a>
              ))}
            </div>
          ) : (
            <div className="p-12 text-center">
              <div className="text-4xl mb-4">🔍</div>
              <h3 className="text-lg font-medium text-gray-900 mb-2">
                No results found
              </h3>
              <p className="text-gray-500">
                Try searching with different keywords or check the spelling
              </p>
            </div>
          )}
        </div>

        {/* Quick Links */}
        <div className="mt-8 grid grid-cols-2 md:grid-cols-4 gap-4">
          <a
            href="/search?q=0x"
            className="bg-white rounded-lg shadow-sm border border-gray-200 p-4 hover:border-blue-300"
          >
            <div className="text-2xl mb-2">📍</div>
            <div className="font-medium">Addresses</div>
          </a>
          <a
            href="/search?q=block"
            className="bg-white rounded-lg shadow-sm border border-gray-200 p-4 hover:border-blue-300"
          >
            <div className="text-2xl mb-2">🧱</div>
            <div className="font-medium">Blocks</div>
          </a>
          <a
            href="/search?q=tx"
            className="bg-white rounded-lg shadow-sm border border-gray-200 p-4 hover:border-blue-300"
          >
            <div className="text-2xl mb-2">📜</div>
            <div className="font-medium">Transactions</div>
          </a>
          <a
            href="/verified"
            className="bg-white rounded-lg shadow-sm border border-gray-200 p-4 hover:border-blue-300"
          >
            <div className="text-2xl mb-2">📝</div>
            <div className="font-medium">Verified Contracts</div>
          </a>
        </div>
      </main>
    </div>
  );
}