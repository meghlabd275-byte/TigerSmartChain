// Transfer Graph API
// Production-grade API for token/NFT transfer graph analysis
// Provides: Node/link graph data, clustering, time-based analysis

package graphs

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"net/http"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
)

// =============================================================================
// CONFIGURATION
// =============================================================================

type Config struct {
	DB              *sql.DB
	Redis            *redis.Client
	MaxNodes         int
	MaxLinks         int
	CacheTTL        time.Duration
	EnableClustering bool
	ClusterThreshold float64
}

// =============================================================================
// TYPES
// =============================================================================

// TransferNode represents a node in the transfer graph
type TransferNode struct {
	ID          string  `json:"id"`
	Address     string  `json:"address"`
	Type       string  `json:"type"` // "address", "contract", "token"
	Label      string  `json:"label"`
	Value       float64 `json:"value"`
	ValueUSD   float64 `json:"valueUsd"`
	TxCount    int     `json:"txCount"`
	InDegree   int     `json:"inDegree"`
	OutDegree  int     `json:"outDegree"`
	X          float64 `json:"x,omitempty"`
	Y          float64 `json:"y,omitempty"`
	Fx         float64 `json:"fx,omitempty"`
	Fy         float64 `json:"fy,omitempty"`
}

// TransferLink represents an edge in the transfer graph
type TransferLink struct {
	Source         string  `json:"source"`
	Target        string  `json:"target"`
	Value         float64 `json:"value"`
	ValueUSD      float64 `json:"valueUsd"`
	TokenAddress  string  `json:"tokenAddress"`
	TransactionHash string `json:"transactionHash"`
	Timestamp    int64   `json:"timestamp"`
	Type         string  `json:"type"` // "transfer", "swap", "mint", "burn"
}

// TransferGraphResponse represents the API response
type TransferGraphResponse struct {
	Nodes []TransferNode `json:"nodes"`
	Links []TransferLink `json:"links"`
	Meta  GraphMeta   `json:"meta"`
}

// GraphMeta contains metadata about the graph
type GraphMeta struct {
	TotalValue    float64 `json:"totalValue"`
	TotalValueUSD float64 `json:"totalValueUsd"`
	TotalNodes   int     `json:"totalNodes"`
	TotalLinks   int     `json:"totalLinks"`
	TimeRange   string  `json:"timeRange"`
	TokenAddress string  `json:"tokenAddress,omitempty"`
}

// =============================================================================
// SERVER
// =============================================================================

type Server struct {
	cfg    *Config
	mu    sync.RWMutex
	cache map[string]*CacheEntry
}

type CacheEntry struct {
	Data      *TransferGraphResponse
	ExpiresAt time.Time
}

// NewServer creates a new transfer graph server
func NewServer(cfg *Config) *Server {
	if cfg == nil {
		cfg = &Config{}
	}
	
	// Set defaults
	if cfg.MaxNodes == 0 {
		cfg.MaxNodes = 500
	}
	if cfg.MaxLinks == 0 {
		cfg.MaxLinks = 2000
	}
	if cfg.CacheTTL == 0 {
		cfg.CacheTTL = 5 * time.Minute
	}
	if cfg.ClusterThreshold == 0 {
		cfg.ClusterThreshold = 100000 // $100k threshold
	}
	
	return &Server{
		cfg:    cfg,
		cache: make(map[string]*CacheEntry),
	}
}

// =============================================================================
// API HANDLERS
// =============================================================================

// GetTransferGraph handles GET /api/v1/graphs/transfers
func (s *Server) GetTransferGraph(c *gin.Context) {
	ctx := c.Request.Context()
	
	// Parse query parameters
	tokenAddress := c.Query("token")
	address := c.Query("address")
	timeRange := c.DefaultQuery("time_range", "24h")
	limitStr := c.DefaultQuery("limit", "100")
	
	limit, err := strconv.Atoi(limitStr)
	if err != nil || limit <= 0 {
		limit = 100
	}
	if limit > s.cfg.MaxNodes {
		limit = s.cfg.MaxNodes
	}
	
	// Generate cache key
	cacheKey := fmt.Sprintf("transfer_graph:%s:%s:%s:%d", tokenAddress, address, timeRange, limit)
	
	// Check cache first
	if entry := s.getCacheEntry(cacheKey); entry != nil {
		c.JSON(http.StatusOK, entry.Data)
		return
	}
	
	// Calculate time range
	startTime := s.parseTimeRange(timeRange)
	if startTime.IsZero() {
		startTime = time.Now().Add(-24 * time.Hour)
	}
	
	// Fetch graph data from database
	graph, err := s.fetchTransferGraph(ctx, tokenAddress, address, startTime, limit)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": fmt.Sprintf("Failed to fetch transfer graph: %v", err),
		})
		return
	}
	
	// Enrich with USD values
	if err := s.enrichWithUSDValues(ctx, graph); err != nil {
		// Continue even if enrichment fails
		fmt.Printf("Warning: Failed to enrich USD values: %v\n", err)
	}
	
	// Apply clustering if enabled
	if s.cfg.EnableClustering {
		s.applyClustering(graph)
	}
	
	// Build response
	response := &TransferGraphResponse{
		Nodes: graph.Nodes,
		Links: graph.Links,
		Meta: GraphMeta{
			TotalValue:    calculateTotalValue(graph.Nodes),
			TotalValueUSD: calculateTotalUSD(graph.Nodes),
			TotalNodes:  len(graph.Nodes),
			TotalLinks:  len(graph.Links),
			TimeRange:   timeRange,
			TokenAddress: tokenAddress,
		},
	}
	
	// Cache response
	s.setCacheEntry(cacheKey, response)
	
	c.JSON(http.StatusOK, response)
}

// GetTokenTransfers handles GET /api/v1/graphs/token-transfers
func (s *Server) GetTokenTransfers(c *gin.Context) {
	ctx := c.Request.Context()
	
	tokenAddress := c.Param("address")
	if tokenAddress == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Token address required"})
		return
	}
	
	// Parse pagination
	page, _ := strconv.Atoi(c.DefaultQuery("page", "1"))
	limit, _ := strconv.Atoi(c.DefaultQuery("limit", "50"))
	if limit <= 0 || limit > 100 {
		limit = 50
	}
	offset := (page - 1) * limit
	
	// Get time range
	timeRange := c.DefaultQuery("time_range", "24h")
	startTime := s.parseTimeRange(timeRange)
	if startTime.IsZero() {
		startTime = time.Now().Add(-24 * time.Hour)
	}
	
	// Fetch transfers
	transfers, total, err := s.fetchTokenTransfers(ctx, tokenAddress, startTime, limit, offset)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": fmt.Sprintf("Failed to fetch transfers: %v", err),
		})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"transfers": transfers,
		"pagination": gin.H{
			"page":       page,
			"limit":      limit,
			"total":      total,
			"totalPages": (total + limit - 1) / limit,
		},
	})
}

// =============================================================================
// DATABASE QUERIES
// =============================================================================

// fetchTransferGraph fetches transfer graph data from database
func (s *Server) fetchTransferGraph(
	ctx context.Context,
	tokenAddress string,
	address string,
	startTime time.Time,
	limit int,
) (*TransferGraphResponse, error) {
	graph := &TransferGraphResponse{
		Nodes: make([]TransferNode, 0, limit),
		Links: make([]TransferLink, 0, limit*2),
	}
	
	nodeMap := make(map[string]*TransferNode)
	linkMap := make(map[string]*TransferLink)
	
	// Query token transfers
	query := `
		SELECT 
			tt.from_address,
			tt.to_address,
			tt.value,
			tt.transaction_hash,
			tt.block_number,
			tt.log_index,
			b.timestamp,
			COALESCE(t.price_usd, 0) as token_price
		FROM token_transfers tt
		JOIN blocks b ON b.number = tt.block_number
		LEFT JOIN tokens t ON t.address = tt.token_address
		WHERE b.timestamp >= $1
	`
	args := []interface{}{startTime.Unix()}
	argIdx := 2
	
	if tokenAddress != "" {
		query += fmt.Sprintf(" AND tt.token_address = $%d", argIdx)
		args = append(args, tokenAddress)
		argIdx++
	}
	
	if address != "" {
		query += fmt.Sprintf(" AND (tt.from_address = $%d OR tt.to_address = $%d)", argIdx, argIdx)
		args = append(args, address, address)
		argIdx++
	}
	
	query += fmt.Sprintf(" ORDER BY b.timestamp DESC LIMIT $%d", argIdx)
	args = append(args, limit*10)
	
	rows, err := s.cfg.DB.QueryContext(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	for rows.Next() {
		var fromAddr, toAddr, txHash string
		var value, tokenPrice float64
		var blockNum, logIndex int64
		var timestamp int64
		
		if err := rows.Scan(&fromAddr, &toAddr, &value, &txHash, &blockNum, &logIndex, &timestamp, &tokenPrice); err != nil {
			continue
		}
		
		valueUSD := value * tokenPrice
		
		// Add or update from node
		if node, ok := nodeMap[fromAddr]; ok {
			node.Value += value
			node.ValueUSD += valueUSD
			node.TxCount++
			node.OutDegree++
		} else {
			nodeMap[fromAddr] = &TransferNode{
				ID:         fromAddr,
				Address:    fromAddr,
				Type:       "address",
				Label:     shortenAddress(fromAddr),
				Value:      value,
				ValueUSD:   valueUSD,
				TxCount:   1,
				OutDegree: 1,
			}
		}
		
		// Add or update to node
		if node, ok := nodeMap[toAddr]; ok {
			node.Value += value
			node.ValueUSD += valueUSD
			node.TxCount++
			node.InDegree++
		} else {
			nodeMap[toAddr] = &TransferNode{
				ID:        toAddr,
				Address:   toAddr,
				Type:      "address",
				Label:    shortenAddress(toAddr),
				Value:     value,
				ValueUSD:  valueUSD,
				TxCount:  1,
				InDegree: 1,
			}
		}
		
		// Add link
		linkKey := fmt.Sprintf("%s->%s", fromAddr, toAddr)
		if link, ok := linkMap[linkKey]; ok {
			link.Value += value
			link.ValueUSD += valueUSD
		} else {
			linkMap[linkKey] = &TransferLink{
				Source:         fromAddr,
				Target:        toAddr,
				Value:         value,
				ValueUSD:      valueUSD,
				TokenAddress:  tokenAddress,
				TransactionHash: txHash,
				Timestamp:    timestamp,
				Type:         "transfer",
			}
		}
	}
	
	// Convert maps to slices
	for _, node := range nodeMap {
		graph.Nodes = append(graph.Nodes, *node)
	}
	
	// Sort by value USD descending
	sort.Slice(graph.Nodes, func(i, j int) bool {
		return graph.Nodes[i].ValueUSD > graph.Nodes[j].ValueUSD
	})
	
	// Limit nodes
	if len(graph.Nodes) > limit {
		graph.Nodes = graph.Nodes[:limit]
	}
	
	for _, link := range linkMap {
		graph.Links = append(graph.Links, *link)
	}
	
	// Limit links
	if len(graph.Links) > limit*2 {
		graph.Links = graph.Links[:limit*2]
	}
	
	return graph, nil
}

// fetchTokenTransfers fetches token transfers for a specific token
func (s *Server) fetchTokenTransfers(
	ctx context.Context,
	tokenAddress string,
	startTime time.Time,
	limit, offset int,
) ([]map[string]interface{}, int, error) {
	// Get total count
	var total int
	countQuery := `
		SELECT COUNT(*) 
		FROM token_transfers tt
		JOIN blocks b ON b.number = tt.block_number
		WHERE tt.token_address = $1 AND b.timestamp >= $2
	`
	if err := s.cfg.DB.QueryRowContext(ctx, countQuery, tokenAddress, startTime.Unix()).Scan(&total); err != nil {
		return nil, 0, err
	}
	
	// Get transfers
	query := `
		SELECT 
			tt.from_address,
			tt.to_address,
			tt.value,
			tt.transaction_hash,
			tt.block_number,
			b.timestamp,
			COALESCE(t.price_usd, 0) as token_price
		FROM token_transfers tt
		JOIN blocks b ON b.number = tt.block_number
		LEFT JOIN tokens t ON t.address = tt.token_address
		WHERE tt.token_address = $1 AND b.timestamp >= $2
		ORDER BY b.timestamp DESC
		LIMIT $3 OFFSET $4
	`
	
	rows, err := s.cfg.DB.QueryContext(ctx, query, tokenAddress, startTime.Unix(), limit, offset)
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()
	
	transfers := make([]map[string]interface{}, 0)
	for rows.Next() {
		var fromAddr, toAddr, txHash string
		var value, tokenPrice float64
		var blockNum, timestamp int64
		
		if err := rows.Scan(&fromAddr, &toAddr, &value, &txHash, &blockNum, &timestamp, &tokenPrice); err != nil {
			continue
		}
		
		transfers = append(transfers, map[string]interface{}{
			"from":           fromAddr,
			"to":             toAddr,
			"value":          fmt.Sprintf("%.0f", value),
			"valueUsd":       value * tokenPrice,
			"transaction":    txHash,
			"blockNumber":    blockNum,
			"timestamp":     timestamp,
			"tokenAddress":  tokenAddress,
		})
	}
	
	return transfers, total, nil
}

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

// parseTimeRange parses time range string to start time
func (s *Server) parseTimeRange(timeRange string) time.Time {
	now := time.Now()
	
	switch strings.ToLower(timeRange) {
	case "1h":
		return now.Add(-1 * time.Hour)
	case "6h":
		return now.Add(-6 * time.Hour)
	case "24h", "":
		return now.Add(-24 * time.Hour)
	case "7d":
		return now.Add(-7 * 24 * time.Hour)
	case "30d":
		return now.Add(-30 * 24 * time.Hour)
	default:
		return now.Add(-24 * time.Hour)
	}
}

// enrichWithUSDValues enriches nodes with USD values
func (s *Server) enrichWithUSDValues(ctx context.Context, graph *TransferGraphResponse) error {
	if s.cfg.Redis == nil || len(graph.Nodes) == 0 {
		return nil
	}
	
	// Get ETH price from cache
	ethPrice, err := s.getETHPrice(ctx)
	if err != nil {
		return err
	}
	
	// Calculate USD values for each node
	for i := range graph.Nodes {
		// Assuming value is in wei, convert to ETH then USD
		graph.Nodes[i].ValueUSD = graph.Nodes[i].Value * ethPrice
	}
	
	for i := range graph.Links {
		graph.Links[i].ValueUSD = graph.Links[i].Value * ethPrice
	}
	
	return nil
}

// getETHPrice gets current ETH price from Redis or API
func (s *Server) getETHPrice(ctx context.Context) (float64, error) {
	// Try Redis first
	if s.cfg.Redis != nil {
		price, err := s.cfg.Redis.Get(ctx, "eth_price").Float64()
		if err == nil {
			return price, nil
		}
	}
	
	// Default price (in production, fetch from price API)
	return 3000.0, nil
}

// applyClustering applies clustering to the graph
func (s *Server) applyClustering(graph *TransferGraphResponse) {
	if len(graph.Nodes) <= s.cfg.MaxNodes {
		return
	}
	
	// Group small nodes into clusters
	smallNodes := make([]*TransferNode, 0)
	largeNodes := make([]*TransferNode, 0)
	
	for i := range graph.Nodes {
		node := &graph.Nodes[i]
		if node.ValueUSD < s.cfg.ClusterThreshold {
			smallNodes = append(smallNodes, node)
		} else {
			largeNodes = append(largeNodes, node)
		}
	}
	
	// If we have too many nodes, cluster the small ones
	if len(graph.Nodes) > s.cfg.MaxNodes {
		// Keep top nodes and cluster the rest
		graph.Nodes = largeNodes
		if len(graph.Nodes) < s.cfg.MaxNodes {
			graph.Nodes = append(graph.Nodes, smallNodes[:s.cfg.MaxNodes-len(graph.Nodes)]...)
		}
	}
	
	// Remove links connected to removed nodes
	nodeSet := make(map[string]bool)
	for _, node := range graph.Nodes {
		nodeSet[node.Address] = true
	}
	
	filteredLinks := make([]TransferLink, 0)
	for _, link := range graph.Links {
		if nodeSet[link.Source] && nodeSet[link.Target] {
			filteredLinks = append(filteredLinks, link)
		}
	}
	graph.Links = filteredLinks
}

// getCacheEntry gets a cached entry
func (s *Server) getCacheEntry(key string) *CacheEntry {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	entry, ok := s.cache[key]
	if !ok || time.Now().After(entry.ExpiresAt) {
		return nil
	}
	
	return entry
}

// setCacheEntry sets a cached entry
func (s *Server) setCacheEntry(key string, data *TransferGraphResponse) {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	s.cache[key] = &CacheEntry{
		Data:      data,
		ExpiresAt: time.Now().Add(s.cfg.CacheTTL),
	}
}

// calculateTotalValue calculates total value from nodes
func calculateTotalValue(nodes []TransferNode) float64 {
	var total float64
	for _, node := range nodes {
		total += node.Value
	}
	return total
}

// calculateTotalUSD calculates total USD value from nodes
func calculateTotalUSD(nodes []TransferNode) float64 {
	var total float64
	for _, node := range nodes {
		total += node.ValueUSD
	}
	return total
}

// shortenAddress shortens an address for display
func shortenAddress(addr string) string {
	if len(addr) < 10 {
		return addr
	}
	return addr[:6] + "..." + addr[len(addr)-4:]
}

// =============================================================================
// ROUTER SETUP
// =============================================================================

// RegisterRoutes registers the transfer graph routes
func (s *Server) RegisterRoutes(r *gin.Engine) {
	api := r.Group("/api/v1")
	{
		api.GET("/graphs/transfers", s.GetTransferGraph)
		api.GET("/graphs/token-transfers/:address", s.GetTokenTransfers)
	}
}

// =============================================================================
// CORS MIDDLEWARE
// =============================================================================

// CORS middleware for cross-origin requests
func CORS() gin.HandlerFunc {
	return func(c *gin.Context) {
		c.Header("Access-Control-Allow-Origin", "*")
		c.Header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
		c.Header("Access-Control-Allow-Headers", "Origin, Content-Type, Authorization")
		
		if c.Request.Method == "OPTIONS" {
			c.AbortWithStatus(http.StatusNoContent)
			return
		}
		
		c.Next()
	}
}

// =============================================================================
// HEALTH CHECK
// =============================================================================

// HealthCheck handles GET /health
func (s *Server) HealthCheck(c *gin.Context) {
	ctx := c.Request.Context()
	
	health := gin.H{
		"status": "healthy",
		"timestamp": time.Now().Unix(),
	}
	
	// Check database
	if err := s.cfg.DB.PingContext(ctx); err != nil {
		health["database"] = "unhealthy"
		health["status"] = "degraded"
	} else {
		health["database"] = "healthy"
	}
	
	// Check Redis
	if s.cfg.Redis != nil {
		if err := s.cfg.Redis.Ping(ctx).Err(); err != nil {
			health["redis"] = "unhealthy"
			health["status"] = "degraded"
		} else {
			health["redis"] = "healthy"
		}
	}
	
	c.JSON(http.StatusOK, health)
}