// Package defi provides DeFi analytics - TVL, DEX pairs, liquidity pools
package defi

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"math"
	"sort"
	"strconv"
	"strings"
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
	
	// Try to fetch real TVL from DeFiLlama API
	tvlData, err := s.fetchTVLFromDeFiLlama()
	if err != nil {
		// Fallback to database
		tvlData = s.fetchTVLFromDB(ctx)
	}
	
	s.mu.Lock()
	s.tvl = tvlData
	s.mu.Unlock()
	
	// Cache in Redis
	if tvlData != nil {
		if data, err := json.Marshal(tvlData); err == nil {
			s.redis.Set(ctx, "defi:tvl", string(data), time.Hour)
		}
	}
	
	return nil
}

// fetchTVLFromDeFiLlama fetches real TVL from DeFiLlama API
func (s *Server) fetchTVLFromDeFiLlama() (*TVLData, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	
	// Fetch TVL for BNB Chain (chain id: 56)
	url := "https://api.llama.fi/v2/chains"
	
	req, err := http.NewRequestWithContext(ctx, "GET", url, nil)
	if err != nil {
		return nil, err
	}
	
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("DeFiLlama API returned status %d", resp.StatusCode)
	}
	
	var chains []DeFiLlamaChain
	if err := json.NewDecoder(resp.Body).Decode(&chains); err != nil {
		return nil, err
	}
	
	// Find BNB Chain
	for _, chain := range chains {
		if chain.ChainID == 56 || strings.ToLower(chain.Name) == "binance" {
			byProtocol := make(map[string]float64)
			byChain := make(map[string]float64)
			byChain["bsc"] = chain.TVL
			
			return &TVLData{
				TotalTVL:   chain.TVL,
				ByProtocol: byProtocol,
				ByChain:    byChain,
				Change24h:  chain.TVLChange24h,
				Timestamp:  time.Now(),
			}, nil
		}
	}
	
	return &TVLData{TotalTVL: 0, Timestamp: time.Now()}, nil
}

// fetchTVLFromDB fetches TVL from database as fallback
func (s *Server) fetchTVLFromDB(ctx context.Context) *TVLData {
	rows, err := s.pool.Query(ctx, `SELECT address, liquidity FROM dex_pairs UNION ALL SELECT address, liquidity FROM liquidity_pools`)
	if err != nil {
		return &TVLData{TotalTVL: 0, Timestamp: time.Now()}
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

	return &TVLData{
		TotalTVL:   totalTVL,
		ByProtocol: byProtocol,
		ByChain:    byChain,
		Timestamp:  time.Now(),
	}
}

// GetTopPools returns top liquidity pools
func (s *Server) GetTopPools(ctx context.Context, limit int) ([]*LiquidityPool, error) {
	// Try to fetch from DeFiLlama
	pools, err := s.fetchPoolsFromDeFiLlama(limit)
	if err == nil && len(pools) > 0 {
		return pools, nil
	}
	
	// Fallback to database
	return s.getPoolsFromDB(ctx, limit)
}

// fetchPoolsFromDeFiLlama fetches pools from DeFiLlama
func (s *Server) fetchPoolsFromDeFiLlama(limit int) ([]*LiquidityPool, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	
	url := fmt.Sprintf("https://api.llama.fi/pools/chain/BSC?limit=%d", limit)
	
	resp, err := http.GetContext(ctx, url)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	
	var pools []DeFiLlamaPool
	if err := json.NewDecoder(resp.Body).Decode(&pools); err != nil {
		return nil, err
	}
	
	result := make([]*LiquidityPool, 0, len(pools))
	for _, p := range pools {
		result = append(result, &LiquidityPool{
			Address:   p.Address,
			Protocol:  p.Project,
			Tokens:    []string{p.Token0, p.Token1},
			Reserves:  []float64{p.TVL, 0},
			Liquidity: p.TVL,
			Volume24h: p.Volume24h,
			APR:       p.APY,
		})
	}
	
	return result, nil
}

// getPoolsFromDB gets pools from database
func (s *Server) getPoolsFromDB(ctx context.Context, limit int) ([]*LiquidityPool, error) {
	rows, err := s.pool.Query(ctx, `SELECT address, protocol, tokens, reserves, liquidity, apr, volume_24h FROM liquidity_pools ORDER BY liquidity DESC LIMIT $1`, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var pools []*LiquidityPool
	for rows.Next() {
		var pool LiquidityPool
		var tokens, reserves string
		if err := rows.Scan(&pool.Address, &pool.Protocol, &tokens, &reserves, &pool.Liquidity, &pool.APR, &pool.Volume24h); err != nil {
			continue
		}
		json.Unmarshal([]byte(tokens), &pool.Tokens)
		json.Unmarshal([]byte(reserves), &pool.Reserves)
		pools = append(pools, &pool)
	}

	return pools, nil
}

// GetDEXPairs returns DEX pairs with real data
func (s *Server) GetDEXPairs(ctx context.Context, limit int) ([]*DEXPair, error) {
	// Try to fetch from CoinGecko or DeFiLlama
	pairs, err := s.fetchDEXPairsFromAPI(limit)
	if err == nil && len(pairs) > 0 {
		return pairs, nil
	}
	
	// Fallback to database
	return s.getDEXPairsFromDB(ctx, limit)
}

// fetchDEXPairsFromAPI fetches DEX pairs from API
func (s *Server) fetchDEXPairsFromAPI(limit int) ([]*DEXPair, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	
	// Get PancakeSwap pairs from DeFiLlama
	url := "https://api.llama.fi/pools/pancakeswap"
	
	resp, err := http.GetContext(ctx, url)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	
	var pools []DeFiLlamaPool
	if err := json.NewDecoder(resp.Body).Decode(&pools); err != nil {
		return nil, err
	}
	
	result := make([]*DEXPair, 0, len(pools))
	for i, p := range pools {
		if i >= limit {
			break
		}
		result = append(result, &DEXPair{
			Address:      p.Address,
			Token0:       p.Token0,
			Token1:       p.Token1,
			Reserve0:     0,
			Reserve1:     0,
			Liquidity:    p.TVL,
			Volume24h:    p.Volume24h,
			VolumeChange: 0,
			TxCount24h:   0,
		})
	}
	
	return result, nil
}

// getDEXPairsFromDB gets DEX pairs from database
func (s *Server) getDEXPairsFromDB(ctx context.Context, limit int) ([]*DEXPair, error) {
	rows, err := s.pool.Query(ctx, `SELECT address, token0, token1, reserve0, reserve1, liquidity, volume_24h FROM dex_pairs ORDER BY liquidity DESC LIMIT $1`, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var pairs []*DEXPair
	for rows.Next() {
		var pair DEXPair
		if err := rows.Scan(&pair.Address, &pair.Token0, &pair.Token1, &pair.Reserve0, &pair.Reserve1, &pair.Liquidity, &pair.Volume24h); err != nil {
			continue
		}
		pairs = append(pairs, &pair)
	}

	return pairs, nil
}

// DeFiLlama types
type DeFiLlamaChain struct {
	ChainID      int     `json:"chainId"`
	Name         string  `json:"name"`
	TVL          float64 `json:"tvl"`
	TVLChange24h float64 `json:"tvlChange24h"`
}

type DeFiLlamaPool struct {
	Address      string  `json:"address"`
	Project      string  `json:"project"`
	Token0       string  `json:"token0"`
	Token1       string  `json:"token1"`
	TVL          float64 `json:"tvl"`
	Volume24h    float64 `json:"volume24h"`
	APY          float64 `json:"apy"`
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