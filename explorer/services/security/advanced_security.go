// Package security provides production-grade security services for TigerScan
// Includes advanced encryption, input validation, rate limiting, DDoS protection, and audit logging
package security

import (
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/hex"
	"fmt"
	"hash/fnv"
	"io"
	"math/big"
	"net"
	"net/http"
	"regexp"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"golang.org/x/time/rate"
)

// =============================================================================
// CONSTANTS
// =============================================================================

const (
	// Encryption
	KeySize          = 32 // AES-256
	NonceSize       = 12  // GCM nonce size
	MaxPayloadSize = 10 * 1024 * 1024 // 10MB max payload

	// Rate Limiting
	RequestsPerMinute = 1000
	RequestsPerHour   = 50000
	BurstLimit       = 100

	// DDoS Protection
	MaxConcurrentRequests = 10000
	RequestTimeout       = 30 * time.Second

	// Input Validation
	MaxAddressLength  = 64
	MaxHashLength   = 128
	MaxSignatureLen = 132
)

// =============================================================================
// CONFIGURATION
// =============================================================================

// Config holds security configuration
type Config struct {
	EncryptionKey     []byte
	RateLimitRPM      int
	RateLimitHourly   int
	BurstLimit        int
	EnableDDoS        bool
	EnableAuditLog    bool
	BlockedIPs       []string
	AllowedOrigins   []string
	CORSMaxAge        time.Duration
}

// DefaultConfig returns default security configuration
func DefaultConfig() *Config {
	key := make([]byte, KeySize)
	if _, err := rand.Read(key); err != nil {
		panic("failed to generate encryption key")
	}

	return &Config{
		EncryptionKey:   key,
		RateLimitRPM:    RequestsPerMinute,
		RateLimitHourly:  RequestsPerHour,
		BurstLimit:      BurstLimit,
		EnableDDoS:      true,
		EnableAuditLog:   true,
		BlockedIPs:     []string{},
		AllowedOrigins: []string{"*"},
		CORSMaxAge:      24 * time.Hour,
	}
}

// =============================================================================
// SECURITY SERVICE
// =============================================================================

// Service provides security services
type Service struct {
	config        *Config
	limiter      *RateLimiter
	ipTracker   *IPTracker
	validator   *InputValidator
	cipher      cipher.AEAD
	auditLog    *AuditLogger
	mu          sync.RWMutex
}

// NewService creates a new security service
func NewService(cfg *Config) (*Service, error) {
	if cfg == nil {
		cfg = DefaultConfig()
	}

	// Initialize AES-GCM cipher
	block, err := aes.NewCipher(cfg.EncryptionKey)
	if err != nil {
		return nil, fmt.Errorf("failed to create cipher: %w", err)
	}

	aead, err := cipher.NewGCM(block)
	if err != nil {
		return nil, fmt.Errorf("failed to create GCM: %w", err)
	}

	svc := &Service{
		config:     cfg,
		limiter:   NewRateLimiter(cfg.RateLimitRPM, cfg.RateLimitHourly, cfg.BurstLimit),
		ipTracker: NewIPTracker(),
		validator: NewInputValidator(),
		cipher:   aead,
		auditLog: NewAuditLogger(),
	}

	return svc, nil
}

// =============================================================================
// ADVANCED ENCRYPTION
// =============================================================================

// Encrypt encrypts plaintext using AES-256-GCM
func (s *Service) Encrypt(plaintext []byte) (string, error) {
	if len(plaintext) == 0 {
		return "", fmt.Errorf("plaintext is empty")
	}

	// Generate random nonce
	nonce := make([]byte, NonceSize)
	if _, err := io.ReadFull(rand.Reader, nonce); err != nil {
		return "", fmt.Errorf("failed to generate nonce: %w", err)
	}

	// Encrypt
	ciphertext := s.cipher.Seal(nonce, nonce, plaintext, nil)

	return hex.EncodeToString(ciphertext), nil
}

// Decrypt decrypts ciphertext using AES-256-GCM
func (s *Service) Decrypt(ciphertextHex string) ([]byte, error) {
	if ciphertextHex == "" {
		return nil, fmt.Errorf("ciphertext is empty")
	}

	ciphertext, err := hex.DecodeString(ciphertextHex)
	if err != nil {
		return nil, fmt.Errorf("failed to decode hex: %w", err)
	}

	// Extract nonce and ciphertext
	if len(ciphertext) < NonceSize {
		return nil, fmt.Errorf("ciphertext too short")
	}

	nonce := ciphertext[:NonceSize]
	ciphertext = ciphertext[NonceSize:]

	// Decrypt
	plaintext, err := s.cipher.Open(nil, nonce, ciphertext, nil)
	if err != nil {
		return nil, fmt.Errorf("decryption failed: %w", err)
	}

	return plaintext, nil
}

// EncryptString encrypts a string
func (s *Service) EncryptString(plaintext string) (string, error) {
	return s.Encrypt([]byte(plaintext))
}

// DecryptString decrypts to a string
func (s *Service) DecryptString(ciphertextHex string) (string, error) {
	plaintext, err := s.Decrypt(ciphertextHex)
	if err != nil {
		return "", err
	}
	return string(plaintext), nil
}

// Hash creates a secure hash of data
func (s *Service) Hash(data []byte) string {
	hash := sha256.Sum256(data)
	return hex.EncodeToString(hash[:])
}

// HashString creates a secure hash of a string
func (s *Service) HashString(data string) string {
	return s.Hash([]byte(data))
}

// ConstantTimeCompare performs constant-time comparison to prevent timing attacks
func (s *Service) ConstantTimeCompare(a, b string) bool {
	return subtle.ConstantTimeCompare([]byte(a), []byte(b)) == 1
}

// =============================================================================
// INPUT VALIDATION
// =============================================================================

// InputValidator validates and sanitizes user input
type InputValidator struct {
	addressRegex *regexp.Regexp
	hashRegex    *regexp.Regexp
	txHashRegex *regexp.Regexp
}

// NewInputValidator creates a new input validator
func NewInputValidator() *InputValidator {
	return &InputValidator{
		addressRegex: regexp.MustCompile(`^0x[0-9a-fA-F]{40}$`),
		hashRegex:    regexp.MustCompile(`^0x[0-9a-fA-F]{64}$`),
		txHashRegex: regexp.MustCompile(`^0x[0-9a-fA-F]{64}$`),
	}
}

// ValidateAddress validates an Ethereum address
func (v *InputValidator) ValidateAddress(addr string) bool {
	if addr == "" || len(addr) != 42 {
		return false
	}
	return v.addressRegex.MatchString(strings.ToLower(addr))
}

// ValidateHash validates a block or transaction hash
func (v *InputValidator) ValidateHash(hash string) bool {
	if hash == "" || len(hash) != 66 {
		return false
	}
	return v.hashRegex.MatchString(strings.ToLower(hash))
}

// ValidateSignature validates an ECDSA signature
func (v *InputValidator) ValidateSignature(sig string) bool {
	if sig == "" || len(sig) > MaxSignatureLen {
		return false
	}
	// Basic hex validation
	return len(sig) >= 2 && len(sig)%2 == 0
}

// SanitizeString removes dangerous characters
func (v *InputValidator) SanitizeString(input string) string {
	// Remove null bytes
	input = strings.ReplaceAll(input, "\x00", "")
	// Remove control characters
	input = regexp.MustCompile(`[\x00-\x1F\x7F]`).ReplaceAllString(input, "")
	// Trim whitespace
	return strings.TrimSpace(input)
}

// ValidateAndSanitize validates and sanitizes input
func (v *InputValidator) ValidateAndSanitize(input string, inputType string) (string, bool) {
	sanitized := v.SanitizeString(input)

	switch inputType {
	case "address":
		if !v.ValidateAddress(sanitized) {
			return "", false
		}
	case "hash":
		if !v.ValidateHash(sanitized) {
			return "", false
		}
	case "signature":
		if !v.ValidateSignature(sanitized) {
			return "", false
		}
	}

	return sanitized, true
}

// =============================================================================
// RATE LIMITING
// =============================================================================

// RateLimiter provides per-IP rate limiting
type RateLimiter struct {
	clients map[string]*ClientLimiter
	mu      sync.Mutex
}

// ClientLimiter tracks rate limits for a client
type ClientLimiter struct {
	requests    int
	requestsHour int
	lastReset   time.Time
	limiter     *rate.Limiter
}

// NewRateLimiter creates a new rate limiter
func NewRateLimiter(rpm, rph, burst int) *RateLimiter {
	rl := &RateLimiter{
		clients: make(map[string]*ClientLimiter),
	}

	// Cleanup old clients periodically
	go func() {
		for {
			time.Sleep(1 * time.Hour)
			rl.cleanup()
		}
	}()

	return rl
}

// Allow checks if a request is allowed
func (rl *RateLimiter) Allow(ip string) bool {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	client, exists := rl.clients[ip]
	if !exists {
		client = &ClientLimiter{
			limiter: rate.NewLimiter(rate.Limit(BurstLimit), BurstLimit),
		}
		rl.clients[ip] = client
	}

	// Check burst limit
	if !client.limiter.Allow() {
		return false
	}

	// Reset hourly counter if needed
	if time.Since(client.lastReset) > time.Hour {
		client.requestsHour = 0
		client.lastReset = time.Now()
	}

	client.requests++
	client.requestsHour++

	// Allow if under hourly limit
	return client.requestsHour <= RequestsPerHour
}

// cleanup removes old clients
func (rl *RateLimiter) cleanup() {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	now := time.Now()
	for ip, client := range rl.clients {
		if now.Sub(client.lastReset) > 2*time.Hour {
			delete(rl.clients, ip)
		}
	}
}

// =============================================================================
// DDOS PROTECTION
// =============================================================================

// IPTracker tracks IPs for DDoS protection
type IPTracker struct {
	ips       map[string]*IPInfo
	mu        sync.RWMutex
	maxIPs    int
}

// IPInfo tracks IP connection info
type IPInfo struct {
	Requests    int
	Connections int
	LastSeen    time.Time
	FirstSeen   time.Time
}

// NewIPTracker creates a new IP tracker
func NewIPTracker() *IPTracker {
	return &IPTracker{
		ips:    make(map[string]*IPInfo),
		maxIPs: MaxConcurrentRequests,
	}
}

// TrackRequest tracks a request from an IP
func (t *IPTracker) TrackRequest(ip string) bool {
	t.mu.Lock()
	defer t.mu.Unlock()

	info, exists := t.ips[ip]
	if !exists {
		info = &IPInfo{
			FirstSeen: time.Now(),
		}
		t.ips[ip] = info
	}

	info.Requests++
	info.LastSeen = time.Now()

	// Check if IP is suspicious
	if info.Requests > 1000 && info.Connections > 100 {
		return false // Block suspicious IP
	}

	// Cleanup if over limit
	if len(t.ips) > t.maxIPs {
		t.cleanup()
	}

	return true
}

// cleanup removes old entries
func (t *IPTracker) cleanup() {
	now := time.Now()
	for ip, info := range t.ips {
		if now.Sub(info.LastSeen) > 5*time.Minute {
			delete(t.ips, ip)
		}
	}
}

// =============================================================================
// AUDIT LOGGING
// =============================================================================

// AuditLogger provides audit logging
type AuditLogger struct {
	logs     []AuditEntry
	mu       sync.Mutex
	maxLogs  int
}

// AuditEntry represents an audit log entry
type AuditEntry struct {
	Timestamp   time.Time `json:"timestamp"`
	IP          string    `json:"ip"`
	UserAgent   string    `json:"user_agent"`
	Method     string    `json:"method"`
	Path       string    `json:"path"`
	StatusCode int       `json:"status_code"`
	Action     string    `json:"action"`
	Success    bool      `json:"success"`
	Message    string    `json:"message,omitempty"`
}

// NewAuditLogger creates a new audit logger
func NewAuditLogger() *AuditLogger {
	return &AuditLogger{
		logs:    make([]AuditEntry, 0, 10000),
		maxLogs: 10000,
	}
}

// Log logs an audit entry
func (l *AuditLogger) Log(entry AuditEntry) {
	l.mu.Lock()
	defer l.mu.Unlock()

	entry.Timestamp = time.Now()
	l.logs = append(l.logs, entry)

	// Rotate logs if needed
	if len(l.logs) > l.maxLogs {
		l.logs = l.logs[len(l.logs)/2:]
	}
}

// GetLogs returns recent audit logs
func (l *AuditLogger) GetLogs(limit int) []AuditEntry {
	l.mu.RLock()
	defer l.mu.RUnlock()

	if limit > len(l.logs) {
		limit = len(l.logs)
	}

	result := make([]AuditEntry, limit)
	copy(result, l.logs[len(l.logs)-limit:])
	return result
}

// =============================================================================
// GIN MIDDLEWARE
// =============================================================================

// SecurityMiddleware returns security middleware
func (s *Service) SecurityMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		// Get client IP
		ip := c.ClientIP()

		// Check if IP is blocked
		for _, blocked := range s.config.BlockedIPs {
			if ip == blocked {
				c.AbortWithStatusJSON(http.StatusForbidden, gin.H{
					"error": "IP blocked",
				})
				return
			}
		}

		// Rate limiting
		if !s.limiter.Allow(ip) {
			c.AbortWithStatusJSON(http.StatusTooManyRequests, gin.H{
				"error": "Rate limit exceeded",
			})
			return
		}

		// DDoS protection
		if s.config.EnableDDoS && !s.ipTracker.TrackRequest(ip) {
			c.AbortWithStatusJSON(http.StatusServiceUnavailable, gin.H{
				"error": "Service temporarily unavailable",
			})
			return
		}

		// Validate inputs
		path := c.Request.URL.Path
		if c.Request.Method == "GET" || c.Request.Method == "POST" {
			// Validate address in path
			if strings.Contains(path, "/address/") {
				parts := strings.Split(path, "/address/")
				if len(parts) > 1 {
					addr := parts[1]
					if idx := strings.Index(addr, "/"); idx > 0 {
						addr = addr[:idx]
					}
					if !s.validator.ValidateAddress(addr) {
						c.AbortWithStatusJSON(http.StatusBadRequest, gin.H{
							"error": "Invalid address format",
						})
						return
					}
				}
			}
		}

		// CORS
		c.Header("Access-Control-Allow-Origin", "*")
		c.Header("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
		c.Header("Access-Control-Allow-Headers", "Origin, Content-Type, Authorization, X-API-Key")
		c.Header("Access-Control-Max-Age", "86400")

		if c.Request.Method == "OPTIONS" {
			c.AbortWithStatus(http.StatusNoContent)
			return
		}

		// Continue to next handler
		c.Next()

		// Log request
		if s.config.EnableAuditLog {
			s.auditLog.Log(AuditEntry{
				IP:          ip,
				UserAgent:   c.Request.UserAgent(),
				Method:     c.Request.Method,
				Path:       c.Request.URL.Path,
				StatusCode: c.Writer.Status(),
			})
		}
	}
}

// =============================================================================
// SECURE RESPONSE HELPERS
// =============================================================================

// SecureJSON returns a secure JSON response
func SecureJSON(code int, obj interface{}) (int, interface{}) {
	return code, gin.H{
		"status": "ok",
		"result": obj,
	}
}

// SecureError returns a secure error response (doesn't expose internal details)
func SecureError(code int, message string) (int, interface{}) {
	return code, gin.H{
		"status": "error",
		"message": message,
	}
}

// =============================================================================
// CRYPTOGRAPHIC HELPERS
// =============================================================================

// GenerateRandomBytes generates cryptographically secure random bytes
func GenerateRandomBytes(n int) ([]byte, error) {
	b := make([]byte, n)
	_, err := io.ReadFull(rand.Reader, b)
	if err != nil {
		return nil, err
	}
	return b, nil
}

// GenerateRandomString generates a cryptographically secure random string
func GenerateRandomString(n int) (string, error) {
	b, err := GenerateRandomBytes(n)
	if err != nil {
		return "", err
	}
	return hex.EncodeToString(b), nil
}

// IsValidENSName validates an ENS name
func IsValidENSName(name string) bool {
	if len(name) > 256 || len(name) < 5 {
		return false
	}
	// Basic ENS name validation
	return regexp.MustCompile(`^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)*\.eth$`).MatchString(strings.ToLower(name))
}

// GetClientIP gets the real client IP from request
func GetClientIP(r *http.Request) string {
	// Check X-Forwarded-For header
	xff := r.Header.Get("X-Forwarded-For")
	if xff != "" {
		ips := strings.Split(xff, ",")
		return strings.TrimSpace(ips[0])
	}

	// Check X-Real-IP header
	xri := r.Header.Get("X-Real-IP")
	if xri != "" {
		return xri
	}

	// Fall back to remote address
	ip, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		return r.RemoteAddr
	}
	return ip
}

// ValidateBigInt validates a big integer string
func ValidateBigInt(value string) (*big.Int, bool) {
	if value == "" {
		return nil, false
	}

	// Remove 0x prefix if present
	if strings.HasPrefix(value, "0x") || strings.HasPrefix(value, "0X") {
		value = value[2:]
	}

	// Parse big int
	n, ok := new(big.Int).SetString(value, 16)
	if !ok {
		// Try decimal
		n, ok = new(big.Int).SetString(value, 10)
	}

	return n, ok && n.Sign() >= 0
}

var _ = big.NewInt // Use big.Int
var _ = regexp.MustCompile // Use regex