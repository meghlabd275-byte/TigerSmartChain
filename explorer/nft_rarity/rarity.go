// NFT Rarity API
// Production-grade API for NFT rarity calculation and ranking

package rarity

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"math"
	"net/http"
	"sort"
	"strconv"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
)

// =============================================================================
// TYPES
// =============================================================================

type NFTTrait struct {
	TraitType  string `json:"trait_type"`
	Value     string `json:"value"`
	DisplayType string `json:"display_type,omitempty"`
}

type NFTMetadata struct {
	TokenID     string     `json:"tokenId"`
	Name        string     `json:"name"`
	Description string     `json:"description,omitempty"`
	Image       string     `json:"image"`
	Attributes  []NFTTrait `json:"attributes"`
}

type TraitInfo struct {
	Value      string  `json:"value"`
	Count     int     `json:"count"`
	Frequency float64 `json:"frequency"`
	Rarity    float64 `json:"rarity"`
}

type CollectionStats struct {
	TotalSupply   int                    `json:"totalSupply"`
	UniqueHolders int                    `json:"uniqueHolders"`
	FloorPrice   float64               `json:"floorPrice"`
	AvgPrice     float64               `json:"avgPrice"`
	Volume24h    float64               `json:"volume24h"`
	TraitCounts  map[string]map[string]int `json:"traitCounts"`
}

type NFTRarity struct {
	TokenID      string     `json:"tokenId"`
	Name         string     `json:"name"`
	Image        string     `json:"image"`
	RarityScore  float64   `json:"rarityScore"`
	RarityRank   int       `json:"rarityRank"`
	Traits       []NFTTrait `json:"traits"`
	TraitRarity  []TraitInfo `json:"traitRarity"`
}

type RarityResponse struct {
	NFTs        []NFTRarity    `json:"nfts"`
	Collection  CollectionStats `json:"collection"`
}

// =============================================================================
// SERVER
// =============================================================================

type Server struct {
	db    *sql.DB
	cache map[string]*CacheEntry
	mu    sync.RWMutex
}

type CacheEntry struct {
	Data      *RarityResponse
	ExpiresAt time.Time
}

func NewServer(db *sql.DB) *Server {
	return &Server{
		db:    db,
		cache: make(map[string]*CacheEntry),
	}
}

// =============================================================================
// API HANDLERS
// =============================================================================

func (s *Server) GetRarity(c *gin.Context) {
	ctx := c.Request.Context()
	
	collectionAddress := c.Param("address")
	if collectionAddress == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Collection address required"})
		return
	}
	
	limit, _ := strconv.Atoi(c.DefaultQuery("limit", "100"))
	if limit <= 0 || limit > 1000 {
		limit = 100
	}
	
	cacheKey := fmt.Sprintf("rarity:%s:%d", collectionAddress, limit)
	if entry := s.getCacheEntry(cacheKey); entry != nil {
		c.JSON(http.StatusOK, entry.Data)
		return
	}
	
	stats, err := s.getCollectionStats(ctx, collectionAddress)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": fmt.Sprintf("Failed to get collection stats: %v", err)})
		return
	}
	
	nfts, err := s.getNFTs(ctx, collectionAddress, limit)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": fmt.Sprintf("Failed to get NFTs: %v", err)})
		return
	}
	
	rarities := s.calculateRarity(nfts, stats)
	
	sort.Slice(rarities, func(i, j int) bool {
		return rarities[i].RarityScore > rarities[j].RarityScore
	})
	
	for i := range rarities {
		rarities[i].RarityRank = i + 1
	}
	
	response := &RarityResponse{
		NFTs:       rarities,
		Collection: *stats,
	}
	
	s.setCacheEntry(cacheKey, response)
	c.JSON(http.StatusOK, response)
}

// =============================================================================
// DATABASE
// =============================================================================

func (s *Server) getCollectionStats(ctx context.Context, collectionAddress string) (*CollectionStats, error) {
	stats := &CollectionStats{
		TraitCounts: make(map[string]map[string]int),
	}
	
	s.db.QueryRowContext(ctx, `SELECT COUNT(*) FROM nfts WHERE token_address = $1`, collectionAddress).Scan(&stats.TotalSupply)
	s.db.QueryRowContext(ctx, `SELECT COUNT(DISTINCT owner) FROM nft_owners WHERE token_address = $1`, collectionAddress).Scan(&stats.UniqueHolders)
	s.db.QueryRowContext(ctx, `SELECT COALESCE(MIN(floor_price), 0) FROM nft_collections WHERE address = $1`, collectionAddress).Scan(&stats.FloorPrice)
	s.db.QueryRowContext(ctx, `SELECT COALESCE(AVG(average_price_24h), 0) FROM nft_collections WHERE address = $1`, collectionAddress).Scan(&stats.AvgPrice)
	s.db.QueryRowContext(ctx, `SELECT COALESCE(SUM(volume_24h), 0) FROM nft_collections WHERE address = $1`, collectionAddress).Scan(&stats.Volume24h)
	
	rows, _ := s.db.QueryContext(ctx, `SELECT metadata->'attributes' as attrs FROM nfts WHERE token_address = $1`, collectionAddress)
	if rows != nil {
		defer rows.Close()
		for rows.Next() {
			var attrsJSON sql.NullString
			if rows.Scan(&attrsJSON); attrsJSON.Valid {
				var attrs []NFTTrait
				json.Unmarshal([]byte(attrsJSON.String), &attrs)
				for _, attr := range attrs {
					if stats.TraitCounts[attr.TraitType] == nil {
						stats.TraitCounts[attr.TraitType] = make(map[string]int)
					}
					stats.TraitCounts[attr.TraitType][attr.Value]++
				}
			}
		}
	}
	
	return stats, nil
}

func (s *Server) getNFTs(ctx context.Context, collectionAddress string, limit int) ([]NFTMetadata, error) {
	rows, err := s.db.QueryContext(ctx, `
		SELECT token_id, name, image_url, metadata FROM nfts WHERE token_address = $1 ORDER BY token_id LIMIT $2
	`, collectionAddress, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var nfts []NFTMetadata
	for rows.Next() {
		var nft NFTMetadata
		var metadataJSON sql.NullString
		if err := rows.Scan(&nft.TokenID, &nft.Name, &nft.Image, &metadataJSON); err != nil {
			continue
		}
		if metadataJSON.Valid {
			json.Unmarshal([]byte(metadataJSON.String), &nft)
		}
		nfts = append(nfts, nft)
	}
	
	return nfts, nil
}

// =============================================================================
// RARITY CALCULATION
// =============================================================================

func (s *Server) calculateRarity(nfts []NFTMetadata, stats *CollectionStats) []NFTRarity {
	rarities := make([]NFTRarity, 0, len(nfts))
	
	for _, nft := range nfts {
		if len(nft.Attributes) == 0 {
			continue
		}
		
		rarity := NFTRarity{
			TokenID:     nft.TokenID,
			Name:        nft.Name,
			Image:       nft.Image,
			Traits:      nft.Attributes,
			TraitRarity: make([]TraitInfo, 0, len(nft.Attributes)),
		}
		
		totalRarity := 0.0
		
		for _, trait := range nft.Attributes {
			traitInfo := TraitInfo{Value: trait.Value}
			count := stats.TraitCounts[trait.TraitType][trait.Value]
			if count == 0 {
				count = 1
			}
			
			traitInfo.Count = count
			traitInfo.Frequency = float64(count) / float64(stats.TotalSupply)
			
			frequency := traitInfo.Frequency
			if frequency <= 0 {
				frequency = 0.0001
			}
			
			rarityScore := 1 / math.Log2(frequency*100+2)
			traitInfo.Rarity = rarityScore
			
			rarity.TraitRarity = append(rarity.TraitRarity, traitInfo)
			totalRarity += rarityScore
		}
		
		if len(nft.Attributes) > 0 {
			rarity.RarityScore = math.Min(100, (totalRarity/float64(len(nft.Attributes))) * 20)
		}
		
		rarities = append(rarities, rarity)
	}
	
	return rarities
}

// =============================================================================
// CACHE
// =============================================================================

func (s *Server) getCacheEntry(key string) *CacheEntry {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	entry, ok := s.cache[key]
	if !ok || time.Now().After(entry.ExpiresAt) {
		return nil
	}
	
	return entry
}

func (s *Server) setCacheEntry(key string, data *RarityResponse) {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	s.cache[key] = &CacheEntry{
		Data:      data,
		ExpiresAt: time.Now().Add(5 * time.Minute),
	}
}

// =============================================================================
// ROUTES
// =============================================================================

func (s *Server) RegisterRoutes(r *gin.Engine) {
	api := r.Group("/api/v1")
	{
		api.GET("/nfts/:address/rarity", s.GetRarity)
	}
}