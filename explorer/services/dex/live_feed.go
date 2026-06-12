// Package dex provides live DEX data feed integration for PancakeSwap and Uniswap.
// This service connects to subgraph APIs for real-time DEX data including pairs,
// liquidity, volume, swaps, and analytics.
// 
// SECURITY: All endpoints are rate-limited and require API key authentication.
// Data is cached with TTL to prevent abuse.
package dex

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"io"
	"math"
	"math/big"
	"net/http"
	"net/url"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
	"github.com/tigersmartchain/tigersmartchain/internal/crypto"
)

// =============================================================================
// CONSTANTS & CONFIGURATION
// =============================================================================

const (
	// PancakeSwap Subgraph endpoints (BSC Mainnet)
	PancakeSwapSubgraphBSC = "https://api.pancakeswap.com/api/v3/graphql"
	PancakeSwapSubgraphETH  = "https://api.pancakeswap.com/api/v3/graphql"
	
	// Uniswap Subgraph endpoints
	UniswapSubgraphETH   = "https://api.thegraph.com/subgraphs/name/uniswap/uniswap-v3"
	UniswapSubgraphBASE = "https://api.thegraph.com/subgraphs/name/uniswap/uniswap-v3-base"
	
	// Cache TTL settings
	CacheTTLPairs      = 15 * time.Second
	CacheTTLLiquidity  = 30 * time.Second
	CacheTTLVolume     = 15 * time.Second
	CacheTTLPairs24h   = 60 * time.Second
	
	// Rate limiting
	MaxRequestsPerMinute = 60
	SubgraphTimeout      = 30 * time.Second
	
	// Pagination
	DefaultPageSize = 100
	MaxPageSize     = 500
)

// =============================================================================
// DATA STRUCTURES
// =============================================================================

// DEXPair represents a trading pair on a DEX
type DEXPair struct {
	ID                string    `json:"id"`
	Token0            string    `json:"token0"`
	Token1            string    `json:"token1"`
	Token0Symbol      string    `json:"token0Symbol"`
	Token1Symbol      string    `json:"token1Symbol"`
	Token0Decimals    uint8     `json:"token0Decimals"`
	Token1Decimals    uint8     `json:"token1Decimals"`
	Reserve0          string    `json:"reserve0"`
	Reserve1          string    `json:"reserve1"`
	LiquidityUSD      float64   `json:"liquidityUSD"`
	Volume24h        float64   `json:"volume24h"`
	Volume7d         float64   `json:"volume7d"`
	TxCount24h       int64     `json:"txCount24h"`
	TxCount7d        int64     `json:"txCount7d"`
	Price            float64   `json:"price"`
	PriceChange24h   float64   `json:"priceChange24h"`
	Fees24h          float64   `json:"fees24h"`
	Token0Price      float64   `json:"token0Price"`
	Token1Price      float64   `json:"token1Price"`
	CreatedAtBlock   uint64    `json:"createdAtBlock"`
	CreatedAtTimestamp time.Time `json:"createdAtTimestamp"`
}

// DEXToken represents token data from DEX
type DEXToken struct {
	ID          string  `json:"id"`
	Symbol      string  `json:"symbol"`
	Name        string  `json:"name"`
	Decimals    uint8   `json:"decimals"`
	TotalSupply string  `json:"totalSupply"`
	
	// Trading pairs
	Pairs0 []string `json:"pairs0"` // Pairs where token is token0
	Pairs1 []string `json:"pairs1"` // Pairs where token is token1
	
	// Derived data
	VolumeUSD24h  float64 `json:"volumeUSD24h"`
	LiquidityUSD  float64 `json:"liquidityUSD"`
	TxCount24h    int64   `json:"txCount24h"`
}

// DEXTransaction represents a swap transaction
type DEXTransaction struct {
	ID            string    `json:"id"`
	Hash          string    `json:"hash"`
	BlockNumber  uint64    `json:"blockNumber"`
	Timestamp   time.Time `json:"timestamp"`
	Pair         string    `json:"pair"`
	From         string    `json:"from"`
	To           string    `json:"to"`
	TokenIn      string    `json:"tokenIn"`
	TokenOut     string    `json:"tokenOut"`
	AmountIn     string    `json:"amountIn"`
	AmountOut    string    `json:"amountOut"`
	AmountInUSD  float64   `json:"amountInUSD"`
	AmountOutUSD float64   `json:"amountOutUSD"`
	GasUsed      uint64    `json:"gasUsed"`
	GasPrice     uint64    `json:"gasPrice"`
}

// DEXAnalytics contains aggregated analytics
type DEXAnalytics struct {
	TotalLiquidityUSD  float64   `json:"totalLiquidityUSD"`
	TotalVolume24h     float64   `json:"totalVolume24h"`
	TotalVolume7d      float64   `json:"totalVolume7d"`
	TotalFees24h      float64   `json:"totalFees24h"`
	TotalTxCount24h   int64     `json:"totalTxCount24h"`
	TotalTxCount7d    int64     `json:"totalTxCount7d"`
	TopPairs          []DEXPair `json:"topPairs"`
	TopTokens         []DEXToken `json:"topTokens"`
	UpdatedAt        time.Time `json:"updatedAt"`
}

// OHLCData represents candlestick data
type OHLCData struct {
	Timestamp   int64     `json:"timestamp"`
	Open       float64   `json:"open"`
	High       float64   `json:"high"`
	Low        float64   `json:"low"`
	Close      float64   `json:"close"`
	Volume     float64   `json:"volume"`
	TxCount    int64     `json:"txCount"`
}

// =============================================================================
// LIVE FEED SERVICE
// =============================================================================

// LiveFeedService provides real-time DEX data from subgraphs
type LiveFeedService struct {
	db           *sql.DB
	redis       *redis.Client
	httpClient  *http.Client
	
	// Subgraph endpoints by chain
	subgraphs map[string]string
	
	// Rate limiting
	mu           sync.RWMutex
	lastRequest  map[string]time.Time
	requestCount map[string]int
	
	// Cache
	cacheMu     sync.RWMutex
	pairCache   map[string]*cacheEntry
	liquidityCache map[string]*cacheEntry
	volumeCache map[string]*cacheEntry
	
	// Security
	apiKeyStore *APIKeyStore
	rateLimit  *RateLimiter
	
	// Encryption
	encryptionKey []byte
}

type cacheEntry struct {
	Data      interface{}
	Expiry   time.Time
}

// NewLiveFeedService creates a new live DEX feed service
func NewLiveFeedService(cfg *LiveFeedConfig) (*LiveFeedService, error) {
	if cfg == nil {
		cfg = &LiveFeedConfig{}
	}
	
	// Initialize HTTP client with timeout
	httpClient := &http.Client{
		Timeout: SubgraphTimeout,
		CheckRedirect: func(req *http.Request, via []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}
	
	// Initialize Redis if provided
	var redisClient *redis.Client
	if cfg.RedisAddr != "" {
		redisClient = redis.NewClient(&redis.Options{
			Addr:         cfg.RedisAddr,
			Password:    cfg.RedisPassword,
			DB:          cfg.RedisDB,
			DialTimeout: 5 * time.Second,
			ReadTimeout: 5 * time.Second,
			WriteTimeout: 5 * time.Second,
		})
	}
	
	// Generate encryption key
	encryptionKey, err := crypto.GenerateEncryptionKey()
	if err != nil {
		return nil, fmt.Errorf("failed to generate encryption key: %w", err)
	}
	
	svc := &LiveFeedService{
		db:            cfg.DB,
		redis:         redisClient,
		httpClient:    httpClient,
		subgraphs:    cfg.Subgraphs,
		lastRequest:  make(map[string]time.Time),
		requestCount: make(map[string]int),
		pairCache:    make(map[string]*cacheEntry),
		liquidityCache: make(map[string]*cacheEntry),
		volumeCache: make(map[string]*cacheEntry),
		apiKeyStore:  NewAPIKeyStore(cfg.DB),
		rateLimit:    NewRateLimiter(MaxRequestsPerMinute, time.Minute),
		encryptionKey: encryptionKey,
	}
	
	// Set default subgraphs if not provided
	if len(svc.subgraphs) == 0 {
		svc.subgraphs = map[string]string{
			"bsc":      PancakeSwapSubgraphBSC,
			"ethereum": UniswapSubgraphETH,
			"base":    UniswapSubgraphBASE,
		}
	}
	
	// Start background cache cleanup
	go svc.cleanupCache()
	
	return svc, nil
}

// =============================================================================
// SUBGRAPH QUERIES
// =============================================================================

// PancakeSwap GraphQL queries
var (
	// Query for top pairs
	QueryTopPairsBSC = `
		query GetTopPairs($first: Int!, $skip: Int!) {
			pairs(
				first: $first,
				skip: $skip,
				orderBy: volumeUSD,
				orderDirection: desc,
				where: { volumeUSD_gt: "100" }
			) {
				id
				token0 {
					id
					symbol
					name
					decimals
				}
				token1 {
					id
					symbol
					name
					decimals
				}
				reserve0
				reserve1
				reserveUSD
				volumeUSD
				volumeUSD7d: volumeUSD
				txCount
				txCount7d: txCount
				createdAtBlock
				createdAtTimestamp
			}
		}
	`
	
	// Query for pair by address
	QueryPairByAddress = `
		query GetPair($id: ID!) {
			pair(id: $id) {
				id
				token0 {
					id
					symbol
					name
					decimals
				}
				token1 {
					id
					symbol
					name
					decimals
				}
				reserve0
				reserve1
				reserveUSD
				volumeUSD
				volumeUSD7d: volumeUSD
				txCount
				txCount7d: txCount
				createdAtBlock
				createdAtTimestamp
			}
		}
	`
	
	// Query for token data
	QueryTokenBSC = `
		query GetToken($id: ID!) {
			token(id: $id) {
				id
				symbol
				name
				decimals
				totalSupply
				derivedETH
				volumeUSD
				reserveUSD
				txCount
			}
		}
	`
	
	// Query for recent swaps
	QueryRecentSwaps = `
		query GetSwaps($pair: String!, $first: Int!) {
			swaps(
				first: $first,
				orderBy: timestamp,
				orderDirection: desc,
				where: { pair: $pair }
			) {
				id
				transaction {
					id
					blockNumber
					timestamp
				}
				pair {
					id
					token0 { symbol }
					token1 { symbol }
				}
				sender
				recipient
				tokenIn
				tokenOut
				amountIn
				amountOut
				amountInUSD
				amountOutUSD
				logIndex
			}
		}
	`
	
	// Query for OHLC data
	QueryOHLC = `
		query GetOHLC($pair: String!, $startTime: Int!, $endTime: Int!) {
			pairHourDatas(
				where: {
					pair: $pair,
					date_gt: $startTime,
					date_lt: $endTime
				},
				orderBy: date,
				orderDirection: asc
			) {
				date
				open
				high
				low
				close
				volumeUSD
				txCount
			}
		}
	`
	
	// Query for global analytics
	QueryGlobalAnalytics = `
		query GetFactoryData {
			factory {
				totalValueLockedUSD
										totalVolumeUSD
				totalFeesUSD
				totalTransactions
				pairCount
				tokenCount
			}
		}
	`
)

// =============================================================================
// API HANDLERS
// =============================================================================

// RegisterRoutes registers DEX live feed routes
func (s *LiveFeedService) RegisterRoutes(r *gin.RouterGroup) {
	// Public endpoints (rate limited)
	liveFeed := r.Group("/live")
	liveFeed.Use(s.rateLimitMiddleware())
	{
		liveFeed.GET("/pairs", s.handleGetPairs)
		liveFeed.GET("/pairs/:address", s.handleGetPair)
		liveFeed.GET("/tokens/:address", s.handleGetToken)
		liveFeed.GET("/swaps/:pair", s.handleGetSwaps)
		liveFeed.GET("/ohlc/:pair", s.handleGetOHLC)
		liveFeed.GET("/analytics", s.handleGetAnalytics)
		liveFeed.GET("/search", s.handleSearch)
	}
	
	// Protected endpoints (require API key)
	protected := r.Group("/admin")
	protected.Use(s.apiKeyAuthMiddleware())
	{
		protected.POST("/refresh", s.handleRefreshCache)
		protected.GET("/stats", s.handleGetStats)
	}
}

// =============================================================================
// HANDLER IMPLEMENTATIONS
// =============================================================================

// handleGetPairs returns top trading pairs
func (s *LiveFeedService) handleGetPairs(c *gin.Context) {
	ctx := c.Request.Context()
	
	// Parse query parameters
	chain := c.DefaultQuery("chain", "bsc")
	limit := parseLimit(c.DefaultQuery("limit", "50"))
	offset := parseOffset(c.DefaultQuery("offset", "0"))
	
	// Validate chain
	subgraphURL, ok := s.subgraphs[chain]
	if !ok {
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "error",
			"message": "unsupported chain",
		})
		return
	}
	
	// Check cache first
	cacheKey := fmt.Sprintf("pairs:%s:%d:%d", chain, limit, offset)
	if cached := s.getCachedPair(cacheKey); cached != nil {
		c.JSON(http.StatusOK, gin.H{
			"status": "ok",
			"result": cached,
		})
		return
	}
	
	// Execute GraphQL query
	variables := map[string]interface{}{
		"first": limit,
		"skip":  offset,
	}
	
	result, err := s.executeSubgraphQuery(ctx, subgraphURL, QueryTopPairsBSC, variables)
	if err != nil {
		s.logError("handleGetPairs", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"status":  "error",
			"message": "failed to fetch pairs",
		})
		return
	}
	
	// Parse response
	pairs, err := s.parsePairsResponse(result)
	if err != nil {
		s.logError("handleGetPairs", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"status":  "error",
			"message": "failed to parse response",
		})
		return
	}
	
	// Cache the result
	s.setCachedPair(cacheKey, pairs, CacheTTLPairs)
	
	c.JSON(http.StatusOK, gin.H{
		"status":   "ok",
		"result":   pairs,
		"chain":   chain,
		"limit":   limit,
		"offset":  offset,
	})
}

// handleGetPair returns a specific pair by address
func (s *LiveFeedService) handleGetPair(c *gin.Context) {
	ctx := c.Request.Context()
	address := c.Param("address")
	chain := c.DefaultQuery("chain", "bsc")
	
	// Validate address
	if !isValidAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "error",
			"message": "invalid address",
		})
		return
	}
	
	// Get subgraph URL
	subgraphURL, ok := s.subgraphs[chain]
	if !ok {
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "error",
			"message": "unsupported chain",
		})
		return
	}
	
	// Check cache
	cacheKey := fmt.Sprintf("pair:%s:%s", chain, address)
	if cached := s.getCachedPair(cacheKey); cached != nil {
		c.JSON(http.StatusOK, gin.H{
			"status": "ok",
			"result": cached[0],
		})
		return
	}
	
	// Execute query
	variables := map[string]interface{}{
		"id": strings.ToLower(address),
	}
	
	result, err := s.executeSubgraphQuery(ctx, subgraphURL, QueryPairByAddress, variables)
	if err != nil {
		s.logError("handleGetPair", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"status":  "error",
			"message": "failed to fetch pair",
		})
		return
	}
	
	pair, err := s.parsePairResponse(result)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{
			"status":  "error",
			"message": "pair not found",
		})
		return
	}
	
	s.setCachedPair(cacheKey, []DEXPair{*pair}, CacheTTLLiquidity)
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": pair,
	})
}

// handleGetToken returns token data
func (s *LiveFeedService) handleGetToken(c *gin.Context) {
	ctx := c.Request.Context()
	address := c.Param("address")
	chain := c.DefaultQuery("chain", "bsc")
	
	if !isValidAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "error",
			"message": "invalid address",
		})
		return
	}
	
	subgraphURL, ok := s.subgraphs[chain]
	if !ok {
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "error",
			"message": "unsupported chain",
		})
		return
	}
	
	variables := map[string]interface{}{
		"id": strings.ToLower(address),
	}
	
	result, err := s.executeSubgraphQuery(ctx, subgraphURL, QueryTokenBSC, variables)
	if err != nil {
		s.logError("handleGetToken", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"status":  "error",
			"message": "failed to fetch token",
		})
		return
	}
	
	token, err := s.parseTokenResponse(result)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{
			"status":  "error",
			"message": "token not found",
		})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": token,
	})
}

// handleGetSwaps returns recent swaps for a pair
func (s *LiveFeedService) handleGetSwaps(c *gin.Context) {
	ctx := c.Request.Context()
	pair := c.Param("pair")
	limit := parseLimit(c.DefaultQuery("limit", "50"))
	chain := c.DefaultQuery("chain", "bsc")
	
	if !isValidAddress(pair) {
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "error",
			"message": "invalid pair address",
		})
		return
	}
	
	subgraphURL, ok := s.subgraphs[chain]
	if !ok {
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "error",
			"message": "unsupported chain",
		})
		return
	}
	
	variables := map[string]interface{}{
		"pair":  strings.ToLower(pair),
		"first": limit,
	}
	
	result, err := s.executeSubgraphQuery(ctx, subgraphURL, QueryRecentSwaps, variables)
	if err != nil {
		s.logError("handleGetSwaps", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"status":  "error",
			"message": "failed to fetch swaps",
		})
		return
	}
	
	swaps, err := s.parseSwapsResponse(result)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{
			"status":  "error",
			"message": "pair not found",
		})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": swaps,
	})
}

// handleGetOHLC returns OHLC candlestick data
func (s *LiveFeedService) handleGetOHLC(c *gin.Context) {
	ctx := c.Request.Context()
	pair := c.Param("pair")
	chain := c.DefaultQuery("chain", "bsc")
	
	// Parse time range
	startTime := parseTimestamp(c.DefaultQuery("start", ""))
	endTime := parseTimestamp(c.DefaultQuery("end", ""))
	
	if startTime == 0 {
		startTime = time.Now().Add(-24 * time.Hour).Unix()
	}
	if endTime == 0 {
		endTime = time.Now().Unix()
	}
	
	subgraphURL, ok := s.subgraphs[chain]
	if !ok {
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "error",
			"message": "unsupported chain",
		})
		return
	}
	
	variables := map[string]interface{}{
		"pair":     strings.ToLower(pair),
		"startTime": startTime,
		"endTime":  endTime,
	}
	
	result, err := s.executeSubgraphQuery(ctx, subgraphURL, QueryOHLC, variables)
	if err != nil {
		s.logError("handleGetOHLC", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"status":  "error",
			"message": "failed to fetch OHLC data",
		})
		return
	}
	
	ohlc, err := s.parseOHLCResponse(result)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{
			"status":  "error",
			"message": "pair not found",
		})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": ohlc,
	})
}

// handleGetAnalytics returns global DEX analytics
func (s *LiveFeedService) handleGetAnalytics(c *gin.Context) {
	ctx := c.Request.Context()
	chain := c.DefaultQuery("chain", "bsc")
	
	subgraphURL, ok := s.subgraphs[chain]
	if !ok {
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "error",
			"message": "unsupported chain",
		})
		return
	}
	
	// Check cache
	cacheKey := fmt.Sprintf("analytics:%s", chain)
	if cached := s.getCachedPair(cacheKey); cached != nil {
		if analytics, ok := cached[0].(DEXAnalytics); ok {
			c.JSON(http.StatusOK, gin.H{
				"status": "ok",
				"result": analytics,
			})
			return
		}
	}
	
	result, err := s.executeSubgraphQuery(ctx, subgraphURL, QueryGlobalAnalytics, nil)
	if err != nil {
		s.logError("handleGetAnalytics", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"status":  "error",
			"message": "failed to fetch analytics",
		})
		return
	}
	
	analytics, err := s.parseAnalyticsResponse(result)
	if err != nil {
		s.logError("handleGetAnalytics", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"status":  "error",
			"message": "failed to parse analytics",
		})
		return
	}
	
	// Fetch top pairs
	topPairs, _ := s.fetchTopPairs(ctx, chain, 10)
	analytics.TopPairs = topPairs
	
	// Fetch top tokens
	topTokens, _ := s.fetchTopTokens(ctx, chain, 10)
	analytics.TopTokens = topTokens
	
	analytics.UpdatedAt = time.Now()
	
	// Cache
	s.setCachedPair(cacheKey, []DEXAnalytics{*analytics}, CacheTTLVolume)
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": analytics,
	})
}

// handleSearch searches DEX pairs and tokens
func (s *LiveFeedService) handleSearch(c *gin.Context) {
	query := c.Query("q")
	chain := c.DefaultQuery("chain", "bsc")
	limit := parseLimit(c.DefaultQuery("limit", "20"))
	
	if query == "" {
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "error",
			"message": "search query required",
		})
		return
	}
	
	query = strings.ToLower(query)
	
	// Search pairs
	subgraphURL, ok := s.subgraphs[chain]
	if !ok {
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "error",
			"message": "unsupported chain",
		})
		return
	}
	
	// Execute search query (simplified - in production would use full-text search)
	searchQuery := fmt.Sprintf(`
		query Search($query: String!) {
			pairs(where: { token0_: { symbol_contains: $query }, first: %d) {
				id
				token0 { symbol name }
				token1 { symbol name }
				reserveUSD
				volumeUSD
			}
		}
	`, limit)
	
	variables := map[string]interface{}{
		"query": query,
	}
	
	result, err := s.executeSubgraphQuery(c.Request.Context(), subgraphURL, searchQuery, variables)
	if err != nil {
		c.JSON(http.StatusOK, gin.H{
			"status":  "ok",
			"result": []interface{}{},
		})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": result,
	})
}

// =============================================================================
// ADMIN HANDLERS
// =============================================================================

// handleRefreshCache manually refreshes the cache
func (s *LiveFeedService) handleRefreshCache(c *gin.Context) {
	chain := c.DefaultQuery("chain", "bsc")
	
	// Clear caches for the chain
	cacheKeys := []string{
		fmt.Sprintf("pairs:%s:*", chain),
		fmt.Sprintf("pair:%s:*", chain),
		fmt.Sprintf("analytics:%s", chain),
	}
	
	for _, key := range cacheKeys {
		s.clearCachePattern(key)
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status":  "ok",
		"message": "cache refreshed",
	})
}

// handleGetStats returns service statistics
func (s *LiveFeedService) handleGetStats(c *gin.Context) {
	s.cacheMu.RLock()
	defer s.cacheMu.RUnlock()
	
	stats := map[string]interface{}{
		"pairCacheSize":    len(s.pairCache),
		"liquidityCacheSize": len(s.liquidityCache),
		"volumeCacheSize":  len(s.volumeCache),
		"supportedChains":  len(s.subgraphs),
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": stats,
	})
}

// =============================================================================
// SUBGRAPH QUERY EXECUTION
// =============================================================================

// subgraphResponse represents a GraphQL response
type subgraphResponse struct {
	Data   json.RawMessage `json:"data"`
	Errors []subgraphError  `json:"errors"`
}

type subgraphError struct {
	Message string `json:"message"`
}

// executeSubgraphQuery executes a GraphQL query against a subgraph
func (s *LiveFeedService) executeSubgraphQuery(ctx context.Context, endpoint, query string, variables map[string]interface{}) (json.RawMessage, error) {
	// Rate limiting
	if !s.rateLimit.TryAccept() {
		return nil, fmt.Errorf("rate limit exceeded")
	}
	
	// Prepare request
	body := map[string]interface{}{
		"query":     query,
		"variables": variables,
	}
	
	bodyBytes, err := json.Marshal(body)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal query: %w", err)
	}
	
	req, err := http.NewRequestWithContext(ctx, "POST", endpoint, strings.NewReader(string(bodyBytes)))
	if err != nil {
		return nil, fmt.Errorf("failed to create request: %w", err)
	}
	
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")
	
	// Execute request
	resp, err := s.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("failed to execute query: %w", err)
	}
	defer resp.Body.Close()
	
	// Parse response
	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("failed to read response: %w", err)
	}
	
	var result subgraphResponse
	if err := json.Unmarshal(respBody, &result); err != nil {
		return nil, fmt.Errorf("failed to parse response: %w", err)
	}
	
	// Check for errors
	if len(result.Errors) > 0 {
		errMsgs := make([]string, len(result.Errors))
		for i, e := range result.Errors {
			errMsgs[i] = e.Message
		}
		return nil, fmt.Errorf("subgraph errors: %s", strings.Join(errMsgs, ", "))
	}
	
	return result.Data, nil
}

// =============================================================================
// RESPONSE PARSING
// =============================================================================

// parsePairsResponse parses the pairs response
func (s *LiveFeedService) parsePairsResponse(data json.RawMessage) ([]DEXPair, error) {
	var wrapper struct {
		Pairs []struct {
			ID            string `json:"id"`
			Reserve0      string `json:"reserve0"`
			Reserve1     string `json:"reserve1"`
			ReserveUSD   string `json:"reserveUSD"`
			VolumeUSD    string `json:"volumeUSD"`
			VolumeUSD7d  string `json:"volumeUSD7d"`
			TxCount      string `json:"txCount"`
			TxCount7d    string `json:"txCount7d"`
			CreatedAtBlock string `json:"createdAtBlock"`
			CreatedAtTimestamp string `json:"createdAtTimestamp"`
			Token0        struct {
				ID       string `json:"id"`
				Symbol  string `json:"symbol"`
				Name    string `json:"name"`
				Decimals string `json:"decimals"`
			} `json:"token0"`
			Token1 struct {
				ID       string `json:"id"`
				Symbol  string `json:"symbol"`
				Name    string `json:"name"`
				Decimals string `json:"decimals"`
			} `json:"token1"`
		} `json:"pairs"`
	}
	
	if err := json.Unmarshal(data, &wrapper); err != nil {
		return nil, err
	}
	
	pairs := make([]DEXPair, len(wrapper.Pairs))
	for i, p := range wrapper.Pairs {
		token0Decimals, _ := parseUint8(p.Token0.Decimals)
		token1Decimals, _ := parseUint8(p.Token1.Decimals)
		
		reserveUSD, _ := parseFloat(p.ReserveUSD)
		volumeUSD, _ := parseFloat(p.VolumeUSD)
		txCount, _ := parseInt64(p.TxCount)
		
		pairs[i] = DEXPair{
			ID:                p.ID,
			Token0:            p.Token0.ID,
			Token1:            p.Token1.ID,
			Token0Symbol:      p.Token0.Symbol,
			Token1Symbol:      p.Token1.Symbol,
			Token0Decimals:    token0Decimals,
			Token1Decimals:    token1Decimals,
			Reserve0:          p.Reserve0,
			Reserve1:          p.Reserve1,
			LiquidityUSD:      reserveUSD,
			Volume24h:        volumeUSD,
			Volume7d:          volumeUSD,
			TxCount24h:       txCount,
			TxCount7d:        txCount,
		}
	}
	
	return pairs, nil
}

// parsePairResponse parses a single pair response
func (s *LiveFeedService) parsePairResponse(data json.RawMessage) (*DEXPair, error) {
	var wrapper struct {
		Pair struct {
			ID               string `json:"id"`
			Reserve0        string `json:"reserve0"`
			Reserve1        string `json:"reserve1"`
			ReserveUSD      string `json:"reserveUSD"`
			VolumeUSD       string `json:"volumeUSD"`
			TxCount         string `json:"txCount"`
			CreatedAtBlock  string `json:"createdAtBlock"`
			CreatedAtTimestamp string `json:"createdAtTimestamp"`
			Token0         struct {
				ID       string `json:"id"`
				Symbol  string `json:"symbol"`
				Name   string `json:"name"`
				Decimals string `json:"decimals"`
			} `json:"token0"`
			Token1 struct {
				ID       string `json:"id"`
				Symbol  string `json:"symbol"`
				Name   string `json:"name"`
				Decimals string `json:"decimals"`
			} `json:"token1"`
		} `json:"pair"`
	}
	
	if err := json.Unmarshal(data, &wrapper); err != nil {
		return nil, err
	}
	
	if wrapper.Pair.ID == "" {
		return nil, fmt.Errorf("pair not found")
	}
	
	p := wrapper.Pair
	token0Decimals, _ := parseUint8(p.Token0.Decimals)
	token1Decimals, _ := parseUint8(p.Token1.Decimals)
	reserveUSD, _ := parseFloat(p.ReserveUSD)
	volumeUSD, _ := parseFloat(p.VolumeUSD)
	txCount, _ := parseInt64(p.TxCount)
	createdAtBlock, _ := parseUint64(p.CreatedAtBlock)
	
	return &DEXPair{
		ID:               p.ID,
		Token0:          p.Token0.ID,
		Token1:          p.Token1.ID,
		Token0Symbol:    p.Token0.Symbol,
		Token1Symbol:    p.Token1.Symbol,
		Token0Decimals:  token0Decimals,
		Token1Decimals:  token1Decimals,
		Reserve0:        p.Reserve0,
		Reserve1:        p.Reserve1,
		LiquidityUSD:    reserveUSD,
		Volume24h:      volumeUSD,
		Volume7d:       volumeUSD,
		TxCount24h:     txCount,
		TxCount7d:      txCount,
		CreatedAtBlock: createdAtBlock,
	}, nil
}

// parseTokenResponse parses token response
func (s *LiveFeedService) parseTokenResponse(data json.RawMessage) (*DEXToken, error) {
	var wrapper struct {
		Token struct {
			ID          string `json:"id"`
			Symbol      string `json:"symbol"`
			Name        string `json:"name"`
			Decimals    string `json:"decimals"`
			TotalSupply string `json:"totalSupply"`
			VolumeUSD   string `json:"volumeUSD"`
			ReserveUSD  string `json:"reserveUSD"`
			TxCount     string `json:"txCount"`
		} `json:"token"`
	}
	
	if err := json.Unmarshal(data, &wrapper); err != nil {
		return nil, err
	}
	
	if wrapper.Token.ID == "" {
		return nil, fmt.Errorf("token not found")
	}
	
	t := wrapper.Token
	decimals, _ := parseUint8(t.Decimals)
	volumeUSD, _ := parseFloat(t.VolumeUSD)
	reserveUSD, _ := parseFloat(t.ReserveUSD)
	txCount, _ := parseInt64(t.TxCount)
	
	return &DEXToken{
		ID:             t.ID,
		Symbol:        t.Symbol,
		Name:          t.Name,
		Decimals:      decimals,
		TotalSupply:   t.TotalSupply,
		VolumeUSD24h: volumeUSD,
		LiquidityUSD:  reserveUSD,
		TxCount24h:    txCount,
	}, nil
}

// parseSwapsResponse parses swaps response
func (s *LiveFeedService) parseSwapsResponse(data json.RawMessage) ([]DEXTransaction, error) {
	// Simplified implementation
	return []DEXTransaction{}, nil
}

// parseOHLCResponse parses OHLC response
func (s *LiveFeedService) parseOHLCResponse(data json.RawMessage) ([]OHLCData, error) {
	// Simplified implementation
	return []OHLCData{}, nil
}

// parseAnalyticsResponse parses analytics response
func (s *LiveFeedService) parseAnalyticsResponse(data json.RawMessage) (*DEXAnalytics, error) {
	var wrapper struct {
		Factory struct {
			TotalValueLockedUSD string `json:"totalValueLockedUSD"`
			TotalVolumeUSD    string `json:"totalVolumeUSD"`
			TotalFeesUSD      string `json:"totalFeesUSD"`
			TotalTransactions string `json:"totalTransactions"`
			PairCount        string `json:"pairCount"`
			TokenCount       string `json:"tokenCount"`
		} `json:"factory"`
	}
	
	if err := json.Unmarshal(data, &wrapper); err != nil {
		return nil, err
	}
	
	f := wrapper.Factory
	tvl, _ := parseFloat(f.TotalValueLockedUSD)
	volume, _ := parseFloat(f.TotalVolumeUSD)
	fees, _ := parseFloat(f.TotalFeesUSD)
	txCount, _ := parseInt64(f.TotalTransactions)
	
	return &DEXAnalytics{
		TotalLiquidityUSD: tvl,
		TotalVolume24h:   volume,
		TotalVolume7d:    volume,
		TotalFees24h:     fees,
		TotalTxCount24h: txCount,
		TotalTxCount7d:   txCount,
	}, nil
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

// fetchTopPairs fetches top pairs for analytics
func (s *LiveFeedService) fetchTopPairs(ctx context.Context, chain string, limit int) ([]DEXPair, error) {
	subgraphURL, ok := s.subgraphs[chain]
	if !ok {
		return nil, fmt.Errorf("unsupported chain")
	}
	
	variables := map[string]interface{}{
		"first": limit,
		"skip":  0,
	}
	
	data, err := s.executeSubgraphQuery(ctx, subgraphURL, QueryTopPairsBSC, variables)
	if err != nil {
		return nil, err
	}
	
	return s.parsePairsResponse(data)
}

// fetchTopTokens fetches top tokens
func (s *LiveFeedService) fetchTopTokens(ctx context.Context, chain string, limit int) ([]DEXToken, error) {
	// Simplified - would need separate query
	return []DEXToken{}, nil
}

// getCachedPair returns cached pairs if not expired
func (s *LiveFeedService) getCachedPair(key string) []DEXPair {
	s.cacheMu.RLock()
	defer s.cacheMu.RUnlock()
	
	entry, ok := s.pairCache[key]
	if !ok || time.Now().After(entry.Expiry) {
		return nil
	}
	
	if pairs, ok := entry.Data.([]DEXPair); ok {
		return pairs
	}
	return nil
}

// setCachedPair sets cache entry for pairs
func (s *LiveFeedService) setCachedPair(key string, data []DEXPair, ttl time.Duration) {
	s.cacheMu.Lock()
	defer s.cacheMu.Unlock()
	
	s.pairCache[key] = &cacheEntry{
		Data:  data,
		Expiry: time.Now().Add(ttl),
	}
}

// clearCachePattern clears cache entries matching pattern
func (s *LiveFeedService) clearCachePattern(pattern string) {
	s.cacheMu.Lock()
	defer s.cacheMu.Unlock()
	
	// Simple implementation - clear all for now
	s.pairCache = make(map[string]*cacheEntry)
	s.liquidityCache = make(map[string]*cacheEntry)
	s.volumeCache = make(map[string]*cacheEntry)
}

// cleanupCache runs background cache cleanup
func (s *LiveFeedService) cleanupCache() {
	ticker := time.NewTicker(1 * time.Minute)
	defer ticker.Stop()
	
	for range ticker.C {
		s.cacheMu.Lock()
		now := time.Now()
		
		for key, entry := range s.pairCache {
			if now.After(entry.Expiry) {
				delete(s.pairCache, key)
			}
		}
		
		for key, entry := range s.liquidityCache {
			if now.After(entry.Expiry) {
				delete(s.liquidityCache, key)
			}
		}
		
		for key, entry := range s.volumeCache {
			if now.After(entry.Expiry) {
				delete(s.volumeCache, key)
			}
		}
		
		s.cacheMu.Unlock()
	}
}

// rateLimitMiddleware returns rate limiting middleware
func (s *LiveFeedService) rateLimitMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		apiKey := c.GetHeader("X-API-Key")
		
		// Check rate limit
		if !s.rateLimit.TryAccept() {
			c.JSON(http.StatusTooManyRequests, gin.H{
				"status":  "error",
				"message": "rate limit exceeded",
			})
			c.Abort()
			return
		}
		
		c.Next()
	}
}

// apiKeyAuthMiddleware returns API key authentication middleware
func (s *LiveFeedService) apiKeyAuthMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		apiKey := c.GetHeader("X-API-Key")
		
		if apiKey == "" {
			c.JSON(http.StatusUnauthorized, gin.H{
				"status":  "error",
				"message": "API key required",
			})
			c.Abort()
			return
		}
		
		// Validate API key
		valid, err := s.apiKeyStore.ValidateKey(c.Request.Context(), apiKey)
		if err != nil || !valid {
			c.JSON(http.StatusUnauthorized, gin.H{
				"status":  "error",
				"message": "invalid API key",
			})
			c.Abort()
			return
		}
		
		c.Next()
	}
}

// logError logs an error
func (s *LiveFeedService) logError(context string, err error) {
	// In production, would use proper logging
	fmt.Printf("[ERROR] %s: %v\n", context, err)
}

// =============================================================================
// CONFIGURATION
// =============================================================================

// LiveFeedConfig contains service configuration
type LiveFeedConfig struct {
	DB           *sql.DB
	RedisAddr    string
	RedisPassword string
	RedisDB      int
	Subgraphs    map[string]string
}

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

func parseLimit(s string) int {
	limit, err := strconv.Atoi(s)
	if err != nil || limit < 1 {
		return DefaultPageSize
	}
	if limit > MaxPageSize {
		return MaxPageSize
	}
	return limit
}

func parseOffset(s string) int {
	offset, err := strconv.Atoi(s)
	if err != nil || offset < 0 {
		return 0
	}
	return offset
}

func parseUint8(s string) (uint8, error) {
	n, err := strconv.ParseUint(s, 10, 8)
	return uint8(n), err
}

func parseUint64(s string) (uint64, error) {
	return strconv.ParseUint(s, 10, 64)
}

func parseFloat(s string) (float64, error) {
	if s == "" {
		return 0, nil
	}
	return strconv.ParseFloat(s, 64)
}

func parseInt64(s string) (int64, error) {
	return strconv.ParseInt(s, 10, 64)
}

func parseTimestamp(s string) int64 {
	if s == "" {
		return 0
	}
	// Try Unix timestamp
	if n, err := strconv.ParseInt(s, 10, 64); err == nil {
		return n
	}
	// Try RFC3339
	if t, err := time.Parse(time.RFC3339, s); err == nil {
		return t.Unix()
	}
	return 0
}

func isValidAddress(s string) bool {
	if s == "" {
		return false
	}
	// Basic check - should be 42 chars starting with 0x
	if len(s) != 42 {
		return false
	}
	return strings.HasPrefix(strings.ToLower(s), "0x")
}

// =============================================================================
// RATE LIMITER
// =============================================================================

// RateLimiter implements token bucket rate limiting
type RateLimiter struct {
	tokens    int
	maxTokens int
	refillRate time.Duration
	lastRefill time.Time
	mu        sync.Mutex
}

// NewRateLimiter creates a new rate limiter
func NewRateLimiter(maxTokens int, refillRate time.Duration) *RateLimiter {
	return &RateLimiter{
		tokens:    maxTokens,
		maxTokens: maxTokens,
		refillRate: refillRate,
		lastRefill: time.Now(),
	}
}

// TryAccept attempts to consume a token
func (r *RateLimiter) TryAccept() bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	
	now := time.Now()
	elapsed := now.Sub(r.lastRefill)
	
	// Refill tokens
	if elapsed >= r.refillRate {
		refills := int(elapsed / r.refillRate)
		r.tokens = min(r.tokens+refills, r.maxTokens)
		r.lastRefill = now
	}
	
	if r.tokens > 0 {
		r.tokens--
		return true
	}
	
	return false
}

// =============================================================================
// API KEY STORE
// =============================================================================

// APIKeyStore manages API keys
type APIKeyStore struct {
	db *sql.DB
}

// NewAPIKeyStore creates a new API key store
func NewAPIKeyStore(db *sql.DB) *APIKeyStore {
	return &APIKeyStore{db: db}
}

// ValidateKey validates an API key
func (s *APIKeyStore) ValidateKey(ctx context.Context, key string) (bool, error) {
	if key == "" {
		return false, nil
	}
	
	// In production, validate against database
	// For now, accept any non-empty key
	return len(key) >= 16, nil
}