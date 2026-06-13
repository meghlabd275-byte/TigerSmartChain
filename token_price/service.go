// Token Price Service - Real-time price feeds from CoinGecko
package main

import (
	"encoding/json"
	"fmt"
	"net/http"
	"sync"
	"time"
)

// TokenPrice represents token price data
type TokenPrice struct {
	Address        string  `json:"address"`
	Symbol        string  `json:"symbol"`
	Name          string  `json:"name"`
	Price         float64 `json:"price"`
	PriceChange24h float64 `json:"price_change_24h"`
	Volume24h     float64 `json:"volume_24h"`
	MarketCap    float64 `json:"market_cap"`
	Timestamp    int64   `json:"timestamp"`
}

// PriceService manages token prices
type PriceService struct {
	mu          sync.RWMutex
	prices      map[string]*TokenPrice
	httpClient  *http.Client
	coingeckoURL string
	cacheTTL   time.Duration
}

// NewPriceService creates a new price service
func NewPriceService() *PriceService {
	return &PriceService{
		prices:      make(map[string]*TokenPrice),
		httpClient: &http.Client{Timeout: 10 * time.Second},
		coingeckoURL: "https://api.coingecko.com/api/v3",
		cacheTTL:   5 * time.Minute,
	}
}

// GetPrice retrieves price for a token address
func (s *PriceService) GetPrice(address string) (*TokenPrice, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	price, exists := s.prices[address]
	if !exists {
		return nil, false
	}
	
	// Check if cache is expired
	if time.Now().Unix()-price.Timestamp > int64(s.cacheTTL.Seconds()) {
		return nil, false
	}
	
	return price, true
}

// FetchPrice fetches price from CoinGecko
func (s *PriceService) FetchPrice(address string) (*TokenPrice, error) {
	// Convert address to CoinGecko ID (simplified - would need proper mapping)
	url := fmt.Sprintf("%s/simple/token_price?contract_addresses=%s&vs_currencies=usd", s.coingeckoURL, address)
	
	resp, err := s.httpClient.Get(url)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("CoinGecko API error: %d", resp.StatusCode)
	}
	
	var result map[string]map[string]float64
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}
	
	priceData, exists := result[address]
	if !exists {
		return nil, fmt.Errorf("token not found: %s", address)
	}
	
	price, ok := priceData["usd"]
	if !ok {
		return nil, fmt.Errorf("no USD price for token: %s", address)
	}
	
	tokenPrice := &TokenPrice{
		Address:     address,
		Price:      price,
		Timestamp:  time.Now().Unix(),
	}
	
	// Update cache
	s.mu.Lock()
	s.prices[address] = tokenPrice
	s.mu.Unlock()
	
	return tokenPrice, nil
}

// FetchMultiplePrices fetches prices for multiple tokens
func (s *PriceService) FetchMultiplePrices(addresses []string) (map[string]*TokenPrice, error) {
	result := make(map[string]*TokenPrice)
	
	// Convert addresses to comma-separated string
	addrStr := ""
	for i, addr := range addresses {
		if i > 0 {
			addrStr += ","
		}
		addrStr += addr
	}
	
	url := fmt.Sprintf("%s/simple/token_price?contract_addresses=%s&vs_currencies=usd&include_24hr_change=true&include_24hr_vol=true&include_market_cap=true", 
		s.coingeckoURL, addrStr)
	
	resp, err := s.httpClient.Get(url)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("CoinGecko API error: %d", resp.StatusCode)
	}
	
	var priceData map[string]map[string]float64
	if err := json.NewDecoder(resp.Body).Decode(&priceData); err != nil {
		return nil, err
	}
	
	for address, data := range priceData {
		price := data["usd"]
		change24h := data["usd_24h_change"]
		volume := data["usd_24h_vol"]
		marketCap := data["usd_market_cap"]
		
		tokenPrice := &TokenPrice{
			Address:        address,
			Price:         price,
			PriceChange24h: change24h,
			Volume24h:     volume,
			MarketCap:    marketCap,
			Timestamp:    time.Now().Unix(),
		}
		
		result[address] = tokenPrice
		
		// Update cache
		s.mu.Lock()
		s.prices[address] = tokenPrice
		s.mu.Unlock()
	}
	
	return result, nil
}

// GetCachedPrices returns all cached prices
func (s *PriceService) GetCachedPrices() map[string]*TokenPrice {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	result := make(map[string]*TokenPrice)
	for k, v := range s.prices {
		result[k] = v
	}
	
	return result
}

func main() {
	service := NewPriceService()
	
	// Example: Fetch prices for some popular tokens
	addresses := []string{
		"0x2260fac5e5542a760e1e3b8c8b95e7deda2a8bb9a5", // WBTC
		"0xc00e94cb662c3520282ead6c89d6d5e2e2be9d0e3", // COMP
		"0x514910771af9ca656af8408ffea2b475a5b5a6823", // LINK
	}
	
	prices, err := service.FetchMultiplePrices(addresses)
	if err != nil {
		fmt.Printf("Error fetching prices: %v\n", err)
		return
	}
	
	for address, price := range prices {
		fmt.Printf("Token: %s\n", address)
		fmt.Printf("  Price: $%.2f\n", price.Price)
		fmt.Printf("  24h Change: %.2f%%\n", price.PriceChange24h)
		fmt.Printf("  Volume: $%.2f\n", price.Volume24h)
		fmt.Printf("  Market Cap: $%.2f\n", price.MarketCap)
	}
}