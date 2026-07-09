package gateway

import (
	"net/http"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"golang.org/x/time/rate"
)

// Middleware provides HTTP middleware
type Middleware struct {
	config Config
	limiter *rate.Limiter
}

// NewMiddleware creates a new middleware
func NewMiddleware(config Config) *Middleware {
	// Allow 100 requests per second with bursts of 200
	limiter := rate.NewLimiter(rate.Limit(config.RateLimitRPS), config.RateLimitBurst)
	
	return &Middleware{
		config:  config,
		limiter: limiter,
	}
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

// RateLimiter middleware
func (m *Middleware) RateLimiter() gin.HandlerFunc {
	return func(c *gin.Context) {
		// Skip rate limiting for health checks
		if c.Request.URL.Path == "/api/v1/health" {
			c.Next()
			return
		}
		
		// Get API key for per-key limiting
		apiKey := c.GetHeader("X-API-Key")
		if apiKey != "" {
			// Would implement per-key limiting here
		}
		
		if !m.limiter.Allow() {
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
