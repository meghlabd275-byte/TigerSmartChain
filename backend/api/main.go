// Package main is the TigerScan API server.
package main

import (
	"fmt"
	"log"
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
)

const (
	Version = "1.0.0"
	Port    = 8080
)

func main() {
	log.Printf("TigerScan API v%s starting...", Version)

	// Setup router
	r := setupRouter()

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

func setupRouter() *gin.Engine {
	r := gin.Default()

	// CORS middleware
	r.Use(func(c *gin.Context) {
		c.Header("Access-Control-Allow-Origin", "*")
		c.Header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
		c.Next()
	})

	// Health check
	r.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{
			"status":    "healthy",
			"version":  Version,
			"timestamp": time.Now().Unix(),
		})
	})

	// API v1 routes
	v1 := r.Group("/api/v1")
	{
		// Block endpoints
		blocks := v1.Group("/blocks")
		{
			blocks.GET("", handleGetBlocks)
			blocks.GET("/latest", handleGetLatestBlock)
		}

		// Transaction endpoints
		txs := v1.Group("/transactions")
		{
			txs.GET("", handleGetTransactions)
		}

		// Account endpoints
		accounts := v1.Group("/accounts")
		{
			accounts.GET("/:address", handleGetAccount)
		}

		// Token endpoints
		tokens := v1.Group("/tokens")
		{
			tokens.GET("", handleGetTokens)
		}

		// Validator endpoints
		validators := v1.Group("/validators")
		{
			validators.GET("", handleGetValidators)
		}

		// Analytics endpoints
		analytics := v1.Group("/analytics")
		{
			analytics.GET("/stats", handleGetStats)
			analytics.GET("/tps", handleGetTPS)
			analytics.GET("/gas", handleGetGasAnalytics)
		}

		// Search endpoints
		search := v1.Group("/search")
		{
			search.GET("", handleSearch)
		}
	}

	return r
}

func handleGetBlocks(c *gin.Context) {
	c.JSON(http.StatusOK, []interface{}{
		map[string]interface{}{"number": 1, "hash": "0x1", "timestamp": time.Now().Unix()},
	})
}

func handleGetLatestBlock(c *gin.Context) {
	c.JSON(http.StatusOK, map[string]interface{}{
		"number":   1,
		"hash":    "0x1",
		"gasUsed": 15000000,
		"gasLimit": 30000000,
	})
}

func handleGetTransactions(c *gin.Context) {
	c.JSON(http.StatusOK, []interface{}{})
}

func handleGetAccount(c *gin.Context) {
	addr := c.Param("address")
	c.JSON(http.StatusOK, map[string]interface{}{
		"address":   addr,
		"balance":  "0",
		"txCount":  0,
	})
}

func handleGetTokens(c *gin.Context) {
	c.JSON(http.StatusOK, []interface{}{})
}

func handleGetValidators(c *gin.Context) {
	c.JSON(http.StatusOK, []interface{}{})
}

func handleGetStats(c *gin.Context) {
	c.JSON(http.StatusOK, map[string]interface{}{
		"totalBlocks":       0,
		"totalTransactions": 0,
		"totalAccounts":    0,
		"totalTokens":      0,
	})
}

func handleGetTPS(c *gin.Context) {
	now := time.Now().Unix()
	c.JSON(http.StatusOK, []map[string]interface{}{
		{"timestamp": now - 300, "value": 45.2},
		{"timestamp": now, "value": 49.2},
	})
}

func handleGetGasAnalytics(c *gin.Context) {
	now := time.Now().Unix()
	c.JSON(http.StatusOK, []map[string]interface{}{
		{"timestamp": now, "low": 2000000000, "medium": 5000000000, "high": 10000000000},
	})
}

func handleSearch(c *gin.Context) {
	query := c.Query("q")
	if query == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Search query required"})
		return
	}
	c.JSON(http.StatusOK, []map[string]interface{}{
		{"type": "address", "id": query},
	})
}