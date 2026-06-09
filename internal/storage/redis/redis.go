// Package redis provides Redis caching for TigerSmartChain.

package redis

import (
	"context"
	"fmt"
	"time"

	"github.com/go-redis/redis/v8"
)

// Cache provides Redis-based caching
type Cache struct {
	client *redis.Client
}

// Config holds Redis configuration
type Config struct {
	Address     string
	Password   string
	DB         int
	PoolSize   int
	MinIdleConn int
}

// NewCache creates new Redis cache
func NewCache(cfg *Config) (*Cache, error) {
	client := redis.NewClient(&redis.Options{
		Addr:     cfg.Address,
		Password: cfg.Password,
		DB:       cfg.DB,
		PoolSize:  cfg.PoolSize,
		MinIdleConn: cfg.MinIdleConn,
	})

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	if err := client.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to Redis: %w", err)
	}

	return &Cache{client: client}, nil
}

// Get retrieves value by key
func (c *Cache) Get(ctx context.Context, key string) (string, error) {
	return c.client.Get(ctx, key).Result()
}

// Set sets value with expiration
func (c *Cache) Set(ctx context.Context, key string, value string, expiration time.Duration) error {
	return c.client.Set(ctx, key, value, expiration).Err()
}

// Delete removes key
func (c *Cache) Delete(ctx context.Context, key string) error {
	return c.client.Del(ctx, key).Err()
}

// Exists checks if key exists
func (c *Cache) Exists(ctx context.Context, key string) (bool, error) {
	n, err := c.client.Exists(ctx, key).Result()
	return n > 0, err
}

// SetNX sets if not exists (for locking)
func (c *Cache) SetNX(ctx context.Context, key string, value string, expiration time.Duration) (bool, error) {
	return c.client.SetNX(ctx, key, value, expiration).Result()
}

// Incr increments counter
func (c *Cache) Incr(ctx context.Context, key string) (int64, error) {
	return c.client.Incr(ctx, key).Result()
}

// Decr decrements counter
func (c *Cache) Decr(ctx context.Context, key string) (int64, error) {
	return c.client.Decr(ctx, key).Result()
}

// Expire sets expiration on key
func (c *Cache) Expire(ctx context.Context, key string, expiration time.Duration) error {
	return c.client.Expire(ctx, key, expiration).Err()
}

// TTL returns time to live
func (c *Cache) TTL(ctx context.Context, key string) (time.Duration, error) {
	return c.client.TTL(ctx, key).Result()
}

// Close closes the connection
func (c *Cache) Close() error {
	return c.client.Close()
}

// RateLimiter provides rate limiting
type RateLimiter struct {
	cache *Cache
}

// NewRateLimiter creates new rate limiter
func NewRateLimiter(cache *Cache) *RateLimiter {
	return &RateLimiter{cache: cache}
}

// Allow checks if request is allowed
func (rl *RateLimiter) Allow(ctx context.Context, key string, limit int, window time.Duration) (bool, error) {
	key = "ratelimit:" + key

	count, err := rl.cache.client.Incr(ctx, key).Result()
	if err != nil {
		return false, err
	}

	if count == 1 {
		rl.cache.client.Expire(ctx, key, window)
	}

	return count <= int64(limit), nil
}

// Session provides session management
type Session struct {
	cache *Cache
}

// NewSession creates new session manager
func NewSession(cache *Cache) *Session {
	return &Session{cache: cache}
}

// Create creates new session
func (s *Session) Create(ctx context.Context, userID string, data map[string]interface{}, expiration time.Duration) (string, error) {
	sessionID := generateSessionID()
	key := "session:" + sessionID

	// Serialize data
	data["user_id"] = userID
	data["created_at"] = time.Now().Unix()

	// Store in Redis
	if err := s.cache.client.HMSet(ctx, key, data).Err(); err != nil {
		return "", err
	}

	// Set expiration
	s.cache.client.Expire(ctx, key, expiration)

	return sessionID, nil
}

// Get retrieves session data
func (s *Session) Get(ctx context.Context, sessionID string) (map[string]interface{}, error) {
	key := "session:" + sessionID
	data, err := s.cache.client.HGetAll(ctx, key).Result()
	if err != nil {
		return nil, err
	}

	if len(data) == 0 {
		return nil, fmt.Errorf("session not found")
	}

	result := make(map[string]interface{})
	for k, v := range data {
		result[k] = v
	}

	return result, nil
}

// Delete removes session
func (s *Session) Delete(ctx context.Context, sessionID string) error {
	key := "session:" + sessionID
	return s.cache.client.Del(ctx, key).Err()
}

// Extend extends session expiration
func (s *Session) Extend(ctx context.Context, sessionID string, expiration time.Duration) error {
	key := "session:" + sessionID
	return s.cache.client.Expire(ctx, key, expiration).Err()
}

// generateSessionID generates unique session ID
func generateSessionID() string {
	return fmt.Sprintf("sess_%d_%d", time.Now().UnixNano(), time.Now().Nanosecond())
}

// CacheStats returns cache statistics
func (c *Cache) CacheStats() (map[string]interface{}, error) {
	info, err := c.client.Info(context.Background()).Result()
	if err != nil {
		return nil, err
	}

	stats := make(map[string]interface{})
	// Parse info string
	// In production, parse the info string

	return stats, nil
}

var _ = redis.NewClient // Use redis
var _ = fmt.Sprintf   // Use fmt