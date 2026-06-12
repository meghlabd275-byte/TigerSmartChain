// Package nftanalytics provides advanced NFT analytics with rarity calculation
package nftanalytics

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"math"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

// =============================================================================
// DATA STRUCTURES
// =============================================================================

// NFTAnalyticsService provides advanced NFT analytics
type NFTAnalyticsService struct {
	db          *sql.DB
	collectionCache map[string]*CollectionAnalytics
	mu            sync.RWMutex
}

// CollectionAnalytics holds analytics for a collection
type CollectionAnalytics struct {
	Address           string             `json:"address"`
	Name             string            `json:"name"`
	TotalSupply      int64             `json:"totalSupply"`
	HoldersCount    int64             `json:"holdersCount"`
	TransfersCount  int64             `json:"transfersCount"`
	FloorPrice      float64           `json:"floorPrice"`
	AvgPrice        float64           `json:"avgPrice"`
	Volume24h       float64           `json:"volume24h"`
	VolumeChange24h float64           `json:"volumeChange24h"`
	MarketCap      float64           `json:"marketCap"`
	RarityScores    map[string]float64 `json:"rarityScores"`
	TraitDistribution map[string]map[string]int `json:"traitDistribution"`
	UpdatedAt       time.Time        `json:"updatedAt"`
}

// NFTWithRarity represents an NFT with rarity data
type NFTWithRarity struct {
	TokenID         string            `json:"tokenId"`
	Owner           string            `json:"owner"`
	Collection     string            `json:"collection"`
	Name           string            `json:"name"`
	ImageURL       string            `json:"imageUrl"`
	Attributes     []Trait           `json:"attributes"`
	RarityScore    float64           `json:"rarityScore"`
	RarityRank     int               `json:"rarityRank"`
	Percentile    float64           `json:"percentile"`
	EstimatedValue float64           `json:"estimatedValue"`
}

// Trait represents an NFT trait
type Trait struct {
	TraitType   string  `json:"traitType"`
	TraitValue string  `json:"traitValue"`
	Rarity    float64 `json:"rarity"`
}

// CollectionStats represents collection statistics
type CollectionStats struct {
	Address           string  `json:"address"`
	TotalSupply     int64   `json:"totalSupply"`
	UniqueHolders  int64   `json:"uniqueHolders"`
	TotalTransfers int64   `json:"totalTransfers"`
	OwnerDistribution map[int]int `json:"ownerDistribution"`
	HolderConcentration float64 `json:"holderConcentration"`
}

// RarityConfig holds rarity calculation configuration
type RarityConfig struct {
	UseOpenRarity bool
	UseTraitRarity bool
	UseStatisticalRarity bool
	WeightTraitCount bool
}

// =============================================================================
// CONSTRUCTOR
// =============================================================================

// NewNFTAnalyticsService creates a new NFT analytics service
func NewNFTAnalyticsService(db *sql.DB) *NFTAnalyticsService {
	return &NFTAnalyticsService{
		db:              db,
		collectionCache: make(map[string]*CollectionAnalytics),
	}
}

// =============================================================================
// RARITY CALCULATION
// =============================================================================

// CalculateRarity calculates rarity scores for NFTs in a collection
func (s *NFTAnalyticsService) CalculateRarity(ctx context.Context, collectionAddr string, config *RarityConfig) (map[string]float64, error) {
	if config == nil {
		config = &RarityConfig{
			UseTraitRarity:        true,
			UseStatisticalRarity: true,
			WeightTraitCount:    true,
		}
	}

	// Get all NFTs with traits
	nfts, err := s.getNFTsWithTraits(ctx, collectionAddr)
	if err != nil {
		return nil, err
	}

	if len(nfts) == 0 {
		return nil, fmt.Errorf("no NFTs found")
	}

	// Calculate trait frequencies
	traitFrequencies := s.calculateTraitFrequencies(nfts)

	// Calculate rarity scores using multiple methods
	scores := make(map[string]float64)

	if config.UseTraitRarity {
		traitRarityScores := s.calculateTraitRarity(nfts, traitFrequencies)
		for tokenID, score := range traitRarityScores {
			scores[tokenID] = score
		}
	}

	if config.UseStatisticalRarity {
		statisticalScores := s.calculateStatisticalRarity(nfts)
		for tokenID, score := range statisticalScores {
			if config.UseTraitRarity {
				// Average the scores
				scores[tokenID] = (scores[tokenID] + score) / 2
			} else {
				scores[tokenID] = score
			}
		}
	}

	// Rank NFTs
	ranks := s.calculateRanks(scores)

	// Store in cache
	s.mu.Lock()
	s.collectionCache[collectionAddr] = &CollectionAnalytics{
		Address:        collectionAddr,
		RarityScores:   scores,
		UpdatedAt:     time.Now(),
	}
	s.mu.Unlock()

	return scores, nil
}

// calculateTraitRarity calculates rarity using trait frequencies
func (s *NFTAnalyticsService) calculateTraitRarity(nfts []NFTWithRarity, frequencies map[string]map[string]int) map[string]float64 {
	scores := make(map[string]float64)
	totalNFTs := float64(len(nfts))

	for _, nft := range nfts {
		score := 0.0
		traitCount := float64(len(nft.Attributes))

		for _, trait := range nft.Attributes {
			freq := frequencies[trait.TraitType][trait.TraitValue]
			if freq > 0 {
				// Calculate trait rarity (1 / frequency)
				traitRarity := totalNFTs / float64(freq)
				// Log scale to prevent extreme values
				score += math.Log(traitRarity)
			}
		}

		// Weight by trait count
		if traitCount > 0 {
			score = score / traitCount
		}

		scores[nft.TokenID] = score
	}

	return scores
}

// calculateStatisticalRarity calculates rarity using statistical methods
func (s *NFTAnalyticsService) calculateStatisticalRarity(nfts []NFTWithRarity) map[string]float64 {
	scores := make(map[string]float64)

	if len(nfts) == 0 {
		return scores
	}

	// Calculate average trait count
	var totalTraits float64
	for _, nft := range nfts {
		totalTraits += float64(len(nft.Attributes))
	}
	avgTraits := totalTraits / float64(len(nfts))

	// Calculate standard deviation
	var variance float64
	for _, nft := range nfts {
		diff := float64(len(nft.Attributes)) - avgTraits
		variance += diff * diff
	}
	stdDev := math.Sqrt(variance / float64(len(nfts)))

	// Calculate statistical rarity score
	for _, nft := range nfts {
		traitCount := float64(len(nft.Attributes))
		
		if stdDev > 0 {
			// Z-score based rarity
			zScore := (traitCount - avgTraits) / stdDev
			// Convert to rarity score (higher is more rare)
			scores[nft.TokenID] = math.Exp(zScore)
		} else {
			scores[nft.TokenID] = 1.0
		}
	}

	return scores
}

// calculateTraitFrequencies calculates trait value frequencies
func (s *NFTAnalyticsService) calculateTraitFrequencies(nfts []NFTWithRarity) map[string]map[string]int {
	frequencies := make(map[string]map[string]int)

	for _, nft := range nfts {
		for _, trait := range nft.Attributes {
			if frequencies[trait.TraitType] == nil {
				frequencies[trait.TraitType] = make(map[string]int)
			}
			frequencies[trait.TraitType][trait.TraitValue]++
		}
	}

	return frequencies
}

// calculateRanks calculates rarity ranks from scores
func (s *NFTAnalyticsService) calculateRanks(scores map[string]float64) map[string]int {
	// Sort by score (descending)
	type pair struct {
		id    string
		score float64
	}

	pairs := make([]pair, 0, len(scores))
	for id, score := range scores {
		pairs = append(pairs, pair{id, score})
	}

	sort.Slice(pairs, func(i, j int) bool {
		return pairs[i].score > pairs[j].score
	})

	ranks := make(map[string]int)
	for i, p := range pairs {
		ranks[p.id] = i + 1
	}

	return ranks
}

// =============================================================================
// ANALYTICS
// =============================================================================

// GetCollectionAnalytics returns comprehensive analytics for a collection
func (s *NFTAnalyticsService) GetCollectionAnalytics(ctx context.Context, collectionAddr string) (*CollectionAnalytics, error) {
	// Check cache first
	s.mu.RLock()
	if cached, ok := s.collectionCache[collectionAddr]; ok {
		if time.Since(cached.UpdatedAt) < 5*time.Minute {
			s.mu.RUnlock()
			return cached, nil
		}
	}
	s.mu.RUnlock()

	// Get from database
	query := `
		SELECT 
			c.address,
			c.name,
			c.total_supply,
			COUNT(DISTINCT nft_owner) as holders,
			COUNT(nft_transfers.id) as transfers
		FROM nft_collections c
		LEFT JOIN nfts ON nft_collection = c.address
		LEFT JOIN nft_transfers ON nft_transfers.collection = c.address
		WHERE c.address = $1
		GROUP BY c.address
	`

	analytics := &CollectionAnalytics{
		Address:        collectionAddr,
		TraitDistribution: make(map[string]map[string]int),
		UpdatedAt:     time.Now(),
	}

	err := s.db.QueryRowContext(ctx, query, collectionAddr).Scan(
		&analytics.Address,
		&analytics.Name,
		&analytics.TotalSupply,
		&analytics.HoldersCount,
		&analytics.TransfersCount,
	)

	if err != nil {
		return nil, err
	}

	// Calculate market cap
	analytics.MarketCap = analytics.FloorPrice * float64(analytics.TotalSupply)

	// Store in cache
	s.mu.Lock()
	s.collectionCache[collectionAddr] = analytics
	s.mu.Unlock()

	return analytics, nil
}

// GetTopMinters returns the top minters by volume
func (s *NFTAnalyticsService) GetTopMinters(ctx context.Context, collectionAddr string, limit int) ([]struct {
	Address string `json:"address"`
	Mints  int64  `json:"mints"`
}, error) {
	query := `
		SELECT to_address, COUNT(*) as mints
		FROM nft_mints
		WHERE collection_address = $1
		GROUP BY to_address
		ORDER BY mints DESC
		LIMIT $2
	`

	rows, err := s.db.QueryContext(ctx, query, collectionAddr, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var result []struct {
		Address string
		Mints  int64
	}

	for rows.Next() {
		var r struct {
			Address string
			Mints  int64
		}
		if err := rows.Scan(&r.Address, &r.Mints); err != nil {
			continue
		}
		result = append(result, r)
	}

	return result, nil
}

// GetHolderDistribution returns holder distribution statistics
func (s *NFTAnalyticsService) GetHolderDistribution(ctx context.Context, collectionAddr string) (*CollectionStats, error) {
	query := `
		SELECT 
			collection_address,
			COUNT(*) as total,
			COUNT(DISTINCT owner) as unique_holders
		FROM nft_owners
		WHERE collection_address = $1
		GROUP BY collection_address
	`

	stats := &CollectionStats{
		Address:             collectionAddr,
		OwnerDistribution: make(map[int]int),
	}

	err := s.db.QueryRowContext(ctx, query, collectionAddr).Scan(
		&stats.Address,
		&stats.TotalSupply,
		&stats.UniqueHolders,
	)

	if err != nil {
		return nil, err
	}

	// Calculate holder concentration (top 10% holders % of supply)
	stats.HolderConcentration = s.calculateHolderConcentration(ctx, collectionAddr)

	return stats, nil
}

// calculateHolderConcentration calculates how concentrated ownership is
func (s *NFTAnalyticsService) calculateHolderConcentration(ctx context.Context, collectionAddr string) float64 {
	query := `
		SELECT COUNT(*)
		FROM nft_owners
		WHERE collection_address = $1
		AND owner IN (
			SELECT owner
			FROM nft_owners
			WHERE collection_address = $1
			GROUP BY owner
			ORDER BY COUNT(*) DESC
			LIMIT (
				SELECT COUNT(DISTINCT owner) * 0.1
				FROM nft_owners
				WHERE collection_address = $1
			)
		)
	`

	var count int64
	err := s.db.QueryRowContext(ctx, query, collectionAddr).Scan(&count)
	if err != nil {
		return 0
	}

	return float64(count)
}

// =============================================================================
// INTERNAL HELPERS
// =============================================================================

// getNFTsWithTraits retrieves NFTs with their traits
func (s *NFTAnalyticsService) getNFTsWithTraits(ctx context.Context, collectionAddr string) ([]NFTWithRarity, error) {
	query := `
		SELECT 
			n.token_id,
			n.owner,
			n.collection,
			n.name,
			n.image_url,
			t.attributes
		FROM nfts n
		LEFT JOIN (
			SELECT collection, token_id, 
			       json_agg(json_build2('traitType', trait_type, 'traitValue', trait_value)) as attributes
			FROM nft_traits
			WHERE collection = $1
			GROUP BY collection, token_id
		) t ON n.collection = t.collection AND n.token_id = t.token_id
		WHERE n.collection = $1
	`

	rows, err := s.db.QueryContext(ctx, query, collectionAddr)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var nfts []NFTWithRarity
	for rows.Next() {
		var nft NFTWithRarity
		var attrsJSON sql.NullString

		if err := rows.Scan(&nft.TokenID, &nft.Owner, &nft.Collection, &nft.Name, &nft.ImageURL, &attrsJSON); err != nil {
			continue
		}

		if attrsJSON.Valid {
			json.Unmarshal([]byte(attrsJSON.String), &nft.Attributes)
		}

		nfts = append(nfts, nft)
	}

	return nfts, nil
}

// ParseTokenID parses a token ID from various formats
func ParseTokenID(tokenID string) (string, error) {
	tokenID = strings.TrimSpace(tokenID)

	// Check if it's a number
	if _, err := strconv.Atoi(tokenID); err == nil {
		return tokenID, nil
	}

	// Check if it's hex
	if strings.HasPrefix(tokenID, "0x") || strings.HasPrefix(tokenID, "0X") {
		n, ok := new(big.Int).SetString(tokenID[2:], 16)
		if !ok {
			return "", fmt.Errorf("invalid token ID: %s", tokenID)
		}
		return n.String(), nil
	}

	return tokenID, nil
}

var _ = json.Marshal // Use JSON
var _ = fmt.Sprintf // Use fmt
var _ = math.Log // Use math.Log
var _ = sort.Slice // Use sort
var _ = strconv.Atoi // Use strconv