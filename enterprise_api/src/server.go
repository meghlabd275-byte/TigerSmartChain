// Package enterprise provides enterprise API with SLA and custom features
package enterprise

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"net/http"
	"sync"
	"time"

	"github.com/golang-jwt/jwt/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

type Config struct {
	DBURL          string
	RedisURL       string
	Port           string
	JWTSecret      string
}

type Subscription struct {
	ID            string    `json:"id"`
	UserID       int       `json:"userId"`
	Plan         string    `json:"plan"` // pro, enterprise, custom
	Status       string    `json:"status"` // active, expired, cancelled
	MonthlyRateLimit int64   `json:"monthlyRateLimit"`
	RequestsUsed int64     `json:"requestsUsed"`
	RequestsRemaining int64 `json:"requestsRemaining"`
	StartedAt    time.Time `json:"startedAt"`
	ExpiresAt   *time.Time `json:"expiresAt"`
}

type APIEndpoint struct {
	Path        string `json:"path"`
	Method     string `json:"method"`
	RateLimit  int    `json:"rateLimit"`
	Timeout    int    `json:"timeout"`
}

type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
}

func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, err
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 16})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, err
	}
	createTables(ctx, pool)
	return &Server{cfg: cfg, pool: pool, redis: rdb}, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) {
	pool.Exec(ctx, `CREATE TABLE IF NOT EXISTS enterprise_subscriptions (id VARCHAR(36) PRIMARY KEY, user_id INTEGER NOT NULL, plan VARCHAR(50) NOT NULL, status VARCHAR(20) DEFAULT 'active', monthly_rate_limit BIGINT DEFAULT 1000000, requests_used BIGINT DEFAULT 0, started_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, expires_at TIMESTAMP)`)
	pool.Exec(ctx, `CREATE TABLE IF NOT EXISTS enterprise_api_keys (id VARCHAR(36) PRIMARY KEY, subscription_id VARCHAR(36) NOT NULL, key_value VARCHAR(64) NOT NULL UNIQUE, name VARCHAR(255), rate_limit INTEGER, is_active BOOLEAN DEFAULT TRUE, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`)
	pool.Exec(ctx, `CREATE TABLE IF NOT EXISTS enterprise_usage_logs (id SERIAL PRIMARY KEY, subscription_id VARCHAR(36) NOT NULL, endpoint VARCHAR(255), method VARCHAR(10), status_code INTEGER, response_time_ms INTEGER, timestamp BIGINT NOT NULL)`)
}

func (s *Server) Start() error {
	http.HandleFunc("/api/enterprise/v1/subscription", s.handleSubscription)
	http.HandleFunc("/api/enterprise/v1/create-key", s.handleCreateKey)
	http.HandleFunc("/api/enterprise/v1/usage", s.handleUsage)
	http.HandleFunc("/api/enterprise/v1/limits", s.handleLimits)
	return http.ListenAndServe(":"+s.cfg.Port, nil)
}

func (s *Server) handleSubscription(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	
	if r.Method == "POST" {
		var sub struct {
			UserID int `json:"userId"`
			Plan   string `json:"plan"`
		}
		json.NewDecoder(r.Body).Decode(&sub)
		
		subID := fmt.Sprintf("sub_%d_%d", sub.UserID, time.Now().Unix())
		
		rateLimit := int64(100000)
		if sub.Plan == "enterprise" {
			rateLimit = 10000000
		} else if sub.Plan == "custom" {
			rateLimit = 100000000
		}
		
		_, err := s.pool.Exec(ctx, "INSERT INTO enterprise_subscriptions (id, user_id, plan, monthly_rate_limit) VALUES ($1, $2, $3, $4)", subID, sub.UserID, sub.Plan, rateLimit)
		if err != nil {
			http.Error(w, err.Error(), 500)
			return
		}
		
		s.redis.Set(ctx, fmt.Sprintf("enterprise:sub:%s", subID), fmt.Sprintf("%d", rateLimit), 30*24*time.Hour)
		
		json.NewEncoder(w).Encode(Subscription{ID: subID, UserID: sub.UserID, Plan: sub.Plan, Status: "active", MonthlyRateLimit: rateLimit, StartedAt: time.Now()})
		return
	}
	
	userID := r.URL.Query().Get("userId")
	
	rows, err := s.pool.Query(ctx, "SELECT id, user_id, plan, status, monthly_rate_limit, requests_used, started_at, expires_at FROM enterprise_subscriptions WHERE user_id = $1", userID)
	if err != nil {
		http.Error(w, err.Error(), 500)
		return
	}
	defer rows.Close()
	
	var subs []Subscription
	for rows.Next() {
		var sub Subscription
		rows.Scan(&sub.ID, &sub.UserID, &sub.Plan, &sub.Status, &sub.MonthlyRateLimit, &sub.RequestsUsed, &sub.StartedAt, &sub.ExpiresAt)
		sub.RequestsRemaining = sub.MonthlyRateLimit - sub.RequestsUsed
		subs = append(subs, sub)
	}
	
	json.NewEncoder(w).Encode(subs)
}

func (s *Server) handleCreateKey(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	
	var req struct {
		SubscriptionID string `json:"subscriptionId"`
		Name          string `json:"name"`
		RateLimit     int    `json:"rateLimit"`
	}
	json.NewDecoder(r.Body).Decode(&req)
	
	keyID := fmt.Sprintf("key_%d", time.Now().UnixNano())
	keyValue := generateSecureKey(32)
	
	_, err := s.pool.Exec(ctx, "INSERT INTO enterprise_api_keys (id, subscription_id, key_value, name, rate_limit) VALUES ($1, $2, $3, $4, $5)", keyID, req.SubscriptionID, keyValue, req.Name, req.RateLimit)
	if err != nil {
		http.Error(w, err.Error(), 500)
		return
	}
	
	json.NewEncoder(w).Encode(map[string]string{"id": keyID, "key": keyValue})
}

func (s *Server) handleUsage(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	subID := r.URL.Query().Get("subscriptionId")
	
	type Usage struct {
		Endpoint     string `json:"endpoint"`
		Requests    int    `json:"requests"`
		AvgResponseTime float64 `json:"avgResponseTime"`
	}
	
	rows, err := s.pool.Query(ctx, "SELECT endpoint, COUNT(*), AVG(response_time_ms) FROM enterprise_usage_logs WHERE subscription_id = $1 AND timestamp > $2 GROUP BY endpoint", subId, time.Now().Unix()-86400)
	if err != nil {
		http.Error(w, err.Error(), 500)
		return
	}
	defer rows.Close()
	
	var usages []Usage
	for rows.Next() {
		var u Usage
		rows.Scan(&u.Endpoint, &u.Requests, &u.AvgResponseTime)
		usages = append(usages, u)
	}
	
	json.NewEncoder(w).Encode(usages)
}

func (s *Server) handleLimits(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	subID := r.URL.Query().Get("subscriptionId")
	
	var sub Subscription
	err := s.pool.QueryRow(ctx, "SELECT id, monthly_rate_limit, requests_used FROM enterprise_subscriptions WHERE id = $1", subID).Scan(&sub.ID, &sub.MonthlyRateLimit, &sub.RequestsUsed)
	if err != nil {
		http.Error(w, "subscription not found", 404)
		return
	}
	
	sub.RequestsRemaining = sub.MonthlyRateLimit - sub.RequestsUsed
	
	json.NewEncoder(w).Encode(sub)
}

func generateSecureKey(length int) string {
	// Use crypto/rand for secure random generation
	bytes := make([]byte, length)
	if _, err := rand.Read(bytes); err != nil {
		// Fallback to less secure but deterministic
		chars := "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
		for i := range bytes {
			bytes[i] = chars[i%len(chars)]
		}
		return string(bytes)
	}
	
	// Use hex encoding for safe string representation
	key := make([]byte, length*2)
	for i, b := range bytes {
		key[i*2] = chars[b%byte(len(chars))]
		key[i*2+1] = '0'
	}
	
	return string(key[:length])
}

// hashKey hashes an API key for secure storage
func hashKey(key string) string {
	hash := sha256.Sum256([]byte(key))
	return hex.EncodeToString(hash[:])
}

// =============================================================================
// JWT AUTHENTICATION
// =============================================================================

// Claims represents JWT claims
type Claims struct {
	UserID    int    `json:"userId"`
	Plan     string `json:"plan"`
	APIKeyID string `json:"apiKeyId"`
	jwt.RegisteredClaims
}

// generateToken generates a JWT token
func (s *Server) generateToken(userID int, plan, apiKeyID string, expiry time.Duration) (string, error) {
	claims := Claims{
		UserID:    userID,
		Plan:     plan,
		APIKeyID: apiKeyID,
		RegisteredClaims: jwt.RegisteredClaims{
			ExpiresAt: jwt.NewNumericDate(time.Now().Add(expiry)),
			IssuedAt:  jwt.NewNumericTime(time.Now()),
		},
	}
	
	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	return token.SignedString([]byte(s.cfg.JWTSecret))
}

// validateToken validates a JWT token
func (s *Server) validateToken(tokenString string) (*Claims, error) {
	token, err := jwt.ParseWithClaims(tokenString, &Claims{}, func(token *jwt.Token) (interface{}, error) {
		return []byte(s.cfg.JWTSecret), nil
	})
	
	if err != nil {
		return nil, err
	}
	
	if claims, ok := token.Claims.(*Claims); ok && token.Valid {
		return claims, nil
	}
	
	return nil, fmt.Errorf("invalid token")
}

// =============================================================================
// API KEY MIDDLEWARE
// =============================================================================

// APIKeyMiddleware validates API keys
func (s *Server) APIKeyMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ctx := r.Context()
		
		// Get API key from header
		apiKey := r.Header.Get("X-API-Key")
		if apiKey == "" {
			apiKey = r.URL.Query().Get("apikey")
		}
		
		if apiKey == "" {
			http.Error(w, "API key required", 401)
			return
		}
		
		// Hash the key for lookup
		keyHash := hashKey(apiKey)
		
		// Check if key exists and is active
		var keyInfo struct {
			ID             string
			SubscriptionID string
			RateLimit    int
			IsActive    bool
		}
		
		err := s.pool.QueryRow(ctx, 
			"SELECT id, subscription_id, rate_limit, is_active FROM enterprise_api_keys WHERE key_value = $1", keyHash,
		).Scan(&keyInfo.ID, &keyInfo.SubscriptionID, &keyInfo.RateLimit, &keyInfo.IsActive)
		
		if err != nil || !keyInfo.IsActive {
			http.Error(w, "Invalid or inactive API key", 401)
			return
		}
		
		// Check rate limit
		allowed, err := s.CheckRateLimit(ctx, keyInfo.SubscriptionID, keyInfo.RateLimit)
		if err != nil || !allowed {
			http.Error(w, "Rate limit exceeded", 429)
			return
		}
		
		// Add to context
		ctx = context.WithValue(ctx, "subscriptionId", keyInfo.SubscriptionID)
		ctx = context.WithValue(ctx, "apiKeyId", keyInfo.ID)
		
		next.ServeHTTP(w, r.WithContext(ctx))
	})
}

// =============================================================================
// BATCH API ENDPOINTS
// =============================================================================

// BatchRequest represents a batch API request
type BatchRequest struct {
	Requests []BatchItem `json:"requests"`
}

// BatchItem represents a single request in a batch
type BatchItem struct {
	ID      string      `json:"id"`
	Method  string      `json:"method"`
	Params interface{} `json:"params"`
}

// BatchResponse represents a batch API response
type BatchResponse struct {
	Results []BatchResult `json:"results"`
}

// BatchResult represents a single result in a batch
type BatchResult struct {
	ID     string      `json:"id"`
	Result interface{} `json:"result,omitempty"`
	Error  string      `json:"error,omitempty"`
}

// handleBatch handles batch API requests
func (s *Server) handleBatch(w http.ResponseWriter, r *http.Request) {
	if r.Method != "POST" {
		http.Error(w, "Method not allowed", 405)
		return
	}
	
	var req BatchRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), 400)
		return
	}
	
	// Limit batch size
	if len(req.Requests) > 100 {
		http.Error(w, "Batch size exceeds limit of 100", 400)
		return
	}
	
	results := make([]BatchResult, len(req.Requests))
	
	// Process in parallel
	var wg sync.WaitGroup
	for i, item := range req.Requests {
		wg.Add(1)
		go func(i int, item BatchItem) {
			defer wg.Done()
			
			result, err := s.executeBatchItem(item)
			results[i] = BatchResult{
				ID: item.ID,
				Result: result,
				Error:  err,
			}
		}(i, item)
	}
	
	wg.Wait()
	
	json.NewEncoder(w).Encode(BatchResponse{Results: results})
}

// executeBatchItem executes a single batch item
func (s *Server) executeBatchItem(item BatchItem) (interface{}, error) {
	// Route to appropriate handler based on method
	switch item.Method {
	case "eth_blockNumber":
		return s.getBlockNumber(), nil
	case "eth_getBlockByNumber":
		return nil, nil // Would call RPC
	case "eth_getTransactionReceipt":
		return nil, nil // Would call RPC
	default:
		return nil, fmt.Errorf("unknown method: %s", item.Method)
	}
}

func (s *Server) getBlockNumber() string {
	// Would call RPC
	return "0x0"
}

func (s *Server) LogUsage(ctx context.Context, subID, endpoint, method string, statusCode, responseTime int) error {
	_, err := s.pool.Exec(ctx, "INSERT INTO enterprise_usage_logs (subscription_id, endpoint, method, status_code, response_time_ms, timestamp) VALUES ($1, $2, $3, $4, $5, $6)", subID, endpoint, method, statusCode, responseTime, time.Now().Unix())
	if err != nil {
		return err
	}
	
	// Update usage count
	_, err = s.pool.Exec(ctx, "UPDATE enterprise_subscriptions SET requests_used = requests_used + 1 WHERE id = $1", subID)
	return err
}

func (s *Server) CheckRateLimit(ctx context.Context, subID string) (bool, error) {
	key := fmt.Sprintf("enterprise:ratelimit:%s", subID)
	
	count, err := s.redis.Get(ctx, key).Int()
	if err == redis.Nil {
		s.redis.Set(ctx, key, "1", time.Minute)
		return true, nil
	}
	if err != nil {
		return false, err
	}
	
	var limit int64
	s.pool.QueryRow(ctx, "SELECT monthly_rate_limit FROM enterprise_subscriptions WHERE id = $1", subID).Scan(&limit)
	
	return int64(count) < limit, nil
}