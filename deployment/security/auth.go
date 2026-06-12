// API Authentication Module for TigerScan
// Advanced authentication with JWT, OAuth2, and API keys

package security

import (
	"context"
	"crypto/rand"
	"crypto/rsa"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"math/big"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/golang-jwt/jwt/v5"
	"github.com/redis/go-redis/v9"
)

// =============================================================================
// CONFIGURATION
// =============================================================================

// AuthConfig holds authentication configuration
type AuthConfig struct {
	RedisClient       *redis.Client
	JWTKey           []byte
	JWTIssuer        string
	JWTAccessExpiry  time.Duration
	JWTRefreshExpiry time.Duration
	OAuth2Enabled    bool
	APIKeyEnabled    bool
	BasicAuthEnabled bool
	KeyPrefix        string
}

// NewAuthConfig creates default auth configuration
func NewAuthConfig(redisClient *redis.Client) *AuthConfig {
	key := make([]byte, 32)
	rand.Read(key)
	
	return &AuthConfig{
		RedisClient:       redisClient,
		JWTKey:            key,
		JWTIssuer:         "tigerscan",
		JWTAccessExpiry:   15 * time.Minute,
		JWTRefreshExpiry:  7 * 24 * time.Hour,
		OAuth2Enabled:    true,
		APIKeyEnabled:    true,
		BasicAuthEnabled: true,
		KeyPrefix:        "tigerscan:auth:",
	}
}

// =============================================================================
// TYPES
// =============================================================================

// Claims represents JWT claims
type Claims struct {
	jwt.RegisteredClaims
	UserID    string   `json:"user_id"`
	Email     string   `json:"email"`
	Tier      string   `json:"tier"`
	Scopes    []string `json:"scopes"`
	APIKeyID  string   `json:"api_key_id"`
}

// TokenPair represents access and refresh token pair
type TokenPair struct {
	AccessToken  string `json:"access_token"`
	RefreshToken string `json:"refresh_token"`
	ExpiresIn   int64  `json:"expires_in"`
	TokenType   string `json:"token_type"`
}

// User represents a user
type User struct {
	ID           string    `json:"id"`
	Email        string    `json:"email"`
	PasswordHash string    `json:"password_hash"`
	Tier        string    `json:"tier"`
	Scopes       []string `json:"scopes"`
	CreatedAt    time.Time `json:"created_at"`
	UpdatedAt    time.Time `json:"updated_at"`
	LastLogin    time.Time `json:"last_login"`
}

// =============================================================================
// AUTHENTICATOR
// =============================================================================

// Authenticator provides authentication functionality
type Authenticator struct {
	config *AuthConfig
	key    *rsa.PrivateKey
}

// NewAuthenticator creates a new authenticator
func NewAuthenticator(config *AuthConfig) (*Authenticator, error) {
	// Generate RSA key pair for JWT signing
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		return nil, err
	}
	
	return &Authenticator{
		config: config,
		key:    key,
	}, nil
}

// =============================================================================
// JWT AUTHENTICATION
// =============================================================================

// GenerateTokenPair generates access and refresh tokens
func (a *Authenticator) GenerateTokenPair(user *User) (*TokenPair, error) {
	// Access token
	accessClaims := &Claims{
		RegisteredClaims: jwt.RegisteredClaims{
			ExpiresAt: jwt.NewNumericDate(time.Now().Add(a.config.JWTAccessExpiry)),
			IssuedAt:  jwt.NewNumericDate(time.Now()),
			NotBefore: jwt.NewNumericDate(time.Now()),
			Issuer:    a.config.JWTIssuer,
			Subject:   user.ID,
			ID:        generateRandomID(),
		},
		UserID:   user.ID,
		Email:    user.Email,
		Tier:     user.Tier,
		Scopes:   user.Scopes,
	}
	
	accessToken := jwt.NewWithClaims(jwt.SigningMethodRS256, accessClaims)
	accessTokenString, err := accessToken.SignedString(a.key)
	if err != nil {
		return nil, err
	}
	
	// Refresh token
	refreshClaims := &Claims{
		RegisteredClaims: jwt.RegisteredClaims{
			ExpiresAt: jwt.NewNumericDate(time.Now().Add(a.config.JWTRefreshExpiry)),
			IssuedAt:  jwt.NewNumericDate(time.Now()),
			Issuer:    a.config.JWTIssuer,
			Subject:   user.ID,
			ID:        generateRandomID(),
		},
		UserID: user.ID,
		Email:  user.Email,
		Tier:   user.Tier,
	}
	
	refreshToken := jwt.NewWithClaims(jwt.SigningMethodRS256, refreshClaims)
	refreshTokenString, err := refreshToken.SignedString(a.key)
	if err != nil {
		return nil, err
	}
	
	// Store refresh token in Redis
	if a.config.RedisClient != nil {
		ctx := context.Background()
		refreshKey := a.config.KeyPrefix + "refresh:" + user.ID
		a.config.RedisClient.Set(ctx, refreshKey, refreshTokenString, a.config.JWTRefreshExpiry)
	}
	
	return &TokenPair{
		AccessToken:  accessTokenString,
		RefreshToken: refreshTokenString,
		ExpiresIn:   int64(a.config.JWTAccessExpiry.Seconds()),
		TokenType:   "Bearer",
	}, nil
}

// ValidateToken validates a JWT token
func (a *Authenticator) ValidateToken(tokenString string) (*Claims, error) {
	token, err := jwt.ParseWithClaims(tokenString, &Claims{}, func(token *jwt.Token) (interface{}, error) {
		if _, ok := token.Method.(*jwt.SigningMethodRSA); !ok {
			return nil, fmt.Errorf("unexpected signing method: %v", token.Header["alg"])
		}
		return &a.key.PublicKey, nil
	})
	
	if err != nil {
		return nil, err
	}
	
	if claims, ok := token.Claims.(*Claims); ok && token.Valid {
		// Check if token is blacklisted
		if a.config.RedisClient != nil {
			ctx := context.Background()
			blacklistKey := a.config.KeyPrefix + "blacklist:" + claims.ID
			exists, _ := a.config.RedisClient.Exists(ctx, blacklistKey).Result()
			if exists == 1 {
				return nil, errors.New("token is blacklisted")
			}
		}
		return claims, nil
	}
	
	return nil, errors.New("invalid token")
}

// RefreshTokens refreshes access and refresh tokens
func (a *Authenticator) RefreshTokens(refreshToken string) (*TokenPair, error) {
	// Validate refresh token
	claims, err := a.ValidateToken(refreshToken)
	if err != nil {
		return nil, err
	}
	
	// Get user
	user, err := a.GetUser(claims.UserID)
	if err != nil {
		return nil, err
	}
	
	// Blacklist old refresh token
	if a.config.RedisClient != nil {
		ctx := context.Background()
		refreshKey := a.config.KeyPrefix + "refresh:" + user.ID
		a.config.RedisClient.Del(ctx, refreshKey)
	}
	
	// Generate new tokens
	return a.GenerateTokenPair(user)
}

// Logout invalidates tokens
func (a *Authenticator) Logout(ctx context.Context, userID, tokenID string) error {
	if a.config.RedisClient != nil {
		// Blacklist access token
		accessKey := a.config.KeyPrefix + "blacklist:" + tokenID
		a.config.RedisClient.Set(ctx, accessKey, "1", a.config.JWTAccessExpiry)
		
		// Remove refresh token
		refreshKey := a.config.KeyPrefix + "refresh:" + userID
		a.config.RedisClient.Del(ctx, refreshKey)
	}
	
	return nil
}

// =============================================================================
// USER MANAGEMENT
// =============================================================================

// CreateUser creates a new user
func (a *Authenticator) CreateUser(ctx context.Context, email, password, tier string, scopes []string) (*User, error) {
	// Hash password
	passwordHash := HashPassword(password, nil)
	
	user := &User{
		ID:           generateRandomID(),
		Email:        email,
		PasswordHash: passwordHash,
		Tier:         tier,
		Scopes:       scopes,
		CreatedAt:    time.Now(),
		UpdatedAt:    time.Now(),
	}
	
	// Store in Redis
	userJSON, err := json.Marshal(user)
	if err != nil {
		return nil, err
	}
	
	key := a.config.KeyPrefix + "user:" + user.ID
	if a.config.RedisClient != nil {
		a.config.RedisClient.Set(ctx, key, string(userJSON), 0)
		
		// Also store by email
		emailKey := a.config.KeyPrefix + "email:" + email
		a.config.RedisClient.Set(ctx, emailKey, user.ID, 0)
	}
	
	return user, nil
}

// GetUser gets a user by ID
func (a *Authenticator) GetUser(userID string) (*User, error) {
	if a.config.RedisClient == nil {
		return nil, errors.New("redis not configured")
	}
	
	ctx := context.Background()
	key := a.config.KeyPrefix + "user:" + userID
	
	userJSON, err := a.config.RedisClient.Get(ctx, key).Result()
	if err != nil {
		return nil, err
	}
	
	var user User
	if err := json.Unmarshal([]byte(userJSON), &user); err != nil {
		return nil, err
	}
	
	return &user, nil
}

// GetUserByEmail gets a user by email
func (a *Authenticator) GetUserByEmail(email string) (*User, error) {
	if a.config.RedisClient == nil {
		return nil, errors.New("redis not configured")
	}
	
	ctx := context.Background()
	emailKey := a.config.KeyPrefix + "email:" + email
	
	userID, err := a.config.RedisClient.Get(ctx, emailKey).Result()
	if err != nil {
		return nil, err
	}
	
	return a.GetUser(userID)
}

// ValidatePassword validates user password
func (a *Authenticator) ValidatePassword(userID, password string) (bool, error) {
	user, err := a.GetUser(userID)
	if err != nil {
		return false, err
	}
	
	return VerifyPassword(password, user.PasswordHash), nil
}

// =============================================================================
// API KEY AUTHENTICATION
// =============================================================================

// CreateAPIKey creates a new API key
func (a *Authenticator) CreateAPIKey(ctx context.Context, userID, name, tier string, scopes []string) (string, error) {
	// Generate random key
	keyBytes := make([]byte, 32)
	rand.Read(keyBytes)
	key := fmt.Sprintf("tgr_%s", base64.URLEncoding.EncodeToString(keyBytes))
	
	// Hash key for storage
	keyHash := fmt.Sprintf("%x", sha256.Sum256(keyBytes))
	
	// Store key
	keyData := map[string]interface{}{
		"id":       generateRandomID(),
		"key":      keyHash,
		"name":     name,
		"user_id":  userID,
		"tier":     tier,
		"scopes":   scopes,
		"created":  time.Now().Unix(),
		"expires":  time.Now().Add(365 * 24 * time.Hour).Unix(),
	}
	
	keyJSON, _ := json.Marshal(keyData)
	keyRedisKey := a.config.KeyPrefix + "apikey:" + keyHash
	if a.config.RedisClient != nil {
		a.config.RedisClient.Set(ctx, keyRedisKey, string(keyJSON), 365*24*time.Hour)
		
		// Also store user reference
		userKey := a.config.KeyPrefix + "userapikey:" + userID + ":" + keyHash
		a.config.RedisClient.Set(ctx, userKey, key, 365*24*time.Hour)
	}
	
	return key, nil
}

// ValidateAPIKey validates an API key
func (a *Authenticator) ValidateAPIKey(ctx context.Context, key string) (*User, error) {
	// Hash key
	keyHash := fmt.Sprintf("%x", sha256.Sum256([]byte(key)))
	
	// Get key data
	keyRedisKey := a.config.KeyPrefix + "apikey:" + keyHash
	keyJSON, err := a.config.RedisClient.Get(ctx, keyRedisKey).Result()
	if err != nil {
		return nil, errors.New("invalid API key")
	}
	
	var keyData map[string]interface{}
	if err := json.Unmarshal([]byte(keyJSON), &keyData); err != nil {
		return nil, err
	}
	
	// Check expiration
	expires := int64(keyData["expires"].(float64))
	if time.Now().Unix() > expires {
		return nil, errors.New("API key expired")
	}
	
	// Get user
	userID := keyData["user_id"].(string)
	return a.GetUser(userID)
}

// RevokeAPIKey revokes an API key
func (a *Authenticator) RevokeAPIKey(ctx context.Context, key string) error {
	keyHash := fmt.Sprintf("%x", sha256.Sum256([]byte(key)))
	
	keyRedisKey := a.config.KeyPrefix + "apikey:" + keyHash
	if a.config.RedisClient != nil {
		a.config.RedisClient.Del(ctx, keyRedisKey)
	}
	
	return nil
}

// =============================================================================
// BASIC AUTHENTICATION
// =============================================================================

// ValidateBasicAuth validates basic authentication
func (a *Authenticator) ValidateBasicAuth(ctx context.Context, email, password string) (*User, error) {
	user, err := a.GetUserByEmail(email)
	if err != nil {
		return nil, err
	}
	
	if !VerifyPassword(password, user.PasswordHash) {
		return nil, errors.New("invalid credentials")
	}
	
	// Update last login
	user.LastLogin = time.Now()
	userJSON, _ := json.Marshal(user)
	key := a.config.KeyPrefix + "user:" + user.ID
	a.config.RedisClient.Set(ctx, key, string(userJSON), 0)
	
	return user, nil
}

// =============================================================================
// GIN MIDDLEWARE
// =============================================================================

// Middleware returns Gin authentication middleware
func (a *Authenticator) Middleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		// Check for API key
		apiKey := c.GetHeader("X-API-Key")
		if apiKey != "" {
			user, err := a.ValidateAPIKey(c.Request.Context(), apiKey)
			if err == nil {
				c.Set("user", user)
				c.Set("auth_type", "api_key")
				c.Next()
				return
			}
		}
		
		// Check for Bearer token
		auth := c.GetHeader("Authorization")
		if strings.HasPrefix(auth, "Bearer ") {
			token := strings.TrimPrefix(auth, "Bearer ")
			claims, err := a.ValidateToken(token)
			if err == nil {
				user, _ := a.GetUser(claims.UserID)
				c.Set("user", user)
				c.Set("claims", claims)
				c.Set("auth_type", "jwt")
				c.Next()
				return
			}
		}
		
		// Check for Basic auth
		if a.config.BasicAuthEnabled && strings.HasPrefix(auth, "Basic ") {
			encoded := strings.TrimPrefix(auth, "Basic ")
			decoded, err := base64.StdEncoding.DecodeString(encoded)
			if err == nil {
				parts := strings.Split(string(decoded), ":")
				if len(parts) == 2 {
					user, err := a.ValidateBasicAuth(c.Request.Context(), parts[0], parts[1])
					if err == nil {
						c.Set("user", user)
						c.Set("auth_type", "basic")
						c.Next()
						return
					}
				}
			}
		}
		
		// No valid authentication
		c.AbortWithStatusJSON(401, gin.H{
			"status":  "error",
			"message": "Authentication required",
		})
	}
}

// RequireScope returns middleware that requires specific scopes
func (a *Authenticator) RequireScope(scopes ...string) gin.HandlerFunc {
	return func(c *gin.Context) {
		claims, exists := c.Get("claims")
		if !exists {
			c.AbortWithStatusJSON(403, gin.H{
				"status":  "error",
				"message": "Invalid claims",
			})
			return
		}
		
		userClaims := claims.(*Claims)
		
		// Check if user has all required scopes
		for _, required := range scopes {
			found := false
			for _, userScope := range userClaims.Scopes {
				if subtle.ConstantTimeCompare([]byte(userScope), []byte(required)) == 1 {
					found = true
					break
				}
			}
			if !found {
				c.AbortWithStatusJSON(403, gin.H{
					"status":  "error",
					"message": "Insufficient permissions",
					"required": required,
				})
				return
			}
		}
		
		c.Next()
	}
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

func generateRandomID() string {
	bytes := make([]byte, 16)
	rand.Read(bytes)
	return fmt.Sprintf("%x", bytes)
}

// IntToBigInt converts int to big.Int
func IntToBigInt(i int64) *big.Int {
	return big.NewInt(i)
}