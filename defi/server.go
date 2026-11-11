// Package defi provides DeFi analytics - TVL, DEX pairs, liquidity pools
package defi

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

// Config holds DeFi analytics configuration
type Config struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
}

// TVLData represents Total Value Locked data
type TVLData struct {
	TotalTVL   float64            `json:"totalTvl"`
	ByProtocol map[string]float64 `json:"byProtocol"`
	ByChain    map[string]float64 `json:"byChain"`
	Change24h  float64            `json:"change24h"`
	Timestamp  time.Time          `json:"timestamp"`
}

// DEXPair represents a DEX pair
type DEXPair struct {
	Address      string  `json:"address"`
	Token0       string  `json:"token0"`
	Token1       string  `json:"token1"`
	Reserve0     float64 `json:"reserve0"`
	Reserve1     float64 `json:"reserve1"`
	Liquidity    float64 `json:"liquidity"`
	Volume24h    float64 `json:"volume24h"`
	VolumeChange float64 `json:"volumeChange24h"`
	TxCount24h   int     `json:"txCount24h"`
}

// LiquidityPool represents a liquidity pool
type LiquidityPool struct {
	Address   string    `json:"address"`
	Protocol  string    `json:"protocol"`
	Tokens    []string  `json:"tokens"`
	Reserves  []float64 `json:"reserves"`
	Liquidity float64   `json:"liquidity"`
	APR       float64   `json:"apr"`
	Volume24h float64   `json:"volume24h"`
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

	tvlData := s.fetchTVLFromDB(ctx)

	s.mu.Lock()
	s.tvl = tvlData
	s.mu.Unlock()

	if data, err := json.Marshal(tvlData); err == nil {
		s.redis.Set(ctx, "defi:tvl", string(data), time.Hour)
	}

	return nil
}

// fetchTVLFromDB aggregates locked value from the indexed DEX pairs, records a
// per-protocol snapshot in defi_tvl and reports the change against the value
// stored for the previous period.
func (s *Server) fetchTVLFromDB(ctx context.Context) *TVLData {
	type protocolTVL struct {
		protocol  string
		liquidity float64
		volume    float64
	}

	rows, err := s.pool.Query(ctx, `
		SELECT COALESCE(pair_type, 'unknown'),
		       COALESCE(SUM(liquidity_usd), 0)::float8,
		       COALESCE(SUM(volume_24h), 0)::float8
		FROM dex_pairs
		GROUP BY pair_type`)
	if err != nil {
		fmt.Printf("defi: read TVL from dex_pairs: %v\n", err)
		return &TVLData{Timestamp: time.Now()}
	}

	var protocols []protocolTVL
	for rows.Next() {
		var p protocolTVL
		if err := rows.Scan(&p.protocol, &p.liquidity, &p.volume); err != nil {
			continue
		}
		protocols = append(protocols, p)
	}
	rows.Close()

	byProtocol := make(map[string]float64, len(protocols))
	var totalTVL, totalVolume float64
	for _, p := range protocols {
		byProtocol[p.protocol] = p.liquidity
		totalTVL += p.liquidity
		totalVolume += p.volume

		var previousTVL float64
		var change float64
		if err := s.pool.QueryRow(ctx, `
			SELECT tvl::float8 FROM defi_tvl
			WHERE protocol = $1 AND updated_at::date < CURRENT_DATE`, p.protocol).Scan(&previousTVL); err == nil && previousTVL > 0 {
			change = (p.liquidity - previousTVL) / previousTVL * 100
		}

		if _, err := s.pool.Exec(ctx, `
			INSERT INTO defi_tvl (protocol, tvl, tvl_change_24h, volume_24h, updated_at)
			VALUES ($1, $2, $3, $4, NOW())
			ON CONFLICT (protocol) DO UPDATE
			SET tvl = EXCLUDED.tvl, tvl_change_24h = EXCLUDED.tvl_change_24h,
			    volume_24h = EXCLUDED.volume_24h, updated_at = EXCLUDED.updated_at`,
			p.protocol, p.liquidity, change, p.volume); err != nil {
			fmt.Printf("defi: persist TVL for %s: %v\n", p.protocol, err)
		}
	}

	// The factory breakdown shows which AMM each pool belongs to.
	byChain := make(map[string]float64)
	chainRows, err := s.pool.Query(ctx, `
		SELECT COALESCE(factory_address, 'unknown'), COALESCE(SUM(liquidity_usd), 0)::float8
		FROM dex_pairs GROUP BY factory_address`)
	if err == nil {
		defer chainRows.Close()
		for chainRows.Next() {
			var factory string
			var liquidity float64
			if err := chainRows.Scan(&factory, &liquidity); err != nil {
				continue
			}
			byChain[factory] = liquidity
		}
	}

	var previousTotal float64
	var change24h float64
	if err := s.pool.QueryRow(ctx, `
		SELECT tvl::float8 FROM defi_tvl
		WHERE protocol = 'all' AND updated_at::date < CURRENT_DATE`).Scan(&previousTotal); err == nil && previousTotal > 0 {
		change24h = (totalTVL - previousTotal) / previousTotal * 100
	}

	if _, err := s.pool.Exec(ctx, `
		INSERT INTO defi_tvl (protocol, tvl, tvl_change_24h, volume_24h, updated_at)
		VALUES ('all', $1, $2, $3, NOW())
		ON CONFLICT (protocol) DO UPDATE
		SET tvl = EXCLUDED.tvl, tvl_change_24h = EXCLUDED.tvl_change_24h,
		    volume_24h = EXCLUDED.volume_24h, updated_at = EXCLUDED.updated_at`,
		totalTVL, change24h, totalVolume); err != nil {
		fmt.Printf("defi: persist aggregate TVL: %v\n", err)
	}

	return &TVLData{
		TotalTVL:   totalTVL,
		ByProtocol: byProtocol,
		ByChain:    byChain,
		Change24h:  change24h,
		Timestamp:  time.Now(),
	}
}

// GetTopPools returns the deepest indexed liquidity pools.
func (s *Server) GetTopPools(ctx context.Context, limit int) ([]*LiquidityPool, error) {
	if limit <= 0 || limit > 500 {
		limit = 100
	}

	rows, err := s.pool.Query(ctx, `
		SELECT pair_address, COALESCE(pair_type, ''), token0_address, token1_address,
		       COALESCE(reserve0, 0)::float8, COALESCE(reserve1, 0)::float8,
		       COALESCE(liquidity_usd, 0)::float8, COALESCE(volume_24h, 0)::float8
		FROM dex_pairs
		ORDER BY liquidity_usd DESC NULLS LAST
		LIMIT $1`, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	pools := make([]*LiquidityPool, 0, limit)
	for rows.Next() {
		var (
			pool               LiquidityPool
			token0, token1     string
			reserve0, reserve1 float64
		)
		if err := rows.Scan(&pool.Address, &pool.Protocol, &token0, &token1,
			&reserve0, &reserve1, &pool.Liquidity, &pool.Volume24h); err != nil {
			continue
		}
		pool.Tokens = []string{token0, token1}
		pool.Reserves = []float64{reserve0, reserve1}
		pool.APR = CalculateAPR(pool.Volume24h, pool.Liquidity)
		pools = append(pools, &pool)
	}

	return pools, nil
}

// GetDEXPairs returns the most liquid indexed DEX pairs.
func (s *Server) GetDEXPairs(ctx context.Context, limit int) ([]*DEXPair, error) {
	if limit <= 0 || limit > 500 {
		limit = 100
	}

	rows, err := s.pool.Query(ctx, `
		SELECT pair_address, token0_address, token1_address,
		       COALESCE(reserve0, 0)::float8, COALESCE(reserve1, 0)::float8,
		       COALESCE(liquidity_usd, 0)::float8, COALESCE(volume_24h, 0)::float8,
		       COALESCE(volume_change_24h, 0)::float8
		FROM dex_pairs
		ORDER BY liquidity_usd DESC NULLS LAST
		LIMIT $1`, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	pairs := make([]*DEXPair, 0, limit)
	for rows.Next() {
		var pair DEXPair
		if err := rows.Scan(&pair.Address, &pair.Token0, &pair.Token1,
			&pair.Reserve0, &pair.Reserve1, &pair.Liquidity,
			&pair.Volume24h, &pair.VolumeChange); err != nil {
			continue
		}
		pairs = append(pairs, &pair)
	}

	// Transaction counts come from the swaps indexed for the same pairs over the
	// trailing window, measured in blocks.
	counts := make(map[string]int)
	countRows, err := s.pool.Query(ctx, `
		SELECT pair_address, COUNT(*)::int FROM dex_swaps
		WHERE block_number > (SELECT COALESCE(MAX(number), 0) - 28800 FROM blocks)
		GROUP BY pair_address`)
	if err == nil {
		defer countRows.Close()
		for countRows.Next() {
			var address string
			var count int
			if err := countRows.Scan(&address, &count); err != nil {
				continue
			}
			counts[address] = count
		}
	}
	for _, pair := range pairs {
		pair.TxCount24h = counts[pair.Address]
	}

	return pairs, nil
}

// GetTVL returns the latest TVL snapshot, refreshing it when none is cached.
func (s *Server) GetTVL(ctx context.Context) (*TVLData, error) {
	s.mu.RLock()
	cached := s.tvl
	s.mu.RUnlock()
	if cached != nil {
		return cached, nil
	}

	if data, err := s.redis.Get(ctx, "defi:tvl").Result(); err == nil {
		var tvl TVLData
		if err := json.Unmarshal([]byte(data), &tvl); err == nil {
			return &tvl, nil
		}
	}

	if err := s.updateTVL(); err != nil {
		return nil, err
	}

	s.mu.RLock()
	defer s.mu.RUnlock()
	if s.tvl == nil {
		return nil, fmt.Errorf("TVL is unavailable: no indexed liquidity")
	}
	return s.tvl, nil
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
