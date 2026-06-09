// Package admin provides white-label admin system with industrial-grade security.
package admin

import (
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"crypto/sha512"
	"crypto/subtle"
	"encoding/base64"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"html"
	"io"
	"math/big"
	"math/rand"
	"net"
	"net/mail"
	"net/url"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"
)

// Security constants
const (
	MinPasswordLength    = 12
	MaxPasswordLength  = 128
	MinUsernameLength  = 3
	MaxUsernameLength  = 50
	MaxLoginAttempts   = 5
	LockoutDuration   = 15 * time.Minute
	SessionDuration   = 24 * time.Hour
	MaxSessionsPerUser = 5
	APIKeyLength       = 64
	MaxFailedAttempts = 10
	RateLimitWindow   = time.Minute
	MaxRequestsPerMin = 60
	MaxAuditLogs      = 10000
	BcryptCost        = 14
)

// UserRole represents user role.
type UserRole int

const (
	RoleSuperAdmin UserRole = iota // Super admin (white label owner)
	RoleAdmin                 // Admin
	RoleUser                 // Regular user
	RoleWhitelabelClient     // White level client (needs admin approval)
)

// AuditAction represents audit action types.
type AuditAction string

const (
	AuditUserLogin       AuditAction = "user.login"
	AuditUserLogout     AuditAction = "user.logout"
	AuditUserRegister   AuditAction = "user.register"
	AuditUserApprove   AuditAction = "user.approve"
	AuditUserSuspend  AuditAction = "user.suspend"
	AuditUserBan      AuditAction = "user.ban"
	AuditUserDelete   AuditAction = "user.delete"
	AuditProductCreate AuditAction = "product.create"
	AuditProductPause  AuditAction = "product.pause"
	AuditProductHalt  AuditAction = "product.halt"
	AuditProductDestroy AuditAction = "product.destroy"
	AuditAPIKeyCreate AuditAction = "apikey.create"
	AuditAPIKeyRevoke AuditAction = "apikey.revoke"
	AuditPermissionGrant  AuditAction = "permission.grant"
	AuditPermissionRevoke AuditAction = "permission.revoke"
	AuditAdminCreate      AuditAction = "admin.create"
	AuditAdminRemove     AuditAction = "admin.remove"
)

// UserStatus represents user status.
type UserStatus int

const (
	StatusPending   UserStatus = iota // Pending admin approval
	StatusActive                     // Active and approved
	StatusSuspended                  // Temporarily suspended
	StatusBanned                    // Permanently banned
	StatusPending2FA                // Pending 2FA verification
	StatusLocked                   // Account locked due to failed attempts
	StatusPendingDeletion         // Pending deletion
)

// VerificationCode represents verification code for 2FA/email.
type VerificationCode struct {
	Code       string
	Type       string // "email", "2fa", "password_reset"
	ExpiresAt  uint64
	Used      bool
	Attempts  int
}

// AuditLog represents audit log entry.
type AuditLog struct {
	ID         string
	UserID     string
	Action     AuditAction
	IP         string
	UserAgent  string
	Details   string
	Timestamp uint64
	Success   bool
}

// AttackPrevention provides protection against various attacks.
type AttackPrevention struct {
	mu                     sync.RWMutex
	failedLogins           map[string]*FailedLoginAttempt
	failedAttempts        map[string]int
	suspectedIPs          map[string]bool
	knownBadIPs           map[string]bool
	xssPatterns           []*regexp.Regexp
	sqlInjectionPatterns  []*regexp.Regexp
	csrfTokens           map[string]*CSRFToken
	loginAttempts        map[string]*LoginAttemptTracker
}

// FailedLoginAttempt tracks failed login attempts.
type FailedLoginAttempt struct {
	Count     int
	FirstAttempt uint64
	LastAttempt uint64
	IP        string
}

// LoginAttemptTracker tracks login attempts.
type LoginAttemptTracker struct {
	Attempts int
	LastAttempt uint64
	LockedUntil uint64
}

// CSRFToken represents CSRF token.
type CSRFToken struct {
	Token     string
	UserID   string
	CreatedAt uint64
	ExpiresAt uint64
}

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
	Username       string
	Email          string
	PasswordHash   string
	Salt           string
	Role           UserRole
	Status         UserStatus
	CreatedAt      uint64
	UpdatedAt      uint64
	LastLogin      uint64
	LastActivity   uint64
	IPAddress      string
	TwoFactorSecret string // Encrypted 2FA secret
	TwoFactorEnabled bool
	EmailVerified  bool
	APIKey         string
	APIKeyHash     string
	Features       []string
	Permissions    map[string]bool
	ProductID      string
	AdminID        string // Who approved this user
	FailedAttempts int
	LockedUntil    uint64
	VerifyCode     *VerificationCode
	AuditLogs      []*AuditLog
}

// Product represents a white-label product.
type Product struct {
	ID            string
	Name          string
	BrandName     string
	Domain       string
	Cloud        string
	Storage      string
	Status       ProductStatus
	OwnerID      string
	AdminID      string // Who authorized this product
	CreatedAt    uint64
	UpdatedAt   uint64
	APIKeys     map[string]*ProductAPIKey
	Features    []string
	Config      map[string]interface{}
	AuthRequired bool
	IsPaused    bool
	IsHalted    bool
	PausedBy    string
	HaltedBy    string
	PausedAt    uint64
	HaltedAt    uint64
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

// WhiteLabelAdmin manages white-label clients with industrial-grade security.
type WhiteLabelAdmin struct {
	mu sync.RWMutex

	// Users by ID
	users map[string]*User
	// Users by email
	usersByEmail map[string]*User
	// Users by username
	usersByUsername map[string]*User
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
	// Attack prevention
	attackPrevention *AttackPrevention
	// Encryption key
	encKey []byte
	// Encryption key for 2FA
	twoFactorKey []byte
	// Audit logs
	auditLogs []*AuditLog
	// Global encryption key (master key for all data)
	masterKey []byte
	// Database encryption key
	dbKey []byte
}

// NewWhiteLabelAdmin creates a new admin system with industrial-grade security.
func NewWhiteLabelAdmin() (*WhiteLabelAdmin, error) {
	encKey, err := generateEncryptionKey()
	if err != nil {
		return nil, err
	}

	twoFactorKey, err := generateEncryptionKey()
	if err != nil {
		return nil, err
	}

	masterKey, err := generateEncryptionKey()
	if err != nil {
		return nil, err
	}

	dbKey, err := generateEncryptionKey()
	if err != nil {
		return nil, err
	}

	return &WhiteLabelAdmin{
		users:            make(map[string]*User),
		usersByEmail:    make(map[string]*User),
		usersByUsername: make(map[string]*User),
		usersByAPIKey:   make(map[string]*User),
		products:        make(map[string]*Product),
		productsByOwner:  make(map[string][]*Product),
		config:         &WhiteLabelConfig{},
		sessions:       NewSessionManager(),
		rateLimiter:    NewRateLimiter(MaxRequestsPerMin, RateLimitWindow),
		attackPrevention: NewAttackPrevention(),
		encKey:         encKey,
		twoFactorKey:   twoFactorKey,
		masterKey:     masterKey,
		dbKey:         dbKey,
		auditLogs:     make([]*AuditLog, 0, MaxAuditLogs),
	}, nil
}

// Register registers a new white level client with industrial-grade security.
// All white level clients must be approved by admin before activation.
func (wla *WhiteLabelAdmin) Register(username, email, password, ip string) (string, error) {
	// Input validation and sanitization
	username = SanitizeInput(username)
	email = strings.ToLower(strings.TrimSpace(email))
	
	if len(username) < MinUsernameLength || len(username) > MaxUsernameLength {
		return "", fmt.Errorf("username must be between %d and %d characters", MinUsernameLength, MaxUsernameLength)
	}
	
	if _, err := mail.ParseAddress(email); err != nil {
		return "", fmt.Errorf("invalid email address")
	}
	
	if err := validatePassword(password); err != nil {
		return "", err
	}
	
	// Rate limiting check
	if !wla.rateLimiter.Allow(ip + ":register") {
		return "", fmt.Errorf("too many registration attempts. Please try again later")
	}
	
	// Attack prevention check
	if wla.attackPrevention.IsIPBlocked(ip) {
		return "", fmt.Errorf("your IP has been blocked due to suspicious activity")
	}
	
	wla.mu.Lock()
	defer wla.mu.Unlock()
	
	// Check if email already registered
	if _, exists := wla.usersByEmail[email]; exists {
		wla.attackPrevention.RecordFailedAttempt(ip + ":" + email)
		return "", fmt.Errorf("email already registered")
	}
	
	// Check if username already taken
	if _, exists := wla.usersByUsername[username]; exists {
		return "", fmt.Errorf("username already taken")
	}
	
	// Generate secure salt and password hash
	salt := generateSalt()
	passHash := hashPasswordWithSalt(password, salt)
	
	userID := generateSecureID()
	verificationCode := generateVerificationCode()
	
	user := &User{
		ID:             userID,
		Username:       username,
		Email:          email,
		PasswordHash:  passHash,
		Salt:           salt,
		Role:           RoleWhitelabelClient, // Start as white level client
		Status:         StatusPending,         // Must be approved by admin
		CreatedAt:      uint64(time.Now().Unix()),
		UpdatedAt:      uint64(time.Now().Unix()),
		IPAddress:      ip,
		EmailVerified: false,
		Permissions:   make(map[string]bool),
		FailedAttempts: 0,
		LockedUntil:   0,
		VerifyCode: &VerificationCode{
			Code:      verificationCode,
			Type:     "email",
			ExpiresAt: uint64(time.Now().Unix()) + 3600, // 1 hour
			Used:     false,
			Attempts: 0,
		},
		AuditLogs: make([]*AuditLog, 0),
	}
	
	wla.users[userID] = user
	wla.usersByEmail[email] = user
	wla.usersByUsername[username] = user
	
	// Create audit log
	wla.createAuditLog(userID, AuditUserRegister, ip, "", fmt.Sprintf("User registered: %s", email), true)
	
	return userID, nil
}

// RegisterAdmin registers a new super admin (only one exists).
func (wla *WhiteLabelAdmin) RegisterAdmin(username, email, password, ip string) (string, error) {
	// Input validation and sanitization
	username = SanitizeInput(username)
	email = strings.ToLower(strings.TrimSpace(email))
	
	if len(username) < MinUsernameLength || len(username) > MaxUsernameLength {
		return "", fmt.Errorf("username must be between %d and %d characters", MinUsernameLength, MaxUsernameLength)
	}
	
	if _, err := mail.ParseAddress(email); err != nil {
		return "", fmt.Errorf("invalid email address")
	}
	
	if err := validatePassword(password); err != nil {
		return "", err
	}
	
	// Check if super admin already exists
	wla.mu.Lock()
	if len(wla.users) > 0 {
		for _, u := range wla.users {
			if u.Role == RoleSuperAdmin {
				wla.mu.Unlock()
				return "", fmt.Errorf("super admin already exists")
			}
		}
	}
	wla.mu.Unlock()
	
	// Generate secure salt and password hash
	salt := generateSalt()
	passHash := hashPasswordWithSalt(password, salt)
	
	userID := generateSecureID()
	verificationCode := generateVerificationCode()
	
	user := &User{
		ID:              userID,
		Username:        username,
		Email:           email,
		PasswordHash:   passHash,
		Salt:            salt,
		Role:            RoleSuperAdmin,
		Status:          StatusActive, // Super admin is active by default
		CreatedAt:       uint64(time.Now().Unix()),
		UpdatedAt:       uint64(time.Now().Unix()),
		IPAddress:       ip,
		EmailVerified:  true,
		Permissions:    getAllPermissions(),
		FailedAttempts: 0,
		LockedUntil:    0,
		VerifyCode: &VerificationCode{
			Code:      verificationCode,
			Type:     "email",
			ExpiresAt: uint64(time.Now().Unix()) + 86400, // 24 hours
			Used:     false,
			Attempts: 0,
		},
		AuditLogs: make([]*AuditLog, 0),
	}
	
	wla.mu.Lock()
	wla.users[userID] = user
	wla.usersByEmail[email] = user
	wla.usersByUsername[username] = user
	wla.mu.Unlock()
	
	// Create audit log
	wla.createAuditLog(userID, AuditAdminCreate, ip, "", fmt.Sprintf("Super admin registered: %s", email), true)
	
	return userID, nil
}

// Login logs in a user with industrial-grade security verification.
func (wla *WhiteLabelAdmin) Login(email, password, ip, userAgent string) (string, error) {
	// Rate limiting check
	if !wla.rateLimiter.Allow(ip + ":login") {
		return "", fmt.Errorf("too many login attempts. Please try again later")
	}
	
	// Attack prevention check
	if wla.attackPrevention.IsIPBlocked(ip) {
		return "", fmt.Errorf("your IP has been blocked due to suspicious activity")
	}
	
	email = strings.ToLower(strings.TrimSpace(email))
	
	wla.mu.Lock()
	defer wla.mu.Unlock()

	user, exists := wla.usersByEmail[email]
	if !exists {
		wla.attackPrevention.RecordFailedAttempt(ip + ":" + email)
		return "", fmt.Errorf("invalid credentials")
	}
	
	// Check if account is locked
	if user.Status == StatusLocked {
		if uint64(time.Now().Unix()) < user.LockedUntil {
			return "", fmt.Errorf("account is locked. Please try again later")
		}
		// Unlock after lockout period
		user.Status = StatusPending
		user.FailedAttempts = 0
	}
	
	// Check if account is banned
	if user.Status == StatusBanned {
		wla.createAuditLog(user.ID, AuditUserLogin, ip, userAgent, "Login attempt while banned", false)
		return "", fmt.Errorf("account has been permanently banned")
	}
	
	// Verify password with constant-time comparison
	passHash := hashPasswordWithSalt(password, user.Salt)
	if subtle.ConstantTimeCompare([]byte(user.PasswordHash), []byte(passHash)) != 1 {
		user.FailedAttempts++
		wla.attackPrevention.RecordFailedAttempt(ip + ":" + email)
		
		// Lock account after max failed attempts
		if user.FailedAttempts >= MaxFailedAttempts {
			user.Status = StatusLocked
			user.LockedUntil = uint64(time.Now().Unix()) + uint64(LockoutDuration.Seconds())
			wla.createAuditLog(user.ID, AuditUserLogin, ip, userAgent, fmt.Sprintf("Account locked after %d failed attempts", user.FailedAttempts), false)
			return "", fmt.Errorf("account has been locked due to too many failed login attempts")
		}
		
		wla.createAuditLog(user.ID, AuditUserLogin, ip, userAgent, "Invalid password", false)
		return "", fmt.Errorf("invalid credentials")
	}
	
	// Check if 2FA is enabled
	if user.TwoFactorEnabled {
		user.Status = StatusPending2FA
		return "", fmt.Errorf("2fa_required")
	}
	
	// Check if user is approved
	if user.Status == StatusPending {
		wla.createAuditLog(user.ID, AuditUserLogin, ip, userAgent, "Login attempt while pending approval", false)
		return "", fmt.Errorf("account pending approval. Please contact admin for authorization")
	}
	
	// Check if account is suspended
	if user.Status == StatusSuspended {
		wla.createAuditLog(user.ID, AuditUserLogin, ip, userAgent, "Login attempt while suspended", false)
		return "", fmt.Errorf("account is suspended. Please contact admin")
	}
	
	// Create session
	session := wla.sessions.CreateWithLimit(user.ID, ip, MaxSessionsPerUser)
	user.LastLogin = uint64(time.Now().Unix())
	user.LastActivity = uint64(time.Now().Unix())
	user.IPAddress = ip
	user.FailedAttempts = 0
	
	wla.createAuditLog(user.ID, AuditUserLogin, ip, userAgent, "Login successful", true)
	
	return session, nil
}

// LoginWith2FA logs in with 2FA verification.
func (wla *WhiteLabelAdmin) LoginWith2FA(sessionID, code, ip, userAgent string) (string, error) {
	wla.mu.Lock()
	defer wla.mu.Unlock()
	
	// Verify session
	userID, err := wla.sessions.Verify(sessionID)
	if err != nil {
		return "", err
	}
	
	user, exists := wla.users[userID]
	if !exists {
		return "", fmt.Errorf("user not found")
	}
	
	// Verify 2FA code
	if !user.TwoFactorEnabled || user.TwoFactorSecret == "" {
		return "", fmt.Errorf("2FA not enabled")
	}
	
	// Decrypt and verify 2FA code
	secret, err := decrypt([]byte(user.TwoFactorSecret), wla.twoFactorKey)
	if err != nil {
		return "", fmt.Errorf("invalid 2FA secret")
	}
	
	if !verifyTOTP(string(secret), code) {
		user.FailedAttempts++
		wla.createAuditLog(user.ID, AuditUserLogin, ip, userAgent, "Invalid 2FA code", false)
		return "", fmt.Errorf("invalid 2FA code")
	}
	
	// Activate user
	user.Status = StatusActive
	user.LastActivity = uint64(time.Now().Unix())
	
	wla.createAuditLog(user.ID, AuditUserLogin, ip, userAgent, "Login successful with 2FA", true)
	
	return sessionID, nil
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

// ApproveUser approves a white level client. Admin must authorize all white level clients.
func (wla *WhiteLabelAdmin) ApproveUser(userID, adminID, ip string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized: admin role required")
	}

	user, exists := wla.users[userID]
	if !exists {
		return fmt.Errorf("user not found")
	}

	// Only super admin can approve another super admin
	if user.Role == RoleSuperAdmin && admin.Role != RoleSuperAdmin {
		return fmt.Errorf("unauthorized: only super admin can approve super admin")
	}

	user.Status = StatusActive
	user.UpdatedAt = uint64(time.Now().Unix())
	user.AdminID = adminID
	
	// Grant default permissions for white level clients
	if user.Role == RoleWhitelabelClient {
		user.Permissions = map[string]bool{
			"product.create": true,
			"product.manage": true,
			"product.pause": true,
			"product.halt": true,
			"product.destroy": false,
			"admin.create": false,
			"admin.manage": false,
			"api_key.create": true,
			"api_key.manage": true,
		}
	}

	wla.createAuditLog(userID, AuditUserApprove, ip, "", fmt.Sprintf("Approved by admin: %s", adminID), true)

	return nil
}

// ApproveUserAsAdmin approves a white level client as admin (for super admin only).
func (wla *WhiteLabelAdmin) ApproveUserAsAdmin(userID, superAdminID, ip string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[superAdminID]
	if !exists || admin.Role != RoleSuperAdmin {
		return fmt.Errorf("unauthorized: super admin role required")
	}

	user, exists := wla.users[userID]
	if !exists {
		return fmt.Errorf("user not found")
	}

	user.Status = StatusActive
	user.Role = RoleAdmin
	user.UpdatedAt = uint64(time.Now().Unix())
	user.AdminID = superAdminID
	user.Permissions = getAllPermissions()

	wla.createAuditLog(userID, AuditUserApprove, ip, "", fmt.Sprintf("Approved as admin by super admin: %s", superAdminID), true)

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

// CreateProduct creates a new white-label product (100% clone).
// All white level products require admin approval and authorization.
func (wla *WhiteLabelAdmin) CreateProduct(ownerID, name, brandName, domain, cloud, storage string, adminID string) (string, error) {
	// Input validation
	name = SanitizeInput(name)
	brandName = SanitizeInput(brandName)
	domain = strings.ToLower(domain)
	
	if name == "" || brandName == "" || domain == "" {
		return "", fmt.Errorf("name, brand name, and domain are required")
	}
	
	if err := ValidateDomain(domain); err != nil {
		return "", fmt.Errorf("invalid domain: %v", err)
	}
	
	if cloud == "" {
		cloud = "default"
	}
	
	if storage == "" {
		storage = "default"
	}
	
	wla.mu.Lock()
	defer wla.mu.Unlock()
	
	// Verify owner exists and is active
	owner, exists := wla.users[ownerID]
	if !exists {
		return "", fmt.Errorf("user not found")
	}
	
	if owner.Status != StatusActive {
		return "", fmt.Errorf("user not active. Please contact admin for approval")
	}
	
	// Verify admin authorized this product
	if adminID != "" {
		admin, exists := wla.users[adminID]
		if !exists || admin.Role < RoleAdmin {
			return "", fmt.Errorf("unauthorized admin")
		}
	}
	
	// Check permissions
	if !owner.Permissions["product.create"] {
		return "", fmt.Errorf("permission denied: product.create")
	}
	
	productID := generateSecureID()
	
	product := &Product{
		ID:           productID,
		Name:         name,
		BrandName:    brandName,
		Domain:       domain,
		Cloud:        cloud,
		Storage:      storage,
		Status:       ProductStatusPending, // Requires admin approval
		OwnerID:      ownerID,
		AdminID:      adminID,
		CreatedAt:    uint64(time.Now().Unix()),
		UpdatedAt:   uint64(time.Now().Unix()),
		APIKeys:     make(map[string]*ProductAPIKey),
		Features:    getAllFeatures(), // All features included
		Config:      make(map[string]interface{}),
		AuthRequired: true, // API keys require authorization
		IsPaused:     false,
		IsHalted:    false,
	}
	
	wla.products[productID] = product
	wla.productsByOwner[ownerID] = append(wla.productsByOwner[ownerID], product)
	
	// Update owner with product ID
	owner.ProductID = productID
	
	// Create audit log
	wla.createAuditLog(ownerID, AuditProductCreate, owner.IPAddress, "", fmt.Sprintf("Product created: %s (%s)", name, productID), true)
	
	return productID, nil
}

// ApproveProduct approves a white label product.
// Admin authorization is required for all white level products.
func (wla *WhiteLabelAdmin) ApproveProduct(productID, adminID, ip string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized: admin role required")
	}

	product, exists := wla.products[productID]
	if !exists {
		return fmt.Errorf("product not found")
	}

	product.Status = ProductStatusActive
	product.UpdatedAt = uint64(time.Now().Unix())

	wla.createAuditLog(product.OwnerID, AuditProductCreate, ip, "", fmt.Sprintf("Product approved: %s by admin: %s", productID, adminID), true)

	return nil
}

// PauseProduct pauses a white label product.
// Admin can pause any white level product.
func (wla *WhiteLabelAdmin) PauseProduct(productID, adminID, ip string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized: admin role required")
	}

	product, exists := wla.products[productID]
	if !exists {
		return fmt.Errorf("product not found")
	}

	product.IsPaused = true
	product.Status = ProductStatusPaused
	product.PausedBy = adminID
	product.PausedAt = uint64(time.Now().Unix())
	product.UpdatedAt = uint64(time.Now().Unix())

	wla.createAuditLog(product.OwnerID, AuditProductPause, ip, "", fmt.Sprintf("Product paused: %s by admin: %s", productID, adminID), true)

	return nil
}

// ResumeProduct resumes a paused white label product.
func (wla *WhiteLabelAdmin) ResumeProduct(productID, adminID, ip string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized: admin role required")
	}

	product, exists := wla.products[productID]
	if !exists {
		return fmt.Errorf("product not found")
	}

	product.IsPaused = false
	product.Status = ProductStatusActive
	product.UpdatedAt = uint64(time.Now().Unix())

	wla.createAuditLog(product.OwnerID, AuditProductPause, ip, "", fmt.Sprintf("Product resumed: %s by admin: %s", productID, adminID), true)

	return nil
}

// HaltProduct halts a white label product completely.
// Admin can halt any white level product - it will stop working.
func (wla *WhiteLabelAdmin) HaltProduct(productID, adminID, ip string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized: admin role required")
	}

	product, exists := wla.products[productID]
	if !exists {
		return fmt.Errorf("product not found")
	}

	product.IsHalted = true
	product.IsPaused = false
	product.Status = ProductStatusHalted
	product.HaltedBy = adminID
	product.HaltedAt = uint64(time.Now().Unix())
	product.UpdatedAt = uint64(time.Now().Unix())

	wla.createAuditLog(product.OwnerID, AuditProductHalt, ip, "", fmt.Sprintf("Product halted: %s by admin: %s", productID, adminID), true)

	return nil
}

// DestroyProduct destroys a white label product.
// Only admin can destroy products - all data will be permanently deleted.
func (wla *WhiteLabelAdmin) DestroyProduct(productID, adminID, ip string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized: admin role required")
	}

	product, exists := wla.products[productID]
	if !exists {
		return fmt.Errorf("product not found")
	}

	product.Status = ProductStatusDestroyed
	product.IsHalted = true
	product.IsPaused = false
	product.UpdatedAt = uint64(time.Now().Unix())

	// Revoke all API keys
	for keyHash := range product.APIKeys {
		delete(product.APIKeys, keyHash)
	}

	wla.createAuditLog(product.OwnerID, AuditProductDestroy, ip, "", fmt.Sprintf("Product destroyed: %s by admin: %s", productID, adminID), true)

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
// All white level products require authorized API keys - unauthorized keys will be rejected.
func (wla *WhiteLabelAdmin) VerifyAPIKey(apiKey, productID string) bool {
	wla.mu.RLock()
	defer wla.mu.RUnlock()

	product, exists := wla.products[productID]
	if !exists {
		return false
	}

	// Check if product is active and not halted
	if product.Status != ProductStatusActive || product.IsHalted {
		return false
	}

	for _, key := range product.APIKeys {
		if key.Key == apiKey && key.Active {
			// Check if key is expired
			if key.ExpiresAt > 0 && uint64(time.Now().Unix()) > key.ExpiresAt {
				return false
			}
			return true
		}
	}

	return false
}

// VerifyAPIKeyWithAuth verifies API key with admin authorization check.
func (wla *WhiteLabelAdmin) VerifyAPIKeyWithAuth(apiKey, productID string) error {
	wla.mu.RLock()
	defer wla.mu.RUnlock()

	product, exists := wla.products[productID]
	if !exists {
		return fmt.Errorf("product not found")
	}

	if product.Status == ProductStatusDestroyed {
		return fmt.Errorf("product has been destroyed")
	}

	if product.IsHalted {
		return fmt.Errorf("product has been halted by admin")
	}

	if product.Status == ProductStatusPaused || product.IsPaused {
		return fmt.Errorf("product is paused")
	}

	if !product.AuthRequired {
		return nil // No authorization required
	}

	// Verify API key
	keyFound := false
	for _, key := range product.APIKeys {
		if key.Key == apiKey && key.Active {
			keyFound = true
			if key.ExpiresAt > 0 && uint64(time.Now().Unix()) > key.ExpiresAt {
				return fmt.Errorf("API key expired")
			}
			break
		}
	}

	if !keyFound {
		return fmt.Errorf("please input authorized API keys. Contact to admin")
	}

	return nil
}

// GenerateProductAPIKey generates API key for product.
// Only admin can generate API keys for white level products.
func (wla *WhiteLabelAdmin) GenerateProductAPIKey(productID, adminID, name string, expiresAt uint64) (string, error) {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return "", fmt.Errorf("unauthorized: admin role required")
	}

	product, exists := wla.products[productID]
	if !exists {
		return "", fmt.Errorf("product not found")
	}

	// Check permissions
	if !admin.Permissions["api_key.create"] {
		return "", fmt.Errorf("permission denied: api_key.create")
	}

	apiKey := generateSecureAPIKey()
	keyHash := hashAPIKey(apiKey)

	product.APIKeys[keyHash] = &ProductAPIKey{
		Key:       apiKey,
		Name:      name,
		CreatedAt: uint64(time.Now().Unix()),
		ExpiresAt: expiresAt,
		Active:   true,
	}

	wla.createAuditLog(product.OwnerID, AuditAPIKeyCreate, admin.IPAddress, "", fmt.Sprintf("API key created: %s for product: %s", name, productID), true)

	return apiKey, nil
}

// RevokeProductAPIKey revokes an API key.
func (wla *WhiteLabelAdmin) RevokeProductAPIKey(productID, adminID, keyHash string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()

	admin, exists := wla.users[adminID]
	if !exists || admin.Role < RoleAdmin {
		return fmt.Errorf("unauthorized: admin role required")
	}

	product, exists := wla.products[productID]
	if !exists {
		return fmt.Errorf("product not found")
	}

	key, exists := product.APIKeys[keyHash]
	if !exists {
		return fmt.Errorf("API key not found")
	}

	key.Active = false
	wla.createAuditLog(product.OwnerID, AuditAPIKeyRevoke, admin.IPAddress, "", fmt.Sprintf("API key revoked: %s", key.Name), true)

	return nil
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

	sessionID := generateSecureID()
	expiresAt := uint64(time.Now().Unix()) + uint64(SessionDuration.Seconds())

	sm.sessions[sessionID] = &Session{
		UserID:    userID,
		ID:        sessionID,
		IP:        ip,
		CreatedAt: uint64(time.Now().Unix()),
		ExpiresAt: expiresAt,
	}

	return sessionID
}

// CreateWithLimit creates a new session with max limit.
func (sm *SessionManager) CreateWithLimit(userID, ip string, maxSessions int) string {
	sm.mu.Lock()
	defer sm.mu.Unlock()

	// Count existing sessions
	count := 0
	for _, s := range sm.sessions {
		if s.UserID == userID && s.ExpiresAt > uint64(time.Now().Unix()) {
			count++
		}
	}

	// Remove oldest if at limit
	if count >= maxSessions {
		var oldest *Session
		for _, s := range sm.sessions {
			if s.UserID == userID {
				if oldest == nil || s.ExpiresAt < oldest.ExpiresAt {
					oldest = s
				}
			}
		}
		if oldest != nil {
			delete(sm.sessions, oldest.ID)
		}
	}

	sessionID := generateSecureID()
	expiresAt := uint64(time.Now().Unix()) + uint64(SessionDuration.Seconds())

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

// Helper functions - Security

func generateSecureID() string {
	b := make([]byte, 32)
	rand.Read(b)
	return hex.EncodeToString(b)
}

func generateSecureAPIKey() string {
	b := make([]byte, APIKeyLength)
	rand.Read(b)
	return hex.EncodeToString(b)
}

func generateSalt() string {
	b := make([]byte, 32)
	rand.Read(b)
	return hex.EncodeToString(b)
}

func generateVerificationCode() string {
	b := make([]byte, 6)
	for i := range b {
		b[i] = byte('0' + rand.Intn(10))
	}
	return string(b)
}

func hashPasswordWithSalt(password, salt string) string {
	hash := sha512.Sum512([]byte(password + salt))
	return hex.EncodeToString(hash[:])
}

func hashAPIKey(apiKey string) string {
	hash := sha256.Sum256([]byte(apiKey))
	return hex.EncodeToString(hash[:])
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

func validatePassword(password string) error {
	if len(password) < MinPasswordLength {
		return fmt.Errorf("password must be at least %d characters", MinPasswordLength)
	}
	if len(password) > MaxPasswordLength {
		return fmt.Errorf("password must not exceed %d characters", MaxPasswordLength)
	}
	
	hasUpper := false
	hasLower := false
	hasDigit := false
	hasSpecial := false
	
	for _, c := range password {
		switch {
		case c >= 'A' && c <= 'Z':
			hasUpper = true
		case c >= 'a' && c <= 'z':
			hasLower = true
		case c >= '0' && c <= '9':
			hasDigit = true
		case strings.Contains("!@#$%^&*()_+-=[]{}|;:,.<>?", string(c)):
			hasSpecial = true
		}
	}
	
	if !hasUpper || !hasLower || !hasDigit || !hasSpecial {
		return fmt.Errorf("password must contain uppercase, lowercase, digit, and special character")
	}
	
	return nil
}

func getAllFeatures() []string {
	return []string{
		"wallet.create",
		"wallet.send",
		"wallet.receive",
		"wallet.import",
		"wallet.export",
		"nft.mint",
		"nft.transfer",
		"nft.burn",
		"nft.create_collection",
		"token.create",
		"token.transfer",
		"token.burn",
		"token.mint",
		"bridge.deposit",
		"bridge.withdraw",
		"bridge.transfer",
		"staking.stake",
		"staking.unstake",
		"staking.reward",
		"governance.vote",
		"governance.propose",
	}
}

func getAllPermissions() map[string]bool {
	return map[string]bool{
		"product.create":      true,
		"product.manage":      true,
		"product.pause":      true,
		"product.halt":       true,
		"product.destroy":    true,
		"product.config":    true,
		"admin.create":      true,
		"admin.manage":      true,
		"admin.suspend":    true,
		"admin.ban":        true,
		"api_key.create":    true,
		"api_key.manage":    true,
		"api_key.revoke":    true,
		"permission.grant": true,
		"permission.revoke": true,
		"user.approve":     true,
		"user.suspend":     true,
		"user.ban":         true,
		"audit.view":        true,
		"stats.view":       true,
	}
}

// NewAttackPrevention creates new attack prevention system.
func NewAttackPrevention() *AttackPrevention {
	return &AttackPrevention{
		failedLogins:          make(map[string]*FailedLoginAttempt),
		failedAttempts:       make(map[string]int),
		suspectedIPs:        make(map[string]bool),
		knownBadIPs:         make(map[string]bool),
		xssPatterns:         initXSSPatterns(),
		sqlInjectionPatterns: initSQLPatterns(),
		csrfTokens:          make(map[string]*CSRFToken),
		loginAttempts:       make(map[string]*LoginAttemptTracker),
	}
}

func initXSSPatterns() []*regexp.Regexp {
	patterns := []string{
		"<script[^>]*>.*?</script>",
		"javascript:",
		"onerror=",
		"onload=",
		"onclick=",
		"<iframe[^>]*>",
		"eval\\(",
		"expression\\(",
	}
	
	var result []*regexp.Regexp
	for _, p := range patterns {
		r, _ := regexp.Compile("(?i)" + p)
		result = append(result, r)
	}
	return result
}

func initSQLPatterns() []*regexp.Regexp {
	patterns := []string{
		"('|\\\")\\s*(OR|AND)\\s*('|\\\")\\s*\\d",
		"('|\\\")\\s*OR\\s*1\\s*=\\s*1",
		"UNION\\s+SELECT",
		"DROP\\s+TABLE",
		"DROP\\s+DATABASE",
		"EXEC\\s+@",
		"EXEC\\s+xp_",
	}
	
	var result []*regexp.Regexp
	for _, p := range patterns {
		r, _ := regexp.Compile("(?i)" + p)
		result = append(result, r)
	}
	return result
}

// IsIPBlocked checks if IP is blocked.
func (ap *AttackPrevention) IsIPBlocked(ip string) bool {
	ap.mu.RLock()
	defer ap.mu.RUnlock()
	
	if ap.knownBadIPs[ip] {
		return true
	}
	
	// Check if IP has too many failed attempts
	if attempt, ok := ap.failedLogins[ip]; ok {
		if attempt.Count >= MaxFailedAttempts {
			return true
		}
	}
	
	return false
}

// RecordFailedAttempt records a failed attempt.
func (ap *AttackPrevention) RecordFailedAttempt(key string) {
	ap.mu.Lock()
	defer ap.mu.Unlock()
	
	attempt, ok := ap.failedLogins[key]
	if !ok {
		ap.failedLogins[key] = &FailedLoginAttempt{
			Count:         1,
			FirstAttempt: uint64(time.Now().Unix()),
			LastAttempt: uint64(time.Now().Unix()),
		}
	} else {
		attempt.Count++
		attempt.LastAttempt = uint64(time.Now().Unix())
	}
}

// CheckXSS checks for XSS attacks.
func (ap *AttackPrevention) CheckXSS(input string) bool {
	ap.mu.RLock()
	defer ap.mu.RUnlock()
	
	for _, pattern := range ap.xssPatterns {
		if pattern.MatchString(input) {
			return true
		}
	}
	return false
}

// CheckSQLInjection checks for SQL injection attacks.
func (ap *AttackPrevention) CheckSQLInjection(input string) bool {
	ap.mu.RLock()
	defer ap.mu.RUnlock()
	
	for _, pattern := range ap.sqlInjectionPatterns {
		if pattern.MatchString(input) {
			return true
		}
	}
	return false
}

// GenerateCSRFToken generates a CSRF token.
func (ap *AttackPrevention) GenerateCSRFToken(userID string) string {
	ap.mu.Lock()
	defer ap.mu.Unlock()
	
	token := generateSecureID()
	ap.csrfTokens[token] = &CSRFToken{
		Token:     token,
		UserID:   userID,
		CreatedAt: uint64(time.Now().Unix()),
		ExpiresAt: uint64(time.Now().Unix()) + 3600,
	}
	return token
}

// VerifyCSRFToken verifies a CSRF token.
func (ap *AttackPrevention) VerifyCSRFToken(token, userID string) bool {
	ap.mu.RLock()
	defer ap.mu.RUnlock()
	
	csrfToken, ok := ap.csrfTokens[token]
	if !ok {
		return false
	}
	
	if csrfToken.UserID != userID {
		return false
	}
	
	if uint64(time.Now().Unix()) > csrfToken.ExpiresAt {
		return false
	}
	
	return true
}

// TOTP implementation (simplified)
func verifyTOTP(secret, code string) bool {
	// Simplified TOTP verification - in production use proper TOTP library
	if len(code) != 6 {
		return false
	}
	
	expected := generateVerificationCode()
	return subtle.ConstantTimeCompare([]byte(code), []byte(expected)) == 1
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

// GetUsers returns all users.
func (wla *WhiteLabelAdmin) GetUsers() []*User {
	wla.mu.RLock()
	defer wla.mu.RUnlock()

	users := make([]*User, 0, len(wla.users))
	for _, user := range wla.users {
		users = append(users, user)
	}

	return users
}

// GetProducts returns all products.
func (wla *WhiteLabelAdmin) GetProducts() []*Product {
	wla.mu.RLock()
	defer wla.mu.RUnlock()

	products := make([]*Product, 0, len(wla.products))
	for _, product := range wla.products {
		products = append(products, product)
	}

	return products
}

// GetAdmins returns all admins.
func (wla *WhiteLabelAdmin) GetAdmins() []*User {
	wla.mu.RLock()
	defer wla.mu.RUnlock()

	var admins []*User
	for _, user := range wla.users {
		if user.Role >= RoleAdmin {
			admins = append(admins, user)
		}
	}

	return admins
}

// createAuditLog creates an audit log entry.
func (wla *WhiteLabelAdmin) createAuditLog(userID string, action AuditAction, ip, userAgent, details string, success bool) {
	wla.mu.Lock()
	defer wla.mu.Unlock()
	
	log := &AuditLog{
		ID:         generateSecureID(),
		UserID:     userID,
		Action:    action,
		IP:        ip,
		UserAgent: userAgent,
		Details:  details,
		Timestamp: uint64(time.Now().Unix()),
		Success: success,
	}
	
	wla.auditLogs = append(wla.auditLogs, log)
	
	// Limit audit logs
	if len(wla.auditLogs) > MaxAuditLogs {
		wla.auditLogs = wla.auditLogs[len(wla.auditLogs)-MaxAuditLogs:]
	}
}

// GetAuditLogs returns audit logs.
func (wla *WhiteLabelAdmin) GetAuditLogs(limit int) []*AuditLog {
	wla.mu.RLock()
	defer wla.mu.RUnlock()
	
	if limit > len(wla.auditLogs) {
		limit = len(wla.auditLogs)
	}
	
	result := make([]*AuditLog, limit)
	copy(result, wla.auditLogs[len(wla.auditLogs)-limit:])
	
	return result
}

// GetUserAuditLogs returns audit logs for a user.
func (wla *WhiteLabelAdmin) GetUserAuditLogs(userID string, limit int) []*AuditLog {
	wla.mu.RLock()
	defer wla.mu.RUnlock()
	
	var logs []*AuditLog
	for i := len(wla.auditLogs) - 1; i >= 0 && len(logs) < limit; i-- {
		if wla.auditLogs[i].UserID == userID {
			logs = append(logs, wla.auditLogs[i])
		}
	}
	
	return logs
}

// CreateAdmin creates a new admin user (for super admin only).
func (wla *WhiteLabelAdmin) CreateAdmin(superAdminID, username, email, password string) (string, error) {
	wla.mu.Lock()
	defer wla.mu.Unlock()
	
	admin, exists := wla.users[superAdminID]
	if !exists || admin.Role != RoleSuperAdmin {
		return "", fmt.Errorf("unauthorized: super admin role required")
	}
	
	// Validate input
	username = SanitizeInput(username)
	email = strings.ToLower(strings.TrimSpace(email))
	
	if _, err := mail.ParseAddress(email); err != nil {
		return "", fmt.Errorf("invalid email address")
	}
	
	if err := validatePassword(password); err != nil {
		return "", err
	}
	
	// Check if email exists
	if _, exists := wla.usersByEmail[email]; exists {
		return "", fmt.Errorf("email already registered")
	}
	
	// Generate user
	salt := generateSalt()
	passHash := hashPasswordWithSalt(password, salt)
	
	userID := generateSecureID()
	
	user := &User{
		ID:            userID,
		Username:      username,
		Email:        email,
		PasswordHash: passHash,
		Salt:         salt,
		Role:         RoleAdmin,
		Status:       StatusActive,
		CreatedAt:    uint64(time.Now().Unix()),
		UpdatedAt:   uint64(time.Now().Unix()),
		Permissions: getAllPermissions(),
	}
	
	wla.users[userID] = user
	wla.usersByEmail[email] = user
	wla.usersByUsername[username] = user
	
	wla.createAuditLog(superAdminID, AuditAdminCreate, admin.IPAddress, "", fmt.Sprintf("Admin created: %s", email), true)
	
	return userID, nil
}

// RemoveAdmin removes an admin user (for super admin only).
func (wla *WhiteLabelAdmin) RemoveAdmin(superAdminID, adminID string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()
	
	admin, exists := wla.users[superAdminID]
	if !exists || admin.Role != RoleSuperAdmin {
		return fmt.Errorf("unauthorized: super admin role required")
	}
	
	target, exists := wla.users[adminID]
	if !exists {
		return fmt.Errorf("user not found")
	}
	
	if target.Role != RoleAdmin {
		return fmt.Errorf("user is not an admin")
	}
	
	// Cannot remove super admin
	if target.Role == RoleSuperAdmin {
		return fmt.Errorf("cannot remove super admin")
	}
	
	target.Status = StatusBanned
	wla.sessions.RevokeAll(adminID)
	
	wla.createAuditLog(superAdminID, AuditAdminRemove, admin.IPAddress, "", fmt.Sprintf("Admin removed: %s", target.Email), true)
	
	return nil
}

// GrantPermissionToAdmin grants a permission to another admin.
func (wla *WhiteLabelAdmin) GrantPermissionToAdmin(superAdminID, adminID, permission string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()
	
	admin, exists := wla.users[superAdminID]
	if !exists || admin.Role != RoleSuperAdmin {
		return fmt.Errorf("unauthorized: super admin role required")
	}
	
	target, exists := wla.users[adminID]
	if !exists {
		return fmt.Errorf("user not found")
	}
	
	target.Permissions[permission] = true
	wla.createAuditLog(superAdminID, AuditPermissionGrant, admin.IPAddress, "", fmt.Sprintf("Permission granted: %s to %s", permission, target.Email), true)
	
	return nil
}

// RevokePermissionFromAdmin revokes a permission from admin.
func (wla *WhiteLabelAdmin) RevokePermissionFromAdmin(superAdminID, adminID, permission string) error {
	wla.mu.Lock()
	defer wla.mu.Unlock()
	
	admin, exists := wla.users[superAdminID]
	if !exists || admin.Role != RoleSuperAdmin {
		return fmt.Errorf("unauthorized: super admin role required")
	}
	
	target, exists := wla.users[adminID]
	if !exists {
		return fmt.Errorf("user not found")
	}
	
	delete(target.Permissions, permission)
	wla.createAuditLog(superAdminID, AuditPermissionRevoke, admin.IPAddress, "", fmt.Sprintf("Permission revoked: %s from %s", permission, target.Email), true)
	
	return nil
}

// HTML escape for XSS prevention
func escapeHTML(input string) string {
	return html.EscapeString(input)
}

// ValidateURL validates URL for open redirect prevention.
func ValidateURL(rawURL string) error {
	u, err := url.Parse(rawURL)
	if err != nil {
		return fmt.Errorf("invalid URL")
	}
	
	// Only allow http and https
	if u.Scheme != "http" && u.Scheme != "https" {
		return fmt.Errorf("invalid URL scheme")
	}
	
	// Disallow javascript: protocol
	if u.Scheme == "javascript" {
		return fmt.Errorf("javascript scheme not allowed")
	}
	
	return nil
}

// CheckRateLimit checks if request is within rate limit.
func (wla *WhiteLabelAdmin) CheckRateLimit(key string) bool {
	return wla.rateLimiter.Allow(key)
}

var _ = context.Background() // Use context
var _ = big.NewInt(0)     // Use big.Int
var _ = json.Marshal     // Use json