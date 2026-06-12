// Package nfts provides NFT floor price tracking and collection analytics.
package nfts

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"math"
	"math/big"
	"net/http"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
)

// =============================================================================
// CONSTANTS
// =============================================================================

const (
	// Update intervals
	FloorUpdateInterval = 30 * time.Second
	RarityUpdateInterval = 5 * time.Minute
	
	// Cache durations
	FloorCacheDuration = 15 * time.Second
	RarityCacheDuration = 5 * time.Minute
	
	// Analysis windows
	AnalysisWindow7d = 7 * 24 * time.Hour
	AnalysisWindow30d = 30 * 24 * time.Hour
)

// =============================================================================
// DATA STRUCTURES
// =============================================================================

// FloorPrice represents floor price data
type FloorPrice struct {
	Collection string    `json:"collection"`
	FloorPrice string    `json:"floorPrice"`
	FloorPriceUSD float64 `json:"floorPriceUSD"`
	
	// Historical
	Floor7dAgo    string  `json:"floor7dAgo,omitempty"`
	Floor30dAgo   string  `json:"floor30dAgo,omitempty"`
	
	// Change
	FloorChange24h float64 `json:"floorChange24h"`
	FloorChange7d  float64 `json:"floorChange7d"`
	FloorChange30d float64 `json:"floorChange30d"`
	
	// Volume
	Volume24h    string  `json:"volume24h"`
	Volume7d     string  `json:"volume7d"`
	Volume30d    string  `json:"volume30d"`
	
	// Sales
	Sales24h int64   `json:"sales24h"`
	Sales7d  int64   `json:"sales7d"`
	Sales30d int64   `json:"sales30d"`
	
	// Market
	ListedCount    int64   `json:"listedCount"`
	UniqueOwners  int64   `json:"uniqueOwners"`
	
	LastUpdated time.Time `json:"lastUpdated"`
}

// NFTRarity represents NFT rarity data
type NFTRarity struct {
	TokenID      string             `json:"tokenId"`
	Collection   string             `json:"collection"`
	RarityScore   float64            `json:"rarityScore"`
	Rank         int               `json:"rank"`
	TotalSupply  int64             `json:"totalSupply"`
	
	// Trait analysis
	TraitRarity []TraitRarityScore `json:"traitRarity"`
	
	// Percentiles
	Percentile   float64 `json:"percentile"`
	RarityTier   string  `json:"rarityTier"`
	
	LastUpdated time.Time `json:"lastUpdated"`
}

type TraitRarityScore struct {
	TraitType  string  `json:"traitType"`
	TraitValue string  `json:"traitValue"`
	RarityScore float64 `json:"rarityScore"`
	Percentile float64 `json:"percentile"`
}

// CollectionAnalytics represents comprehensive analytics
type CollectionAnalytics struct {
	Collection string `json:"collection"`
	
	// Pricing
	FloorPrice     string  `json:"floorPrice"`
	CeilingPrice  string  `json:"ceilingPrice"`
	AvgPrice7d    string  `json:"avgPrice7d"`
	AvgPrice30d   string  `json:"avgPrice30d"`
	
	// Volume
	Volume24h  string `json:"volume24h"`
	Volume7d   string `json:"volume7d"`
	Volume30d  string `json:"volume30d"`
	
	// Sales
	Sales24h int64 `json:"sales24h"`
	Sales7d  int64 `json:"sales7d"`
	Sales30d int64 `json:"sales30d"`
	
	// Holders
	UniqueHolders int64 `json:"uniqueHolders"`
	HolderDistribution []HolderTier `json:"holderDistribution"`
	
	// Traits
	TopTraits []TraitStats `json:"topTraits"`
	
	// Market health
	HealthScore   float64 `json:"healthScore"`
	LiquidityScore float64 `json:"liquidityScore"`
	VolatilityScore float64 `json:"volatilityScore"`
	
	LastUpdated time.Time `json:"lastUpdated"`
}

type HolderTier struct {
	MinBalance int64   `json:"minBalance"`
	MaxBalance int64   `json:"maxBalance"`
	Count      int64   `json:"count"`
	Percent    float64 `json:"percent"`
}

type TraitStats struct {
	TraitType  string `json:"traitType"`
	TraitValue string `json:"traitValue"`
	Count     int64  `json:"count"`
	Percent   float64 `json:"percent"`
}

// NFTFloorService provides floor price tracking
type NFTFloorService struct {
	db    *sql.DB
	redis *redis.Client
	
	// Floor prices (cached)
	floorMu sync.RWMutex
	floorPrices map[string]*FloorPrice
	
	// Rarity scores (cached)
	rarityMu sync.RWMutex
	rarityScores map[string]*NFTRarity
	
	// Historical data
	historyMu sync.RWMutex
	history map[string][]FloorPrice
	
	// Price feeds (for USD conversion)
	priceFeeds map[string]string
	
	// Config
	config *NFTFloorConfig
}

type NFTFloorConfig struct {
	DB       *sql.DB
	Redis    *redis.Client
	PriceFeeds map[string]string
}

// =============================================================================
// SERVICE INITIALIZATION
// =============================================================================

func NewNFTFloorService(cfg *NFTFloorConfig) (*NFTFloorService, error) {
	if cfg == nil {
		cfg = &NFTFloorConfig{
			PriceFeeds: make(map[string]string),
		}
	}
	
	svc := &NFTFloorService{
		db:       cfg.DB,
		redis:    cfg.Redis,
		floorPrices: make(map[string]*FloorPrice),
		rarityScores: make(map[string]*NFTRarity),
		history: make(map[string][]FloorPrice),
		priceFeeds: cfg.PriceFeeds,
		config:    cfg,
	}
	
	// Start background tasks
	go svc.updateFloorPrices()
	go svc.cleanupHistory()
	
	return svc, nil
}

// =============================================================================
// ROUTES
// =============================================================================

func (s *NFTFloorService) RegisterRoutes(r *gin.RouterGroup) {
	floor := r.Group("/floor")
	{
		floor.GET("/:collection", s.handleGetFloorPrice)
		floor.GET("/:collection/history", s.handleGetFloorHistory)
		floor.GET("/rankings", s.handleGetRankings)
		floor.GET("/trending", s.handleGetTrending)
	}
	
	rarity := r.Group("/rarity")
	{
		rarity.GET("/:collection/:tokenId", s.handleGetRarity)
		rarity.GET("/:collection/:tokenId/similar", s.handleGetSimilar)
		rarity.GET("/:collection/:tokenId/value", s.handleGetValueEstimate)
	}
	
	analytics := r.Group("/analytics")
	{
		analytics.GET("/:collection", s.handleGetAnalytics)
		analytics.GET("/:collection/holders", s.handleGetHolderDistribution)
		analytics.GET("/:collection/traits", s.handleGetTraitStats)
	}
	
	admin := r.Group("/admin")
	admin.Use(s.adminMiddleware())
	{
		admin.POST("/refresh", s.handleRefreshFloor)
		admin.POST("/refresh/:collection", s.handleRefreshCollection)
	}
}

// =============================================================================
// HANDLERS - FLOOR PRICE
// =============================================================================

func (s *NFTFloorService) handleGetFloorPrice(c *gin.Context) {
	collection := c.Param("collection")
	
	if !isValidAddress(collection) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "invalid collection address"})
		return
	}
	
	floor, err := s.getFloorPrice(collection)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "collection not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": floor})
}

func (s *NFTFloorService) handleGetFloorHistory(c *gin.Context) {
	collection := c.Param("collection")
	days := 7
	if d := c.Query("days"); d != "" {
		if parsed, err := strconv.Atoi(d); err == nil {
			days = parsed
		}
	}
	
	s.historyMu.RLock()
	history, ok := s.history[collection]
	s.historyMu.RUnlock()
	
	if !ok {
		c.JSON(http.StatusOK, gin.H{"status": "ok", "result": []FloorPrice{}})
		return
	}
	
	cutoff := time.Now().AddDate(0, 0, -days)
	filtered := make([]FloorPrice, 0)
	for _, h := range history {
		if h.LastUpdated.After(cutoff) {
			filtered = append(filtered, h)
		}
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": filtered})
}

func (s *NFTFloorService) handleGetRankings(c *gin.Context) {
	sortBy := c.DefaultQuery("sort", "floor")
	limit := 50
	if l := c.Query("limit"); l != "" {
		if parsed, err := strconv.Atoi(l); err == nil {
			limit = parsed
		}
	}
	
	floors := make([]*FloorPrice, 0)
	s.floorMu.RLock()
	for _, floor := range s.floorPrices {
		floors = append(floors, floor)
	}
	s.floorMu.RUnlock()
	
	// Sort
	switch sortBy {
	case "volume":
		sort.Slice(floors, func(i, j int) bool {
			vi, _ := strconv.ParseFloat(floors[i].Volume24h, 64)
			vj, _ := strconv.ParseFloat(floors[j].Volume24h, 64)
			return vi > vj
		})
	case "sales":
		sort.Slice(floors, func(i, j int) bool {
			return floors[i].Sales24h > floors[j].Sales24h
		})
	default: // floor
		sort.Slice(floors, func(i, j int) bool {
			fi, _ := strconv.ParseFloat(floors[i].FloorPrice, 64)
			fj, _ := strconv.ParseFloat(floors[j].FloorPrice, 64)
			return fi < fj
		})
	}
	
	if len(floors) > limit {
		floors = floors[:limit]
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": floors})
}

func (s *NFTFloorService) handleGetTrending(c *gin.Context) {
	s.floorMu.RLock()
	floors := make([]*FloorPrice, 0)
	for _, floor := range s.floorPrices {
		if floor.FloorChange24h > 0 || floor.Volume24h != "0" {
			floors = append(floors, floor)
		}
	}
	s.floorMu.RUnlock()
	
	// Sort by volume change
	sort.Slice(floors, func(i, j int) bool {
		vi, _ := strconv.ParseFloat(floors[i].Volume24h, 64)
		vj, _ := strconv.ParseFloat(floors[j].Volume24h, 64)
		return vi > vj
	})
	
	if len(floors) > 20 {
		floors = floors[:20]
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": floors})
}

// =============================================================================
// HANDLERS - RARITY
// =============================================================================

func (s *NFTFloorService) handleGetRarity(c *gin.Context) {
	collection := c.Param("collection")
	tokenID := c.Param("tokenId")
	
	if !isValidAddress(collection) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "invalid collection address"})
		return
	}
	
	rarity, err := s.getRarity(collection, tokenID)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "token not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": rarity})
}

func (s *NFTFloorService) handleGetSimilar(c *gin.Context) {
	collection := c.Param("collection")
	tokenID := c.Param("tokenId")
	limit := 10
	if l := c.Query("limit"); l != "" {
		if parsed, err := strconv.Atoi(l); err == nil {
			limit = parsed
		}
	}
	
	similar := s.getSimilarNFTs(collection, tokenID, limit)
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": similar})
}

func (s *NFTFloorService) handleGetValueEstimate(c *gin.Context) {
	collection := c.Param("collection")
	tokenID := c.Param("tokenId")
	
	estimate := s.getValueEstimate(collection, tokenID)
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": estimate})
}

// =============================================================================
// HANDLERS - ANALYTICS
// =============================================================================

func (s *NFTFloorService) handleGetAnalytics(c *gin.Context) {
	collection := c.Param("collection")
	
	analytics, err := s.getCollectionAnalytics(collection)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "collection not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": analytics})
}

func (s *NFTFloorService) handleGetHolderDistribution(c *gin.Context) {
	collection := c.Param("collection")
	
	holders := s.getHolderDistribution(collection)
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": holders})
}

func (s *NFTFloorService) handleGetTraitStats(c *gin.Context) {
	collection := c.Param("collection")
	
	traits := s.getTraitStats(collection)
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": traits})
}

// =============================================================================
// ADMIN HANDLERS
// =============================================================================

func (s *NFTFloorService) handleRefreshFloor(c *gin.Context) {
	// Refresh all floor prices
	go s.updateFloorPrices()
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "message": "floor refresh initiated"})
}

func (s *NFTFloorService) handleRefreshCollection(c *gin.Context) {
	collection := c.Param("collection")
	
	s.refreshCollection(collection)
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "message": "collection refreshed"})
}

func (s *NFTFloorService) adminMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		apiKey := c.GetHeader("X-API-Key")
		if apiKey == "" {
			apiKey = c.Query("api_key")
		}
		
		if apiKey != "admin" {
			c.JSON(http.StatusForbidden, gin.H{"status": "error", "message": "admin access required"})
			c.Abort()
			return
		}
		c.Next()
	}
}

// =============================================================================
// CORE FUNCTIONS
// =============================================================================

func (s *NFTFloorService) getFloorPrice(collection string) (*FloorPrice, error) {
	s.floorMu.RLock()
	defer s.floorMu.RUnlock()
	
	floor, ok := s.floorPrices[collection]
	if !ok {
		return nil, fmt.Errorf("not found")
	}
	
	return floor, nil
}

func (s *NFTFloorService) getRarity(collection, tokenID string) (*NFTRarity, error) {
	key := fmt.Sprintf("%s:%s", collection, tokenID)
	
	s.rarityMu.RLock()
	defer s.rarityMu.RUnlock()
	
	rarity, ok := s.rarityScores[key]
	if !ok {
		return nil, fmt.Errorf("not found")
	}
	
	return rarity, nil
}

func (s *NFTFloorService) getSimilarNFTs(collection, tokenID string, limit int) []*NFTRarity {
	s.rarityMu.RLock()
	defer s.rarityMu.RUnlock()
	
	// Find the target NFT
	targetKey := fmt.Sprintf("%s:%s", collection, tokenID)
	target, ok := s.rarityScores[targetKey]
	if !ok {
		return nil
	}
	
	// Find similar NFTs
	similar := make([]*NFTRarity, 0)
	for key, rarity := range s.rarityScores {
		if key == targetKey {
			continue
		}
		if strings.HasPrefix(key, collection) {
			// Compare trait rarity
			diff := math.Abs(rarity.RarityScore - target.RarityScore)
			if diff < 10 { // Within 10 points
				r := *rarity
				similar = append(similar, &r)
			}
		}
	}
	
	// Sort by rarity difference
	sort.Slice(similar, func(i, j int) bool {
		di := math.Abs(similar[i].RarityScore - target.RarityScore)
		dj := math.Abs(similar[j].RarityScore - target.RarityScore)
		return di < dj
	})
	
	if len(similar) > limit {
		similar = similar[:limit]
	}
	
	return similar
}

func (s *NFTFloorService) getValueEstimate(collection, tokenID string) map[string]interface{} {
	s.rarityMu.RLock()
	s.floorMu.RUnlock()
	
	key := fmt.Sprintf("%s:%s", collection, tokenID)
	rarity, rarityOK := s.rarityScores[key]
	floor, floorOK := s.floorPrices[collection]
	
	estimate := make(map[string]interface{})
	
	if rarityOK && floorOK {
		floorPrice, _ := strconv.ParseFloat(floor.FloorPrice, 64)
		
		// Calculate based on rarity percentile
		multiplier := 1.0
		if rarity.Percentile < 1 {
			multiplier = 10.0 * (1 - rarity.Percentile)
		}
		if rarity.Percentile < 0.1 {
			multiplier = 50.0
		}
		if rarity.Percentile < 0.01 {
			multiplier = 100.0
		}
		
		estimate["floorMultiple"] = multiplier
		estimate["estimatedValue"] = floorPrice * multiplier
		estimate["floorPrice"] = floor.FloorPrice
		estimate["rarityRank"] = rarity.Rank
		estimate["rarityPercentile"] = rarity.Percentile
	} else {
		estimate["error"] = "insufficient data"
	}
	
	return estimate
}

func (s *NFTFloorService) getCollectionAnalytics(collection string) (*CollectionAnalytics, error) {
	analytics := &CollectionAnalytics{
		Collection:  collection,
		HealthScore: 0.8,
	}
	
	// Get floor price
	s.floorMu.RLock()
	if floor, ok := s.floorPrices[collection]; ok {
		analytics.FloorPrice = floor.FloorPrice
		analytics.Volume24h = floor.Volume24h
		analytics.Volume7d = floor.Volume7d
		analytics.Sales24h = floor.Sales24h
		analytics.Sales7d = floor.Sales7d
	}
	s.floorMu.RUnlock()
	
	return analytics, nil
}

func (s *NFTFloorService) getHolderDistribution(collection string) []HolderTier {
	// Simplified - would analyze from database
	return []HolderTier{
		{MinBalance: 1, MaxBalance: 1, Count: 100, Percent: 50.0},
		{MinBalance: 2, MaxBalance: 5, Count: 50, Percent: 25.0},
		{MinBalance: 6, MaxBalance: 10, Count: 30, Percent: 15.0},
		{MinBalance: 11, MaxBalance: 100, Count: 20, Percent: 10.0},
	}
}

func (s *NFTFloorService) getTraitStats(collection string) []TraitStats {
	// Simplified - would analyze from database
	return []TraitStats{
		{TraitType: "Background", TraitValue: "Blue", Count: 100, Percent: 10.0},
		{TraitType: "Background", TraitValue: "Red", Count: 80, Percent: 8.0},
		{TraitType: "Eyes", TraitValue: "Laser", Count: 50, Percent: 5.0},
	}
}

// =============================================================================
// BACKGROUND TASKS
// =============================================================================

func (s *NFTFloorService) updateFloorPrices() {
	ticker := time.NewTicker(FloorUpdateInterval)
	defer ticker.Stop()
	
	for range ticker.C {
		ctx := context.Background()
		
		// In production, fetch from multiple marketplaces
		// For now, simulate updates
		s.floorMu.Lock()
		for collection := range s.floorPrices {
			floor := s.floorPrices[collection]
			floor.LastUpdated = time.Now()
			floor.FloorChange24h = 0 // Would calculate from history
		}
		s.floorMu.Unlock()
	}
}

func (s *NFTFloorService) refreshCollection(collection string) {
	// In production, fetch fresh data from marketplaces
	s.floorMu.Lock()
	s.floorPrices[collection] = &FloorPrice{
		Collection:     collection,
		FloorPrice:   "1.0",
		FloorPriceUSD: 1800.0,
		LastUpdated: time.Now(),
	}
	s.floorMu.Unlock()
}

func (s *NFTFloorService) cleanupHistory() {
	ticker := time.NewTicker(1 * time.Hour)
	defer ticker.Stop()
	
	for range ticker.C {
		s.historyMu.Lock()
		cutoff := time.Now().AddDate(0, 0, -30)
		for collection, history := range s.history {
			filtered := make([]FloorPrice, 0)
			for _, h := range history {
				if h.LastUpdated.After(cutoff) {
					filtered = append(filtered, h)
				}
			}
			s.history[collection] = filtered
		}
		s.historyMu.Unlock()
	}
}

// =============================================================================
// UTILITIES
// =============================================================================

func isValidAddress(addr string) bool {
	if addr == "" || len(addr) != 42 {
		return false
	}
	return strings.HasPrefix(strings.ToLower(addr), "0x")
}

func calculateRarityScore(traits map[string]string, totalSupply int64) float64 {
	score := 0.0
	
	for traitType, traitValue := range traits {
		// Simplified rarity calculation
		// In production, use actual trait frequencies
		frequency := 0.1 // 10% base
		score += 100 * (1 - frequency)
	}
	
	// Normalize
	if totalSupply > 0 {
		score = score / float64(len(traits))
	}
	
	return score
}

func getRarityTier(percentile float64) string {
	if percentile < 0.01 {
		return "Legendary"
	}
	if percentile < 0.1 {
		return "Epic"
	}
	if percentile < 0.25 {
		return "Rare"
	}
	if percentile < 0.5 {
		return "Uncommon"
	}
	return "Common"
}