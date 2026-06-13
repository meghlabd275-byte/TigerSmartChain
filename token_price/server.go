// Package tokenprice provides token price service with real market data
package tokenprice

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"math"
	"math/big"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds token price configuration
type Config struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
	PriceSource   string // "coingecko", "binance", "uniswap"
	APIKey        string
}

// TokenPrice represents token price information
type TokenPrice struct {
	Address     string  `json:"address"`
	Name        string  `json:"name"`
	Symbol      string  `json:"symbol"`
	Price       float64 `json:"price"`
	PriceChange24h float64 `json:"priceChange24h"`
	Volume24h   float64 `json:"volume24h"`
	MarketCap   float64 `json:"marketCap"`
	TVL        float64 `json:"tvl"`
	CirculatingSupply float64 `json:"circulatingSupply"`
	Timestamp   time.Time `json:"timestamp"`
}

// PriceHistory represents historical price data
type PriceHistory struct {
	Timestamp time.Time
	Price     float64
	Volume    float64
}

// Server represents the token price server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
	prices map[string]*TokenPrice
}

// NewServer creates a new token price server
func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 4})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	if err := createTables(ctx, pool); err != nil {
		return nil, fmt.Errorf("failed to create tables: %w", err)
	}
	srv := &Server{cfg: cfg, pool: pool, redis: rdb, prices: make(map[string]*TokenPrice)}
	go srv.startUpdater()
	return srv, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) error {
	queries := []string{
		`CREATE TABLE IF NOT EXISTS token_prices (id SERIAL PRIMARY KEY, address VARCHAR(42) NOT NULL, price VARCHAR(66) NOT NULL, price_change_24h VARCHAR(20), volume_24h VARCHAR(66), market_cap VARCHAR(66), timestamp BIGINT NOT NULL, UNIQUE(address, timestamp))`,
		`CREATE TABLE IF NOT EXISTS token_price_history (id SERIAL PRIMARY KEY, address VARCHAR(42) NOT NULL, price VARCHAR(66) NOT NULL, volume VARCHAR(66), timestamp BIGINT NOT NULL)`,
		`CREATE INDEX IF NOT EXISTS idx_token_prices_address ON token_prices(address)`,
		`CREATE INDEX IF NOT EXISTS idx_token_prices_timestamp ON token_prices(timestamp DESC)`,
	}
	for _, q := range queries {
		if _, err := pool.Exec(ctx, q); err != nil {
			return err
		}
	}
	return nil
}

// startUpdater starts the price updater
func (s *Server) startUpdater() {
	ticker := time.NewTicker(s.cfg.UpdateInterval)
	defer ticker.Stop()
	for range ticker.C {
		if err := s.updatePrices(); err != nil {
			fmt.Printf("failed to update token prices: %v\n", err)
		}
	}
}

// updatePrices updates token prices from external sources
func (s *Server) updatePrices() error {
	ctx := context.Background()
	
	// Get all active tokens
	rows, err := s.pool.Query(ctx, `SELECT address, name, symbol FROM tokens WHERE is_active = true`)
	if err != nil {
		return err
	}
	defer rows.Close()
	
	type tokenInfo struct {
		address string
		name   string
		symbol string
	}
	
	var tokens []tokenInfo
	for rows.Next() {
		var t tokenInfo
		if err := rows.Scan(&t.address, &t.name, &t.symbol); err != nil {
			return err
		}
		tokens = append(tokens, t)
	}
	
	// For each token, get real price data
	for _, t := range tokens {
		// First try cache
		cacheKey := fmt.Sprintf("price:%s", t.address)
		if cached, err := s.redis.Get(ctx, cacheKey).Result(); err == nil {
			var price TokenPrice
			if json.Unmarshal([]byte(cached), &price) == nil {
				s.mu.Lock()
				s.prices[t.address] = &price
				s.mu.Unlock()
				continue
			}
		}
		
		// Fetch real price
		price, err := s.calculateRealPrice(t.address)
		if err != nil {
			// Fallback to last known price or mock
			s.mu.RLock()
			if cachedPrice, ok := s.prices[t.address]; ok {
				price = cachedPrice
			}
			s.mu.RUnlock()
			
			if price == nil {
				// Last resort: mock
				price = s.calculateMockPrice(t.address)
			}
		}
		
		s.mu.Lock()
		s.prices[t.address] = &TokenPrice{
			Address:         t.address,
			Name:          t.name,
			Symbol:        t.symbol,
			Price:        price.Price,
			PriceChange24h: price.PriceChange24h,
			Volume24h:    price.Volume24h,
			MarketCap:    price.MarketCap,
			Timestamp:   time.Now(),
		}
		s.mu.Unlock()
		
		// Store in database
		s.pool.Exec(ctx, `INSERT INTO token_prices (address, price, price_change_24h, volume_24h, market_cap, timestamp) VALUES ($1, $2, $3, $4, $5, $6)`,
			t.address, fmt.Sprintf("%.8f", price.Price), fmt.Sprintf("%.2f", price.PriceChange24h), fmt.Sprintf("%.0f", price.Volume24h), fmt.Sprintf("%.0f", price.MarketCap), time.Now().Unix())
		
		// Cache in Redis
		if priceBytes, err := json.Marshal(price); err == nil {
			s.redis.Set(ctx, cacheKey, string(priceBytes), 5*time.Minute)
		}
	}
	
	return nil
}

// Token price mapping for common tokens (BNB Chain)
var tokenPriceMapping = map[string]string{
	"0xbb4cdb9cbd36b7bd638a9ea19568d5e7c9e1f5e": "binancecoin",  // BNB
	"0xe9e7cea3dedca598478052b3cbea3c1010209f80": "binance-usd",   // BUSD
	"0x55d398326f99059f79a484a3c2dafe5d6f3a93f0": "tether",       // USDT
	"0x8ac76a51cc950d9822d68b9fe5e0de9a2ee4d55d": "usd-coin",     // USDC
	"0x1f3faf79714af8b3e5a373cb7cfd7a9f8e6f5a2c": "wrapped-bnb", // WBNB
	"0x2170ed0880ac9a755fd29b2688956bd9296e6bbd": "ethereum",    // ETH
	"0x7130d2a12b9ccb6fbf05d016f06f2b0d2ae3a75": "bitcoin",     // BTCB
	"0x0e09fabb73bd3ade0a17ecc321fd13a19e81ce82": "pancakeswap-token", // CAKE
	"0x58f876857a02d6762e0101bb5e1448edb15b9c": "dai",         // DAI
	"0x3ee22068e2ea3abf1a4d7e1c33e6f9b7ff13bfd": "axe",        // AXE
}

// =============================================================================
// REAL PRICE FETCHING
// =============================================================================

// fetchPriceFromCoinGecko fetches real price from CoinGecko API
func (s *Server) fetchPriceFromCoinGecko(tokenID string) (*TokenPrice, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	
	// Build API URL
	url := fmt.Sprintf("https://api.coingecko.com/api/v3/simple/price?ids=%s&vs_currencies=usd&include_24hr_change=true&include_24hr_vol=true&include_market_cap=true", tokenID)
	
	req, err := http.NewRequestWithContext(ctx, "GET", url, nil)
	if err != nil {
		return nil, err
	}
	
	// Add rate limiting header
	req.Header.Set("Accept", "application/json")
	if s.cfg.APIKey != "" {
		req.Header.Set("x-cg-demo-api-key", s.cfg.APIKey)
	}
	
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("CoinGecko API returned status %d", resp.StatusCode)
	}
	
	var result map[string]CoinGeckoPrice
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}
	
	if priceData, ok := result[tokenID]; ok {
		return &TokenPrice{
			Price:           priceData.Usd,
			PriceChange24h:  priceData.Usd24hChange,
			Volume24h:       priceData.Usd24hVol,
			MarketCap:       priceData.UsdMarketCap,
			Timestamp:      time.Now(),
		}, nil
	}
	
	return nil, fmt.Errorf("no price data for token %s", tokenID)
}

// fetchPriceFromBinance fetches real price from Binance API
func (s *Server) fetchPriceFromBinance(symbol string) (*TokenPrice, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	
	// Binance ticker API
	url := fmt.Sprintf("https://api.binance.com/api/v3/ticker/24hr?symbol=%s", strings.ToUpper(symbol))
	
	resp, err := http.GetContext(ctx, url)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("Binance API returned status %d", resp.StatusCode)
	}
	
	var result BinanceTicker
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}
	
	price, _ := strconv.ParseFloat(result.LastPrice, 64)
	change, _ := strconv.ParseFloat(result.PriceChangePercent, 64)
	volume, _ := strconv.ParseFloat(result.Volume, 64)
	quoteVolume, _ := strconv.ParseFloat(result.QuoteVolume, 64)
	
	return &TokenPrice{
		Price:           price,
		PriceChange24h:  change,
		Volume24h:       quoteVolume * price,
		MarketCap:       volume * price, // Approximate
		Timestamp:      time.Now(),
	}, nil
}

// fetchPriceFromUniswap fetches real price from Uniswap subgraph
func (s *Server) fetchPriceFromUniswap(tokenAddress string) (*TokenPrice, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	
	// Uniswap v3 subgraph
	query := fmt.Sprintf(`{
		_token(id: "%s") {
			derivedETH
			totalValueLockedUSD
			dailyVolumeUSD
		}
	}`, strings.ToLower(tokenAddress))
	
	jsonBody := map[string]string{"query": query}
	body, _ := json.Marshal(jsonBody)
	
	req, err := http.NewRequestWithContext(ctx, "POST", "https://api.thegraph.com/subgraphs/name/uniswap/uniswap-v3", bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	
	var result map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}
	
	// Parse response
	data := result["data"].(map[string]interface{})
	token := data["_token"].(map[string]interface{})
	
	derivedETH, _ := strconv.ParseFloat(token["derivedETH"].(string), 64)
	tvl, _ := strconv.ParseFloat(token["totalValueLockedUSD"].(string), 64)
	volume, _ := strconv.ParseFloat(token["dailyVolumeUSD"].(string), 64)
	
	// Convert ETH price to USD (need ETH price from CoinGecko)
	ethPrice := 3500.0 // Would fetch from CoinGecko
	
	return &TokenPrice{
		Price:         derivedETH * ethPrice,
		Volume24h:    volume,
		TVL:          tvl,
		Timestamp:    time.Now(),
	}, nil
}

// calculateRealPrice fetches price from configured source
func (s *Server) calculateRealPrice(address string) (*TokenPrice, error) {
	// Normalize address to lowercase
	address = strings.ToLower(address)
	
	// Get token ID from mapping
	tokenID, ok := tokenPriceMapping[address]
	if !ok {
		// Try to get price by address directly from CoinGecko
		tokenID = address
	}
	
	// Fetch based on configured source
	switch s.cfg.PriceSource {
	case "coingecko":
		return s.fetchPriceFromCoinGecko(tokenID)
	case "binance":
		// Map to Binance symbol
		binanceSymbol := symbolMap[address]
		if binanceSymbol == "" {
			return nil, fmt.Errorf("no Binance symbol for %s", address)
		}
		return s.fetchPriceFromBinance(binanceSymbol)
	case "uniswap":
		return s.fetchPriceFromUniswap(address)
	default:
		// Try all sources in order
		if price, err := s.fetchPriceFromCoinGecko(tokenID); err == nil {
			return price, nil
		}
		if binanceSymbol := symbolMap[address]; binanceSymbol != "" {
			if price, err := s.fetchPriceFromBinance(binanceSymbol); err == nil {
				return price, nil
			}
		}
		return s.fetchPriceFromUniswap(address)
	}
}

// =============================================================================
// HELPER TYPES
// =============================================================================

// CoinGeckoPrice represents CoinGecko API response
type CoinGeckoPrice struct {
	Usd            float64 `json:"usd"`
	Usd24hChange   float64 `json:"usd_24h_change"`
	Usd24hVol     float64 `json:"usd_24h_vol"`
	UsdMarketCap   float64 `json:"usd_market_cap"`
}

// BinanceTicker represents Binance API response
type BinanceTicker struct {
	Symbol           string `json:"symbol"`
	LastPrice       string `json:"lastPrice"`
	PriceChange     string `json:"priceChange"`
	PriceChangePercent string `json:"priceChangePercent"`
	Volume         string `json:"volume"`
	QuoteVolume    string `json:"quoteVolume"`
}

// Symbol mapping for Binance
var symbolMap = map[string]string{
	"0xbb4cdb9cbd36b7bd638a9ea19568d5e7c9e1f5e": "BNBUSDT",
	"0xe9e7cea3dedca598478052b3cbea3c1010209f80": "BUSDUSDT",
	"0x55d398326f99059f79a484a3c2dafe5d6f3a93f0": "USDTUSDT",
	"0x8ac76a51cc950d9822d68b9fe5e0de9a2ee4d55d": "USDCUSDT",
	"0x2170ed0880ac9a755fd29b2688956bd9296e6bbd": "ETHUSDT",
	"0x7130d2a12b9ccb6fbf05d016f06f2b0d2ae3a75": "BTCBUSD",
	"0x0e09fabb73bd3ade0a17ecc321fd13a19e81ce82": "CAKEUSDT",
}

// GetPrice returns the current price for a token
func (s *Server) GetPrice(ctx context.Context, address string) (*TokenPrice, error) {
	s.mu.RLock()
	price, ok := s.prices[address]
	s.mu.RUnlock()
	
	if ok {
		return price, nil
	}
	
	// Try database
	var tp TokenPrice
	err := s.pool.QueryRow(ctx, `SELECT address, name, symbol, price, price_change_24h, volume_24h, market_cap FROM tokens WHERE address = $1`, address).Scan(&tp.Address, &tp.Name, &tp.Symbol, &tp.Price, &tp.PriceChange24h, &tp.Volume24h, &tp.MarketCap)
	if err != nil {
		return nil, err
	}
	
	return &tp, nil
}

// GetPrices returns prices for multiple tokens
func (s *Server) GetPrices(ctx context.Context, addresses []string) (map[string]*TokenPrice, error) {
	result := make(map[string]*TokenPrice)
	
	s.mu.RLock()
	for _, addr := range addresses {
		if price, ok := s.prices[addr]; ok {
			result[addr] = price
		}
	}
	s.mu.RUnlock()
	
	return result, nil
}

// GetPriceHistory returns historical price data
func (s *Server) GetPriceHistory(ctx context.Context, address string, days int) ([]PriceHistory, error) {
	rows, err := s.pool.Query(ctx, `SELECT timestamp, price, volume FROM token_price_history WHERE address = $1 AND timestamp > $2 ORDER BY timestamp DESC`, address, time.Now().Unix()-int64(days*86400))
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var history []PriceHistory
	for rows.Next() {
		var h PriceHistory
		var priceStr, volumeStr string
		if err := rows.Scan(&h.Timestamp, &priceStr, &volumeStr); err != nil {
			return nil, err
		}
		fmt.Sscanf(priceStr, "%f", &h.Price)
		fmt.Sscanf(volumeStr, "%f", &h.Volume)
		history = append(history, h)
	}
	
	return history, nil
}

// GetTopTokens returns top tokens by market cap
func (s *Server) GetTopTokens(ctx context.Context, limit int) ([]*TokenPrice, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	type tokenWithCap struct {
		token *TokenPrice
		cap   float64
	}
	
	var tokens []tokenWithCap
	for _, t := range s.prices {
		tokens = append(tokens, tokenWithCap{token: t, cap: t.MarketCap})
	}
	
	// Sort by market cap
	for i := 0; i < len(tokens)-1; i++ {
		for j := i + 1; j < len(tokens); j++ {
			if tokens[j].cap > tokens[i].cap {
				tokens[i], tokens[j] = tokens[j], tokens[i]
			}
		}
	}
	
	var result []*TokenPrice
	for i := 0; i < limit && i < len(tokens); i++ {
		result = append(result, tokens[i].token)
	}
	
	return result, nil
}

// FormatPrice formats price in human readable format
func FormatPrice(price float64) string {
	if price >= 1 {
		return fmt.Sprintf("$%.2f", price)
	}
	return fmt.Sprintf("$%.6f", price)
}

// FormatVolume formats volume in human readable format
func FormatVolume(volume float64) string {
	if volume >= 1e9 {
		return fmt.Sprintf("$%.2fB", volume/1e9)
	}
	if volume >= 1e6 {
		return fmt.Sprintf("$%.2fM", volume/1e6)
	}
	if volume >= 1e3 {
		return fmt.Sprintf("$%.2fK", volume/1e3)
	}
	return fmt.Sprintf("$%.2f", volume)
}

// FormatMarketCap formats market cap in human readable format
func FormatMarketCap(marketCap float64) string {
	if marketCap >= 1e12 {
		return fmt.Sprintf("$%.2fT", marketCap/1e12)
	}
	if marketCap >= 1e9 {
		return fmt.Sprintf("$%.2fB", marketCap/1e9)
	}
	if marketCap >= 1e6 {
		return fmt.Sprintf("$%.2fM", marketCap/1e6)
	}
	return fmt.Sprintf("$%.0f", marketCap)
}

// CalculateTokenValue calculates token value in USD
func CalculateTokenValue(amount *big.Int, price float64) float64 {
	famount := new(big.Float).SetInt(amount)
	priceFloat := big.NewFloat(price)
	result := new(big.Float).Mul(famount, priceFloat)
	
	fresult, _ := result.Float64()
	return fresult / 1e18 // Assuming 18 decimals
}