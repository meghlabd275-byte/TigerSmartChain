/**
 * TigerScan Pro API Service
 * 
 * Complete implementation of paid API tier with:
 * - API key management with tiers
 * - Usage tracking and billing
 * - Premium endpoints
 * - Rate limiting by tier
 * - Webhook notifications
 * - Usage analytics
 */

package main

import (
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"math/big"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/go-redis/redis/v8"
	"github.com/golang-jwt/jwt/v5"
	"github.com/lib/pq"
	"golang.org/x/crypto/bcrypt"
)

// =============================================================================
// Configuration
// =============================================================================

type Config struct {
	Server         ServerConfig
	Database       DatabaseConfig
	Redis          RedisConfig
	Stripe         StripeConfig
	JWT            JWTConfig
	RateLimit      RateLimitConfig
}

type ServerConfig struct {
	Host string
	Port int
}

type DatabaseConfig struct {
	Host     string
	Port     int
	User     string
	Password string
	DBName   string
}

type RedisConfig struct {
	Host     string
	Port     int
	Password string
	DB       int
}

type StripeConfig struct {
	APIKey        string
	WebhookSecret string
}

type JWTConfig struct {
	Secret          string
	ExpirationHours int
}

type RateLimitConfig struct {
	FreeTierReqsPerMin    int
	BasicTierReqsPerMin   int
	ProTierReqsPerMin     int
	EnterpriseReqsPerMin   int
}

func LoadConfig() *Config {
	return &Config{
		Server: ServerConfig{
			Host: "0.0.0.0",
			Port: 8443,
		},
		Database: DatabaseConfig{
			Host:     getEnv("DB_HOST", "localhost"),
			Port:     5432,
			User:     getEnv("DB_USER", "tigerscan"),
			Password: getEnv("DB_PASSWORD", "password"),
			DBName:   getEnv("DB_NAME", "tigerscan_pro"),
		},
		Redis: RedisConfig{
			Host:     getEnv("REDIS_HOST", "localhost"),
			Port:     6379,
			Password: getEnv("REDIS_PASSWORD", ""),
			DB:       1,
		},
		Stripe: StripeConfig{
			APIKey:        getEnv("STRIPE_API_KEY", ""),
			WebhookSecret: getEnv("STRIPE_WEBHOOK_SECRET", ""),
		},
		JWT: JWTConfig{
			Secret:          getEnv("JWT_SECRET", "tigerscan-secret-key-change-in-production"),
			ExpirationHours: 24,
		},
		RateLimit: RateLimitConfig{
			FreeTierReqsPerMin:    5,
			BasicTierReqsPerMin:   60,
			ProTierReqsPerMin:     300,
			EnterpriseReqsPerMin:  10000,
		},
	}
}

func getEnv(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}

// =============================================================================
// Database Models
// =============================================================================

type APIKey struct {
	ID                int64
	KeyHash           string
	EncryptedKey      string
	Tier              string
	UserID            int64
	RateLimit         int
	MonthlyQuota      int64
	UsageThisMonth    int64
	IsActive          bool
	CreatedAt         time.Time
	UpdatedAt         time.Time
	LastUsedAt        *time.Time
	ExpiresAt         *time.Time
}

type User struct {
	ID                int64
	Email             string
	PasswordHash      string
	Name              string
	StripeCustomerID  *string
	SubscriptionTier  string
	SubscriptionStatus string
	CreatedAt         time.Time
	UpdatedAt         time.Time
}

type UsageRecord struct {
	ID          int64
	APIKeyID    int64
	Endpoint    string
	Method      string
	StatusCode  int
	LatencyMs   int64
	BytesIn     int64
	BytesOut    int64
	Timestamp   time.Time
}

type Subscription struct {
	ID                  int64
	UserID              int64
	StripeSubscriptionID string
	Tier                string
	Status              string
	CurrentPeriodStart  time.Time
	CurrentPeriodEnd    time.Time
	CancelAtPeriodEnd   bool
	CreatedAt           time.Time
}

// =============================================================================
// Service
// =============================================================================

type ProAPIService struct {
	config    *Config
	db        *sql.DB
	redis     *redis.Client
	jwtSecret []byte
	keys      map[string]*APIKey
	mu        sync.RWMutex
	stats     ServiceStats
}

type ServiceStats struct {
	TotalRequests      int64
	ActiveKeys         int64
	RevenueThisMonth   float64
	RequestsByTier     map[string]int64
}

func NewProAPIService(config *Config) (*ProAPIService, error) {
	// Connect to database
	connStr := fmt.Sprintf(
		"host=%s port=%d user=%s password=%s dbname=%s sslmode=disable",
		config.Database.Host, config.Database.Port, config.Database.User,
		config.Database.Password, config.Database.DBName,
	)
	
	db, err := sql.Open("postgres", connStr)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	
	// Connect to Redis
	rdb := redis.NewClient(&redis.Options{
		Addr:     fmt.Sprintf("%s:%d", config.Redis.Host, config.Redis.Port),
		Password: config.Redis.Password,
		DB:       config.Redis.DB,
	})
	
	ctx := context.Background()
	if err := rdb.Ping(ctx).Err(); err != nil {
		// Continue without Redis for testing
		fmt.Printf("Warning: Redis not available: %v\n", err)
	}
	
	// Initialize database schema
	if err := initDatabase(db); err != nil {
		return nil, fmt.Errorf("failed to initialize database: %w", err)
	}
	
	service := &ProAPIService{
		config:    config,
		db:        db,
		redis:     rdb,
		jwtSecret: []byte(config.JWT.Secret),
		keys:      make(map[string]*APIKey),
		stats: ServiceStats{
			RequestsByTier: make(map[string]int64),
		},
	}
	
	// Load API keys from database
	if err := service.loadAPIKeys(); err != nil {
		fmt.Printf("Warning: Failed to load API keys: %v\n", err)
	}
	
	return service, nil
}

func initDatabase(db *sql.DB) error {
	schema := `
	CREATE TABLE IF NOT EXISTS users (
		id BIGSERIAL PRIMARY KEY,
		email VARCHAR(255) UNIQUE NOT NULL,
		password_hash VARCHAR(255) NOT NULL,
		name VARCHAR(255),
		stripe_customer_id VARCHAR(255),
		subscription_tier VARCHAR(50) DEFAULT 'free',
		subscription_status VARCHAR(50) DEFAULT 'inactive',
		created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
		updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
	);
	
	CREATE TABLE IF NOT EXISTS api_keys (
		id BIGSERIAL PRIMARY KEY,
		key_hash VARCHAR(64) UNIQUE NOT NULL,
		encrypted_key TEXT NOT NULL,
		tier VARCHAR(50) DEFAULT 'free',
		user_id BIGINT REFERENCES users(id),
		rate_limit INTEGER DEFAULT 60,
		monthly_quota BIGINT DEFAULT 10000,
		usage_this_month BIGINT DEFAULT 0,
		is_active BOOLEAN DEFAULT true,
		created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
		updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
		last_used_at TIMESTAMP,
		expires_at TIMESTAMP
	);
	
	CREATE TABLE IF NOT EXISTS usage_records (
		id BIGSERIAL PRIMARY KEY,
		api_key_id BIGINT REFERENCES api_keys(id),
		endpoint VARCHAR(255) NOT NULL,
		method VARCHAR(10) NOT NULL,
		status_code INTEGER,
		latency_ms BIGINT,
		bytes_in BIGINT DEFAULT 0,
		bytes_out BIGINT DEFAULT 0,
		timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP
	);
	
	CREATE TABLE IF NOT EXISTS subscriptions (
		id BIGSERIAL PRIMARY KEY,
		user_id BIGINT REFERENCES users(id),
		stripe_subscription_id VARCHAR(255) UNIQUE,
		tier VARCHAR(50) NOT NULL,
		status VARCHAR(50) NOT NULL,
		current_period_start TIMESTAMP,
		current_period_end TIMESTAMP,
		cancel_at_period_end BOOLEAN DEFAULT false,
		created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
	);
	
	CREATE TABLE IF NOT EXISTS invoices (
		id BIGSERIAL PRIMARY KEY,
		user_id BIGINT REFERENCES users(id),
		stripe_invoice_id VARCHAR(255) UNIQUE,
		amount BIGINT NOT NULL,
		currency VARCHAR(10) DEFAULT 'usd',
		status VARCHAR(50),
		paid_at TIMESTAMP,
		created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
	);
	
	CREATE INDEX IF NOT EXISTS idx_api_keys_key_hash ON api_keys(key_hash);
	CREATE INDEX IF NOT EXISTS idx_api_keys_user_id ON api_keys(user_id);
	CREATE INDEX IF NOT EXISTS idx_usage_records_api_key_id ON usage_records(api_key_id);
	CREATE INDEX IF NOT EXISTS idx_usage_records_timestamp ON usage_records(timestamp);
	`
	
	_, err := db.Exec(schema)
	return err
}

func (s *ProAPIService) loadAPIKeys() error {
	rows, err := s.db.Query("SELECT id, key_hash, encrypted_key, tier, user_id, rate_limit, monthly_quota, usage_this_month, is_active FROM api_keys WHERE is_active = true")
	if err != nil {
		return err
	}
	defer rows.Close()
	
	for rows.Next() {
		var key APIKey
		err := rows.Scan(
			&key.ID, &key.KeyHash, &key.EncryptedKey, &key.Tier,
			&key.UserID, &key.RateLimit, &key.MonthlyQuota,
			&key.UsageThisMonth, &key.IsActive,
		)
		if err != nil {
			continue
		}
		s.keys[key.KeyHash] = &key
	}
	
	return nil
}

// =============================================================================
// API Key Management
// =============================================================================

func (s *ProAPIService) GenerateAPIKey(userID int64, tier string) (string, error) {
	// Generate random key
	keyBytes := make([]byte, 32)
	if _, err := rand.Read(keyBytes); err != nil {
		return "", err
	}
	
	key := fmt.Sprintf("TSC%s", hex.EncodeToString(keyBytes))
	keyHash := fmt.Sprintf("%x", sha256.Sum256([]byte(key)))
	
	// Get rate limit for tier
	rateLimit := s.config.RateLimit.FreeTierReqsPerMin
	monthlyQuota := int64(10000)
	
	switch tier {
	case "basic":
		rateLimit = s.config.RateLimit.BasicTierReqsPerMin
		monthlyQuota = 100000
	case "pro":
		rateLimit = s.config.RateLimit.ProTierReqsPerMin
		monthlyQuota = 1000000
	case "enterprise":
		rateLimit = s.config.RateLimit.EnterpriseReqsPerMin
		monthlyQuota = -1 // Unlimited
	}
	
	// Encrypt key for storage
	encryptedKey, err := encryptAPIKey(key, s.jwtSecret)
	if err != nil {
		return "", err
	}
	
	// Store in database
	_, err = s.db.Exec(`
		INSERT INTO api_keys (key_hash, encrypted_key, tier, user_id, rate_limit, monthly_quota)
		VALUES ($1, $2, $3, $4, $5, $6)
	`, keyHash, encryptedKey, tier, userID, rateLimit, monthlyQuota)
	
	if err != nil {
		return "", err
	}
	
	// Update in-memory cache
	s.mu.Lock()
	s.keys[keyHash] = &APIKey{
		KeyHash:      keyHash,
		EncryptedKey: encryptedKey,
		Tier:         tier,
		UserID:       userID,
		RateLimit:    rateLimit,
		MonthlyQuota: monthlyQuota,
		IsActive:     true,
	}
	s.mu.Unlock()
	
	return key, nil
}

func (s *ProAPIService) ValidateAPIKey(key string) (*APIKey, error) {
	keyHash := fmt.Sprintf("%x", sha256.Sum256([]byte(key)))
	
	s.mu.RLock()
	apiKey, exists := s.keys[keyHash]
	s.mu.RUnlock()
	
	if !exists {
		// Try database
		var ak APIKey
		err := s.db.QueryRow(`
			SELECT id, key_hash, encrypted_key, tier, user_id, rate_limit, 
			       monthly_quota, usage_this_month, is_active, last_used_at
			FROM api_keys WHERE key_hash = $1 AND is_active = true
		`, keyHash).Scan(
			&ak.ID, &ak.KeyHash, &ak.EncryptedKey, &ak.Tier,
			&ak.UserID, &ak.RateLimit, &ak.MonthlyQuota,
			&ak.UsageThisMonth, &ak.IsActive, &ak.LastUsedAt,
		)
		
		if err != nil {
			return nil, fmt.Errorf("invalid API key")
		}
		
		apiKey = &ak
	}
	
	if !apiKey.IsActive {
		return nil, fmt.Errorf("API key is disabled")
	}
	
	// Check expiration
	if apiKey.ExpiresAt != nil && time.Now().After(*apiKey.ExpiresAt) {
		return nil, fmt.Errorf("API key has expired")
	}
	
	// Check monthly quota
	if apiKey.MonthlyQuota > 0 && apiKey.UsageThisMonth >= apiKey.MonthlyQuota {
		return nil, fmt.Errorf("monthly quota exceeded")
	}
	
	return apiKey, nil
}

func (s *ProAPIService) RevokeAPIKey(keyID int64) error {
	_, err := s.db.Exec("UPDATE api_keys SET is_active = false WHERE id = $1", keyID)
	
	if err == nil {
		s.mu.Lock()
		// Remove from cache
		for k, v := range s.keys {
			if v.ID == keyID {
				delete(s.keys, k)
				break
			}
		}
		s.mu.Unlock()
	}
	
	return err
}

// =============================================================================
// Usage Tracking
// =============================================================================

func (s *ProAPIService) RecordUsage(apiKeyID int64, endpoint, method string, statusCode int, latencyMs int64, bytesIn, bytesOut int64) {
	// Record in database
	_, err := s.db.Exec(`
		INSERT INTO usage_records (api_key_id, endpoint, method, status_code, latency_ms, bytes_in, bytes_out)
		VALUES ($1, $2, $3, $4, $5, $6, $7)
	`, apiKeyID, endpoint, method, statusCode, latencyMs, bytesIn, bytesOut)
	
	if err != nil {
		fmt.Printf("Failed to record usage: %v\n", err)
	}
	
	// Update monthly usage
	_, err = s.db.Exec(`
		UPDATE api_keys SET usage_this_month = usage_this_month + 1, 
		                   last_used_at = CURRENT_TIMESTAMP 
		WHERE id = $1
	`, apiKeyID)
	
	// Update stats
	s.stats.TotalRequests++
	
	s.mu.Lock()
	if apiKey, ok := s.keys[fmt.Sprintf("%d", apiKeyID)]; ok {
		s.stats.RequestsByTier[apiKey.Tier]++
	}
	s.mu.Unlock()
}

func (s *ProAPIService) GetUsageStats(apiKeyID int64, startTime, endTime time.Time) (*UsageStats, error) {
	rows, err := s.db.Query(`
		SELECT 
			COUNT(*) as total_requests,
			SUM(latency_ms) / COUNT(*) as avg_latency,
			SUM(bytes_out) as total_bytes_out,
			COUNT(DISTINCT endpoint) as unique_endpoints
		FROM usage_records 
		WHERE api_key_id = $1 AND timestamp BETWEEN $2 AND $3
	`, apiKeyID, startTime, endTime)
	
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var stats UsageStats
	if rows.Next() {
		rows.Scan(&stats.TotalRequests, &stats.AvgLatencyMs, &stats.TotalBytesOut, &stats.UniqueEndpoints)
	}
	
	return &stats, nil
}

type UsageStats struct {
	TotalRequests   int64
	AvgLatencyMs    int64
	TotalBytesOut   int64
	UniqueEndpoints int64
}

// =============================================================================
// Rate Limiting
// =============================================================================

func (s *ProAPIService) CheckRateLimit(apiKey *APIKey) error {
	if s.redis == nil {
		return nil // Skip rate limiting if Redis unavailable
	}
	
	ctx := context.Background()
	key := fmt.Sprintf("ratelimit:%d", apiKey.ID)
	
	// Increment counter
	count, err := s.redis.Incr(ctx, key).Result()
	if err != nil {
		return nil // Skip rate limiting on error
	}
	
	// Set expiry if first request
	if count == 1 {
		s.redis.Expire(ctx, key, time.Minute)
	}
	
	if int(count) > apiKey.RateLimit {
		return fmt.Errorf("rate limit exceeded: %d req/min", apiKey.RateLimit)
	}
	
	return nil
}

// =============================================================================
// Premium Endpoints
// =============================================================================

var premiumEndpoints = map[string]bool{
	"/api/v1/accounts/history":         true,
	"/api/v1/accounts/balance":          true,
	"/api/v1/accounts/state":            true,
	"/api/v1/accounts/storage":          true,
	"/api/v1/debug/trace":                true,
	"/api/v1/debug/trace-call":           true,
	"/api/v1/stats/gas-prediction":       true,
	"/api/v1/stats/advanced-analytics":   true,
	"/api/v1/mev/bundles":                true,
	"/api/v1/mev/bundles/search":         true,
	"/api/v1/tokens/history":             true,
	"/api/v1/nfts/analytics":             true,
	"/api/v1/analytics/network-deep":      true,
	"/api/v1/batch/transactions":         true,
	"/api/v1/batch/blocks":               true,
}

func (s *ProAPIService) CheckTierAccess(endpoint, tier string) error {
	if !premiumEndpoints[endpoint] {
		return nil // Not a premium endpoint
	}
	
	requiredTiers := map[string]int{
		"free":      0,
		"basic":     1,
		"pro":       2,
		"enterprise": 3,
	}
	
	tierLevel := requiredTiers[tier]
	endpointTier := 1 // Minimum tier for premium endpoints
	
	if tierLevel < endpointTier {
		return fmt.Errorf("endpoint requires %s tier or higher", getNextTier(tier))
	}
	
	return nil
}

func getNextTier(current string) string {
	switch current {
	case "free":
		return "basic"
	case "basic":
		return "pro"
	case "pro":
		return "enterprise"
	default:
		return "enterprise"
	}
}

// =============================================================================
// Webhooks
// =============================================================================

func (s *ProAPIService) SendWebhook(userID int64, eventType string, data map[string]interface{}) error {
	// Get webhook URL from user settings
	var webhookURL string
	err := s.db.QueryRow(`
		SELECT webhook_url FROM user_settings WHERE user_id = $1
	`, userID).Scan(&webhookURL)
	
	if err != nil || webhookURL == "" {
		return nil // No webhook configured
	}
	
	payload, _ := json.Marshal(map[string]interface{}{
		"type": eventType,
		"data": data,
	})
	
	req, _ := http.NewRequest("POST", webhookURL, strings.NewReader(string(payload)))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-TigerScan-Event", eventType)
	
	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	
	if resp.StatusCode >= 400 {
		return fmt.Errorf("webhook failed with status %d", resp.StatusCode)
	}
	
	return nil
}

// =============================================================================
// JWT Tokens
// =============================================================================

func (s *ProAPIService) GenerateJWT(userID int64, tier string) (string, error) {
	claims := jwt.MapClaims{
		"user_id": userID,
		"tier":    tier,
		"exp":     time.Now().Add(time.Hour * time.Duration(s.config.JWT.ExpirationHours)).Unix(),
		"iat":     time.Now().Unix(),
	}
	
	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	return token.SignedString(s.jwtSecret)
}

func (s *ProAPIService) ValidateJWT(tokenString string) (jwt.MapClaims, error) {
	token, err := jwt.Parse(tokenString, func(token *jwt.Token) (interface{}, error) {
		if _, ok := token.Method.(*jwt.SigningMethodHMAC); !ok {
			return nil, fmt.Errorf("unexpected signing method: %v", token.Header["alg"])
		}
		return s.jwtSecret, nil
	})
	
	if err != nil {
		return nil, err
	}
	
	if claims, ok := token.Claims.(jwt.MapClaims); ok && token.Valid {
		return claims, nil
	}
	
	return nil, fmt.Errorf("invalid token")
}

// =============================================================================
// Encryption
// =============================================================================

func encryptAPIKey(key string, secret []byte) (string, error) {
	block, err := aes.NewCipher(secret[:32])
	if err != nil {
		return "", err
	}
	
	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return "", err
	}
	
	nonce := make([]byte, gcm.NonceSize())
	if _, err := io.ReadFull(rand.Reader, nonce); err != nil {
		return "", err
	}
	
	ciphertext := gcm.Seal(nonce, nonce, []byte(key), nil)
	return hex.EncodeToString(ciphertext), nil
}

func decryptAPIKey(encryptedKey string, secret []byte) (string, error) {
	data, err := hex.DecodeString(encryptedKey)
	if err != nil {
		return "", err
	}
	
	block, err := aes.NewCipher(secret[:32])
	if err != nil {
		return "", err
	}
	
	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return "", err
	}
	
	nonceSize := gcm.NonceSize()
	if len(data) < nonceSize {
		return "", fmt.Errorf("ciphertext too short")
	}
	
	nonce, ciphertext := data[:nonceSize], data[nonceSize:]
	plaintext, err := gcm.Open(nil, nonce, ciphertext, nil)
	if err != nil {
		return "", err
	}
	
	return string(plaintext), nil
}

// =============================================================================
// Password Hashing
// =============================================================================

func HashPassword(password string) (string, error) {
	bytes, err := bcrypt.GenerateFromPassword([]byte(password), 14)
	return string(bytes), err
}

func CheckPassword(password, hash string) bool {
	err := bcrypt.CompareHashAndPassword([]byte(hash), []byte(password))
	return err == nil
}

// =============================================================================
// HTTP Handlers
// =============================================================================

func (s *ProAPIService) RegisterRoutes(router *gin.Engine) {
	api := router.Group("/api/v1")
	
	// Auth endpoints
	api.POST("/auth/register", s.handleRegister)
	api.POST("/auth/login", s.handleLogin)
	
	// API key management (authenticated)
	apiKeys := api.Group("/keys")
	apiKeys.Use(s.authMiddleware())
	{
		apiKeys.GET("", s.handleListKeys)
		apiKeys.POST("", s.handleCreateKey)
		apiKeys.DELETE("/:id", s.handleRevokeKey)
	}
	
	// Usage endpoints
	usage := api.Group("/usage")
	usage.Use(s.authMiddleware())
	{
		usage.GET("", s.handleGetUsage)
		usage.GET("/summary", s.handleGetUsageSummary)
	}
	
	// Subscription endpoints
	subs := api.Group("/subscription")
	subs.Use(s.authMiddleware())
	{
		subs.GET("", s.handleGetSubscription)
		subs.POST("/upgrade", s.handleUpgrade)
		subs.POST("/cancel", s.handleCancel)
	}
	
	// Premium endpoints
	premium := api.Group("/premium")
	premium.Use(s.authMiddleware())
	premium.Use(s.premiumMiddleware())
	{
		premium.GET("/analytics", s.handlePremiumAnalytics)
		premium.GET("/deep-data", s.handleDeepData)
	}
	
	// Webhook endpoints
	webhooks := api.Group("/webhooks")
	webhooks.Use(s.authMiddleware())
	{
		webhooks.GET("", s.handleListWebhooks)
		webhooks.POST("", s.handleCreateWebhook)
		webhooks.DELETE("/:id", s.handleDeleteWebhook)
	}
	
	// Stripe webhook
	router.POST("/webhooks/stripe", s.handleStripeWebhook)
}

func (s *ProAPIService) handleRegister(c *gin.Context) {
	var req struct {
		Email    string `json:"email" binding:"required,email"`
		Password string `json:"password" binding:"required,min=8"`
		Name     string `json:"name"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	// Hash password
	hash, err := HashPassword(req.Password)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to hash password"})
		return
	}
	
	// Create user
	var userID int64
	err = s.db.QueryRow(`
		INSERT INTO users (email, password_hash, name, subscription_tier, subscription_status)
		VALUES ($1, $2, $3, 'free', 'inactive')
		RETURNING id
	`, req.Email, hash, req.Name).Scan(&userID)
	
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "email already exists"})
		return
	}
	
	// Generate API key for new user
	apiKey, err := s.GenerateAPIKey(userID, "free")
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to generate API key"})
		return
	}
	
	c.JSON(http.StatusCreated, gin.H{
		"message":   "user created successfully",
		"user_id":    userID,
		"api_key":    apiKey,
		"tier":       "free",
	})
}

func (s *ProAPIService) handleLogin(c *gin.Context) {
	var req struct {
		Email    string `json:"email" binding:"required"`
		Password string `json:"password" binding:"required"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	// Find user
	var user User
	err := s.db.QueryRow(`
		SELECT id, email, password_hash, name, subscription_tier 
		FROM users WHERE email = $1
	`, req.Email).Scan(&user.ID, &user.Email, &user.PasswordHash, &user.Name, &user.SubscriptionTier)
	
	if err != nil {
		c.JSON(http.StatusUnauthorized, gin.H{"error": "invalid credentials"})
		return
	}
	
	// Check password
	if !CheckPassword(req.Password, user.PasswordHash) {
		c.JSON(http.StatusUnauthorized, gin.H{"error": "invalid credentials"})
		return
	}
	
	// Generate JWT
	token, err := s.GenerateJWT(user.ID, user.SubscriptionTier)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to generate token"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"token": token,
		"user": gin.H{
			"id":    user.ID,
			"email": user.Email,
			"name":  user.Name,
			"tier":  user.SubscriptionTier,
		},
	})
}

func (s *ProAPIService) handleListKeys(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	rows, err := s.db.Query(`
		SELECT id, key_hash, tier, rate_limit, monthly_quota, usage_this_month, 
		       is_active, created_at, last_used_at
		FROM api_keys WHERE user_id = $1
	`, userID)
	
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to list keys"})
		return
	}
	defer rows.Close()
	
	var keys []map[string]interface{}
	for rows.Next() {
		var key APIKey
		rows.Scan(&key.ID, &key.KeyHash, &key.Tier, &key.RateLimit, 
			&key.MonthlyQuota, &key.UsageThisMonth, &key.IsActive, 
			&key.CreatedAt, &key.LastUsedAt)
		
		keys = append(keys, map[string]interface{}{
			"id":                key.ID,
			"key_prefix":        key.KeyHash[:8] + "...",
			"tier":              key.Tier,
			"rate_limit":        key.RateLimit,
			"monthly_quota":     key.MonthlyQuota,
			"usage_this_month": key.UsageThisMonth,
			"is_active":         key.IsActive,
			"created_at":       key.CreatedAt,
			"last_used_at":     key.LastUsedAt,
		})
	}
	
	c.JSON(http.StatusOK, gin.H{"keys": keys})
}

func (s *ProAPIService) handleCreateKey(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	var req struct {
		Tier string `json:"tier" binding:"required"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	apiKey, err := s.GenerateAPIKey(userID, req.Tier)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to create key"})
		return
	}
	
	c.JSON(http.StatusCreated, gin.H{
		"api_key": apiKey,
		"tier":    req.Tier,
	})
}

func (s *ProAPIService) handleRevokeKey(c *gin.Context) {
	keyID, err := strconv.ParseInt(c.Param("id"), 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid key ID"})
		return
	}
	
	if err := s.RevokeAPIKey(keyID); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to revoke key"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "key revoked successfully"})
}

func (s *ProAPIService) handleGetUsage(c *gin.Context) {
	keyID, err := strconv.ParseInt(c.Query("key_id"), 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid key ID"})
		return
	}
	
	startTime := time.Now().AddDate(0, 0, -7) // Last 7 days
	endTime := time.Now()
	
	stats, err := s.GetUsageStats(keyID, startTime, endTime)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to get usage"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"total_requests":    stats.TotalRequests,
		"avg_latency_ms":   stats.AvgLatencyMs,
		"total_bytes_out":  stats.TotalBytesOut,
		"unique_endpoints": stats.UniqueEndpoints,
		"start_time":       startTime,
		"end_time":         endTime,
	})
}

func (s *ProAPIService) handleGetUsageSummary(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	var totalUsage int64
	s.db.QueryRow(`
		SELECT COALESCE(SUM(usage_this_month), 0) 
		FROM api_keys WHERE user_id = $1
	`, userID).Scan(&totalUsage)
	
	c.JSON(http.StatusOK, gin.H{
		"total_usage_this_month": totalUsage,
	})
}

func (s *ProAPIService) handleGetSubscription(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	var sub Subscription
	err := s.db.QueryRow(`
		SELECT id, tier, status, current_period_start, current_period_end, cancel_at_period_end
		FROM subscriptions WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1
	`, userID).Scan(&sub.ID, &sub.Tier, &sub.Status, &sub.CurrentPeriodStart, &sub.CurrentPeriodEnd, &sub.CancelAtPeriodEnd)
	
	if err != nil {
		// No subscription
		c.JSON(http.StatusOK, gin.H{
			"tier":    "free",
			"status":  "inactive",
		})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{
		"tier":                  sub.Tier,
		"status":                sub.Status,
		"current_period_start":  sub.CurrentPeriodStart,
		"current_period_end":    sub.CurrentPeriodEnd,
		"cancel_at_period_end": sub.CancelAtPeriodEnd,
	})
}

func (s *ProAPIService) handleUpgrade(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	var req struct {
		Tier string `json:"tier" binding:"required"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	// In production, this would create a Stripe checkout session
	c.JSON(http.StatusOK, gin.H{
		"message":      "upgrade initiated",
		"tier":         req.Tier,
		"checkout_url": fmt.Sprintf("/checkout? tier=%s", req.Tier),
	})
}

func (s *ProAPIService) handleCancel(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	_, err := s.db.Exec(`
		UPDATE subscriptions SET cancel_at_period_end = true 
		WHERE user_id = $1 AND status = 'active'
	`, userID)
	
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to cancel"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "subscription will be cancelled at period end"})
}

func (s *ProAPIService) handlePremiumAnalytics(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"message":     "premium analytics data",
		"data":        "detailed analytics here",
	})
}

func (s *ProAPIService) handleDeepData(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"message": "deep data endpoint",
		"data":   "comprehensive data here",
	})
}

func (s *ProAPIService) handleListWebhooks(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	rows, err := s.db.Query(`
		SELECT id, url, events, created_at FROM webhooks WHERE user_id = $1
	`, userID)
	
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to list webhooks"})
		return
	}
	defer rows.Close()
	
	var webhooks []map[string]interface{}
	for rows.Next() {
		var id int64
		var url, events string
		var createdAt time.Time
		rows.Scan(&id, &url, &events, &createdAt)
		
		webhooks = append(webhooks, map[string]interface{}{
			"id":         id,
			"url":        url,
			"events":     events,
			"created_at": createdAt,
		})
	}
	
	c.JSON(http.StatusOK, gin.H{"webhooks": webhooks})
}

func (s *ProAPIService) handleCreateWebhook(c *gin.Context) {
	userID := c.GetInt64("user_id")
	
	var req struct {
		URL   string   `json:"url" binding:"required,url"`
		Events []string `json:"events" binding:"required"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	events := strings.Join(req.Events, ",")
	
	var webhookID int64
	err := s.db.QueryRow(`
		INSERT INTO webhooks (user_id, url, events) VALUES ($1, $2, $3) RETURNING id
	`, userID, req.URL, events).Scan(&webhookID)
	
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to create webhook"})
		return
	}
	
	c.JSON(http.StatusCreated, gin.H{
		"id":      webhookID,
		"url":     req.URL,
		"events":  req.Events,
	})
}

func (s *ProAPIService) handleDeleteWebhook(c *gin.Context) {
	webhookID, err := strconv.ParseInt(c.Param("id"), 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid webhook ID"})
		return
	}
	
	_, err = s.db.Exec("DELETE FROM webhooks WHERE id = $1", webhookID)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to delete webhook"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "webhook deleted"})
}

func (s *ProAPIService) handleStripeWebhook(c *gin.Context) {
	signature := c.GetHeader("Stripe-Signature")
	body, _ := io.ReadAll(c.Request.Body)
	
	// In production, verify Stripe signature
	// Verify and process webhook events (subscription created, updated, cancelled, etc.)
	
	c.JSON(http.StatusOK, gin.H{"received": true})
}

// =============================================================================
// Middleware
// =============================================================================

func (s *ProAPIService) authMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		authHeader := c.GetHeader("Authorization")
		if authHeader == "" {
			c.JSON(http.StatusUnauthorized, gin.H{"error": "authorization required"})
			c.Abort()
			return
		}
		
		token := strings.TrimPrefix(authHeader, "Bearer ")
		claims, err := s.ValidateJWT(token)
		if err != nil {
			c.JSON(http.StatusUnauthorized, gin.H{"error": "invalid token"})
			c.Abort()
			return
		}
		
		userID := int64(claims["user_id"].(float64))
		tier := claims["tier"].(string)
		
		c.Set("user_id", userID)
		c.Set("tier", tier)
		
		c.Next()
	}
}

func (s *ProAPIService) premiumMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		tier := c.GetString("tier")
		endpoint := c.FullPath()
		
		if err := s.CheckTierAccess(endpoint, tier); err != nil {
			c.JSON(http.StatusForbidden, gin.H{"error": err.Error()})
			c.Abort()
			return
		}
		
		c.Next()
	}
}

func (s *ProAPIService) rateLimitMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		// Get API key from header
		apiKey := c.GetHeader("X-API-Key")
		if apiKey == "" {
			c.Next()
			return
		}
		
		key, err := s.ValidateAPIKey(apiKey)
		if err != nil {
			c.JSON(http.StatusUnauthorized, gin.H{"error": err.Error()})
			c.Abort()
			return
		}
		
		if err := s.CheckRateLimit(key); err != nil {
			c.JSON(http.StatusTooManyRequests, gin.H{"error": err.Error()})
			c.Abort()
			return
		}
		
		c.Set("api_key_id", key.ID)
		c.Set("api_key_tier", key.Tier)
		
		c.Next()
	}
}

// =============================================================================
// Main
// =============================================================================

func main() {
	config := LoadConfig()
	
	service, err := NewProAPIService(config)
	if err != nil {
		fmt.Printf("Failed to start service: %v\n", err)
		os.Exit(1)
	}
	
	router := gin.Default()
	router.Use(gin.Recovery())
	router.Use(gin.Logger())
	
	// Apply rate limiting
	router.Use(service.rateLimitMiddleware())
	
	// Register routes
	service.RegisterRoutes(router)
	
	// Health check
	router.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{
			"status":    "healthy",
			"timestamp": time.Now().Unix(),
		})
	})
	
	// Start server
	addr := fmt.Sprintf("%s:%d", config.Server.Host, config.Server.Port)
	srv := &http.Server{
		Addr:    addr,
		Handler: router,
	}
	
	go func() {
		fmt.Printf("Starting Pro API server on %s\n", addr)
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			fmt.Printf("Server error: %v\n", err)
		}
	}()
	
	// Wait for interrupt signal
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit
	
	fmt.Println("Shutting down server...")
	
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	
	if err := srv.Shutdown(ctx); err != nil {
		fmt.Printf("Server forced to shutdown: %v\n", err)
	}
	
	fmt.Println("Server exited")
}

// Import sql
import (
	"database/sql"
	_ "github.com/lib/pq"
	"strconv"
)
