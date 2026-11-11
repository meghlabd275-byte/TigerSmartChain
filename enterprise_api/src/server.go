// Package enterprise provides enterprise API with SLA and custom features
package enterprise

import (
	"bytes"
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"sync"
	"time"

	"github.com/golang-jwt/jwt/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

type Config struct {
	DBURL     string
	RedisURL  string
	Port      string
	JWTSecret string
	RPCURL    string
}

type Subscription struct {
	ID                string     `json:"id"`
	UserID            int        `json:"userId"`
	Plan              string     `json:"plan"`   // pro, enterprise, custom
	Status            string     `json:"status"` // active, expired, cancelled
	MonthlyRateLimit  int64      `json:"monthlyRateLimit"`
	RequestsUsed      int64      `json:"requestsUsed"`
	RequestsRemaining int64      `json:"requestsRemaining"`
	StartedAt         time.Time  `json:"startedAt"`
	ExpiresAt         *time.Time `json:"expiresAt"`
}

type APIEndpoint struct {
	Path      string `json:"path"`
	Method    string `json:"method"`
	RateLimit int    `json:"rateLimit"`
	Timeout   int    `json:"timeout"`
}

// Context keys for values attached by the API key middleware.
type subscriptionIDKey struct{}

type apiKeyIDKey struct{}

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
			UserID int    `json:"userId"`
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
		Name           string `json:"name"`
		RateLimit      int    `json:"rateLimit"`
	}
	json.NewDecoder(r.Body).Decode(&req)

	keyID := fmt.Sprintf("key_%d", time.Now().UnixNano())
	keyValue, err := generateSecureKey(32)
	if err != nil {
		http.Error(w, err.Error(), 500)
		return
	}

	// Only the digest is persisted; the plaintext key is returned once, to the
	// caller that created it.
	_, err = s.pool.Exec(ctx, "INSERT INTO enterprise_api_keys (id, subscription_id, key_value, name, rate_limit) VALUES ($1, $2, $3, $4, $5)", keyID, req.SubscriptionID, hashKey(keyValue), req.Name, req.RateLimit)
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
		Endpoint        string  `json:"endpoint"`
		Requests        int     `json:"requests"`
		AvgResponseTime float64 `json:"avgResponseTime"`
	}

	rows, err := s.pool.Query(ctx, "SELECT endpoint, COUNT(*), COALESCE(AVG(response_time_ms), 0) FROM enterprise_usage_logs WHERE subscription_id = $1 AND timestamp > $2 GROUP BY endpoint ORDER BY 2 DESC", subID, time.Now().Unix()-86400)
	if err != nil {
		http.Error(w, err.Error(), 500)
		return
	}
	defer rows.Close()

	usages := make([]Usage, 0)
	for rows.Next() {
		var u Usage
		if err := rows.Scan(&u.Endpoint, &u.Requests, &u.AvgResponseTime); err != nil {
			http.Error(w, err.Error(), 500)
			return
		}
		usages = append(usages, u)
	}
	if err := rows.Err(); err != nil {
		http.Error(w, err.Error(), 500)
		return
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

// generateSecureKey returns a URL-safe API key backed by length bytes of
// entropy from the operating system CSPRNG.
func generateSecureKey(length int) (string, error) {
	if length < 16 {
		return "", fmt.Errorf("api key length must be at least 16 bytes, got %d", length)
	}

	buf := make([]byte, length)
	if _, err := rand.Read(buf); err != nil {
		return "", fmt.Errorf("generate api key: %w", err)
	}

	return base64.RawURLEncoding.EncodeToString(buf), nil
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
	UserID   int    `json:"userId"`
	Plan     string `json:"plan"`
	APIKeyID string `json:"apiKeyId"`
	jwt.RegisteredClaims
}

// generateToken generates a JWT token
func (s *Server) generateToken(userID int, plan, apiKeyID string, expiry time.Duration) (string, error) {
	claims := Claims{
		UserID:   userID,
		Plan:     plan,
		APIKeyID: apiKeyID,
		RegisteredClaims: jwt.RegisteredClaims{
			ExpiresAt: jwt.NewNumericDate(time.Now().Add(expiry)),
			IssuedAt:  jwt.NewNumericDate(time.Now()),
			NotBefore: jwt.NewNumericDate(time.Now()),
			Issuer:    "tigersmartchain-enterprise",
		},
	}

	token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	return token.SignedString([]byte(s.cfg.JWTSecret))
}

// validateToken validates a JWT token
func (s *Server) validateToken(tokenString string) (*Claims, error) {
	if s.cfg.JWTSecret == "" {
		return nil, fmt.Errorf("jwt secret is not configured")
	}

	token, err := jwt.ParseWithClaims(tokenString, &Claims{}, func(token *jwt.Token) (interface{}, error) {
		// Pin the algorithm so a token cannot be re-signed with "none" or an
		// asymmetric algorithm to bypass the shared-secret check.
		if _, ok := token.Method.(*jwt.SigningMethodHMAC); !ok {
			return nil, fmt.Errorf("unexpected signing method: %v", token.Header["alg"])
		}
		return []byte(s.cfg.JWTSecret), nil
	}, jwt.WithValidMethods([]string{jwt.SigningMethodHS256.Alg()}))

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
			RateLimit      int
			IsActive       bool
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
		if err != nil {
			http.Error(w, "rate limit unavailable", http.StatusServiceUnavailable)
			return
		}
		if !allowed {
			http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
			return
		}

		ctx = context.WithValue(ctx, subscriptionIDKey{}, keyInfo.SubscriptionID)
		ctx = context.WithValue(ctx, apiKeyIDKey{}, keyInfo.ID)

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
	ID     string      `json:"id"`
	Method string      `json:"method"`
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
			entry := BatchResult{ID: item.ID, Result: result}
			if err != nil {
				entry.Error = err.Error()
			}
			results[i] = entry
		}(i, item)
	}

	wg.Wait()

	json.NewEncoder(w).Encode(BatchResponse{Results: results})
}

// supportedBatchMethods is the set of JSON-RPC methods the batch endpoint
// forwards to the upstream node.
var supportedBatchMethods = map[string]struct{}{
	"eth_blockNumber":                      {},
	"eth_chainId":                          {},
	"eth_gasPrice":                         {},
	"eth_maxPriorityFeePerGas":             {},
	"eth_feeHistory":                       {},
	"eth_getBlockByNumber":                 {},
	"eth_getBlockByHash":                   {},
	"eth_getBlockTransactionCountByNumber": {},
	"eth_getTransactionByHash":             {},
	"eth_getTransactionReceipt":            {},
	"eth_getTransactionCount":              {},
	"eth_getBalance":                       {},
	"eth_getCode":                          {},
	"eth_getStorageAt":                     {},
	"eth_getLogs":                          {},
	"eth_call":                             {},
	"eth_estimateGas":                      {},
	"net_version":                          {},
	"web3_clientVersion":                   {},
}

// executeBatchItem forwards one batch item to the upstream node.
func (s *Server) executeBatchItem(item BatchItem) (interface{}, error) {
	if _, ok := supportedBatchMethods[item.Method]; !ok {
		return nil, fmt.Errorf("unsupported method: %s", item.Method)
	}

	params, ok := item.Params.([]interface{})
	if !ok {
		params = []interface{}{}
	}

	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()

	var result json.RawMessage
	if err := s.rpcCall(ctx, item.Method, params, &result); err != nil {
		return nil, err
	}

	return result, nil
}

// rpcCall performs a JSON-RPC request against the configured node.
func (s *Server) rpcCall(ctx context.Context, method string, params []interface{}, out interface{}) error {
	if s.cfg.RPCURL == "" {
		return fmt.Errorf("upstream rpc url is not configured")
	}

	payload, err := json.Marshal(map[string]interface{}{
		"jsonrpc": "2.0",
		"id":      1,
		"method":  method,
		"params":  params,
	})
	if err != nil {
		return err
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, s.cfg.RPCURL, bytes.NewReader(payload))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(io.LimitReader(resp.Body, 8<<20))
	if err != nil {
		return err
	}

	var envelope struct {
		Result json.RawMessage `json:"result"`
		Error  *struct {
			Code    int    `json:"code"`
			Message string `json:"message"`
		} `json:"error"`
	}
	if err := json.Unmarshal(body, &envelope); err != nil {
		return fmt.Errorf("decode rpc response: %w", err)
	}
	if envelope.Error != nil {
		return fmt.Errorf("rpc error %d: %s", envelope.Error.Code, envelope.Error.Message)
	}
	if out == nil {
		return nil
	}

	return json.Unmarshal(envelope.Result, out)
}

// getBlockNumber returns the head block height reported by the node.
func (s *Server) getBlockNumber() (string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()

	var height string
	if err := s.rpcCall(ctx, "eth_blockNumber", []interface{}{}, &height); err != nil {
		return "", err
	}
	return height, nil
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

// CheckRateLimit increments the caller's counter for the current month and
// reports whether the request is still within the limit. A limit of zero or
// less means unlimited.
func (s *Server) CheckRateLimit(ctx context.Context, subID string, limit int) (bool, error) {
	if limit <= 0 {
		var stored int64
		if err := s.pool.QueryRow(ctx, "SELECT monthly_rate_limit FROM enterprise_subscriptions WHERE id = $1", subID).Scan(&stored); err != nil {
			return false, err
		}
		if stored <= 0 {
			return true, nil
		}
		limit = int(stored)
	}

	key := fmt.Sprintf("enterprise:ratelimit:%s:%s", subID, time.Now().UTC().Format("2006-01"))

	pipe := s.redis.TxPipeline()
	count := pipe.Incr(ctx, key)
	// Keep the counter slightly beyond the window so it expires on its own.
	pipe.Expire(ctx, key, 32*24*time.Hour)
	if _, err := pipe.Exec(ctx); err != nil {
		return false, err
	}

	return count.Val() <= int64(limit), nil
}
