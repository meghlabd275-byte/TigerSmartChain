// NFT Marketplace Integration
// Production-grade integration with OpenSea, Blur, LooksRare
// Features: Floor prices, collections, trending, trading activity

import React, { useState, useEffect } from 'react';
import { useQuery } from 'react-query';

// =============================================================================
// TYPES
// =============================================================================

interface MarketplaceListing {
  id: string;
  marketplace: 'opensea' | 'blur' | 'looksrare' | 'bluemove';
  tokenId: string;
  collectionAddress: string;
  price: number;
  priceUSD: number;
  currency: string;
  seller: string;
  buyer?: string;
  status: 'listed' | 'sold' | 'cancelled';
  createdAt: number;
  updatedAt: number;
}

interface CollectionMarketplaceData {
  collectionAddress: string;
  marketplaces: {
    opensea?: MarketplaceStats;
    blur?: MarketplaceStats;
    looksrare?: MarketplaceStats;
    bluemove?: MarketplaceStats;
  };
}

interface MarketplaceStats {
  floorPrice: number;
  volume24h: number;
  volume7d: number;
  sales24h: number;
  listings: number;
  avgPrice24h: number;
}

interface NFTMarketplaceProps {
  collectionAddress: string;
  showAllMarketplaces?: boolean;
}

// =============================================================================
// API
// =============================================================================

const API_BASE = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

async function fetchMarketplaceData(collectionAddress: string): Promise<CollectionMarketplaceData> {
  const response = await fetch(
    `${API_BASE}/api/v1/nfts/marketplaces/${collectionAddress}`
  );
  if (!response.ok) {
    throw new Error('Failed to fetch marketplace data');
  }
  return response.json();
}

async function fetchTrendingCollections(): Promise<CollectionMarketplaceData[]> {
  const response = await fetch(`${API_BASE}/api/v1/nfts/marketplaces/trending`);
  if (!response.ok) {
    throw new Error('Failed to fetch trending collections');
  }
  return response.json();
}

async function fetchCollectionListings(
  collectionAddress: string,
  marketplace: string,
  limit: number = 50
): Promise<MarketplaceListing[]> {
  const response = await fetch(
    `${API_BASE}/api/v1/nfts/marketplaces/${collectionAddress}/listings?marketplace=${marketplace}&limit=${limit}`
  );
  if (!response.ok) {
    throw new Error('Failed to fetch listings');
  }
  return response.json();
}

// =============================================================================
// MARKETPLACE CONFIG
// =============================================================================

const MARKETPLACE_CONFIG = {
  opensea: {
    name: 'OpenSea',
    color: '#2081e2',
    logo: '/logos/opensea.svg',
  },
  blur: {
    name: 'Blur',
    color: '#d52c2c',
    logo: '/logos/blur.svg',
  },
  looksrare: {
    name: 'LooksRare',
    color: '#adfc03',
    logo: '/logos/looksrare.svg',
  },
  bluemove: {
    name: 'BlueMove',
    color: '#0d8af0',
    logo: '/logos/bluemove.svg',
  },
};

// =============================================================================
// MAIN COMPONENT
// =============================================================================

export default function NFTMarketplaceIntegration({
  collectionAddress,
  showAllMarketplaces = true,
}: NFTMarketplaceProps) {
  const [selectedMarketplace, setSelectedMarketplace] = useState<string>('all');

  const { data, isLoading, error } = useQuery<CollectionMarketplaceData>(
    ['marketplace-data', collectionAddress],
    () => fetchMarketplaceData(collectionAddress),
    {
      refetchInterval: 60000,
      staleTime: 30000,
    }
  );

  if (isLoading) {
    return (
      <div style={{ padding: 40, textAlign: 'center', color: '#6b7280' }}>
        Loading marketplace data...
      </div>
    );
  }

  if (error || !data) {
    return (
      <div style={{ padding: 40, textAlign: 'center', color: '#ef4444' }}>
        Failed to load marketplace data
      </div>
    );
  }

  const marketplaces = Object.keys(data.marketplaces) as Array<keyof typeof data.marketplaces>;

  return (
    <div>
      {/* Header */}
      <div style={{ marginBottom: 24 }}>
        <h2 style={{ fontSize: 20, fontWeight: 600, marginBottom: 8 }}>
          Marketplace Data
        </h2>
        <p style={{ color: '#6b7280', fontSize: 14 }}>
          Real-time data from major NFT marketplaces
        </p>
      </div>

      {/* Marketplace Tabs */}
      <div
        style={{
          display: 'flex',
          gap: 8,
          marginBottom: 24,
          borderBottom: '1px solid #e5e7eb',
          paddingBottom: 12,
        }}
      >
        <button
          onClick={() => setSelectedMarketplace('all')}
          style={{
            padding: '8px 16px',
            backgroundColor: selectedMarketplace === 'all' ? '#3b82f6' : 'transparent',
            color: selectedMarketplace === 'all' ? '#fff' : '#374151',
            border: '1px solid #e5e7eb',
            borderRadius: 6,
            fontSize: 13,
            cursor: 'pointer',
          }}
        >
          All Marketplaces
        </button>
        {marketplaces.map((mp) => (
          <button
            key={mp}
            onClick={() => setSelectedMarketplace(mp)}
            style={{
              padding: '8px 16px',
              backgroundColor: selectedMarketplace === mp ? MARKETPLACE_CONFIG[mp].color : 'transparent',
              color: selectedMarketplace === mp ? '#fff' : '#374151',
              border: '1px solid #e5e7eb',
              borderRadius: 6,
              fontSize: 13,
              cursor: 'pointer',
            }}
          >
            {MARKETPLACE_CONFIG[mp].name}
          </button>
        ))}
      </div>

      {/* Stats Grid */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))',
          gap: 16,
        }}
      >
        {(selectedMarketplace === 'all' ? marketplaces : [selectedMarketplace]).map((mp) => {
          const stats = data.marketplaces[mp];
          if (!stats) return null;

          return (
            <MarketplaceCard
              key={mp}
              marketplace={mp}
              stats={stats}
            />
          );
        })}
      </div>
    </div>
  );
}

// =============================================================================
// SUB-COMPONENTS
// =============================================================================

function MarketplaceCard({
  marketplace,
  stats,
}: {
  marketplace: keyof typeof MARKETPLACE_CONFIG;
  stats: MarketplaceStats;
}) {
  const config = MARKETPLACE_CONFIG[marketplace];

  return (
    <div
      style={{
        padding: 16,
        backgroundColor: '#fff',
        border: '1px solid #e5e7eb',
        borderRadius: 8,
      }}
    >
      {/* Header */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          marginBottom: 16,
        }}
      >
        <div
          style={{
            width: 32,
            height: 32,
            borderRadius: '50%',
            backgroundColor: config.color,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            color: '#fff',
            fontWeight: 600,
            fontSize: 12,
          }}
        >
          {config.name[0]}
        </div>
        <span style={{ fontWeight: 600, fontSize: 14 }}>{config.name}</span>
      </div>

      {/* Stats */}
      <div style={{ display: 'grid', gap: 12 }}>
        <StatRow label="Floor Price" value={`${stats.floorPrice.toFixed(3)} ETH`} />
        <StatRow label="24h Volume" value={`${stats.volume24h.toFixed(2)} ETH`} />
        <StatRow label="7d Volume" value={`${stats.volume7d.toFixed(2)} ETH`} />
        <StatRow label="24h Sales" value={stats.sales24h.toString()} />
        <StatRow label="Avg Price 24h" value={`${stats.avgPrice24h.toFixed(3)} ETH`} />
        <StatRow label="Active Listings" value={stats.listings.toString()} />
      </div>
    </div>
  );
}

function StatRow({ label, value }: { label: string; value: string }) {
  return (
    <div
      style={{
        display: 'flex',
        justifyContent: 'space-between',
        fontSize: 13,
      }}
    >
      <span style={{ color: '#6b7280' }}>{label}</span>
      <span style={{ fontWeight: 500, color: '#111' }}>{value}</span>
    </div>
  );
}

// =============================================================================
// TRENDING COLLECTIONS
// =============================================================================

export function TrendingCollections() {
  const { data, isLoading, error } = useQuery<CollectionMarketplaceData[]>(
    'trending-collections',
    fetchTrendingCollections,
    {
      refetchInterval: 120000,
    }
  );

  if (isLoading) {
    return <div style={{ padding: 40, textAlign: 'center' }}>Loading...</div>;
  }

  if (error || !data) {
    return <div style={{ padding: 40, textAlign: 'center' }}>Failed to load</div>;
  }

  return (
    <div>
      <h2 style={{ fontSize: 20, fontWeight: 600, marginBottom: 16 }}>
        Trending Collections
      </h2>
      <div style={{ display: 'grid', gap: 12 }}>
        {data.map((collection, index) => (
          <div
            key={collection.collectionAddress}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 16,
              padding: 12,
              backgroundColor: '#fff',
              border: '1px solid #e5e7eb',
              borderRadius: 8,
            }}
          >
            <span
              style={{
                width: 24,
                height: 24,
                borderRadius: '50%',
                backgroundColor: index < 3 ? '#fbbf24' : '#e5e7eb',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                fontSize: 12,
                fontWeight: 600,
                color: index < 3 ? '#fff' : '#6b7280',
              }}
            >
              {index + 1}
            </span>
            <div style={{ flex: 1 }}>
              <p style={{ fontWeight: 500, margin: 0 }}>
                {collection.collectionAddress.slice(0, 6)}...{collection.collectionAddress.slice(-4)}
              </p>
            </div>
            <div style={{ textAlign: 'right' }}>
              <p style={{ fontWeight: 600, margin: 0 }}>
                {Object.values(collection.marketplaces)[0]?.volume24h.toFixed(2) || 0} ETH
              </p>
              <p style={{ fontSize: 12, color: '#6b7280', margin: 0 }}>24h Volume</p>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// =============================================================================
// MARKETPLACE LISTINGS
// =============================================================================

export function MarketplaceListings({
  collectionAddress,
  marketplace = 'opensea',
  limit = 20,
}: {
  collectionAddress: string;
  marketplace?: string;
  limit?: number;
}) {
  const { data, isLoading, error } = useQuery<MarketplaceListing[]>(
    ['marketplace-listings', collectionAddress, marketplace],
    () => fetchCollectionListings(collectionAddress, marketplace, limit),
    {
      refetchInterval: 30000,
    }
  );

  if (isLoading) {
    return <div style={{ padding: 20, textAlign: 'center' }}>Loading listings...</div>;
  }

  if (error || !data) {
    return <div style={{ padding: 20, textAlign: 'center' }}>No listings found</div>;
  }

  return (
    <div>
      <h3 style={{ fontSize: 16, fontWeight: 600, marginBottom: 12 }}>
        {MARKETPLACE_CONFIG[marketplace as keyof typeof MARKETPLACE_CONFIG]?.name || marketplace} Listings
      </h3>
      <div style={{ display: 'grid', gap: 8 }}>
        {data.map((listing) => (
          <ListingRow key={listing.id} listing={listing} />
        ))}
      </div>
    </div>
  );
}

function ListingRow({ listing }: { listing: MarketplaceListing }) {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        padding: 12,
        backgroundColor: '#f9fafb',
        borderRadius: 6,
        fontSize: 13,
      }}
    >
      <div>
        <p style={{ fontWeight: 500, margin: 0 }}>
          #{listing.tokenId}
        </p>
        <p style={{ color: '#6b7280', margin: '4px 0 0', fontSize: 12 }}>
          {listing.seller.slice(0, 6)}...{listing.seller.slice(-4)}
        </p>
      </div>
      <div style={{ textAlign: 'right' }}>
        <p style={{ fontWeight: 600, margin: 0 }}>
          {listing.price.toFixed(3)} ETH
        </p>
        <p style={{ color: '#6b7280', margin: '4px 0 0', fontSize: 12 }}>
          ${listing.priceUSD.toFixed(2)}
        </p>
      </div>
    </div>
  );
}