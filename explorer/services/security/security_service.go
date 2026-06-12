// Package security provides security services for TigerScan.
package security

import (
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"fmt"
	"sync"
	"time"

	"golang.org/x/crypto/bcrypt"
)

// =============================================================================
// SECURITY SERVICE
// =============================================================================

// Service provides security operations
type Service struct {
	db           *sql.DB
	mu           sync.RWMutex
	labelsCache  map[string]*AddressLabel
	phishingDB  map[string]*PhishingEntry
	rateLimiters map[string]*RateLimiter
}

// AddressLabel represents an address label
type AddressLabel struct {
	ID          int64     `json:"id"`
	Address     string    `json:"address"`
	Label       string    `json:"label"`
	LabelType   string    `json:"labelType"`
	Category   string    `json:"category"`
	Reporter   string    `json:"reporter,omitempty"`
	Confidence int       `json:"confidence"`
	IsVerified bool      `json:"isVerified"`
	CreatedAt  time.Time `json:"createdAt"`
	UpdatedAt  time.Time `json:"updatedAt"`
}

// PhishingEntry represents a phishing address
type PhishingEntry struct {
	Address         string    `json:"address"`
	ScamType        string    `json:"scamType"`
	ScamDescription string    `json:"scamDescription"`
	Reporter        string    `json:"reporter,omitempty"`
	ReportsCount    int       `json:"reportsCount"`
	FirstReported   time.Time `json:"firstReported"`
	LastReported    time.Time `json:"lastReported"`
	IsActive        bool      `json:"isActive"`
}

// RateLimiter represents rate limiting
type RateLimiter struct {
	mu          sync.RWMutex
	requests    map[string][]time.Time
	maxRequests int
	window      time.Duration
}

// NewRateLimiter creates a new rate limiter
func NewRateLimiter(maxRequests int, window time.Duration) *RateLimiter {
	return &RateLimiter{
		requests:    make(map[string][]time.Time),
		maxRequests: maxRequests,
		window:      window,
	}
}

// Allow checks if request is allowed
func (rl *RateLimiter) Allow(key string) bool {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	now := time.Now()
	cutoff := now.Add(-rl.window)

	var valid []time.Time
	for _, t := range rl.requests[key] {
		if t.After(cutoff) {
			valid = append(valid, t)
		}
	}

	if len(valid) >= rl.maxRequests {
		rl.requests[key] = valid
		return false
	}

	rl.requests[key] = append(valid, now)
	return true
}

// =============================================================================
// LABEL OPERATIONS
// =============================================================================

// GetLabel gets label for an address
func (s *Service) GetLabel(ctx context.Context, address string) (*AddressLabel, error) {
	s.mu.RLock()
	if label, ok := s.labelsCache[address]; ok {
		s.mu.RUnlock()
		return label, nil
	}
	s.mu.RUnlock()

	var label AddressLabel
	err := s.db.QueryRowContext(ctx, `
		SELECT id, address, label, label_type, category, reporter, confidence, 
		       is_verified, created_at, updated_at
		FROM address_labels 
		WHERE address = $1
	`, address).Scan(
		&label.ID, &label.Address, &label.Label, &label.LabelType,
		&label.Category, &label.Reporter, &label.Confidence,
		&label.IsVerified, &label.CreatedAt, &label.UpdatedAt,
	)

	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}

	s.mu.Lock()
	s.labelsCache[address] = &label
	s.mu.Unlock()

	return &label, nil
}

// AddLabel adds a new label
func (s *Service) AddLabel(ctx context.Context, label *AddressLabel) error {
	_, err := s.db.ExecContext(ctx, `
		INSERT INTO address_labels (address, label, label_type, category, reporter, confidence, is_verified, created_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
		ON CONFLICT (address) DO UPDATE SET
			label = EXCLUDED.label,
			label_type = EXCLUDED.label_type,
			category = EXCLUDED.category,
			updated_at = NOW()
	`, label.Address, label.Label, label.LabelType, label.Category, label.Reporter, label.Confidence, label.IsVerified)

	if err == nil {
		s.mu.Lock()
		s.labelsCache[label.Address] = label
		s.mu.Unlock()
	}

	return err
}

// =============================================================================
// PHISHING DETECTION
// =============================================================================

// CheckPhishing checks if an address is a known phishing address
func (s *Service) CheckPhishing(ctx context.Context, address string) (*PhishingEntry, bool) {
	s.mu.RLock()
	if entry, ok := s.phishingDB[address]; ok {
		s.mu.RUnlock()
		return entry, entry.IsActive
	}
	s.mu.RUnlock()

	var entry PhishingEntry
	err := s.db.QueryRowContext(ctx, `
		SELECT address, scam_type, scam_description, reporter, reports_count, 
		       first_reported, last_reported, is_active
		FROM phishing_reports 
		WHERE address = $1 AND is_active = TRUE
	`, address).Scan(
		&entry.Address, &entry.ScamType, &entry.ScamDescription,
		&entry.Reporter, &entry.ReportsCount,
		&entry.FirstReported, &entry.LastReported, &entry.IsActive,
	)

	if err == sql.ErrNoRows {
		return nil, false
	}
	if err != nil {
		return nil, false
	}

	s.mu.Lock()
	s.phishingDB[address] = &entry
	s.mu.Unlock()

	return &entry, entry.IsActive
}

// ReportPhishing reports a phishing address
func (s *Service) ReportPhishing(ctx context.Context, report *PhishingEntry) error {
	_, err := s.db.ExecContext(ctx, `
		INSERT INTO phishing_reports (address, scam_type, scam_description, reporter, reports_count, first_reported, last_reported, is_active)
		VALUES ($1, $2, $3, $4, 1, NOW(), NOW(), TRUE)
		ON CONFLICT (address) DO UPDATE SET
			reports_count = phishing_reports.reports_count + 1,
			last_reported = NOW()
	`, report.Address, report.ScamType, report.ScamDescription, report.Reporter)

	if err == nil {
		s.mu.Lock()
		delete(s.phishingDB, report.Address)
		s.mu.Unlock()
	}

	return err
}

// =============================================================================
// ENCRYPTION
// =============================================================================

// Encrypt encrypts data using AES-256-GCM
func Encrypt(key []byte, plaintext []byte) (string, error) {
	block, err := aes.NewCipher(key)
	if err != nil {
		return "", err
	}

	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return "", err
	}

	nonce := make([]byte, gcm.NonceSize())
	if _, err := rand.Read(nonce); err != nil {
		return "", err
	}

	ciphertext := gcm.Seal(nonce, nonce, plaintext, nil)
	return hex.EncodeToString(ciphertext), nil
}

// Decrypt decrypts data using AES-256-GCM
func Decrypt(key []byte, ciphertext string) ([]byte, error) {
	data, err := hex.DecodeString(ciphertext)
	if err != nil {
		return nil, err
	}

	block, err := aes.NewCipher(key)
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
	plaintext, err := gcm.Open(nil, nonce, ciphertext, nil)
	if err != nil {
		return nil, err
	}

	return plaintext, nil
}

// HashPassword hashes a password using bcrypt
func HashPassword(password string) (string, error) {
	bytes, err := bcrypt.GenerateFromPassword([]byte(password), bcrypt.DefaultCost)
	return string(bytes), err
}

// CheckPassword checks a password against a hash
func CheckPassword(password, hash string) bool {
	err := bcrypt.CompareHashAndPassword([]byte(hash), []byte(password))
	return err == nil
}

// =============================================================================
// HASHING
// =============================================================================

// HashAddress creates a hash of an address for privacy
func HashAddress(address string) string {
	hash := sha256.Sum256([]byte(address))
	return hex.EncodeToString(hash[:])
}

// =============================================================================
// INPUT VALIDATION
// =============================================================================

// ValidateAddress validates an Ethereum address
func ValidateAddress(address string) bool {
	if len(address) != 42 {
		return false
	}
	if address[:2] != "0x" {
		return false
	}
	for _, c := range address[2:] {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')) {
			return false
		}
	}
	return true
}

// ValidateHash validates a transaction/block hash
func ValidateHash(hash string) bool {
	if len(hash) != 66 {
		return false
	}
	if hash[:2] != "0x" {
		return false
	}
	for _, c := range hash[2:] {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')) {
			return false
		}
	}
	return true
}

// SanitizeInput sanitizes user input
func SanitizeInput(input string) string {
	if len(input) > 10000 {
		input = input[:10000]
	}
	return input
}

// =============================================================================
// AUDIT LOGGING
// =============================================================================

// AuditEntry represents an audit log entry
type AuditEntry struct {
	ID          int64     `json:"id"`
	UserID      string    `json:"userId"`
	Action      string    `json:"action"`
	Resource    string    `json:"resource"`
	IPAddress   string    `json:"ipAddress"`
	UserAgent   string    `json:"userAgent"`
	Success     bool      `json:"success"`
	ErrorMsg    string    `json:"errorMsg,omitempty"`
	Metadata    string    `json:"metadata,omitempty"`
	Timestamp   time.Time `json:"timestamp"`
}

// LogAudit logs an audit event
func (s *Service) LogAudit(ctx context.Context, entry *AuditEntry) error {
	_, err := s.db.ExecContext(ctx, `
		INSERT INTO audit_logs (user_id, action, resource, ip_address, user_agent, success, error_message, metadata, created_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
	`, entry.UserID, entry.Action, entry.Resource, entry.IPAddress, entry.UserAgent, entry.Success, entry.ErrorMsg, entry.Metadata)

	return err
}

// GetAuditLogs gets audit logs for a user
func (s *Service) GetAuditLogs(ctx context.Context, userID string, limit int) ([]*AuditEntry, error) {
	rows, err := s.db.QueryContext(ctx, `
		SELECT id, user_id, action, resource, ip_address, user_agent, success, error_message, metadata, created_at
		FROM audit_logs 
		WHERE user_id = $1
		ORDER BY created_at DESC
		LIMIT $2
	`, userID, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var logs []*AuditEntry
	for rows.Next() {
		var entry AuditEntry
		if err := rows.Scan(
			&entry.ID, &entry.UserID, &entry.Action, &entry.Resource,
			&entry.IPAddress, &entry.UserAgent, &entry.Success,
			&entry.ErrorMsg, &entry.Metadata, &entry.Timestamp,
		); err != nil {
			return nil, err
		}
		logs = append(logs, &entry)
	}

	return logs, nil
}

// =============================================================================
// API KEY MANAGEMENT
// =============================================================================

// APIKey represents an API key
type APIKey struct {
	ID          int64      `json:"id"`
	KeyHash     string     `json:"keyHash"`
	KeyPrefix  string     `json:"keyPrefix"`
	UserID     string     `json:"userId"`
	Label      string     `json:"label"`
	RateLimit  int        `json:"rateLimit"`
	RateWindow int        `json:"rateWindow"`
	IsActive   bool       `json:"isActive"`
	ExpiresAt *time.Time `json:"expiresAt,omitempty"`
	LastUsedAt *time.Time `json:"lastUsedAt,omitempty"`
	CreatedAt  time.Time  `json:"createdAt"`
}

// GenerateAPIKey generates a new API key
func (s *Service) GenerateAPIKey(userID, label string, rateLimit int) (string, error) {
	bytes := make([]byte, 32)
	if _, err := rand.Read(bytes); err != nil {
		return "", err
	}
	key := "tsc_" + hex.EncodeToString(bytes)

	hash := sha256.Sum256([]byte(key))
	keyHash := hex.EncodeToString(hash[:])
	prefix := key[:8]

	_, err := s.db.ExecContext(context.Background(), `
		INSERT INTO api_keys (key_hash, key_prefix, user_id, label, rate_limit, rate_limit_window, is_active, created_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, 60, TRUE, NOW(), NOW())
	`, keyHash, prefix, userID, label, rateLimit)

	if err != nil {
		return "", err
	}

	return key, nil
}

// ValidateAPIKey validates an API key
func (s *Service) ValidateAPIKey(ctx context.Context, key string) (*APIKey, bool) {
	hash := sha256.Sum256([]byte(key))
	keyHash := hex.EncodeToString(hash[:])

	var apiKey APIKey
	err := s.db.QueryRowContext(ctx, `
		SELECT id, key_hash, key_prefix, user_id, label, rate_limit, rate_limit_window, 
		       is_active, expires_at, last_used_at, created_at
		FROM api_keys 
		WHERE key_hash = $1 AND is_active = TRUE
	`, keyHash).Scan(
		&apiKey.ID, &apiKey.KeyHash, &apiKey.KeyPrefix, &apiKey.UserID,
		&apiKey.Label, &apiKey.RateLimit, &apiKey.RateWindow,
		&apiKey.IsActive, &apiKey.ExpiresAt, &apiKey.LastUsedAt, &apiKey.CreatedAt,
	)

	if err != nil || !apiKey.IsActive {
		return nil, false
	}

	if apiKey.ExpiresAt != nil && time.Now().After(*apiKey.ExpiresAt) {
		return nil, false
	}

	s.db.ExecContext(ctx, `UPDATE api_keys SET last_used_at = NOW() WHERE id = $1`, apiKey.ID)

	return &apiKey, true
}

// RevokeAPIKey revokes an API key
func (s *Service) RevokeAPIKey(ctx context.Context, keyID int64) error {
	_, err := s.db.ExecContext(ctx, `UPDATE api_keys SET is_active = FALSE WHERE id = $1`, keyID)
	return err
}
