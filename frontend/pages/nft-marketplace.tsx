/**
 * NFT Marketplace
 *
 * Collections, per-token detail, recent sales and the collection chart all come
 * from the indexed tables (nft_collections, nfts, nft_sales, nft_holders).
 * Nothing on this page is generated in the browser.
 *
 * Prices are stored as the raw wei amount of the payment token and are shown in
 * the native coin, TGR, because that is the unit the chain settles in. The page
 * used to print them with an ETH label and a hardcoded dollar conversion.
 */

import React, { useState, useEffect, useCallback } from 'react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, AreaChart, Area } from 'recharts';
import api from '@/lib/api';
import { formatWei, formatNumber, formatAddress } from '@/lib/utils';

const NATIVE_SYMBOL = 'TGR';
const PAGE_SIZE = 24;

interface CollectionRow {
  address: string;
  name: string | null;
  symbol: string | null;
  contract_type: string | null;
  total_supply: string | null;
  holders_count: string | null;
  transfers_count: string | null;
  floor_price: string | null;
  floor_price_change_24h: string | null;
  volume_24h: string | null;
  average_price_24h: string | null;
  market_cap: string | null;
  description: string | null;
  image_url: string | null;
  is_verified: boolean | null;
  sales_24h: string | null;
}

interface CollectionStats {
  date: string;
  floorPrice: number;
  volume: number;
  sales: number;
  holders: number;
}

interface MarketStats {
  totalVolume24h: string;
  totalSales24h: string;
  averagePrice: string;
  activeListings: string;
}

// A collection row carries prices as raw wei strings. They are converted once,
// here, so the components below can stay in plain numbers.
const toCollection = (row: CollectionRow) => ({
  address: row.address,
  name: row.name || row.address,
  symbol: row.symbol || '',
  contractType: row.contract_type || '',
  supply: Number(row.total_supply || 0),
  holders: Number(row.holders_count || 0),
  floorPrice: Number(formatWei(row.floor_price || '0')),
  floorChange24h: Number(row.floor_price_change_24h || 0),
  volume24h: Number(formatWei(row.volume_24h || '0')),
  marketCap: Number(formatWei(row.market_cap || '0')),
  sales24h: Number(row.sales_24h || 0),
  verified: row.is_verified === true,
  description: row.description || '',
  image: row.image_url || '',
});

type Collection = ReturnType<typeof toCollection>;

interface SaleRow {
  token_address: string;
  token_id: string;
  price: string;
  buyer: string;
  seller: string;
  transaction_hash: string;
  log_index: number;
  block_timestamp: string;
}

interface FloorRow {
  timestamp?: string;
  date?: string;
  floor_price?: string;
  price?: string;
}

interface SeriesRow {
  date: string;
  sales?: string | number;
  volume?: string;
  holders?: string | number;
}

interface Sale {
  id: string;
  tokenAddress: string;
  tokenId: string;
  price: number;
  buyer: string;
  seller: string;
  timestamp: number;
  txHash: string;
}

const useNFTData = () => {
  const [collections, setCollections] = useState<Collection[]>([]);
  const [sales, setSales] = useState<Sale[]>([]);
  const [collectionStats, setCollectionStats] = useState<Record<string, CollectionStats[]>>({});
  const [marketStats, setMarketStats] = useState<MarketStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<string>('all');
  const [search, setSearch] = useState<string>('');

  const fetchNFTData = useCallback(async () => {
    try {
      setLoading(true);

      const collectionsResponse = await api.getNFTMarketplaceCollections({ limit: PAGE_SIZE });
      setCollections((collectionsResponse.items as unknown as CollectionRow[]).map(toCollection));

      const stats = await api.getNFTMarketplaceStats();
      setMarketStats({
        totalVolume24h: stats.totalVolume24h || '0',
        totalSales24h: stats.totalSales24h || '0',
        averagePrice: stats.averagePrice || '0',
        activeListings: stats.activeListings || '0',
      });

      const salesResponse = await api.getNFTSales({ limit: 50 });
      setSales((salesResponse.items as unknown as SaleRow[]).map((row) => ({
        id: `${row.transaction_hash}-${row.log_index}`,
        tokenAddress: row.token_address,
        tokenId: row.token_id,
        price: Number(formatWei(row.price || '0')),
        buyer: row.buyer,
        seller: row.seller,
        timestamp: new Date(row.block_timestamp).getTime(),
        txHash: row.transaction_hash,
      })));

      setError(null);
    } catch (err: any) {
      setError(err?.response?.data?.error || 'Could not load marketplace data.');
    } finally {
      setLoading(false);
    }
  }, []);

  // The chart follows the selected collection. Floor history and the daily sale
  // series are separate indexed queries, so they are fetched together when the
  // selection changes rather than being invented for every collection up front.
  const loadCollectionStats = useCallback(async (address: string) => {
    try {
      const [floor, series, holders] = await Promise.all([
        api.getNFTFloorHistory(address, { limit: 30 }),
        api.getNFTCollectionSalesSeries(address, { limit: 30 }),
        api.getNFTCollectionHoldersSeries(address, { limit: 30 }),
      ]);

      const byDate = new Map<string, CollectionStats>();
      const ensure = (date: string): CollectionStats => {
        if (!byDate.has(date)) {
          byDate.set(date, { date, floorPrice: 0, volume: 0, sales: 0, holders: 0 });
        }
        return byDate.get(date)!;
      };

      for (const row of (floor.items as unknown as FloorRow[])) {
        const date = String(row.timestamp || row.date || '').slice(0, 10);
        if (!date) continue;
        ensure(date).floorPrice = Number(formatWei(row.floor_price || row.price || '0'));
      }
      for (const row of (series.items as unknown as SeriesRow[])) {
        const point = ensure(String(row.date));
        point.sales = Number(row.sales || 0);
        point.volume = Number(formatWei(row.volume || '0'));
      }
      for (const row of (holders.items as unknown as SeriesRow[])) {
        ensure(String(row.date)).holders = Number(row.holders || 0);
      }

      const ordered = [...byDate.values()].sort((a, b) => a.date.localeCompare(b.date));
      setCollectionStats((prev) => ({ ...prev, [address]: ordered }));
    } catch {
      // A collection with no indexed sales yet simply has no chart; the
      // component renders its empty state.
      setCollectionStats((prev) => ({ ...prev, [address]: [] }));
    }
  }, []);

  useEffect(() => {
    fetchNFTData();
    const interval = setInterval(fetchNFTData, 30000);
    return () => clearInterval(interval);
  }, [fetchNFTData]);

  const filteredCollections = filter === 'all'
    ? collections
    : collections.filter((c) => c.contractType === filter);

  const searchResults = search
    ? filteredCollections.filter((c) =>
        c.name.toLowerCase().includes(search.toLowerCase()) ||
        c.symbol.toLowerCase().includes(search.toLowerCase()) ||
        c.address.toLowerCase().includes(search.toLowerCase()))
    : filteredCollections;

  return {
    collections: searchResults,
    allCollections: collections,
    sales,
    collectionStats,
    marketStats,
    loading,
    error,
    filter,
    setFilter,
    search,
    setSearch,
    loadCollectionStats,
    refetch: fetchNFTData,
  };
};

// Market overview component
interface MarketOverviewProps {
  marketStats: MarketStats;
}

const MarketOverview: React.FC<MarketOverviewProps> = ({ marketStats }) => {
  if (!marketStats) return null;
  
  return (
    <div className="market-overview">
      <div className="overview-cards">
        <div className="overview-card">
          <div className="card-label">24h Volume</div>
          <div className="card-value">{formatNumber(marketStats.totalVolume24h)} {NATIVE_SYMBOL}</div>
        </div>
        <div className="overview-card">
          <div className="card-label">24h Sales</div>
          <div className="card-value">{marketStats.totalSales24h}</div>
        </div>
        <div className="overview-card">
          <div className="card-label">Avg Price</div>
          <div className="card-value">{formatNumber(marketStats.averagePrice)} {NATIVE_SYMBOL}</div>
        </div>
        <div className="overview-card">
          <div className="card-label">Active Listings</div>
          <div className="card-value">{marketStats.activeListings}</div>
        </div>
      </div>
      
      <style>{`
        .market-overview { margin-bottom: 24px; }
        .overview-cards {
          display: grid;
          grid-template-columns: repeat(4, 1fr);
          gap: 16px;
        }
        .overview-card {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          text-align: center;
        }
        .card-label {
          font-size: 12px;
          color: #94a3b8;
          text-transform: uppercase;
          margin-bottom: 8px;
        }
        .card-value {
          font-size: 24px;
          font-weight: 700;
          color: #e2e8f0;
        }
        @media (max-width: 768px) {
          .overview-cards { grid-template-columns: repeat(2, 1fr); }
        }
      `}</style>
    </div>
  );
};

// Collection card component
interface CollectionCardProps {
  collection: Collection;
  onSelect: (collection: Collection) => void;
}

const CollectionCard: React.FC<CollectionCardProps> = ({ collection, onSelect }) => {
  const formatPrice = (price: number) => {
    if (price >= 1000) return `${(price / 1000).toFixed(1)}K`;
    return price.toFixed(2);
  };
  
  return (
    <div className="collection-card" onClick={() => onSelect(collection)}>
      <div className="card-image">{collection.image}</div>
      <div className="card-info">
        <div className="card-name">
          {collection.verified && <span className="verified">✓</span>}
          {collection.name}
        </div>
        <div className="card-symbol">{collection.symbol}</div>
      </div>
      <div className="card-floor">
        <div className="floor-price">{formatPrice(collection.floorPrice)} {NATIVE_SYMBOL}</div>
        <div className={`floor-change ${collection.floorChange24h >= 0 ? 'positive' : 'negative'}`}>
          {collection.floorChange24h >= 0 ? '↑' : '↓'} {Math.abs(collection.floorChange24h).toFixed(1)}%
        </div>
      </div>
      <div className="card-stats">
        <div className="stat">
          <span className="stat-label">Vol 24h</span>
          <span className="stat-value">{formatNumber(collection.volume24h)}</span>
        </div>
        <div className="stat">
          <span className="stat-label">Sales</span>
          <span className="stat-value">{collection.sales24h}</span>
        </div>
        <div className="stat">
          <span className="stat-label">Holders</span>
          <span className="stat-value">{collection.holders.toLocaleString()}</span>
        </div>
      </div>
      
      <style>{`
        .collection-card {
          background: #1e293b;
          border-radius: 12px;
          padding: 16px;
          cursor: pointer;
          transition: all 0.2s;
        }
        .collection-card:hover {
          transform: translateY(-4px);
          box-shadow: 0 8px 24px rgba(0,0,0,0.3);
        }
        .card-image {
          font-size: 48px;
          text-align: center;
          margin-bottom: 12px;
        }
        .card-name {
          font-weight: 600;
          color: #e2e8f0;
          margin-bottom: 4px;
        }
        .verified {
          color: #3b82f6;
          margin-right: 4px;
        }
        .card-symbol {
          font-size: 12px;
          color: #64748b;
        }
        .card-floor {
          display: flex;
          align-items: center;
          gap: 8px;
          margin: 12px 0;
        }
        .floor-price {
          font-size: 18px;
          font-weight: 700;
          color: #e2e8f0;
        }
        .floor-change {
          font-size: 12px;
        }
        .floor-change.positive { color: #10b981; }
        .floor-change.negative { color: #ef4444; }
        .card-stats {
          display: flex;
          gap: 16px;
        }
        .stat {
          display: flex;
          flex-direction: column;
        }
        .stat-label {
          font-size: 10px;
          color: #64748b;
        }
        .stat-value {
          font-size: 12px;
          color: #94a3b8;
        }
      `}</style>
    </div>
  );
};

// Collection list component
interface CollectionListProps {
  collections: Collection[];
  onSelect: (collection: Collection) => void;
}

const CollectionList: React.FC<CollectionListProps> = ({ collections, onSelect }) => {
  const sorted = [...collections].sort((a, b) => b.volume24h - a.volume24h);
  
  return (
    <div className="collection-list">
      <h3>Top Collections</h3>
      <div className="collections-grid">
        {sorted.slice(0, 12).map((collection) => (
          <CollectionCard key={collection.address} collection={collection} onSelect={onSelect} />
        ))}
      </div>
      
      <style>{`
        .collection-list {
          margin-bottom: 32px;
        }
        .collection-list h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
        }
        .collections-grid {
          display: grid;
          grid-template-columns: repeat(4, 1fr);
          gap: 16px;
        }
        @media (max-width: 1200px) {
          .collections-grid { grid-template-columns: repeat(3, 1fr); }
        }
        @media (max-width: 900px) {
          .collections-grid { grid-template-columns: repeat(2, 1fr); }
        }
      `}</style>
    </div>
  );
};

// Recent sales component
interface RecentSalesProps {
  sales: Sale[];
}

const RecentSalesProps: React.FC<RecentSalesProps> = ({ sales }) => {
  const formatPrice = (price: number) => {
    if (price >= 1000) return `${(price / 1000).toFixed(1)}K`;
    return price.toFixed(2);
  };
  
  return (
    <div className="recent-sales">
      <h3>Recent Sales</h3>
      <div className="sales-list">
        {sales.slice(0, 15).map((sale) => (
          <div key={sale.id} className="sale-item">
            <div className="sale-collection">{formatAddress(sale.tokenAddress)}</div>
            <div className="sale-token">#{sale.tokenId}</div>
            <div className="sale-price">{formatPrice(sale.price)} {NATIVE_SYMBOL}</div>
            <div className="sale-time">{new Date(sale.timestamp).toLocaleTimeString()}</div>
          </div>
        ))}
      </div>
      
      <style>{`
        .recent-sales {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .recent-sales h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
        }
        .sales-list {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        .sale-item {
          display: grid;
          grid-template-columns: 1fr 80px 80px 80px;
          padding: 10px 12px;
          background: #0f172a;
          border-radius: 8px;
        }
        .sale-collection {
          color: #e2e8f0;
          font-size: 13px;
        }
        .sale-token {
          color: #64748b;
          font-size: 12px;
        }
        .sale-price {
          color: #10b981;
          font-weight: 600;
        }
        .sale-time {
          color: #64748b;
          font-size: 12px;
        }
      `}</style>
    </div>
  );
};

// Collection chart component
interface CollectionChartProps {
  stats: CollectionStats[];
  collection: Collection;
}

const CollectionChart: React.FC<CollectionChartProps> = ({ stats, collection }) => {
  if (stats.length === 0) {
    return (
      <div className="collection-chart">
        <h3>{collection.name}</h3>
        <p style={{ color: '#94a3b8' }}>No indexed sales for this collection yet.</p>
      </div>
    );
  }

  const chartData = stats.slice(-14).map((s) => ({
    date: s.date,
    floor: s.floorPrice,
    volume: s.volume,
    sales: s.sales,
  }));
  
  return (
    <div className="collection-chart">
      <h3>{collection.name} - 14 Day Stats</h3>
      <ResponsiveContainer width="100%" height={200}>
        <AreaChart data={chartData}>
          <defs>
            <linearGradient id="floorGradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor="#8b5cf6" stopOpacity={0.3} />
              <stop offset="95%" stopColor="#8b5cf6" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
          <XAxis dataKey="date" stroke="#94a3b8" fontSize={10} />
          <YAxis yAxisId="left" stroke="#94a3b8" fontSize={10} tickFormatter={(v) => `${v} ${NATIVE_SYMBOL}`} />
          <YAxis yAxisId="right" orientation="right" stroke="#94a3b8" fontSize={10} tickFormatter={(v) => `$${v}M`} />
          <Tooltip contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px' }} />
          <Area yAxisId="left" type="monotone" dataKey="floor" stroke="#8b5cf6" fill="url(#floorGradient)" name={`Floor ${NATIVE_SYMBOL}`} />
          <Line yAxisId="right" type="monotone" dataKey="volume" stroke="#10b981" name="Volume" />
        </AreaChart>
      </ResponsiveContainer>
      
      <style>{`
        .collection-chart {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .collection-chart h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
          font-size: 16px;
        }
      `}</style>
    </div>
  );
};

// NFT token list component. Rows come from the nfts table, which holds the
// indexed owner and metadata for each token. The previous version listed ten
// invented tokens per collection, with random owners, random prices, and a
// "Buy" button that did nothing.
interface TokenListProps {
  collectionAddress: string;
}

interface TokenRow {
  token_id: string;
  owner: string;
  uri: string | null;
  name: string | null;
  image_url: string | null;
}

const TokenList: React.FC<TokenListProps> = ({ collectionAddress }) => {
  const [tokens, setTokens] = useState<TokenRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api.getNFTTokens(collectionAddress, { limit: 24 })
      .then((res) => { if (!cancelled) setTokens(res.items as unknown as TokenRow[]); })
      .catch((err) => {
        if (!cancelled) setError(err?.response?.data?.error || 'Could not load tokens.');
      })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [collectionAddress]);

  if (loading) {
    return <div className="token-list"><h3>Tokens</h3><p style={{ color: '#94a3b8' }}>Loading...</p></div>;
  }
  if (error) {
    return <div className="token-list"><h3>Tokens</h3><p style={{ color: '#f87171' }}>{error}</p></div>;
  }
  if (tokens.length === 0) {
    return <div className="token-list"><h3>Tokens</h3><p style={{ color: '#94a3b8' }}>No tokens indexed for this collection yet.</p></div>;
  }

  return (
    <div className="token-list">
      <h3>Tokens</h3>
      <div className="tokens-grid">
        {tokens.map((token) => (
          <a key={token.token_id} className="token-card" href={`/nft/${collectionAddress}/${token.token_id}`}>
            <div className="token-image">
              {token.image_url
                ? <img src={token.image_url} alt="" style={{ width: '100%', height: '100%', objectFit: 'cover' }} />
                : '#'}
            </div>
            <div className="token-info">
              <div className="token-name">{token.name || `#${token.token_id}`}</div>
              <div className="token-rarity">{formatAddress(token.owner)}</div>
            </div>
          </a>
        ))}
      </div>

      <style>{`
        .token-list {
          background: #1e293b;
          border-radius: 12px;
          padding: 20px;
          margin-bottom: 24px;
        }
        .token-list h3 {
          color: #e2e8f0;
          margin-bottom: 16px;
        }
        .tokens-grid {
          display: grid;
          grid-template-columns: repeat(5, 1fr);
          gap: 12px;
        }
        .token-card {
          background: #0f172a;
          border-radius: 8px;
          padding: 12px;
          text-align: center;
        }
        .token-image {
          font-size: 32px;
        }
        .token-name {
          font-size: 12px;
          color: #e2e8f0;
          margin: 8px 0 4px;
        }
        .token-rarity {
          font-size: 10px;
          color: #64748b;
        }
        .token-price {
          color: #10b981;
          font-weight: 600;
          margin: 8px 0;
        }
        .buy-btn {
          width: 100%;
          padding: 6px;
          background: #8b5cf6;
          border: none;
          border-radius: 6px;
          color: white;
          cursor: pointer;
          font-size: 12px;
        }
        .buy-btn:hover { background: #7c3aed; }
      `}</style>
    </div>
  );
};

// Filter component
interface FilterProps {
  filter: string;
  setFilter: (filter: string) => void;
}

const Filter: React.FC<FilterProps> = ({ filter, setFilter }) => {
  const filters = [
    { value: 'all', label: 'All' },
    { value: 'PFP', label: 'PFP' },
    { value: 'Gaming', label: 'Gaming' },
    { value: 'Art', label: 'Art' },
    { value: 'Utility', label: 'Utility' },
  ];
  
  return (
    <div className="filter-bar">
      {filters.map((f) => (
        <button
          key={f.value}
          className={`filter-btn ${filter === f.value ? 'active' : ''}`}
          onClick={() => setFilter(f.value)}
        >
          {f.label}
        </button>
      ))}
      <style>{`
        .filter-bar {
          display: flex;
          gap: 8px;
          margin-bottom: 24px;
        }
        .filter-btn {
          padding: 8px 16px;
          background: #1e293b;
          border: 1px solid #334155;
          border-radius: 8px;
          color: #94a3b8;
          cursor: pointer;
          transition: all 0.2s;
        }
        .filter-btn:hover { border-color: #8b5cf6; color: #e2e8f0; }
        .filter-btn.active {
          background: #8b5cf6;
          border-color: #8b5cf6;
          color: white;
        }
      `}</style>
    </div>
  );
};

// Search component
interface SearchProps {
  search: string;
  setSearch: (search: string) => void;
}

const Search: React.FC<SearchProps> = ({ search, setSearch }) => {
  return (
    <div className="search-bar">
      <input
        type="text"
        placeholder="Search collections..."
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
      <style>{`
        .search-bar input {
          width: 100%;
          max-width: 400px;
          padding: 12px 16px;
          background: #1e293b;
          border: 1px solid #334155;
          border-radius: 8px;
          color: #e2e8f0;
          font-size: 14px;
        }
        .search-bar input:focus {
          outline: none;
          border-color: #8b5cf6;
        }
        .search-bar input::placeholder {
          color: #64748b;
        }
      `}</style>
    </div>
  );
};

// Main NFT Marketplace component
const NFTMarketplace: React.FC = () => {
  const {
    collections, sales, collectionStats, marketStats, loading, error,
    filter, setFilter, search, setSearch, loadCollectionStats, refetch,
  } = useNFTData();

  const [selectedCollection, setSelectedCollection] = useState<Collection | null>(null);

  const handleSelectCollection = (collection: Collection) => {
    setSelectedCollection(collection);
    loadCollectionStats(collection.address);
  };


  if (loading && collections.length === 0) {
    return (
      <div className="nft-marketplace">
        <div className="loading-container">
          <div className="loading-spinner"></div>
          <p>Loading NFT data...</p>
        </div>
        <style>{`
          .loading-container {
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            height: 400px;
            color: #94a3b8;
          }
          .loading-spinner {
            width: 40px;
            height: 40px;
            border: 3px solid #334155;
            border-top-color: #8b5cf6;
            border-radius: 50%;
            animation: spin 1s linear infinite;
          }
          @keyframes spin { to { transform: rotate(360deg); } }
        `}</style>
      </div>
    );
  }
  
  return (
    <div className="nft-marketplace">
      <div className="page-header">
        <h1>🎨 NFT Marketplace</h1>
        <p>Trade, collect, and analyze NFTs</p>
        <button className="refresh-btn" onClick={refetch}>↻ Refresh</button>
      </div>
      
      <MarketOverview marketStats={marketStats!} />
      
      <Search search={search} setSearch={setSearch} />
      <Filter filter={filter} setFilter={setFilter} />
      
      {selectedCollection && collectionStats[selectedCollection.address] ? (
        <div className="selected-collection">
          <button className="back-btn" onClick={() => setSelectedCollection(null)}>← Back to Collections</button>
          <CollectionChart stats={collectionStats[selectedCollection.address]} collection={selectedCollection} />
          <TokenList collectionAddress={selectedCollection.address} />
        </div>
      ) : (
        <>
          <CollectionList collections={collections} onSelect={handleSelectCollection} />
          <RecentSalesProps sales={sales} />
        </>
      )}
      
      <style>{`
        .nft-marketplace {
          padding: 24px;
          max-width: 1400px;
          margin: 0 auto;
        }
        .page-header {
          margin-bottom: 24px;
          display: flex;
          flex-direction: column;
        }
        .page-header h1 {
          font-size: 32px;
          font-weight: 700;
          color: #e2e8f0;
          margin-bottom: 8px;
        }
        .page-header p { color: #94a3b8; }
        .refresh-btn {
          margin-top: 12px;
          align-self: flex-start;
          padding: 8px 16px;
          background: #8b5cf6;
          border: none;
          border-radius: 8px;
          color: white;
          cursor: pointer;
          font-weight: 500;
        }
        .refresh-btn:hover { background: #7c3aed; }
        .selected-collection { margin-bottom: 24px; }
        .back-btn {
          padding: 8px 16px;
          background: #334155;
          border: none;
          border-radius: 8px;
          color: #e2e8f0;
          cursor: pointer;
          margin-bottom: 16px;
        }
      `}</style>
    </div>
  );
};

export default NFTMarketplace;