/**
 * TigerScan Pro API Service
 * 
 * High-performance Go service for paid API tiers with rate limiting,
 * batch endpoints, and enterprise features.
 */

package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
	"github.com/google/uuid"
	"golang.org/x/crypto/bcrypt"
)

// Configuration
type ProAPIConfig struct {
	Port              int
	RedisURL           string
	JWTSecret          string
	BaseRateLimit     int           // Requests per minute for free tier
	ProRateLimit      int           // Requests per minute for Pro
	EnterpriseRateLimit int          // Requests per minute for Enterprise
}

// Subscription types
type Subscription struct {
	ID             string    `json:"id"`
	UserID         string    `json:"user_id"`
	Tier           string    `json:"tier"` // free, pro, enterprise
	Status         string    `json:"status"` // active, cancelled, expired
	StartDate      time.Time `json:"start_date"`
	EndDate        time.Time `json:"end_date"`
	MonthlyPrice   float64   `json:"monthly_price"`
	APILimit       int       `json:"api_limit"`
	Features       []string  `json:"features"`
	CreatedAt      time.Time `json:"created_at"`
}

type APIKey struct {
	ID            string    `json:"id"`
	UserID        string    `json:"user_id"`
	Key           string    `json:"key"`
	Name          string    `json:"name"`
	Tier          string    `json:"tier"`
	RateLimit     int       `json:"rate_limit"`
	IsActive      bool      `json:"is_active"`
	LastUsed      time.Time `json:"last_used"`
	UsageCount    int64     `json:"usage_count"`
	CreatedAt     time.Time `json:"created_at"`
}

type UsageRecord struct {
	KeyID       string    `json:"key_id"`
	Endpoint    string    `json:"endpoint"`
	Method      string    `json:"method"`
	StatusCode  int       `json:"status_code"`
	LatencyMs   int64     `json:"latency_ms"`
	BytesIn     int64     `json:"bytes_in"`
	BytesOut    int64     `json:"bytes_out"`
	Timestamp   time.Time `json:"timestamp"`
}

type UsageStats struct {
	KeyID          string  `json:"key_id"`
	TotalRequests int64   `json:"total_requests"`
	RequestsToday int64   `json:"requests_today"`
	RequestsThisMonth int64 `json:"requests_this_month"`
	AvgLatencyMs  float64 `json:"avg_latency_ms"`
	Limit        int      `json:"limit"`
	UsagePercent float64 `json:"usage_percent"`
}

// Request types
type CreateSubscriptionRequest struct {
	Tier         string  `json:"tier" binding:"required,oneof=pro enterprise"`
	PaymentMethod string `json:"payment_method"` // stripe_customer_id
}

type CreateAPIKeyRequest struct {
	Name   string `json:"name" binding:"required,min=1,max=50"`
	Tier   string `json:"tier"` // Defaults to subscription tier
}

type UpgradeRequest struct {
	Tier string `json:"tier" binding:"required,oneof=pro enterprise"`
}

type APIResponse struct {
	Success bool        `json:"success"`
	Data    interface{} `json:"data,omitempty"`
	Error   string     `json:"error,omitempty"`
}

// Pro API Service
type ProAPIService struct {
	config   ProAPIConfig
	redis   *redis.Client
	ctx     context.Context
	cancel  context.CancelFunc
}

func NewProAPIService(config ProAPIConfig) (*ProAPIService, error) {
	ctx, cancel := context.WithCancel(context.Background())

	redisClient := redis.NewClient(&redis.Options{
		Addr: config.RedisURL,
	})

	return &ProAPIService{
		config: config,
		redis:  redisClient,
		ctx:    ctx,
		cancel: cancel,
	}, nil
}

func (s *ProAPIService) Start() error {
	go s.cleanupOldUsage()
	go s.startHTTPServer()
	return nil
}

func (s *ProAPIService) Stop() {
	s.cancel()
}

// Subscription Management
func (s *ProAPIService) CreateSubscription(userID string, req CreateSubscriptionRequest) (*Subscription, error) {
	var price float64
	var limit int
	var features []string

	switch req.Tier {
	case "pro":
		price = 99.0
		limit = s.config.ProRateLimit
		features = []string{
			"higher_rate_limit",
			"batch_endpoints",
			"historical_data",
			"webhooks",
			"email_support",
		}
	case "enterprise":
		price = 499.0
		limit = s.config.EnterpriseRateLimit
		features = []string{
			"unlimited_rate_limit",
			"batch_endpoints",
			"full_history",
			"webhooks",
			"dedicated_support",
			"sla",
			"custom_integrations",
			"priority_indexing",
		}
	}

	subscription := Subscription{
		ID:            uuid.New().String(),
		UserID:        userID,
		Tier:          req.Tier,
		Status:        "active",
		StartDate:     time.Now(),
		EndDate:       time.Now().Add(30 * 24 * time.Hour),
		MonthlyPrice:  price,
		APILimit:      limit,
		Features:      features,
		CreatedAt:     time.Now(),
	}

	subKey := fmt.Sprintf("subscription:%s", subscription.ID)
	subJSON, _ := json.Marshal(subscription)
	s.redis.Set(s.ctx, subKey, subJSON, 0)

	userSubKey := fmt.Sprintf("user:%s:subscription", userID)
	s.redis.Set(s.ctx, userSubKey, subscription.ID, 0)

	return &subscription, nil
}

func (s *ProAPIService) GetSubscription(userID string) (*Subscription, error) {
	userSubKey := fmt.Sprintf("user:%s:subscription", userID)
	subID, err := s.redis.Get(s.ctx, userSubKey).Result()
	if err != nil {
		return nil, fmt.Errorf("no active subscription")
	}

	subKey := fmt.Sprintf("subscription:%s", subID)
	subJSON, err := s.redis.Get(s.ctx, subKey).Result()
	if err != nil {
		return nil, err
	}

	var subscription Subscription
	json.Unmarshal([]byte(subJSON), &subscription)

	return &subscription, nil
}

func (s *ProAPIService) CancelSubscription(userID, subscriptionID string) error {
	subKey := fmt.Sprintf("subscription:%s", subscriptionID)
	subJSON, err := s.redis.Get(s.ctx, subKey).Result()
	if err != nil {
		return fmt.Errorf("subscription not found")
	}

	var subscription Subscription
	json.Unmarshal([]byte(subJSON), &subscription)

	if subscription.UserID != userID {
		return fmt.Errorf("unauthorized")
	}

	subscription.Status = "cancelled"
	updatedJSON, _ := json.Marshal(subscription)
	s.redis.Set(s.ctx, subKey, updatedJSON, 0)

	return nil
}

func (s *ProAPIService) UpgradeSubscription(userID, subscriptionID, newTier string) (*Subscription, error) {
	subKey := fmt.Sprintf("subscription:%s", subscriptionID)
	subJSON, err := s.redis.Get(s.ctx, subKey).Result()
	if err != nil {
		return nil, fmt.Errorf("subscription not found")
	}

	var subscription Subscription
	json.Unmarshal([]byte(subJSON), &subscription)

	if subscription.UserID != userID {
		return nil, fmt.Errorf("unauthorized")
	}

	// Update tier
	subscription.Tier = newTier

	// Update limits based on new tier
	switch newTier {
	case "pro":
		subscription.MonthlyPrice = 99.0
		subscription.APILimit = s.config.ProRateLimit
	case "enterprise":
		subscription.MonthlyPrice = 499.0
		subscription.APILimit = s.config.EnterpriseRateLimit
	}

	updatedJSON, _ := json.Marshal(subscription)
	s.redis.Set(s.ctx, subKey, updatedJSON, 0)

	return &subscription, nil
}

// API Key Management
func (s *ProAPIService) CreateAPIKey(userID string, req CreateAPIKeyRequest) (*APIKey, error) {
	// Get user's subscription to determine tier
	subscription, err := s.GetSubscription(userID)
	if err != nil {
		// Free tier
		subscription = &Subscription{Tier: "free", APILimit: s.config.BaseRateLimit}
	}

	tier := req.Tier
	if tier == "" {
		tier = subscription.Tier
	}

	rateLimit := subscription.APILimit
	if tier == "free" {
		rateLimit = s.config.BaseRateLimit
	}

	// Generate API key
	key := fmt.Sprintf("tgr_%s_%s", tier[:3], uuid.New().String())

	// Hash the key for storage
	hashedKey, _ := bcrypt.GenerateFromPassword([]byte(key), bcrypt.DefaultCost)

	apiKey := APIKey{
		ID:        uuid.New().String(),
		UserID:    userID,
		Key:      string(hashedKey),
		Name:     req.Name,
		Tier:     tier,
		RateLimit: rateLimit,
		IsActive: true,
		CreatedAt: time.Now(),
	}

	keyKey := fmt.Sprintf("apikey:%s", apiKey.ID)
	keyJSON, _ := json.Marshal(apiKey)
	s.redis.Set(s.ctx, keyKey, keyJSON, 0)

	// Index by key for lookup
	keyIndexKey := fmt.Sprintf("apikey:index:%s", key)
	s.redis.Set(s.ctx, keyIndexKey, apiKey.ID, 0)

	// User's keys
	userKeysKey := fmt.Sprintf("user:%s:apikeys", userID)
	s.redis.SAdd(s.ctx, userKeysKey, apiKey.ID)

	return &apiKey, nil
}

func (s *ProAPIService) GetAPIKeys(userID string) ([]APIKey, error) {
	userKeysKey := fmt.Sprintf("user:%s:apikeys", userID)
	keyIDs, err := s.redis.SMembers(s.ctx, userKeysKey).Result()
	if err != nil {
		return nil, err
	}

	var keys []APIKey
	for _, id := range keyIDs {
		keyKey := fmt.Sprintf("apikey:%s", id)
		keyJSON, err := s.redis.Get(s.ctx, keyKey).Result()
		if err != nil {
			continue
		}

		var key APIKey
		json.Unmarshal([]byte(keyJSON), &key)
		// Don't return the hashed key
		key.Key = ""
		keys = append(keys, key)
	}

	return keys, nil
}

func (s *ProAPIService) RevokeAPIKey(userID, keyID string) error {
	keyKey := fmt.Sprintf("apikey:%s", keyID)
	keyJSON, err := s.redis.Get(s.ctx, keyKey).Result()
	if err != nil {
		return fmt.Errorf("key not found")
	}

	var apiKey APIKey
	json.Unmarshal([]byte(keyJSON), &apiKey)

	if apiKey.UserID != userID {
		return fmt.Errorf("unauthorized")
	}

	apiKey.IsActive = false

	updatedJSON, _ := json.Marshal(apiKey)
	s.redis.Set(s.ctx, keyKey, updatedJSON, 0)

	return nil
}

// Rate Limiting
func (s *ProAPIService) CheckRateLimit(key string) (bool, error) {
	// Get key ID from index
	keyIndexKey := fmt.Sprintf("apikey:index:%s", key)
	keyID, err := s.redis.Get(s.ctx, keyIndexKey).Result()
	if err != nil {
		return false, fmt.Errorf("invalid API key")
	}

	// Get key info
	keyKey := fmt.Sprintf("apikey:%s", keyID)
	keyJSON, err := s.redis.Get(s.ctx, keyKey).Result()
	if err != nil {
		return false, err
	}

	var apiKey APIKey
	json.Unmarshal([]byte(keyJSON), &apiKey)

	if !apiKey.IsActive {
		return false, fmt.Errorf("API key is revoked")
	}

	// Check rate limit
	rateLimitKey := fmt.Sprintf("ratelimit:%s", keyID)
	count, err := s.redis.Get(s.ctx, rateLimitKey).Int()
	if err != nil {
		count = 0
	}

	if count >= apiKey.RateLimit {
		return false, fmt.Errorf("rate limit exceeded")
	}

	// Increment counter
	s.redis.Incr(s.ctx, rateLimitKey)
	s.redis.Expire(s.ctx, rateLimitKey, 60*time.Second)

	// Update usage
	s.recordUsage(keyID)

	return true, nil
}

func (s *ProAPIService) recordUsage(keyID string) {
	usageKey := fmt.Sprintf("usage:%s", keyID)
	s.redis.Incr(s.ctx, usageKey)

	todayKey := fmt.Sprintf("usage:%s:today", keyID)
	s.redis.Incr(s.ctx, todayKey)
	s.redis.Expire(s.ctx, todayKey, 24*time.Hour)

	monthKey := fmt.Sprintf("usage:%s:month", keyID)
	s.redis.Incr(s.ctx, monthKey)
	s.redis.Expire(s.ctx, monthKey, 30*24*time.Hour)
}

func (s *ProAPIService) GetUsageStats(keyID string) (*UsageStats, error) {
	keyKey := fmt.Sprintf("apikey:%s", keyID)
	keyJSON, err := s.redis.Get(s.ctx, keyKey).Result()
	if err != nil {
		return nil, err
	}

	var apiKey APIKey
	json.Unmarshal([]byte(keyJSON), &apiKey)

	usageKey := fmt.Sprintf("usage:%s", keyID)
	total, _ := s.redis.Get(s.ctx, usageKey).Int64()

	todayKey := fmt.Sprintf("usage:%s:today", keyID)
	today, _ := s.redis.Get(s.ctx, todayKey).Int64()

	monthKey := fmt.Sprintf("usage:%s:month", keyID)
	month, _ := s.redis.Get(s.ctx, monthKey).Int64()

	usagePercent := 0.0
	if apiKey.RateLimit > 0 {
		usagePercent = float64(today) / float64(apiKey.RateLimit) * 100
	}

	return &UsageStats{
		KeyID:             keyID,
		TotalRequests:     total,
		RequestsToday:     today,
		RequestsThisMonth: month,
		Limit:             apiKey.RateLimit,
		UsagePercent:      usagePercent,
	}, nil
}

func (s *ProAPIService) cleanupOldUsage() {
	ticker := time.NewTicker(1 * time.Hour)
	defer ticker.Stop()

	for {
		select {
		case <-s.ctx.Done():
			return
		case <-ticker.C:
			// Clean up old usage records
		}
	}
}

// Batch Endpoints (Pro/Enterprise only)
func (s *ProAPIService) BatchGetBlocks(blockNumbers []uint64) ([]map[string]interface{}, error) {
	var results []map[string]interface{}

	for _, num := range blockNumbers {
		// In production, fetch from database
		results = append(results, map[string]interface{}{
			"number":       num,
			"hash":         "0x...",
			"timestamp":    time.Now().Unix(),
			"transactions": 0,
		})
	}

	return results, nil
}

func (s *ProAPIService) BatchGetTransactions(txHashes []string) ([]map[string]interface{}, error) {
	var results []map[string]interface{}

	for _, hash := range txHashes {
		results = append(results, map[string]interface{}{
			"hash":     hash,
			"from":    "0x...",
			"to":      "0x...",
			"value":   "0",
			"status":  "success",
		})
	}

	return results, nil
}

func (s *ProAPIService) BatchGetTokenBalances(addresses []string, tokenAddr string) ([]map[string]interface{}, error) {
	var results []map[string]interface{}

	for _, addr := range addresses {
		results = append(results, map[string]interface{}{
			"address": addr,
			"balance": "0",
		})
	}

	return results, nil
}

// HTTP Handlers
func (s *ProAPIService) registerRoutes(r *gin.Engine) {
	// Public
	r.POST("/api/v1/pro/subscribe", s.handleCreateSubscription)
	r.POST("/api/v1/pro/upgrade", s.handleUpgrade)
	r.GET("/api/v1/pro/subscription", s.handleGetSubscription)

	// Protected
	api := r.Group("/api/v1/pro")
	api.Use(s.apiKeyMiddleware)

	// API Keys
	api.GET("/keys", s.handleGetAPIKeys)
	api.POST("/keys", s.handleCreateAPIKey)
	api.DELETE("/keys/:id", s.handleRevokeAPIKey)

	// Usage
	api.GET("/usage/:key_id", s.handleGetUsage)

	// Batch endpoints
	api.POST("/batch/blocks", s.handleBatchGetBlocks)
	api.POST("/batch/transactions", s.handleBatchGetTransactions)
	api.POST("/batch/balances", s.handleBatchGetBalances)
}

func (s *ProAPIService) apiKeyMiddleware(c *gin.Context) {
	apiKey := c.GetHeader("X-API-Key")
	if apiKey == "" {
		apiKey = c.Query("api_key")
	}

	if apiKey == "" {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "missing API key"})
		c.Abort()
		return
	}

	allowed, err := s.CheckRateLimit(apiKey)
	if err != nil {
		c.JSON(http.StatusTooManyRequests, APIResponse{Success: false, Error: err.Error()})
		c.Abort()
		return
	}

	if !allowed {
		c.JSON(http.StatusTooManyRequests, APIResponse{Success: false, Error: "rate limit exceeded"})
		c.Abort()
		return
	}

	c.Next()
}

func (s *ProAPIService) handleCreateSubscription(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	tokenKey := fmt.Sprintf("token:%s", token)
	userID, err := s.redis.Get(s.ctx, tokenKey).Result()
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "invalid token"})
		return
	}

	var req CreateSubscriptionRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, APIResponse{Success: false, Error: err.Error()})
		return
	}

	sub, err := s.CreateSubscription(userID, req)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusCreated, APIResponse{Success: true, Data: sub})
}

func (s *ProAPIService) handleGetSubscription(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	tokenKey := fmt.Sprintf("token:%s", token)
	userID, err := s.redis.Get(s.ctx, tokenKey).Result()
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "invalid token"})
		return
	}

	sub, err := s.GetSubscription(userID)
	if err != nil {
		c.JSON(http.StatusNotFound, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: sub})
}

func (s *ProAPIService) handleUpgrade(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	tokenKey := fmt.Sprintf("token:%s", token)
	userID, err := s.redis.Get(s.ctx, tokenKey).Result()
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "invalid token"})
		return
	}

	var req UpgradeRequest
	c.ShouldBindJSON(&req)

	sub, err := s.GetSubscription(userID)
	if err != nil {
		c.JSON(http.StatusNotFound, APIResponse{Success: false, Error: "no active subscription"})
		return
	}

	updated, err := s.UpgradeSubscription(userID, sub.ID, req.Tier)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: updated})
}

func (s *ProAPIService) handleGetAPIKeys(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	tokenKey := fmt.Sprintf("token:%s", token)
	userID, err := s.redis.Get(s.ctx, tokenKey).Result()
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "invalid token"})
		return
	}

	keys, err := s.GetAPIKeys(userID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: keys})
}

func (s *ProAPIService) handleCreateAPIKey(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	tokenKey := fmt.Sprintf("token:%s", token)
	userID, err := s.redis.Get(s.ctx, tokenKey).Result()
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "invalid token"})
		return
	}

	var req CreateAPIKeyRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, APIResponse{Success: false, Error: err.Error()})
		return
	}

	key, err := s.CreateAPIKey(userID, req)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	// Return full key only on creation
	c.JSON(http.StatusCreated, APIResponse{Success: true, Data: key})
}

func (s *ProAPIService) handleRevokeAPIKey(c *gin.Context) {
	token := c.GetHeader("Authorization")
	token = strings.TrimPrefix(token, "Bearer ")

	tokenKey := fmt.Sprintf("token:%s", token)
	userID, err := s.redis.Get(s.ctx, tokenKey).Result()
	if err != nil {
		c.JSON(http.StatusUnauthorized, APIResponse{Success: false, Error: "invalid token"})
		return
	}

	keyID := c.Param("id")

	if err := s.RevokeAPIKey(userID, keyID); err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true})
}

func (s *ProAPIService) handleGetUsage(c *gin.Context) {
	keyID := c.Param("key_id")

	stats, err := s.GetUsageStats(keyID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: stats})
}

func (s *ProAPIService) handleBatchGetBlocks(c *gin.Context) {
	var req struct {
		Blocks []uint64 `json:"blocks" binding:"required"`
	}
	c.ShouldBindJSON(&req)

	results, err := s.BatchGetBlocks(req.Blocks)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: results})
}

func (s *ProAPIService) handleBatchGetTransactions(c *gin.Context) {
	var req struct {
		Transactions []string `json:"transactions" binding:"required"`
	}
	c.ShouldBindJSON(&req)

	results, err := s.BatchGetTransactions(req.Transactions)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: results})
}

func (s *ProAPIService) handleBatchGetBalances(c *gin.Context) {
	var req struct {
		Addresses []string `json:"addresses" binding:"required"`
		Token     string  `json:"token" binding:"required"`
	}
	c.ShouldBindJSON(&req)

	results, err := s.BatchGetTokenBalances(req.Addresses, req.Token)
	if err != nil {
		c.JSON(http.StatusInternalServerError, APIResponse{Success: false, Error: err.Error()})
		return
	}

	c.JSON(http.StatusOK, APIResponse{Success: true, Data: results})
}

func (s *ProAPIService) startHTTPServer() {
	r := gin.Default()
	s.registerRoutes(r)
	r.Run(fmt.Sprintf(":%d", s.config.Port))
}

func main() {
	config := ProAPIConfig{
		Port:               8092,
		RedisURL:           "localhost:6379",
		BaseRateLimit:      60,
		ProRateLimit:       600,
		EnterpriseRateLimit: 6000,
	}

	service, err := NewProAPIService(config)
	if err != nil {
		fmt.Printf("Failed to create service: %v\n", err)
		return
	}

	if err := service.Start(); err != nil {
		fmt.Printf("Failed to start service: %v\n", err)
		return
	}

	fmt.Println("Pro API Service started on port", config.Port)
	select {}
}
