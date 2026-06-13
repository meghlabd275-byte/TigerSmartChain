// NFT Rarity Ranking Component
// Production-grade rarity calculation based on NFT attributes/traits
// Implements: Statistical rarity, trait frequency analysis, rarity scores

import React, { useEffect, useRef, useState, useMemo } from 'react';
import { useQuery } from 'react-query';

// =============================================================================
// TYPES
// =============================================================================

interface NFTTrait {
  trait_type: string;
  value: string;
  display_type?: string;
}

interface NFTMetadata {
  tokenId: string;
  name: string;
  description?: string;
  image: string;
  attributes: NFTTrait[];
}

interface NFTRarity {
  tokenId: string;
  name: string;
  image: string;
  rarityScore: number;
  rarityRank: number;
  traits: NFTTrait[];
  traitRarity: {
    trait_type: string;
    value: string;
    frequency: number;
    rarityScore: number;
  }[];
}

interface NFTCollectionStats {
  totalSupply: number;
  uniqueHolders: number;
  floorPrice: number;
  avgPrice: number;
  volume24h: number;
  traitCounts: {
    [key: string]: {
      [value: string]: number;
    };
  };
}

// =============================================================================
// API
// =============================================================================

const API_BASE = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

interface RarityResponse {
  nfts: NFTRarity[];
  collection: NFTCollectionStats;
}

async function fetchNFTsRarity(
  collectionAddress: string,
  limit: number = 100
): Promise<RarityResponse> {
  const response = await fetch(
    `${API_BASE}/api/v1/nfts/${collectionAddress}/rarity?limit=${limit}`
  );
  if (!response.ok) {
    throw new Error('Failed to fetch NFT rarity data');
  }
  return response.json();
}

// =============================================================================
// RARITY CALCULATION (Client-side for flexibility)
// =============================================================================

// Calculate rarity score based on trait frequency
function calculateRarityScore(
  traits: NFTTrait[],
  traitCounts: { [key: string]: { [value: string]: number } },
  totalSupply: number
): number {
  let score = 0;
  
  for (const trait of traits) {
    const traitType = trait.trait_type;
    const traitValue = trait.value;
    
    const count = traitCounts[traitType]?.[traitValue] || 1;
    const frequency = count / totalSupply;
    
    // Statistical rarity: lower frequency = higher rarity
    // Using logarithmic scale for better distribution
    const traitRarity = 1 / Math.log2(frequency * 100 + 2);
    score += traitRarity;
  }
  
  // Normalize to 0-100 scale
  return Math.min(100, (score / Math.max(traits.length, 1)) * 20);
}

// Sort NFTs by rarity score
function sortByRarity(nfts: NFTRarity[]): NFTRarity[] {
  return [...nfts].sort((a, b) => b.rarityScore - a.rarityScore);
}

// =============================================================================
// MAIN COMPONENT
// =============================================================================

interface NFTRarityRankingProps {
  collectionAddress: string;
  height?: number;
  showFilters?: boolean;
  showStats?: boolean;
}

export default function NFTRarityRanking({
  collectionAddress,
  height = 600,
  showFilters = true,
  showStats = true,
}: NFTRarityProps) {
  const [filterTrait, setFilterTrait] = useState<string | null>(null);
  const [filterValue, setFilterValue] = useState<string | null>(null);
  const [sortBy, setSortBy] = useState<'rarity' | 'price' | 'volume'>('rarity');

  // Fetch data
  const { data, isLoading, error, refetch } = useQuery<RarityResponse>(
    ['nft-rarity', collectionAddress],
    () => fetchNFTsRarity(collectionAddress),
    {
      refetchInterval: 300000,
      staleTime: 60000,
    }
  );

  // Filter and sort NFTs
  const filteredNFTs = useMemo(() => {
    if (!data?.nfts) return [];
    
    let nfts = [...data.nfts];
    
    // Apply trait filter
    if (filterTrait && filterValue) {
      nfts = nfts.filter((nft) =>
        nft.traits.some(
          (t) => t.trait_type === filterTrait && t.value === filterValue
        )
      );
    }
    
    // Sort
    switch (sortBy) {
      case 'rarity':
        return nfts.sort((a, b) => b.rarityScore - a.rarityScore);
      case 'price':
        return nfts.sort((a, b) => (b as any).price - (a as any).price);
      case 'volume':
        return nfts.sort((a, b) => (b as any).volume - (a as any).volume);
      default:
        return nfts;
    }
  }, [data?.nfts, filterTrait, filterValue, sortBy]);

  // Get unique traits for filter dropdown
  const traitOptions = useMemo(() => {
    if (!data?.collection?.traitCounts) return [];
    
    return Object.entries(data.collection.traitCounts).map(([traitType, values]) => ({
      traitType,
      values: Object.entries(values as { [key: string]: number }).map(([value, count]) => ({
        value,
        count,
      })),
    }));
  }, [data?.collection?.traitCounts]);

  // Loading state
  if (isLoading) {
    return (
      <div
        style={{
          width: '100%',
          height,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: '#f9fafb',
          borderRadius: 8,
        }}
      >
        <div style={{ textAlign: 'center' }}>
          <div
            style={{
              width: 40,
              height: 40,
              border: '3px solid #e5e7eb',
              borderTopColor: '#8b5cf6',
              borderRadius: '50%',
              animation: 'spin 1s linear infinite',
              margin: '0 auto 12px',
            }}
          />
          <p style={{ color: '#6b7280', fontSize: 14 }}>
            Calculating rarity scores...
          </p>
        </div>
        <style>{`
          @keyframes spin {
            to { transform: rotate(360deg); }
          }
        `}</style>
      </div>
    );
  }

  // Error state
  if (error || !data) {
    return (
      <div
        style={{
          width: '100%',
          height,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: '#fef2f2',
          borderRadius: 8,
        }}
      >
        <p style={{ color: '#ef4444', fontSize: 14 }}>
          Failed to load rarity data
        </p>
      </div>
    );
  }

  return (
    <div style={{ width: '100%' }}>
      {/* Stats */}
      {showStats && data.collection && (
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(6, 1fr)',
            gap: 16,
            marginBottom: 24,
          }}
        >
          <StatCard
            label="Total Supply"
            value={data.collection.totalSupply.toLocaleString()}
          />
          <StatCard
            label="Unique Holders"
            value={data.collection.uniqueHolders.toLocaleString()}
          />
          <StatCard
            label="Floor Price"
            value={`${data.collection.floorPrice.toFixed(2)} ETH`}
          />
          <StatCard
            label="Avg Price"
            value={`${data.collection.avgPrice.toFixed(2)} ETH`}
          />
          <StatCard
            label="24h Volume"
            value={`${data.collection.volume24h.toFixed(2)} ETH`}
          />
          <StatCard
            label="Traits"
            value={Object.keys(data.collection.traitCounts).length.toString()}
          />
        </div>
      )}

      {/* Filters */}
      {showFilters && traitOptions.length > 0 && (
        <div
          style={{
            display: 'flex',
            gap: 12,
            marginBottom: 24,
            flexWrap: 'wrap',
          }}
        >
          <select
            value={filterTrait || ''}
            onChange={(e) => {
              setFilterTrait(e.target.value || null);
              setFilterValue(null);
            }}
            style={{
              padding: '8px 12px',
              border: '1px solid #e5e7eb',
              borderRadius: 6,
              fontSize: 13,
            }}
          >
            <option value="">All Traits</option>
            {traitOptions.map((t) => (
              <option key={t.traitType} value={t.traitType}>
                {t.traitType}
              </option>
            ))}
          </select>

          {filterTrait && (
            <select
              value={filterValue || ''}
              onChange={(e) => setFilterValue(e.target.value || null)}
              style={{
                padding: '8px 12px',
                border: '1px solid #e5e7eb',
                borderRadius: 6,
                fontSize: 13,
              }}
            >
              <option value="">All Values</option>
              {traitOptions
                .find((t) => t.traitType === filterTrait)
                ?.values.map((v) => (
                  <option key={v.value} value={v.value}>
                    {v.value} ({v.count})
                  </option>
                ))}
            </select>
          )}

          <select
            value={sortBy}
            onChange={(e) => setSortBy(e.target.value as any)}
            style={{
              padding: '8px 12px',
              border: '1px solid #e5e7eb',
              borderRadius: 6,
              fontSize: 13,
            }}
          >
            <option value="rarity">Sort by Rarity</option>
            <option value="price">Sort by Price</option>
            <option value="volume">Sort by Volume</option>
          </select>

          <button
            onClick={() => refetch()}
            style={{
              padding: '8px 16px',
              border: '1px solid #e5e7eb',
              borderRadius: 6,
              fontSize: 13,
              backgroundColor: '#fff',
              cursor: 'pointer',
            }}
          >
            Refresh
          </button>
        </div>
      )}

      {/* Results count */}
      <p style={{ fontSize: 13, color: '#6b7280', marginBottom: 16 }}>
        Showing {filteredNFTs.length} of {data.nfts.length} NFTs
      </p>

      {/* NFT Grid */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))',
          gap: 16,
          maxHeight: height,
          overflowY: 'auto',
        }}
      >
        {filteredNFTs.map((nft, index) => (
          <NFTCard key={nft.tokenId} nft={nft} rank={index + 1} />
        ))}
      </div>
    </div>
  );
}

// =============================================================================
// SUB-COMPONENTS
// =============================================================================

function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <div
      style={{
        padding: 12,
        backgroundColor: '#f9fafb',
        borderRadius: 8,
        textAlign: 'center',
      }}
    >
      <p style={{ fontSize: 12, color: '#6b7280', margin: '0 0 4px' }}>
        {label}
      </p>
      <p style={{ fontSize: 16, fontWeight: 600, margin: 0, color: '#111' }}>
        {value}
      </p>
    </div>
  );
}

function NFTCard({ nft, rank }: { nft: NFTRarity; rank: number }) {
  const [showTraits, setShowTraits] = useState(false);

  // Get rarity color
  const getRarityColor = (score: number) => {
    if (score >= 80) return '#8b5cf6'; // Legendary
    if (score >= 60) return '#3b82f6'; // Rare
    if (score >= 40) return '#22c55e'; // Uncommon
    return '#6b7280'; // Common
  };

  return (
    <div
      style={{
        border: '1px solid #e5e7eb',
        borderRadius: 8,
        overflow: 'hidden',
        transition: 'transform 0.2s, box-shadow 0.2s',
        cursor: 'pointer',
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.transform = 'translateY(-2px)';
        e.currentTarget.style.boxShadow = '0 4px 12px rgba(0,0,0,0.1)';
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.transform = 'translateY(0)';
        e.currentTarget.style.boxShadow = 'none';
      }}
    >
      {/* Image */}
      <div style={{ position: 'relative', aspectRatio: '1' }}>
        <img
          src={nft.image}
          alt={nft.name}
          style={{
            width: '100%',
            height: '100%',
            objectFit: 'cover',
          }}
        />
        
        {/* Rank badge */}
        <div
          style={{
            position: 'absolute',
            top: 8,
            left: 8,
            padding: '4px 8px',
            backgroundColor: getRarityColor(nft.rarityScore),
            color: '#fff',
            borderRadius: 4,
            fontSize: 12,
            fontWeight: 600,
          }}
        >
          #{rank}
        </div>
        
        {/* Rarity score */}
        <div
          style={{
            position: 'absolute',
            top: 8,
            right: 8,
            padding: '4px 8px',
            backgroundColor: 'rgba(0,0,0,0.7)',
            color: getRarityColor(nft.rarityScore),
            borderRadius: 4,
            fontSize: 12,
            fontWeight: 600,
          }}
        >
          {nft.rarityScore.toFixed(1)}
        </div>
      </div>

      {/* Info */}
      <div style={{ padding: 12 }}>
        <h3
          style={{
            fontSize: 14,
            fontWeight: 600,
            margin: '0 0 8px',
            whiteSpace: 'nowrap',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
          }}
        >
          {nft.name}
        </h3>

        {/* Trait preview */}
        <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
          {nft.traits.slice(0, 3).map((trait, i) => (
            <span
              key={i}
              style={{
                padding: '2px 6px',
                backgroundColor: '#f3f4f6',
                borderRadius: 4,
                fontSize: 10,
                color: '#4b5563',
              }}
            >
              {trait.value}
            </span>
          ))}
          {nft.traits.length > 3 && (
            <span
              style={{
                padding: '2px 6px',
                backgroundColor: '#f3f4f6',
                borderRadius: 4,
                fontSize: 10,
                color: '#4b5563',
              }}
            >
              +{nft.traits.length - 3}
            </span>
          )}
        </div>

        {/* Trait details (expandable) */}
        <button
          onClick={() => setShowTraits(!showTraits)}
          style={{
            marginTop: 8,
            padding: '4px 8px',
            backgroundColor: 'transparent',
            border: '1px solid #e5e7eb',
            borderRadius: 4,
            fontSize: 11,
            color: '#6b7280',
            cursor: 'pointer',
            width: '100%',
          }}
        >
          {showTraits ? 'Hide' : 'Show'} Traits ({nft.traits.length})
        </button>

        {showTraits && (
          <div
            style={{
              marginTop: 8,
              padding: 8,
              backgroundColor: '#f9fafb',
              borderRadius: 4,
              fontSize: 11,
            }}
          >
            {nft.traitRarity.map((t, i) => (
              <div
                key={i}
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  marginBottom: 4,
                }}
              >
                <span style={{ color: '#6b7280' }}>{t.trait_type}</span>
                <span style={{ fontWeight: 500 }}>
                  {t.value}{' '}
                  <span style={{ color: '#9ca3af', fontSize: 10 }}>
                    ({t.frequency.toFixed(1)}%)
                  </span>
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

// =============================================================================
// PAGE COMPONENT
// =============================================================================

export function NFTRarityPage() {
  const [collectionAddress, setCollectionAddress] = useState('');

  return (
    <div style={{ padding: 24, maxWidth: 1400, margin: '0 auto' }}>
      <h1 style={{ fontSize: 24, fontWeight: 700, marginBottom: 24 }}>
        NFT Rarity Ranking
      </h1>

      <div style={{ marginBottom: 24 }}>
        <input
          type="text"
          placeholder="Collection address"
          value={collectionAddress}
          onChange={(e) => setCollectionAddress(e.target.value)}
          style={{
            padding: '10px 14px',
            border: '1px solid #e5e7eb',
            borderRadius: 8,
            fontSize: 14,
            width: 400,
          }}
        />
      </div>

      {collectionAddress && (
        <NFTRarityRanking collectionAddress={collectionAddress} height={800} />
      )}
    </div>
  );
}