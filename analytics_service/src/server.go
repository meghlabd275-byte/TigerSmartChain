// Package analytics provides advanced analytics service
package analytics

import (
	"context"
	"fmt"
	"math"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

type Config struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
}

type NetworkStats struct {
	TotalBlocks      int64   `json:"totalBlocks"`
	TotalTransactions int64   `json:"totalTransactions"`
	TotalAddresses   int64   `json:"totalAddresses"`
	TPS             float64 `json:"tps"`
	AvgBlockTime    float64 `json:"avgBlockTime"`
	GasPrice        int64   `json:"gasPrice"`
	GasUsed         int64   `json:"gasUsed"`
	NetworkUtilization float64 `json:"networkUtilization"`
	Timestamp       time.Time `json:"timestamp"`
}

type TokenAnalytics struct {
	Address           string  `json:"address"`
	Name             string  `json:"name"`
	Symbol           string  `json:"symbol"`
	Price            float64 `json:"price"`
	PriceChange24h   float64 `json:"priceChange24h"`
	Volume24h        float64 `json:"volume24h"`
	MarketCap        float64 `json:"marketCap"`
	Holders         int     `json:"holders"`
	Transfers24h    int     `json:"transfers24h"`
	ActiveAddresses int     `json:"activeAddresses"`
}

type AddressAnalytics struct {
	Address          string  `json:"address"`
	TotalReceived   float64 `json:"totalReceived"`
	TotalSent       float64 `json:"totalSent"`
	TransactionCount int    `json:"transactionCount"`
	FirstActivity   time.Time `json:"firstActivity"`
	LastActivity    time.Time `json:"lastActivity"`
	AvgTransactionValue float64 `json:"avgTransactionValue"`
}

type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
}

func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, err
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 21})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, err
	}
	srv := &Server{cfg: cfg, pool: pool, redis: rdb}
	go srv.startUpdater()
	return srv, nil
}

func (s *Server) startUpdater() {
	ticker := time.NewTicker(s.cfg.UpdateInterval)
	defer ticker.Stop()
	for range ticker.C {
		s.updateStats()
	}
}

func (s *Server) updateStats() {
	ctx := context.Background()
	var stats NetworkStats
	
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM blocks").Scan(&stats.TotalBlocks)
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM transactions").Scan(&stats.TotalTransactions)
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM accounts").Scan(&stats.TotalAddresses)
	
	var txsLastHour int64
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM transactions WHERE timestamp > $1", time.Now().Unix()-3600).Scan(&txsLastHour)
	stats.TPS = float64(txsLastHour) / 3600.0
	
	s.pool.QueryRow(ctx, "SELECT AVG(gas_price) FROM transactions WHERE timestamp > $1", time.Now().Unix()-3600).Scan(&stats.GasPrice)
	s.pool.QueryRow(ctx, "SELECT SUM(gas_used) FROM transactions WHERE timestamp > $1", time.Now().Unix()-3600).Scan(&stats.GasUsed)
	
	stats.Timestamp = time.Now()
	
	data, _ := json.Marshal(stats)
	s.redis.Set(ctx, "network:stats", string(data), time.Hour)
}

func (s *Server) GetNetworkStats(ctx context.Context) (*NetworkStats, error) {
	data, err := s.redis.Get(ctx, "network:stats").Result()
	if err == nil {
		var stats NetworkStats
		json.Unmarshal([]byte(data), &stats)
		return &stats, nil
	}
	
	var stats NetworkStats
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM blocks").Scan(&stats.TotalBlocks)
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM transactions").Scan(&stats.TotalTransactions)
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM accounts").Scan(&stats.TotalAddresses)
	stats.Timestamp = time.Now()
	return &stats, nil
}

func (s *Server) GetTPSHistory(ctx context.Context, hours int) ([]struct {
	Timestamp time.Time
	TPS      float64
}, error) {
	rows, err := s.pool.Query(ctx, "SELECT timestamp, COUNT(*) FROM transactions WHERE timestamp > $1 GROUP BY timestamp ORDER BY timestamp DESC", time.Now().Unix()-int64(hours*3600))
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var history []struct {
		Timestamp time.Time
		TPS      float64
	}
	for rows.Next() {
		var h struct {
			Timestamp time.Time
			TPS      float64
		}
		var ts int64
		var count int64
		rows.Scan(&ts, &count)
		h.Timestamp = time.Unix(ts, 0)
		h.TPS = float64(count) / 3600.0
		history = append(history, h)
	}
	return history, nil
}

func (s *Server) GetTokenAnalytics(ctx context.Context, limit int) ([]TokenAnalytics, error) {
	rows, err := s.pool.Query(ctx, `SELECT t.address, t.name, t.symbol, t.price, t.volume_24h, t.market_cap, t.holders_count, 
		(SELECT COUNT(*) FROM token_transfers WHERE token_address = t.address AND timestamp > $1) as transfers_24h
		FROM tokens t WHERE t.is_active = true ORDER BY t.transfers_count DESC LIMIT $2`, time.Now().Unix()-86400, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var analytics []TokenAnalytics
	for rows.Next() {
		var a TokenAnalytics
		var priceStr, volumeStr, marketCapStr string
		if err := rows.Scan(&a.Address, &a.Name, &a.Symbol, &priceStr, &volumeStr, &marketCapStr, &a.Holders, &a.Transfers24h); err != nil {
			continue
		}
		fmt.Sscanf(priceStr, "%f", &a.Price)
		fmt.Sscanf(volumeStr, "%f", &a.Volume24h)
		fmt.Sscanf(marketCapStr, "%f", &a.MarketCap)
		analytics = append(analytics, a)
	}
	return analytics, nil
}

func (s *Server) GetAddressAnalytics(ctx context.Context, address string) (*AddressAnalytics, error) {
	var a AddressAnalytics
	a.Address = address
	
	s.pool.QueryRow(ctx, "SELECT COALESCE(SUM(value), 0) FROM transactions WHERE to_address = $1", address).Scan(&a.TotalReceived)
	s.pool.QueryRow(ctx, "SELECT COALESCE(SUM(value), 0) FROM transactions WHERE from_address = $1", address).Scan(&a.TotalSent)
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM transactions WHERE from_address = $1 OR to_address = $1", address).Scan(&a.TransactionCount)
	
	var firstTs, lastTs int64
	s.pool.QueryRow(ctx, "SELECT MIN(timestamp), MAX(timestamp) FROM transactions WHERE from_address = $1 OR to_address = $1", address).Scan(&firstTs, &lastTs)
	a.FirstActivity = time.Unix(firstTs, 0)
	a.LastActivity = time.Unix(lastTs, 0)
	
	if a.TransactionCount > 0 {
		a.AvgTransactionValue = (a.TotalReceived + a.TotalSent) / float64(a.TransactionCount)
	}
	
	return &a, nil
}

func (s *Server) GetGasAnalytics(ctx context.Context, hours int) ([]struct {
	Timestamp time.Time
	AvgGas   int64
	MinGas   int64
	MaxGas   int64
}, error) {
	rows, err := s.pool.Query(ctx, "SELECT timestamp, AVG(gas_price), MIN(gas_price), MAX(gas_price) FROM transactions WHERE timestamp > $1 GROUP BY timestamp ORDER BY timestamp DESC", time.Now().Unix()-int64(hours*3600))
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var analytics []struct {
		Timestamp time.Time
		AvgGas   int64
		MinGas   int64
		MaxGas   int64
	}
	for rows.Next() {
		var a struct {
			Timestamp time.Time
			AvgGas   int64
			MinGas   int64
			MaxGas   int64
		}
		var ts int64
		rows.Scan(&ts, &a.AvgGas, &a.MinGas, &a.MaxGas)
		a.Timestamp = time.Unix(ts, 0)
		analytics = append(analytics, a)
	}
	return analytics, nil
}

func (s *Server) GetTopAddresses(ctx context.Context, limit int) ([]AddressAnalytics, error) {
	rows, err := s.pool.Query(ctx, "SELECT address, balance FROM accounts ORDER BY balance DESC LIMIT $1", limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var addresses []AddressAnalytics
	for rows.Next() {
		var a AddressAnalytics
		var balance string
		if err := rows.Scan(&a.Address, &balance); err != nil {
			continue
		}
		fmt.Sscanf(balance, "%f", &a.TotalReceived)
		addresses = append(addresses, a)
	}
	return addresses, nil
}

func (s *Server) GetTransactionVolume(ctx context.Context, days int) ([]struct {
	Date  time.Time
	Count int64
	Volume float64
}, error) {
	rows, err := s.pool.Query(ctx, "SELECT FROM_UNIXTIME(timestamp, '%Y-%m-%d'), COUNT(*), SUM(value) FROM transactions WHERE timestamp > $1 GROUP BY FROM_UNIXTIME(timestamp, '%Y-%m-%d')", time.Now().Unix()-int64(days*86400))
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var volume []struct {
		Date  time.Time
		Count int64
		Volume float64
	}
	for rows.Next() {
		var v struct {
			Date  time.Time
			Count int64
			Volume float64
		}
		var dateStr string
		var volStr string
		rows.Scan(&dateStr, &v.Count, &volStr)
		fmt.Sscanf(volStr, "%f", &v.Volume)
		v.Date, _ = time.Parse("2006-01-02", dateStr)
		volume = append(volume, v)
	}
	return volume, nil
}

func CalculateMA(values []float64, period int) []float64 {
	if len(values) < period {
		return values
	}
	var ma []float64
	for i := period - 1; i < len(values); i++ {
		sum := 0.0
		for j := i - period + 1; j <= i; j++ {
			sum += values[j]
		}
		ma = append(ma, sum/float64(period))
	}
	return ma
}

func CalculateVolatility(values []float64) float64 {
	if len(values) < 2 {
		return 0
	}
	mean := 0.0
	for _, v := range values {
		mean += v
	}
	mean /= float64(len(values))
	
	variance := 0.0
	for _, v := range values {
		variance += math.Pow(v-mean, 2)
	}
	variance /= float64(len(values))
	
	return math.Sqrt(variance)
}
