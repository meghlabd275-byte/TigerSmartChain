// Package ratelimit provides advanced rate limiting with Redis
package ratelimit

import (
	"context"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/redis/go-redis/v9"
)

type Config struct {
	RedisURL      string
	DefaultLimit  int
	WindowSize    time.Duration
	BurstSize    int
	BlockDuration time.Duration
}

type Limiter struct {
	cfg   *Config
	redis *redis.Client
	mu    sync.RWMutex
}

type RateLimitResult struct {
	Allowed    bool
	Remaining int
	ResetAt   time.Time
	Limit     int
}

func NewLimiter(cfg *Config) (*Limiter, error) {
	rdb := redis.NewClient(&redis.Options{
		Addr:     cfg.RedisURL,
		Password: "",
		DB:       17,
	})
	
	ctx := context.Background()
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, err
	}
	
	return &Limiter{cfg: cfg, redis: rdb}, nil
}

func (l *Limiter) Allow(key string) (*RateLimitResult, error) {
	return l.AllowN(key, 1)
}

func (l *Limiter) AllowN(key string, n int) (*RateLimitResult, error) {
	ctx := context.Background()
	
	now := time.Now()
	windowStart := now.Add(-l.cfg.WindowSize).Unix()
	
	keyFull := fmt.Sprintf("ratelimit:%s", key)
	
	pipe := l.redis.Pipeline()
	
	countCmd := pipe.ZCount(ctx, keyFull, float64(windowStart), "+inf")
	pipe.Expire(ctx, keyFull, l.cfg.WindowSize)
	
	_, err := pipe.Exec(ctx)
	if err != nil {
		return nil, err
	}
	
	currentCount := countCmd.Val()
	
	allowed := int(currentCount)+n <= l.cfg.DefaultLimit
	
	remaining := l.cfg.DefaultLimit - int(currentCount) - n
	if remaining < 0 {
		remaining = 0
	}
	
	if allowed {
		l.redis.ZAdd(ctx, keyFull, redis.Z{
			Score: float64(now.Unix()),
			Member: fmt.Sprintf("%d:%d", now.Unix(), now.Nanosecond()),
		})
	}
	
	resetAt := now.Add(l.cfg.WindowSize)
	
	return &RateLimitResult{
		Allowed:    allowed,
		Remaining: remaining,
		ResetAt:   resetAt,
		Limit:     l.cfg.DefaultLimit,
	}, nil
}

func (l *Limiter) Block(key string, duration time.Duration) error {
	ctx := context.Background()
	keyBlocked := fmt.Sprintf("blocked:%s", key)
	return l.redis.Set(ctx, keyBlocked, "1", duration).Err()
}

func (l *Limiter) IsBlocked(key string) (bool, error) {
	ctx := context.Background()
	keyBlocked := fmt.Sprintf("blocked:%s", key)
	result, err := l.redis.Exists(ctx, keyBlocked).Result()
	if err != nil {
		return false, err
	}
	return result > 0, nil
}

func (l *Limiter) Unblock(key string) error {
	ctx := context.Background()
	keyBlocked := fmt.Sprintf("blocked:%s", key)
	return l.redis.Del(ctx, keyBlocked).Err()
}

func (l *Limiter) Middleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		key := getRateLimitKey(r)
		
		blocked, err := l.IsBlocked(key)
		if err == nil && blocked {
			http.Error(w, "rate limited", http.StatusTooManyRequests)
			return
		}
		
		result, err := l.Allow(key)
		if err != nil {
			next.ServeHTTP(w, r)
			return
		}
		
		w.Header().Set("X-RateLimit-Limit", strconv.Itoa(result.Limit))
		w.Header().Set("X-RateLimit-Remaining", strconv.Itoa(result.Remaining))
		w.Header().Set("X-RateLimit-Reset", strconv.FormatInt(result.ResetAt.Unix(), 10))
		
		if !result.Allowed {
			http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
			return
		}
		
		next.ServeHTTP(w, r)
	})
}

func getRateLimitKey(r *http.Request) string {
	ip := getClientIP(r)
	apiKey := r.Header.Get("X-API-Key")
	
	if apiKey != "" {
		return fmt.Sprintf("apikey:%s", apiKey)
	}
	
	return fmt.Sprintf("ip:%s", ip)
}

func getClientIP(r *http.Request) string {
	forwarded := r.Header.Get("X-Forwarded-For")
	if forwarded != "" {
		parts := strings.Split(forwarded, ",")
		return strings.TrimSpace(parts[0])
	}
	
	realIP := r.Header.Get("X-Real-IP")
	if realIP != "" {
		return realIP
	}
	
	return r.RemoteAddr
}

func (l *Limiter) GetStats(ctx context.Context) (map[string]int, error) {
	keys, err := l.redis.Keys(ctx, "ratelimit:*").Result()
	if err != nil {
		return nil, err
	}
	
	stats := make(map[string]int)
	for _, key := range keys {
		count, err := l.redis.ZCard(ctx, key).Result()
		if err != nil {
			continue
		}
		stats[strings.TrimPrefix(key, "ratelimit:")] = int(count)
	}
	
	return stats, nil
}

func (l *Limiter) Cleanup(ctx context.Context) error {
	keys, err := l.redis.Keys(ctx, "ratelimit:*").Result()
	if err != nil {
		return err
	}
	
	now := time.Now().Unix()
	
	for _, key := range keys {
		l.redis.ZRemRangeByScore(ctx, key, "-inf", fmt.Sprintf("%d", now-86400))
	}
	
	return nil
}

type TokenLimiter struct {
	redis    *redis.Client
	tokens   int
	refillRate time.Duration
	mu       sync.Mutex
}

func NewTokenLimiter(redisURL string, tokens int, refillRate time.Duration) (*TokenLimiter, error) {
	rdb := redis.NewClient(&redis.Options{
		Addr:     redisURL,
		Password: "",
		DB:       18,
	})
	
	ctx := context.Background()
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, err
	}
	
	return &TokenLimiter{
		redis:    rdb,
		tokens:   tokens,
		refillRate: refillRate,
	}, nil
}

func (t *TokenLimiter) TryConsume(key string, tokens int) (bool, error) {
	ctx := context.Background()
	
	keyFull := fmt.Sprintf("tokenlimiter:%s", key)
	
	current, err := t.redis.Get(ctx, keyFull).Int()
	if err == redis.Nil {
		current = t.tokens
	} else if err != nil {
		return false, err
	}
	
	if current >= tokens {
		newVal := current - tokens
		t.redis.Set(ctx, keyFull, newVal, t.refillRate)
		return true, nil
	}
	
	return false, nil
}

func (t *TokenLimiter) Reset(key string) error {
	ctx := context.Background()
	keyFull := fmt.Sprintf("tokenlimiter:%s", key)
	return t.redis.Del(ctx, keyFull).Err()
}
