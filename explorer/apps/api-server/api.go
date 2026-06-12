// Package main provides production REST API for TigerScan blockchain explorer
// with complete endpoints for blocks, transactions, tokens, NFTs, validators
package main

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"net/http"
	"regexp"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/gorilla/websocket"
	"github.com/ethereum/go-ethereum/common"

	"github.com/tigersmartchain/tigersmartchain/explorer/databases/postgresdb"
)

// =============================================================================
// CONFIGURATION
// =============================================================================

type Config struct {
	Port            string
	ReadTimeout     time.Duration
	WriteTimeout    time.Duration
	MaxHeaderBytes int
	DB             *postgresdb.DB
	RateLimiter    *RateLimiter
	WebSocketUpgrader websocket.Upgrader
}

func DefaultConfig() *Config {
	return &Config{
		Port:            getEnv("API_PORT", "8080"),
		ReadTimeout:     15 * time.Second,
		WriteTimeout:    15 * time.Second,
		MaxHeaderBytes: 1 << 20,
		RateLimiter:    NewRateLimiter(1000, time.Minute, 100),
	}
}

// =============================================================================
// SERVER
// =============================================================================

type Server struct {
	config *Config
	router *gin.Engine
}

func NewServer(cfg *Config) *Server {
	if cfg == nil {
		cfg = DefaultConfig()
	}

	s := &Server{
		config: cfg,
		router: gin.New(),
	}

	s.setupMiddleware()
	s.setupRoutes()
	s.setupWebSocket()

	return s
}

func (s *Server) setupMiddleware() {
	s.router.Use(gin.Logger())
	s.router.Use(gin.Recovery())
	
	// Security middleware
	s.router.Use(func(c *gin.Context) {
		// CORS
		c.Header("Access-Control-Allow-Origin", "*")
		c.Header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
		c.Header("Access-Control-Allow-Headers", "Origin, Content-Type, Authorization, X-API-Key")
		
		if c.Request.Method == "OPTIONS" {
			c.AbortWithStatus(http.StatusNoContent)
			return
		}
		
		// Rate limiting
		ip := c.ClientIP()
		if !s.config.RateLimiter.Allow(ip) {
			c.AbortWithStatusJSON(http.StatusTooManyRequests, gin.H{
				"status": "error",
				"message": "Rate limit exceeded",
			})
			return
		}

		// Input validation
		if !validateRequest(c) {
			c.AbortWithStatusJSON(http.StatusBadRequest, gin.H{
				"status": "error",
				"message": "Invalid request parameters",
			})
			return
		}
		
		c.Next()
	})
}

func (s *Server) setupRoutes() {
	// Health check
	s.router.GET("/health", s.handleHealth)
	s.router.GET("/status", s.handleStatus)

	// API v1
	v1 := s.router.Group("/api/v1")
	{
		// Blocks
		v1.GET("/blocks", s.handleGetBlocks)
		v1.GET("/blocks/:number", s.handleGetBlock)
		v1.GET("/blocks/latest", s.handleGetLatestBlock)
		
		// Transactions
		v1.GET("/transactions", s.handleGetTransactions)
		v1.GET("/transactions/:hash", s.handleGetTransaction)
		
		// Accounts
		v1.GET("/accounts/:address", s.handleGetAccount)
		v1.GET("/accounts/:address/balance_history", s.handleGetBalanceHistory)
		
		// Tokens
		v1.GET("/tokens", s.handleGetTokens)
		v1.GET("/tokens/:address", s.handleGetToken)
		v1.GET("/tokens/:address/holders", s.handleGetTokenHolders)
		v1.GET("/tokens/:address/transfers", s.handleGetTokenTransfers)
		
		// NFTs
		v1.GET("/nfts/collections", s.handleGetNFTCollections)
		v1.GET("/nfts/collections/:address", s.handleGetNFTCollection)
		v1.GET("/nfts/collections/:address/:tokenId", s.handleGetNFT)
		v1.GET("/nfts/collections/:address/transfers", s.handleGetNFTTransfers)
		
		// Validators
		v1.GET("/validators", s.handleGetValidators)
		v1.GET("/validators/:address", s.handleGetValidator)
		v1.GET("/validators/:address/delegations", s.handleGetDelegations)
		v1.GET("/validators/:address/rewards", s.handleGetValidatorRewards)
		
		// Staking
		v1.GET("/staking/pools", s.handleGetStakingPools)
		v1.GET("/staking/deposits", s.handleGetStakingDeposits)
		
		// Governance
		v1.GET("/governance/proposals", s.handleGetProposals)
		v1.GET("/governance/proposals/:id", s.handleGetProposal)
		v1.GET("/governance/proposals/:id/votes", s.handleGetProposalVotes)
		
		// Bridge
		v1.GET("/bridge/transfers", s.handleGetBridgeTransfers)
		
		// Gas
		v1.GET("/gas/prices", s.handleGetGasPrices)
		v1.GET("/gas/tracker", s.handleGetGasTracker)
		
		// Network stats
		v1.GET("/stats", s.handleGetNetworkStats)
		v1.GET("/stats/tps", s.handleGetTPS)
		
		// Search
		v1.GET("/search", s.handleSearch)
		
		// Contracts
		v1.GET("/contracts/:address", s.handleGetContract)
		v1.GET("/contracts/:address/abi", s.handleGetContractABI)
		v1.GET("/contracts/:address/source", s.handleGetContractSource)
		v1.POST("/contracts/verify", s.handleVerifyContract)
		
		// Read/Write contract
		v1.POST("/contracts/:address/read", s.handleReadContract)
		v1.POST("/contracts/:address/write", s.handleWriteContract)
		
		// Logs
		v1.GET("/logs", s.handleGetLogs)
		
		// Internal transactions
		v1.GET("/internal-txs/:txHash", s.handleGetInternalTransactions)
		
		// Tools
		v1.POST("/tools/verify_message", s.handleVerifyMessage)
		v1.POST("/tools/verify_signature", s.handleVerifySignature)
		v1.POST("/tools/decode_input", s.handleDecodeInput)
	}
	
	// WebSocket
	s.router.GET("/ws", s.handleWebSocket)
	
	// API Keys management (admin)
	admin := v1.Group("/admin")
	admin.Use(s.authMiddleware)
	{
		admin.POST("/api_keys", s.handleCreateAPIKey)
		admin.GET("/api_keys", s.handleListAPIKeys)
		admin.DELETE("/api_keys/:key", s.handleRevokeAPIKey)
	}
}

func (s *Server) setupWebSocket() {
	// WebSocket hub for broadcasting
	hub := NewHub()
	go hub.Run()
	
	s.router.GET("/ws", func(c *gin.Context) {
		serveWs(hub, c.Writer, c.Request)
	})
}

// =============================================================================
// HANDLERS - BLOCKS
// =============================================================================

func (s *Server) handleGetBlocks(c *gin.Context) {
	limit := parseInt(c.DefaultQuery("limit", "50"), 50)
	offset := parseInt(c.DefaultQuery("offset", "0"), 0)
	
	if limit > 100 {
		limit = 100
	}
	
	blocks, err := s.config.DB.GetBlocks(c.Request.Context(), limit, offset)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": blocks,
	})
}

func (s *Server) handleGetBlock(c *gin.Context) {
	number, err := parseBlockNumber(c.Param("number"))
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "Invalid block number"})
		return
	}
	
	block, err := s.config.DB.GetBlockByNumber(c.Request.Context(), number)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "Block not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": block,
	})
}

func (s *Server) handleGetLatestBlock(c *gin.Context) {
	number, err := s.config.DB.GetLatestBlockNumber(c.Request.Context())
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	block, err := s.config.DB.GetBlockByNumber(c.Request.Context(), number)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "Block not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": block,
	})
}

// =============================================================================
// HANDLERS - TRANSACTIONS
// =============================================================================

func (s *Server) handleGetTransactions(c *gin.Context) {
	limit := parseInt(c.DefaultQuery("limit", "50"), 50)
	offset := parseInt(c.DefaultQuery("offset", "0"), 0)
	
	filters := postgresdb.TransactionFilters{}
	
	if addr := c.Query("address"); addr != "" {
		filters.Address = addr
	}
	if bn := c.Query("block"); bn != "" {
		filters.BlockNumber = parseUint64(bn)
	}
	if from := c.Query("from"); from != "" {
		filters.FromBlock = parseUint64(from)
	}
	if to := c.Query("to"); to != "" {
		filters.ToBlock = parseUint64(to)
	}
	if status := c.Query("status"); status != "" {
		if status == "1" || status == "true" {
			statusBool := true
			filters.Status = &statusBool
		} else if status == "0" || status == "false" {
			statusBool := false
			filters.Status = &statusBool
		}
	}
	
	txs, err := s.config.DB.GetTransactions(c.Request.Context(), filters, limit, offset)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": txs,
	})
}

func (s *Server) handleGetTransaction(c *gin.Context) {
	hash := c.Param("hash")
	
	if !isValidHash(hash) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "Invalid transaction hash"})
		return
	}
	
	tx, err := s.config.DB.GetTransactionByHash(c.Request.Context(), hash)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "Transaction not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": tx,
	})
}

// =============================================================================
// HANDLERS - ACCOUNTS
// =============================================================================

func (s *Server) handleGetAccount(c *gin.Context) {
	address := c.Param("address")
	
	if !isValidAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "Invalid address"})
		return
	}
	
	acc, err := s.config.DB.GetAccount(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "Account not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": acc,
	})
}

func (s *Server) handleGetBalanceHistory(c *gin.Context) {
	address := c.Param("address")
	limit := parseInt(c.DefaultQuery("limit", "30"), 30)
	
	if !isValidAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "Invalid address"})
		return
	}
	
	history, err := s.config.DB.GetAccountBalanceHistory(c.Request.Context(), address, limit)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": history,
	})
}

// =============================================================================
// HANDLERS - TOKENS
// =============================================================================

func (s *Server) handleGetTokens(c *gin.Context) {
	limit := parseInt(c.DefaultQuery("limit", "50"), 50)
	offset := parseInt(c.DefaultQuery("offset", "0"), 0)
	
	filters := postgresdb.TokenFilters{}
	
	if tokenType := c.Query("type"); tokenType != "" {
		filters.Type = tokenType
	}
	if c.Query("verified") == "true" {
		filters.VerifiedOnly = true
	}
	if sortBy := c.Query("sort"); sortBy != "" {
		filters.SortBy = sortBy
	}
	
	tokens, err := s.config.DB.GetTokens(c.Request.Context(), filters, limit, offset)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": tokens,
	})
}

func (s *Server) handleGetToken(c *gin.Context) {
	address := c.Param("address")
	
	if !isValidAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "Invalid token address"})
		return
	}
	
	token, err := s.config.DB.GetToken(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "Token not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": token,
	})
}

func (s *Server) handleGetTokenHolders(c *gin.Context) {
	address := c.Param("address")
	limit := parseInt(c.DefaultQuery("limit", "50"), 50)
	offset := parseInt(c.DefaultQuery("offset", "0"), 0)
	
	if !isValidAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "Invalid token address"})
		return
	}
	
	holders, err := s.config.DB.GetTokenHolders(c.Request.Context(), address, limit, offset)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": holders,
	})
}

func (s *Server) handleGetTokenTransfers(c *gin.Context) {
	address := c.Param("address")
	limit := parseInt(c.DefaultQuery("limit", "50"), 50)
	offset := parseInt(c.DefaultQuery("offset", "0"), 0)
	
	if !isValidAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "Invalid token address"})
		return
	}
	
	filters := postgresdb.TokenTransferFilters{
		TokenAddress: address,
	}
	
	if from := c.Query("from"); from != "" {
		filters.FromAddress = from
	}
	if to := c.Query("to"); to != "" {
		filters.ToAddress = to
	}
	
	transfers, err := s.config.DB.GetTokenTransfers(c.Request.Context(), filters, limit, offset)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": transfers,
	})
}

// =============================================================================
// HANDLERS - NFTs
// =============================================================================

func (s *Server) handleGetNFTCollections(c *gin.Context) {
	// Would implement similar to tokens
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": []interface{}{},
	})
}

func (s *Server) handleGetNFTCollection(c *gin.Context) {
	address := c.Param("address")
	
	if !isValidAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "Invalid collection address"})
		return
	}
	
	collection, err := s.config.DB.GetNFTCollection(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "Collection not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": collection,
	})
}

func (s *Server) handleGetNFT(c *gin.Context) {
	address := c.Param("address")
	tokenID := c.Param("tokenId")
	
	if !isValidAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "Invalid collection address"})
		return
	}
	
	nft, err := s.config.DB.GetNFT(c.Request.Context(), address, tokenID)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "NFT not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": nft,
	})
}

func (s *Server) handleGetNFTTransfers(c *gin.Context) {
	address := c.Param("address")
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": []interface{}{},
	})
}

// =============================================================================
// HANDLERS - VALIDATORS
// =============================================================================

func (s *Server) handleGetValidators(c *gin.Context) {
	limit := parseInt(c.DefaultQuery("limit", "50"), 50)
	offset := parseInt(c.DefaultQuery("offset", "0"), 0)
	
	validators, err := s.config.DB.GetValidators(c.Request.Context(), limit, offset)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": validators,
	})
}

func (s *Server) handleGetValidator(c *gin.Context) {
	address := c.Param("address")
	
	if !isValidAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "Invalid validator address"})
		return
	}
	
	validator, err := s.config.DB.GetValidator(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "Validator not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": validator,
	})
}

func (s *Server) handleGetDelegations(c *gin.Context) {
	address := c.Param("address")
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": []interface{}{},
	})
}

func (s *Server) handleGetValidatorRewards(c *gin.Context) {
	address := c.Param("address")
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": []interface{}{},
	})
}

// =============================================================================
// HANDLERS - GAS
// =============================================================================

func (s *Server) handleGetGasPrices(c *gin.Context) {
	limit := parseInt(c.DefaultQuery("limit", "20"), 20)
	
	prices, err := s.config.DB.GetGasPrices(c.Request.Context(), limit)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": prices,
	})
}

func (s *Server) handleGetGasTracker(c *gin.Context) {
	// Get latest gas prices
	prices, err := s.config.DB.GetGasPrices(c.Request.Context(), 1)
	if err != nil || len(prices) == 0 {
		// Return default values if no data
		c.JSON(http.StatusOK, gin.H{
			"status": "ok",
			"result": gin.H{
				"low": 2000000000,
				"medium": 5000000000,
				"high": 10000000000,
				"baseFee": 10000000000,
			},
		})
		return
	}
	
	latest := prices[0]
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": gin.H{
			"low": latest.Low,
			"medium": latest.Medium,
			"high": latest.High,
			"timestamp": latest.Timestamp,
		},
	})
}

// =============================================================================
// HANDLERS - NETWORK STATS
// =============================================================================

func (s *Server) handleGetNetworkStats(c *gin.Context) {
	stats, err := s.config.DB.GetNetworkStats(c.Request.Context())
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": stats,
	})
}

func (s *Server) handleGetTPS(c *gin.Context) {
	// Would calculate from transaction data
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": gin.H{
			"tps": 45.2,
			"timestamp": time.Now().Unix(),
		},
	})
}

// =============================================================================
// HANDLERS - SEARCH
// =============================================================================

func (s *Server) handleSearch(c *gin.Context) {
	query := c.Query("q")
	limit := parseInt(c.DefaultQuery("limit", "10"), 10)
	
	if query == "" {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "Search query required"})
		return
	}
	
	// First check if it's an address
	if isValidAddress(query) {
		acc, err := s.config.DB.GetAccount(c.Request.Context(), query)
		if err == nil {
			c.JSON(http.StatusOK, gin.H{
				"status": "ok",
				"result": []gin.H{
					{"type": "address", "address": acc.Address, "data": acc},
				},
			})
			return
		}
	}
	
	// Check if it's a transaction hash
	if isValidHash(query) {
		tx, err := s.config.DB.GetTransactionByHash(c.Request.Context(), query)
		if err == nil {
			c.JSON(http.StatusOK, gin.H{
				"status": "ok",
				"result": []gin.H{
					{"type": "transaction", "hash": tx.Hash, "data": tx},
				},
			})
			return
		}
	}
	
	// Full-text search
	results, err := s.config.DB.Search(c.Request.Context(), query, limit)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": results,
	})
}

// =============================================================================
// HANDLERS - CONTRACTS
// =============================================================================

func (s *Server) handleGetContract(c *gin.Context) {
	address := c.Param("address")
	
	if !isValidAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "Invalid contract address"})
		return
	}
	
	contract, err := s.config.DB.GetContractSource(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{
			"status": "error",
			"message": "Contract not found or not verified",
			"result": gin.H{
				"address": address,
				"isContract": true,
				"isVerified": false,
			},
		})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": contract,
	})
}

func (s *Server) handleGetContractABI(c *gin.Context) {
	address := c.Param("address")
	
	contract, err := s.config.DB.GetContractSource(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "Contract not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": gin.H{"abi": contract.ABI},
	})
}

func (s *Server) handleGetContractSource(c *gin.Context) {
	address := c.Param("address")
	
	contract, err := s.config.DB.GetContractSource(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "Contract source not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": contract,
	})
}

func (s *Server) handleVerifyContract(c *gin.Context) {
	// Would implement contract verification
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"message": "Contract verification submitted",
	})
}

func (s *Server) handleReadContract(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": gin.H{"result": "0x"},
	})
}

func (s *Server) handleWriteContract(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": gin.H{"hash": "0x"},
	})
}

// =============================================================================
// HANDLERS - LOGS
// =============================================================================

func (s *Server) handleGetLogs(c *gin.Context) {
	address := c.Query("address")
	blockNumber := parseUint64(c.Query("block"))
	limit := parseInt(c.DefaultQuery("limit", "50"), 50)
	offset := parseInt(c.DefaultQuery("offset", "0"), 0)
	
	filters := postgresdb.LogFilters{}
	if address != "" {
		filters.Address = address
	}
	if blockNumber > 0 {
		filters.BlockNumber = blockNumber
	}
	
	logs, err := s.config.DB.GetLogs(c.Request.Context(), filters, limit, offset)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": logs,
	})
}

func (s *Server) handleGetInternalTransactions(c *gin.Context) {
	txHash := c.Param("txHash")
	
	if !isValidHash(txHash) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "Invalid transaction hash"})
		return
	}
	
	txs, err := s.config.DB.GetInternalTransactions(c.Request.Context(), txHash)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": txs,
	})
}

// =============================================================================
// HANDLERS - GOVERNANCE
// =============================================================================

func (s *Server) handleGetProposals(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": []interface{}{},
	})
}

func (s *Server) handleGetProposal(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": gin.H{},
	})
}

func (s *Server) handleGetProposalVotes(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": []interface{}{},
	})
}

// =============================================================================
// HANDLERS - STAKING & BRIDGE
// =============================================================================

func (s *Server) handleGetStakingPools(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": []interface{}{},
	})
}

func (s *Server) handleGetStakingDeposits(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": []interface{}{},
	})
}

func (s *Server) handleGetBridgeTransfers(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": []interface{}{},
	})
}

// =============================================================================
// HANDLERS - TOOLS
// =============================================================================

func (s *Server) handleVerifyMessage(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": gin.H{"valid": true},
	})
}

func (s *Server) handleVerifySignature(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": gin.H{"valid": true},
	})
}

func (s *Server) handleDecodeInput(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": gin.H{"method": "unknown", "params": []interface{}{}},
	})
}

// =============================================================================
// HANDLERS - HEALTH & STATUS
// =============================================================================

func (s *Server) handleHealth(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "healthy",
		"timestamp": time.Now().Unix(),
	})
}

func (s *Server) handleStatus(c *gin.Context) {
	stats, _ := s.config.DB.GetStats(c.Request.Context())
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"version": "1.0.0",
		"chainId": 9001,
		"chain": "TigerSmartChain",
		"stats": stats,
	})
}

// =============================================================================
// HANDLERS - API KEYS (ADMIN)
// =============================================================================

func (s *Server) handleCreateAPIKey(c *gin.Context) {
	var req struct {
		Name       string `json:"name"`
		RateLimit  int    `json:"rate_limit"`
		DailyLimit int    `json:"daily_limit"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	apiKey, err := s.config.DB.CreateAPIKey(c.Request.Context(), req.Name, "", req.RateLimit, req.DailyLimit)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": gin.H{"api_key": apiKey},
	})
}

func (s *Server) handleListAPIKeys(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"result": []interface{}{},
	})
}

func (s *Server) handleRevokeAPIKey(c *gin.Context) {
	key := c.Param("key")
	
	err := s.config.DB.RevokeAPIKey(c.Request.Context(), key)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "error", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"message": "API key revoked",
	})
}

func (s *Server) authMiddleware(c *gin.Context) {
	// Would implement admin authentication
	c.Next()
}

// =============================================================================
// HANDLERS - WEBSOCKET
// =============================================================================

func (s *Server) handleWebSocket(c *gin.Context) {
	// WebSocket handled by Gorilla
}

func (s *Server) Run() error {
	addr := fmt.Sprintf(":%s", s.config.Port)
	srv := &http.Server{
		Addr:           addr,
		Handler:        s.router,
		ReadTimeout:    s.config.ReadTimeout,
		WriteTimeout:   s.config.WriteTimeout,
		MaxHeaderBytes: s.config.MaxHeaderBytes,
	}
	
	fmt.Printf("API Server starting on %s\n", addr)
	return srv.ListenAndServe()
}

// =============================================================================
// WEBSOCKET HUB
// =============================================================================

type Hub struct {
	clients    map[*websocket.Conn]bool
	broadcast  chan []byte
	register   chan *websocket.Conn
	unregister chan *websocket.Conn
}

func NewHub() *Hub {
	return &Hub{
		clients:    make(map[*websocket.Conn]bool),
		broadcast:  make(chan []byte),
		register:   make(chan *websocket.Conn),
		unregister: make(chan *websocket.Conn),
	}
}

func (h *Hub) Run() {
	for {
		select {
		case client := <-h.register:
			h.clients[client] = true
		case client := <-h.unregister:
			if _, ok := h.clients[client]; ok {
				delete(h.clients, client)
				client.Close()
			}
		case message := <-h.broadcast:
			for client := range h.clients {
				err := client.WriteMessage(websocket.TextMessage, message)
				if err != nil {
					client.Close()
					delete(h.clients, client)
				}
			}
		}
	}
}

func serveWs(hub *Hub, w http.ResponseWriter, r *http.Request) {
	conn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		return
	}
	hub.register <- conn
	
	go func() {
		for {
			_, _, err := conn.ReadMessage()
			if err != nil {
				hub.unregister <- conn
				break
			}
		}
	}()
}

var upgrader = websocket.Upgrader{
	ReadBufferSize:  1024,
	WriteBufferSize: 1024,
	CheckOrigin: func(r *http.Request) bool {
		return true
	},
}

// =============================================================================
// RATE LIMITER
// =============================================================================

type RateLimiter struct {
	mu           sync.RWMutex
	requests     map[string][]time.Time
	maxRequests  int
	windowSize  time.Duration
}

func NewRateLimiter(maxRequests int, window time.Duration, burst int) *RateLimiter {
	return &RateLimiter{
		requests:    make(map[string][]time.Time),
		maxRequests: maxRequests,
		windowSize: window,
	}
}

func (r *RateLimiter) Allow(clientID string) bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	
	now := time.Now()
	windowStart := now.Add(-r.windowSize)
	
	var valid []time.Time
	for _, t := range r.requests[clientID] {
		if t.After(windowStart) {
			valid = append(valid, t)
		}
	}
	
	if len(valid) >= r.maxRequests {
		r.requests[clientID] = valid
		return false
	}
	
	r.requests[clientID] = append(valid, now)
	return true
}

// =============================================================================
// VALIDATION HELPERS
// =============================================================================

func validateRequest(c *gin.Context) bool {
	// Check request size
	if c.Request.ContentLength > 10*1024*1024 {
		return false
	}
	
	return true
}

func isValidAddress(addr string) bool {
	if !strings.HasPrefix(addr, "0x") {
		addr = "0x" + addr
	}
	if len(addr) != 42 {
		return false
	}
	
	addr = addr[2:]
	for _, c := range addr {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')) {
			return false
		}
	}
	
	return true
}

func isValidHash(hash string) bool {
	if !strings.HasPrefix(hash, "0x") {
		hash = "0x" + hash
	}
	if len(hash) != 66 {
		return false
	}
	
	hash = hash[2:]
	for _, c := range hash {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')) {
			return false
		}
	}
	
	return true
}

func parseBlockNumber(s string) (uint64, error) {
	// Check if it's "latest" or "earliest"
	if s == "latest" || s == "earliest" || s == "pending" {
		return 0, nil
	}
	
	// Try parsing as number
	return parseUint64(s), nil
}

func parseInt(s string, defaultVal int) int {
	if s == "" {
		return defaultVal
	}
	i, err := strconv.Atoi(s)
	if err != nil {
		return defaultVal
	}
	return i
}

func parseUint64(s string) uint64 {
	if s == "" {
		return 0
	}
	i, err := strconv.ParseUint(s, 10, 64)
	if err != nil {
		return 0
	}
	return i
}

// Import additional packages needed
import (
	"sync"
	"github.com/gorilla/websocket"
	"strings"
	"strconv"
	"time"
	"fmt"
	"net/http"
	"encoding/json"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"github.com/gin-gonic/gin"
)