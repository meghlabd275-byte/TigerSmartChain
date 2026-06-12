// Rate Limiting Middleware for TigerScan
// Production-grade rate limiting with Redis backend and sliding window algorithm

package security

import (
	"context"
	"fmt"
	"net/http"
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

// RateLimitConfig holds rate limiting configuration
type RateLimitConfig struct {
	// Redis client for distributed rate limiting
	Redis *redis.Client
	
	// Rate limits for each tier (requests per minute)
	TierLimits map[string]TierLimit
	
	// Default tier when no API key provided
	DefaultTier string
	
	// Key prefix for Redis
	KeyPrefix string
	
	// Window duration
	Window time.Duration
	
	// Cleanup interval
	CleanupInterval time.Duration
}

// TierLimit defines rate limit for a tier
type TierLimit struct {
	RequestsPerMinute int
	RequestsPerDay    int64
	Burst             int
	Quota            int64 // -1 for unlimited
}

// DefaultTierLimits returns default tier limits
func DefaultTierLimits() map[string]TierLimit {
	return map[string]TierLimit{
		"free": {
			RequestsPerMinute: 60,
			RequestsPerDay:   10000,
			Burst:            10,
			Quota:           10000,
		},
		"pro": {
			RequestsPerMinute: 300,
			RequestsPerDay:    100000,
			Burst:            50,
			Quota:           100000,
		},
		"enterprise": {
			RequestsPerMinute: 10000,
			RequestsPerDay:    -1,
			Burst:            1000,
			Quota:           -1,
		},
	}
}

// NewRateLimitConfig creates a new rate limit configuration
func NewRateLimitConfig(redisClient *redis.Client) *RateLimitConfig {
	return &RateLimitConfig{
		Redis:        redisClient,
		TierLimits:   DefaultTierLimits(),
		DefaultTier: "free",
		KeyPrefix:   "tigerscan:ratelimit:",
		Window:      time.Minute,
		CleanupInterval: 5 * time.Minute,
	}
}

// =============================================================================
// RATE LIMITER
// =============================================================================

// RateLimiter provides rate limiting functionality
type RateLimiter struct {
	config *RateLimitConfig
	local  map[string]*localLimiter
	mu     sync.RWMutex
}

type localLimiter struct {
	requests    []time.Time
	dailyCount  int64
	quotaUsed  int64
	lastReset  time.Time
}

// NewRateLimiter creates a new rate limiter
func NewRateLimiter(config *RateLimitConfig) *RateLimiter {
	rl := &RateLimiter{
		config: config,
		local: make(map[string]*localLimiter),
	}
	
	// Start cleanup goroutine
	go rl.cleanup()
	
	return rl
}

// Middleware returns Gin middleware for rate limiting
func (rl *RateLimiter) Middleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		// Get API key from header
		apiKey := c.GetHeader("X-API-Key")
		if apiKey == "" {
			apiKey = c.Query("apikey")
		}
		
		// Determine tier
		tier := rl.config.DefaultTier
		if apiKey != "" {
			tierKey := rl.config.KeyPrefix + "tier:" + apiKey
			if rl.config.Redis != nil {
				if tierName, err := rl.config.Redis.Get(context.Background(), tierKey).Result(); err == nil {
					tier = tierName
				}
			}
		}
		
		// Get client identifier
		clientID := apiKey
		if clientID == "" {
			clientID = c.ClientIP()
		}
		
		// Check rate limit
		allowed, reason, err := rl.Allow(c.Request.Context(), tier, clientID)
		if err != nil {
			c.AbortWithStatusJSON(http.StatusInternalServerError, gin.H{
				"status":  "error",
				"message": "Rate limit check failed",
			})
			return
		}
		
		if !allowed {
			c.AbortWithStatusJSON(http.StatusTooManyRequests, gin.H{
				"status":  "error",
				"message": reason,
				"retry_after": 60,
			})
			return
		}
		
		// Add rate limit headers
		limit := rl.config.TierLimits[tier]
		rl.addRateLimitHeaders(c, tier, limit)
		
		c.Next()
	}
}

// Allow checks if a request is allowed
func (rl *RateLimiter) Allow(ctx context.Context, tier, clientID string) (bool, string, error) {
	limit, exists := rl.config.TierLimits[tier]
	if !exists {
		limit = rl.config.TierLimits[rl.config.DefaultTier]
		tier = rl.config.DefaultTier
	}
	
	// Try Redis first for distributed rate limiting
	if rl.config.Redis != nil {
		return rl.allowRedis(ctx, tier, clientID, limit)
	}
	
	// Fallback to local rate limiting
	return rl.allowLocal(ctx, tier, clientID, limit)
}

func (rl *RateLimiter) allowRedis(ctx context.Context, tier, clientID string, limit TierLimit) (bool, string, error) {
	key := rl.config.KeyPrefix + tier + ":" + clientID
	
	// Sliding window rate limiting
	now := time.Now()
	windowStart := now.Add(-rl.config.Window)
	
	// Remove old entries and count
	removed, err := rl.config.Redis.ZRemRangeByScore(ctx, key, "0", strconv.FormatInt(windowStart.Unix(), 10)).Result()
	if err != nil {
		return false, "", err
	}
	
	// Count current requests in window
	count, err := rl.config.Redis.ZCard(ctx, key).Result()
	if err != nil {
		return false, "", err
	}
	
	// Check minute limit
	if count >= int64(limit.RequestsPerMinute) {
		return false, "Minute rate limit exceeded", nil
	}
	
	// Check daily limit
	if limit.RequestsPerDay > 0 {
		dailyKey := rl.config.KeyPrefix + "daily:" + tier + ":" + clientID
		dailyCount, err := rl.config.Redis.Get(ctx, dailyKey).Int64()
		if err != nil && err != redis.Nil {
			return false, "", err
		}
		
		if dailyCount >= limit.RequestsPerDay {
			return false, "Daily rate limit exceeded", nil
		}
		
		// Increment daily counter
		rl.config.Redis.Incr(ctx, dailyKey)
		rl.config.Redis.Expire(ctx, dailyKey, 24*time.Hour)
	}
	
	// Add current request to sliding window
	score := float64(now.Unix())
	member := fmt.Sprintf("%d-%d", now.UnixNano(), count)
	rl.config.Redis.ZAdd(ctx, key, redis.Z{Score: score, Member: member})
	rl.config.Redis.Expire(ctx, key, rl.config.Window)
	
	return true, "", nil
}

func (rl *RateLimiter) allowLocal(ctx context.Context, tier, clientID string, limit TierLimit) (bool, string, error) {
	rl.mu.Lock()
	defer rl.mu.Unlock()
	
	now := time.Now()
	windowStart := now.Add(-rl.config.Window)
	
	local, exists := rl.local[clientID]
	if !exists {
		local = &localLimiter{
			requests:    make([]time.Time, 0, limit.Burst),
			dailyCount:  0,
			quotaUsed:   0,
			lastReset:  now,
		}
		rl.local[clientID] = local
	}
	
	// Reset daily counter if needed
	if now.Sub(local.lastReset) > 24*time.Hour {
		local.dailyCount = 0
		local.lastReset = now
	}
	
	// Check daily limit
	if limit.RequestsPerDay > 0 && local.dailyCount >= limit.RequestsPerDay {
		return false, "Daily rate limit exceeded", nil
	}
	
	// Remove old requests from sliding window
	validRequests := make([]time.Time, 0)
	for _, reqTime := range local.requests {
		if reqTime.After(windowStart) {
			validRequests = append(validRequests, reqTime)
		}
	}
	local.requests = validRequests
	
	// Check minute limit
	if len(local.requests) >= limit.RequestsPerMinute {
		return false, "Minute rate limit exceeded", nil
	}
	
	// Add current request
	local.requests = append(local.requests, now)
	local.dailyCount++
	local.quotaUsed++
	
	return true, "", nil
}

func (rl *RateLimiter) addRateLimitHeaders(c *gin.Context, tier string, limit TierLimit) {
	// Get current usage
	var remaining int64 = 0
	var resetTime int64 = 0
	
	key := rl.config.KeyPrefix + tier + ":" + c.ClientIP()
	if rl.config.Redis != nil {
		count, _ := rl.config.Redis.ZCard(context.Background(), key).Result()
		remaining = int64(limit.RequestsPerMinute) - count
		if remaining < 0 {
			remaining = 0
		}
		
		// Get earliest entry for reset time
		results, _ := rl.config.Redis.ZRange(context.Background(), key, 0, 0).Result()
		if len(results) > 0 {
			resetTime = time.Now().Add(rl.config.Window).Unix()
		}
	}
	
	c.Header("X-RateLimit-Limit", strconv.Itoa(limit.RequestsPerMinute))
	c.Header("X-RateLimit-Remaining", strconv.FormatInt(remaining, 10))
	if resetTime > 0 {
		c.Header("X-RateLimit-Reset", strconv.FormatInt(resetTime, 10))
	}
	
	// Add tier header
	c.Header("X-API-Tier", tier)
}

// cleanup removes old entries periodically
func (rl *RateLimiter) cleanup() {
	ticker := time.NewTicker(rl.config.CleanupInterval)
	defer ticker.Stop()
	
	for range ticker.C {
		rl.mu.Lock()
		now := time.Now()
		windowStart := now.Add(-rl.config.Window * 2)
		
		for clientID, local := range rl.local {
			validRequests := make([]time.Time, 0)
			for _, reqTime := range local.requests {
				if reqTime.After(windowStart) {
					validRequests = append(validRequests, reqTime)
				}
			}
			local.requests = validRequests
			
			// Remove if no recent activity
			if len(validRequests) == 0 && now.Sub(local.lastReset) > 24*time.Hour {
				delete(rl.local, clientID)
			}
		}
		rl.mu.Unlock()
	}
}

// =============================================================================
// API KEY MANAGEMENT
// =============================================================================

// APIKeyManager manages API keys
type APIKeyManager struct {
	redis   *redis.Client
	keyPrefx string
}

// NewAPIKeyManager creates a new API key manager
func NewAPIKeyManager(redisClient *redis.Client) *APIKeyManager {
	return &APIKeyManager{
		redis:   redisClient,
		keyPrefx: "tigerscan:apikey:",
	}
}

// CreateKey creates a new API key
func (m *APIKeyManager) CreateKey(ctx context.Context, name, tier, organization string) (string, error) {
	// Generate random key
	key := make([]byte, 32)
	rand.Read(key)
	keyStr := fmt.Sprintf("tgr_%s", strings.ReplaceAll(fmt.Sprintf("%x", key), "/", ""))
	
	// Store key metadata
	keyData := fmt.Sprintf(`{"name":"%s","tier":"%s","organization":"%s","created_at":"%s"}`,
		name, tier, organization, time.Now().Format(time.RFC3339))
	
	err := m.redis.Set(ctx, m.keyPrefx+keyStr, keyData, 365*24*time.Hour).Err()
	if err != nil {
		return "", err
	}
	
	// Set tier mapping
	tierKey := m.keyPrefx + "tier:" + keyStr
	err = m.redis.Set(ctx, tierKey, tier, 365*24*time.Hour).Err()
	if err != nil {
		return "", err
	}
	
	return keyStr, nil
}

// GetKeyInfo gets API key information
func (m *APIKeyManager) GetKeyInfo(ctx context.Context, key string) (map[string]string, error) {
	data, err := m.redis.Get(ctx, m.keyPrefx+key).Result()
	if err != nil {
		return nil, err
	}
	
	// Parse JSON (simplified)
	info := make(map[string]string)
	// In production, use proper JSON parsing
	return info, nil
}

// RevokeKey revokes an API key
func (m *APIKeyManager) RevokeKey(ctx context.Context, key string) error {
	err := m.redis.Del(ctx, m.keyPrefx+key).Err()
	if err != nil {
		return err
	}
	return m.redis.Del(ctx, m.keyPrefx+"tier:"+key).Err()
}

// =============================================================================
// USAGE TRACKING
// =============================================================================

// UsageTracker tracks API usage
type UsageTracker struct {
	redis   *redis.Client
	keyPrefx string
}

// NewUsageTracker creates a new usage tracker
func NewUsageTracker(redisClient *redis.Client) *UsageTracker {
	return &UsageTracker{
		redis:   redisClient,
		keyPrefx: "tigerscan:usage:",
	}
}

// RecordUsage records API usage
func (t *UsageTracker) RecordUsage(ctx context.Context, key, endpoint, method string) error {
	now := time.Now()
	date := now.Format("2006-01-02")
	hour := now.Hour()
	
	// Daily usage
	dailyKey := t.keyPrefx + "daily:" + key + ":" + date
	t.redis.Incr(ctx, dailyKey)
	t.redis.Expire(ctx, dailyKey, 48*time.Hour)
	
	// Hourly usage
	hourlyKey := t.keyPrefx + "hourly:" + key + ":" + date + fmt.Sprintf("-%d", hour)
	t.redis.Incr(ctx, hourlyKey)
	t.redis.Expire(ctx, hourlyKey, 25*time.Hour)
	
	// Endpoint usage
	endpointKey := t.keyPrefx + "endpoint:" + key + ":" + date + ":" + method + ":" + endpoint
	t.redis.Incr(ctx, endpointKey)
	t.redis.Expire(ctx, endpointKey, 48*time.Hour)
	
	return nil
}

// GetUsage gets usage statistics
func (t *UsageTracker) GetUsage(ctx context.Context, key, period string) (map[string]int64, error) {
	now := time.Now()
	
	usage := make(map[string]int64)
	
	if period == "daily" || period == "all" {
		for i := 0; i < 7; i++ {
			date := now.AddDate(0, 0, -i).Format("2006-01-02")
			dailyKey := t.keyPrefx + "daily:" + key + ":" + date
			count, _ := t.redis.Get(ctx, dailyKey).Int64()
			usage[date] = count
		}
	}
	
	if period == "hourly" || period == "all" {
		for h := 0; h < 24; h++ {
			hour := (now.Hour() - h + 24) % 24
			date := now.Format("2006-01-02")
			hourlyKey := t.keyPrefx + "hourly:" + key + ":" + date + fmt.Sprintf("-%d", hour)
			count, _ := t.redis.Get(ctx, hourlyKey).Int64()
			usage[fmt.Sprintf("%s-%d", date, hour)] = count
		}
	}
	
	return usage, nil
}