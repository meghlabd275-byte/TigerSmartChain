// Package apikey provides API key management with rate limiting
package apikey

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"database/sql"
	"encoding/base64"
	"encoding/hex"
	"errors"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds API key configuration
type Config struct {
	DBURL              string
	RedisURL           string
	DefaultRateLimit   int
	MaxRateLimit       int
	RateLimitWindow    time.Duration
	AdminRateLimit    int
	KeyLength         int
	KeyPrefix         string
	EnableIPWhitelist bool
}

// APIKey represents an API key
type APIKey struct {
	ID            int
	Key           string
	UserID        int
	Name          string
	RateLimit     int
	MonthlyLimit  int64
	MonthlyUsage int64
	ExpiresAt    *time.Time
	IsActive      bool
	IsAdmin      bool
	IPWhitelist  []string
	CreatedAt    time.Time
	LastUsedAt  *time.Time
}

// RateLimitInfo represents rate limit information
type RateLimitInfo struct {
	Allowed    bool
	Remaining int
	ResetAt   time.Time
	Limit     int
}

// Server represents the API key server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	rateLimits map[string]*RateLimitInfo
	rateLimitsMu sync.RWMutex
}

// NewServer creates a new API key server
func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 2})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	if err := createTables(ctx, pool); err != nil {
		return nil, fmt.Errorf("failed to create tables: %w", err)
	}
	return &Server{cfg: cfg, pool: pool, redis: rdb, rateLimits: make(map[string]*RateLimitInfo)}, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) error {
	queries := []string{
		`CREATE TABLE IF NOT EXISTS api_keys (id SERIAL PRIMARY KEY, key VARCHAR(64) UNIQUE NOT NULL, user_id INTEGER NOT NULL, name VARCHAR(255) NOT NULL, rate_limit INTEGER DEFAULT 60, monthly_limit BIGINT DEFAULT 1000000, monthly_usage BIGINT DEFAULT 0, expires_at TIMESTAMP, is_active BOOLEAN DEFAULT TRUE, is_admin BOOLEAN DEFAULT FALSE, ip_whitelist TEXT[], created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, last_used_at TIMESTAMP)`,
		`CREATE TABLE IF NOT EXISTS api_usage (id SERIAL PRIMARY KEY, key_id INTEGER REFERENCES api_keys(id) ON DELETE CASCADE, endpoint VARCHAR(255) NOT NULL, requests INTEGER DEFAULT 1, timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`,
		`CREATE INDEX IF NOT EXISTS idx_api_keys_user ON api_keys(user_id)`,
		`CREATE INDEX IF NOT EXISTS idx_api_keys_key ON api_keys(key)`,
	}
	for _, q := range queries {
		if _, err := pool.Exec(ctx, q); err != nil {
			return err
		}
	}
	return nil
}

// CreateKey creates a new API key
func (s *Server) CreateKey(ctx context.Context, userID int, name string, rateLimit int, expiresAt *time.Time, isAdmin bool) (*APIKey, error) {
	keyBytes := make([]byte, s.cfg.KeyLength)
	if _, err := rand.Read(keyBytes); err != nil {
		return nil, err
	}
	hash := sha256.Sum256(keyBytes)
	keyHash := hex.EncodeToString(hash[:])
	displayKey := s.cfg.KeyPrefix + base64.StdEncoding.EncodeToString(keyBytes)
	if rateLimit <= 0 {
		rateLimit = s.cfg.DefaultRateLimit
	}
	if rateLimit > s.cfg.MaxRateLimit && !isAdmin {
		rateLimit = s.cfg.MaxRateLimit
	}
	if isAdmin {
		rateLimit = s.cfg.AdminRateLimit
	}
	var key APIKey
	err := s.pool.QueryRow(ctx, `INSERT INTO api_keys (key, user_id, name, rate_limit, expires_at, is_admin) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id, key, user_id, name, rate_limit, expires_at, is_active, is_admin, created_at`, keyHash, userID, name, rateLimit, expiresAt, isAdmin).Scan(&key.ID, &key.Key, &key.UserID, &key.Name, &key.RateLimit, &key.ExpiresAt, &key.IsActive, &key.IsAdmin, &key.CreatedAt)
	if err != nil {
		return nil, err
	}
	key.Key = displayKey
	return &key, nil
}

// GetKeyByValue gets an API key by its value
func (s *Server) GetKeyByValue(ctx context.Context, keyValue string) (*APIKey, error) {
	if strings.HasPrefix(keyValue, s.cfg.KeyPrefix) {
		keyValue = strings.TrimPrefix(keyValue, s.cfg.KeyPrefix)
	}
	keyBytes, err := base64.StdEncoding.DecodeString(keyValue)
	if err != nil {
		return nil, errors.New("invalid API key format")
	}
	hash := sha256.Sum256(keyBytes)
	keyHash := hex.EncodeToString(hash[:])
	var key APIKey
	var ipWhitelist []sql.NullString
	err = s.pool.QueryRow(ctx, `SELECT id, key, user_id, name, rate_limit, monthly_limit, monthly_usage, expires_at, is_active, is_admin, ip_whitelist, created_at, last_used_at FROM api_keys WHERE key = $1`, keyHash).Scan(&key.ID, &key.Key, &key.UserID, &key.Name, &key.RateLimit, &key.MonthlyLimit, &key.MonthlyUsage, &key.ExpiresAt, &key.IsActive, &key.IsAdmin, &ipWhitelist, &key.CreatedAt, &key.LastUsedAt)
	if err != nil {
		return nil, err
	}
	key.Key = s.cfg.KeyPrefix + base64.StdEncoding.EncodeToString([]byte(keyHash))
	for _, v := range ipWhitelist {
		key.IPWhitelist = append(key.IPWhitelist, v.String)
	}
	return &key, nil
}

// ValidateKey validates an API key and checks rate limits
func (s *Server) ValidateKey(ctx context.Context, keyValue, endpoint, ipAddress string) (*APIKey, *RateLimitInfo, error) {
	key, err := s.GetKeyByValue(ctx, keyValue)
	if err != nil {
		return nil, nil, err
	}
	if !key.IsActive {
		return nil, nil, errors.New("API key is inactive")
	}
	if key.ExpiresAt != nil && time.Now().After(*key.ExpiresAt) {
		return nil, nil, errors.New("API key has expired")
	}
	if s.cfg.EnableIPWhitelist && len(key.IPWhitelist) > 0 {
		allowed := false
		for _, ip := range key.IPWhitelist {
			if ip == ipAddress {
				allowed = true
				break
			}
		}
		if !allowed {
			return nil, nil, errors.New("IP address not allowed")
		}
	}
	rateLimitInfo, err := s.CheckRateLimit(ctx, key, endpoint)
	if err != nil {
		return nil, nil, err
	}
	if !rateLimitInfo.Allowed {
		return key, rateLimitInfo, errors.New("rate limit exceeded")
	}
	if err := s.UpdateUsage(ctx, key.ID, endpoint); err != nil {
		return nil, nil, err
	}
	return key, rateLimitInfo, nil
}

// CheckRateLimit checks if the request is within rate limits
func (s *Server) CheckRateLimit(ctx context.Context, key *APIKey, endpoint string) (*RateLimitInfo, error) {
	keyStr := fmt.Sprintf("ratelimit:%d:%s", key.ID, endpoint)
	count, err := s.redis.Get(ctx, keyStr).Int()
	if err != nil && err != redis.Nil {
		return nil, err
	}
	if err == redis.Nil {
		var totalRequests int
		s.pool.QueryRow(ctx, `SELECT COALESCE(SUM(requests), 0) FROM api_usage WHERE key_id = $1 AND endpoint = $2 AND timestamp > $3`, key.ID, endpoint, time.Now().Add(-s.cfg.RateLimitWindow)).Scan(&totalRequests)
		count = totalRequests
	}
	allowed := count < key.RateLimit
	remaining := key.RateLimit - count
	if remaining < 0 {
		remaining = 0
	}
	info := &RateLimitInfo{Allowed: allowed, Remaining: remaining, Limit: key.RateLimit, ResetAt: time.Now().Add(s.cfg.RateLimitWindow)}
	s.redis.Set(ctx, keyStr, count+1, s.cfg.RateLimitWindow)
	return info, nil
}

// UpdateUsage updates the API key usage
func (s *Server) UpdateUsage(ctx context.Context, keyID int, endpoint string) error {
	_, err := s.pool.Exec(ctx, `INSERT INTO api_usage (key_id, endpoint, requests) VALUES ($1, $2, 1)`, keyID, endpoint)
	if err != nil {
		return err
	}
	_, err = s.pool.Exec(ctx, `UPDATE api_keys SET monthly_usage = monthly_usage + 1, last_used_at = NOW() WHERE id = $1`, keyID)
	return err
}

// GetUserKeys gets all API keys for a user
func (s *Server) GetUserKeys(ctx context.Context, userID int) ([]APIKey, error) {
	rows, err := s.pool.Query(ctx, `SELECT id, key, name, rate_limit, monthly_limit, monthly_usage, expires_at, is_active, is_admin, created_at, last_used_at FROM api_keys WHERE user_id = $1 ORDER BY created_at DESC`, userID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var keys []APIKey
	for rows.Next() {
		var key APIKey
		if err := rows.Scan(&key.ID, &key.Key, &key.Name, &key.RateLimit, &key.MonthlyLimit, &key.MonthlyUsage, &key.ExpiresAt, &key.IsActive, &key.IsAdmin, &key.CreatedAt, &key.LastUsedAt); err != nil {
			return nil, err
		}
		key.Key = s.cfg.KeyPrefix + "********"
		keys = append(keys, key)
	}
	return keys, nil
}

// RevokeKey revokes an API key
func (s *Server) RevokeKey(ctx context.Context, keyID int) error {
	result, err := s.pool.Exec(ctx, `UPDATE api_keys SET is_active = FALSE WHERE id = $1`, keyID)
	if err != nil {
		return err
	}
	if result.RowsAffected() == 0 {
		return errors.New("key not found")
	}
	s.redis.Del(ctx, fmt.Sprintf("ratelimit:%d:*", keyID))
	return nil
}