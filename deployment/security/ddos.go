// DDoS Protection Module for TigerScan
// Advanced distributed denial of service protection with dynamic filtering

package security

import (
	"context"
	"fmt"
	"net"
	"net/http"
	"regexp"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
)

// =============================================================================
// CONFIGURATION
// =============================================================================

// DDoSConfig holds DDoS protection configuration
type DDoSConfig struct {
	RedisClient          *redis.Client
	KeyPrefix          string
	BurstLimit         int           // Requests per second allowed in burst
	SustainedLimit     int           // Sustained requests per second
	BlockDuration     time.Duration // How long to block offenders
	AnalysisWindow    time.Duration // Window to analyze traffic
	GeoBlockEnabled   bool          // Enable geo-blocking
	AllowedCountries []string      // Allowed country codes
	BlockedCountries []string      // Blocked country codes
	CSEnabled        bool          // Enable challenge/response
	CSTimeout       time.Duration // Challenge timeout
}

// NewDDoSConfig creates default DDoS configuration
func NewDDoSConfig(redisClient *redis.Client) *DDoSConfig {
	return &DDoSConfig{
		RedisClient:        redisClient,
		KeyPrefix:        "tigerscan:ddos:",
		BurstLimit:       100,
		SustainedLimit:     50,
		BlockDuration:    15 * time.Minute,
		AnalysisWindow:   10 * time.Second,
		GeoBlockEnabled: false,
		AllowedCountries: []string{"US", "GB", "DE", "JP", "SG", "AU", "CA"},
		BlockedCountries: []string{},
		CSEnabled:       true,
		CSTimeout:      10 * time.Second,
	}
}

// =============================================================================
// PROTECTOR
// =============================================================================

// Protector provides DDoS protection
type Protector struct {
	config       *DDoSConfig
	ipStats      map[string]*IPStats
	mu           sync.RWMutex
	blocklist    map[string]time.Time
	blocklistMu   sync.RWMutex
	challenge   map[string]*Challenge
	challengeMu sync.RWMutex
}

// IPStats tracks IP traffic statistics
type IPStats struct {
	Requests     int
	FirstSeen   time.Time
	LastSeen   time.Time
	UserAgents  map[string]int
	Endpoints  map[string]int
	Countries map[string]int
}

// Challenge represents a CAPTCHA challenge
type Challenge struct {
	Token       string
	IP          string
	Created     time.Time
	Expiry      time.Time
	Completed   bool
}

// NewProtector creates a new DDoS protector
func NewProtector(config *DDoSConfig) *Protector {
	p := &Protector{
		config:    config,
		ipStats:  make(map[string]*IPStats),
		blocklist: make(map[string]time.Time),
		challenge: make(map[string]*Challenge),
	}
	
	// Start cleanup goroutine
	go p.cleanup()
	
	return p
}

// Middleware returns Gin middleware
func (p *Protector) Middleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		ctx := c.Request.Context()
		ip := getClientIP(c.Request)
		
		// Check if blocked
		if p.isBlocked(ip) {
			p.logAttack(ctx, ip, "blocked_ip")
			c.AbortWithStatusJSON(http.StatusForbidden, gin.H{
				"status":  "error",
				"message": "Access denied",
			})
			return
		}
		
		// Check rate limit
		if !p.checkRateLimit(ctx, ip) {
			p.blockIP(ctx, ip)
			p.logAttack(ctx, ip, "rate_limit_exceeded")
			c.AbortWithStatusJSON(http.StatusTooManyRequests, gin.H{
				"status":  "error",
				"message": "Rate limit exceeded",
				"retry_after": 60,
			})
			return
		}
		
		// Check for suspicious patterns
		if p.detectSuspicious(c.Request) {
			p.blockIP(ctx, ip)
			p.logAttack(ctx, ip, "suspicious_pattern")
			c.AbortWithStatusJSON(http.StatusForbidden, gin.H{
				"status":  "error",
				"message": "Suspicious activity detected",
			})
			return
		}
		
		// Record request
		p.recordRequest(ctx, ip, c.Request)
		
		c.Next()
	}
}

// =============================================================================
// PROTECTION LOGIC
// =============================================================================

func (p *Protector) checkRateLimit(ctx context.Context, ip string) bool {
	key := p.config.KeyPrefix + "rate:" + ip
	
	// Get current count
	count, err := p.config.RedisClient.Get(ctx, key).Int()
	if err != nil && err != redis.Nil {
		return true // On error, allow
	}
	
	// Increment
	p.config.RedisClient.Incr(ctx, key)
	p.config.RedisClient.Expire(ctx, key, p.config.AnalysisWindow)
	
	// Check limits
	if count > p.config.BurstLimit {
		return false
	}
	
	return true
}

func (p *Protector) isBlocked(ip string) bool {
	p.blocklistMu.RLock()
	defer p.blocklistMu.RUnlock()
	
	if expiry, exists := p.blocklist[ip]; exists {
		if time.Now().Before(expiry) {
			return true
		}
	}
	return false
}

func (p *Protector) blockIP(ctx context.Context, ip string) {
	p.blocklistMu.Lock()
	defer p.blocklistMu.Unlock()
	
	p.blocklist[ip] = time.Now().Add(p.config.BlockDuration)
	
	// Also store in Redis for distributed blocking
	key := p.config.KeyPrefix + "blocked:" + ip
	p.config.RedisClient.Set(ctx, key, "1", p.config.BlockDuration)
	
	// Log to security index
	p.logAttack(ctx, ip, "blocked")
}

func (p *Protector) recordRequest(ctx context.Context, ip string, req *http.Request) {
	p.mu.Lock()
	defer p.mu.Unlock()
	
	stats, exists := p.ipStats[ip]
	if !exists {
		stats = &IPStats{
			FirstSeen: time.Now(),
			UserAgents: make(map[string]int),
			Endpoints: make(map[string]int),
			Countries: make(map[string]int),
		}
		p.ipStats[ip] = stats
	}
	
	stats.Requests++
	stats.LastSeen = time.Now()
	
	// Record user agent
	ua := req.Header.Get("User-Agent")
	if ua != "" {
		stats.UserAgents[ua]++
	}
	
	// Record endpoint
	stats.Endpoints[req.URL.Path]++
}

func (p *Protector) detectSuspicious(req *http.Request) bool {
	// Check for common attack patterns
	suspiciousPatterns := []string{
		"../",
		"..\\",
		"%2e%2e",
		"%252e",
		"eval\\(",
		"base64_decode",
		"<script",
		"javascript:",
		"onerror=",
		"onload=",
		"union select",
		"union all select",
		"drop table",
		"drop database",
		"exec(",
		"xp_cmdshell",
	}
	
	path := req.URL.Path
	query := req.URL.Query()
	
	for _, pattern := range suspiciousPatterns {
		if contains(path, pattern) || contains(query.Encode(), pattern) {
			return true
		}
	}
	
	// Check for extremely long URLs
	if len(path) > 2048 {
		return true
	}
	
	// Check for too many parameters
	if len(query) > 50 {
		return true
	}
	
	return false
}

func (p *Protector) logAttack(ctx context.Context, ip string, attackType string) {
	key := p.config.KeyPrefix + "log:" + time.Now().Format("2006-01-02")
	
	data := fmt.Sprintf(`{"ip":"%s","type":"%s","timestamp":"%s","user_agent":"%s"}`,
		ip, attackType, time.Now().Format(time.RFC3339), "unknown")
	
	p.config.RedisClient.LPush(ctx, key, data)
	p.config.RedisClient.LTrim(ctx, key, 0, 10000)
	p.config.RedisClient.Expire(ctx, key, 7*24*time.Hour)
}

func (p *Protector) cleanup() {
	ticker := time.NewTicker(1 * time.Minute)
	defer ticker.Stop()
	
	for range ticker.C {
		p.blocklistMu.Lock()
		now := time.Now()
		for ip, expiry := range p.blocklist {
			if now.After(expiry) {
				delete(p.blocklist, ip)
			}
		}
		p.blocklistMu.Unlock()
		
		p.mu.Lock()
		for ip, stats := range p.ipStats {
			if now.Sub(stats.LastSeen) > 10*time.Minute {
				delete(p.ipStats, ip)
			}
		}
		p.mu.Unlock()
	}
}

// Helper functions
func getClientIP(req *http.Request) string {
	xff := req.Header.Get("X-Forwarded-For")
	if xff != "" {
		if i := regexp.MustCompile(",").Split(xff, -1); len(i) > 0 {
			return i[0]
		}
	}
	xri := req.Header.Get("X-Real-IP")
	if xri != "" {
		return xri
	}
	ip, _, _ := net.SplitHostPort(req.RemoteAddr)
	return ip
}

func contains(s, substr string) bool {
	return len(s) > 0 && len(substr) > 0 && 
		(len(s) >= len(substr)) && 
		(func() bool {
			for i := 0; i <= len(s)-len(substr); i++ {
				if s[i:i+len(substr)] == substr {
					return true
				}
			}
			return false
		})()
}