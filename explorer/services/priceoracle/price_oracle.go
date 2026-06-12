// Package priceoracle provides real-time price oracle services
package priceoracle

import (
	"fmt"
	"math/big"
	"sort"
	"sync"
	"time"
)

// PriceData represents price data from a source
type PriceData struct {
	Symbol     string    `json:"symbol"`
	Price      float64   `json:"price"`
	Change24h   float64   `json:"change24h"`
	Volume24h  float64   `json:"volume24h"`
	MarketCap  float64   `json:"marketCap"`
	Source     string    `json:"source"`
	UpdatedAt  time.Time `json:"updatedAt"`
}

// PriceOracle provides price data aggregation
type PriceOracle struct {
	sources   map[string]PriceSource
	prices    map[string]*PriceData
	mu        sync.RWMutex
	cacheTTL  time.Duration
}

// PriceSource represents a price data source
type PriceSource struct {
	Name     string
	Endpoint string
	Priority int
	Active   bool
}

// NewPriceOracle creates a new price oracle
func NewPriceOracle() *PriceOracle {
	return &PriceOracle{
		sources:  initSources(),
		prices:   make(map[string]*PriceData),
		cacheTTL: 30 * time.Second,
	}
}

// initSources initializes price sources
func initSources() map[string]PriceSource {
	return map[string]PriceSource{
		"coinbase": {
			Name:     "Coinbase",
			Endpoint: "https://api.coinbase.com",
			Priority: 1,
			Active:   true,
		},
		"binance": {
			Name:     "Binance",
			Endpoint: "https://api.binance.com",
			Priority: 2,
			Active:   true,
		},
		"kraken": {
			Name:     "Kraken",
			Endpoint: "https://api.kraken.com",
			Priority: 3,
			Active:   true,
		},
		"coingecko": {
			Name:     "CoinGecko",
			Endpoint: "https://api.coingecko.com",
			Priority: 4,
			Active:   true,
		},
		"defillama": {
			Name:     "DeFiLlama",
			Endpoint: "https://api.llama.fi",
			Priority: 5,
			Active:   true,
		},
	}
}

// GetPrice gets price for a token
func (p *PriceOracle) GetPrice(symbol string) (*PriceData, error) {
	symbol = normalizeSymbol(symbol)
	
	p.mu.RLock()
	price, ok := p.prices[symbol]
	p.mu.RUnlock()
	
	if ok && time.Since(price.UpdatedAt) < p.cacheTTL {
		return price, nil
	}
	
	// Fetch fresh price
	newPrice, err := p.fetchPrice(symbol)
	if err != nil {
		if ok {
			return price, nil // Return stale data
		}
		return nil, err
	}
	
	p.mu.Lock()
	p.prices[symbol] = newPrice
	p.mu.Unlock()
	
	return newPrice, nil
}

// fetchPrice fetches price from sources
func (p *PriceOracle) fetchPrice(symbol string) (*PriceData, error) {
	// Get sources sorted by priority
	sources := make([]PriceSource, 0)
	for _, s := range p.sources {
		if s.Active {
			sources = append(sources, s)
		}
	}
	sort.Slice(sources, func(i, j int) bool {
		return sources[i].Priority < sources[j].Priority
	})
	
	// Try each source
	for _, source := range sources {
		price, err := p.fetchFromSource(symbol, source.Name)
		if err == nil && price != nil {
			price.Source = source.Name
			return price, nil
		}
	}
	
	return nil, fmt.Errorf("no price data available")
}

// fetchFromSource fetches from a specific source
func (p *PriceOracle) fetchFromSource(symbol, source string) (*PriceData, error) {
	// In production, would make HTTP request
	// For now, return mock data
	return &PriceData{
		Symbol:    symbol,
		Price:    0.0,
		Change24h: 0.0,
		Source:   source,
		UpdatedAt: time.Now(),
	}, nil
}

// GetPrices gets prices for multiple tokens
func (p *PriceOracle) GetPrices(symbols []string) (map[string]*PriceData, error) {
	result := make(map[string]*PriceData)
	
	for _, symbol := range symbols {
		price, err := p.GetPrice(symbol)
		if err != nil {
			continue
		}
		result[symbol] = price
	}
	
	return result, nil
}

// GetHistoricalPrice gets historical price
func (p *PriceOracle) GetHistoricalPrice(symbol string, timestamp time.Time) (*PriceData, error) {
	// In production, would query historical data
	return &PriceData{
		Symbol:    symbol,
		Price:    0.0,
		UpdatedAt: timestamp,
	}, nil
}

// GetPriceRange gets price range over time
func (p *PriceOracle) GetPriceRange(symbol string, start, end time.Time) ([]*PricePoint, error) {
	// In production, would query historical database
	return []*PricePoint{}, nil
}

// PricePoint represents a price point
type PricePoint struct {
	Timestamp time.Time `json:"timestamp"`
	Price    float64   `json:"price"`
	Volume   float64   `json:"volume"`
}

// GetGasPrice gets gas price for a chain
func (p *PriceOracle) GetGasPrice(chainID uint64) (*GasPrice, error) {
	// In production, would fetch from multiple sources
	return &GasPrice{
		ChainID:   chainID,
		Safe:     20,
		Standard: 30,
		Fast:     50,
		Updated:  time.Now(),
	}, nil
}

// GasPrice represents gas price recommendations
type GasPrice struct {
	ChainID   uint64    `json:"chainId"`
	Safe     float64  `json:"safe"`
	Standard float64  `json:"standard"`
	Fast     float64  `json:"fast"`
	Updated  time.Time `json:"updated"`
}

// GetTokenPrice gets token price with fallback
func (p *PriceOracle) GetTokenPrice(token, vsToken string) (float64, error) {
	if vsToken == "" {
		vsToken = "USD"
	}
	
	tokenPrice, err := p.GetPrice(token)
	if err != nil {
		return 0, err
	}
	
	vsPrice, err := p.GetPrice(vsToken)
	if err != nil {
		return tokenPrice.Price, nil
	}
	
	// Convert to vsToken
	if vsToken != "USD" {
		return tokenPrice.Price / vsPrice.Price, nil
	}
	
	return tokenPrice.Price, nil
}

// normalizeSymbol normalizes token symbol
func normalizeSymbol(symbol string) string {
	symbols := map[string]string{
		"ETH": "ETH",
		"WETH": "ETH",
		"BNB": "BNB",
		"MATIC": "MATIC",
		"USDC": "USDC",
		"USDT": "USDT",
		"DAI": "DAI",
		"WBTC": "WBTC",
		"BTC": "BTC",
	}
	
	normalized, ok := symbols[symbol]
	if ok {
		return normalized
	}
	
	return symbol
}

// AddPriceSource adds a price source
func (p *PriceOracle) AddPriceSource(source PriceSource) error {
	if source.Name == "" {
		return fmt.Errorf("source name required")
	}
	
	p.mu.Lock()
	p.sources[source.Name] = source
	p.mu.Unlock()
	
	return nil
}

// GetSources gets all price sources
func (p *PriceOracle) GetSources() []PriceSource {
	p.mu.RLock()
	defer p.mu.RUnlock()
	
	result := make([]PriceSource, 0, len(p.sources))
	for _, s := range p.sources {
		result = append(result, s)
	}
	
	sort.Slice(result, func(i, j int) bool {
		return result[i].Priority < result[j].Priority
	})
	
	return result
}

// SetCacheTTL sets cache TTL
func (p *PriceOracle) SetCacheTTL(ttl time.Duration) {
	p.mu.Lock()
	p.cacheTTL = ttl
	p.mu.Unlock()
}

// GetDEXPrice gets price from DEX
func (p *PriceOracle) GetDEXPrice(tokenA, tokenB string, amount *big.Int) (*DEXPrice, error) {
	priceA, err := p.GetPrice(tokenA)
	if err != nil {
		return nil, err
	}
	
	priceB, err := p.GetPrice(tokenB)
	if err != nil {
		return nil, err
	}
	
	rate := priceA.Price / priceB.Price
	
	return &DEXPrice{
		TokenA: tokenA,
		TokenB: tokenB,
		Rate:   rate,
		Amount: amount.String(),
	}, nil
}

// DEXPrice represents DEX price
type DEXPrice struct {
	TokenA string  `json:"tokenA"`
	TokenB string  `json:"tokenB"`
	Rate  float64 `json:"rate"`
	Amount string `json:"amount"`
}

// GetAllPrices gets all tracked prices
func (p *PriceOracle) GetAllPrices() (map[string]*PriceData, error) {
	p.mu.RLock()
	defer p.mu.RUnlock()
	
	result := make(map[string]*PriceData)
	for k, v := range p.prices {
		result[k] = v
	}
	
	return result, nil
}

// GetMarketData gets market data for a token
func (p *PriceOracle) GetMarketData(symbol string) (*MarketData, error) {
	price, err := p.GetPrice(symbol)
	if err != nil {
		return nil, err
	}
	
	return &MarketData{
		Symbol:    price.Symbol,
		Price:     price.Price,
		Change24h: price.Change24h,
		Volume24h: price.Volume24h,
		MarketCap: price.MarketCap,
	}, nil
}

// MarketData represents market data
type MarketData struct {
	Symbol    string  `json:"symbol"`
	Price    float64 `json:"price"`
	Change24h float64 `json:"change24h"`
	Volume24h float64 `json:"volume24h"`
	MarketCap float64 `json:"marketCap"`
}

// InitPriceOracle initializes the service
func InitPriceOracle() (*PriceOracle, error) {
	return NewPriceOracle(), nil
}