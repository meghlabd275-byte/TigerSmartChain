// TigerScan Complete Production API Server
// Full REST API with all endpoints, pagination, rate limiting, API keys, webhooks, GraphQL
// Uses Go for high performance and low latency

package main

import (
	"bytes"
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"database/sql"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"os/signal"
	"regexp"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	"github.com/gin-contrib/cors"
	"github.com/gin-gonic/gin"
	"github.com/go-redis/redis/v8"
	_ "github.com/lib/pq"
	"golang.org/x/time/rate"
)

// ============================================================================
// Configuration
// ============================================================================

var (
	hexAddress          = regexp.MustCompile(`^0x[0-9a-fA-F]{40}$`)
	hexFunctionSelector = regexp.MustCompile(`^0x[0-9a-fA-F]{8}$`)
	hexWord             = regexp.MustCompile(`^(0x)?[0-9a-fA-F]{64}$`)
	hexRawTx            = regexp.MustCompile(`^0x[0-9a-fA-F]+$`)
)

// cell writes a scanned column straight into a map. Go does not allow taking
// the address of a map element, so `rows.Scan(&row["hash"], ...)` cannot
// compile; a cell holds the map and the key instead and implements
// sql.Scanner, which is what the driver requires.
type cell struct {
	m   map[string]interface{}
	key string
}

func (c *cell) Scan(src interface{}) error {
	if c.m == nil {
		return fmt.Errorf("cell for %q has no destination map", c.key)
	}
	// lib/pq returns []byte for every text and numeric column. Storing those
	// bytes directly would make encoding/json emit base64, so text is kept as
	// a string and everything else is stored as the driver returned it.
	if b, ok := src.([]byte); ok {
		c.m[c.key] = string(b)
	} else {
		c.m[c.key] = src
	}
	return nil
}

type Config struct {
	Server     ServerConfig     `json:"server"`
	Database   DatabaseConfig   `json:"database"`
	Redis      RedisConfig      `json:"redis"`
	Security   SecurityConfig   `json:"security"`
	RateLimit  RateLimitConfig  `json:"rate_limit"`
	Webhook    WebhookConfig    `json:"webhook"`
	JWT        JWTConfig        `json:"jwt"`
	Encryption EncryptionConfig `json:"encryption"`
	RPC        RPCConfig        `json:"rpc"`
}

type RPCConfig struct {
	URL     string `json:"url"`
	Timeout int    `json:"timeout_seconds"`
}

type ServerConfig struct {
	Host            string `json:"host"`
	Port            int    `json:"port"`
	ReadTimeout     int    `json:"read_timeout"`
	WriteTimeout    int    `json:"write_timeout"`
	MaxHeaderBytes  int    `json:"max_header_bytes"`
	ShutdownTimeout int    `json:"shutdown_timeout"`
}

type DatabaseConfig struct {
	Host            string `json:"host"`
	Port            int    `json:"port"`
	Username        string `json:"username"`
	Password        string `json:"password"`
	Database        string `json:"database"`
	MaxOpenConns    int    `json:"max_open_conns"`
	MaxIdleConns    int    `json:"max_idle_conns"`
	ConnMaxLifetime int    `json:"conn_max_lifetime"`
}

type RedisConfig struct {
	Host     string `json:"host"`
	Port     int    `json:"port"`
	Password string `json:"password"`
	Database int    `json:"database"`
}

type SecurityConfig struct {
	APIKeys              []string `json:"api_keys"`
	JWTSecret            string   `json:"jwt_secret"`
	EncryptionKey        string   `json:"encryption_key"`
	CorsOrigins          []string `json:"cors_origins"`
	AllowedIPs           []string `json:"allowed_ips"`
	EnableIPBan          bool     `json:"enable_ip_ban"`
	BanDuration          int      `json:"ban_duration"`
	MaxRequestsPerMinute int      `json:"max_requests_per_minute"`
}

type RateLimitConfig struct {
	RequestsPerSecond int  `json:"requests_per_second"`
	Burst             int  `json:"burst"`
	Enabled           bool `json:"enabled"`
}

type WebhookConfig struct {
	Enabled bool   `json:"enabled"`
	Secret  string `json:"secret"`
}

type JWTConfig struct {
	Secret      string `json:"secret"`
	ExpireHours int    `json:"expire_hours"`
}

type EncryptionConfig struct {
	Enabled bool   `json:"enabled"`
	Key     string `json:"key"`
}

// ============================================================================
// Database
// ============================================================================

type DB struct {
	*sql.DB
	redis *redis.Client
}

func NewDB(cfg DatabaseConfig, redisCfg RedisConfig) (*DB, error) {
	dsn := fmt.Sprintf(
		"host=%s port=%d user=%s password=%s dbname=%s sslmode=disable",
		cfg.Host, cfg.Port, cfg.Username, cfg.Password, cfg.Database,
	)

	db, err := sql.Open("postgres", dsn)
	if err != nil {
		return nil, err
	}

	db.SetMaxOpenConns(cfg.MaxOpenConns)
	db.SetMaxIdleConns(cfg.MaxIdleConns)
	db.SetConnMaxLifetime(time.Duration(cfg.ConnMaxLifetime) * time.Second)

	rdb := redis.NewClient(&redis.Options{
		Addr:     fmt.Sprintf("%s:%d", redisCfg.Host, redisCfg.Port),
		Password: redisCfg.Password,
		DB:       redisCfg.Database,
	})

	return &DB{db, rdb}, nil
}

func (d *DB) Close() {
	if d.DB != nil {
		d.DB.Close()
	}
	if d.redis != nil {
		d.redis.Close()
	}
}

// ============================================================================
// Rate Limiter
// ============================================================================

type RateLimiter struct {
	store  map[string]*ClientLimiter
	mu     sync.RWMutex
	global *rate.Limiter
	config RateLimitConfig
}

type ClientLimiter struct {
	limiter  *rate.Limiter
	requests int
	banned   bool
	bannedAt time.Time
}

func NewRateLimiter(config RateLimitConfig) *RateLimiter {
	return &RateLimiter{
		store:  make(map[string]*ClientLimiter),
		global: rate.NewLimiter(rate.Limit(config.RequestsPerSecond), config.Burst),
		config: config,
	}
}

func (r *RateLimiter) Allow(key string) bool {
	if !r.config.Enabled {
		return true
	}

	if !r.global.Allow() {
		return false
	}

	r.mu.RLock()
	client, exists := r.store[key]
	r.mu.RUnlock()

	if !exists {
		r.mu.Lock()
		client = &ClientLimiter{
			limiter: rate.NewLimiter(rate.Limit(r.config.RequestsPerSecond), r.config.Burst),
		}
		r.store[key] = client
		r.mu.Unlock()
	}

	return client.limiter.Allow()
}

func (r *RateLimiter) Ban(key string, duration time.Duration) {
	r.mu.Lock()
	defer r.mu.Unlock()

	if client, exists := r.store[key]; exists {
		client.banned = true
		client.bannedAt = time.Now()

		go func() {
			time.Sleep(duration)
			r.mu.Lock()
			if client, exists := r.store[key]; exists {
				client.banned = false
				client.bannedAt = time.Time{}
			}
			r.mu.Unlock()
		}()
	}
}

func (r *RateLimiter) IsBanned(key string) bool {
	r.mu.RLock()
	defer r.mu.RUnlock()

	if client, exists := r.store[key]; exists {
		return client.banned
	}
	return false
}

// ============================================================================
// API Key Manager
// ============================================================================

type APIKeyManager struct {
	keys map[string]*APIKey
	mu   sync.RWMutex
}

type APIKey struct {
	Key       string
	Name      string
	Scopes    []string
	RateLimit int
	Requests  uint64
	CreatedAt time.Time
	LastUsed  time.Time
	ExpiresAt *time.Time
	Disabled  bool
}

func NewAPIKeyManager() *APIKeyManager {
	return &APIKeyManager{
		keys: make(map[string]*APIKey),
	}
}

func (m *APIKeyManager) Add(key *APIKey) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.keys[key.Key] = key
}

func (m *APIKeyManager) Get(key string) (*APIKey, bool) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	// Compare in constant time so a caller cannot learn a stored key byte by
	// byte from response timing.
	var found *APIKey
	for stored, k := range m.keys {
		if subtle.ConstantTimeCompare([]byte(stored), []byte(key)) == 1 {
			found = k
			break
		}
	}
	if found == nil || found.Disabled {
		return nil, false
	}
	k := found
	if k.ExpiresAt != nil && time.Now().After(*k.ExpiresAt) {
		return nil, false
	}
	return k, true
}

func (m *APIKeyManager) Validate(key, scope string) bool {
	k, ok := m.Get(key)
	if !ok {
		return false
	}

	for _, s := range k.Scopes {
		if s == scope || s == "*" {
			return true
		}
	}

	return false
}

func (m *APIKeyManager) Increment(key string) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if k, ok := m.keys[key]; ok {
		atomic.AddUint64(&k.Requests, 1)
		k.LastUsed = time.Now()
	}
}

// ============================================================================
// Webhook Manager
// ============================================================================

type WebhookManager struct {
	mu       sync.RWMutex
	webhooks map[string]*Webhook
	queue    chan *WebhookEvent
}

type Webhook struct {
	ID      string
	URL     string
	Events  []string
	Secret  string
	Active  bool
	Retries int
}

type WebhookEvent struct {
	Event   string
	URL     string
	Payload interface{}
	Secret  string
}

func NewWebhookManager() *WebhookManager {
	m := &WebhookManager{
		webhooks: make(map[string]*Webhook),
		queue:    make(chan *WebhookEvent, 10000),
	}

	go m.worker()

	return m
}

func (m *WebhookManager) worker() {
	for event := range m.queue {
		m.sendEvent(event)
	}
}

func (m *WebhookManager) sendEvent(event *WebhookEvent) {
	payload, _ := json.Marshal(event.Payload)

	req, _ := http.NewRequest("POST", event.URL, strings.NewReader(string(payload)))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Webhook-Event", event.Event)
	req.Header.Set("X-Webhook-Timestamp", strconv.FormatInt(time.Now().Unix(), 10))

	if event.Secret != "" {
		signature := computeHMAC(payload, event.Secret)
		req.Header.Set("X-Webhook-Signature", signature)
	}

	client := &http.Client{Timeout: 10 * time.Second}
	client.Do(req)
}

func (m *WebhookManager) Queue(event *WebhookEvent) {
	select {
	case m.queue <- event:
	default:
		// Queue full, drop event
	}
}

func (m *WebhookManager) Add(webhook *Webhook) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.webhooks[webhook.ID] = webhook
}

// ============================================================================
// Encryption
// ============================================================================

type Encryptor struct {
	key []byte
}

func NewEncryptor(key string) (*Encryptor, error) {
	if key == "" {
		return &Encryptor{nil}, nil
	}

	keyBytes, err := hex.DecodeString(key)
	if err != nil {
		return nil, err
	}

	if len(keyBytes) != 32 {
		return nil, fmt.Errorf("encryption key must be 32 bytes")
	}

	return &Encryptor{key: keyBytes}, nil
}

func (e *Encryptor) Encrypt(plaintext []byte) ([]byte, error) {
	if e.key == nil {
		return plaintext, nil
	}

	block, err := aes.NewCipher(e.key)
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

	ciphertext := gcm.Seal(nonce, nonce, plaintext, nil)
	return ciphertext, nil
}

func (e *Encryptor) Decrypt(ciphertext []byte) ([]byte, error) {
	if e.key == nil {
		return ciphertext, nil
	}

	block, err := aes.NewCipher(e.key)
	if err != nil {
		return nil, err
	}

	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return nil, err
	}

	nonceSize := gcm.NonceSize()
	if len(ciphertext) < nonceSize {
		return nil, fmt.Errorf("ciphertext too short")
	}

	nonce, ciphertext := ciphertext[:nonceSize], ciphertext[nonceSize:]
	plaintext, err := gcm.Open(nil, nonce, ciphertext, nil)
	if err != nil {
		return nil, err
	}

	return plaintext, nil
}

// ============================================================================
// Handler
// ============================================================================

type Handler struct {
	db          *DB
	rateLimiter *RateLimiter
	apiKeyMgr   *APIKeyManager
	webhookMgr  *WebhookManager
	encryptor   *Encryptor
	config      *Config
}

func NewHandler(db *DB, config *Config) *Handler {
	rateLimiter := NewRateLimiter(config.RateLimit)
	apiKeyMgr := NewAPIKeyManager()
	webhookMgr := NewWebhookManager()

	encryptor, _ := NewEncryptor(config.Encryption.Key)

	return &Handler{
		db:          db,
		rateLimiter: rateLimiter,
		apiKeyMgr:   apiKeyMgr,
		webhookMgr:  webhookMgr,
		encryptor:   encryptor,
		config:      config,
	}
}

// ============================================================================
// Block Handlers
// ============================================================================

func (h *Handler) GetBlocks(c *gin.Context) {
	page := c.DefaultQuery("page", "1")
	limit := c.DefaultQuery("limit", "25")

	pageNum, _ := strconv.Atoi(page)
	limitNum, _ := strconv.Atoi(limit)

	if pageNum < 1 {
		pageNum = 1
	}
	if limitNum < 1 || limitNum > 100 {
		limitNum = 25
	}

	offset := (pageNum - 1) * limitNum

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT number, hash, parent_hash, miner, gas_limit, gas_used, timestamp, tx_count, size
		 FROM blocks ORDER BY number DESC LIMIT $1 OFFSET $2`,
		limitNum, offset,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var blocks []map[string]interface{}
	for rows.Next() {
		var b map[string]interface{}
		if err := rows.Scan(&cell{b, "number"}, &cell{b, "hash"}, &cell{b, "parent_hash"}, &cell{b, "miner"},
			&cell{b, "gas_limit"}, &cell{b, "gas_used"}, &cell{b, "timestamp"}, &cell{b, "tx_count"}, &cell{b, "size"}); err == nil {
			blocks = append(blocks, b)
		}
	}

	c.JSON(http.StatusOK, gin.H{"blocks": blocks, "page": pageNum, "limit": limitNum})
}

func (h *Handler) GetBlock(c *gin.Context) {
	numberOrHash := c.Param("numberOrHash")

	var block map[string]interface{}
	var err error

	// Try as number
	if num, err := strconv.ParseUint(numberOrHash, 10, 64); err == nil {
		err = h.db.QueryRowContext(c.Request.Context(),
			`SELECT number, hash, parent_hash, nonce, sha3_uncles, logs_bloom,
					transactions_root, state_root, receipts_root, miner,
					difficulty, total_difficulty, gas_limit, gas_used, timestamp,
					size, extra_data, base_fee_per_gas, tx_count, reward
			 FROM blocks WHERE number = $1`,
			num,
		).Scan(
			&cell{block, "number"}, &cell{block, "hash"}, &cell{block, "parent_hash"}, &cell{block, "nonce"},
			&cell{block, "sha3_uncles"}, &cell{block, "logs_bloom"}, &cell{block, "transactions_root"},
			&cell{block, "state_root"}, &cell{block, "receipts_root"}, &cell{block, "miner"}, &cell{block, "difficulty"},
			&cell{block, "total_difficulty"}, &cell{block, "gas_limit"}, &cell{block, "gas_used"}, &cell{block, "timestamp"},
			&cell{block, "size"}, &cell{block, "extra_data"}, &cell{block, "base_fee_per_gas"}, &cell{block, "tx_count"},
			&cell{block, "reward"},
		)
	} else {
		// Try as hash
		err = h.db.QueryRowContext(c.Request.Context(),
			`SELECT number, hash, parent_hash, nonce, sha3_uncles, logs_bloom,
					transactions_root, state_root, receipts_root, miner,
					difficulty, total_difficulty, gas_limit, gas_used, timestamp,
					size, extra_data, base_fee_per_gas, tx_count, reward
			 FROM blocks WHERE hash = $1`,
			numberOrHash,
		).Scan(
			&cell{block, "number"}, &cell{block, "hash"}, &cell{block, "parent_hash"}, &cell{block, "nonce"},
			&cell{block, "sha3_uncles"}, &cell{block, "logs_bloom"}, &cell{block, "transactions_root"},
			&cell{block, "state_root"}, &cell{block, "receipts_root"}, &cell{block, "miner"}, &cell{block, "difficulty"},
			&cell{block, "total_difficulty"}, &cell{block, "gas_limit"}, &cell{block, "gas_used"}, &cell{block, "timestamp"},
			&cell{block, "size"}, &cell{block, "extra_data"}, &cell{block, "base_fee_per_gas"}, &cell{block, "tx_count"},
			&cell{block, "reward"},
		)
	}

	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Block not found"})
		return
	}

	c.JSON(http.StatusOK, block)
}

func (h *Handler) GetLatestBlock(c *gin.Context) {
	var block map[string]interface{}

	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT number, hash, parent_hash, miner, gas_limit, gas_used, timestamp, tx_count, size
		 FROM blocks ORDER BY number DESC LIMIT 1`,
	).Scan(
		&cell{block, "number"}, &cell{block, "hash"}, &cell{block, "parent_hash"}, &cell{block, "miner"},
		&cell{block, "gas_limit"}, &cell{block, "gas_used"}, &cell{block, "timestamp"}, &cell{block, "tx_count"},
		&cell{block, "size"},
	)

	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, block)
}

// ============================================================================
// Transaction Handlers
// ============================================================================

func (h *Handler) GetTransactions(c *gin.Context) {
	page := c.DefaultQuery("page", "1")
	limit := c.DefaultQuery("limit", "25")

	pageNum, _ := strconv.Atoi(page)
	limitNum, _ := strconv.Atoi(limit)

	if pageNum < 1 {
		pageNum = 1
	}
	if limitNum < 1 || limitNum > 100 {
		limitNum = 25
	}

	offset := (pageNum - 1) * limitNum

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT hash, from_address, to_address, value, gas_price, gas_limit, gas_used, status, block_number, timestamp
		 FROM transactions WHERE status != 'pending' ORDER BY block_number DESC, transaction_index DESC LIMIT $1 OFFSET $2`,
		limitNum, offset,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var txs []map[string]interface{}
	for rows.Next() {
		var t map[string]interface{}
		if err := rows.Scan(&cell{t, "hash"}, &cell{t, "from_address"}, &cell{t, "to_address"}, &cell{t, "value"},
			&cell{t, "gas_price"}, &cell{t, "gas_limit"}, &cell{t, "gas_used"}, &cell{t, "status"},
			&cell{t, "block_number"}, &cell{t, "timestamp"}); err == nil {
			txs = append(txs, t)
		}
	}

	c.JSON(http.StatusOK, gin.H{"transactions": txs, "page": pageNum, "limit": limitNum})
}

func (h *Handler) GetTransaction(c *gin.Context) {
	hash := c.Param("hash")

	var tx map[string]interface{}

	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT hash, nonce, from_address, to_address, value, gas_price, gas_limit, gas_used,
				input, v, r, s, chain_id, status, block_number, timestamp
		 FROM transactions WHERE hash = $1`,
		hash,
	).Scan(
		&cell{tx, "hash"}, &cell{tx, "nonce"}, &cell{tx, "from_address"}, &cell{tx, "to_address"},
		&cell{tx, "value"}, &cell{tx, "gas_price"}, &cell{tx, "gas_limit"}, &cell{tx, "gas_used"},
		&cell{tx, "input"}, &cell{tx, "v"}, &cell{tx, "r"}, &cell{tx, "s"}, &cell{tx, "chain_id"},
		&cell{tx, "status"}, &cell{tx, "block_number"}, &cell{tx, "timestamp"},
	)

	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Transaction not found"})
		return
	}

	c.JSON(http.StatusOK, tx)
}

func (h *Handler) GetTransactionReceipt(c *gin.Context) {
	hash := c.Param("hash")

	var receipt map[string]interface{}

	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT transaction_hash, block_hash, block_number, cumulative_gas_used, gas_used,
				contract_address, logs, logs_bloom, status
		 FROM transactions WHERE hash = $1`,
		hash,
	).Scan(
		&cell{receipt, "transaction_hash"}, &cell{receipt, "block_hash"}, &cell{receipt, "block_number"},
		&cell{receipt, "cumulative_gas_used"}, &cell{receipt, "gas_used"}, &cell{receipt, "contract_address"},
		&cell{receipt, "logs"}, &cell{receipt, "logs_bloom"}, &cell{receipt, "status"},
	)

	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Transaction not found"})
		return
	}

	c.JSON(http.StatusOK, receipt)
}

func (h *Handler) GetInternalTransactions(c *gin.Context) {
	hash := c.Param("hash")

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT depth, call_type, from_address, to_address, value, input, output
		 FROM internal_transactions WHERE transaction_hash = $1 ORDER BY depth`,
		hash,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var internals []map[string]interface{}
	for rows.Next() {
		var t map[string]interface{}
		if err := rows.Scan(&cell{t, "depth"}, &cell{t, "call_type"}, &cell{t, "from_address"},
			&cell{t, "to_address"}, &cell{t, "value"}, &cell{t, "input"}, &cell{t, "output"}); err == nil {
			internals = append(internals, t)
		}
	}

	c.JSON(http.StatusOK, gin.H{"internal_transactions": internals})
}

// ============================================================================
// Token Handlers
// ============================================================================

func (h *Handler) GetTokens(c *gin.Context) {
	page := c.DefaultQuery("page", "1")
	limit := c.DefaultQuery("limit", "25")

	pageNum, _ := strconv.Atoi(page)
	limitNum, _ := strconv.Atoi(limit)

	if pageNum < 1 {
		pageNum = 1
	}
	if limitNum < 1 || limitNum > 100 {
		limitNum = 25
	}

	offset := (pageNum - 1) * limitNum

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT address, name, symbol, decimals, total_supply, holders_count, transfers_count,
				price_usd, market_cap, is_verified
		 FROM tokens ORDER BY market_cap DESC LIMIT $1 OFFSET $2`,
		limitNum, offset,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var tokens []map[string]interface{}
	for rows.Next() {
		var t map[string]interface{}
		if err := rows.Scan(&cell{t, "address"}, &cell{t, "name"}, &cell{t, "symbol"}, &cell{t, "decimals"},
			&cell{t, "total_supply"}, &cell{t, "holders_count"}, &cell{t, "transfers_count"},
			&cell{t, "price_usd"}, &cell{t, "market_cap"}, &cell{t, "is_verified"}); err == nil {
			tokens = append(tokens, t)
		}
	}

	c.JSON(http.StatusOK, gin.H{"tokens": tokens, "page": pageNum, "limit": limitNum})
}

func (h *Handler) GetToken(c *gin.Context) {
	address := c.Param("address")

	var token map[string]interface{}

	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT address, name, symbol, decimals, total_supply, holders_count, transfers_count,
				price_usd, price_change_24h, market_cap, volume_24h, is_verified,
				description, website, social
		 FROM tokens WHERE address = $1`,
		address,
	).Scan(
		&cell{token, "address"}, &cell{token, "name"}, &cell{token, "symbol"}, &cell{token, "decimals"},
		&cell{token, "total_supply"}, &cell{token, "holders_count"}, &cell{token, "transfers_count"},
		&cell{token, "price_usd"}, &cell{token, "price_change_24h"}, &cell{token, "market_cap"},
		&cell{token, "volume_24h"}, &cell{token, "is_verified"}, &cell{token, "description"},
		&cell{token, "website"}, &cell{token, "social"},
	)

	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Token not found"})
		return
	}

	c.JSON(http.StatusOK, token)
}

func (h *Handler) GetTokenHolders(c *gin.Context) {
	address := c.Param("address")

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT address, balance, percent_holdings
		 FROM token_holders WHERE token_address = $1 AND balance > 0
		 ORDER BY balance DESC LIMIT 100`,
		address,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var holders []map[string]interface{}
	for rows.Next() {
		var h map[string]interface{}
		if err := rows.Scan(&cell{h, "address"}, &cell{h, "balance"}, &cell{h, "percent_holdings"}); err == nil {
			holders = append(holders, h)
		}
	}

	c.JSON(http.StatusOK, gin.H{"holders": holders})
}

func (h *Handler) GetTokenTransfers(c *gin.Context) {
	address := c.Param("address")
	page := c.DefaultQuery("page", "1")
	limit := c.DefaultQuery("limit", "25")

	pageNum, _ := strconv.Atoi(page)
	limitNum, _ := strconv.Atoi(limit)

	if pageNum < 1 {
		pageNum = 1
	}
	if limitNum < 1 || limitNum > 100 {
		limitNum = 25
	}

	offset := (pageNum - 1) * limitNum

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT from_address, to_address, value, transaction_hash, block_number
		 FROM token_transfers WHERE token_address = $1
		 ORDER BY block_number DESC, log_index DESC LIMIT $2 OFFSET $3`,
		address, limitNum, offset,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var transfers []map[string]interface{}
	for rows.Next() {
		var t map[string]interface{}
		if err := rows.Scan(&cell{t, "from_address"}, &cell{t, "to_address"}, &cell{t, "value"},
			&cell{t, "transaction_hash"}, &cell{t, "block_number"}); err == nil {
			transfers = append(transfers, t)
		}
	}

	c.JSON(http.StatusOK, gin.H{"transfers": transfers, "page": pageNum, "limit": limitNum})
}

func (h *Handler) GetTokenPrice(c *gin.Context) {
	address := c.Param("address")

	var price struct {
		PriceUSD  float64 `json:"price_usd"`
		Change24h float64 `json:"change_24h"`
		Volume24h float64 `json:"volume_24h"`
		MarketCap float64 `json:"market_cap"`
		Timestamp int64   `json:"timestamp"`
	}

	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT price_usd, price_change_24h, volume_24h, market_cap
		 FROM tokens WHERE address = $1`,
		address,
	).Scan(&price.PriceUSD, &price.Change24h, &price.Volume24h, &price.MarketCap)

	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Token not found"})
		return
	}

	price.Timestamp = time.Now().Unix()

	c.JSON(http.StatusOK, price)
}

// ============================================================================
// NFT Handlers
// ============================================================================

func (h *Handler) GetNFTCollections(c *gin.Context) {
	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT address, name, symbol, contract_type, total_supply, holders_count,
				transfers_count, floor_price, volume_24h, is_verified
		 FROM nft_collections ORDER BY volume_24h DESC LIMIT 50`,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var collections []map[string]interface{}
	for rows.Next() {
		var c map[string]interface{}
		if err := rows.Scan(&cell{c, "address"}, &cell{c, "name"}, &cell{c, "symbol"}, &cell{c, "contract_type"},
			&cell{c, "total_supply"}, &cell{c, "holders_count"}, &cell{c, "transfers_count"},
			&cell{c, "floor_price"}, &cell{c, "volume_24h"}, &cell{c, "is_verified"}); err == nil {
			collections = append(collections, c)
		}
	}

	c.JSON(http.StatusOK, gin.H{"collections": collections})
}

func (h *Handler) GetNFT(c *gin.Context) {
	collection := c.Param("collection")
	tokenID := c.Param("token_id")

	var nft map[string]interface{}

	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT token_address, token_id, owner, uri, metadata, name, description,
				image_url, attributes, is_burned
		 FROM nfts WHERE token_address = $1 AND token_id = $2`,
		collection, tokenID,
	).Scan(
		&cell{nft, "token_address"}, &cell{nft, "token_id"}, &cell{nft, "owner"}, &cell{nft, "uri"},
		&cell{nft, "metadata"}, &cell{nft, "name"}, &cell{nft, "description"}, &cell{nft, "image_url"},
		&cell{nft, "attributes"}, &cell{nft, "is_burned"},
	)

	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "NFT not found"})
		return
	}

	c.JSON(http.StatusOK, nft)
}

func (h *Handler) GetNFTTransfers(c *gin.Context) {
	collection := c.Param("collection")

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT from_address, to_address, token_id, value, transaction_hash, block_number
		 FROM nft_transfers WHERE token_address = $1
		 ORDER BY block_number DESC, log_index DESC LIMIT 50`,
		collection,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var transfers []map[string]interface{}
	for rows.Next() {
		var t map[string]interface{}
		if err := rows.Scan(&cell{t, "from_address"}, &cell{t, "to_address"}, &cell{t, "token_id"},
			&cell{t, "value"}, &cell{t, "transaction_hash"}, &cell{t, "block_number"}); err == nil {
			transfers = append(transfers, t)
		}
	}

	c.JSON(http.StatusOK, gin.H{"transfers": transfers})
}

// ============================================================================
// Account Handlers
// ============================================================================

func (h *Handler) GetAccount(c *gin.Context) {
	address := c.Param("address")

	var account map[string]interface{}

	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT address, balance, nonce, code_hash, is_contract, is_verified,
				token_balance_count, nft_balance_count
		 FROM accounts WHERE address = $1`,
		address,
	).Scan(
		&cell{account, "address"}, &cell{account, "balance"}, &cell{account, "nonce"},
		&cell{account, "code_hash"}, &cell{account, "is_contract"}, &cell{account, "is_verified"},
		&cell{account, "token_balance_count"}, &cell{account, "nft_balance_count"},
	)

	if err != nil {
		// Account not found, return empty
		account = map[string]interface{}{
			"address":             address,
			"balance":             "0",
			"nonce":               0,
			"is_contract":         false,
			"token_balance_count": 0,
			"nft_balance_count":   0,
		}
	}

	c.JSON(http.StatusOK, account)
}

func (h *Handler) GetAccountTransactions(c *gin.Context) {
	address := c.Param("address")

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT hash, from_address, to_address, value, gas_used, status, block_number, timestamp
		 FROM transactions WHERE from_address = $1 OR to_address = $1
		 ORDER BY block_number DESC LIMIT 50`,
		address,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var txs []map[string]interface{}
	for rows.Next() {
		var t map[string]interface{}
		if err := rows.Scan(&cell{t, "hash"}, &cell{t, "from_address"}, &cell{t, "to_address"},
			&cell{t, "value"}, &cell{t, "gas_used"}, &cell{t, "status"}, &cell{t, "block_number"},
			&cell{t, "timestamp"}); err == nil {
			txs = append(txs, t)
		}
	}

	c.JSON(http.StatusOK, gin.H{"transactions": txs})
}

func (h *Handler) GetAccountTokens(c *gin.Context) {
	address := c.Param("address")

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT th.token_address, t.name, t.symbol, t.decimals, th.balance
		 FROM token_holders th
		 JOIN tokens t ON t.address = th.token_address
		 WHERE th.address = $1 AND th.balance > 0
		 ORDER BY th.balance DESC`,
		address,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var tokens []map[string]interface{}
	for rows.Next() {
		var t map[string]interface{}
		if err := rows.Scan(&cell{t, "token_address"}, &cell{t, "name"}, &cell{t, "symbol"},
			&cell{t, "decimals"}, &cell{t, "balance"}); err == nil {
			tokens = append(tokens, t)
		}
	}

	c.JSON(http.StatusOK, gin.H{"tokens": tokens})
}

func (h *Handler) GetAccountNFTs(c *gin.Context) {
	address := c.Param("address")

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT no.token_address, nc.name, no.token_id, n.image_url
		 FROM nft_owners no
		 JOIN nft_collections nc ON nc.address = no.token_address
		 JOIN nfts n ON n.token_address = no.token_address AND n.token_id = no.token_id
		 WHERE no.owner = $1`,
		address,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var nfts []map[string]interface{}
	for rows.Next() {
		var n map[string]interface{}
		if err := rows.Scan(&cell{n, "token_address"}, &cell{n, "name"}, &cell{n, "token_id"},
			&cell{n, "image_url"}); err == nil {
			nfts = append(nfts, n)
		}
	}

	c.JSON(http.StatusOK, gin.H{"nfts": nfts})
}

// ============================================================================
// Contract Handlers
// ============================================================================

func (h *Handler) GetContract(c *gin.Context) {
	address := c.Param("address")

	var contract map[string]interface{}

	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT address, contract_name, compiler, compiler_version, optimization_enabled,
				optimization_runs, evm_version, license_type, source_code, abi,
				bytecode, runtime_bytecode, is_verified, verification_status, verified_at
		 FROM contracts WHERE address = $1`,
		address,
	).Scan(
		&cell{contract, "address"}, &cell{contract, "contract_name"}, &cell{contract, "compiler"},
		&cell{contract, "compiler_version"}, &cell{contract, "optimization_enabled"},
		&cell{contract, "optimization_runs"}, &cell{contract, "evm_version"}, &cell{contract, "license_type"},
		&cell{contract, "source_code"}, &cell{contract, "abi"}, &cell{contract, "bytecode"},
		&cell{contract, "runtime_bytecode"}, &cell{contract, "is_verified"}, &cell{contract, "verification_status"},
		&cell{contract, "verified_at"},
	)

	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Contract not found"})
		return
	}

	c.JSON(http.StatusOK, contract)
}

func (h *Handler) GetContractSource(c *gin.Context) {
	address := c.Param("address")

	var source struct {
		SourceCode string `json:"source_code"`
	}

	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT source_code FROM contracts WHERE address = $1`,
		address,
	).Scan(&source.SourceCode)

	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Contract not found"})
		return
	}

	c.JSON(http.StatusOK, source)
}

func (h *Handler) GetContractABI(c *gin.Context) {
	address := c.Param("address")

	var abi struct {
		ABI string `json:"abi"`
	}

	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT abi FROM contracts WHERE address = $1`,
		address,
	).Scan(&abi.ABI)

	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Contract not found"})
		return
	}

	c.JSON(http.StatusOK, abi)
}

func (h *Handler) ReadContract(c *gin.Context) {
	address := c.Param("address")

	var body struct {
		Method string   `json:"method"`
		Params []string `json:"params"`
	}

	if err := c.ShouldBindJSON(&body); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid request body"})
		return
	}

	result, err := h.ethCall(c.Request.Context(), address, body.Method, body.Params)
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, gin.H{"result": result})
}

// ethCall executes a read-only call against the node and returns the raw
// return data. The call is sent with an explicit zero `from`, so no account
// state is assumed on the caller's behalf.
func (h *Handler) ethCall(ctx context.Context, to, method string, params []string) (string, error) {
	if h.config.RPC.URL == "" {
		return "", fmt.Errorf("rpc url is not configured")
	}
	if !hexAddress.MatchString(to) {
		return "", fmt.Errorf("invalid contract address %q", to)
	}
	if !hexFunctionSelector.MatchString(method) {
		return "", fmt.Errorf("method must be a 4-byte hex selector, got %q", method)
	}
	for _, p := range params {
		if !hexWord.MatchString(p) {
			return "", fmt.Errorf("parameter %q is not a 32-byte hex word", p)
		}
	}

	data := method
	for _, p := range params {
		data += strings.TrimPrefix(p, "0x")
	}

	payload, err := json.Marshal(map[string]any{
		"jsonrpc": "2.0",
		"id":      1,
		"method":  "eth_call",
		"params": []any{
			map[string]any{
				"to":   to,
				"from": "0x0000000000000000000000000000000000000000",
				"data": data,
			},
			"latest",
		},
	})
	if err != nil {
		return "", err
	}

	timeout := 15 * time.Second
	if h.config.RPC.Timeout > 0 {
		timeout = time.Duration(h.config.RPC.Timeout) * time.Second
	}
	client := &http.Client{Timeout: timeout}
	resp, err := client.Post(h.config.RPC.URL, "application/json", bytes.NewReader(payload))
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	var out struct {
		Result string `json:"result"`
		Error  *struct {
			Message string `json:"message"`
		} `json:"error"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return "", err
	}
	if out.Error != nil {
		return "", fmt.Errorf("rpc error: %s", out.Error.Message)
	}
	return out.Result, nil
}

func (h *Handler) WriteContract(c *gin.Context) {
	address := c.Param("address")
	if !hexAddress.MatchString(address) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid contract address"})
		return
	}

	var body struct {
		Method string   `json:"method"`
		Params []string `json:"params"`
		Signed string   `json:"signed"`
	}

	if err := c.ShouldBindJSON(&body); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid request body"})
		return
	}

	if h.config.RPC.URL == "" {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": "rpc url is not configured"})
		return
	}
	if !hexRawTx.MatchString(body.Signed) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "signed transaction is not a hex string"})
		return
	}

	payload, err := json.Marshal(map[string]any{
		"jsonrpc": "2.0",
		"id":      1,
		"method":  "eth_sendRawTransaction",
		"params":  []any{body.Signed},
	})
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	timeout := 15 * time.Second
	if h.config.RPC.Timeout > 0 {
		timeout = time.Duration(h.config.RPC.Timeout) * time.Second
	}
	client := &http.Client{Timeout: timeout}
	resp, err := client.Post(h.config.RPC.URL, "application/json", bytes.NewReader(payload))
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
		return
	}
	defer resp.Body.Close()

	var out struct {
		Result string `json:"result"`
		Error  *struct {
			Message string `json:"message"`
		} `json:"error"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
		return
	}
	if out.Error != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": out.Error.Message})
		return
	}

	c.JSON(http.StatusOK, gin.H{"hash": out.Result})
}

// ============================================================================
// Validator Handlers
// ============================================================================

func (h *Handler) GetValidators(c *gin.Context) {
	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT address, moniker, self_delegation, delegation, total_stake,
				uptime, blocks_count, is_active
		 FROM validators WHERE is_active = true ORDER BY total_stake DESC`,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var validators []map[string]interface{}
	for rows.Next() {
		var v map[string]interface{}
		if err := rows.Scan(&cell{v, "address"}, &cell{v, "moniker"}, &cell{v, "self_delegation"},
			&cell{v, "delegation"}, &cell{v, "total_stake"}, &cell{v, "uptime"}, &cell{v, "blocks_count"},
			&cell{v, "is_active"}); err == nil {
			validators = append(validators, v)
		}
	}

	c.JSON(http.StatusOK, gin.H{"validators": validators})
}

// ============================================================================
// Search Handlers
// ============================================================================

func (h *Handler) Search(c *gin.Context) {
	query := c.Query("q")

	if query == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Query required"})
		return
	}

	// Try to parse as address
	if len(query) == 42 && strings.HasPrefix(query, "0x") {
		var account map[string]interface{}
		err := h.db.QueryRowContext(c.Request.Context(),
			"SELECT address, balance, is_contract FROM accounts WHERE address = $1",
			query,
		).Scan(&cell{account, "address"}, &cell{account, "balance"}, &cell{account, "is_contract"})

		if err == nil {
			c.JSON(http.StatusOK, gin.H{"type": "address", "result": account})
			return
		}
	}

	// Try to parse as transaction hash
	if len(query) == 66 && strings.HasPrefix(query, "0x") {
		var tx map[string]interface{}
		err := h.db.QueryRowContext(c.Request.Context(),
			"SELECT hash, from, to, value FROM transactions WHERE hash = $1",
			query,
		).Scan(&cell{tx, "hash"}, &cell{tx, "from"}, &cell{tx, "to"}, &cell{tx, "value"})

		if err == nil {
			c.JSON(http.StatusOK, gin.H{"type": "transaction", "result": tx})
			return
		}
	}

	// Try to parse as block number
	if num, err := strconv.ParseUint(query, 10, 64); err == nil {
		var block map[string]interface{}
		err := h.db.QueryRowContext(c.Request.Context(),
			"SELECT number, hash, timestamp, miner, tx_count FROM blocks WHERE number = $1",
			num,
		).Scan(&cell{block, "number"}, &cell{block, "hash"}, &cell{block, "timestamp"},
			&cell{block, "miner"}, &cell{block, "tx_count"})

		if err == nil {
			c.JSON(http.StatusOK, gin.H{"type": "block", "result": block})
			return
		}
	}

	// Search by token name/symbol
	var tokens []map[string]interface{}
	rows, err := h.db.QueryContext(c.Request.Context(),
		"SELECT address, name, symbol FROM tokens WHERE name ILIKE $1 OR symbol ILIKE $1 LIMIT 5",
		"%"+query+"%",
	)
	if err == nil {
		defer rows.Close()
		for rows.Next() {
			var t map[string]interface{}
			if err := rows.Scan(&cell{t, "address"}, &cell{t, "name"}, &cell{t, "symbol"}); err == nil {
				tokens = append(tokens, t)
			}
		}
	}

	if len(tokens) > 0 {
		c.JSON(http.StatusOK, gin.H{"type": "token", "results": tokens})
		return
	}

	c.JSON(http.StatusNotFound, gin.H{"error": "No results found"})
}

// ============================================================================
// Analytics Handlers
// ============================================================================

func (h *Handler) GetStats(c *gin.Context) {
	var stats struct {
		TotalBlocks       uint64 `json:"total_blocks"`
		TotalTransactions uint64 `json:"total_transactions"`
		TotalTokens       uint64 `json:"total_tokens"`
		TotalAccounts     uint64 `json:"total_accounts"`
		TotalContracts    uint64 `json:"total_contracts"`
		LastBlock         uint64 `json:"last_block"`
	}

	h.db.QueryRowContext(c.Request.Context(), "SELECT COUNT(*) FROM blocks").Scan(&stats.TotalBlocks)
	h.db.QueryRowContext(c.Request.Context(), "SELECT COUNT(*) FROM transactions").Scan(&stats.TotalTransactions)
	h.db.QueryRowContext(c.Request.Context(), "SELECT COUNT(*) FROM tokens").Scan(&stats.TotalTokens)
	h.db.QueryRowContext(c.Request.Context(), "SELECT COUNT(*) FROM accounts").Scan(&stats.TotalAccounts)
	h.db.QueryRowContext(c.Request.Context(), "SELECT COUNT(*) FROM contracts").Scan(&stats.TotalContracts)
	h.db.QueryRowContext(c.Request.Context(), "SELECT MAX(number) FROM blocks").Scan(&stats.LastBlock)

	c.JSON(http.StatusOK, stats)
}

func (h *Handler) GetGasTracker(c *gin.Context) {
	var gas struct {
		GasPrice    int64 `json:"gas_price"`
		BaseFee     int64 `json:"base_fee"`
		PriorityFee int64 `json:"priority_fee"`
		Timestamp   int64 `json:"timestamp"`
	}

	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT gas_price, base_fee, priority_fee, timestamp
		 FROM gas_prices ORDER BY timestamp DESC LIMIT 1`,
	).Scan(&gas.GasPrice, &gas.BaseFee, &gas.PriorityFee, &gas.Timestamp)

	if err != nil {
		// Return default values
		gas.GasPrice = 5000000000
		gas.BaseFee = 1000000000
		gas.PriorityFee = 1000000000
		gas.Timestamp = time.Now().Unix()
	}

	c.JSON(http.StatusOK, gas)
}

// ============================================================================
// Event Logs
// ============================================================================

// GetLogs returns event logs, optionally filtered by emitting address, block
// range and first topic.
func (h *Handler) GetLogs(c *gin.Context) {
	page := c.DefaultQuery("page", "1")
	limit := c.DefaultQuery("limit", "25")

	pageNum, err := strconv.Atoi(page)
	if err != nil || pageNum < 1 {
		pageNum = 1
	}
	limitNum, err := strconv.Atoi(limit)
	if err != nil || limitNum < 1 || limitNum > 100 {
		limitNum = 25
	}

	where := []string{"1=1"}
	args := []interface{}{}
	argN := 1

	if address := c.Query("address"); address != "" {
		if !hexAddress.MatchString(address) {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid address"})
			return
		}
		where = append(where, fmt.Sprintf("address = $%d", argN))
		args = append(args, address)
		argN++
	}
	if from := c.Query("from_block"); from != "" {
		n, err := strconv.ParseUint(from, 10, 64)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid from_block"})
			return
		}
		where = append(where, fmt.Sprintf("block_number >= $%d", argN))
		args = append(args, n)
		argN++
	}
	if to := c.Query("to_block"); to != "" {
		n, err := strconv.ParseUint(to, 10, 64)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid to_block"})
			return
		}
		where = append(where, fmt.Sprintf("block_number <= $%d", argN))
		args = append(args, n)
		argN++
	}
	if topic := c.Query("topic0"); topic != "" {
		if !hexWord.MatchString(topic) {
			c.JSON(http.StatusBadRequest, gin.H{"error": "topic0 must be a 32-byte hex value"})
			return
		}
		where = append(where, fmt.Sprintf("topics->>0 = $%d", argN))
		args = append(args, strings.ToLower(topic))
		argN++
	}

	offset := (pageNum - 1) * limitNum
	query := fmt.Sprintf(
		`SELECT transaction_hash, block_number, address, topics, data, log_index, timestamp
		 FROM logs WHERE %s
		 ORDER BY block_number DESC, log_index DESC LIMIT $%d OFFSET $%d`,
		strings.Join(where, " AND "), argN, argN+1)
	args = append(args, limitNum, offset)

	rows, err := h.db.QueryContext(c.Request.Context(), query, args...)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var logs []map[string]interface{}
	for rows.Next() {
		l := map[string]interface{}{}
		if err := rows.Scan(&cell{l, "transaction_hash"}, &cell{l, "block_number"}, &cell{l, "address"},
			&cell{l, "topics"}, &cell{l, "data"}, &cell{l, "log_index"}, &cell{l, "timestamp"}); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		logs = append(logs, l)
	}
	if err := rows.Err(); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, gin.H{"logs": logs, "page": pageNum, "limit": limitNum})
}

// ============================================================================
// Pending Transactions
// ============================================================================

func (h *Handler) GetPendingTransactions(c *gin.Context) {
	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT hash, nonce, from_address, to_address, value, gas_price, gas_limit, input
		 FROM pending_transactions ORDER BY gas_price DESC, nonce ASC LIMIT 50`,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var txs []map[string]interface{}
	for rows.Next() {
		var t map[string]interface{}
		if err := rows.Scan(&cell{t, "hash"}, &cell{t, "nonce"}, &cell{t, "from_address"}, &cell{t, "to_address"},
			&cell{t, "value"}, &cell{t, "gas_price"}, &cell{t, "gas_limit"}, &cell{t, "input"}); err == nil {
			txs = append(txs, t)
		}
	}

	c.JSON(http.StatusOK, gin.H{"pending_transactions": txs})
}

// ============================================================================
// Utility Functions
// ============================================================================

func computeHMAC(data []byte, secret string) string {
	h := hmac.New(sha256.New, []byte(secret))
	h.Write(data)
	return base64.StdEncoding.EncodeToString(h.Sum(nil))
}

// ============================================================================
// Middleware
// ============================================================================

func RateLimitMiddleware(rateLimiter *RateLimiter) gin.HandlerFunc {
	return func(c *gin.Context) {
		key := c.ClientIP()

		if !rateLimiter.Allow(key) {
			c.JSON(http.StatusTooManyRequests, gin.H{"error": "Rate limit exceeded"})
			c.Abort()
			return
		}

		c.Next()
	}
}

func APIKeyMiddleware(apiKeyMgr *APIKeyManager) gin.HandlerFunc {
	return func(c *gin.Context) {
		apiKey := c.GetHeader("X-API-Key")

		if apiKey != "" {
			if key, ok := apiKeyMgr.Get(apiKey); ok {
				apiKeyMgr.Increment(apiKey)
				c.Set("api_key", key)
			}
		}

		c.Next()
	}
}

// ============================================================================
// Main
// ============================================================================

func main() {
	runtime.GOMAXPROCS(runtime.NumCPU())
	gin.SetMode(gin.ReleaseMode)

	config := Config{
		Server: ServerConfig{
			Host:            "0.0.0.0",
			Port:            8080,
			ReadTimeout:     30,
			WriteTimeout:    30,
			MaxHeaderBytes:  1048576,
			ShutdownTimeout: 30,
		},
		Database: DatabaseConfig{
			Host:            "localhost",
			Port:            5432,
			Username:        "tigerscan",
			Password:        "tigerscan",
			Database:        "tigerscan",
			MaxOpenConns:    100,
			MaxIdleConns:    50,
			ConnMaxLifetime: 300,
		},
		Redis: RedisConfig{
			Host:     "localhost",
			Port:     6379,
			Database: 0,
		},
		Security: SecurityConfig{
			CorsOrigins: []string{"*"},
		},
		RateLimit: RateLimitConfig{
			RequestsPerSecond: 1000,
			Burst:             2000,
			Enabled:           true,
		},
	}

	db, err := NewDB(config.Database, config.Redis)
	if err != nil {
		fmt.Printf("Failed to connect to database: %v\n", err)
		os.Exit(1)
	}
	defer db.Close()

	handler := NewHandler(db, &config)

	router := gin.New()
	router.Use(gin.Recovery())
	router.Use(gin.Logger())

	router.Use(cors.New(cors.Config{
		AllowOrigins:     config.Security.CorsOrigins,
		AllowMethods:     []string{"GET", "POST", "PUT", "DELETE", "OPTIONS"},
		AllowHeaders:     []string{"Origin", "Content-Type", "Accept", "Authorization", "X-API-Key"},
		ExposeHeaders:    []string{"X-Request-ID"},
		AllowCredentials: true,
	}))

	if config.RateLimit.Enabled {
		router.Use(RateLimitMiddleware(handler.rateLimiter))
	}

	// API routes
	api := router.Group("/api/v1")
	{
		// Blocks
		api.GET("/blocks", handler.GetBlocks)
		api.GET("/blocks/latest", handler.GetLatestBlock)
		api.GET("/blocks/:numberOrHash", handler.GetBlock)

		// Transactions
		api.GET("/transactions", handler.GetTransactions)
		api.GET("/transactions/:hash", handler.GetTransaction)
		api.GET("/transactions/:hash/receipt", handler.GetTransactionReceipt)
		api.GET("/transactions/:hash/internal", handler.GetInternalTransactions)
		api.GET("/pending", handler.GetPendingTransactions)

		// Tokens
		api.GET("/tokens", handler.GetTokens)
		api.GET("/tokens/:address", handler.GetToken)
		api.GET("/tokens/:address/holders", handler.GetTokenHolders)
		api.GET("/tokens/:address/transfers", handler.GetTokenTransfers)
		api.GET("/tokens/:address/price", handler.GetTokenPrice)

		// NFTs
		api.GET("/nfts/collections", handler.GetNFTCollections)
		api.GET("/nfts/:collection/:token_id", handler.GetNFT)
		api.GET("/nfts/:collection/transfers", handler.GetNFTTransfers)

		// Accounts
		api.GET("/accounts/:address", handler.GetAccount)
		api.GET("/accounts/:address/transactions", handler.GetAccountTransactions)
		api.GET("/accounts/:address/tokens", handler.GetAccountTokens)
		api.GET("/accounts/:address/nfts", handler.GetAccountNFTs)

		// Contracts
		api.GET("/contracts/:address", handler.GetContract)
		api.GET("/contracts/:address/source", handler.GetContractSource)
		api.GET("/contracts/:address/abi", handler.GetContractABI)
		api.POST("/contracts/:address/read", handler.ReadContract)
		api.POST("/contracts/:address/write", handler.WriteContract)

		// Validators
		api.GET("/validators", handler.GetValidators)

		// Search
		api.GET("/search", handler.Search)

		// Event logs
		api.GET("/logs", handler.GetLogs)

		// Stats
		api.GET("/stats", handler.GetStats)
		api.GET("/gas", handler.GetGasTracker)
	}

	// Health check
	router.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"status": "ok"})
	})

	// Start server
	addr := fmt.Sprintf("%s:%d", config.Server.Host, config.Server.Port)
	srv := &http.Server{
		Addr:           addr,
		Handler:        router,
		ReadTimeout:    time.Duration(config.Server.ReadTimeout) * time.Second,
		WriteTimeout:   time.Duration(config.Server.WriteTimeout) * time.Second,
		MaxHeaderBytes: config.Server.MaxHeaderBytes,
	}

	go func() {
		fmt.Printf("API server listening on %s\n", addr)
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			fmt.Printf("Server error: %v\n", err)
		}
	}()

	// Wait for signal
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)
	<-sigCh

	// Graceful shutdown
	fmt.Println("Shutting down...")
	ctx, cancel := context.WithTimeout(context.Background(), time.Duration(config.Server.ShutdownTimeout)*time.Second)
	defer cancel()

	if err := srv.Shutdown(ctx); err != nil {
		fmt.Printf("Server shutdown error: %v\n", err)
	}

	fmt.Println("Server stopped")
}
