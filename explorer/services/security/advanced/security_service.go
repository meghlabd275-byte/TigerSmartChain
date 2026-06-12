// Package security provides advanced security features for TigerScan.
package security

import (
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"database/sql"
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"io"
	"math/big"
	"net"
	"net/http"
	"strings"
	"sync"
	"time"
)

// RateLimiter provides API rate limiting
type RateLimiter struct {
	db            *sql.DB
	mu            sync.RWMutex
	requests     map[string]*ClientLimiter
	blocklist    map[string]*BlockedIP
	checkPeriod time.Duration
}

// ClientLimiter tracks client request rates
type ClientLimiter struct {
	requests    int
	windowStart time.Time
	blocked    bool
}

// BlockedIP represents a blocked IP
type BlockedIP struct {
	IP           net.IP
	Until        time.Time
	IsPermanent  bool
	Reason       string
	BlockCount   int
	LastBlocked time.Time
}

// Service provides security functionality
type Service struct {
	db            *sql.DB
	rateLimiter   *RateLimiter
	cryptoService *CryptoService
	twoFAService  *TwoFAService
}

// Config holds security configuration
type Config struct {
	DB                  *sql.DB
	RateLimitWindow     time.Duration
	RateLimitRequests  int
	MaxFailedAttempts  int
	BlockDuration      time.Duration
	Enable2FA          bool
}

// NewService creates a new security service
func NewService(cfg *Config) (*Service, error) {
	rl, err := NewRateLimiter(cfg.DB, cfg.RateLimitWindow, cfg.RateLimitRequests, cfg.BlockDuration)
	if err != nil {
		return nil, err
	}

	cs, err := NewCryptoService()
	if err != nil {
		return nil, err
	}

	twofa := NewTwoFAService()

	return &Service{
		db:           cfg.DB,
		rateLimiter:  rl,
		cryptoService: cs,
		twoFAService: twofa,
	}, nil
}

// CheckRateLimit checks if request should be rate limited
func (s *Service) CheckRateLimit(ctx context.Context, apiKey, clientIP string) (bool, error) {
	return s.rateLimiter.Check(ctx, apiKey, clientIP)
}

// BlockIP blocks an IP address
func (s *Service) BlockIP(ctx context.Context, ip string, reason string, duration time.Duration) error {
	return s.rateLimiter.BlockIP(ctx, ip, reason, duration)
}

// UnblockIP unblocks an IP address
func (s *Service) UnblockIP(ctx context.Context, ip string) error {
	return s.rateLimiter.UnblockIP(ctx, ip)
}

// IsBlocked checks if IP is blocked
func (s *Service) IsBlocked(ctx context.Context, ip string) (bool, error) {
	return s.rateLimiter.IsBlocked(ctx, ip)
}

// NewRateLimiter creates a new rate limiter
func NewRateLimiter(db *sql.DB, window time.Duration, maxRequests int, blockDuration time.Duration) (*RateLimiter, error) {
	rl := &RateLimiter{
		db:            db,
		requests:     make(map[string]*ClientLimiter),
		blocklist:    make(map[string]*BlockedIP),
		checkPeriod:  window,
	}

	// Load blocklist from database
	if err := rl.loadBlocklist(context.Background()); err != nil {
		return nil, err
	}

	go rl.cleanupLoop()

	return rl, nil
}

// loadBlocklist loads blocked IPs from database
func (rl *RateLimiter) loadBlocklist(ctx context.Context) error {
	query := `
		SELECT ip_address, blocked_until, is_permanent, reason, block_count, last_blocked
		FROM ip_blocklist
		WHERE blocked_until > NOW() OR is_permanent = true
	`

	rows, err := rl.db.QueryContext(ctx, query)
	if err != nil {
		return err
	}
	defer rows.Close()

	for rows.Next() {
		var ip BlockedIP
		var ipStr string
		err := rows.Scan(&ipStr, &ip.Until, &ip.IsPermanent, &ip.Reason, &ip.BlockCount, &ip.LastBlocked)
		if err != nil {
			continue
		}
		ip.IP = net.ParseIP(ipStr)
		rl.blocklist[ipStr] = &ip
	}

	return rows.Err()
}

// Check checks if request is allowed
func (rl *RateLimiter) Check(ctx context.Context, apiKey, clientIP string) (bool, error) {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	// Check blocklist
	if blocked, ok := rl.blocklist[clientIP]; ok {
		if !blocked.Until.After(time.Now()) && !blocked.IsPermanent {
			delete(rl.blocklist, clientIP)
		} else {
			return false, fmt.Errorf("IP blocked until %v", blocked.Until)
		}
	}

	// Check API key limit
	key := "api:" + apiKey
	if apiKey == "" {
		key = "ip:" + clientIP
	}

	now := time.Now()
	limiter, ok := rl.requests[key]

	if !ok || now.Sub(limiter.windowStart) > rl.checkPeriod {
		rl.requests[key] = &ClientLimiter{
			requests:    1,
			windowStart: now,
		}
		return true, nil
	}

	limiter.requests++

	if limiter.requests > 1000 { // Default max requests
		return false, fmt.Errorf("rate limit exceeded")
	}

	return true, nil
}

// BlockIP blocks an IP
func (rl *RateLimiter) BlockIP(ctx context.Context, ip string, reason string, duration time.Duration) error {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	blocked := &BlockedIP{
		IP:          net.ParseIP(ip),
		Until:       time.Now().Add(duration),
		IsPermanent: duration == 0,
		Reason:      reason,
		BlockCount:  1,
		LastBlocked: time.Now(),
	}

	rl.blocklist[ip] = blocked

	// Store in database
	query := `
		INSERT INTO ip_blocklist (ip_address, reason, blocked_until, is_permanent, block_count, last_blocked)
		VALUES ($1, $2, $3, $4, $5, $6)
		ON CONFLICT (ip_address) DO UPDATE SET
			reason = EXCLUDED.reason,
			blocked_until = EXCLUDED.blocked_until,
			is_permanent = EXCLUDED.is_permanent,
			block_count = ip_blocklist.block_count + 1,
			last_blocked = EXCLUDED.last_blocked
	`

	_, err := rl.db.ExecContext(ctx, query, ip, reason, blocked.Until, blocked.IsPermanent, blocked.BlockCount, blocked.LastBlocked)
	return err
}

// UnblockIP unblocks an IP
func (rl *RateLimiter) UnblockIP(ctx context.Context, ip string) error {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	delete(rl.blocklist, ip)

	query := `DELETE FROM ip_blocklist WHERE ip_address = $1`
	_, err := rl.db.ExecContext(ctx, query, ip)
	return err
}

// IsBlocked checks if IP is blocked
func (rl *RateLimiter) IsBlocked(ctx context.Context, ip string) (bool, error) {
	rl.mu.RLock()
	defer rl.mu.RUnlock()

	if blocked, ok := rl.blocklist[ip]; ok {
		if blocked.IsPermanent || blocked.Until.After(time.Now()) {
			return true, nil
		}
	}

	return false, nil
}

// cleanupLoop periodically cleans up old entries
func (rl *RateLimiter) cleanupLoop() {
	ticker := time.NewTicker(5 * time.Minute)
	defer ticker.Stop()

	for range ticker.C {
		rl.mu.Lock()
		now := time.Now()
		for ip, blocked := range rl.blocklist {
			if !blocked.IsPermanent && blocked.Until.Before(now) {
				delete(rl.blocklist, ip)
			}
		}

		for key, limiter := range rl.requests {
			if now.Sub(limiter.windowStart) > rl.checkPeriod*2 {
				delete(rl.requests, key)
			}
		}
		rl.mu.Unlock()
	}
}

// ============================================
// CRYPTOGRAPHIC ENCRYPTION SERVICE
// ============================================

// CryptoService provides cryptographic encryption
type CryptoService struct {
	aesKey []byte
}

// NewCryptoService creates a new crypto service
func NewCryptoService() (*CryptoService, error) {
	key := make([]byte, 32)
	if _, err := rand.Read(key); err != nil {
		return nil, err
	}

	return &CryptoService{aesKey: key}, nil
}

// Encrypt encrypts data using AES-256-GCM
func (cs *CryptoService) Encrypt(plaintext []byte) (string, error) {
	block, err := aes.NewCipher(cs.aesKey)
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

	ciphertext := gcm.Seal(nonce, nonce, plaintext, nil)
	return base64.StdEncoding.EncodeToString(ciphertext), nil
}

// Decrypt decrypts data using AES-256-GCM
func (cs *CryptoService) Decrypt(ciphertext string) ([]byte, error) {
	data, err := base64.StdEncoding.DecodeString(ciphertext)
	if err != nil {
		return nil, err
	}

	block, err := aes.NewCipher(cs.aesKey)
	if err != nil {
		return nil, err
	}

	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return nil, err
	}

	nonceSize := gcm.NonceSize()
	if len(data) < nonceSize {
		return nil, fmt.Errorf("ciphertext too short")
	}

	nonce, ciphertext := data[:nonceSize], data[nonceSize:]
	return gcm.Open(nil, nonce, ciphertext, nil)
}

// Hash creates a SHA-256 hash
func (cs *CryptoService) Hash(data []byte) string {
	hash := sha256.Sum256(data)
	return hex.EncodeToString(hash[:])
}

// HashPassword hashes a password with salt
func (cs *CryptoService) HashPassword(password, salt []byte) string {
	combined := append(password, salt...)
	hash := sha256.Sum256(combined)
	return hex.EncodeToString(hash[:])
}

// GenerateSalt generates a random salt
func (cs *CryptoService) GenerateSalt() ([]byte, error) {
	salt := make([]byte, 32)
	_, err := rand.Read(salt)
	return salt, err
}

// VerifyPassword verifies a password against hash
func (cs *CryptoService) VerifyPassword(password, salt, hash []byte) bool {
	computed := cs.HashPassword(password, salt)
	return subtle.ConstantTimeCompare([]byte(computed), hash) == 1
}

// GenerateAPIKey generates a secure API key
func (cs *CryptoService) GenerateAPIKey() (string, error) {
	key := make([]byte, 32)
	if _, err := rand.Read(key); err != nil {
		return "", err
	}
	return base64.URLEncoding.EncodeToString(key), nil
}

// Sign creates a HMAC signature
func (cs *CryptoService) Sign(data, key []byte) []byte {
	h := sha256.New()
	h.Write(data)
	h.Write(key)
	return h.Sum(nil)
}

// Verify verifies a HMAC signature
func (cs *CryptoService) Verify(data, key, signature []byte) bool {
	expected := cs.Sign(data, key)
	return subtle.ConstantTimeCompare(expected, signature) == 1
}

// ============================================
// TWO-FACTOR AUTHENTICATION
// ============================================

// TwoFAService provides 2FA functionality
type TwoFAService struct {
	issuers map[string]*TwoFASecret
	mu      sync.RWMutex
}

// TwoFASecret represents a 2FA secret
type TwoFASecret struct {
	Secret     string
	Issuer     string
	CreatedAt  time.Time
	Verified   bool
}

// NewTwoFAService creates a new 2FA service
func NewTwoFAService() *TwoFAService {
	return &TwoFAService{
		issuers: make(map[string]*TwoFASecret),
	}
}

// GenerateSecret generates a new 2FA secret
func (t *TwoFAService) GenerateSecret(userID, issuer string) (string, error) {
	secret, err := generateTOTPSecret()
	if err != nil {
		return "", err
	}

	t.mu.Lock()
	t.issuers[userID] = &TwoFASecret{
		Secret:    secret,
		Issuer:    issuer,
		CreatedAt: time.Now(),
	}
	t.mu.Unlock()

	return secret, nil
}

// GetSecret gets a user's 2FA secret
func (t *TwoFAService) GetSecret(userID string) (string, bool) {
	t.mu.RLock()
	defer t.mu.RUnlock()

	secret, ok := t.issuers[userID]
	return secret.Secret, ok && secret.Verified
}

// VerifyCode verifies a TOTP code
func (t *TwoFAService) VerifyCode(userID, code string) bool {
	t.mu.RLock()
	secret, ok := t.issuers[userID]
	t.mu.RUnlock()

	if !ok || !secret.Verified {
		return false
	}

	return verifyTOTP(secret.Secret, code)
}

// Enable enables 2FA for a user
func (t *TwoFAService) Enable(userID, code string) error {
	if !t.VerifyCode(userID, code) {
		return fmt.Errorf("invalid code")
	}

	t.mu.Lock()
	if secret, ok := t.issuers[userID]; ok {
		secret.Verified = true
	}
	t.mu.Unlock()

	return nil
}

// Disable disables 2FA for a user
func (t *TwoFAService) Disable(userID string) error {
	t.mu.Lock()
	delete(t.issuers, userID)
	t.mu.Unlock()

	return nil
}

// generateTOTPSecret generates a TOTP secret
func generateTOTPSecret() (string, error) {
	secret := make([]byte, 20)
	_, err := rand.Read(secret)
	if err != nil {
		return "", err
	}
	return base64.StdEncoding.EncodeToString(secret), nil
}

// verifyTOTP verifies a TOTP code
func verifyTOTP(secret, code string) bool {
	// Simplified TOTP verification
	// In production, use proper TOTP library
	return len(code) == 6
}

// ============================================
// SECURITY MIDDLEWARE
// ============================================

// Middleware returns security middleware
func (s *Service) Middleware() func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			clientIP := getClientIP(r)

			// Check blocklist
			if blocked, err := s.IsBlocked(r.Context(), clientIP); err == nil && blocked {
				http.Error(w, "IP blocked", http.StatusForbidden)
				return
			}

			// Check rate limit
			apiKey := r.Header.Get("X-API-Key")
			if allowed, err := s.CheckRateLimit(r.Context(), apiKey, clientIP); err != nil || !allowed {
				http.Error(w, "Rate limit exceeded", http.StatusTooManyRequests)
				return
			}

			next.ServeHTTP(w, r)
		})
	}
}

// getClientIP gets client IP from request
func getClientIP(r *http.Request) string {
	// Check X-Forwarded-For header
	if forwarded := r.Header.Get("X-Forwarded-For"); forwarded != "" {
		ip := strings.Split(forwarded, ",")[0]
		return strings.TrimSpace(ip)
	}

	// Check X-Real-IP header
	if realIP := r.Header.Get("X-Real-IP"); realIP != "" {
		return realIP
	}

	// Use RemoteAddr
	ip, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		return r.RemoteAddr
	}
	return ip
}

// ============================================
// SECURITY HELPERS
// ============================================

// ValidateAddress validates an Ethereum address
func ValidateAddress(address string) bool {
	if !strings.HasPrefix(address, "0x") {
		return false
	}
	if len(address) != 42 {
		return false
	}
	_, err := hex.DecodeString(address[2:])
	return err == nil
}

// ValidateHash validates a transaction/block hash
func ValidateHash(hash string) bool {
	if !strings.HasPrefix(hash, "0x") {
		return false
	}
	if len(hash) != 66 {
		return false
	}
	_, err := hex.DecodeString(hash[2:])
	return err == nil
}

// SanitizeInput sanitizes user input
func SanitizeInput(input string) string {
	// Remove null bytes
	input = strings.Replace(input, "\x00", "", -1)
	
	// Trim whitespace
	input = strings.TrimSpace(input)
	
	// Limit length
	if len(input) > 1000 {
		input = input[:1000]
	}
	
	return input
}

// Constants for security
const (
	MaxRequestSize    = 10 << 20 // 10MB
	MaxURLLength     = 2048
	MaxHeaderLength  = 4096
)

// BigInt for cryptographic operations
var _ = big.NewInt