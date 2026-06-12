// Package api_pro provides premium API endpoints with advanced rate limiting
// and higher limits for professional users.
package api_pro

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/go-redis/redis/v8"
	"github.com/gorilla/websocket"
	
	"tigersmartchain/explorer/services/tokens"
	"tigersmartchain/explorer/services/nfts"
	"tigersmartchain/explorer/services/analytics"
	"tigersmartchain/explorer/services/blocks"
)

// Config holds the API Pro configuration
type Config struct {
	Port              string
	RedisURL          string
	TokenRateLimit    int           // requests per minute for basic tier
	ProRateLimit      int           // requests per minute for pro tier
	EnterpriseRateLimit int          // requests per minute for enterprise tier
	RateLimitWindow    time.Duration // rate limit window
	BurstMultiplier  int          // burst multiplier
	JWTSecret       string       // JWT secret for authentication
}

// APIKeyTier represents an API key's tier
type APIKeyTier string

const (
	TierFree      APIKeyTier = "free"
	TierPro      APIKeyTier = "pro"
	TierEnterprise APIKeyTier = "enterprise"
)

// RateLimiter handles rate limiting per API key
type RateLimiter struct {
	redis      *redis.Client
	mu         sync.RWMutex
	limits     map[APIKeyTier]int
	window     time.Duration
}

// NewRateLimiter creates a new rate limiter
func NewRateLimiter(redisURL string, limits map[APIKeyTier]int, window time.Duration) (*RateLimiter, error) {
	rdb, err := redis.ParseURL(redisURL)
	if err != nil {
		return nil, err
	}
	
	client := redis.NewClient(rdb)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	
	if err := client.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	
	return &RateLimiter{
		redis:   client,
		limits:  limits,
		window:  window,
	}, nil
}

// Allow checks if a request is allowed and increments the counter
func (rl *RateLimiter) Allow(ctx context.Context, apiKey string, tier APIKeyTier) (bool, error) {
	key := fmt.Sprintf("ratelimit:%s:%s", apiKey, time.Now().Truncate(rl.window).Unix())
	limit := rl.limits[tier]
	
	// Increment counter
	count, err := rl.redis.Incr(ctx, key).Result()
	if err != nil {
		return false, err
	}
	
	// Set expiry on first request
	if count == 1 {
		rl.redis.Expire(ctx, key, rl.window)
	}
	
	return int(count) <= limit, nil
}

// GetRemaining returns the remaining requests for the window
func (rl *RateLimiter) GetRemaining(ctx context.Context, apiKey string, tier APIKeyTier) (int, error) {
	key := fmt.Sprintf("ratelimit:%s:%s", apiKey, time.Now().Truncate(rl.window).Unix())
	limit := rl.limits[tier]
	
	count, err := rl.redis.Get(ctx, key).Int()
	if err == redis.Nil {
		return limit, nil
	}
	if err != nil {
		return 0, err
	}
	
	return limit - count, nil
}

// Reset resets the rate limit for an API key
func (rl *RateLimiter) Reset(ctx context.Context, apiKey string) error {
	pattern := fmt.Sprintf("ratelimit:%s:*", apiKey)
	keys, err := rl.redis.Keys(ctx, pattern).Result()
	if err != nil {
		return err
	}
	
	if len(keys) > 0 {
		return rl.redis.Del(ctx, keys...).Err()
	}
	return nil
}

// APIService handles the premium API requests
type APIService struct {
	config      *Config
	rateLimiter *RateLimiter
	tokenSvc   *tokens.TokenService
	nftSvc     *nfts.NFTService
	analyticsSvc *analytics.AnalyticsService
	blockSvc   *blocks.BlockService
	upgrader   websocket.Upgrader
}

// NewAPIService creates a new API Pro service
func NewAPIService(config *Config) (*APIService, error) {
	limits := map[APIKeyTier]int{
		TierFree:       config.TokenRateLimit,
		TierPro:       config.ProRateLimit,
		TierEnterprise: config.EnterpriseRateLimit,
	}
	
	rl, err := NewRateLimiter(config.RedisURL, limits, config.RateLimitWindow)
	if err != nil {
		return nil, err
	}
	
	return &APIService{
		config:    config,
		rateLimiter: rl,
	}, nil
}

// SetServices sets the backend services
func (s *APIService) SetServices(tokenSvc *tokens.TokenService, nftSvc *nfts.NFTService, analyticsSvc *analytics.AnalyticsService, blockSvc *blocks.BlockService) {
	s.tokenSvc = tokenSvc
	s.nftSvc = nftSvc
	s.analyticsSvc = analyticsSvc
	blockSvc = blocks.NewBlockService()
	s.blockSvc = blockSvc
}

// middleware for rate limiting
func (s *APIService) rateLimitMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		apiKey := c.GetHeader("X-API-Key")
		if apiKey == "" {
			apiKey = c.Query("apikey")
		}
		
		if apiKey == "" {
			c.JSON(http.StatusUnauthorized, gin.H{
				"status": "0",
				"message": "Missing API key",
			})
			c.Abort()
			return
		}
		
		// Get tier from database (simplified for now)
		tier := s.getTierFromKey(apiKey)
		
		allowed, err := s.rateLimiter.Allow(c.Request.Context(), apiKey, tier)
		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{
				"status": "0",
				"message": "Rate limiter error",
			})
			c.Abort()
			return
		}
		
		if !allowed {
			remaining, _ := s.rateLimiter.GetRemaining(c.Request.Context(), apiKey, tier)
			c.Header("X-RateLimit-Remaining", fmt.Sprintf("%d", remaining))
			c.JSON(http.StatusTooManyRequests, gin.H{
				"status": "0",
				"message": "Rate limit exceeded",
				"result": []interface{}{},
			})
			c.Abort()
			return
		}
		
		remaining, _ := s.rateLimiter.GetRemaining(c.Request.Context(), apiKey, tier)
		c.Header("X-RateLimit-Remaining", fmt.Sprintf("%d", remaining))
		c.Next()
	}
}

// getTierFromKey retrieves the tier for an API key
func (s *APIService) getTierFromKey(apiKey string) APIKeyTier {
	// Simplified - in production, look up from database
	if len(apiKey) > 32 {
		return TierEnterprise
	}
	if len(apiKey) > 16 {
		return TierPro
	}
	return TierFree
}

// TokenEndpoints returns the token API endpoints
func (s *APIService) TokenEndpoints(r *gin.RouterGroup) {
	r.GET("/tokens", s.getTokenList)
	r.GET("/token/:address", s.getTokenInfo)
	r.GET("/token/:address/holders", s.getTokenHolders)
	r.GET("/token/:address/transfers", s.getTokenTransfers)
	r.GET("/token/:address/history", s.getTokenPriceHistory)
	r.GET("/token/:address/holderdistribution", s.getHolderDistribution)
	r.GET("/token/:address/approved", s.getTokenApprovals)
	r.GET("/token/:address/allowance", s.getTokenAllowances)
}

// NFTEndpoints returns the NFT API endpoints
func (s *APIService) NFTEndpoints(r *gin.RouterGroup) {
	r.GET("/nfts", s.getNFTList)
	r.GET("/nft/:address", s.getNFTInfo)
	r.GET("/nft/:address/holders", s.getNFTHolders)
	r.GET("/nft/:address/transfers", s.getNFTTransfers)
	r.GET("/nft/:address/history", s.getNFTOwnershipHistory)
	r.GET("/nft/:address/metadata", s.getNFTMetadata)
	r.GET("/nft/:address/floor", s.getNFTFloorPrice)
	r.GET("/nft/:address/traits", s.getNFTTraits)
	r.GET("/nft/:address/royalty", s.getNFTRoyalty)
}

// BlockEndpoints returns the block API endpoints
func (s *APIService) BlockEndpoints(r *gin.RouterGroup) {
	r.GET("/blocks", s.getBlockList)
	r.GET("/block/:number", s.getBlockInfo)
	r.GET("/block/:number/transactions", s.getBlockTransactions)
	r.GET("/block/:number/uncles", s.getBlockUncles)
	r.GET("/block/:number/rewards", s.getBlockRewards)
	r.GET("/block/:number/logs", s.getBlockLogs)
}

// TransactionEndpoints returns the transaction API endpoints
func (s *APIService) TransactionEndpoints(r *gin.RouterGroup) {
	r.GET("/txs", s.getTransactionList)
	r.GET("/tx/:hash", s.getTransactionInfo)
	r.GET("/tx/:hash/logs", s.getTransactionLogs)
	r.GET("/tx/:hash/internal", s.getInternalTransactions)
	r.GET("/tx/:hash/state", s.getStateChanges)
	r.GET("/tx/:hash/receipt", s.getTransactionReceipt)
}

// AccountEndpoints returns the account API endpoints
func (s *APIService) AccountEndpoints(r *gin.RouterGroup) {
	r.GET("/account/:address", s.getAccountInfo)
	r.GET("/account/:address/transactions", s.getAccountTransactions)
	r.GET("/account/:address/tokens", s.getAccountTokens)
	r.GET("/account/:address/nfts", s.getAccountNFTs)
	r.GET("/account/:address/balancehistory", s.getAccountBalanceHistory)
}

// AnalyticsEndpoints returns the analytics API endpoints
func (s *APIService) AnalyticsEndpoints(r *gin.RouterGroup) {
	r.GET("/stats", s.getNetworkStats)
	r.GET("/tps", s.getTPS)
	r.GET("/gas", s.getGasTracker)
	r.GET("/richlist", s.getRichList)
	r.GET("/toptokens", s.getTopTokens)
	r.GET("/topnfts", s.getTopNFTs)
	r.GET("/tvl", s.getTVL)
	r.GET("/daily", s.getDailyStats)
	r.GET("/marketcap", s.getMarketCap)
}

// EventEndpoints returns the event subscription endpoints
func (s *APIService) EventEndpoints(r *gin.RouterGroup) {
	r.GET("/events/newBlocks", s.handleNewBlocksWS)
	r.GET("/events/newTransactions", s.handleNewTransactionsWS)
	r.GET("/events/pendingTransactions", s.handlePendingTransactionsWS)
	r.GET("/events/tokenTransfers", s.handleTokenTransfersWS)
	r.GET("/events/nftTransfers", s.handleNFTTransfersWS)
}

// Handlers for each endpoint

func (s *APIService) getTokenList(c *gin.Context) {
	page := c.DefaultQuery("page", "1")
	offset := c.DefaultQuery("offset", "50")
	
	tokens, err := s.tokenSvc.GetTokenList(c.Request.Context(), page, offset)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": tokens})
}

func (s *APIService) getTokenInfo(c *gin.Context) {
	address := c.Param("address")
	
	token, err := s.tokenSvc.GetTokenInfo(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "0", "message": "Token not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": token})
}

func (s *APIService) getTokenHolders(c *gin.Context) {
	address := c.Param("address")
	page := c.DefaultQuery("page", "1")
	
	holders, err := s.tokenSvc.GetTokenHolders(c.Request.Context(), address, page)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": holders})
}

func (s *APIService) getTokenTransfers(c *gin.Context) {
	address := c.Param("address")
	page := c.DefaultQuery("page", "1")
	
	transfers, err := s.tokenSvc.GetTokenTransfers(c.Request.Context(), address, page)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": transfers})
}

func (s *APIService) getTokenPriceHistory(c *gin.Context) {
	address := c.Param("address")
	days := c.DefaultQuery("days", "30")
	
	history, err := s.tokenSvc.GetPriceHistory(c.Request.Context(), address, days)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": history})
}

func (s *APIService) getHolderDistribution(c *gin.Context) {
	address := c.Param("address")
	
	distribution, err := s.tokenSvc.GetHolderDistribution(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": distribution})
}

func (s *APIService) getTokenApprovals(c *gin.Context) {
	address := c.Param("address")
	page := c.DefaultQuery("page", "1")
	
	approvals, err := s.tokenSvc.GetTokenApprovals(c.Request.Context(), address, page)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": approvals})
}

func (s *APIService) getTokenAllowances(c *gin.Context) {
	owner := c.Query("owner")
	spender := c.Query("spender")
	address := c.Param("address")
	
	allowance, err := s.tokenSvc.GetAllowance(c.Request.Context(), address, owner, spender)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": allowance})
}

func (s *APIService) getNFTList(c *gin.Context) {
	page := c.DefaultQuery("page", "1")
	filter := c.Query("filter")
	
	nfts, err := s.nftSvc.GetNFTList(c.Request.Context(), page, filter)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": nfts})
}

func (s *APIService) getNFTInfo(c *gin.Context) {
	address := c.Param("address")
	
	nft, err := s.nftSvc.GetNFTInfo(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "0", "message": "NFT not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": nft})
}

func (s *APIService) getNFTHolders(c *gin.Context) {
	address := c.Param("address")
	page := c.DefaultQuery("page", "1")
	
	holders, err := s.nftSvc.GetNFTHolders(c.Request.Context(), address, page)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": holders})
}

func (s *APIService) getNFTTransfers(c *gin.Context) {
	address := c.Param("address")
	page := c.DefaultQuery("page", "1")
	
	transfers, err := s.nftSvc.GetNFTTransfers(c.Request.Context(), address, page)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": transfers})
}

func (s *APIService) getNFTOwnershipHistory(c *gin.Context) {
	address := c.Param("address")
	tokenID := c.Query("tokenId")
	
	history, err := s.nftSvc.GetOwnershipHistory(c.Request.Context(), address, tokenID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": history})
}

func (s *APIService) getNFTMetadata(c *gin.Context) {
	address := c.Param("address")
	tokenID := c.Query("tokenId")
	
	metadata, err := s.nftSvc.GetMetadata(c.Request.Context(), address, tokenID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": metadata})
}

func (s *APIService) getNFTFloorPrice(c *gin.Context) {
	address := c.Param("address")
	
	floor, err := s.nftSvc.GetFloorPrice(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": floor})
}

func (s *APIService) getNFTTraits(c *gin.Context) {
	address := c.Param("address")
	
	traits, err := s.nftSvc.GetTraits(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": traits})
}

func (s *APIService) getNFTRoyalty(c *gin.Context) {
	address := c.Param("address")
	
	royalty, err := s.nftSvc.GetRoyaltyInfo(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": royalty})
}

func (s *APIService) getBlockList(c *gin.Context) {
	page := c.DefaultQuery("page", "1")
	
	blocks, err := s.blockSvc.GetBlockList(c.Request.Context(), page)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": blocks})
}

func (s *APIService) getBlockInfo(c *gin.Context) {
	number := c.Param("number")
	
	block, err := s.blockSvc.GetBlockInfo(c.Request.Context(), number)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "0", "message": "Block not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": block})
}

func (s *APIService) getBlockTransactions(c *gin.Context) {
	number := c.Param("number")
	page := c.DefaultQuery("page", "1")
	
	txs, err := s.blockSvc.GetBlockTransactions(c.Request.Context(), number, page)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": txs})
}

func (s *APIService) getBlockUncles(c *gin.Context) {
	number := c.Param("number")
	
	uncles, err := s.blockSvc.GetBlockUncles(c.Request.Context(), number)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": uncles})
}

func (s *APIService) getBlockRewards(c *gin.Context) {
	number := c.Param("number")
	
	rewards, err := s.blockSvc.GetBlockRewards(c.Request.Context(), number)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": rewards})
}

func (s *APIService) getBlockLogs(c *gin.Context) {
	number := c.Param("number")
	page := c.DefaultQuery("page", "1")
	
	logs, err := s.blockSvc.GetBlockLogs(c.Request.Context(), number, page)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": logs})
}

func (s *APIService) getTransactionList(c *gin.Context) {
	page := c.DefaultQuery("page", "1")
	
	txs, err := s.analyticsSvc.GetTransactionList(c.Request.Context(), page)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": txs})
}

func (s *APIService) getTransactionInfo(c *gin.Context) {
	hash := c.Param("hash")
	
	tx, err := s.analyticsSvc.GetTransactionInfo(c.Request.Context(), hash)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "0", "message": "Transaction not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": tx})
}

func (s *APIService) getTransactionLogs(c *gin.Context) {
	hash := c.Param("hash")
	
	logs, err := s.analyticsSvc.GetTransactionLogs(c.Request.Context(), hash)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": logs})
}

func (s *APIService) getInternalTransactions(c *gin.Context) {
	hash := c.Param("hash")
	
	txs, err := s.analyticsSvc.GetInternalTransactions(c.Request.Context(), hash)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": txs})
}

func (s *APIService) getStateChanges(c *gin.Context) {
	hash := c.Param("hash")
	
	changes, err := s.analyticsSvc.GetStateChanges(c.Request.Context(), hash)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": changes})
}

func (s *APIService) getTransactionReceipt(c *gin.Context) {
	hash := c.Param("hash")
	
	receipt, err := s.analyticsSvc.GetTransactionReceipt(c.Request.Context(), hash)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "0", "message": "Receipt not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": receipt})
}

func (s *APIService) getAccountInfo(c *gin.Context) {
	address := c.Param("address")
	
	account, err := s.analyticsSvc.GetAccountInfo(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"status": "0", "message": "Account not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": account})
}

func (s *APIService) getAccountTransactions(c *gin.Context) {
	address := c.Param("address")
	page := c.DefaultQuery("page", "1")
	
	txs, err := s.analyticsSvc.GetAccountTransactions(c.Request.Context(), address, page)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": txs})
}

func (s *APIService) getAccountTokens(c *gin.Context) {
	address := c.Param("address")
	
	tokens, err := s.analyticsSvc.GetAccountTokens(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": tokens})
}

func (s *APIService) getAccountNFTs(c *gin.Context) {
	address := c.Param("address")
	
	nfts, err := s.analyticsSvc.GetAccountNFTs(c.Request.Context(), address)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": nfts})
}

func (s *APIService) getAccountBalanceHistory(c *gin.Context) {
	address := c.Param("address")
	days := c.DefaultQuery("days", "30")
	
	history, err := s.analyticsSvc.GetBalanceHistory(c.Request.Context(), address, days)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": history})
}

func (s *APIService) getNetworkStats(c *gin.Context) {
	stats, err := s.analyticsSvc.GetNetworkStats(c.Request.Context())
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": stats})
}

func (s *APIService) getTPS(c *gin.Context) {
	interval := c.DefaultQuery("interval", "24h")
	
	tps, err := s.analyticsSvc.GetTPS(c.Request.Context(), interval)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": tps})
}

func (s *APIService) getGasTracker(c *gin.Context) {
	gas, err := s.analyticsSvc.GetGasTracker(c.Request.Context())
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": gas})
}

func (s *APIService) getRichList(c *gin.Context) {
	page := c.DefaultQuery("page", "1")
	
	rich, err := s.analyticsSvc.GetRichList(c.Request.Context(), page)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": rich})
}

func (s *APIService) getTopTokens(c *gin.Context) {
	page := c.DefaultQuery("page", "1")
	
	tokens, err := s.analyticsSvc.GetTopTokens(c.Request.Context(), page)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": tokens})
}

func (s *APIService) getTopNFTs(c *gin.Context) {
	page := c.DefaultQuery("page", "1")
	
	nfts, err := s.analyticsSvc.GetTopNFTs(c.Request.Context(), page)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": nfts})
}

func (s *APIService) getTVL(c *gin.Context) {
	tvl, err := s.analyticsSvc.GetTVL(c.Request.Context())
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": tvl})
}

func (s *APIService) getDailyStats(c *gin.Context) {
	stats, err := s.analyticsSvc.GetDailyStats(c.Request.Context())
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": stats})
}

func (s *APIService) getMarketCap(c *gin.Context) {
	cap, err := s.analyticsSvc.GetMarketCap(c.Request.Context())
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"status": "0", "message": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"status": "1", "message": "OK", "result": cap})
}

// WebSocket handlers for real-time events

func (s *APIService) handleNewBlocksWS(c *gin.Context) {
	s.upgrader = websocket.Upgrader{
		CheckOrigin: func(r *http.Request) bool { return true },
	}
	
	conn, err := s.upgrader.Upgrade(c.Writer, c.Request, nil)
	if err != nil {
		return
	}
	defer conn.Close()
	
	// Subscribe to new blocks
	ctx, cancel := context.WithCancel(c.Request.Context())
	defer cancel()
	
	// In production, this would subscribe to a pub/sub channel
	for {
		select {
		case <-ctx.Done():
			return
		default:
			// Send block data periodically
			time.Sleep(5 * time.Second)
			msg := map[string]interface{}{
				"type": "newBlock",
				"data": s.blockSvc.GetLatestBlock(),
			}
			conn.WriteJSON(msg)
		}
	}
}

func (s *APIService) handleNewTransactionsWS(c *gin.Context) {
	s.upgrader = websocket.Upgrader{
		CheckOrigin: func(r *http.Request) bool { return true },
	}
	
	conn, err := s.upgrader.Upgrade(c.Writer, c.Request, nil)
	if err != nil {
		return
	}
	defer conn.Close()
	
	ctx, cancel := context.WithCancel(c.Request.Context())
	defer cancel()
	
	for {
		select {
		case <-ctx.Done():
			return
		default:
			time.Sleep(2 * time.Second)
			msg := map[string]interface{}{
				"type": "newTransaction",
				"data": s.analyticsSvc.GetLatestTransactions(),
			}
			conn.WriteJSON(msg)
		}
	}
}

func (s *APIService) handlePendingTransactionsWS(c *gin.Context) {
	s.upgrader = websocket.Upgrader{
		CheckOrigin: func(r *http.Request) bool { return true },
	}
	
	conn, err := s.upgrader.Upgrade(c.Writer, c.Request, nil)
	if err != nil {
		return
	}
	defer conn.Close()
	
	ctx, cancel := context.WithCancel(c.Request.Context())
	defer cancel()
	
	for {
		select {
		case <-ctx.Done():
			return
		default:
			time.Sleep(1 * time.Second)
			msg := map[string]interface{}{
				"type": "pendingTransaction",
				"data": s.analyticsSvc.GetPendingTransactions(),
			}
			conn.WriteJSON(msg)
		}
	}
}

func (s *APIService) handleTokenTransfersWS(c *gin.Context) {
	s.upgrader = websocket.Upgrader{
		CheckOrigin: func(r *http.Request) bool { return true },
	}
	
	conn, err := s.upgrader.Upgrade(c.Writer, c.Request, nil)
	if err != nil {
		return
	}
	defer conn.Close()
	
	ctx, cancel := context.WithCancel(c.Request.Context())
	defer cancel()
	
	for {
		select {
		case <-ctx.Done():
			return
		default:
			time.Sleep(2 * time.Second)
			msg := map[string]interface{}{
				"type": "tokenTransfer",
				"data": s.tokenSvc.GetLatestTransfers(),
			}
			conn.WriteJSON(msg)
		}
	}
}

func (s *APIService) handleNFTTransfersWS(c *gin.Context) {
	s.upgrader = websocket.Upgrader{
		CheckOrigin: func(r *http.Request) bool { return true },
	}
	
	conn, err := s.upgrader.Upgrade(c.Writer, c.Request, nil)
	if err != nil {
		return
	}
	defer conn.Close()
	
	ctx, cancel := context.WithCancel(c.Request.Context())
	defer cancel()
	
	for {
		select {
		case <-ctx.Done():
			return
		default:
			time.Sleep(2 * time.Second)
			msg := map[string]interface{}{
				"type": "nftTransfer",
				"data": s.nftSvc.GetLatestTransfers(),
			}
			conn.WriteJSON(msg)
		}
	}
}

// Router sets up the API Pro router
func (s *APIService) Router() *gin.Engine {
	r := gin.Default()
	
	// Apply rate limiting middleware
	r.Use(s.rateLimitMiddleware())
	
	// API v2 group
	v2 := r.Group("/api/v2")
	{
		s.TokenEndpoints(v2.Group("/tokens"))
		s.NFTEndpoints(v2.Group("/nfts"))
		s.BlockEndpoints(v2.Group("/blocks"))
		s.TransactionEndpoints(v2.Group("/txs"))
		s.AccountEndpoints(v2.Group("/account"))
		s.AnalyticsEndpoints(v2.Group("/stats"))
		s.EventEndpoints(v2.Group("/events"))
	}
	
	return r
}

// Start starts the API Pro server
func (s *APIService) Start() error {
	r := s.Router()
	return r.Run(s.config.Port)
}

// BatchRequest represents a batch API request
type BatchRequest struct {
	Queries []BatchQuery `json:"queries"`
}

// BatchQuery represents a single query in a batch request
type BatchQuery struct {
	ID     string      `json:"id"`
	Method string      `json:"method"`
	Params interface{} `json:"params"`
}

// BatchResponse represents a batch API response
type BatchResponse struct {
	Results []BatchResult `json:"results"`
}

// BatchResult represents the result of a batch query
type BatchResult struct {
	ID     string      `json:"id"`
	Result interface{} `json:"result"`
	Error  string      `json:"error,omitempty"`
}

// HandleBatch handles batch API requests
func (s *APIService) HandleBatch(c *gin.Context) {
	var req BatchRequest
	if err := json.NewDecoder(c.Request.Body).Decode(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid request"})
		return
	}
	
	results := make([]BatchResult, len(req.Queries))
	for i, q := range req.Queries {
		// Process each query
		results[i] = BatchResult{
			ID:     q.ID,
			Result: nil, // Process based on method
		}
	}
	
	c.JSON(http.StatusOK, gin.H{"results": results})
}

// ExportData exports data in CSV or JSON format
func (s *APIService) ExportData(c *gin.Context) {
	format := c.DefaultQuery("format", "json")
	address := c.Query("address")
	txType := c.Query("type")
	
	var data interface{}
	var err error
	
	switch txType {
	case "transactions":
		data, err = s.analyticsSvc.GetAccountTransactions(c.Request.Context(), address, "1")
	case "transfers":
		data, err = s.tokenSvc.GetTokenTransfers(c.Request.Context(), address, "1")
	default:
		data, err = s.analyticsSvc.GetTransactionList(c.Request.Context(), "1")
	}
	
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	if format == "csv" {
		c.Header("Content-Type", "text/csv")
		c.Header("Content-Disposition", "attachment; filename=export.csv")
		// Convert to CSV
		c.String(http.StatusOK, "address,hash,value,timestamp\n")
	} else {
		c.JSON(http.StatusOK, data)
	}
}

// StartProServer starts the premium API server
func StartProServer(port string, redisURL string) error {
	config := &Config{
		Port:               port,
		RedisURL:           redisURL,
		TokenRateLimit:     5,
		ProRateLimit:       100,
		EnterpriseRateLimit: 1000,
		RateLimitWindow:    time.Minute,
		BurstMultiplier:   10,
	}
	
	svc, err := NewAPIService(config)
	if err != nil {
		return err
	}
	
	return svc.Start()
}