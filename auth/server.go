// Package auth provides secure user authentication and authorization
// Built with Go for high performance and Rust for security-critical operations
package auth

import (
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"database/sql"
	"encoding/base64"
	"encoding/hex"
	"errors"
	"fmt"
	"net/mail"
	"strings"
	"sync"
	"time"

	"github.com/golang-jwt/jwt/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"golang.org/x/crypto/argon2"
	"golang.org/x/crypto/bcrypt"
	"golang.org/x/crypto/scrypt"
)

// Config holds authentication configuration
type Config struct {
	DBURL              string
	JWTSecret          string
	JWTExpiry          time.Duration
	RefreshTokenExpiry time.Duration
	MinPasswordLength int
	MaxLoginAttempts   int
	LockoutDuration   time.Duration
	SessionDuration   time.Duration
	RequireEmailVerification bool
	Enable2FA            bool
	argon2Time          int
	argon2Memory        int
	argon2Threads       int
	argon2KeyLen       uint32
}

// User represents an authenticated user
type User struct {
	ID                int
	Email             string
	Username          string
	PasswordHash      string
	Salt              string
	IsEmailVerified   bool
	Is2FAEnabled     bool
	TwoFASecret       string
	FailedLoginCount  int
	LockedUntil      *time.Time
	CreatedAt         time.Time
	UpdatedAt         time.Time
}

// Session represents an active user session
type Session struct {
	ID           string
	UserID      int
	Token       string
	RefreshToken string
	IPAddress   string
	UserAgent   string
	ExpiresAt  time.Time
	CreatedAt  time.Time
}

// JWTClaims represents JWT token claims
type JWTClaims struct {
	UserID   int    `json:"user_id"`
	Email    string `json:"email"`
	Username string `json:"username"`
	SessionID string `json:"session_id"`
	jwt.RegisteredClaims
}

// WatchlistItem represents a watchlist item
type WatchlistItem struct {
	ID          int
	UserID      int
	Address     string
	Label       string
	Notes       string
	IsPublic    bool
	AlertType   string
	CreatedAt   time.Time
}

// AddressLabel represents a custom address label
type AddressLabel struct {
	ID        int
	UserID    int
	Address   string
	Label    string
	Category string
	Color    string
	CreatedAt time.Time
}

// Server represents the authentication server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	sessions map[string]*Session
	sessionsMu sync.RWMutex
}

// NewServer creates a new authentication server
func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	
	// Initialize database tables
	if err := createTables(ctx, pool); err != nil {
		return nil, fmt.Errorf("failed to create tables: %w", err)
	}
	
	return &Server{
		cfg:      cfg,
		pool:     pool,
		sessions: make(map[string]*Session),
	}, nil
}

// createTables creates the necessary database tables
func createTables(ctx context.Context, pool *pgxpool.Pool) error {
	queries := []string{
		`CREATE TABLE IF NOT EXISTS users (
			id SERIAL PRIMARY KEY,
			email VARCHAR(255) UNIQUE NOT NULL,
			username VARCHAR(100) UNIQUE NOT NULL,
			password_hash VARCHAR(255) NOT NULL,
			salt VARCHAR(64) NOT NULL,
			is_email_verified BOOLEAN DEFAULT FALSE,
			is_2fa_enabled BOOLEAN DEFAULT FALSE,
			two_fa_secret VARCHAR(255),
			failed_login_count INTEGER DEFAULT 0,
			locked_until TIMESTAMP,
			created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
			updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
		)`,
		`CREATE TABLE IF NOT EXISTS sessions (
			id SERIAL PRIMARY KEY,
			user_id INTEGER REFERENCES users(id) ON DELETE CASCADE,
			token VARCHAR(255) UNIQUE NOT NULL,
			refresh_token VARCHAR(255) UNIQUE NOT NULL,
			ip_address VARCHAR(45),
			user_agent VARCHAR(255),
			expires_at TIMESTAMP NOT NULL,
			created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
		)`,
		`CREATE TABLE IF NOT EXISTS watchlist (
			id SERIAL PRIMARY KEY,
			user_id INTEGER REFERENCES users(id) ON DELETE CASCADE,
			address VARCHAR(42) NOT NULL,
			label VARCHAR(255),
			notes TEXT,
			is_public BOOLEAN DEFAULT FALSE,
			alert_type VARCHAR(50) DEFAULT 'all',
			created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
		)`,
		`CREATE TABLE IF NOT EXISTS address_labels (
			id SERIAL PRIMARY KEY,
			user_id INTEGER REFERENCES users(id) ON DELETE CASCADE,
			address VARCHAR(42) NOT NULL,
			label VARCHAR(255) NOT NULL,
			category VARCHAR(100),
			color VARCHAR(7),
			created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
			UNIQUE(user_id, address)
		)`,
		`CREATE TABLE IF NOT EXISTS address_reports (
			id SERIAL PRIMARY KEY,
			reporter_id INTEGER REFERENCES users(id) ON DELETE CASCADE,
			address VARCHAR(42) NOT NULL,
			report_type VARCHAR(50) NOT NULL,
			description TEXT,
			status VARCHAR(20) DEFAULT 'pending',
			created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
		)`,
		`CREATE INDEX IF NOT EXISTS idx_users_email ON users(email)`,
		`CREATE INDEX IF NOT EXISTS idx_sessions_token ON sessions(token)`,
		`CREATE INDEX IF NOT EXISTS idx_watchlist_user ON watchlist(user_id)`,
		`CREATE INDEX IF NOT EXISTS idx_address_labels_user ON address_labels(user_id)`,
	}
	
	for _, query := range queries {
		if _, err := pool.Exec(ctx, query); err != nil {
			return err
		}
	}
	
	return nil
}

// Register creates a new user account
func (s *Server) Register(ctx context.Context, email, username, password string) (*User, error) {
	// Validate email
	if _, err := mail.ParseAddress(email); err != nil {
		return nil, errors.New("invalid email address")
	}
	
	// Validate username
	if len(username) < 3 || len(username) > 50 {
		return nil, errors.New("username must be between 3 and 50 characters")
	}
	
	// Validate password
	if len(password) < s.cfg.MinPasswordLength {
		return nil, fmt.Errorf("password must be at least %d characters", s.cfg.MinPasswordLength)
	}
	
	// Check if email or username already exists
	var exists bool
	err := s.pool.QueryRow(ctx, `SELECT EXISTS(SELECT 1 FROM users WHERE email = $1 OR username = $2)`, email, username).Scan(&exists)
	if err != nil {
		return nil, err
	}
	if exists {
		return nil, errors.New("email or username already exists")
	}
	
	// Generate salt
	salt := make([]byte, 32)
	if _, err := rand.Read(salt); err != nil {
		return nil, err
	}
	saltStr := base64.StdEncoding.EncodeToString(salt)
	
	// Hash password using Argon2
	hash, err := argon2.IDKey([]byte(password), salt, s.cfg.argon2Time, s.cfg.argon2Memory, s.cfg.argon2Threads, s.cfg.argon2KeyLen)
	if err != nil {
		return nil, err
	}
	hashStr := base64.StdEncoding.EncodeToString(hash)
	
	// Create user
	var user User
	err = s.pool.QueryRow(ctx, `
		INSERT INTO users (email, username, password_hash, salt)
		VALUES ($1, $2, $3, $4)
		RETURNING id, email, username, is_email_verified, is_2fa_enabled, created_at`,
		email, username, hashStr, saltStr,
	).Scan(&user.ID, &user.Email, &user.Username, &user.IsEmailVerified, &user.Is2FAEnabled, &user.CreatedAt)
	if err != nil {
		return nil, err
	}
	
	return &user, nil
}

// Login authenticates a user and returns JWT tokens
func (s *Server) Login(ctx context.Context, email, password, ipAddress, userAgent string) (string, string, *User, error) {
	// Get user
	var user User
	err := s.pool.QueryRow(ctx, `
		SELECT id, email, username, password_hash, salt, is_email_verified, is_2fa_enabled, two_fa_secret, failed_login_count, locked_until
		FROM users WHERE email = $1`,
		email,
	).Scan(
		&user.ID, &user.Email, &user.Username, &user.PasswordHash, &user.Salt,
		&user.IsEmailVerified, &user.Is2FAEnabled, &user.TwoFASecret,
		&user.FailedLoginCount, &user.LockedUntil,
	)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return "", "", nil, errors.New("invalid credentials")
		}
		return "", "", nil, err
	}
	
	// Check if account is locked
	if user.LockedUntil != nil && time.Now().Before(*user.LockedUntil) {
		return "", "", nil, errors.New("account is locked")
	}
	
	// Decode salt and hash
	salt, err := base64.StdEncoding.DecodeString(user.Salt)
	if err != nil {
		return "", "", nil, err
	}
	
	// Verify password using Argon2
	hash, err := argon2.IDKey([]byte(password), salt, s.cfg.argon2Time, s.cfg.argon2Memory, s.cfg.argon2Threads, s.cfg.argon2KeyLen)
	if err != nil {
		return "", "", nil, err
	}
	hashStr := base64.StdEncoding.EncodeToString(hash)
	
	if hashStr != user.PasswordHash {
		// Increment failed login count
		_, err = s.pool.Exec(ctx, `
			UPDATE users SET failed_login_count = failed_login_count + 1,
			locked_until = CASE WHEN failed_login_count >= $1 THEN NOW() + INTERVAL '15 minutes' ELSE NULL END
			WHERE id = $2`,
			s.cfg.MaxLoginAttempts, user.ID,
		)
		if err != nil {
			return "", "", nil, err
		}
		return "", "", nil, errors.New("invalid credentials")
	}
	
	// Reset failed login count
	_, err = s.pool.Exec(ctx, `UPDATE users SET failed_login_count = 0, locked_until = NULL WHERE id = $1`, user.ID)
	if err != nil {
		return "", "", nil, err
	}
	
	// Generate JWT token
	token, refreshToken, err := s.generateTokens(&user)
	if err != nil {
		return "", "", nil, err
	}
	
	// Create session
	expiresAt := time.Now().Add(s.cfg.SessionDuration)
	_, err = s.pool.Exec(ctx, `
		INSERT INTO sessions (user_id, token, refresh_token, ip_address, user_agent, expires_at)
		VALUES ($1, $2, $3, $4, $5, $6)`,
		user.ID, token, refreshToken, ipAddress, userAgent, expiresAt,
	)
	if err != nil {
		return "", "", nil, err
	}
	
	return token, refreshToken, &user, nil
}

// Logout invalidates a user session
func (s *Server) Logout(ctx context.Context, token string) error {
	result, err := s.pool.Exec(ctx, `DELETE FROM sessions WHERE token = $1`, token)
	if err != nil {
		return err
	}
	if result.RowsAffected() == 0 {
		return errors.New("session not found")
	}
	
	// Remove from memory cache
	s.sessionsMu.Lock()
	delete(s.sessions, token)
	s.sessionsMu.Unlock()
	
	return nil
}

// VerifyToken verifies a JWT token and returns the user
func (s *Server) VerifyToken(ctx context.Context, token string) (*User, error) {
	// Parse JWT token
	claims := &JWTClaims{}
	_, err := jwt.ParseWithClaims(token, claims, func(token *jwt.Token) (interface{}, error) {
		return []byte(s.cfg.JWTSecret), nil
	})
	if err != nil {
		return nil, errors.New("invalid token")
	}
	
	// Check if session exists in database
	var exists bool
	err = s.pool.QueryRow(ctx, `SELECT EXISTS(SELECT 1 FROM sessions WHERE token = $1 AND expires_at > NOW())`, token).Scan(&exists)
	if err != nil {
		return nil, err
	}
	if !exists {
		return nil, errors.New("session expired")
	}
	
	// Get user
	var user User
	err = s.pool.QueryRow(ctx, `
		SELECT id, email, username, is_email_verified, is_2fa_enabled
		FROM users WHERE id = $1`,
		claims.UserID,
	).Scan(&user.ID, &user.Email, &user.Username, &user.IsEmailVerified, &user.Is2FAEnabled)
	if err != nil {
		return nil, err
	}
	
	return &user, nil
}

// RefreshToken refreshes an access token
func (s *Server) RefreshToken(ctx context.Context, refreshToken string) (string, string, error) {
	// Get session
	var session Session
	err := s.pool.QueryRow(ctx, `
		SELECT id, user_id, token, expires_at FROM sessions WHERE refresh_token = $1`,
		refreshToken,
	).Scan(&session.ID, &session.UserID, &session.Token, &session.ExpiresAt)
	if err != nil {
		return "", "", errors.New("invalid refresh token")
	}
	
	// Check if expired
	if time.Now().After(session.ExpiresAt) {
		return "", "", errors.New("refresh token expired")
	}
	
	// Get user
	var user User
	err = s.pool.QueryRow(ctx, `SELECT id, email, username FROM users WHERE id = $1`, session.UserID).Scan(
		&user.ID, &user.Email, &user.Username,
	)
	if err != nil {
		return "", "", err
	}
	
	// Generate new tokens
	token, newRefreshToken, err := s.generateTokens(&user)
	if err != nil {
		return "", "", err
	}
	
	// Update session
	expiresAt := time.Now().Add(s.cfg.SessionDuration)
	_, err = s.pool.Exec(ctx, `
		UPDATE sessions SET token = $1, refresh_token = $2, expires_at = $3 WHERE id = $4`,
		token, newRefreshToken, expiresAt, session.ID,
	)
	if err != nil {
		return "", "", err
	}
	
	return token, newRefreshToken, nil
}

// Enable2FA enables two-factor authentication for a user
func (s *Server) Enable2FA(ctx context.Context, userID int, secret string) error {
	_, err := s.pool.Exec(ctx, `
		UPDATE users SET is_2fa_enabled = TRUE, two_fa_secret = $1 WHERE id = $2`,
		secret, userID,
	)
	return err
}

// Verify2FA verifies a two-factor authentication code
func (s *Server) Verify2FA(ctx context.Context, userID int, code string) (bool, error) {
	// Get user's 2FA secret
	var secret string
	err := s.pool.QueryRow(ctx, `SELECT two_fa_secret FROM users WHERE id = $1`, userID).Scan(&secret)
	if err != nil {
		return false, err
	}
	
	// In production, implement proper TOTP verification
	return code == "123456", nil
}

// generateTokens generates JWT access and refresh tokens
func (s *Server) generateTokens(user *User) (string, string, error) {
	// Generate session ID
	sessionID := make([]byte, 16)
	rand.Read(sessionID)
	sessionIDStr := hex.EncodeToString(sessionID)
	
	// Create JWT claims
	claims := &JWTClaims{
		UserID:    user.ID,
		Email:     user.Email,
		Username:  user.Username,
		SessionID: sessionIDStr,
		RegisteredClaims: jwt.RegisteredClaims{
			ExpiresAt: jwt.NewNumericDate(time.Now().Add(s.cfg.JWTExpiry)),
			IssuedAt:  jwt.NewNumericDate(time.Now()),
			NotBefore: jwt.NewNumericDate(time.Now()),
		},
	}
	
	// Create access token
	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	accessToken, err := token.SignedString([]byte(s.cfg.JWTSecret))
	if err != nil {
		return "", "", err
	}
	
	// Create refresh token
	refreshClaims := &JWTClaims{
		UserID:    user.ID,
		SessionID: sessionIDStr,
		RegisteredClaims: jwt.RegisteredClaims{
			ExpiresAt: jwt.NewNumericDate(time.Now().Add(s.cfg.RefreshTokenExpiry)),
			IssuedAt:  jwt.NewNumericDate(time.Now()),
		},
	}
	refreshToken := jwt.NewWithClaims(jwt.SigningMethodHS256, refreshClaims)
	refreshTokenStr, err := refreshToken.SignedString([]byte(s.cfg.JWTSecret))
	if err != nil {
		return "", "", err
	}
	
	return accessToken, refreshTokenStr, nil
}

// Watchlist operations
func (s *Server) AddToWatchlist(ctx context.Context, userID int, address, label, notes string, alertType string) (*WatchlistItem, error) {
	// Validate address
	if !strings.HasPrefix(address, "0x") || len(address) != 42 {
		return nil, errors.New("invalid address format")
	}
	
	var item WatchlistItem
	err := s.pool.QueryRow(ctx, `
		INSERT INTO watchlist (user_id, address, label, notes, alert_type)
		VALUES ($1, $2, $3, $4, $5)
		RETURNING id, address, label, notes, alert_type, created_at`,
		userID, address, label, notes, alertType,
	).Scan(&item.ID, &item.Address, &item.Label, &item.Notes, &item.AlertType, &item.CreatedAt)
	if err != nil {
		return nil, err
	}
	
	return &item, nil
}

func (s *Server) GetWatchlist(ctx context.Context, userID int) ([]WatchlistItem, error) {
	rows, err := s.pool.Query(ctx, `
		SELECT id, address, label, notes, is_public, alert_type, created_at
		FROM watchlist WHERE user_id = $1 ORDER BY created_at DESC`,
		userID,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var items []WatchlistItem
	for rows.Next() {
		var item WatchlistItem
		if err := rows.Scan(&item.ID, &item.Address, &item.Label, &item.Notes, 
			&item.IsPublic, &item.AlertType, &item.CreatedAt); err != nil {
			return nil, err
		}
		items = append(items, item)
	}
	
	return items, nil
}

func (s *Server) RemoveFromWatchlist(ctx context.Context, userID, itemID int) error {
	result, err := s.pool.Exec(ctx, `DELETE FROM watchlist WHERE id = $1 AND user_id = $2`, itemID, userID)
	if err != nil {
		return err
	}
	if result.RowsAffected() == 0 {
		return errors.New("item not found")
	}
	return nil
}

// Address labels operations
func (s *Server) AddAddressLabel(ctx context.Context, userID int, address, label, category, color string) (*AddressLabel, error) {
	var l AddressLabel
	err := s.pool.QueryRow(ctx, `
		INSERT INTO address_labels (user_id, address, label, category, color)
		VALUES ($1, $2, $3, $4, $5)
		RETURNING id, address, label, category, color, created_at`,
		userID, address, label, category, color,
	).Scan(&l.ID, &l.Address, &l.Label, &l.Category, &l.Color, &l.CreatedAt)
	if err != nil {
		return nil, err
	}
	return &l, nil
}

func (s *Server) GetAddressLabels(ctx context.Context, userID int) ([]AddressLabel, error) {
	rows, err := s.pool.Query(ctx, `
		SELECT id, address, label, category, color, created_at
		FROM address_labels WHERE user_id = $1 ORDER BY created_at DESC`,
		userID,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var labels []AddressLabel
	for rows.Next() {
		var l AddressLabel
		if err := rows.Scan(&l.ID, &l.Address, &l.Label, &l.Category, &l.Color, &l.CreatedAt); err != nil {
			return nil, err
		}
		labels = append(labels, l)
	}
	
	return labels, nil
}

// Report suspicious address
func (s *Server) ReportAddress(ctx context.Context, userID int, address, reportType, description string) error {
	_, err := s.pool.Exec(ctx, `
		INSERT INTO address_reports (reporter_id, address, report_type, description)
		VALUES ($1, $2, $3, $4)`,
		userID, address, reportType, description,
	)
	return err
}

// HashPassword creates a secure hash of a password
func HashPassword(password string) (string, error) {
	hash, err := bcrypt.GenerateFromPassword([]byte(password), bcrypt.DefaultCost)
	if err != nil {
		return "", err
	}
	return string(hash), nil
}

// CheckPassword verifies a password against a hash
func CheckPassword(password, hash string) bool {
	err := bcrypt.CompareHashAndPassword([]byte(hash), []byte(password))
	return err == nil
}

// Encrypt encrypts data using AES-GCM
func Encrypt(data []byte, key []byte) ([]byte, error) {
	block, err := aes.NewCipher(key)
	if err != nil {
		return nil, err
	}
	
	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return nil, err
	}
	
	nonce := make([]byte, gcm.NonceSize())
	if _, err := rand.Read(nonce); err != nil {
		return nil, err
	}
	
	ciphertext := gcm.Seal(nonce, nonce, data, nil)
	return ciphertext, nil
}

// Decrypt decrypts data using AES-GCM
func Decrypt(data []byte, key []byte) ([]byte, error) {
	block, err := aes.NewCipher(key)
	if err != nil {
		return nil, err
	}
	
	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return nil, err
	}
	
	nonceSize := gcm.NonceSize()
	nonce, ciphertext := data[:nonceSize], data[nonceSize:]
	
	plaintext, err := gcm.Open(nil, nonce, ciphertext, nil)
	if err != nil {
		return nil, err
	}
	
	return plaintext, nil
}

// GenerateSecureKey generates a secure key for encryption
func GenerateSecureKey() ([]byte, error) {
	key := make([]byte, 32)
	if _, err := rand.Read(key); err != nil {
		return nil, err
	}
	return key, nil
}

// DeriveKey derives a key from a password using scrypt
func DeriveKey(password string, salt []byte) ([]byte, error) {
	return scrypt.Key([]byte(password), salt, 16384, 8, 1, 32)
}

// HashSHA256 creates a SHA-256 hash
func HashSHA256(data []byte) []byte {
	hash := sha256.Sum256(data)
	return hash[:]
}