// Package admin provides white-label admin system.
package admin

import (
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"math/big"
	"net"
	"net/mail"
	"sync"
	"time"
)

// UserRole represents user role.
type UserRole int

const (
	RoleSuperAdmin UserRole = iota // Super admin (white label owner)
	RoleAdmin                 // Admin
	RoleUser                 // Regular user
)

// UserStatus represents user status.
type UserStatus int

const (
	StatusPending UserStatus = iota
	StatusActive
	StatusSuspended
	StatusBanned
)

// ProductStatus represents product status.
type ProductStatus int

const (
	ProductStatusPending ProductStatus = iota
	ProductStatusActive
	ProductStatusPaused
	ProductStatusHalted
	ProductStatusDestroyed
)

// User represents a white-label client.
type User struct {
	ID             string
	Email          string
	PasswordHash   string
	Role          UserRole
	Status        UserStatus
	CreatedAt     uint64
	UpdatedAt     uint64
	LastLogin     uint64
	APIKey        string
	APIKeyHash    string
	Features      []string
	Permissions   map[string]bool
	ProductID     string
	AdminID       string // Who approved this user
}

// Product represents a white-label product.
type Product struct {
	ID           string
	Name         string
	Domain      string
	Cloud       string
	Storage     string
	Status      ProductStatus
	OwnerID     string
	CreatedAt   uint64
	APIKeys     map[string]*ProductAPIKey
	Features    []string
	Config      map[string]interface{}
}

// ProductAPIKey represents API key for product.
type ProductAPIKey struct {
	Key        string
	Name       string
	CreatedAt  uint64
	ExpiresAt uint64
	Active    bool
}

// WhiteLabelConfig represents white-label configuration.
type WhiteLabelConfig struct {
	CompanyName      string
	SupportEmail    string
	LogoURL         string
	FaviconURL      string
	PrimaryColor    string
	SecondaryColor  string
	AccentColor     string
	TermsURL        string
	PrivacyURL      string
}

// WhiteLabelAdmin manages white-label clients.
type WhiteLabelAdmin struct {
	mu sync.RWMutex

	// Users by ID
	users map[string]*User
	// Users by email
	usersByEmail map[string]*User
	// Users by API key
	usersByAPIKey map[string]*User
	// Products
	products map[string]*Product
	// Products by owner
	productsByOwner map[string][]*Product
	// Config
	config *WhiteLabelConfig
	// Session manager
	sessions *SessionManager
	// Rate limiter
	rateLimiter *RateLimiter
	// Encryption key
	encKey []byte
}

// NewWhiteLabelAdmin creates a new admin system.
func NewWhiteLabelAdmin() (*WhiteLabelAdmin, error) {
	encKey, err := generateEncryptionKey()
	if err != nil {
		return nil, err
	}

	return &WhiteLabelAdmin{
		users:           make(map[string]*User),
		usersByEmail:    make(map[string]*User),
		usersByAPIKey:  make(map[string]*User),
		products:      make(map[string]*Product),
		productsByOwner: make(map[string][]*Product),
		config:        &WhiteLabelConfig{},
		sessions:      NewSessionManager(),
		rateLimiter:   NewRateLimiter(100, time.Minute),
		encKey:       encKey,
	}, nil
}

// Register registers a new user.
func (wla *WhiteLabelAdmin) Register(email, password string) (string, error) {
	if _, err := mail.ParseAddress(email); err != nil {
		return "", fmt.Errorf("invalid email")
	}

	if len(password) < 12 {
		return "", fmt.Errorf("password must be at least 12 characters")
	}

	wla.mu.Lock()
	defer wla.mu.Unlock()

	if _, exists := wla.usersByEmail[email]; exists {
		return "", fmt.Errorf("email already registered")
	}

	passHash := hashPassword(password)
	userID := generateID()

	user := &User{
		ID:           userID,
		Email:        email,
		PasswordHash: passHash,
		Role:         RoleUser,
		Status:       StatusPending,
		CreatedAt:    uint64(time.Now().Unix()),
		UpdatedAt:   uint64(time.Now().Unix()),
		Permissions: make(map[string]bool),
	}

	wla.users[userID] = user
	wla.usersByEmail[email] = user

	return userID, nil
}

// Login logs in a user.
func (wla *WhiteLabelAdmin) Login(email, password, ip string) (string, error) {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	user, exists := wla.usersByEmail[email]
	if !exists {
		return "", fmt.Errorf("invalid credentials")
	}

	passHash := hashPassword(password)
	if subtle.ConstantTimeCompare([]byte(user.PasswordHash), []byte(passHash)) != 1 {
		return "", fmt.Errorf("invalid credentials")
	}

	if user.Status == StatusBanned {
		return "", fmt.Errorf("account banned")
	}

	session := wla.sessions.Create(user.ID, ip)
	user.LastLogin = uint64(time.Now().Unix())

	return session, nil
}

// Logout logs out a user.
func (wla *WhiteLabelAdmin) Logout(sessionID string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()
	return wla.sessions.Revoke(sessionID)
}

// VerifySession verifies a session.
func (wla *WhiteLabelAdmin) VerifySession(sessionID string) (string, error) {
	wla.mu.RLock()
	defer wla.mu.RUnlock()
	return wla.sessions.Verify(sessionID)
}

// ApproveUser approves a user.
func (wla *WhiteLabelAdmin) ApproveUser(userID, adminID string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized")
	}

	user, exists := wla.users[userID]
	if !exists {
		return fmt.Errorf("user not found")
	}

	user.Status = StatusActive
	user.UpdatedAt = uint64(time.Now().Unix())
	user.AdminID = adminID

	return nil
}

// GrantRole grants role to user.
func (wla *WhiteLabelAdmin) GrantRole(userID, adminID string, role UserRole) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized")
	}

	user, exists := wla.users[userID]
	if !exists {
		return fmt.Errorf("user not found")
	}

	if role == RoleSuperAdmin && admin.Role != RoleSuperAdmin {
		return fmt.Errorf("unauthorized")
	}

	user.Role = role
	user.UpdatedAt = uint64(time.Now().Unix())

	return nil
}

// SuspendUser suspends a user.
func (wla *WhiteLabelAdmin) SuspendUser(userID, adminID string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized")
	}

	user, exists := wla.users[userID]
	if !exists {
		return fmt.Errorf("user not found")
	}

	user.Status = StatusSuspended
	user.UpdatedAt = uint64(time.Now().Unix())

	return nil
}

// BanUser bans a user.
func (wla *WhiteLabelAdmin) BanUser(userID, adminID string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized")
	}

	user, exists := wla.users[userID]
	if !exists {
		return fmt.Errorf("user not found")
	}

	user.Status = StatusBanned
	user.UpdatedAt = uint64(time.Now().Unix())
	wla.sessions.RevokeAll(userID)

	return nil
}

// GetUser returns user by ID.
func (wla *WhiteLabelAdmin) GetUser(userID string) (*User, bool) {
	wla.mu.RLock()
	defer wla.mu.RUnlock()
	user, ok := wla.users[userID]
	return user, ok
}

// CreateProduct creates a new white-label product.
func (wla *WhiteLabelAdmin) CreateProduct(ownerID, name, domain, cloud, storage string) (string, error) {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	owner, exists := wla.users[ownerID]
	if !exists || owner.Status != StatusActive {
		return "", fmt.Errorf("user not active")
	}

	productID := generateID()

	product := &Product{
		ID:          productID,
		Name:        name,
		Domain:     domain,
		Cloud:     cloud,
		Storage:   storage,
		Status:    ProductStatusPending,
		OwnerID:   ownerID,
		CreatedAt: uint64(time.Now().Unix()),
		APIKeys:  make(map[string]*ProductAPIKey),
		Features: getAllFeatures(),
		Config: make(map[string]interface{}),
	}

	wla.products[productID] = product
	wla.productsByOwner[ownerID] = append(wla.productsByOwner[ownerID], product)

	return productID, nil
}

// ApproveProduct approves a product.
func (wla *WhiteLabelAdmin) ApproveProduct(productID, adminID string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized")
	}

	product, exists := wla.products[productID]
	if !exists {
		return fmt.Errorf("product not found")
	}

	product.Status = ProductStatusActive

	return nil
}

// PauseProduct pauses a product.
func (wla *WhiteLabelAdmin) PauseProduct(productID, adminID string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized")
	}

	product, exists := wla.products[productID]
	if !exists {
		return fmt.Errorf("product not found")
	}

	product.Status = ProductStatusPaused

	return nil
}

// HaltProduct halts a product.
func (wla *WhiteLabelAdmin) HaltProduct(productID, adminID string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized")
	}

	product, exists := wla.products[productID]
	if !exists {
		return fmt.Errorf("product not found")
	}

	product.Status = ProductStatusHalted

	return nil
}

// DestroyProduct destroys a product.
func (wla *WhiteLabelAdmin) DestroyProduct(productID, adminID string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized")
	}

	product, exists := wla.products[productID]
	if !exists {
		return fmt.Errorf("product not found")
	}

	product.Status = ProductStatusDestroyed

	return nil
}

// GetProduct returns product by ID.
func (wla *WhiteLabelAdmin) GetProduct(productID string) (*Product, bool) {
	wla.mu.RLock()
	defer wla.mu.RUnlock()
	product, ok := wla.products[productID]
	return product, ok
}

// GrantPermission grants feature permission to user.
func (wla *WhiteLabelAdmin) GrantPermission(userID, adminID, feature string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized")
	}

	user, exists := wla.users[userID]
	if !exists {
		return fmt.Errorf("user not found")
	}

	user.Permissions[feature] = true

	return nil
}

// RevokePermission revokes feature permission.
func (wla *WhiteLabelAdmin) RevokePermission(userID, adminID, feature string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized")
	}

	user, exists := wla.users[userID]
	if !exists {
		return fmt.Errorf("user not found")
	}

	delete(user.Permissions, feature)

	return nil
}

// HasPermission checks if user has permission.
func (wla *WhiteLabelAdmin) HasPermission(userID, feature string) bool {
	wla.mu.RLock()
	defer wla.mu.RUnlock()

	user, exists := wla.users[userID]
	if !exists {
		return false
	}

	return user.Permissions[feature]
}

// VerifyAPIKey verifies API key.
func (wla *WhiteLabelAdmin) VerifyAPIKey(apiKey, productID string) bool {
	wla.mu.RLock()
	defer wla.mu.RUnlock()

	product, exists := wla.products[productID]
	if !exists {
		return false
	}

	for _, key := range product.APIKeys {
		if key.Key == apiKey && key.Active {
			return true
		}
	}

	return false
}

// GenerateProductAPIKey generates API key for product.
func (wla *WhiteLabelAdmin) GenerateProductAPIKey(productID, adminID, name string) (string, error) {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return "", fmt.Errorf("unauthorized")
	}

	product, exists := wla.products[productID]
	if !exists {
		return "", fmt.Errorf("product not found")
	}

	apiKey := generateAPIKey()
	keyHash := hashAPIKey(apiKey)

	product.APIKeys[keyHash] = &ProductAPIKey{
		Key:       apiKey,
		Name:      name,
		CreatedAt: uint64(time.Now().Unix()),
		Active:   true,
	}

	return apiKey, nil
}

// SessionManager manages user sessions.
type SessionManager struct {
	mu sync.RWMutex
	sessions map[string]*Session
}

// Session represents a user session.
type Session struct {
	UserID    string
	ID        string
	IP        string
	CreatedAt uint64
	ExpiresAt uint64
}

// NewSessionManager creates a new session manager.
func NewSessionManager() *SessionManager {
	return &SessionManager{
		sessions: make(map[string]*Session),
	}
}

// Create creates a new session.
func (sm *SessionManager) Create(userID, ip string) string {
	sm.mu.Lock()
	defer sm.mu.Unlock()

	sessionID := generateID()
	expiresAt := uint64(time.Now().Unix()) + 86400

	sm.sessions[sessionID] = &Session{
		UserID:    userID,
		ID:        sessionID,
		IP:        ip,
		CreatedAt: uint64(time.Now().Unix()),
		ExpiresAt: expiresAt,
	}

	return sessionID
}

// Verify verifies a session.
func (sm *SessionManager) Verify(sessionID string) (string, error) {
	sm.mu.RLock()
	defer sm.mu.RUnlock()

	session, exists := sm.sessions[sessionID]
	if !exists {
		return "", fmt.Errorf("invalid session")
	}

	if uint64(time.Now().Unix()) > session.ExpiresAt {
		return "", fmt.Errorf("session expired")
	}

	return session.UserID, nil
}

// Revoke revokes a session.
func (sm *SessionManager) Revoke(sessionID string) error {
	sm.mu.Lock()
	defer sm.mu.Unlock()
	delete(sm.sessions, sessionID)
	return nil
}

// RevokeAll revokes all sessions for a user.
func (sm *SessionManager) RevokeAll(userID string) {
	sm.mu.Lock()
	defer sm.mu.Unlock()

	for id, session := range sm.sessions {
		if session.UserID == userID {
			delete(sm.sessions, id)
		}
	}
}

// RateLimiter provides rate limiting.
type RateLimiter struct {
	mu       sync.RWMutex
	requests map[string][]uint64
	maxReq   int
	window   time.Duration
}

// NewRateLimiter creates a new rate limiter.
func NewRateLimiter(max int, window time.Duration) *RateLimiter {
	return &RateLimiter{
		requests: make(map[string][]uint64),
		maxReq:   max,
		window:   window,
	}
}

// Allow checks if request is allowed.
func (rl *RateLimiter) Allow(key string) bool {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	now := uint64(time.Now().Unix())
	requests := rl.requests[key]

	valid := make([]uint64, 0)
	for _, t := range requests {
		if now-uint64(rl.window.Seconds()) < t {
			valid = append(valid, t)
		}
	}

	if len(valid) >= rl.maxReq {
		return false
	}

	valid = append(valid, now)
	rl.requests[key] = valid

	return true
}

// Helper functions

func generateID() string {
	b := make([]byte, 16)
	rand.Read(b)
	return fmt.Sprintf("0x%x", b)
}

func generateAPIKey() string {
	b := make([]byte, 32)
	rand.Read(b)
	return base64.URLEncoding.EncodeToString(b)
}

func hashPassword(password string) string {
	hash := sha256.Sum256([]byte(password))
	return fmt.Sprintf("0x%x", hash)
}

func hashAPIKey(apiKey string) string {
	hash := sha256.Sum256([]byte(apiKey))
	return fmt.Sprintf("0x%x", hash)
}

func generateEncryptionKey() ([]byte, error) {
	key := make([]byte, 32)
	_, err := rand.Read(key)
	if err != nil {
		return nil, err
	}
	return key, nil
}

func encrypt(data []byte, key []byte) ([]byte, error) {
	block, err := aes.NewCipher(key)
	if err != nil {
		return nil, err
	}

	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return nil, err
	}

	nonce := make([]byte, gcm.NonceSize())
	if _, err := io.ReadFull(rand, nonce); err != nil {
		return nil, err
	}

	return gcm.Seal(nonce, nonce, data, nil), nil
}

func decrypt(data []byte, key []byte) ([]byte, error) {
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
	return gcm.Open(nil, nonce, ciphertext, nil)
}

func getAllFeatures() []string {
	return []string{
		"wallet.create",
		"wallet.send",
		"wallet.receive",
		"nft.mint",
		"nft.transfer",
		"token.create",
		"token.transfer",
		"bridge.deposit",
		"bridge.withdraw",
		"staking.stake",
		"staking.unstake",
	}
}

// SanitizeInput sanitizes user input.
func SanitizeInput(input string) string {
	result := make([]rune, 0)
	for _, r := range input {
		if (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9') || r == '-' || r == '_' {
			result = append(result, r)
		}
	}
	return string(result)
}

// ValidateDomain validates domain.
func ValidateDomain(domain string) error {
	if len(domain) > 253 {
		return fmt.Errorf("domain too long")
	}

	if net.ParseIP(domain) != nil {
		return fmt.Errorf("IP not allowed")
	}

	return nil
}

// GetStats returns admin statistics.
func (wla *WhiteLabelAdmin) GetStats() (map[string]interface{}, error) {
	wla.mu.RLock()
	defer wla.mu.RUnlock()

	stats := map[string]interface{}{
		"totalUsers":      len(wla.users),
		"activeUsers":    0,
		"suspended":     0,
		"banned":        0,
		"totalProducts": len(wla.products),
		"activeProducts": 0,
	}

	for _, user := range wla.users {
		switch user.Status {
		case StatusActive:
			stats["activeUsers"] = stats["activeUsers"].(int) + 1
		case StatusSuspended:
			stats["suspended"] = stats["suspended"].(int) + 1
		case StatusBanned:
			stats["banned"] = stats["banned"].(int) + 1
		}
	}

	for _, product := range wla.products {
		if product.Status == ProductStatusActive {
			stats["activeProducts"] = stats["activeProducts"].(int) + 1
		}
	}

	return stats, nil
}

// VerifyIP verifies client IP.
func (wla *WhiteLabelAdmin) VerifyIP(ip string) bool {
	parsedIP := net.ParseIP(ip)
	if parsedIP == nil {
		return false
	}

	if parsedIP.IsPrivate() || parsedIP.IsUnspecified() {
		return false
	}

	return true
}

var _ = context.Background() // Use context
var _ = big.NewInt(0)     // Use big.Int
var _ = json.Marshal     // Use json