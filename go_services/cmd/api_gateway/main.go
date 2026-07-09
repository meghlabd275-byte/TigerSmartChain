package main

import (
	"context"
	"log"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/tigersmartchain/go_services/internal/gateway"
)

func main() {
	// Initialize Gin
	gin.SetMode(gin.ReleaseMode)
	router := gin.New()
	router.Use(gin.Recovery())
	router.Use(gin.Logger())

	// Load configuration
	config := gateway.LoadConfig()

	// Initialize middleware
	middleware := gateway.NewMiddleware(config)
	router.Use(middleware.CORS())
	router.Use(middleware.RateLimiter())
	router.Use(middleware.RequestLogger())

	// Initialize handlers
	handler := gateway.NewHandler(config)

	// Setup routes
	setupRoutes(router, handler)

	// Create HTTP server
	srv := &http.Server{
		Addr:           ":" + config.Port,
		Handler:        router,
		ReadTimeout:    30 * time.Second,
		WriteTimeout:   30 * time.Second,
		MaxHeaderBytes: 1 << 20,
	}

	// Start server in goroutine
	go func() {
		log.Printf("Starting API Gateway on port %s", config.Port)
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Fatalf("Failed to start server: %v", err)
		}
	}()

	// Wait for interrupt signal
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit
	log.Println("Shutting down server...")

	// Graceful shutdown
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := srv.Shutdown(ctx); err != nil {
		log.Fatal("Server forced to shutdown:", err)
	}

	log.Println("Server exited")
}

func setupRoutes(router *gin.Engine, h *gateway.Handler) {
	api := router.Group("/api/v1")
	{
		// Health check
		api.GET("/health", h.HealthCheck)

		// Blocks
		api.GET("/blocks/latest", h.GetLatestBlock)
		api.GET("/blocks/:number", h.GetBlock)
		api.GET("/blocks/:number/transactions", h.GetBlockTransactions)
		api.GET("/blocks", h.GetBlocks)

		// Transactions
		api.GET("/txs/:hash", h.GetTransaction)
		api.GET("/txs", h.GetTransactions)
		api.GET("/txs/pending", h.GetPendingTransactions)
		api.GET("/txs/:hash/internal", h.GetInternalTransactions)
		api.GET("/txs/:hash/trace", h.GetTrace)

		// Tokens
		api.GET("/tokens", h.GetTokens)
		api.GET("/tokens/:address", h.GetToken)
		api.GET("/tokens/:address/holders", h.GetTokenHolders)
		api.GET("/tokens/:address/transfers", h.GetTokenTransfers)
		api.GET("/tokens/:address/price-history", h.GetTokenPriceHistory)

		// NFTs
		api.GET("/nfts", h.GetNFTCollections)
		api.GET("/nfts/:address", h.GetNFTCollection)
		api.GET("/nfts/:address/tokens/:tokenId", h.GetNFTToken)
		api.GET("/nfts/:address/transfers", h.GetNFTTransfers)
		api.GET("/nfts/:address/floor", h.GetNFTFloorPrice)

		// Contracts
		api.GET("/contracts/:address", h.GetContract)
		api.GET("/contracts/:address/code", h.GetContractCode)
		api.GET("/contracts/:address/storage/:slot", h.GetStorageAt)
		api.POST("/contracts/verify", h.VerifyContract)

		// Addresses
		api.GET("/addresses/:address", h.GetAddress)
		api.GET("/addresses/:address/tokens", h.GetAddressTokens)
		api.GET("/addresses/:address/nfts", h.GetAddressNFTs)

		// Analytics
		api.GET("/stats/network", h.GetNetworkStats)
		api.GET("/charts/transactions", h.GetTransactionChart)
		api.GET("/charts/addresses", h.GetAddressChart)
		api.GET("/gas/oracle", h.GetGasOracle)

		// Search
		api.GET("/search", h.Search)
		api.GET("/search/advanced", h.AdvancedSearch)

		// Labels
		api.GET("/labels", h.GetLabels)
		api.GET("/labels/:address", h.GetAddressLabel)

		// DEX
		api.GET("/dex/pairs", h.GetDexPairs)
		api.GET("/dex/pairs/:address", h.GetDexPair)

		// Governance
		api.GET("/governance/proposals", h.GetGovernanceProposals)
		api.GET("/governance/proposals/:id", h.GetGovernanceProposal)
	}

	// WebSocket
	router.GET("/ws", h.HandleWebSocket)
}
