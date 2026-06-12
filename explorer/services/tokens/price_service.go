// Package price provides token price service for TigerScan.
package price

import (
	"context"
	"encoding/json"
	"fmt"
	"math/big"
	"sync"
	"time"

	"github.com/tigersmartchain/tigersmartchain/explorer/databases/postgres"
)

// =============================================================================
// PRICE SERVICE
// =============================================================================

// Service provides token price tracking
type Service struct {
	db           *postgres.DB
	mu           sync.RWMutex
	prices       map[string]*PriceData
	updateTicker *time.Ticker
	priceFeeds   map[string]PriceFeed
}

// PriceData holds price information
type PriceData struct {
	TokenAddress string    `json:"tokenAddress"`
	PriceUSD    float64   `json:"priceUSD"`
	Change24h   float64   `json:"change24h"`
	Change7d    float64   `json:"change7d"`
	Volume24h   float64   `json:"volume24h"`
	MarketCap   float64   `json:"marketCap"`
	Supply      *big.Int `json:"supply"`
	Timestamp   time.Time `json:"timestamp"`
}

// PriceFeed represents a price feed source
type PriceFeed interface {
	GetPrice(address string) (*PriceData, error)
	GetPrices(addresses []string) (map[string]*PriceData, error)
}

// CoinGeckoFeed implements price feed using CoinGecko API
type CoinGeckoFeed struct {
	APIKey     string
	BaseURL    string
	HTTPClient *HTTPClient
}

// HTTPClient represents HTTP client
type HTTPClient struct {
	BaseURL string
	APIKey  string
}

// NewCoinGeckoFeed creates a new CoinGecko price feed
func NewCoinGeckoFeed(apiKey string) *CoinGeckoFeed {
	return &CoinGeckoFeed{
		APIKey:  apiKey,
		BaseURL: "https://api.coingecko.com/api/v3",
		HTTPClient: &HTTPClient{
			BaseURL: "https://api.coingecko.com/api/v3",
			APIKey:  apiKey,
		},
	}
}

// GetPrice returns price for a single token
func (f *CoinGeckoFeed) GetPrice(address string) (*PriceData, error) {
	// Implementation would call CoinGecko API
	// For now, return mock data
	return &PriceData{
		TokenAddress: address,
		PriceUSD:    0.0,
		Change24h:   0.0,
		Change7d:    0.0,
		Volume24h:   0.0,
		MarketCap:   0.0,
		Supply:      big.NewInt(0),
		Timestamp:   time.Now(),
	}, nil
}

// GetPrices returns prices for multiple tokens
func (f *CoinGeckoFeed) GetPrices(addresses []string) (map[string]*PriceData, error) {
	result := make(map[string]*PriceData)
	for _, addr := range addresses {
		price, err := f.GetPrice(addr)
		if err != nil {
			continue
		}
		result[addr] = price
	}
	return result, nil
}

// BinanceFeed implements price feed using Binance API
type BinanceFeed struct {
	BaseURL string
}

// NewBinanceFeed creates a new Binance price feed
func NewBinanceFeed() *BinanceFeed {
	return &BinanceFeed{
		BaseURL: "https://api.binance.com/api/v3",
	}
}

// GetPrice returns price from Binance
func (f *BinanceFeed) GetPrice(address string) (*PriceData, error) {
	// Would query Binance API for token price
	// For now, return mock
	return &PriceData{
		TokenAddress: address,
		PriceUSD:    0.0,
		Timestamp:   time.Now(),
	}, nil
}

// GetPrices returns prices from Binance
func (f *BinanceFeed) GetPrices(addresses []string) (map[string]*PriceData, error) {
	result := make(map[string]*PriceData)
	for _, addr := range addresses {
		price, _ := f.GetPrice(addr)
		result[addr] = price
	}
	return result, nil
}

// NewService creates a new price service
func NewService(db *postgres.DB) *Service {
	return &Service{
		db:         db,
		prices:     make(map[string]*PriceData),
		priceFeeds: make(map[string]PriceFeed),
	}
}

// RegisterPriceFeed registers a price feed
func (s *Service) RegisterPriceFeed(name string, feed PriceFeed) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.priceFeeds[name] = feed
}

// Start starts the price update ticker
func (s *Service) Start(ctx context.Context, interval time.Duration) {
	s.updateTicker = time.NewTicker(interval)
	go func() {
		for {
			select {
			case <-s.updateTicker.C:
				s.updatePrices(ctx)
			case <-ctx.Done():
				s.updateTicker.Stop()
				return
			}
		}
	}()
}

// Stop stops the price service
func (s *Service) Stop() {
	if s.updateTicker != nil {
		s.updateTicker.Stop()
	}
}

// =============================================================================
// PRICE OPERATIONS
// =============================================================================

// GetPrice returns price for a token
func (s *Service) GetPrice(ctx context.Context, address string) (*PriceData, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	// Check cache
	if price, ok := s.prices[address]; ok {
		if time.Since(price.Timestamp) < 5*time.Minute {
			return price, nil
		}
	}

	// Get from database
	token, err := s.db.GetToken(ctx, address)
	if err != nil {
		return nil, err
	}
	if token == nil {
		return nil, fmt.Errorf("token not found")
	}

	// Parse price
	priceUSD := 0.0
	if token.Price != "" {
		fmt.Sscanf(token.Price, "%f", &priceUSD)
	}

	change24h := 0.0
	if token.PriceChange24h != "" {
		fmt.Sscanf(token.PriceChange24h, "%f", &change24h)
	}

	volume24h := 0.0
	if token.Volume24h != "" {
		fmt.Sscanf(token.Volume24h, "%f", &volume24h)
	}

	marketCap := 0.0
	if token.MarketCap != "" {
		fmt.Sscanf(token.MarketCap, "%f", &marketCap)
	}

	supply := big.NewInt(0)
	if token.TotalSupply != "" {
		supply.SetString(token.TotalSupply, 10)
	}

	data := &PriceData{
		TokenAddress: address,
		PriceUSD:    priceUSD,
		Change24h:   change24h,
		Volume24h:   volume24h,
		MarketCap:   marketCap,
		Supply:      supply,
		Timestamp:   time.Now(),
	}

	// Update cache
	s.prices[address] = data

	return data, nil
}

// GetPrices returns prices for multiple tokens
func (s *Service) GetPrices(ctx context.Context, addresses []string) (map[string]*PriceData, error) {
	result := make(map[string]*PriceData)
	for _, addr := range addresses {
		price, err := s.GetPrice(ctx, addr)
		if err != nil {
			continue
		}
		result[addr] = price
	}
	return result, nil
}

// GetHistoricalPrice returns historical price data
func (s *Service) GetHistoricalPrice(ctx context.Context, address string, since time.Time) ([]*PriceData, error) {
	// Would query price history from database
	// For now, return current price
	current, err := s.GetPrice(ctx, address)
	if err != nil {
		return nil, err
	}
	return []*PriceData{current}, nil
}

// UpdatePrice updates price for a token
func (s *Service) UpdatePrice(ctx context.Context, address string, price *PriceData) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	s.prices[address] = price

	// Also update in database
	priceStr := fmt.Sprintf("%.18f", price.PriceUSD)
	volumeStr := fmt.Sprintf("%.18f", price.Volume24h)
	marketCapStr := fmt.Sprintf("%.18f", price.MarketCap)
	changeStr := fmt.Sprintf("%.2f", price.Change24h)

	token, err := s.db.GetToken(ctx, address)
	if err != nil || token == nil {
		return fmt.Errorf("token not found")
	}

	token.Price = &priceStr
	token.Volume24h = &volumeStr
	token.MarketCap = &marketCapStr
	token.PriceChange24h = &changeStr

	return s.db.InsertToken(ctx, token)
}

// =============================================================================
// PRICE AGGREGATION
// =============================================================================

// AggregatePrice aggregates prices from multiple feeds
func (s *Service) AggregatePrice(address string) (*PriceData, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	var prices []float64
	var volume24h float64
	var marketCap float64

	for name, feed := range s.priceFeeds {
		price, err := feed.GetPrice(address)
		if err != nil {
			continue
		}
		prices = append(prices, price.PriceUSD)
		volume24h += price.Volume24h
		marketCap += price.MarketCap
	}

	if len(prices) == 0 {
		return nil, fmt.Errorf("no price feeds available")
	}

	// Calculate average
	var sum float64
	for _, p := range prices {
		sum += p
	}
	avgPrice := sum / float64(len(prices))

	// Get change from first available feed
	feed, _ := s.priceFeeds["coingecko"]
	if feed == nil {
		for _, f := range s.priceFeeds {
			price, err := f.GetPrice(address)
			if err == nil {
				return &PriceData{
					TokenAddress: address,
					PriceUSD:    avgPrice,
					Change24h:   price.Change24h,
					Change7d:    price.Change7d,
					Volume24h:   volume24h,
					MarketCap:   marketCap,
					Timestamp:   time.Now(),
				}, nil
			}
		}
	}

	return &PriceData{
		TokenAddress: address,
		PriceUSD:    avgPrice,
		Volume24h:   volume24h,
		MarketCap:   marketCap,
		Timestamp:   time.Now(),
	}, nil
}

// =============================================================================
// MARKET DATA
// =============================================================================

// GetMarketData returns overall market data
func (s *Service) GetMarketData(ctx context.Context) (map[string]interface{}, error) {
	tokens, err := s.db.GetTokens(ctx, 1000, 0)
	if err != nil {
		return nil, err
	}

	totalMarketCap := 0.0
	totalVolume24h := 0.0
	tokenCount := 0

	for _, token := range tokens {
		if token.Price != "" {
			marketCap := 0.0
			if token.MarketCap != "" {
				fmt.Sscanf(token.MarketCap, "%f", &marketCap)
			}
			totalMarketCap += marketCap

			volume := 0.0
			if token.Volume24h != "" {
				fmt.Sscanf(token.Volume24h, "%f", &volume)
			}
			totalVolume24h += volume
			tokenCount++
		}
	}

	return map[string]interface{}{
		"totalMarketCap":    totalMarketCap,
		"totalVolume24h":   totalVolume24h,
		"tokenCount":       tokenCount,
		"stablecoinMarketCap": totalMarketCap * 0.1, // Estimate
	}, nil
}

// GetTopTokens returns top tokens by market cap
func (s *Service) GetTopTokens(ctx context.Context, limit int) ([]*PriceData, error) {
	tokens, err := s.db.GetTokens(ctx, limit, 0)
	if err != nil {
		return nil, err
	}

	result := make([]*PriceData, 0, len(tokens))
	for _, token := range tokens {
		price, err := s.GetPrice(ctx, token.Address)
		if err != nil {
			continue
		}
		result = append(result, price)
	}

	return result, nil
}

// =============================================================================
// PRICE ALERTS
// =============================================================================

// PriceAlert represents a price alert
type PriceAlert struct {
	ID            string    `json:"id"`
	TokenAddress string    `json:"tokenAddress"`
	Condition    string    `json:"condition"` // "above" or "below"
	TargetPrice float64   `json:"targetPrice"`
	NotifyURL   string    `json:"notifyURL"`
	Active      bool      `json:"active"`
	CreatedAt   time.Time `json:"createdAt"`
}

var priceAlerts = struct {
	mu     sync.RWMutex
	alerts map[string]*PriceAlert
}{
	alerts: make(map[string]*PriceAlert),
}

// CreateAlert creates a new price alert
func (s *Service) CreateAlert(alert *PriceAlert) error {
	priceAlerts.mu.Lock()
	defer priceAlerts.mu.Unlock()

	alert.ID = fmt.Sprintf("alert-%d", time.Now().UnixNano())
	alert.CreatedAt = time.Now()
	alert.Active = true

	priceAlerts.alerts[alert.ID] = alert
	return nil
}

// CheckAlerts checks and triggers price alerts
func (s *Service) CheckAlerts(ctx context.Context, address string, currentPrice float64) {
	priceAlerts.mu.RLock()
	defer priceAlerts.mu.RUnlock()

	for _, alert := range priceAlerts.alerts {
		if !alert.Active || alert.TokenAddress != address {
			continue
		}

		triggered := false
		switch alert.Condition {
		case "above":
			triggered = currentPrice >= alert.TargetPrice
		case "below":
			triggered = currentPrice <= alert.TargetPrice
		}

		if triggered {
			go s.triggerAlert(alert, currentPrice)
		}
	}
}

// triggerAlert triggers a price alert notification
func (s *Service) triggerAlert(alert *PriceAlert, currentPrice float64) {
	if alert.NotifyURL == "" {
		return
	}

	// Would send HTTP POST to notify URL
	data, _ := json.Marshal(map[string]interface{}{
		"alertId":      alert.ID,
		"tokenAddress": alert.TokenAddress,
		"currentPrice": currentPrice,
		"targetPrice": alert.TargetPrice,
		"condition":   alert.Condition,
	})
	fmt.Printf("Price alert triggered: %s\n", string(data))
}

// =============================================================================
// HELPERS
// =============================================================================

func (s *Service) updatePrices(ctx context.Context) {
	// Get all tokens with prices
	tokens, err := s.db.GetTokens(ctx, 100, 0)
	if err != nil {
		return
	}

	for _, token := range tokens {
		// Try to get price from feeds
		price, err := s.AggregatePrice(token.Address)
		if err != nil {
			continue
		}

		// Update in cache
		s.mu.Lock()
		s.prices[token.Address] = price
		s.mu.Unlock()

		// Update in database
		s.UpdatePrice(ctx, token.Address, price)

		// Check alerts
		s.CheckAlerts(ctx, token.Address, price.PriceUSD)
	}
}
