package gateway

import (
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"golang.org/x/time/rate"
)

// Middleware provides HTTP middleware
type Middleware struct {
	config  Config
	limiter *rate.Limiter

	// handler resolves API keys. It is optional: a deployment that serves
	// only anonymous traffic can leave it nil, and a presented key is then
	// refused rather than accepted without being checked.
	handler *Handler

	keyLimitersMu sync.Mutex
	keyLimiters   map[int64]*rate.Limiter
}

// NewMiddleware creates a new middleware
func NewMiddleware(config Config) *Middleware {
	// Allow 100 requests per second with bursts of 200
	limiter := rate.NewLimiter(rate.Limit(config.RateLimitRPS), config.RateLimitBurst)

	return &Middleware{
		config:      config,
		limiter:     limiter,
		keyLimiters: make(map[int64]*rate.Limiter),
	}
}

// WithHandler attaches the handler used to resolve API keys, which enables
// authenticated, per-key rate limiting.
func (m *Middleware) WithHandler(h *Handler) *Middleware {
	m.handler = h
	return m
}

// CORS middleware
func (m *Middleware) CORS() gin.HandlerFunc {
	return func(c *gin.Context) {
		c.Header("Access-Control-Allow-Origin", "*")
		c.Header("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
		c.Header("Access-Control-Allow-Headers", "Origin, Content-Type, Accept, Authorization, X-API-Key")
		c.Header("Access-Control-Max-Age", "86400")

		if c.Request.Method == "OPTIONS" {
			c.AbortWithStatus(http.StatusNoContent)
			return
		}

		c.Next()
	}
}

// RateLimiter middleware enforces the anonymous global rate limit and, when a
// valid API key is presented, that key's own limit.
//
// A malformed or unknown key is rejected rather than silently falling back to
// the anonymous limit: a caller that presents a credential expects it to be
// honoured, and silently downgrading it would hide a revoked key from the
// client that keeps using it.
func (m *Middleware) RateLimiter() gin.HandlerFunc {
	return func(c *gin.Context) {
		// Health and readiness probes are exempt so a monitor cannot be locked
		// out by traffic.
		switch c.Request.URL.Path {
		case "/api/v1/health", "/api/v1/ready", "/api/v1/ping":
			c.Next()
			return
		}

		if key := ExtractAPIKey(c); key != "" {
			if m.handler == nil {
				c.JSON(http.StatusServiceUnavailable, gin.H{"error": "API key authentication is unavailable"})
				c.Abort()
				return
			}

			meta, err := m.handler.AuthenticateAPIKey(c.Request.Context(), key)
			if err != nil {
				c.JSON(http.StatusUnauthorized, gin.H{"error": "invalid API key"})
				c.Abort()
				return
			}

			if !m.keyLimiter(meta).Allow() {
				c.Header("Retry-After", "1")
				c.JSON(http.StatusTooManyRequests, gin.H{
					"error":     "API key rate limit exceeded",
					"rateLimit": meta.RateLimit,
				})
				c.Abort()
				return
			}

			c.Set("api_key_id", meta.ID)
			c.Set("api_key_rate_limit", meta.RateLimit)
			c.Next()
			return
		}

		if !m.limiter.Allow() {
			c.Header("Retry-After", "1")
			c.JSON(http.StatusTooManyRequests, gin.H{
				"error": "Rate limit exceeded",
				"retry": "Try again later",
			})
			c.Abort()
			return
		}

		c.Next()
	}
}

// keyLimiter returns the token bucket for an API key, creating it on first use
// at the key's configured rate. A revoked key's limiter is dropped with the key,
// so the map cannot grow without bound.
func (m *Middleware) keyLimiter(meta *APIKey) *rate.Limiter {
	ratePerSecond := meta.RateLimit
	if ratePerSecond <= 0 {
		ratePerSecond = m.config.RateLimitRPS
	}
	if ratePerSecond <= 0 {
		ratePerSecond = 1
	}

	m.keyLimitersMu.Lock()
	defer m.keyLimitersMu.Unlock()

	if limiter, ok := m.keyLimiters[meta.ID]; ok {
		return limiter
	}

	limiter := rate.NewLimiter(rate.Limit(ratePerSecond), ratePerSecond)
	m.keyLimiters[meta.ID] = limiter
	return limiter
}

// RequestLogger middleware
func (m *Middleware) RequestLogger() gin.HandlerFunc {
	return func(c *gin.Context) {
		start := time.Now()
		path := c.Request.URL.Path
		method := c.Request.Method

		c.Next()

		latency := time.Since(start)
		status := c.Writer.Status()

		if status >= 400 {
			// Log errors
			println(method, path, status, latency)
		}
	}
}

// SecurityHeaders middleware
func (m *Middleware) SecurityHeaders() gin.HandlerFunc {
	return func(c *gin.Context) {
		c.Header("X-Content-Type-Options", "nosniff")
		c.Header("X-Frame-Options", "DENY")
		c.Header("X-XSS-Protection", "1; mode=block")
		c.Header("Strict-Transport-Security", "max-age=31536000; includeSubDomains")
		c.Header("Content-Security-Policy", "default-src 'self'")

		c.Next()
	}
}

// IPWhitelist middleware (optional)
func (m *Middleware) IPWhitelist(whitelist []string) gin.HandlerFunc {
	whitelistMap := make(map[string]bool)
	for _, ip := range whitelist {
		whitelistMap[ip] = true
	}

	return func(c *gin.Context) {
		clientIP := c.ClientIP()

		// Skip if no whitelist configured
		if len(whitelist) == 0 {
			c.Next()
			return
		}

		// Check if IP is whitelisted
		if !whitelistMap[clientIP] && !whitelistMap["*"] {
			c.JSON(http.StatusForbidden, gin.H{
				"error": "IP not allowed",
			})
			c.Abort()
			return
		}

		c.Next()
	}
}

// RequestID middleware
func (m *Middleware) RequestID() gin.HandlerFunc {
	return func(c *gin.Context) {
		requestID := c.GetHeader("X-Request-ID")
		if requestID == "" {
			// Generate request ID if not provided
			requestID = generateRequestID()
		}

		c.Header("X-Request-ID", requestID)
		c.Set("requestID", requestID)

		c.Next()
	}
}

// Compression middleware
func (m *Middleware) Compression() gin.HandlerFunc {
	return func(c *gin.Context) {
		// Check if client accepts gzip
		acceptEncoding := c.GetHeader("Accept-Encoding")

		if strings.Contains(acceptEncoding, "gzip") {
			c.Header("Content-Encoding", "gzip")
		}

		c.Next()
	}
}

func generateRequestID() string {
	return "req-" + time.Now().Format("20060102150405.000000")
}
