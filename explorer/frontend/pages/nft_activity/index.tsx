// NFT Activity - Live activity feed with advanced filtering and real-time updates
import { useState, useEffect, useMemo } from 'react';
import { useSearchParams } from 'next/navigation';

interface NFTAction {
    id: string;
    type: 'mint' | 'transfer' | 'sale' | 'burn' | 'bid' | 'list' | 'cancel_list';
    collection: string;
    collection_name?: string;
    token_id: string;
    from: string;
    to: string;
    price?: number;
    price_usd?: number;
    tx_hash: string;
    timestamp: number;
    block_number: number;
}

interface FilterState {
    type: string;
    collection: string;
    minPrice: string;
    maxPrice: string;
    search: string;
}

const ACTIVITY_TYPE_COLORS: Record<string, string> = {
    mint: 'bg-green-100 text-green-800',
    transfer: 'bg-blue-100 text-blue-800',
    sale: 'bg-purple-100 text-purple-800',
    burn: 'bg-red-100 text-red-800',
    bid: 'bg-yellow-100 text-yellow-800',
    list: 'bg-indigo-100 text-indigo-800',
    cancel_list: 'bg-gray-100 text-gray-800',
};

const ACTIVITY_TYPE_LABELS: Record<string, string> = {
    mint: 'Minted',
    transfer: 'Transferred',
    sale: 'Sold',
    burn: 'Burned',
    bid: 'Bid Placed',
    list: 'Listed',
    cancel_list: 'Listing Cancelled',
};

export default function NFTActivity() {
    const [activities, setActivities] = useState<NFTAction[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [filter, setFilter] = useState<FilterState>({
        type: '',
        collection: '',
        minPrice: '',
        maxPrice: '',
        search: '',
    });
    const [page, setPage] = useState(1);
    const [totalPages, setTotalPages] = useState(1);
    const [liveMode, setLiveMode] = useState(true);
    const [lastUpdate, setLastUpdate] = useState<Date | null>(null);

    // Generate sample data for demonstration
    const sampleActivities: NFTAction[] = useMemo(() => {
        const collections = [
            { addr: '0xBC4CA0Ed7644E63388EaE0d6f5A84d8F1F1A7bF3', name: 'BoredApeYachtClub' },
            { addr: '0x23502f47eF5eB8C9C0c1b2F2E5D6F8A1B3C4D5E6', name: 'CryptoPunks' },
            { addr: '0x49cF6f5B5B8C9A0d1E2F3G4H5I6J7K8L9M0N1', name: 'Azuki' },
            { addr: '0xA86cD0f2B3C4e5F6a7b8c9D0e1F2g3H4i5J6', name: 'Otherdeed' },
            { addr: '0x1234567890abcdef1234567890abcdef12345678', name: 'PudgyPenguins' },
        ];
        
        const types: NFTAction['type'][] = ['mint', 'transfer', 'sale', 'burn', 'bid', 'list', 'cancel_list'];
        const actions: NFTAction[] = [];
        
        for (let i = 0; i < 100; i++) {
            const coll = collections[Math.floor(Math.random() * collections.length)];
            const type = types[Math.floor(Math.random() * types.length)];
            const from = '0x' + Math.random().toString(16).substring(2, 42).padStart(40, '0').substring(0, 40);
            const to = '0x' + Math.random().toString(16).substring(2, 42).padStart(40, '0').substring(0, 40);
            const price = type === 'sale' || type === 'list' ? Math.random() * 10 + 0.1 : undefined;
            
            actions.push({
                id: `activity-${i}`,
                type,
                collection: coll.addr,
                collection_name: coll.name,
                token_id: String(Math.floor(Math.random() * 10000)),
                from,
                to,
                price,
                price_usd: price ? price * 3500 : undefined,
                tx_hash: '0x' + Math.random().toString(16).substring(2, 66).padStart(64, '0').substring(0, 64),
                timestamp: Date.now() - Math.floor(Math.random() * 7 * 24 * 60 * 60 * 1000),
                block_number: 15000000 + Math.floor(Math.random() * 100000),
            });
        }
        
        return actions.sort((a, b) => b.timestamp - a.timestamp);
    }, []);

    useEffect(() => {
        async function fetchActivities() {
            try {
                setLoading(true);
                // Simulate API call
                await new Promise(resolve => setTimeout(resolve, 500));
                
                // In production, this would be:
                // const res = await fetch('/api/v1/nfts/activity');
                // const data = await res.json();
                
                setActivities(sampleActivities);
                setTotalPages(Math.ceil(sampleActivities.length / 20));
                setLastUpdate(new Date());
            } catch (e) {
                console.error(e);
                setError('Failed to load NFT activity');
            }
            setLoading(false);
        }
        
        fetchActivities();
        
        // Set up polling for live updates
        if (liveMode) {
            const interval = setInterval(fetchActivities, 30000);
            return () => clearInterval(interval);
        }
    }, [liveMode, page]);

    // Filter activities
    const filteredActivities = useMemo(() => {
        let filtered = [...activities];
        
        if (filter.type) {
            filtered = filtered.filter(a => a.type === filter.type);
        }
        
        if (filter.collection) {
            filtered = filtered.filter(a => 
                a.collection.toLowerCase().includes(filter.collection.toLowerCase()) ||
                a.collection_name?.toLowerCase().includes(filter.collection.toLowerCase())
            );
        }
        
        if (filter.minPrice) {
            const min = parseFloat(filter.minPrice);
            filtered = filtered.filter(a => a.price_usd && a.price_usd >= min);
        }
        
        if (filter.maxPrice) {
            const max = parseFloat(filter.maxPrice);
            filtered = filtered.filter(a => a.price_usd && a.price_usd <= max);
        }
        
        if (filter.search) {
            const search = filter.search.toLowerCase();
            filtered = filtered.filter(a =>
                a.collection.toLowerCase().includes(search) ||
                a.collection_name?.toLowerCase().includes(search) ||
                a.token_id.includes(search) ||
                a.tx_hash.toLowerCase().includes(search)
            );
        }
        
        return filtered;
    }, [activities, filter]);

    // Paginate
    const paginatedActivities = useMemo(() => {
        const start = (page - 1) * 20;
        return filteredActivities.slice(start, start + 20);
    }, [filteredActivities, page]);

    const formatAddress = (addr: string) => {
        if (!addr || addr.length < 10) return addr;
        return `${addr.substring(0, 6)}...${addr.substring(38)}`;
    };

    const formatPrice = (price?: number) => {
        if (!price) return '-';
        return price.toLocaleString('en-US', { style: 'currency', currency: 'USD' });
    };

    const formatTime = (timestamp: number) => {
        const diff = Date.now() - timestamp;
        const minutes = Math.floor(diff / 60000);
        const hours = Math.floor(diff / 3600000);
        const days = Math.floor(diff / 86400000);
        
        if (minutes < 1) return 'Just now';
        if (minutes < 60) return `${minutes}m ago`;
        if (hours < 24) return `${hours}h ago`;
        if (days < 7) return `${days}d ago`;
        return new Date(timestamp).toLocaleDateString();
    };

    if (loading && !activities.length) {
        return (
            <div className="min-h-screen bg-gray-50 dark:bg-gray-900 flex items-center justify-center">
                <div className="text-center">
                    <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600 mx-auto mb-4"></div>
                    <p className="text-gray-600 dark:text-gray-400">Loading NFT Activity...</p>
                </div>
            </div>
        );
    }

    return (
        <div className="min-h-screen bg-gray-50 dark:bg-gray-900">
            {/* Header */}
            <div className="bg-white dark:bg-gray-800 shadow-sm border-b border-gray-200 dark:border-gray-700">
                <div className="container mx-auto px-4 py-6">
                    <div className="flex items-center justify-between">
                        <div>
                            <h1 className="text-3xl font-bold text-gray-900 dark:text-white">NFT Activity</h1>
                            <p className="text-gray-600 dark:text-gray-400 mt-1">
                                Real-time NFT transfers, sales, and mints across all collections
                            </p>
                        </div>
                        <div className="flex items-center gap-3">
                            <button
                                onClick={() => setLiveMode(!liveMode)}
                                className={`px-4 py-2 rounded-lg font-medium transition-colors ${
                                    liveMode 
                                        ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200' 
                                        : 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-300'
                                }`}
                            >
                                {liveMode ? '🔴 Live' : '⏸ Paused'}
                            </button>
                            {lastUpdate && (
                                <span className="text-sm text-gray-500 dark:text-gray-400">
                                    Updated {formatTime(lastUpdate.getTime())}
                                </span>
                            )}
                        </div>
                    </div>
                </div>
            </div>

            {/* Filters */}
            <div className="bg-white dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700">
                <div className="container mx-auto px-4 py-4">
                    <div className="grid grid-cols-1 md:grid-cols-5 gap-4">
                        <select
                            value={filter.type}
                            onChange={(e) => setFilter({ ...filter, type: e.target.value })}
                            className="px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                        >
                            <option value="">All Types</option>
                            <option value="mint">Minted</option>
                            <option value="transfer">Transferred</option>
                            <option value="sale">Sold</option>
                            <option value="burn">Burned</option>
                            <option value="bid">Bid Placed</option>
                            <option value="list">Listed</option>
                        </select>
                        
                        <input
                            type="text"
                            placeholder="Collection address..."
                            value={filter.collection}
                            onChange={(e) => setFilter({ ...filter, collection: e.target.value })}
                            className="px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                        />
                        
                        <input
                            type="number"
                            placeholder="Min price (USD)..."
                            value={filter.minPrice}
                            onChange={(e) => setFilter({ ...filter, minPrice: e.target.value })}
                            className="px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                        />
                        
                        <input
                            type="number"
                            placeholder="Max price (USD)..."
                            value={filter.maxPrice}
                            onChange={(e) => setFilter({ ...filter, maxPrice: e.target.value })}
                            className="px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                        />
                        
                        <input
                            type="text"
                            placeholder="Search token, tx..."
                            value={filter.search}
                            onChange={(e) => setFilter({ ...filter, search: e.target.value })}
                            className="px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                        />
                    </div>
                </div>
            </div>

            {/* Activity Feed */}
            <div className="container mx-auto px-4 py-8">
                {error && (
                    <div className="bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-4 mb-4">
                        <p className="text-red-800 dark:text-red-200">{error}</p>
                    </div>
                )}

                {/* Stats */}
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
                    <div className="bg-white dark:bg-gray-800 rounded-lg p-4 shadow-sm">
                        <p className="text-sm text-gray-600 dark:text-gray-400">Total Events</p>
                        <p className="text-2xl font-bold text-gray-900 dark:text-white">{filteredActivities.length.toLocaleString()}</p>
                    </div>
                    <div className="bg-white dark:bg-gray-800 rounded-lg p-4 shadow-sm">
                        <p className="text-sm text-gray-600 dark:text-gray-400">Sales Volume</p>
                        <p className="text-2xl font-bold text-gray-900 dark:text-white">
                            {formatPrice(filteredActivities.filter(a => a.type === 'sale').reduce((sum, a) => sum + (a.price_usd || 0), 0))}
                        </p>
                    </div>
                    <div className="bg-white dark:bg-gray-800 rounded-lg p-4 shadow-sm">
                        <p className="text-sm text-gray-600 dark:text-gray-400">Floor Price (Avg)</p>
                        <p className="text-2xl font-bold text-gray-900 dark:text-white">
                            {formatPrice(
                                filteredActivities.filter(a => a.type === 'sale').length > 0
                                    ? filteredActivities.filter(a => a.type === 'sale').reduce((sum, a) => sum + (a.price_usd || 0), 0) / filteredActivities.filter(a => a.type === 'sale').length
                                    : 0
                            )}
                        </p>
                    </div>
                    <div className="bg-white dark:bg-gray-800 rounded-lg p-4 shadow-sm">
                        <p className="text-sm text-gray-600 dark:text-gray-400">Unique Collectors</p>
                        <p className="text-2xl font-bold text-gray-900 dark:text-white">
                            {new Set([...filteredActivities.map(a => a.to), ...filteredActivities.map(a => a.from)]).size.toLocaleString()}
                        </p>
                    </div>
                </div>

                {/* Activity List */}
                <div className="bg-white dark:bg-gray-800 rounded-lg shadow-sm overflow-hidden">
                    <div className="overflow-x-auto">
                        <table className="w-full">
                            <thead className="bg-gray-50 dark:bg-gray-700">
                                <tr>
                                    <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">Type</th>
                                    <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">Collection</th>
                                    <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">Token</th>
                                    <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">From</th>
                                    <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">To</th>
                                    <th className="px-4 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">Price</th>
                                    <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">Time</th>
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-gray-200 dark:divide-gray-700">
                                {paginatedActivities.map((action) => (
                                    <tr key={action.id} className="hover:bg-gray-50 dark:hover:bg-gray-700/50">
                                        <td className="px-4 py-3">
                                            <span className={`inline-flex items-center px-2 py-1 rounded-full text-xs font-medium ${ACTIVITY_TYPE_COLORS[action.type]}`}>
                                                {ACTIVITY_TYPE_LABELS[action.type]}
                                            </span>
                                        </td>
                                        <td className="px-4 py-3">
                                            <a href={`/nft_collections/${action.collection}`} className="text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300">
                                                {action.collection_name || formatAddress(action.collection)}
                                            </a>
                                        </td>
                                        <td className="px-4 py-3 text-gray-900 dark:text-white">
                                            #{action.token_id}
                                        </td>
                                        <td className="px-4 py-3">
                                            <a href={`/address/${action.from}`} className="text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300 font-mono text-sm">
                                                {formatAddress(action.from)}
                                            </a>
                                        </td>
                                        <td className="px-4 py-3">
                                            <a href={`/address/${action.to}`} className="text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300 font-mono text-sm">
                                                {formatAddress(action.to)}
                                            </a>
                                        </td>
                                        <td className="px-4 py-3 text-right font-medium text-gray-900 dark:text-white">
                                            {action.price_usd ? formatPrice(action.price_usd) : '-'}
                                        </td>
                                        <td className="px-4 py-3 text-sm text-gray-500 dark:text-gray-400">
                                            {formatTime(action.timestamp)}
                                        </td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                    
                    {/* Pagination */}
                    <div className="px-4 py-3 bg-gray-50 dark:bg-gray-700 border-t border-gray-200 dark:border-gray-600 flex items-center justify-between">
                        <button
                            onClick={() => setPage(Math.max(1, page - 1))}
                            disabled={page === 1}
                            className="px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg text-sm font-medium text-gray-700 dark:text-gray-300 disabled:opacity-50 disabled:cursor-not-allowed hover:bg-gray-50 dark:hover:bg-gray-600"
                        >
                            Previous
                        </button>
                        <span className="text-sm text-gray-600 dark:text-gray-400">
                            Page {page} of {totalPages}
                        </span>
                        <button
                            onClick={() => setPage(Math.min(totalPages, page + 1))}
                            disabled={page === totalPages}
                            className="px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg text-sm font-medium text-gray-700 dark:text-gray-300 disabled:opacity-50 disabled:cursor-not-allowed hover:bg-gray-50 dark:hover:bg-gray-600"
                        >
                            Next
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
}