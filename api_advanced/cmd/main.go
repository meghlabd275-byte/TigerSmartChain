package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"math/big"
	"net/http"
	"regexp"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/go-redis/redis/v8"
	"github.com/jackc/pgx/v4"
	"golang.org/x/time/rate"
)

// ============================================
// Advanced API Configuration
// ============================================

type Config struct {
	Port           string
	DatabaseURL    string
	RedisURL       string
	JWTSecret      string
	RateLimitRPS   float64
	RateLimitBurst int
	CacheTTL       time.Duration
}

var (
	cfg          *Config
	db           *pgx.Conn
	redisClient  *redis.Client
	rateLimiter  *rate.Limiter
)

// ============================================
// Cursor-based Pagination
// ============================================

type Cursor struct {
	BlockNumber uint64 `json:"block_number"`
	Index      uint64 `json:"index"`
	Hash       string `json:"hash"`
}

type PageInfo struct {
	HasNextPage     bool   `json:"hasNextPage"`
	HasPreviousPage bool   `json:"hasPreviousPage"`
	StartCursor    string `json:"startCursor"`
	EndCursor     string `json:"endCursor"`
}

type PaginatedResponse struct {
	Data       interface{} `json:"data"`
	PageInfo  PageInfo   `json:"pageInfo"`
	TotalCount int64     `json:"totalCount"`
}

// Encode cursor to base64
func encodeCursor(c *Cursor) string {
	data := fmt.Sprintf("%d:%d:%s", c.BlockNumber, c.Index, c.Hash)
	hash := sha256.Sum256([]byte(data))
	return hex.EncodeToString(hash[:])
}

// Decode cursor from base64
func decodeCursor(encoded string) (*Cursor, error) {
	data, err := hex.DecodeString(encoded)
	if err != nil {
		return nil, err
	}
	
	var c Cursor
	_, err = fmt.Sscanf(string(data), "%d:%d:%s", &c.BlockNumber, &c.Index, &c.Hash)
	if err != nil {
		return nil, err
	}
	
	return &c, nil
}

// ============================================
// Advanced API Handlers
// ============================================

// Get blocks with cursor pagination
func GetBlocks(c *gin.Context) {
	// Parse pagination params
	limit := 50
	if l := c.Query("limit"); l != "" {
		if parsed, err := strconv.Atoi(l); err == nil && parsed > 0 && parsed <= 100 {
			limit = parsed
		}
	}
	
	cursor := c.Query("cursor")
	direction := c.DefaultQuery("direction", "next") // next or previous
	
	// Build query
	var query string
	var args []interface{}
	
	if cursor == "" {
		if direction == "next" {
			query = `SELECT number, hash, parent_hash, miner, gas_limit, gas_used, timestamp, size, tx_count
				FROM blocks ORDER BY number DESC LIMIT $1`
			args = []interface{}{limit + 1}
		} else {
			query = `SELECT number, hash, parent_hash, miner, gas_limit, gas_used, timestamp, size, tx_count
				FROM blocks ORDER BY number ASC LIMIT $1`
			args = []interface{}{limit + 1}
		}
	} else {
		// Decode cursor
		cur, err := decodeCursor(cursor)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid cursor"})
			return
		}
		
		if direction == "next" {
			query = `SELECT number, hash, parent_hash, miner, gas_limit, gas_used, timestamp, size, tx_count
				FROM blocks WHERE number < $1 ORDER BY number DESC LIMIT $2`
			args = []interface{}{cur.BlockNumber, limit + 1}
		} else {
			query = `SELECT number, hash, parent_hash, miner, gas_limit, gas_used, timestamp, size, tx_count
				FROM blocks WHERE number > $1 ORDER BY number ASC LIMIT $2`
			args = []interface{}{cur.BlockNumber, limit + 1}
		}
	}
	
	// Execute query
	rows, err := db.Query(context.Background(), query, args...)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	var blocks []map[string]interface{}
	for rows.Next() {
		var number, gasLimit, gasUsed, timestamp, size, txCount uint64
		var hash, parentHash, miner string
		
		err := rows.Scan(&number, &hash, &parentHash, &miner, &gasLimit, &gasUsed, &timestamp, &size, &txCount)
		if err != nil {
			continue
		}
		
		blocks = append(blocks, map[string]interface{}{
			"number":         number,
			"hash":           hash,
			"parentHash":    parentHash,
			"miner":         miner,
			"gasLimit":      gasLimit,
			"gasUsed":       gasUsed,
			"timestamp":     timestamp,
			"size":          size,
			"transactionCount": txCount,
		})
	}
	
	// Check if more pages exist
	hasMore := len(blocks) > limit
	if hasMore {
		blocks = blocks[:limit]
	}
	
	// Build response
	pageInfo := PageInfo{
		HasNextPage:     hasMore,
		HasPreviousPage: cursor != "",
	}
	
	if len(blocks) > 0 {
		first := blocks[0]
		last := blocks[len(blocks)-1]
		pageInfo.StartCursor = encodeCursor(&Cursor{
			BlockNumber: first["number"].(uint64),
			Hash:        first["hash"].(string),
		})
		pageInfo.EndCursor = encodeCursor(&Cursor{
			BlockNumber: last["number"].(uint64),
			Hash:        last["hash"].(string),
		})
	}
	
	// Get total count
	var total int64
	db.QueryRow(context.Background(), "SELECT COUNT(*) FROM blocks").Scan(&total)
	
	c.JSON(http.StatusOK, PaginatedResponse{
		Data:       blocks,
		PageInfo:  pageInfo,
		TotalCount: total,
	})
}

// Get transactions with advanced filtering
func GetTransactions(c *gin.Context) {
	limit := 50
	if l := c.Query("limit"); l != "" {
		if parsed, err := strconv.Atoi(l); err == nil && parsed > 0 && parsed <= 100 {
			limit = parsed
		}
	}
	
	// Build filters
	filters := []string{"1=1"}
	args := []interface{}{}
	argNum := 1
	
	// Address filter (from or to)
	if addr := c.Query("address"); addr != "" {
		filters = append(filters, fmt.Sprintf("(from_address = $%d OR to_address = $%d)", argNum, argNum))
		args = append(args, strings.ToLower(addr))
		argNum++
	}
	
	// Block range
	if from := c.Query("fromBlock"); from != "" {
		if block, err := strconv.ParseUint(from, 10, 64); err == nil {
			filters = append(filters, fmt.Sprintf("block_number >= $%d", argNum))
			args = append(args, block)
			argNum++
		}
	}
	
	if to := c.Query("toBlock"); to != "" {
		if block, err := strconv.ParseUint(to, 10, 64); err == nil {
			filters = append(filters, fmt.Sprintf("block_number <= $%d", argNum))
			args = append(args, block)
			argNum++
		}
	}
	
	// Value filter
	if minVal := c.Query("minValue"); minVal != "" {
		if val, ok := new(big.Int).SetString(minVal, 10); ok {
			filters = append(filters, fmt.Sprintf("value >= $%d", argNum))
			args = append(args, val.String())
			argNum++
		}
	}
	
	// Status filter
	if status := c.Query("status"); status != "" {
		filters = append(filters, fmt.Sprintf("status = $%d", argNum))
		args = append(args, status == "success")
		argNum++
	}
	
	// Method ID filter
	if methodID := c.Query("methodID"); methodID != "" {
		filters = append(filters, fmt.Sprintf("LEFT(input, 10) = $%d", argNum))
		args = append(args, methodID)
		argNum++
	}
	
	// Build query
	whereClause := strings.Join(filters, " AND ")
	query := fmt.Sprintf(`
		SELECT hash, block_number, block_hash, timestamp, from_address, to_address, 
		       value, gas_price, gas_used, input, status
		FROM transactions 
		WHERE %s 
		ORDER BY block_number DESC, hash
		LIMIT $%d
	`, whereClause, argNum)
	args = append(args, limit+1)
	
	rows, err := db.Query(context.Background(), query, args...)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	var txs []map[string]interface{}
	for rows.Next() {
		var hash, from, to, input, blockHash string
		var blockNumber, timestamp, gasPrice, gasUsed uint64
		var value *big.Int
		var status bool
		
		err := rows.Scan(&hash, &blockNumber, &blockHash, &timestamp, &from, &to, &value, &gasPrice, &gasUsed, &input, &status)
		if err != nil {
			continue
		}
		
		txs = append(txs, map[string]interface{}{
			"hash":         hash,
			"blockNumber":  blockNumber,
			"blockHash":    blockHash,
			"timestamp":    timestamp,
			"from":         from,
			"to":           to,
			"value":        value.String(),
			"gasPrice":     gasPrice,
			"gasUsed":      gasUsed,
			"input":        input,
			"status":       status,
		})
	}
	
	hasMore := len(txs) > limit
	if hasMore {
		txs = txs[:limit]
	}
	
	c.JSON(http.StatusOK, gin.H{
		"data": txs,
		"pageInfo": gin.H{
			"hasNextPage": hasMore,
		},
	})
}

// Get token transfers
func GetTokenTransfers(c *gin.Context) {
	limit := 50
	if l := c.Query("limit"); l != "" {
		if parsed, err := strconv.Atoi(l); err == nil && parsed > 0 && parsed <= 100 {
			limit = parsed
		}
	}
	
	filters := []string{"1=1"}
	args := []interface{}{}
	argNum := 1
	
	// Token address
	if token := c.Query("token"); token != "" {
		filters = append(filters, fmt.Sprintf("token_address = $%d", argNum))
		args = append(args, strings.ToLower(token))
		argNum++
	}
	
	// Address
	if addr := c.Query("address"); addr != "" {
		filters = append(filters, fmt.Sprintf("(from_address = $%d OR to_address = $%d)", argNum, argNum))
		args = append(args, strings.ToLower(addr))
		argNum++
	}
	
	whereClause := strings.Join(filters, " AND ")
	query := fmt.Sprintf(`
		SELECT transaction_hash, block_number, timestamp, from_address, to_address, 
		       token_address, value, log_index
		FROM token_transfers 
		WHERE %s 
		ORDER BY block_number DESC, log_index DESC
		LIMIT $%d
	`, whereClause, argNum)
	args = append(args, limit+1)
	
	rows, err := db.Query(context.Background(), query, args...)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	var transfers []map[string]interface{}
	for rows.Next() {
		var txHash, from, to, tokenAddr string
		var blockNumber, timestamp, logIndex uint64
		var value *big.Int
		
		err := rows.Scan(&txHash, &blockNumber, &timestamp, &from, &to, &tokenAddr, &value, &logIndex)
		if err != nil {
			continue
		}
		
		transfers = append(transfers, map[string]interface{}{
			"hash":           txHash,
			"blockNumber":    blockNumber,
			"timestamp":      timestamp,
			"from":           from,
			"to":             to,
			"tokenAddress":   tokenAddr,
			"value":          value.String(),
			"logIndex":       logIndex,
		})
	}
	
	hasMore := len(transfers) > limit
	if hasMore {
		transfers = transfers[:limit]
	}
	
	c.JSON(http.StatusOK, gin.H{
		"data":       transfers,
		"pageInfo":  gin.H{"hasNextPage": hasMore},
	})
}

// Get contract ABI
func GetContractABI(c *gin.Context) {
	address := c.Param("address")
	
	// Check cache first
	cacheKey := "contract:abi:" + address
	if cached, err := redisClient.Get(context.Background(), cacheKey).Result(); err == nil {
		c.JSON(http.StatusOK, gin.H{"abi": cached})
		return
	}
	
	// Query database
	var abi string
	err := db.QueryRow(context.Background(), 
		"SELECT abi FROM contracts WHERE address = $1", 
		strings.ToLower(address),
	).Scan(&abi)
	
	if err == pgx.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{"error": "Contract not found"})
		return
	}
	
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	// Cache result
	redisClient.Set(context.Background(), cacheKey, abi, time.Hour)
	
	c.JSON(http.StatusOK, gin.H{"abi": abi})
}

// Batch query handler
func BatchQuery(c *gin.Context) {
	var requests []map[string]interface{}
	if err := c.ShouldBindJSON(&requests); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	// Limit batch size
	if len(requests) > 10 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Maximum 10 requests per batch"})
		return
	}
	
	results := make([]interface{}, len(requests))
	
	for i, req := range requests {
		queryType, _ := req["type"].(string)
		
		switch queryType {
		case "block":
			if num, ok := req["blockNumber"].(float64); ok {
				results[i] = getBlockByNumber(uint64(num))
			}
		case "transaction":
			if hash, ok := req["hash"].(string); ok {
				results[i] = getTransactionByHash(hash)
			}
		case "address":
			if addr, ok := req["address"].(string); ok {
				results[i] = getAddressInfo(addr)
			}
		}
	}
	
	c.JSON(http.StatusOK, gin.H{"results": results})
}

// Helper functions
func getBlockByNumber(num uint64) map[string]interface{} {
	var hash, parentHash, miner string
	var gasLimit, gasUsed, timestamp, size, txCount uint64
	
	err := db.QueryRow(context.Background(), `
		SELECT hash, parent_hash, miner, gas_limit, gas_used, timestamp, size, tx_count
		FROM blocks WHERE number = $1
	`, num).Scan(&hash, &parentHash, &miner, &gasLimit, &gasUsed, &timestamp, &size, &txCount)
	
	if err != nil {
		return nil
	}
	
	return map[string]interface{}{
		"number":     num,
		"hash":       hash,
		"parentHash": parentHash,
		"miner":      miner,
		"gasLimit":   gasLimit,
		"gasUsed":    gasUsed,
		"timestamp":  timestamp,
		"size":       size,
		"txCount":    txCount,
	}
}

func getTransactionByHash(hash string) map[string]interface{} {
	var from, to, input, blockHash string
	var blockNumber, timestamp, gasPrice, gasUsed uint64
	var value *big.Int
	var status bool
	
	err := db.QueryRow(context.Background(), `
		SELECT from_address, to_address, value, gas_price, gas_used, input, status, block_number, block_hash, timestamp
		FROM transactions WHERE hash = $1
	`, hash).Scan(&from, &to, &value, &gasPrice, &gasUsed, &input, &status, &blockNumber, &blockHash, &timestamp)
	
	if err != nil {
		return nil
	}
	
	return map[string]interface{}{
		"hash":         hash,
		"from":         from,
		"to":           to,
		"value":        value.String(),
		"gasPrice":     gasPrice,
		"gasUsed":      gasUsed,
		"input":        input,
		"status":       status,
		"blockNumber":  blockNumber,
		"blockHash":    blockHash,
		"timestamp":    timestamp,
	}
}

func getAddressInfo(address string) map[string]interface{} {
	var balance *big.Int
	var nonce uint64
	var codeHash string
	var isContract bool
	
	err := db.QueryRow(context.Background(), `
		SELECT balance, nonce, code_hash, is_contract 
		FROM addresses WHERE address = $1
	`, strings.ToLower(address)).Scan(&balance, &nonce, &codeHash, &isContract)
	
	if err != nil {
		return nil
	}
	
	return map[string]interface{}{
		"address":     address,
		"balance":     balance.String(),
		"nonce":      nonce,
		"codeHash":   codeHash,
		"isContract": isContract,
	}
}

// ============================================
// Rate Limiting Middleware
// ============================================

func RateLimiter() gin.HandlerFunc {
	return func(c *gin.Context) {
		if !rateLimiter.Allow() {
			c.JSON(http.StatusTooManyRequests, gin.H{
				"error": "Rate limit exceeded",
				"retryAfter": 60,
			})
			c.Abort()
			return
		}
		c.Next()
	}
}

// ============================================
// JWT Authentication Middleware
// ============================================

func JWTAuth() gin.HandlerFunc {
	return func(c *gin.Context) {
		token := c.GetHeader("Authorization")
		if token == "" {
			c.JSON(http.StatusUnauthorized, gin.H{"error": "Authorization header required"})
			c.Abort()
			return
		}
		
		// Remove "Bearer " prefix
		token = strings.TrimPrefix(token, "Bearer ")
		
		// Validate JWT (simplified)
		if !validateJWT(token) {
			c.JSON(http.StatusUnauthorized, gin.H{"error": "Invalid token"})
			c.Abort()
			return
		}
		
		c.Next()
	}
}

func validateJWT(token string) bool {
	// Simplified JWT validation
	// In production, use proper JWT library
	return len(token) > 20
}

// ============================================
// Main
// ============================================

func main() {
	// Initialize configuration
	cfg = &Config{
		Port:           getEnv("PORT", "8080"),
		DatabaseURL:    getEnv("DATABASE_URL", "postgres://localhost:5432/tigersmartchain"),
		RedisURL:       getEnv("REDIS_URL", "redis://localhost:6379"),
		JWTSecret:      getEnv("JWT_SECRET", "secret"),
		RateLimitRPS:   100,
		RateLimitBurst: 200,
		CacheTTL:       time.Minute * 5,
	}
	
	// Initialize database
	var err error
	db, err = pgx.Connect(context.Background(), cfg.DatabaseURL)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Database connection failed: %v\n", err)
		os.Exit(1)
	}
	defer db.Close(context.Background())
	
	// Initialize Redis
	redisClient = redis.NewClient(&redis.Options{
		Addr:     strings.TrimPrefix(cfg.RedisURL, "redis://"),
		Password: "",
		DB:       0,
	})
	
	// Initialize rate limiter
	rateLimiter = rate.NewLimiter(rate.Limit(cfg.RateLimitRPS), cfg.RateLimitBurst)
	
	// Setup Gin
	gin.SetMode(gin.ReleaseMode)
	r := gin.New()
	r.Use(gin.Recovery())
	r.Use(gin.Logger())
	
	// Public routes
	public := r.Group("/api/v1")
	public.Use(RateLimiter())
	{
		// Blocks
		public.GET("/blocks", GetBlocks)
		public.GET("/blocks/:number", GetBlockByNumber)
		
		// Transactions
		public.GET("/transactions", GetTransactions)
		public.GET("/transactions/:hash", GetTransactionByHash)
		
		// Token transfers
		public.GET("/token/transfers", GetTokenTransfers)
		
		// Contracts
		public.GET("/contracts/:address", GetContractABI)
		
		// Search
		public.GET("/search", Search)
	}
	
	// Authenticated routes
	authenticated := r.Group("/api/v1")
	authenticated.Use(RateLimiter())
	authenticated.Use(JWTAuth())
	{
		// Batch queries
		authenticated.POST("/batch", BatchQuery)
		
		// Historical data
		authenticated.GET("/history/balance", GetBalanceHistory)
		authenticated.GET("/history/nonce", GetNonceHistory)
	}
	
	// Health check
	r.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{
			"status": "healthy",
			"timestamp": time.Now().Unix(),
		})
	})
	
	fmt.Printf("Starting API server on port %s\n", cfg.Port)
	r.Run(":" + cfg.Port)
}

func getEnv(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}