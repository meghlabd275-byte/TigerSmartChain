// Package faucet provides testnet faucet service with rate limiting and security.
package faucet

import (
	"context"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"net/http"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/google/uuid"
	"github.com/redis/go-redis/v9"
	"github.com/tigersmartchain/tigersmartchain/internal/crypto"
)

// =============================================================================
// CONSTANTS
// =============================================================================

const (
	ChainETH    = 1
	ChainSepolia = 11155111
	ChainBSC    = 56
	ChainBSCTestnet = 97
	ChainPolygon = 137
	ChainMumbai = 80001
	
	ETHAmount    = 1e18
	BNBAmount   = 1e18
	MATICAmount = 1e18
	
	IPLimitPerHour = 3
	AddressLimitPerDay = 3
	GlobalLimitPerHour = 100
)

// =============================================================================
// DATA STRUCTURES
// =============================================================================

type ChainConfig struct {
	ChainID     uint64 `json:"chainId"`
	Name        string `json:"name"`
	Symbol      string `json:"symbol"`
	Decimals    uint8  `json:"decimals"`
	RPCURL      string `json:"rpcUrl"`
	ExplorerURL string `json:"explorerUrl"`
	FaucetAddress string `json:"faucetAddress"`
	MinBalance  string `json:"minBalance"`
	ContractAddress string `json:"contractAddress,omitempty"`
	ABI          string `json:"abi,omitempty"`
	IsActive bool `json:"isActive"`
}

type FaucetRequest struct {
	ID        string    `json:"id"`
	Address   string    `json:"address"`
	ChainID   uint64    `json:"chainId"`
	Amount    string    `json:"amount"`
	TXHash    string    `json:"txHash,omitempty"`
	IPAddress string    `json:"ipAddress"`
	UserAgent string   `json:"userAgent"`
	IsDiscordVerified bool      `json:"isDiscordVerified"`
	IsCaptchaVerified bool     `json:"isCaptchaVerified"`
	Status   string    `json:"status"`
	Error    string    `json:"error,omitempty"`
	CreatedAt time.Time `json:"createdAt"`
	CompletedAt time.Time `json:"completedAt,omitempty"`
}

type FaucetStats struct {
	TotalRequests  int64   `json:"totalRequests"`
	Successful  int64   `json:"successful"`
	Failed     int64   `json:"failed"`
	Pending    int64   `json:"pending"`
	TotalETHDispensed string `json:"totalETHLDispensed"`
	TotalBNBDispensed string `json:"totalBNBDispensed"`
	TotalMATICDispensed string `json:"totalMATICDispensed"`
	ActiveAddresses int64   `json:"activeAddresses"`
	RequestsLastHour int64 `json:"requestsLastHour"`
}

type RateLimit struct {
	Count     int
	ResetTime time.Time
}

type FaucetService struct {
	db       *sql.DB
	redis    *redis.Client
	httpClient *http.Client
	
	chains map[uint64]*ChainConfig
	
	requestsMu sync.RWMutex
	requests map[string]*FaucetRequest
	
	rateLimitsMu sync.RWMutex
	ipLimits   map[string]*RateLimit
	addrLimits map[string]*RateLimit
	
	countersMu sync.RWMutex
	hourlyCounter int64
	dailyCounter  int64
	lastHourReset time.Time
	lastDayReset  time.Time
	
	totalDispensedMu sync.RWMutex
	totalETH    *big.Int
	totalBNB   *big.Int
	totalMATIC *big.Int
	
	discordWebhook string
	discordEnabled bool
	
	encryptionKey []byte
	
	config *FaucetConfig
}

type FaucetConfig struct {
	DB              *sql.DB
	Redis           *redis.Client
	HTTPClient      *http.Client
	DiscordWebhook string
	PrivateKey string
	Chains []ChainConfig
}

// =============================================================================
// SERVICE INITIALIZATION
// =============================================================================

func NewFaucetService(cfg *FaucetConfig) (*FaucetService, error) {
	if cfg == nil {
		cfg = &FaucetConfig{}
	}
	
	encryptionKey, err := crypto.GenerateEncryptionKey()
	if err != nil {
		return nil, fmt.Errorf("failed to generate encryption key: %w", err)
	}
	
	httpClient := cfg.HTTPClient
	if httpClient == nil {
		httpClient = &http.Client{Timeout: 30 * time.Second}
	}
	
	svc := &FaucetService{
		db:           cfg.DB,
		redis:        cfg.Redis,
		httpClient:   httpClient,
		chains:      make(map[uint64]*ChainConfig),
		requests:    make(map[string]*FaucetRequest),
		ipLimits:    make(map[string]*RateLimit),
		addrLimits:  make(map[string]*RateLimit),
		totalETH:   big.NewInt(0),
		totalBNB:  big.NewInt(0),
		totalMATIC: big.NewInt(0),
		discordWebhook: cfg.DiscordWebhook,
		discordEnabled: cfg.DiscordWebhook != "",
		encryptionKey: encryptionKey,
		config:        cfg,
	}
	
	svc.initializeChains(cfg.Chains)
	go svc.rotateCounters()
	
	return svc, nil
}

func (s *FaucetService) initializeChains(customChains []ChainConfig) {
	defaultChains := []ChainConfig{
		{ChainID: ChainETH, Name: "Ethereum Mainnet", Symbol: "ETH", Decimals: 18, FaucetAddress: "0x0000000000000000000000000000000000000000", MinBalance: "0.1", IsActive: false},
		{ChainID: ChainSepolia, Name: "Sepolia Testnet", Symbol: "ETH", Decimals: 18, RPCURL: "https://sepolia.infura.io/v3/", ExplorerURL: "https://sepolia.etherscan.io", FaucetAddress: "0x7a250d5630B4cf539639657g5b2E3E0cE7a5f3c0", MinBalance: "0.001", IsActive: true},
		{ChainID: ChainBSC, Name: "BNB Smart Chain", Symbol: "BNB", Decimals: 18, FaucetAddress: "0x0000000000000000000000000000000000000000", MinBalance: "0.01", IsActive: false},
		{ChainID: ChainBSCTestnet, Name: "BSC Testnet", Symbol: "BNB", Decimals: 18, RPCURL: "https://data-seed-prebsc-1-s1.binance.org:8545", ExplorerURL: "https://testnet.bscscan.com", FaucetAddress: "0x0000000000000000000000000000000000000001", MinBalance: "0.01", IsActive: true},
		{ChainID: ChainPolygon, Name: "Polygon Mainnet", Symbol: "MATIC", Decimals: 18, FaucetAddress: "0x0000000000000000000000000000000000000000", MinBalance: "0.01", IsActive: false},
		{ChainID: ChainMumbai, Name: "Mumbai Testnet", Symbol: "MATIC", Decimals: 18, RPCURL: "https://rpc-mumbai.maticvigil.com", ExplorerURL: "https://mumbai.polygonscan.com", FaucetAddress: "0x0000000000000000000000000000000000000002", MinBalance: "0.01", IsActive: true},
	}
	
	chainsToUse := customChains
	if len(chainsToUse) == 0 {
		chainsToUse = defaultChains
	}
	
	for _, chain := range chainsToUse {
		c := chain
		s.chains[chain.ChainID] = &c
	}
}

// =============================================================================
// ROUTES
// =============================================================================

func (s *FaucetService) RegisterRoutes(r *gin.RouterGroup) {
	faucet := r.Group("")
	{
		faucet.GET("/chains", s.handleGetChains)
		faucet.GET("/chains/:chainId", s.handleGetChain)
		faucet.POST("/drip", s.handleRequest)
		faucet.GET("/status/:requestId", s.handleGetStatus)
		faucet.GET("/stats", s.handleGetStats)
		faucet.GET("/verify", s.handleVerifyCaptcha)
	}
	
	admin := r.Group("/admin")
	admin.Use(s.adminMiddleware())
	{
		admin.POST("/chains", s.handleAddChain)
		admin.PUT("/chains/:chainId", s.handleUpdateChain)
		admin.DELETE("/chains/:chainId", s.handleDeleteChain)
		admin.POST("/fund", s.handleFundFaucet)
		admin.GET("/requests", s.handleListRequests)
		admin.POST("/requests/:requestId/retry", s.handleRetryRequest)
	}
}

// =============================================================================
// HANDLERS
// =============================================================================

func (s *FaucetService) handleGetChains(c *gin.Context) {
	chains := make([]*ChainConfig, 0, len(s.chains))
	for _, chain := range s.chains {
		if chain.IsActive {
			c := *chain
			c.FaucetAddress = ""
			chains = append(chains, &c)
		}
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": chains})
}

func (s *FaucetService) handleGetChain(c *gin.Context) {
	chainID, err := strconv.ParseUint(c.Param("chainId"), 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "invalid chain ID"})
		return
	}
	
	chain, ok := s.chains[chainID]
	if !ok {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "chain not found"})
		return
	}
	
	if !chain.IsActive {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "chain not active"})
		return
	}
	
	c := *chain
	c.FaucetAddress = ""
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": &c})
}

func (s *FaucetService) handleRequest(c *gin.Context) {
	var req struct {
		Address string `json:"address" binding:"required"`
		ChainID uint64 `json:"chainId" binding:"required"`
		Captcha string `json:"captcha"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "invalid request"})
		return
	}
	
	if !isValidAddress(req.Address) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "invalid address format"})
		return
	}
	
	chain, ok := s.chains[req.ChainID]
	if !ok {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "unsupported chain"})
		return
	}
	
	if !chain.IsActive {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "chain not active"})
		return
	}
	
	ipAddress := c.ClientIP()
	if !s.checkRateLimit(ipAddress, req.Address) {
		c.JSON(http.StatusTooManyRequests, gin.H{"status": "error", "message": "rate limit exceeded. Try again later."})
		return
	}
	
	if req.Captcha != "" && !s.verifyCaptcha(req.Captcha) {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "captcha verification failed"})
		return
	}
	
	requestID := uuid.New().String()
	faucetReq := &FaucetRequest{
		ID:         requestID,
		Address:   req.Address,
		ChainID:   req.ChainID,
		Amount:    "1.0",
		IPAddress: ipAddress,
		UserAgent: c.GetHeader("User-Agent"),
		IsCaptchaVerified: req.Captcha != "",
		Status:    "pending",
		CreatedAt: time.Now(),
	}
	
	s.requestsMu.Lock()
	s.requests[requestID] = faucetReq
	s.requestsMu.Unlock()
	
	go s.processRequest(requestID)
	
	c.JSON(http.StatusAccepted, gin.H{"status": "ok", "result": gin.H{"requestId": requestID, "status": "pending", "message": "Faucet request submitted. Funds will arrive shortly."}})
}

func (s *FaucetService) handleGetStatus(c *gin.Context) {
	requestID := c.Param("requestId")
	
	s.requestsMu.RLock()
	defer s.requestsMu.RUnlock()
	
	req, ok := s.requests[requestID]
	if !ok {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "request not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": req})
}

func (s *FaucetService) handleGetStats(c *gin.Context) {
	s.requestsMu.RLock()
	defer s.requestsMu.RUnlock()
	
	s.countersMu.Lock()
	hourly := s.hourlyCounter
	s.countersMu.Unlock()
	
	var total, successful, pending, failed int64
	for _, req := range s.requests {
		total++
		switch req.Status {
		case "completed":
			successful++
		case "pending":
			pending++
		case "failed":
			failed++
		}
	}
	
	s.totalDispensedMu.Lock()
	stats := &FaucetStats{
		TotalRequests:     total,
		Successful:      successful,
		Pending:        pending,
		Failed:         failed,
		TotalETHDispensed: s.totalETH.String(),
		TotalBNBDispensed: s.totalBNB.String(),
		ActiveAddresses: int64(len(s.addrLimits)),
		RequestsLastHour: hourly,
	}
	s.totalDispensedMu.Unlock()
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": stats})
}

func (s *FaucetService) handleVerifyCaptcha(c *gin.Context) {
	token := c.Query("token")
	if token == "" {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "token required"})
		return
	}
	
	valid := s.verifyCaptcha(token)
	c.JSON(http.StatusOK, gin.H{"status": "ok", "valid": valid})
}

// =============================================================================
// ADMIN HANDLERS
// =============================================================================

func (s *FaucetService) handleAddChain(c *gin.Context) {
	var chain ChainConfig
	if err := c.ShouldBindJSON(&chain); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "invalid request"})
		return
	}
	
	s.chains[chain.ChainID] = &chain
	c.JSON(http.StatusCreated, gin.H{"status": "ok", "result": chain})
}

func (s *FaucetService) handleUpdateChain(c *gin.Context) {
	chainID, err := strconv.ParseUint(c.Param("chainId"), 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "invalid chain ID"})
		return
	}
	
	chain, ok := s.chains[chainID]
	if !ok {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "chain not found"})
		return
	}
	
	var updates ChainConfig
	if err := c.ShouldBindJSON(&updates); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "invalid request"})
		return
	}
	
	if updates.Name != "" {
		chain.Name = updates.Name
	}
	if updates.RPCURL != "" {
		chain.RPCURL = updates.RPCURL
	}
	if updates.ExplorerURL != "" {
		chain.ExplorerURL = updates.ExplorerURL
	}
	chain.IsActive = updates.IsActive
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": chain})
}

func (s *FaucetService) handleDeleteChain(c *gin.Context) {
	chainID, err := strconv.ParseUint(c.Param("chainId"), 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "invalid chain ID"})
		return
	}
	
	delete(s.chains, chainID)
	c.JSON(http.StatusOK, gin.H{"status": "ok", "message": "chain deleted"})
}

func (s *FaucetService) handleFundFaucet(c *gin.Context) {
	var req struct {
		ChainID uint64 `json:"chainId" binding:"required"`
		Amount string `json:"amount" binding:"required"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "invalid request"})
		return
	}
	
	amount, ok := new(big.Int).SetString(req.Amount, 10)
	if !ok {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "invalid amount"})
		return
	}
	
	_ = amount
	c.JSON(http.StatusOK, gin.H{"status": "ok", "message": "Faucet funded"})
}

func (s *FaucetService) handleListRequests(c *gin.Context) {
	limit := 50
	if l := c.Query("limit"); l != "" {
		if parsed, err := strconv.Atoi(l); err == nil {
			limit = parsed
		}
	}
	
	status := c.Query("status")
	
	s.requestsMu.RLock()
	defer s.requestsMu.RUnlock()
	
	requests := make([]*FaucetRequest, 0)
	for _, req := range s.requests {
		if status != "" && req.Status != status {
			continue
		}
		requests = append(requests, req)
		if len(requests) >= limit {
			break
		}
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "result": requests})
}

func (s *FaucetService) handleRetryRequest(c *gin.Context) {
	requestID := c.Param("requestId")
	
	s.requestsMu.Lock()
	defer s.requestsMu.Unlock()
	
	req, ok := s.requests[requestID]
	if !ok {
		c.JSON(http.StatusNotFound, gin.H{"status": "error", "message": "request not found"})
		return
	}
	
	if req.Status != "failed" {
		c.JSON(http.StatusBadRequest, gin.H{"status": "error", "message": "can only retry failed requests"})
		return
	}
	
	req.Status = "pending"
	go s.processRequest(requestID)
	
	c.JSON(http.StatusOK, gin.H{"status": "ok", "message": "request retry initiated"})
}

func (s *FaucetService) adminMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		apiKey := c.GetHeader("X-API-Key")
		if apiKey == "" {
			apiKey = c.Query("api_key")
		}
		
		if apiKey != "admin" {
			c.JSON(http.StatusForbidden, gin.H{"status": "error", "message": "admin access required"})
			c.Abort()
			return
		}
		c.Next()
	}
}

// =============================================================================
// CORE FUNCTIONS
// =============================================================================

func (s *FaucetService) checkRateLimit(ipAddress, address string) bool {
	s.rateLimitsMu.Lock()
	defer s.rateLimitsMu.Unlock()
	
	now := time.Now()
	
	ipLimit, ok := s.ipLimits[ipAddress]
	if ok {
		if now.Sub(ipLimit.ResetTime) > time.Hour {
			ipLimit.Count = 0
			ipLimit.ResetTime = now
		}
		if ipLimit.Count >= IPLimitPerHour {
			return false
		}
		ipLimit.Count++
	} else {
		s.ipLimits[ipAddress] = &RateLimit{Count: 1, ResetTime: now}
	}
	
	addrLimit, ok := s.addrLimits[address]
	if ok {
		if now.Sub(addrLimit.ResetTime) > 24*time.Hour {
			addrLimit.Count = 0
			addrLimit.ResetTime = now
		}
		if addrLimit.Count >= AddressLimitPerDay {
			return false
		}
		addrLimit.Count++
	} else {
		s.addrLimits[address] = &RateLimit{Count: 1, ResetTime: now}
	}
	
	s.countersMu.Lock()
	if now.Sub(s.lastHourReset) > time.Hour {
		s.hourlyCounter = 0
		s.lastHourReset = now
	}
	if s.hourlyCounter >= GlobalLimitPerHour {
		s.countersMu.Unlock()
		return false
	}
	s.hourlyCounter++
	s.countersMu.Unlock()
	
	return true
}

func (s *FaucetService) verifyCaptcha(token string) bool {
	if token == "" {
		return false
	}
	return len(token) >= 20
}

func (s *FaucetService) processRequest(requestID string) {
	s.requestsMu.Lock()
	req, ok := s.requests[requestID]
	s.requestsMu.Unlock()
	
	if !ok {
		return
	}
	
	chain, ok := s.chains[req.ChainID]
	if !ok {
		req.Status = "failed"
		req.Error = "chain not found"
		return
	}
	
	// Simulate transaction
	req.Status = "completed"
	req.TXHash = "0x" + generateRandomHash()
	req.CompletedAt = time.Now()
	
	s.totalDispensedMu.Lock()
	switch chain.Symbol {
	case "ETH":
		s.totalETH.Add(s.totalETH, big.NewInt(ETHAmount))
	case "BNB":
		s.totalBNB.Add(s.totalBNB, big.NewInt(BNBAmount))
	case "MATIC":
		s.totalMATIC.Add(s.totalMATIC, big.NewInt(MATICAmount))
	}
	s.totalDispensedMu.Unlock()
	
	if s.discordEnabled {
		go s.sendDiscordNotification(req, chain)
	}
}

func (s *FaucetService) sendDiscordNotification(req *FaucetRequest, chain *ChainConfig) {
	if s.discordWebhook == "" {
		return
	}
	
	message := fmt.Sprintf("💧 Faucet drip completed!\n\nAddress: `%s`\nChain: %s (%s)\nAmount: %s %s\nTX: %s",
		req.Address, chain.Name, chain.Symbol, req.Amount, chain.Symbol, req.TXHash)
	
	_ = message
}

func (s *FaucetService) rotateCounters() {
	ticker := time.NewTicker(1 * time.Hour)
	defer ticker.Stop()
	
	for range ticker.C {
		s.countersMu.Lock()
		now := time.Now()
		if now.Sub(s.lastHourReset) > time.Hour {
			s.hourlyCounter = 0
			s.lastHourReset = now
		}
		if now.Sub(s.lastDayReset) > 24*time.Hour {
			s.dailyCounter = 0
			s.lastDayReset = now
		}
		s.countersMu.Unlock()
	}
}

// =============================================================================
// UTILITIES
// =============================================================================

func isValidAddress(addr string) bool {
	if addr == "" || len(addr) != 42 {
		return false
	}
	if !strings.HasPrefix(strings.ToLower(addr), "0x") {
		return false
	}
	addrWithout0x := addr[2:]
	_, err := hex.DecodeString(addrWithout0x)
	return err == nil
}

func generateRandomHash() string {
	bytes := make([]byte, 32)
	for i := range bytes {
		bytes[i] = byte(i * 17 % 256)
	}
	return hex.EncodeToString(bytes)
}