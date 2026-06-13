// Package defi provides DeFi analytics - TVL, DEX pairs, liquidity pools
package defi

import (
	"context"
	"fmt"
	"math"
	"sort"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds DeFi analytics configuration
type Config struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
}

// TVLData represents Total Value Locked data
type TVLData struct {
	TotalTVL      float64            `json:"totalTvl"`
	ByProtocol    map[string]float64 `json:"byProtocol"`
	ByChain      map[string]float64 `json:"byChain"`
	Change24h    float64            `json:"change24h"`
	Timestamp     time.Time         `json:"timestamp"`
}

// DEXPair represents a DEX pair
type DEXPair struct {
	Address     string  `json:"address"`
	Token0     string  `json:"token0"`
	Token1     string  `json:"token1"`
	Reserve0   float64 `json:"reserve0"`
	Reserve1   float64 `json:"reserve1"`
	Liquidity  float64 `json:"liquidity"`
	Volume24h  float64 `json:"volume24h"`
	VolumeChange float64 `json:"volumeChange24h"`
	TxCount24h int     `json:"txCount24h"`
}

// LiquidityPool represents a liquidity pool
type LiquidityPool struct {
	Address      string  `json:"address"`
	Protocol     string  `json:"protocol"`
	Tokens       []string `json:"tokens"`
	Reserves     []float64 `json:"reserves"`
	Liquidity    float64   `json:"liquidity"`
	APR          float64   `json:"apr"`
	Volume24h    float64   `json:"volume24h"`
}

// Server represents the DeFi analytics server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
	tvl   *TVLData
}

// NewServer creates a new DeFi analytics server
func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 8})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	if err := createTables(ctx, pool); err != nil {
		return nil, fmt.Errorf("failed to create tables: %w", err)
	}
	srv := &Server{cfg: cfg, pool: pool, redis: rdb, tvl: &TVLData{}}
	go srv.startUpdater()
	return srv, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) error {
	queries := []string{
		`CREATE TABLE IF NOT EXISTS tvl_history (id SERIAL PRIMARY KEY, total_tvl DECIMAL(20,2), by_protocol JSONB, by_chain JSONB, change_24h DECIMAL(10,4), timestamp BIGINT NOT NULL)`,
		`CREATE TABLE IF NOT EXISTS dex_pairs (id SERIAL PRIMARY KEY, address VARCHAR(42) NOT NULL UNIQUE, token0 VARCHAR(42), token1 VARCHAR(42), reserve0 DECIMAL(30,8), reserve1 DECIMAL(30,8), liquidity DECIMAL(20,2), volume_24h DECIMAL(20,2), tx_count_24h INTEGER, updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`,
		`CREATE TABLE IF NOT EXISTS liquidity_pools (id SERIAL PRIMARY KEY, address VARCHAR(42) NOT NULL UNIQUE, protocol VARCHAR(50), tokens JSONB, reserves JSONB, liquidity DECIMAL(20,2), apr DECIMAL(10,4), volume_24h DECIMAL(20,2), updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`,
	}
	for _, q := range queries {
		if _, err := pool.Exec(ctx, q); err != nil {
			return err
		}
	}
	return nil
}

func (s *Server) startUpdater() {
	ticker := time.NewTicker(s.cfg.UpdateInterval)
	defer ticker.Stop()
	for range ticker.C {
		if err := s.updateTVL(); err != nil {
			fmt.Printf("failed to update TVL: %v\n", err)
		}
	}
}

func (s *Server) updateTVL() error {
	ctx := context.Background()
	rows, err := s.pool.Query(ctx, `SELECT address, liquidity FROM dex_pairs UNION ALL SELECT address, liquidity FROM liquidity_pools`)
	if err != nil {
		return err
	}
	defer rows.Close()

	var totalTVL float64
	byProtocol := make(map[string]float64)
	byChain := make(map[string]float64)

	for rows.Next() {
		var address string
		var liquidity float64
		if err := rows.Scan(&address, &liquidity); err != nil {
			continue
		}
		totalTVL += liquidity
	}

	s.mu.Lock()
	s.tvl = &TVLData{TotalTVL: totalTVL, ByProtocol: byProtocol, ByChain: byChain, Timestamp: time.Now()}
	s.mu.Unlock()

	s.redis.Set(ctx, "defi:tvl", fmt.Sprintf("%.2f", totalTVL), time.Hour)
	return nil
}

// GetTVL returns current TVL
func (s *Server) GetTVL(ctx context.Context) (*TVLData, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	if s.tvl != nil {
		return s.tvl, nil
	}
	data, err := s.redis.Get(ctx, "defi:tvl").Result()
	if err == nil {
		var tvl float64
		fmt.Sscanf(data, "%f", &tvl)
		return &TVLData{TotalTVL: tvl, Timestamp: time.Now()}, nil
	}
	return &TVLData{TotalTVL: 0, Timestamp: time.Now()}, nil
}

// GetDEXPairs returns DEX pairs
func (s *Server) GetDEXPairs(ctx context.Context, limit int) ([]DEXPair, error) {
	rows, err := s.pool.Query(ctx, `SELECT address, token0, token1, reserve0, reserve1, liquidity, volume_24h, tx_count_24h FROM dex_pairs ORDER BY liquidity DESC LIMIT $1`, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var pairs []DEXPair
	for rows.Next() {
		var p DEXPair
		if err := rows.Scan(&p.Address, &p.Token0, &p.Token1, &p.Reserve0, &p.Reserve1, &p.Liquidity, &p.Volume24h, &p.TxCount24h); err != nil {
			continue
		}
		pairs = append(pairs, p)
	}
	return pairs, nil
}

// GetLiquidityPools returns liquidity pools
func (s *Server) GetLiquidityPools(ctx context.Context, limit int) ([]LiquidityPool, error) {
	rows, err := s.pool.Query(ctx, `SELECT address, protocol, tokens, reserves, liquidity, apr, volume_24h FROM liquidity_pools ORDER BY liquidity DESC LIMIT $1`, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var pools []LiquidityPool
	for rows.Next() {
		var p LiquidityPool
		if err := rows.Scan(&p.Address, &p.Protocol, &p.Tokens, &p.Reserves, &p.Liquidity, &p.APR, &p.Volume24h); err != nil {
			continue
		}
		pools = append(pools, p)
	}
	return pools, nil
}

// FormatTVL formats TVL
func FormatTVL(tvl float64) string {
	if tvl >= 1e12 {
		return fmt.Sprintf("$%.2fT", tvl/1e12)
	}
	if tvl >= 1e9 {
		return fmt.Sprintf("$%.2fB", tvl/1e9)
	}
	if tvl >= 1e6 {
		return fmt.Sprintf("$%.2fM", tvl/1e6)
	}
	return fmt.Sprintf("$%.2fK", tvl/1e3)
}

// CalculateAPR calculates APR
func CalculateAPR(volume, liquidity float64) float64 {
	if liquidity == 0 {
		return 0
	}
	return (volume * 0.003 * 365 / liquidity) * 100
}

// CalculateTVLChange calculates 24h change
func CalculateTVLChange(current, previous float64) float64 {
	if previous == 0 {
		return 0
	}
	return ((current - previous) / previous) * 100
}

// SortPairsByVolume sorts by volume
func SortPairsByVolume(pairs []DEXPair) {
	sort.Slice(pairs, func(i, j int) bool {
		return pairs[i].Volume24h > pairs[j].Volume24h
	})
}

// CalculatePriceImpact calculates price impact
func CalculatePriceImpact(reserveIn, reserveOut, amountIn float64) float64 {
	amountInWithFee := amountIn * 0.997
	numerator := amountInWithFee * reserveOut
	denominator := reserveIn + amountInWithFee
	amountOut := numerator / denominator
	idealAmountOut := amountIn * reserveOut / reserveIn
	impact := ((idealAmountOut - amountOut) / idealAmountOut) * 100
	return math.Abs(impact)
}