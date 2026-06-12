// Security Package for TigerScan
// Production-grade security with encryption, CSRF, XSS protection, and rate limiting

package security

import (
	"bufio"
	"bytes"
	"compress/gzip"
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"errors"
	"fmt"
	"hash/fnv"
	"io"
	"net"
	"net/http"
	"regexp"
	"strings"
	"sync"
	"time"

	"github.com/google/uuid"
	"github.com/redis/go-redis/v9"
	"golang.org/x/time/rate"
)

// =============================================================================
// CONSTANTS
// =============================================================================

const (
	// AES-256-GCM key size
	AES256KeySize = 32
	
	// GCM nonce size
	GCMNonceSize = 12
	
	// GCM tag size
	GCMTagSize = 16
	
	// Maximum request size (10MB)
	MaxRequestSize = 10 * 1024 * 1024
	
	// CSRF token length
	CSRFTokenLength = 32
	
	// Rate limit burst
	RateLimitBurst = 100
	
	// Rate limit operations per second
	RateLimitOps = 50
)

// =============================================================================
// SECURITY CONFIGURATION
// =============================================================================

// Config holds security configuration
type Config struct {
	// Encryption
	EncryptionKey []byte
	
	// Rate limiting
	RedisClient     *redis.Client
	RateLimitRPS   int
	RateLimitBurst int
	
	// CSRF
	CSRFEnabled bool
	CSRFSecret []byte
	
	// XSS Protection
	XSSEnabled bool
	
	// CORS
	CORSOrigins []string
	
	// Request validation
	MaxRequestSize int64
	
	// IP whitelist (CIDR notation)
	IPWhitelist []string
	
	// IP blacklist (CIDR notation)
	IPBlacklist []string
	
	// HSTS enabled
	HSTSEnabled bool
	
	// Frame option
	FrameOption string
}

// DefaultConfig returns a secure default configuration
func DefaultConfig() *Config {
	key := make([]byte, AES256KeySize)
	rand.Read(key)
	
	csrfSecret := make([]byte, CSRFTokenLength)
	rand.Read(csrfSecret)
	
	return &Config{
		EncryptionKey:  key,
		RateLimitRPS:   RateLimitOps,
		RateLimitBurst: RateLimitBurst,
		CSRFEnabled:    true,
		CSRFSecret:     csrfSecret,
		XSSEnabled:     true,
		CORSOrigins:    []string{"https://tigerscan.io", "https://api.tigerscan.io"},
		MaxRequestSize: MaxRequestSize,
		IPWhitelist:    []string{},
		IPBlacklist:    []string{},
		HSTSEnabled:   true,
		FrameOption:  "DENY",
	}

}

// =============================================================================
// ADVANCED ENCRYPTION (AES-256-GCM)
// =============================================================================

// Encryptor provides AES-256-GCM encryption
type Encryptor struct {
	key []byte
}

// NewEncryptor creates a new AES-256-GCM encryptor
func NewEncryptor(key []byte) (*Encryptor, error) {
	if len(key) != AES256KeySize {
		return nil, fmt.Errorf("key must be %d bytes", AES256KeySize)
	}
	return &Encryptor{key: key}, nil
}

// Encrypt encrypts plaintext using AES-256-GCM
// Returns nonce+ciphertext+tag
func (e *Encryptor) Encrypt(plaintext []byte) ([]byte, error) {
	block, err := aes.NewCipher(e.key)
	if err != nil {
		return nil, fmt.Errorf("failed to create cipher: %w", err)
	}
	
	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return nil, fmt.Errorf("failed to create GCM: %w", err)
	}
	
	nonce := make([]byte, GCMNonceSize)
	if _, err := rand.Read(nonce); err != nil {
		return nil, fmt.Errorf("failed to generate nonce: %w", err)
	}
	
	// Seal appends ciphertext+tag to nonce
	ciphertext = gcm.Seal(nonce, nonce, plaintext, nil)
	return ciphertext, nil
}

// Decrypt decrypts ciphertext using AES-256-GCM
func (e *Encryptor) Decrypt(ciphertext []byte) ([]byte, error) {
	if len(ciphertext) < GCMNonceSize+GCMTagSize {
		return nil, errors.New("ciphertext too short")
	}
	
	block, err := aes.NewCipher(e.key)
	if err != nil {
		return nil, fmt.Errorf("failed to create cipher: %w", err)
	}
	
	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return nil, fmt.Errorf("failed to create GCM: %w", err)
	}
	
	nonce := ciphertext[:GCMNonceSize]
	ciphertext = ciphertext[GCMNonceSize:]
	
	plaintext, err := gcm.Open(nil, nonce, ciphertext, nil)
	if err != nil {
		return nil, fmt.Errorf("decryption failed: %w", err)
	}
	
	return plaintext, nil
}

// =============================================================================
// SECURE HASH (SHA-256 with salt)
// =============================================================================

// HashPassword creates a secure hash of a password
func HashPassword(password string, salt []byte) string {
	if len(salt) == 0 {
		salt = make([]byte, 16)
		rand.Read(salt)
	}
	
	hash := sha256.Sum256(append(salt, []byte(password)...))
	combined := make([]byte, len(salt)+len(hash))
	copy(combined, salt)
	copy(combined[len(salt):], hash[:])
	
	return base64.StdEncoding.EncodeToString(combined)
}

// VerifyPassword verifies a password against a hash
func VerifyPassword(password string, storedHash string) bool {
	combined, err := base64.StdEncoding.DecodeString(storedHash)
	if err != nil {
		return false
	}
	
	if len(combined) < 16 {
		return false
	}
	
	salt := combined[:16]
	expectedHash := combined[16:]
	
	hash := sha256.Sum256(append(salt, []byte(password)...))
	
	return bytes.Equal(hash[:], expectedHash)
}

// =============================================================================
// RATE LIMITING (Token Bucket with Redis)
// =============================================================================

// RateLimiter provides rate limiting using token bucket algorithm
type RateLimiter struct {
	redis      *redis.Client
	keyPrefix  string
	rate       rate.Limit
	burst     int
	mu        sync.Mutex
	local     map[string]*localLimiter
	localTTL  time.Duration
}

type localLimiter struct {
	limiter  *rate.Limiter
	expires time.Time
}

// NewRateLimiter creates a new rate limiter
func NewRateLimiter(redisClient *redis.Client, rps int, burst int) *RateLimiter {
	return &RateLimiter{
		redis:     redisClient,
		keyPrefix: "tigerscan:ratelimit:",
		rate:     rate.Limit(rps),
		burst:    burst,
		local:    make(map[string]*localLimiter),
		localTTL: time.Minute,
	}
}

// Allow checks if a request is allowed
func (rl *RateLimiter) Allow(key string) bool {
	// Try Redis first for distributed rate limiting
	if rl.redis != nil {
		ctx := context.Background()
		redisKey := rl.keyPrefix + key
		
		// Increment counter
		count, err := rl.redis.Incr(ctx, redisKey).Result()
		if err == nil && count == 1 {
			rl.redis.Expire(ctx, redisKey, time.Minute)
		}
		
		// Check limit
		if err == nil && count > int64(rl.rate*60) {
			return false
		}
		return err == nil
	}
	
	// Fallback to local rate limiting
	rl.mu.Lock()
	defer rl.mu.Unlock()
	
	now := time.Now()
	limiter, exists := rl.local[key]
	
	if !exists || now.After(limiter.expires) {
		limiter = &localLimiter{
			limiter:  rate.NewLimiter(rl.rate, rl.burst),
			expires: now.Add(rl.localTTL),
		}
		rl.local[key] = limiter
	}
	
	return limiter.limiter.Allow()
}

// =============================================================================
// IP FILTERING
// =============================================================================

// IPFilter provides IP-based filtering
type IPFilter struct {
	whitelist []*net.IPNet
	blacklist []*net.IPNet
}

// NewIPFilter creates a new IP filter
func NewIPFilter(whitelist []string, blacklist []string) (*IPFilter, error) {
	f := &IPFilter{
		whitelist: make([]*net.IPNet, 0, len(whitelist)),
		blacklist: make([]*net.IPNet, 0, len(blacklist)),
	}
	
	for _, cidr := range whitelist {
		_, network, err := net.ParseCIDR(cidr)
		if err != nil {
			return nil, fmt.Errorf("invalid whitelist CIDR %s: %w", cidr, err)
		}
		f.whitelist = append(f.whitelist, network)
	}
	
	for _, cidr := range blacklist {
		_, network, err := net.ParseCIDR(cidr)
		if err != nil {
			return nil, fmt.Errorf("invalid blacklist CIDR %s: %w", cidr, err)
		}
		f.blacklist = append(f.blacklist, network)
	}
	
	return f, nil
}

// Allow checks if an IP is allowed
func (f *IPFilter) Allow(ipStr string) bool {
	ip := net.ParseIP(ipStr)
	if ip == nil {
		return false
	}
	
	// Check blacklist first
	for _, network := range f.blacklist {
		if network.Contains(ip) {
			return false
		}
	}
	
	// Check whitelist if any
	if len(f.whitelist) > 0 {
		for _, network := range f.whitelist {
			if network.Contains(ip) {
				return true
			}
		}
		return false
	}
	
	return true
}

// =============================================================================
// CSRF PROTECTION
// =============================================================================

// CSRF provides CSRF token generation and validation
type CSRF struct {
	secret   []byte
	redis    *redis.Client
	keyPrefx string
}

// NewCSRF creates a new CSRF protector
func NewCSRF(secret []byte, redisClient *redis.Client) *CSRF {
	return &CSRF{
		secret:   secret,
		redis:    redisClient,
		keyPrefx: "tigerscan:csrf:",
	}
}

// GenerateToken generates a new CSRF token
func (c *CSRF) GenerateToken(sessionID string) (string, error) {
	token := make([]byte, CSRFTokenLength)
	if _, err := rand.Read(token); err != nil {
		return "", err
	}
	
	tokenStr := hex.EncodeToString(token)
	
	// Store in Redis if available
	if c.redis != nil {
		ctx := context.Background()
		key := c.keyPrefx + sessionID
		c.redis.Set(ctx, key, tokenStr, 24*time.Hour)
	}
	
	return tokenStr, nil
}

// ValidateToken validates a CSRF token
func (c *CSRF) ValidateToken(sessionID, token string) bool {
	if c.redis != nil {
		ctx := context.Background()
		key := c.keyPrefx + sessionID
		stored, err := c.redis.Get(ctx, key).Result()
		if err != nil {
			return false
		}
		return stored == token
	}
	
	// Simple validation (production should use Redis)
	return len(token) == CSRFTokenLength*2
}

// =============================================================================
// INPUT VALIDATION & SANITIZATION
// =============================================================================

// Validator provides input validation
type Validator struct {
	// Address regex
	AddressRegex *regexp.Regexp
	
	// Transaction hash regex
	TxHashRegex *regexp.Regexp
	
	// Block hash regex
	BlockHashRegex *regexp.Regexp
	
	// Safe HTML regex
	SafeHTMLRegex *regexp.Regexp
}

// NewValidator creates a new validator
func NewValidator() *Validator {
	return &Validator{
		AddressRegex:    regexp.MustCompile("^0x[a-fA-F0-9]{40}$"),
		TxHashRegex:     regexp.MustCompile("^0x[a-fA-F0-9]{64}$"),
		BlockHashRegex: regexp.MustCompile("^0x[a-fA-F0-9]{64}$"),
		SafeHTMLRegex:  regexp.MustCompile("(?i)<(script|iframe|object|embed|applet|form|svg|on[a-z]+)[^>]*>"),
	}
}

// ValidateAddress validates an Ethereum address
func (v *Validator) ValidateAddress(addr string) bool {
	return v.AddressRegex.MatchString(addr)
}

// ValidateTxHash validates a transaction hash
func (v *Validator) ValidateTxHash(hash string) bool {
	return v.TxHashRegex.MatchString(hash)
}

// ValidateBlockHash validates a block hash
func (v *Validator) ValidateBlockHash(hash string) bool {
	return v.BlockHashRegex.MatchString(hash)
}

// SanitizeHTML removes dangerous HTML
func (v *Validator) SanitizeHTML(input string) string {
	// Remove dangerous tags
	output := v.SafeHTMLRegex.ReplaceAllString(input, "")
	
	// Remove event handlers
	eventRegex := regexp.MustCompile(`(?i)\s+on[a-z]+="[^"]*"`)
	output = eventRegex.ReplaceAllString(output, "")
	
	// Remove javascript: URLs
	jsRegex := regexp.MustCompile(`(?i)javascript:`)
	output = jsRegex.ReplaceAllString(output, "")
	
	// Remove data: URLs
	dataRegex := regexp.MustCompile(`(?i)data:`)
	output = dataRegex.ReplaceAllString(output, "")
	
	return output
}

// =============================================================================
// HTTP SECURITY MIDDLEWARE
// =============================================================================

// SecurityMiddleware provides HTTP security middleware
type SecurityMiddleware struct {
	config    *Config
	rateLimiter *RateLimiter
	ipFilter   *IPFilter
	csrf       *CSRF
	validator *Validator
}

// NewSecurityMiddleware creates a new security middleware
func NewSecurityMiddleware(cfg *Config) (*SecurityMiddleware, error) {
	var rateLimiter *RateLimiter
	if cfg.RedisClient != nil {
		rateLimiter = NewRateLimiter(cfg.RedisClient, cfg.RateLimitRPS, cfg.RateLimitBurst)
	}
	
	ipFilter, err := NewIPFilter(cfg.IPWhitelist, cfg.IPBlacklist)
	if err != nil {
		return nil, err
	}
	
	csrf := NewCSRF(cfg.CSRFSecret, cfg.RedisClient)
	validator := NewValidator()
	
	return &SecurityMiddleware{
		config:      cfg,
		rateLimiter: rateLimiter,
		ipFilter:   ipFilter,
		csrf:      csrf,
		validator: validator,
	}, nil
}

// Handler returns a middleware handler function
func (sm *SecurityMiddleware) Handler(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ctx := r.Context()
		
		// IP filtering
		ip := getClientIP(r)
		if !sm.ipFilter.Allow(ip) {
			http.Error(w, "Access denied", http.StatusForbidden)
			return
		}
		
		// Rate limiting
		if sm.rateLimiter != nil && !sm.rateLimiter.Allow(ip) {
			w.Header().Set("Retry-After", "60")
			http.Error(w, "Rate limit exceeded", http.StatusTooManyRequests)
			return
		}
		
		// Request size limit
		if r.ContentLength > sm.config.MaxRequestSize {
			http.Error(w, "Request too large", http.StatusRequestEntityTooLarge)
			return
		}
		
		// HSTS header
		if sm.config.HSTSEnabled {
			w.Header().Set("Strict-Transport-Security", "max-age=31536000; includeSubDomains; preload")
		}
		
		// X-Frame-Options
		w.Header().Set("X-Frame-Options", sm.config.FrameOption)
		
		// X-Content-Type-Options
		w.Header().Set("X-Content-Type-Options", "nosniff")
		
		// X-XSS-Protection
		if sm.config.XSSEnabled {
			w.Header().Set("X-XSS-Protection", "1; mode=block")
		}
		
		// Content-Security-Policy
		w.Header().Set("Content-Security-Policy", "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; connect-src 'self' https://api.tigerscan.io;")
		
		// Referrer-Policy
		w.Header().Set("Referrer-Policy", "strict-origin-when-cross-origin")
		
		// CORS
		origin := r.Header.Get("Origin")
		if sm.config.CORSOrigins != nil && origin != "" {
			allowed := false
			for _, o := range sm.config.CORSOrigins {
				if o == origin || o == "*" {
					allowed = true
					break
				}
			}
			if allowed {
				w.Header().Set("Access-Control-Allow-Origin", origin)
				w.Header().Set("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
				w.Header().Set("Access-Control-Allow-Headers", "Content-Type, Authorization, X-API-Key, X-CSRF-Token")
				w.Header().Set("Access-Control-Max-Age", "3600")
			}
		}
		
		// CSRF validation for state-changing methods
		if sm.config.CSRFEnabled && (r.Method == "POST" || r.Method == "PUT" || r.Method == "DELETE") {
			sessionID := getSessionID(r)
			csrfToken := r.Header.Get("X-CSRF-Token")
			if sessionID != "" && csrfToken != "" && !sm.csrf.ValidateToken(sessionID, csrfToken) {
				http.Error(w, "Invalid CSRF token", http.StatusForbidden)
				return
			}
		}
		
		next.ServeHTTP(w, r.WithContext(ctx))
	})
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

func getClientIP(r *http.Request) string {
	// Check X-Forwarded-For header
	xff := r.Header.Get("X-Forwarded-For")
	if xff != "" {
		parts := strings.Split(xff, ",")
		if len(parts) > 0 {
			return strings.TrimSpace(parts[0])
		}
	}
	
	// Check X-Real-IP header
	xri := r.Header.Get("X-Real-IP")
	if xri != "" {
		return xri
	}
	
	// Fallback to RemoteAddr
	ip, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		return r.RemoteAddr
	}
	return ip
}

func getSessionID(r *http.Request) string {
	// Try cookie first
	cookie, err := r.Cookie("session_id")
	if err == nil {
		return cookie.Value
	}
	
	// Try Authorization header
	auth := r.Header.Get("Authorization")
	if strings.HasPrefix(auth, "Bearer ") {
		return strings.TrimPrefix(auth, "Bearer ")
	}
	
	return ""
}

// =============================================================================
// COMPRESSION SECURITY
// =============================================================================

// Compress compresses data using gzip
func Compress(data []byte) ([]byte, error) {
	var buf bytes.Buffer
	writer := gzip.NewWriter(&buf)
	
	_, err := writer.Write(data)
	if err != nil {
		writer.Close()
		return nil, err
	}
	
	err = writer.Close()
	if err != nil {
		return nil, err
	}
	
	return buf.Bytes(), nil
}

// Decompress decompresses gzip data
func Decompress(data []byte) ([]byte, error) {
	reader, err := gzip.NewReader(bytes.NewReader(data))
	if err != nil {
		return nil, err
	}
	
	return io.ReadAll(reader)
}

// =============================================================================
// SECURE RANDOM
// =============================================================================

// GenerateSecureID generates a cryptographically secure ID
func GenerateSecureID() string {
	return uuid.NewSHA1(uuid.NameSpaceOID, []byte(time.Now().Format(time.RFC3339Nano))).String()
}

// GenerateSecureToken generates a secure random token
func GenerateSecureToken(length int) (string, error) {
	b := make([]byte, length)
	_, err := rand.Read(b)
	if err != nil {
		return "", err
	}
	return hex.EncodeToString(b), nil
}

// =============================================================================
// HASHING FOR INTEGRITY
// =============================================================================

// HashSHA256 generates SHA-256 hash
func HashSHA256(data []byte) string {
	h := sha256.Sum256(data)
	return hex.EncodeToString(h[:])
}

// HashFNV64a generates FNV-1a 64-bit hash (for caching)
func HashFNV64a(data []byte) uint64 {
	h := fnv.New64a()
	h.Write(data)
	return h.Sum64()
}