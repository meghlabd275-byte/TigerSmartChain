// Package nftfloor provides NFT floor price service
package nftfloor

import (
	"context"
	"encoding/json"
	"fmt"
	"math"
	"sort"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds NFT floor price configuration
type Config struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
}

// NFTCollection represents NFT collection data
type NFTCollection struct {
	Address          string  `json:"address"`
	Name            string  `json:"name"`
	Symbol          string  `json:"symbol"`
	FloorPrice      float64 `json:"floorPrice"`
	FloorPriceChange float64 `json:"floorPriceChange24h"`
	Volume24h       float64 `json:"volume24h"`
	VolumeChange24h float64 `json:"volumeChange24h"`
	TotalSupply    int     `json:"totalSupply"`
	NumOwners      int     `json:"numOwners"`
	AvgPrice       float64 `json:"avgPrice"`
	Timestamp      time.Time `json:"timestamp"`
}

// NFTTrait represents NFT trait
type NFTTrait struct {
	TraitType   string  `json:"traitType"`
	TraitValue  string  `json:"traitValue"`
	Rarity     float64 `json:"rarity"`
	NumNFTs    int     `json:"numNfts"`
}

// NFT represents an NFT
type NFT struct {
	Address       string          `json:"address"`
	TokenID       string          `json:"tokenId"`
	Name         string          `json:"name"`
	Owner        string          `json:"owner"`
	ImageURL     string          `json:"imageUrl"`
	Price        float64        `json:"price"`
	Traits       []NFTTrait     `json:"traits"`
	IsForSale    bool           `json:"isForSale"`
	LastSalePrice float64       `json:"lastSalePrice"`
}

// FloorPriceHistory represents historical floor price
type FloorPriceHistory struct {
	Timestamp    time.Time
	FloorPrice  float64
	Volume     float64
}

// Server represents the NFT floor price server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
	collections map[string]*NFTCollection
}

// NewServer creates a new NFT floor price server
func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 5})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	if err := createTables(ctx, pool); err != nil {
		return nil, fmt.Errorf("failed to create tables: %w", err)
	}
	srv := &Server{cfg: cfg, pool: pool, redis: rdb, collections: make(map[string]*NFTCollection)}
	go srv.startUpdater()
	return srv, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) error {
	queries := []string{
		`CREATE TABLE IF NOT EXISTS nft_floor_prices (id SERIAL PRIMARY KEY, collection_address VARCHAR(42) NOT NULL, floor_price VARCHAR(66), volume_24h VARCHAR(66), total_supply INTEGER, num_owners INTEGER, timestamp BIGINT NOT NULL, UNIQUE(collection_address, timestamp))`,
		`CREATE TABLE IF NOT EXISTS nft_traits (id SERIAL PRIMARY KEY, collection_address VARCHAR(42) NOT NULL, trait_type VARCHAR(100) NOT NULL, trait_value VARCHAR(255) NOT NULL, rarity_score DECIMAL(5,4), count INTEGER DEFAULT 0)`,
		`CREATE INDEX IF NOT EXISTS idx_nft_floor_address ON nft_floor_prices(collection_address)`,
		`CREATE INDEX IF NOT EXISTS idx_nft_traits_collection ON nft_traits(collection_address)`,
	}
	for _, q := range queries {
		if _, err := pool.Exec(ctx, q); err != nil {
			return err
		}
	}
	return nil
}

// startUpdater starts the floor price updater
func (s *Server) startUpdater() {
	ticker := time.NewTicker(s.cfg.UpdateInterval)
	defer ticker.Stop()
	for range ticker.C {
		if err := s.updateFloorPrices(); err != nil {
			fmt.Printf("failed to update floor prices: %v\n", err)
		}
	}
}

// updateFloorPrices updates floor prices for all collections
func (s *Server) updateFloorPrices() error {
	ctx := context.Background()
	
	rows, err := s.pool.Query(ctx, `SELECT address, name, symbol FROM collections WHERE is_active = true`)
	if err != nil {
		return err
	}
	defer rows.Close()
	
	type collectionInfo struct {
		address string
		name    string
		symbol  string
	}
	
	var collections []collectionInfo
	for rows.Next() {
		var c collectionInfo
		if err := rows.Scan(&c.address, &c.name, &c.symbol); err != nil {
			return err
		}
		collections = append(collections, c)
	}
	
	for _, c := range collections {
		floorPrice := s.calculateMockFloorPrice(c.address)
		
		s.mu.Lock()
		s.collections[c.address] = &NFTCollection{
			Address:          c.address,
			Name:            c.name,
			Symbol:          c.symbol,
			FloorPrice:      floorPrice.FloorPrice,
			FloorPriceChange: floorPrice.FloorPriceChange,
			Volume24h:       floorPrice.Volume24h,
			TotalSupply:     floorPrice.TotalSupply,
			NumOwners:       floorPrice.NumOwners,
			Timestamp:       time.Now(),
		}
		s.mu.Unlock()
		
		// Store in database
		s.pool.Exec(ctx, `INSERT INTO nft_floor_prices (collection_address, floor_price, volume_24h, total_supply, num_owners, timestamp) VALUES ($1, $2, $3, $4, $5, $6)`,
			c.address, fmt.Sprintf("%.8f", floorPrice.FloorPrice), fmt.Sprintf("%.0f", floorPrice.Volume24h), floorPrice.TotalSupply, floorPrice.NumOwners, time.Now().Unix())
	}
	
	return nil
}

func (s *Server) calculateMockFloorPrice(address string) *NFTCollection {
	hash := int64(0)
	for i, c := range address {
		hash += int64(c) * int64(i+1)
	}
	floorPrice := 0.1 + float64(hash%100)/10
	volume := 1e5 + float64(hash%100000)
	supply := 1000 + (hash % 10000)
	owners := supply / 2
	
	return &NFTCollection{
		Address:        address,
		FloorPrice:    floorPrice,
		FloorPriceChange: (float64(hash%20) - 10) / 10,
		Volume24h:     volume,
		TotalSupply:   supply,
		NumOwners:     owners,
	}
}

// GetFloorPrice returns floor price for a collection
func (s *Server) GetFloorPrice(ctx context.Context, address string) (*NFTCollection, error) {
	s.mu.RLock()
	collection, ok := s.collections[address]
	s.mu.RUnlock()
	
	if ok {
		return collection, nil
	}
	
	// Try database
	var nc NFTCollection
	var floorPriceStr, volumeStr string
	err := s.pool.QueryRow(ctx, `SELECT address, name, symbol, floor_price, volume_24h, total_supply, num_owners FROM collections WHERE address = $1`, address).Scan(&nc.Address, &nc.Name, &nc.Symbol, &floorPriceStr, &volumeStr, &nc.TotalSupply, &nc.NumOwners)
	if err != nil {
		return nil, err
	}
	fmt.Sscanf(floorPriceStr, "%f", &nc.FloorPrice)
	fmt.Sscanf(volumeStr, "%f", &nc.Volume24h)
	
	return &nc, nil
}

// GetTopCollections returns top collections by volume
func (s *Server) GetTopCollections(ctx context.Context, limit int) ([]*NFTCollection, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	type collectionWithVolume struct {
		collection *NFTCollection
		volume     float64
	}
	
	var collections []collectionWithVolume
	for _, c := range s.collections {
		collections = append(collections, collectionWithVolume{collection: c, volume: c.Volume24h})
	}
	
	// Sort by volume
	sort.Slice(collections, func(i, j int) bool {
		return collections[i].volume > collections[j].volume
	})
	
	var result []*NFTCollection
	for i := 0; i < limit && i < len(collections); i++ {
		result = append(result, collections[i].collection)
	}
	
	return result, nil
}

// GetTraitRarity calculates trait rarity for a collection
func (s *Server) GetTraitRarity(ctx context.Context, address string) ([]NFTTrait, error) {
	rows, err := s.pool.Query(ctx, `SELECT trait_type, trait_value, rarity_score, count FROM nft_traits WHERE collection_address = $1 ORDER BY rarity_score ASC`, address)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var traits []NFTTrait
	for rows.Next() {
		var t NFTTrait
		if err := rows.Scan(&t.TraitType, &t.TraitValue, &t.Rarity, &t.NumNFTs); err != nil {
			return nil, err
		}
		traits = append(traits, t)
	}
	
	return traits, nil
}

// GetFloorPriceHistory returns historical floor price
func (s *Server) GetFloorPriceHistory(ctx context.Context, address string, days int) ([]FloorPriceHistory, error) {
	rows, err := s.pool.Query(ctx, `SELECT timestamp, floor_price, volume_24h FROM nft_floor_prices WHERE collection_address = $1 AND timestamp > $2 ORDER BY timestamp DESC`, address, time.Now().Unix()-int64(days*86400))
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var history []FloorPriceHistory
	for rows.Next() {
		var h FloorPriceHistory
		var floorPriceStr, volumeStr string
		if err := rows.Scan(&h.Timestamp, &floorPriceStr, &volumeStr); err != nil {
			return nil, err
		}
		fmt.Sscanf(floorPriceStr, "%f", &h.FloorPrice)
		fmt.Sscanf(volumeStr, "%f", &h.Volume)
		history = append(history, h)
	}
	
	return history, nil
}

// FormatFloorPrice formats floor price
func FormatFloorPrice(price float64) string {
	if price >= 1 {
		return fmt.Sprintf("%.2f ETH", price)
	}
	return fmt.Sprintf("%.4f ETH", price)
}

// CalculateRarityScore calculates rarity score for an NFT
func CalculateRarityScore(traits []NFTTrait) float64 {
	var rarityScore float64 = 1.0
	for _, t := range traits {
		if t.Rarity > 0 {
			rarityScore *= t.Rarity
		}
	}
	// Normalize to 0-100 scale
	return math.Max(1, 100*(1-rarityScore))
}