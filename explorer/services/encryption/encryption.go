// Package encryption provides cryptographic security services for the explorer
// Implements AES-256-GCM encryption, rate limiting, and security protections
package encryption

import (
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"math/big"
	"strings"
	"sync"
	"time"
)

// SecurityService provides comprehensive security services
type SecurityService struct {
	encryptionKey []byte
	rateLimiters map[string]*RateLimiter
	mu          sync.RWMutex
	blockedIPs  map[string]time.Time
}

// RateLimiter provides rate limiting
type RateLimiter struct {
	requests    int
	windowStart time.Time
	maxRequests int
	windowDuration time.Duration
	mu          sync.Mutex
}

// NewSecurityService creates a new security service
func NewSecurityService() (*SecurityService, error) {
	key := make([]byte, 32)
	if _, err := rand.Read(key); err != nil {
		return nil, fmt.Errorf("failed to generate key: %w", err)
	}
	
	return &SecurityService{
		encryptionKey: key,
		rateLimiters:  make(map[string]*RateLimiter),
		blockedIPs:  make(map[string]time.Time),
	}, nil
}

// NewSecurityServiceWithKey creates service with existing key
func NewSecurityServiceWithKey(keyHex string) (*SecurityService, error) {
	key, err := hex.DecodeString(strings.TrimPrefix(keyHex, "0x"))
	if err != nil {
		return nil, fmt.Errorf("invalid key: %w", err)
	}
	
	if len(key) != 32 {
		return nil, errors.New("key must be 32 bytes (256 bits)")
	}
	
	return &SecurityService{
		encryptionKey: key,
		rateLimiters:  make(map[string]*RateLimiter),
		blockedIPs:  make(map[string]time.Time),
	}, nil
}

// Encrypt encrypts data using AES-256-GCM
func (s *SecurityService) Encrypt(plaintext []byte) (string, error) {
	block, err := aes.NewCipher(s.encryptionKey)
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
	return hex.EncodeToString(ciphertext), nil
}

// Decrypt decrypts data using AES-256-GCM
func (s *SecurityService) Decrypt(ciphertextHex string) ([]byte, error) {
	ciphertext, err := hex.DecodeString(ciphertextHex)
	if err != nil {
		return nil, err
	}
	
	block, err := aes.NewCipher(s.encryptionKey)
	if err != nil {
		return nil, err
	}
	
	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return nil, err
	}
	
	nonceSize := gcm.NonceSize()
	if len(ciphertext) < nonceSize {
		return nil, errors.New("ciphertext too short")
	}
	
	nonce, ciphertext := ciphertext[:nonceSize], ciphertext[nonceSize:]
	
	plaintext, err := gcm.Open(nil, nonce, ciphertext, nil)
	if err != nil {
		return nil, err
	}
	
	return plaintext, nil
}

// EncryptString encrypts a string
func (s *SecurityService) EncryptString(plaintext string) (string, error) {
	result, err := s.Encrypt([]byte(plaintext))
	return result, err
}

// DecryptString decrypts to a string
func (s *SecurityService) DecryptString(ciphertext string) (string, error) {
	result, err := s.Decrypt(ciphertext)
	if err != nil {
		return "", err
	}
	return string(result), nil
}

// Hash creates a secure hash of data
func (s *SecurityService) Hash(data []byte) string {
	hash := sha256.Sum256(data)
	return hex.EncodeToString(hash[:])
}

// HashString hashes a string
func (s *SecurityService) HashString(plaintext string) string {
	return s.Hash([]byte(plaintext))
}

// ConstantTimeCompare provides constant-time comparison
func ConstantTimeCompare(a, b string) bool {
	return subtleConstantTimeCompare([]byte(a), []byte(b))
}

func subtleConstantTimeCompare(a, b []byte) bool {
	if len(a) != len(b) {
		return false
	}
	
	var result byte
	for i := range a {
		result |= a[i] ^ b[i]
	}
	
	return result == 0
}

// RateLimitCheck checks rate limits for an identifier
func (s *SecurityService) RateLimitCheck(id string, maxRequests int, windowDuration time.Duration) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	// Check if IP is blocked
	if blockedUntil, ok := s.blockedIPs[id]; ok {
		if time.Now().Before(blockedUntil) {
			return false
		}
		delete(s.blockedIPs, id)
	}
	
	limiter, ok := s.rateLimiters[id]
	if !ok {
		limiter = &RateLimiter{
			maxRequests:  maxRequests,
			windowDuration: windowDuration,
		}
		s.rateLimiters[id] = limiter
	}
	
	limiter.mu.Lock()
	defer limiter.mu.Unlock()
	
	now := time.Now()
	if now.Sub(limiter.windowStart) > limiter.windowDuration {
		limiter.requests = 0
		limiter.windowStart = now
	}
	
	limiter.requests++
	
	if limiter.requests > limiter.maxRequests {
		// Block the IP for a duration
		s.blockedIPs[id] = now.Add(windowDuration * 10)
		return false
	}
	
	return true
}

// BlockIP blocks an IP address
func (s *SecurityService) BlockIP(ip string, duration time.Duration) {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	s.blockedIPs[ip] = time.Now().Add(duration)
}

// UnblockIP unblocks an IP address
func (s *SecurityService) UnblockIP(ip string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	delete(s.blockedIPs, ip)
}

// IsBlocked checks if an IP is blocked
func (s *SecurityService) IsBlocked(ip string) bool {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	if blockedUntil, ok := s.blockedIPs[ip]; ok {
		if time.Now().Before(blockedUntil) {
			return true
		}
	}
	return false
}

// SecureRandom generates a cryptographically secure random string
func (s *SecurityService) SecureRandom(length int) (string, error) {
	bytes := make([]byte, length)
	if _, err := rand.Read(bytes); err != nil {
		return "", err
	}
	
	return hex.EncodeToString(bytes)[:length], nil
}

// GenerateKey generates a new encryption key
func (s *SecurityService) GenerateKey() (string, error) {
	key := make([]byte, 32)
	if _, err := rand.Read(key); err != nil {
		return "", err
	}
	
	return hex.EncodeToString(key), nil
}

// ValidateKey validates an encryption key
func (s *SecurityService) ValidateKey(keyHex string) bool {
	key, err := hex.DecodeString(keyHex)
	if err != nil {
		return false
	}
	
	return len(key) == 32
}

// SanitizeInput sanitizes user input
func (s *SecurityService) SanitizeInput(input string) string {
	// Remove null bytes
	input = strings.Replace(input, "\x00", "", -1)
	
	// Trim whitespace
	input = strings.TrimSpace(input)
	
	// Limit length
	if len(input) > 10000 {
		input = input[:10000]
	}
	
	return input
}

// ValidateAddress validates and normalizes an address
func (s *SecurityService) ValidateAddress(address string) (string, bool) {
	address = strings.ToLower(address)
	address = strings.TrimPrefix(address, "0x")
	
	if len(address) != 40 {
		return "", false
	}
	
	// Validate hex
	_, err := hex.DecodeString(address)
	if err != nil {
		return "", false
	}
	
	return "0x" + address, true
}

// ValidateHash validates a transaction/hash
func (s *SecurityService) ValidateHash(hash string) bool {
	hash = strings.TrimPrefix(hash, "0x")
	
	if len(hash) != 64 {
		return false
	}
	
	_, err := hex.DecodeString(hash)
	return err == nil
}

// SecurityConfig represents security configuration
type SecurityConfig struct {
	MaxRequestSize   int           `json:"maxRequestSize"`
	RateLimit       int           `json:"rateLimit"`
	RateLimitWindow  time.Duration `json:"rateLimitWindow"`
	BlockDuration   time.Duration `json:"blockDuration"`
	EnableEncryption bool         `json:"enableEncryption"`
	AllowedOrigins  []string      `json:"allowedOrigins"`
}

// DefaultConfig returns default security configuration
func DefaultConfig() *SecurityConfig {
	return &SecurityConfig{
		MaxRequestSize:   1024 * 1024, // 1MB
		RateLimit:       100,
		RateLimitWindow:  time.Minute,
		BlockDuration:  time.Hour,
		EnableEncryption: true,
		AllowedOrigins: []string{"*"},
	}
}

// BigIntToString safely converts big.Int to string
func BigIntToString(n *big.Int) string {
	if n == nil {
		return "0"
	}
	return n.String()
}

// InitSecurityService initializes the security service
func InitSecurityService() (*SecurityService, error) {
	return NewSecurityService()
}