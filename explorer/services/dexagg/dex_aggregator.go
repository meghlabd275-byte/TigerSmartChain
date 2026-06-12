// Package dexagg provides DEX aggregation and price comparison services
// Real-time price aggregation from multiple DEXes for optimal trade execution
package dexagg

import (
	"encoding/json"
	"fmt"
	"math/big"
	"sort"
	"strings"
	"sync"
	"time"
)

// DEX represents a decentralized exchange
type DEX struct {
	Name     string `json:"name"`
	Address  string `json:"address"`
	ChainID  uint64 `json:"chainId"`
	Router  string `json:"router"`
	Factory string `json:"factory"`
}

// PoolInfo represents liquidity pool information
type PoolInfo struct {
	Address      string    `json:"address"`
	DEX          string    `json:"dex"`
	Token0       string    `json:"token0"`
	Token1       string    `json:"token1"`
	Reserve0     *big.Int `json:"reserve0"`
	Reserve1     *big.Int `json:"reserve1"`
	LiquidityUSD float64  `json:"liquidityUsd"`
	Fee         float64  `json:"fee"`
	Version     string   `json:"version"` // v2, v3
}

// TradeQuote represents a trade quote from a DEX
type TradeQuote struct {
	DEX           string   `json:"dex"`
	Path          []string `json:"path"`
	AmountIn      string   `json:"amountIn"`
	AmountOutMin  string   `json:"amountOutMin"`
	AmountOut     string   `json:"amountOut"`
	GasEstimate   uint64   `json:"gasEstimate"`
	GasCostUSD    float64  `json:"gasCostUsd"`
	PriceImpact   float64  `json:"priceImpact"`
	Slippage     float64  `json:"slippage"`
	Route        string   `json:"route"`
	Approved     bool     `json:"approved"`
}

// AggregatorService provides DEX aggregation
type AggregatorService struct {
	dexes        map[string]*DEX
	pools       map[string][]*PoolInfo
	priceCache  map[string]*PriceCache
	mu          sync.RWMutex
	httpClient  *HTTPClient
}

// PriceCache caches price data
type PriceCache struct {
	Price   float64   `json:"price"`
	Updated time.Time `json:"updated"`
}

// HTTPClient represents an HTTP client for API calls
type HTTPClient struct {
	baseURL string
	apiKey string
}

// NewAggregatorService creates a new DEX aggregator
func NewAggregatorService() *AggregatorService {
	return &AggregatorService{
		dexes:       initDEXes(),
		pools:       make(map[string][]*PoolInfo),
		priceCache:  make(map[string]*PriceCache),
		httpClient: &HTTPClient{},
	}
}

// initDEXes initializes supported DEXes
func initDEXes() map[string]*DEX {
	return map[string]*DEX{
		"uniswap_v3": {
			Name:     "Uniswap V3",
			Address:  "0xE592427A0AEce92De3E3411F660110D3918EEE28",
			ChainID:  1,
			Router:  "0xE592427A0AEce92De3E3411F660110D3918EEE28",
			Factory: "0x1F98431c8aD985320F61aA480528e1Dc08EdA31E",
		},
		"uniswap_v2": {
			Name:     "Uniswap V2",
			Address:  "0x7a250d5630B4cF539099dD8255d7f164bA127D2E0",
			ChainID:  1,
			Router:  "0x7a250d5630B4cF539099dD8255d7f164bA127D2E0",
			Factory: "0x5C69bEe701ef814a2B6CE3fb22e2D7F8eC0D3B8",
		},
		"sushiswap": {
			Name:     "SushiSwap",
			Address:  "0xd9e1ce17f264cfe78a2f0441de90e3f6acdcf61a",
			ChainID:  1,
			Router:  "0xd9e1ce17f264cfe78a2f0441de90e3f6acdcf61a",
			Factory: "0xC0AEe478e6818BF6E2C7B2E618549eDb4C5D6FaE8",
		},
		"curve": {
			Name:     "Curve",
			Address:  "0x99a584b8196252e0a15b5394d2e4e87e1d21e9a7",
			ChainID:  1,
			Router:  "0x99a584b8196252e0a15b5394d2e4e87e1d21e9a7",
			Factory: "0x90E00ACe2dF84C3d2ad2aE8a6f689E98F3F06E72",
		},
		"balancer": {
			Name:     "Balancer",
			Address:  "0xBA12222222228d8Ba445958a635a7a98CABc8B76",
			ChainID:  1,
			Router:  "0xBA12222222228d8Ba445958a635a7a98CABc8B76",
			Factory: "0xBA12222222228d8Ba445958a635a7a98CABc8B76",
		},
		"pancakeswap": {
			Name:     "PancakeSwap",
			Address:  "0x10ED43C718714eb63D5aA57B78B72304C824248",
			ChainID:  56,
			Router:  "0x10ED43C718714eb63D5aA57B78B72304C824248",
			Factory: "0x0eD7e52940161407D3F6A6E84B3D1B3C4b4Bb2b",
		},
		"spookyswap": {
			Name:     "SpookySwap",
			Address:  "0xF491e7B56E92adF0aFBABc0b7acA189a50e9487a",
			ChainID:  250,
			Router:  "0xF491e7B56E92adF0aFBABc0b7acA189a50e9487a",
			Factory: "0x152eE697a4B4Ba8F97cBAE59F3dF7bE8D5cA8b8F",
		},
		"quickswap": {
			Name:     "QuickSwap",
			Address:  "0xa25298fD6323A56f5D9f2Fe5B0d4FaF94F6d21F6",
			ChainID:  137,
			Router:  "0xa25298fD6323A56f5D9f2Fe5B0d4FaF94F6d21F6",
			Factory: "0x411B0c4aF86A2f45B4A22ac10C7E5C5C9B24c0F3",
		},
	}
}

// GetSupportedDEXes returns all supported DEXes
func (s *AggregatorService) GetSupportedDEXes() []*DEX {
	dexes := make([]*DEX, 0, len(s.dexes))
	for _, dex := range s.dexes {
		dexes = append(dexes, dex)
	}
	return dexes
}

// GetPoolInfo gets pool information for a token pair
func (s *AggregatorService) GetPoolInfo(tokenA, tokenB string) ([]*PoolInfo, error) {
	key := fmt.Sprintf("%s_%s", strings.ToLower(tokenA), strings.ToLower(tokenB))
	
	s.mu.RLock()
	pools, ok := s.pools[key]
	s.mu.RUnlock()
	
	if !ok {
		// Would fetch from blockchain
		return []*PoolInfo{}, nil
	}
	
	// Sort by liquidity
	sorted := make([]*PoolInfo, len(pools))
	copy(sorted, pools)
	sort.Slice(sorted, func(i, j int) bool {
		return sorted[i].LiquidityUSD > sorted[j].LiquidityUSD
	})
	
	return sorted, nil
}

// GetBestQuote gets the best trade quote for a swap
func (s *AggregatorService) GetBestQuote(tokenIn, tokenOut, amountIn string, options *QuoteOptions) (*TradeQuote, error) {
	if options == nil {
		options = &QuoteOptions{}
	}
	
	amount := parseAmount(amountIn)
	if amount == nil || amount.Sign() <= 0 {
		return nil, fmt.Errorf("invalid amount")
	}
	
	// Get all pools for this pair
	pools, err := s.GetPoolInfo(tokenIn, tokenOut)
	if err != nil {
		return nil, err
	}
	
	if len(pools) == 0 {
		return nil, fmt.Errorf("no pools found")
	}
	
	// Get quote from best pool
	bestQuote := s.calculateQuote(pools[0], amount, options)
	
	// Check for better multi-hop quote
	multiHopQuote, err := s.getMultiHopQuote(tokenIn, tokenOut, amount, options)
	if err == nil && multiHopQuote != nil {
		if new(big.Float).SetInt(multiHopQuote.AmountOut()).Cmp(big.NewFloat(0).SetInt(bestQuote.AmountOut())) > 0 {
			return multiHopQuote, nil
		}
	}
	
	return bestQuote, nil
}

// QuoteOptions represents quote request options
type QuoteOptions struct {
	MaxSlippage float64 `json:"maxSlippage"`
	MaxGas    uint64 `json:"maxGas"`
	DEXFilter []string `json:"dexFilter"`
}

// AmountOut returns the amount out as big.Int
func (q *TradeQuote) AmountOut() *big.Int {
	amount, _ := new(big.Int).SetString(q.AmountOut, 10)
	return amount
}

// calculateQuote calculates a quote from a pool
func (s *AggregatorService) calculateQuote(pool *PoolInfo, amount *big.Int, options *QuoteOptions) *TradeQuote {
	// Constant product: amountOut = (amountIn * reserve1) / (reserve0 + amountIn)
	one := big.NewInt(1)
	
	// Apply fee (0.3% default)
	fee := big.NewFloat(0.997)
	amountWithFee := new(big.Float).SetInt(amount)
	amountWithFee.Mul(amountWithFee, fee)
	
	reserve0 := new(big.Float).SetInt(pool.Reserve0)
	reserve1 := new(big.Float).SetInt(pool.Reserve1)
	
	// amountOut = (amountIn * reserve1) / (reserve0 + amountIn)
	denom := new(big.Float).SetInt(amountWithFee.Int())
	denom.Add(denom, reserve0)
	
	amountOut := new(big.Float).SetInt(reserve1)
	amountOut.Mul(amountOut, amountWithFee)
	amountOut.Quo(amountOut, denom)
	
	// Calculate price impact
	priceImpact := calculatePriceImpact(amount, pool.Reserve0, pool.Reserve1)
	
	return &TradeQuote{
		DEX:          pool.DEX,
		Path:        []string{pool.Token0, pool.Token1},
		AmountIn:    amount.String(),
		AmountOut:   amountOut.Text('f', 0),
		AmountOutMin: amountOut.Text('f', 0),
		GasEstimate: 150000,
		PriceImpact: priceImpact,
		Route:      fmt.Sprintf("%s > %s", pool.Token0, pool.Token1),
	}
}

// getMultiHopQuote gets a multi-hop quote
func (s *AggregatorService) getMultiHopQuote(tokenIn, tokenOut string, amount *big.Int, options *QuoteOptions) (*TradeQuote, error) {
	// Find intermediate tokens
	intermediates := s.findIntermediatePools(tokenIn, tokenOut)
	
	if len(intermediates) == 0 {
		return nil, fmt.Errorf("no multi-hop route found")
	}
	
	// Calculate multi-hop
	hop0, err := s.GetBestQuote(tokenIn, intermediates[0], amount.String(), options)
	if err != nil {
		return nil, err
	}
	
	hop1, err := s.GetBestQuote(intermediates[0], tokenOut, hop0.AmountOut, options)
	if err != nil {
		return nil, err
	}
	
	totalGas := hop0.GasEstimate + hop1.GasEstimate
	
	return &TradeQuote{
		DEX:          "multi_hop",
		Path:        []string{tokenIn, intermediates[0], tokenOut},
		AmountIn:    amount.String(),
		AmountOut:   hop1.AmountOut,
		AmountOutMin: hop1.AmountOutMin,
		GasEstimate: totalGas,
		PriceImpact: hop0.PriceImpact + hop1.PriceImpact,
		Route:      fmt.Sprintf("%s > %s > %s", tokenIn, intermediates[0], tokenOut),
	}, nil
}

// findIntermediatePools finds intermediate token pairs
func (s *AggregatorService) findIntermediatePools(tokenA, tokenB string) []string {
	// Common intermediates: USDC, USDT, WETH, DAI
	intermediates := []string{
		"0xA0b86991c6218b42c683a8939B725AED76A24ac63", // USDC
		"0xdAC17F958D2ee523a2206206994597C13D831ec7", // USDT
		"0xC02aaA39b223FE8D0A0e5C4F27eAD9083C3Cc51E4", // WETH
		"0x6B175474E89094C44Da98b954EedeAC495271d0F", // DAI
	}
	return intermediates
}

// calculatePriceImpact calculates price impact
func calculatePriceImpact(amount, reserve0, reserve1 *big.Int) float64 {
	inputRatio := new(big.Float).SetInt(amount)
	inputRatio.Quo(inputRatio, new(big.Float).SetInt(reserve0))
	
	impact, _ := inputRatio.Float64()
	return impact * 100
}

// parseAmount parses amount string to big.Int
func parseAmount(amountStr string) *big.Int {
	amountStr = strings.TrimPrefix(amountStr, "0x")
	
	// Check if wei (no decimal) or ether
	amount, ok := new(big.Int).SetString(amountStr, 10)
	if ok {
		return amount
	}
	
	// Try parsing as float (ether)
	var f big.Float
	f.SetString(amountStr)
	
	// Convert to wei (18 decimals)
	amount = new(big.Int)
	f.Mul(&f, big.NewFloat(1e18))
	
	return amount
}

// GetAllQuotes gets quotes from all DEXes for comparison
func (s *AggregatorService) GetAllQuotes(tokenIn, tokenOut, amountIn string) ([]*TradeQuote, error) {
	pools, err := s.GetPoolInfo(tokenIn, tokenOut)
	if err != nil {
		return nil, err
	}
	
	amount := parseAmount(amountIn)
	if amount == nil {
		return nil, fmt.Errorf("invalid amount")
	}
	
	quotes := make([]*TradeQuote, 0, len(pools))
	for _, pool := range pools {
		quote := s.calculateQuote(pool, amount, &QuoteOptions{})
		quotes = append(quotes, quote)
	}
	
	// Sort by best output
	sort.Slice(quotes, func(i, j int) bool {
		a, _ := new(big.Int).SetString(quotes[i].AmountOut, 10)
		b, _ := new(big.Int).SetString(quotes[j].AmountOut, 10)
		return a.Cmp(b) > 0
	})
	
	return quotes, nil
}

// GetPrice gets current price for a token pair
func (s *AggregatorService) GetPrice(tokenA, tokenB string) (float64, error) {
	cacheKey := fmt.Sprintf("%s_%s", tokenA, tokenB)
	
	s.mu.RLock()
	if cached, ok := s.priceCache[cacheKey]; ok {
		if time.Since(cached.Updated) < 30*time.Second {
			s.mu.RUnlock()
			return cached.Price, nil
		}
	}
	s.mu.RUnlock()
	
	// Fetch fresh price
	pools, err := s.GetPoolInfo(tokenA, tokenB)
	if err != nil || len(pools) == 0 {
		return 0, fmt.Errorf("no price data")
	}
	
	price := calculatePrice(pools[0])
	
	s.mu.Lock()
	s.priceCache[cacheKey] = &PriceCache{
		Price:   price,
		Updated: time.Now(),
	}
	s.mu.Unlock()
	
	return price, nil
}

// calculatePrice calculates price from pool
func calculatePrice(pool *PoolInfo) float64 {
	if pool.Reserve0.Sign() == 0 {
		return 0
	}
	
	price := new(big.Float).SetInt(pool.Reserve1)
	price.Quo(price, new(big.Float).SetInt(pool.Reserve0))
	
	p, _ := price.Float64()
	return p
}

// AddPool adds a pool to tracking
func (s *AggregatorService) AddPool(pool *PoolInfo) error {
	if pool == nil {
		return fmt.Errorf("nil pool")
	}
	
	key := fmt.Sprintf("%s_%s", strings.ToLower(pool.Token0), strings.ToLower(pool.Token1))
	
	s.mu.Lock()
	s.pools[key] = append(s.pools[key], pool)
	s.mu.Unlock()
	
	return nil
}

// GetStats gets aggregator statistics
func (s *AggregatorService) GetStats() (*AggregatorStats, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	totalPools := 0
	for _, pools := range s.pools {
		totalPools += len(pools)
	}
	
	return &AggregatorStats{
		SupportedDEXes: len(s.dexes),
		TotalPools:   totalPools,
		ChainIDs:    []uint64{1, 56, 137, 250},
	}, nil
}

// AggregatorStats represents aggregator statistics
type AggregatorStats struct {
	SupportedDEXes int      `json:"supportedDEXes"`
	TotalPools    int       `json:"totalPools"`
	ChainIDs     []uint64  `json:"chainIds"`
}

// JSON serializes to JSON
func (q *TradeQuote) JSON() (string, error) {
	data, err := json.Marshal(q)
	return string(data), err
}

// InitAggregatorService initializes the service
func InitAggregatorService() (*AggregatorService, error) {
	return NewAggregatorService(), nil
}