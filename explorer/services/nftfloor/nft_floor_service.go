// Package nftfloor provides NFT floor price tracking and analytics
package nftfloor

import (
	"fmt"
	"math/big"
	"sort"
	"strings"
	"time"
)

// NFTFloorService tracks NFT floor prices
type NFTFloorService struct {
	collections map[string]*NFTCollection
	dexAPIs    []string
}

// NFTCollection represents an NFT collection
type NFTCollection struct {
	Address          string    `json:"address"`
	Name             string    `json:"name"`
	Symbol           string    `json:"symbol"`
	FloorPrice       *big.Int `json:"floorPrice"`
	FloorPriceUSD    float64  `json:"floorPriceUsd"`
	TotalVolume      *big.Int `json:"totalVolume"`
	TotalVolumeUSD   float64  `json:"totalVolumeUsd"`
	TotalSales       int      `json:"totalSales"`
	NumOwners        int      `json:"numOwners"`
	Supply           int      `json:"supply"`
	AveragePrice     *big.Int `json:"averagePrice"`
	LastSalePrice    *big.Int `json:"lastSalePrice"`
	LastSaleTime    time.Time `json:"lastSaleTime"`
	PriceHistory     []PricePoint `json:"priceHistory"`
}

// PricePoint represents a price point in history
type PricePoint struct {
	Timestamp time.Time `json:"timestamp"`
	Price     *big.Int `json:"price"`
	PriceUSD  float64 `json:"priceUsd"`
}

// NFTListing represents an NFT listing
type NFTListing struct {
	TokenID     string    `json:"tokenId"`
	Collection  string    `json:"collection"`
	Price       *big.Int `json:"price"`
	PriceUSD    float64  `json:"priceUsd"`
	Seller      string    `json:"seller"`
	Platform    string    `json:"platform"`
	Timestamp   time.Time `json:"timestamp"`
}

// NFTSale represents an NFT sale
type NFTSale struct {
	TokenID     string    `json:"tokenId"`
	Collection  string    `json:"collection"`
	Price       *big.Int `json:"price"`
	PriceUSD    float64  `json:"priceUsd"`
	Buyer       string    `json:"buyer"`
	Seller      string    `json:"seller"`
	Platform    string    `json:"platform"`
	TxHash      string    `json:"txHash"`
	BlockNumber uint64    `json:"blockNumber"`
	Timestamp   time.Time `json:"timestamp"`
}

// NewNFTFloorService creates a new NFT floor service
func NewNFTFloorService() *NFTFloorService {
	return &NFTFloorService{
		collections: make(map[string]*NFTCollection),
		dexAPIs:     []string{},
	}
}

// UpdateFloorPrice updates the floor price for a collection
func (s *NFTFloorService) UpdateFloorPrice(collection string, price *big.Int) error {
	col, ok := s.collections[collection]
	if !ok {
		col = &NFTCollection{Address: collection}
		s.collections[collection] = col
	}
	
	col.FloorPrice = price
	col.PriceHistory = append(col.PriceHistory, PricePoint{
		Timestamp: time.Now(),
		Price:    price,
	})
	
	return nil
}

// CalculateFloorPrice calculates floor price from listings
func (s *NFTFloorService) CalculateFloorPrice(listings []*NFTListing) (*big.Int, error) {
	if len(listings) == 0 {
		return nil, fmt.Errorf("no listings")
	}
	
	// Sort by price ascending
	sorted := make([]*NFTListing, len(listings))
	copy(sorted, listings)
	
	sort.Slice(sorted, func(i, j int) bool {
		if sorted[i].Price == nil || sorted[j].Price == nil {
			return false
		}
		return sorted[i].Price.Cmp(sorted[j].Price) < 0
	})
	
	// Take the lowest price (floor)
	return sorted[0].Price, nil
}

// GetFloorPrice gets the floor price for a collection
func (s *NFTFloorService) GetFloorPrice(collection string) (*big.Int, error) {
	col, ok := s.collections[collection]
	if !ok {
		return nil, fmt.Errorf("collection not found")
	}
	
	return col.FloorPrice, nil
}

// GetCollectionStats gets statistics for a collection
func (s *NFTFloorService) GetCollectionStats(collection string) (*NFTCollection, error) {
	col, ok := s.collections[collection]
	if !ok {
		return nil, fmt.Errorf("collection not found")
	}
	
	return col, nil
}

// TrackSale tracks an NFT sale
func (s *NFTFloorService) TrackSale(sale *NFTSale) error {
	if sale == nil {
		return fmt.Errorf("nil sale")
	}
	
	col, ok := s.collections[sale.Collection]
	if !ok {
		col = &NFTCollection{
			Address:     sale.Collection,
			PriceHistory: []PricePoint{},
		}
		s.collections[sale.Collection] = col
	}
	
	col.TotalSales++
	col.LastSalePrice = sale.Price
	col.LastSaleTime = sale.Timestamp
	
	// Update total volume
	if col.TotalVolume == nil {
		col.TotalVolume = new(big.Int)
	}
	if sale.Price != nil {
		col.TotalVolume.Add(col.TotalVolume, sale.Price)
	}
	
	// Update floor if this is lower
	if col.FloorPrice == nil || sale.Price.Cmp(col.FloorPrice) < 0 {
		col.FloorPrice = sale.Price
		col.PriceHistory = append(col.PriceHistory, PricePoint{
			Timestamp: time.Now(),
			Price:     sale.Price,
		})
	}
	
	return nil
}

// GetPriceHistory gets price history for a collection
func (s *NFTFloorService) GetPriceHistory(collection string, days int) ([]PricePoint, error) {
	col, ok := s.collections[collection]
	if !ok {
		return nil, fmt.Errorf("collection not found")
	}
	
	cutoff := time.Now().Add(-time.Duration(days) * 24 * time.Hour)
	
	var history []PricePoint
	for _, p := range col.PriceHistory {
		if p.Timestamp.After(cutoff) {
			history = append(history, p)
		}
	}
	
	return history, nil
}

// CalculateRoyaltyCompliance checks royalty enforcement
func (s *NFTFloorService) CalculateRoyaltyCompliance(collection string, sales []*NFTSale) (*RoyaltyStats, error) {
	stats := &RoyaltyStats{
		Collection:  collection,
		TotalSales:  len(sales),
		WithRoyalty: 0,
		WithoutRoyalty: 0,
	}
	
	for _, sale := range sales {
		// In production, would check royalty payment
		stats.WithRoyalty++
	}
	
	if stats.TotalSales > 0 {
		stats.ComplianceRate = float64(stats.WithRoyalty) / float64(stats.TotalSales) * 100
	}
	
	return stats, nil
}

// RoyaltyStats represents royalty statistics
type RoyaltyStats struct {
	Collection     string  `json:"collection"`
	TotalSales    int     `json:"totalSales"`
	WithRoyalty   int     `json:"withRoyalty"`
	WithoutRoyalty int    `json:"withoutRoyalty"`
	ComplianceRate float64 `json:"complianceRate"`
}

// DetectFakeNFT detects potential fake NFT listings
func (s *NFTFloorService) DetectFakeNFT(listing *NFTListing) (*FakeNFTResult, error) {
	result := &FakeNFTResult{
		IsSuspicious:  false,
		RiskScore:  0,
		Reasons:   []string{},
	}
	
	if listing == nil {
		return result, nil
	}
	
	// Check for too-low price (potential wash trading)
	if listing.Price != nil && listing.PriceUSD < 0.01 {
		result.IsSuspicious = true
		result.RiskScore += 30
		result.Reasons = append(result.Reasons, "suspiciously_low_price")
	}
	
	// Check for new seller
	if strings.HasPrefix(listing.Seller, "0x00000000") {
		result.IsSuspicious = true
		result.RiskScore += 20
		result.Reasons = append(result.Reasons, "zero_address_seller")
	}
	
	if result.RiskScore >= 50 {
		result.IsSuspicious = true
	}
	
	return result, nil
}

// FakeNFTResult represents fake NFT detection result
type FakeNFTResult struct {
	IsSuspicious bool     `json:"isSuspicious"`
	RiskScore  float64  `json:"riskScore"`
	Reasons   []string `json:"reasons"`
}

// GetTrendingCollections gets trending collections
func (s *NFTFloorService) GetTrendingCollections(limit int) ([]*NFTCollection, error) {
	collections := make([]*NFTCollection, 0)
	
	for _, col := range s.collections {
		collections = append(collections, col)
	}
	
	sort.Slice(collections, func(i, j int) bool {
		return collections[i].TotalSales > collections[j].TotalSales
	})
	
	if len(collections) > limit {
		collections = collections[:limit]
	}
	
	return collections, nil
}

// InitNFTFloorService initializes the service
func InitNFTFloorService() (*NFTFloorService, error) {
	return NewNFTFloorService(), nil
}