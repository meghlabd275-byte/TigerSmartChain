// Package gasoracle provides gas price oracle with slow/standard/fast tiers
package gasoracle

import (
	"context"
	"fmt"
	"math"
	"math/big"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds gas oracle configuration
type Config struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
	SlowMultiplier float64
	FastMultiplier float64
	MinGasPrice   uint64
	MaxGasPrice   uint64
}

// GasPrice represents gas price information
type GasPrice struct {
	Slow    uint64 `json:"slow"`
	Standard uint64 `json:"standard"`
	Fast   uint64 `json:"fast"`
	BaseFee uint64 `json:"baseFee"`
	Updated time.Time `json:"updated"`
}

// GasPriceHistory represents historical gas price
type GasPriceHistory struct {
	Timestamp time.Time
	Slow      uint64
	Standard  uint64
	Fast     uint64
}

// Server represents the gas oracle server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
	current *GasPrice
}

// NewServer creates a new gas oracle server
func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 3})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	if err := createTables(ctx, pool); err != nil {
		return nil, fmt.Errorf("failed to create tables: %w", err)
	}
	srv := &Server{cfg: cfg, pool: pool, redis: rdb, current: &GasPrice{}}
	go srv.startUpdater()
	return srv, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) error {
	_, err := pool.Exec(ctx, `CREATE TABLE IF NOT EXISTS gas_prices (id SERIAL PRIMARY KEY, slow INTEGER NOT NULL, standard INTEGER NOT NULL, fast INTEGER NOT NULL, base_fee INTEGER NOT NULL, timestamp BIGINT NOT NULL, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`)
	return err
}

// startUpdater starts the gas price updater
func (s *Server) startUpdater() {
	ticker := time.NewTicker(s.cfg.UpdateInterval)
	defer ticker.Stop()
	for range ticker.C {
		if err := s.updateGasPrices(); err != nil {
			fmt.Printf("failed to update gas prices: %v\n", err)
		}
	}
}

// updateGasPrices updates the gas prices from the network
func (s *Server) updateGasPrices() error {
	ctx := context.Background()
	// Get recent transaction gas prices
	rows, err := s.pool.Query(ctx, `SELECT gas_price FROM transactions WHERE timestamp > $1 ORDER BY gas_price ASC LIMIT 1000`, time.Now().Unix()-3600)
	if err != nil {
		return err
	}
	defer rows.Close()
	
	var gasPrices []uint64
	for rows.Next() {
		var gp uint64
		if err := rows.Scan(&gp); err != nil {
			return err
		}
		gasPrices = append(gasPrices, gp)
	}
	
	if len(gasPrices) == 0 {
		// Use default if no data
		s.current = &GasPrice{
			Slow:     s.cfg.MinGasPrice,
			Standard: 5000000000,
			Fast:    10000000000,
			Updated: time.Now(),
		}
		return nil
	}
	
	// Calculate percentiles
	slow := calculatePercentile(gasPrices, 0.10)
	standard := calculatePercentile(gasPrices, 0.50)
	fast := calculatePercentile(gasPrices, 0.90)
	
	// Apply multipliers
	slow = uint64(float64(slow) * s.cfg.SlowMultiplier)
	fast = uint64(float64(fast) * s.cfg.FastMultiplier)
	
	// Ensure within bounds
	if slow < s.cfg.MinGasPrice {
		slow = s.cfg.MinGasPrice
	}
	if fast > s.cfg.MaxGasPrice {
		fast = s.cfg.MaxGasPrice
	}
	
	// Estimate base fee (simplified)
	baseFee := standard / 2
	
	s.current = &GasPrice{
		Slow:     slow,
		Standard: standard,
		Fast:    fast,
		BaseFee:  baseFee,
		Updated: time.Now(),
	}
	
	// Store in Redis
	data := fmt.Sprintf("%d,%d,%d,%d", slow, standard, fast, baseFee)
	s.redis.Set(ctx, "gas_price:latest", data, time.Hour)
	
	// Store in database
	s.pool.Exec(ctx, `INSERT INTO gas_prices (slow, standard, fast, base_fee, timestamp) VALUES ($1, $2, $3, $4, $5)`, slow, standard, fast, baseFee, time.Now().Unix())
	
	return nil
}

func calculatePercentile(sorted []uint64, percentile float64) uint64 {
	if len(sorted) == 0 {
		return 0
	}
	index := int(float64(len(sorted)-1) * percentile)
	if index >= len(sorted) {
		index = len(sorted) - 1
	}
	if index < 0 {
		index = 0
	}
	return sorted[index]
}

// GetGasPrice returns the current gas price
func (s *Server) GetGasPrice(ctx context.Context) (*GasPrice, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	if s.current == nil {
		// Try to get from Redis
		data, err := s.redis.Get(ctx, "gas_price:latest").Result()
		if err == nil {
			var slow, standard, fast, baseFee uint64
			fmt.Sscanf(data, "%d,%d,%d,%d", &slow, &standard, &fast, &baseFee)
			return &GasPrice{Slow: slow, Standard: standard, Fast: fast, BaseFee: baseFee, Updated: time.Now()}, nil
		}
		// Return default
		return &GasPrice{Slow: s.cfg.MinGasPrice, Standard: 5000000000, Fast: 10000000000, Updated: time.Now()}, nil
	}
	
	return s.current, nil
}

// GetGasPriceHistory returns historical gas prices
func (s *Server) GetGasPriceHistory(ctx context.Context, hours int) ([]GasPriceHistory, error) {
	rows, err := s.pool.Query(ctx, `SELECT timestamp, slow, standard, fast FROM gas_prices WHERE timestamp > $1 ORDER BY timestamp DESC`, time.Now().Unix()-int64(hours*3600))
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var history []GasPriceHistory
	for rows.Next() {
		var h GasPriceHistory
		if err := rows.Scan(&h.Timestamp, &h.Slow, &h.Standard, &h.Fast); err != nil {
			return nil, err
		}
		history = append(history, h)
	}
	return history, nil
}

// EstimateGas estimates gas for a transaction
func (s *Server) EstimateGas(ctx context.Context, to string, data []byte, gasLimit uint64) (uint64, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	if s.current == nil {
		return gasLimit, nil
	}
	
	// If no recipient, it's a contract creation
	if to == "" {
		// Estimate based on data length
		estimate := uint64(21000 + len(data)*68)
		if estimate > gasLimit {
			return estimate, nil
		}
		return gasLimit, nil
	}
	
	// Use standard price for estimation
	return s.current.Standard * gasLimit, nil
}

// GweiToWei converts Gwei to Wei
func GweiToWei(gwei float64) uint64 {
	return uint64(gwei * 1e9)
}

// WeiToGwei converts Wei to Gwei
func WeiToGwei(wei uint64) float64 {
	return float64(wei) / 1e9
}

// FormatGas formats gas price in human readable format
func FormatGas(wei uint64) string {
	gwei := WeiToGwei(wei)
	if gwei >= 1 {
		return fmt.Sprintf("%.2f Gwei", gwei)
	}
	return fmt.Sprintf("%.0f Wei", wei)
}