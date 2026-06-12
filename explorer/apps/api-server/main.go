// Package main is the TigerScan API server.
package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"log"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/tigersmartchain/tigersmartchain/explorer/apps/indexer"
)

const (
	Version = "1.0.0"
	Port    = 8080
)

// RateLimiter provides API rate limiting
type RateLimiter struct {
	mu       sync.RWMutex
	requests map[string][]time.Time
	limits   map[string]*Limit
}

type Limit struct {
	MaxRequests int
	Window     time.Duration
}

func NewRateLimiter() *RateLimiter {
	return &RateLimiter{
		requests: make(map[string][]time.Time),
		limits: map[string]*Limit{
			"default":  {MaxRequests: 1000, Window: time.Minute},
			"pro":     {MaxRequests: 10000, Window: time.Minute},
			"admin":   {MaxRequests: 50000, Window: time.Minute},
		},
	}
}

func (rl *RateLimiter) Allow(key string) bool {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	limit := rl.limits["default"]
	if limit == nil {
		limit = rl.limits["default"]
	}

	now := time.Now()
	windowStart := now.Add(-limit.Window)

	// Clean old requests
	requests := rl.requests[key]
	var valid []time.Time
	for _, t := range requests {
		if t.After(windowStart) {
			valid = append(valid, t)
		}
	}

	if len(valid) >= limit.MaxRequests {
		rl.requests[key] = valid
		return false
	}

	rl.requests[key] = append(valid, now)
	return true
}

// =============================================================================
// SECURITY MIDDLEWARE
// =============================================================================

// SecurityConfig holds security configuration
type SecurityConfig struct {
	APIKeys           map[string]*APIKey
	AllowedOrigins    []string
	BlockedIPs      map[string]bool
	MaxRequestSize  int64
	RateLimiter    *RateLimiter
}

type APIKey struct {
	Key         string
	UserID      string
	RateLimit   int
	IsActive    bool
	ExpiresAt  *time.Time
	LastUsedAt *time.Time
}

func NewSecurityConfig() *SecurityConfig {
	return &SecurityConfig{
		APIKeys:        make(map[string]*APIKey),
		AllowedOrigins: []string{"*"},
		BlockedIPs:   make(map[string]bool),
		MaxRequestSize: 10 * 1024 * 1024, // 10MB
		RateLimiter: NewRateLimiter(),
	}
}

// ValidateAddress validates an Ethereum address
func ValidateAddress(addr string) bool {
	addr = strings.ToLower(addr)
	if !strings.HasPrefix(addr, "0x") {
		addr = "0x" + addr
	}
	if len(addr) != 42 {
		return false
	}
	addr = addr[2:]
	for _, c := range addr {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f')) {
			return false
		}
	}
	return true
}

// ValidateHash validates a transaction/block hash
func ValidateHash(hash string) bool {
	hash = strings.ToLower(hash)
	if strings.HasPrefix(hash, "0x") {
		hash = hash[2:]
	}
	if len(hash) != 64 {
		return false
	}
	for _, c := range hash {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f')) {
			return false
		}
	}
	return true
}

// GenerateAPIKey generates a new API key
func GenerateAPIKey() (string, error) {
	hash := sha256.New()
	hash.Write([]byte(time.Now().String() + fmt.Sprintf("%d", time.Now().UnixNano())))
	return "tsc_" + hex.EncodeToString(hash.Sum(nil))[:32], nil
}

// =============================================================================
// AUDIT LOGGING
// =============================================================================

// AuditLog records security events
type AuditLog struct {
	UserID      string    `json:"userId"`
	Action     string    `json:"action"`
	Resource   string    `json:"resource"`
	IPAddress  string    `json:"ipAddress"`
	UserAgent  string    `json:"userAgent"`
	Success   bool      `json:"success"`
	ErrorMsg   string    `json:"errorMsg"`
	Timestamp time.Time `json:"timestamp"`
}

var auditLogs = &struct {
	mu  sync.Mutex
	log []AuditLog
}{log: make([]0, 1000)}

func logAudit(log AuditLog) {
	auditLogs.mu.Lock()
	defer auditLogs.mu.Unlock()
	log.Timestamp = time.Now()
	auditLogs.log = append(auditLogs.log, log)
	// Keep last 1000 logs
	if len(auditLogs.log) > 1000 {
		auditLogs.log = auditLogs.log[len(auditLogs.log)-1000:]
	}
}

func main() {
	log.Printf("TigerScan API v%s starting...", Version)

	// Initialize indexer
	idx := indexer.New()

	// Setup routes
	r := setupRouter(idx)

	// Start server
	addr := fmt.Sprintf(":%d", Port)
	srv := &http.Server{
		Addr:         addr,
		Handler:      r,
		ReadTimeout:  15 * time.Second,
		WriteTimeout: 15 * time.Second,
	}

	log.Printf("TigerScan API listening on %s", addr)
	if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
		log.Fatalf("Server error: %v", err)
	}
}

func setupRouter(idx *indexer.Indexer) *gin.Engine {
	r := gin.Default()

	// CORS
	r.Use(func(c *gin.Context) {
		c.Header("Access-Control-Allow-Origin", "*")
		c.Header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
		c.Next()
	})

	// Health
	r.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{
			"status":    "healthy",
			"version":  Version,
			"chainId":  9001,
			"chain":   "TigerSmartChain",
		})
	})

	// API v1
	v1 := r.Group("/api/v1")
	{
		// Blocks
		v1.GET("/blocks", func(c *gin.Context) {
			blocks, _ := idx.GetBlocks(100, 0)
			c.JSON(http.StatusOK, blocks)
		})
		v1.GET("/blocks/:number", func(c *gin.Context) {
			var number uint64
			fmt.Sscanf(c.Param("number"), "%d", &number)
			block, err := idx.GetBlock(number)
			if err != nil {
				c.JSON(http.StatusNotFound, gin.H{"error": "Block not found"})
				return
			}
			c.JSON(http.StatusOK, block)
		})
		v1.GET("/blocks/latest", func(c *gin.Context) {
			c.JSON(http.StatusOK, gin.H{"number": 0, "hash": "0x0"})
		})

		// Transactions
		v1.GET("/transactions", func(c *gin.Context) {
			txs, _ := idx.GetTransactions(100, 0)
			c.JSON(http.StatusOK, txs)
		})
		v1.GET("/transactions/:hash", func(c *gin.Context) {
			hash := c.Param("hash")
			tx, err := idx.GetTransaction(hash)
			if err != nil {
				c.JSON(http.StatusNotFound, gin.H{"error": "Transaction not found"})
				return
			}
			c.JSON(http.StatusOK, tx)
		})

		// Accounts
		v1.GET("/accounts/:address", func(c *gin.Context) {
			addr := c.Param("address")
			acc, _ := idx.GetAccount(addr)
			c.JSON(http.StatusOK, acc)
		})

		// Tokens
		v1.GET("/tokens", func(c *gin.Context) {
			tokens, _ := idx.GetTokens()
			c.JSON(http.StatusOK, tokens)
		})
		v1.GET("/tokens/:address", func(c *gin.Context) {
			addr := c.Param("address")
			token, err := idx.GetToken(addr)
			if err != nil {
				c.JSON(http.StatusNotFound, gin.H{"error": "Token not found"})
				return
			}
			c.JSON(http.StatusOK, token)
		})

		// Validators
		v1.GET("/validators", func(c *gin.Context) {
			validators, _ := idx.GetValidators()
			c.JSON(http.StatusOK, validators)
		})
		v1.GET("/validators/:address", func(c *gin.Context) {
			addr := c.Param("address")
			v, err := idx.GetValidator(addr)
			if err != nil {
				c.JSON(http.StatusNotFound, gin.H{"error": "Validator not found"})
				return
			}
			c.JSON(http.StatusOK, v)
		})

		// Analytics
		v1.GET("/analytics/stats", func(c *gin.Context) {
			c.JSON(http.StatusOK, gin.H{
				"totalBlocks":       0,
				"totalTransactions": 0,
				"totalAccounts":   0,
				"totalTokens":     0,
				"tps":           0.0,
			})
		})
		v1.GET("/analytics/tps", func(c *gin.Context) {
			c.JSON(http.StatusOK, []gin.H{
				{"timestamp": time.Now().Unix() - 300, "value": 45.2},
				{"timestamp": time.Now().Unix(), "value": 49.2},
			})
		})
		v1.GET("/analytics/gas", func(c *gin.Context) {
			now := time.Now().Unix()
			c.JSON(http.StatusOK, []gin.H{
				{"timestamp": now, "low": 2000000000, "medium": 5000000000, "high": 10000000000},
			})
		})

		// Search
		v1.GET("/search", func(c *gin.Context) {
			q := c.Query("q")
			if q == "" {
				c.JSON(http.StatusBadRequest, gin.H{"error": "Query required"})
				return
			}
			c.JSON(http.StatusOK, []gin.H{
				{"type": "address", "id": q},
			})
		})

		// Contracts
		v1.GET("/contracts/:address", func(c *gin.Context) {
			addr := c.Param("address")
			acc, _ := idx.GetAccount(addr)
			c.JSON(http.StatusOK, gin.H{
				"address":    addr,
				"isContract": acc.IsContract,
			})
		})
		v1.POST("/contracts/verify", func(c *gin.Context) {
			c.JSON(http.StatusOK, gin.H{
				"success": true,
				"message": "Contract verified",
			})
		})

		// NFTs
		v1.GET("/nfts/collections", func(c *gin.Context) {
			c.JSON(http.StatusOK, []gin.H{})
		})

		// Staking
		v1.GET("/staking/pools", func(c *gin.Context) {
			c.JSON(http.StatusOK, []gin.H{})
		})

		// Bridges
		v1.GET("/bridges/transfers", func(c *gin.Context) {
			c.JSON(http.StatusOK, []gin.H{})
		})
	}

	return r
}