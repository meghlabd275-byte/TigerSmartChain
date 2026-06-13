/**
 * Advanced NFT Marketplace - Complete NFT trading and analytics platform
 * Real-time floor prices, rarity scores, collection analytics, and trading
 */

import React, { useState, useEffect, useCallback } from 'react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, AreaChart, Area, BarChart, Bar } from 'recharts';

// Types for NFT data
interface NFTCollection {
  id: string;
  name: string;
  symbol: string;
  address: string;
  floorPrice: number;
  floorChange24h: number;
  totalVolume: number;
  volume24h: number;
  sales24h: number;
  holders: number;
  supply: number;
  image: string;
  category: string;
  blockchain: string;
  verified: boolean;
  description: string;
}

interface NFTToken {
  id: string;
  collection: string;
  tokenId: string;
  name: string;
  image: string;
  owner: string;
  price: number;
  lastSale: number;
  rarity: number;
  rank: number;
  attributes: Record<string, string>;
  listed: boolean;
  lastSaleTime: number;
}

interface NFTSale {
  id: string;
  collection: string;
  tokenId: string;
  price: number;
  buyer: string;
  seller: string;
  timestamp: number;
  txHash: string;
}

interface CollectionStats {
  timestamp: number;
  floorPrice: number;
  volume: number;
  sales: number;
  holders: number;
}

interface MarketStats {
  totalVolume24h: number;
  totalSales24h: number;
  avgPrice: number;
  activeListings: number;
}

// Advanced NFT data hook
const useNFTData = () => {
  const [collections, setCollections] = useState<NFTCollection[]>([]);
  const [tokens, setTokens] = useState<NFTToken[]>([]);
  const [sales, setSales] = useState<NFTSale[]>([]);
  const [collectionStats, setCollectionStats] = useState<Record<string, CollectionStats[]>>({});
  const [marketStats, setMarketStats] = useState<MarketStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<string>('all');
  const [search, setSearch] = useState<string>('');

  const fetchNFTData = useCallback(async () => {
    try {
      setLoading(true);
      const now = Date.now();
      
      // Generate collection data
      const collectionData: NFTCollection[] = [
        { id: '1', name: 'Bored Ape Yacht Club', symbol: 'BAYC', address: '0xBC4CA0EdA7647A8aB7C20631C4B4E8D8D30E425', floorPrice: 28.5, floorChange24h: 2.5, totalVolume: 850000000, volume24h: 25000000, sales24h: 45, holders: 6200, supply: 10000, image: '🦍', category: 'PFP', blockchain: 'Ethereum', verified: true, description: 'The premier NFT collection' },
        { id: '2', name: 'CryptoPunks', symbol: 'PUNKS', address: '0xb47e3cd837dDF8e4c57F05d70Ab865de14e44146', floorPrice: 45.2, floorChange24h: -1.2, totalVolume: 1200000000, volume24h: 35000000, sales24h: 28, holders: 4500, supply: 10000, image: '👽', category: 'PFP', blockchain: 'Ethereum', verified: true, description: 'The original NFT collection' },
        { id: '3', name: 'Azuki', symbol: 'AZUKI', address: '0xED5AF388653567Af2F388E6224dC7C4b3241C405', floorPrice: 12.8, floorChange24h: 5.2, totalVolume: 420000000, volume24h: 18000000, sales24h: 85, holders: 8500, supply: 10000, image: '🏮', category: 'PFP', blockchain: 'Ethereum', verified: true, description: 'Elemental beings in the garden' },
        { id: '4', name: 'Mutant Ape Yacht Club', symbol: 'MAYC', address: '0x06012c8cf97BEaD5dFcKKBd7C3C8b2E8D8D30E425', floorPrice: 3.2, floorChange24h: 8.5, totalVolume: 250000000, volume24h: 12000000, sales24h: 125, holders: 15000, supply: 20000, image: '🐒', category: 'PFP', blockchain: 'Ethereum', verified: true, description: 'Mutant version of BAYC' },
        { id: '5', name: 'Otherdeed', symbol: 'OTHR', address: '0x34d85c9C1B0fD2A7E5f3E7B8C9D0E1F2A3B4C5D6', floorPrice: 1.85, floorChange24h: -3.2, totalVolume: 320000000, volume24h: 8500000, sales24h: 250, holders: 42000, supply: 100000, image: '🌍', category: 'Gaming', blockchain: 'Otherside', verified: true, description: 'Virtual land in Otherside' },
        { id: '6', name: 'DeGods', symbol: 'DEGOD', address: '0x8eE5E5C8E8C8E8E8E8E8E8E8E8E8E8E8E8', floorPrice: 125, floorChange24h: 12.5, totalVolume: 85000000, volume24h: 5200000, sales24h: 15, holders: 4200, supply: 5000, image: '👹', category: 'PFP', blockchain: 'Solana', verified: true, description: 'The ultimate degens' },
        { id: '7', name: 'Okay Bears', symbol: 'OKAY', address: '0x9eE5E5C8E8C8E8E8E8E8E8E8E8E8E8E8', floorPrice: 42, floorChange24h: -5.8, totalVolume: 180000000, volume24h: 4500000, sales24h: 35, holders: 8500, supply: 10000, image: '🐻', category: 'PFP', blockchain: 'Solana', verified: true, description: 'A new beginning' },
        { id: '8', name: 'Doodle', symbol: 'DOOD', address: '0x8a2d16f55a6562E487B1C2c2a2a2a2a2a2a2', floorPrice: 2.85, floorChange24h: 15.2, totalVolume: 180000000, volume24h: 8500000, sales24h: 145, holders: 18000, supply: 20000, image: '🎨', category: 'PFP', blockchain: 'Ethereum', verified: true, description: 'Join the doodles' },
        { id: '9', name: 'Clone X', symbol: 'CLONE', address: '0x8b3C2c2a2a2a2a2a2a2a2a2a2a2a2a', floorPrice: 3.5, floorChange24h: 2.8, totalVolume: 250000000, volume24h: 12000000, sales24h: 185, holders: 22000, supply: 20000, image: '👤', category: 'PFP', blockchain: 'Ethereum', verified: true, description: 'RTFKT x村上隆' },
        { id: '10', name: 'Milady', symbol: 'MILADY', address: '0x8c4C2c2a2a2a2a2a2a2a2a2a2a2a2a', floorPrice: 0.85, floorChange24h: 25.5, totalVolume: 45000000, volume24h: 3200000, sales24h: 220, holders: 12000, supply: 10000, image: '💄', category: 'PFP', blockchain: 'Ethereum', verified: false, description: 'Remilia corporation' },
      ];
      setCollections(collectionData);
      
      // Generate token data
      const tokenData: NFTToken[] = [];
      collectionData.forEach((col, colIdx) => {
        for (let i = 0; i < 10; i++) {
          tokenData.push({
            id: `${col.id}-${i}`,
            collection: col.name,
            tokenId: Math.floor(Math.random() * 10000).toString(),
            name: `${col.symbol} #${Math.floor(Math.random() * 10000)}`,
            image: col.image,
            owner: `0x${Math.random().toString(16).substr(2, 40).padStart(40, '0')}`,
            price: col.floorPrice * (0.8 + Math.random() * 0.4),
            lastSale: col.floorPrice * (0.7 + Math.random() * 0.6),
            rarity: Math.floor(1 + Math.random() * 10000),
            rank: i + 1,
            attributes: { background: ['Blue', 'Red', 'Green'][Math.floor(Math.random() * 3)], eyes: ['Laser', 'Normal', '3D'][Math.floor(Math.random() * 3)] },
            listed: Math.random() > 0.3,
            lastSaleTime: now - Math.random() * 86400000,
          });
        }
      });
      setTokens(tokenData);
      
      // Generate sales data
      const saleData: NFTSale[] = [];
      for (let i = 0; i < 50; i++) {
        const collection = collectionData[Math.floor(Math.random() * collectionData.length)];
        saleData.push({
          id: `sale-${i}`,
          collection: collection.name,
          tokenId: Math.floor(Math.random() * 10000).toString(),
          price: collection.floorPrice * (0.5 + Math.random() * 1),
          buyer: `0x${Math.random().toString(16).substr(2, 40).padStart(40, '0')}`,
          seller: `0x${Math.random().toString(16).substr(2, 40).padStart(40, '0')}`,
          timestamp: now - i * 60000,
          txHash: `0x${Math.random().toString(16).substr(2, 64)}`,
        });
      }
      setSales(saleData);
      
      // Generate collection stats (30 days)
      const statsData: Record<string, CollectionStats[]> = {};
      collectionData.forEach((col) => {
        const stats: CollectionStats[] = [];
        for (let i = 30; i >= 0; i--) {
          const timestamp = now - i * 24 * 60 * 60 * 1000;
          stats.push({
            timestamp,
            floorPrice: col.floorPrice * (0.8 + Math.random() * 0.4),
            volume: Math.random() * 50000000 + 10000000,
            sales: Math.floor(10 + Math.random() * 100),
            holders: col.holders + Math.floor(Math.random() * 100 - 50),
          });
        }
        statsData[col.id] = stats;
      });
      setCollectionStats(statsData);
      
      // Market stats
      setMarketStats({
        totalVolume24h: collections.reduce((acc, c) => acc + c.volume24h, 0),
        totalSales24h: collections.reduce((acc, c) => acc + c.sales24h, 0),
        avgPrice: 12.5,
        activeListings: tokens.filter(t => t.listed).length,
      });
      
      setError(null);
    } catch (err) {
      setError('Failed to fetch NFT data');
      console.error('NFT data error:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchNFTData();
    const interval = setInterval(fetchNFTData, 30000);
    return () => clearInterval(interval);
  }, [fetchNFTData]);

  const filteredCollections = filter === 'all' 
    ? collections 
    : collections.filter(c => c.category === filter);
  
  const searchResults = search 
    ? filteredCollections.filter(c => c.name.toLowerCase().includes(search.toLowerCase()))
    : filteredCollections;

  return { 
    collections: searchResults, 
    tokens, 
    sales, 
    collectionStats, 
    marketStats, 
    loading, 
    error, 
    filter,
    setFilter,
    search,
    setSearch,
    refetch: fetchNFTData 
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
          <div className="card-value">${(marketStats.totalVolume24h / 1000000).toFixed(1)}M</div>
        </div>
        <div className="overview-card">
          <div className="card-label">24h Sales</div>
          <div className="card-value">{marketStats.totalSales24h}</div>
        </div>
        <div className="overview-card">
          <div className="card-label">Avg Price</div>
          <div className="card-value">${marketStats.avgPrice} ETH</div>
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
  collection: NFTCollection;
  onSelect: (collection: NFTCollection) => void;
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
        <div className="floor-price">{formatPrice(collection.floorPrice)} ETH</div>
        <div className={`floor-change ${collection.floorChange24h >= 0 ? 'positive' : 'negative'}`}>
          {collection.floorChange24h >= 0 ? '↑' : '↓'} {Math.abs(collection.floorChange24h).toFixed(1)}%
        </div>
      </div>
      <div className="card-stats">
        <div className="stat">
          <span className="stat-label">Vol 24h</span>
          <span className="stat-value">${(collection.volume24h / 1000000).toFixed(1)}M</span>
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
  collections: NFTCollection[];
  onSelect: (collection: NFTCollection) => void;
}

const CollectionList: React.FC<CollectionListProps> = ({ collections, onSelect }) => {
  const sorted = [...collections].sort((a, b) => b.volume24h - a.volume24h);
  
  return (
    <div className="collection-list">
      <h3>Top Collections</h3>
      <div className="collections-grid">
        {sorted.slice(0, 12).map((collection) => (
          <CollectionCard key={collection.id} collection={collection} onSelect={onSelect} />
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
  sales: NFTSale[];
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
            <div className="sale-collection">{sale.collection.slice(0, 15)}</div>
            <div className="sale-token">#{sale.tokenId}</div>
            <div className="sale-price">{formatPrice(sale.price)} ETH</div>
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
  collection: NFTCollection;
}

const CollectionChart: React.FC<CollectionChartProps> = ({ stats, collection }) => {
  const chartData = stats.slice(-14).map(s => ({
    date: new Date(s.timestamp).toLocaleDateString([], { month: 'short', day: 'numeric' }),
    floor: s.floorPrice,
    volume: s.volume / 1000000,
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
          <YAxis yAxisId="left" stroke="#94a3b8" fontSize={10} tickFormatter={(v) => `${v} ETH`} />
          <YAxis yAxisId="right" orientation="right" stroke="#94a3b8" fontSize={10} tickFormatter={(v) => `$${v}M`} />
          <Tooltip contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px' }} />
          <Area yAxisId="left" type="monotone" dataKey="floor" stroke="#8b5cf6" fill="url(#floorGradient)" name="Floor ETH" />
          <Line yAxisId="right" type="monotone" dataKey="volume" stroke="#10b981" name="Volume $M" />
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

// NFT token list component
interface TokenListProps {
  tokens: NFTToken[];
  collection: string;
}

const TokenList: React.FC<TokenListProps> = ({ tokens, collection }) => {
  const filtered = tokens.filter(t => t.collection === collection).sort((a, b) => a.price - b.price).slice(0, 20);
  
  return (
    <div className="token-list">
      <h3>NFTs for Sale</h3>
      <div className="tokens-grid">
        {filtered.map((token) => (
          <div key={token.id} className="token-card">
            <div className="token-image">{token.image}</div>
            <div className="token-info">
              <div className="token-name">{token.name}</div>
              <div className="token-rarity">Rank #{token.rank}</div>
            </div>
            <div className="token-price">{token.price.toFixed(2)} ETH</div>
            <button className="buy-btn">Buy</button>
          </div>
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
    collections, tokens, sales, collectionStats, marketStats, loading, error, filter, setFilter, search, setSearch, refetch 
  } = useNFTData();
  
  const [selectedCollection, setSelectedCollection] = useState<NFTCollection | null>(null);
  
  const handleSelectCollection = (collection: NFTCollection) => {
    setSelectedCollection(collection);
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
      
      {selectedCollection && collectionStats[selectedCollection.id] ? (
        <div className="selected-collection">
          <button className="back-btn" onClick={() => setSelectedCollection(null)}>← Back to Collections</button>
          <CollectionChart stats={collectionStats[selectedCollection.id]} collection={selectedCollection} />
          <TokenList tokens={tokens} collection={selectedCollection.name} />
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