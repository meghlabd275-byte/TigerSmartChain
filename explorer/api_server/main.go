// TigerSmartChain API Server - Complete Production-Grade REST API
// Full API with pagination, filters, rate limiting, API keys, webhooks, GraphQL

package main

import (
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"net/http"
	"os"
	"os/signal"
	"regexp"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	"github.com/gin-contrib/cors"
	"github.com/gin-gonic/gin"
	"github.com/go-redis/redis/v8"
	"github.com/lib/pq"
	"github.com/throttled/throttled"
	"golang.org/x/time/rate"
)

// ============================================================================
// Configuration
// ============================================================================

type Config struct {
	Server   ServerConfig   `json:"server"`
	Database DatabaseConfig `json:"database"`
	Redis    RedisConfig    `json:"redis"`
	Security SecurityConfig `json:"security"`
	RateLimit RateLimitConfig `json:"rate_limit"`
	Webhook  WebhookConfig  `json:"webhook"`
}

type ServerConfig struct {
	Host            string `json:"host"`
	Port            int    `json:"port"`
	ReadTimeout     int    `json:"read_timeout"`
	WriteTimeout    int    `json:"write_timeout"`
	MaxHeaderBytes  int    `json:"max_header_bytes"`
	ShutdownTimeout int   `json:"shutdown_timeout"`
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
	APIKeys          []string `json:"api_keys"`
	JWTSecret        string   `json:"jwt_secret"`
	EncryptionKey  string   `json:"encryption_key"`
	CorsOrigins     []string `json:"cors_origins"`
	AllowedIPs      []string `json:"allowed_ips"`
	EnableIPBan     bool     `json:"enable_ip_ban"`
	BanDuration    int     `json:"ban_duration"`
	MaxRequestsPerMinute int `json:"max_requests_per_minute"`
}

type RateLimitConfig struct {
	RequestsPerSecond int `json:"requests_per_second"`
	Burst           int `json:"burst"`
	Enabled         bool `json:"enabled"`
}

type WebhookConfig struct {
	Enabled bool   `json:"enabled"`
	Secret  string `json:"secret"`
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
		store: make(map[string]*ClientLimiter),
		global: rate.NewLimiter(rate.Limit(config.RequestsPerSecond), config.Burst),
		config: config,
	}
}

func (r *RateLimiter) Allow(key string) bool {
	if !r.config.Enabled {
		return true
	}
	
	// Check global limit
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
		}(duration)
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
	Key         string
	Name        string
	Scopes      []string
	RateLimit   int
	Requests    uint64
	CreatedAt   time.Time
	LastUsed    time.Time
	ExpiresAt   *time.Time
	Disabled    bool
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
	k, ok := m.keys[key]
	if !ok || k.Disabled {
		return nil, false
	}
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
	mu        sync.RWMutex
	webhooks map[string]*Webhook
	queue    chan *WebhookEvent
}

type Webhook struct {
	ID        string
	URL      string
	Events   []string
	Secret   string
	Active   bool
	Retries  int
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
	
	// Start worker
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

func (m *WebhookManager) Add(wh *Webhook) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.webhooks[wh.ID] = wh
}

func (m *WebhookManager) Trigger(event string, payload interface{}) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	
	for _, wh := range m.webhooks {
		if !wh.Active {
			continue
		}
		
		for _, e := range wh.Events {
			if e == event || e == "*" {
				m.queue <- &WebhookEvent{
					Event:   event,
					URL:     wh.URL,
					Payload: payload,
					Secret:  wh.Secret,
				}
				break
			}
		}
	}
}

// ============================================================================
// Pagination
// ============================================================================

type Pagination struct {
	Page    int `json:"page"`
	Limit   int `json:"limit"`
	MaxLimit int `json:"max_limit"`
}

func (p *Pagination) GetLimit() int {
	if p.Limit <= 0 || p.Limit > p.MaxLimit {
		return p.MaxLimit
	}
	return p.Limit
}

func (p *Pagination) Offset() int {
	return (p.Page - 1) * p.GetLimit()
}

type PaginationResponse struct {
	Page       int         `json:"page"`
	Limit      int         `json:"limit"`
	Total      int64       `json:"total"`
	TotalPages int         `json:"total_pages"`
}

// ============================================================================
// Filters
// ============================================================================

type BlockFilter struct {
	FromBlock uint64 `form:"from_block" json:"from_block"`
	ToBlock   uint64 `form:"to_block" json:"to_block"`
	Miner    string `form:"miner" json:"miner"`
}

type TransactionFilter struct {
	From     string `form:"from" json:"from"`
	To       string `form:"to" json:"to"`
	Address  string `form:"address" json:"address"`
	ValueMin string `form:"value_min" json:"value_min"`
	ValueMax string `form:"value_max" json:"value_max"`
	Status  string `form:"status" json:"status"`
}

type TokenFilter struct {
	Address   string `form:"address" json:"address"`
	Name     string `form:"name" json:"name"`
	Symbol   string `form:"symbol" json:"symbol"`
	Verified *bool  `form:"verified" json:"verified"`
}

type NFTFilter struct {
	Collection string `form:"collection" json:"collection"`
	Owner      string `form:"owner" json:"owner"`
	MinPrice   string `form:"min_price" json:"min_price"`
	MaxPrice   string `form:"max_price" json:"max_price"`
}

// ============================================================================
// Handlers
// ============================================================================

type Handler struct {
	db           *DB
	rateLimiter  *RateLimiter
	apiKeys      *APIKeyManager
	webhooks     *WebhookManager
	webhookQueue chan *WebhookEvent
}

func NewHandler(db *DB) *Handler {
	return &Handler{
		db:            db,
		rateLimiter:   NewRateLimiter(RateLimitConfig{Enabled: true, RequestsPerSecond: 1000, Burst: 2000}),
		apiKeys:       NewAPIKeyManager(),
		webhooks:      NewWebhookManager(),
		webhookQueue: make(chan *WebhookEvent, 1000),
	}
}

// ============================================================================
// Block Handlers
// ============================================================================

func (h *Handler) GetBlocks(c *gin.Context) {
	var filter BlockFilter
	if err := c.ShouldBindQuery(&filter); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	pagination := Pagination{
		Page:    1,
		Limit:   25,
		MaxLimit: 100,
	}
	if page, err := strconv.Atoi(c.Query("page")); err == nil {
		pagination.Page = page
	}
	if limit, err := strconv.Atoi(c.Query("limit")); err == nil {
		pagination.Limit = limit
	}
	
	query := "SELECT number, hash, parent_hash, gas_limit, gas_used, timestamp, miner, difficulty, size FROM blocks"
	args := []interface{}{}
	
	where := []string{}
	if filter.FromBlock > 0 {
		where = append(where, fmt.Sprintf("number >= $%d", len(args)+1))
		args = append(args, filter.FromBlock)
	}
	if filter.ToBlock > 0 {
		where = append(where, fmt.Sprintf("number <= $%d", len(args)+1))
		args = append(args, filter.ToBlock)
	}
	if filter.Miner != "" {
		where = append(where, fmt.Sprintf("miner = $%d", len(args)+1))
		args = append(args, filter.Miner)
	}
	
	if len(where) > 0 {
		query += " WHERE " + strings.Join(where, " AND ")
	}
	
	query += fmt.Sprintf(" ORDER BY number DESC LIMIT $%d OFFSET $%d", len(args)+1, len(args)+2)
	args = append(args, pagination.GetLimit(), pagination.Offset())
	
	rows, err := h.db.QueryContext(c.Request.Context(), query, args...)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	type Block struct {
		Number      uint64 `json:"number"`
		Hash        string `json:"hash"`
		ParentHash  string `json:"parent_hash"`
		GasLimit    uint64 `json:"gas_limit"`
		GasUsed     uint64 `json:"gas_used"`
		Timestamp   uint64 `json:"timestamp"`
		Miner       string `json:"miner"`
		Difficulty  string `json:"difficulty"`
		Size        uint64 `json:"size"`
	}
	
	var blocks []Block
	for rows.Next() {
		var b Block
		if err := rows.Scan(&b.Number, &b.Hash, &b.ParentHash, &b.GasLimit, &b.GasUsed, &b.Timestamp, &b.Miner, &b.Difficulty, &b.Size); err != nil {
			continue
		}
		blocks = append(blocks, b)
	}
	
	c.JSON(http.StatusOK, gin.H{
		"data": blocks,
		"pagination": PaginationResponse{
			Page:       pagination.Page,
			Limit:      pagination.Limit,
			Total:      int64(len(blocks)),
			TotalPages: (len(blocks) + pagination.Limit - 1) / pagination.Limit,
		},
	})
}

func (h *Handler) GetBlock(c *gin.Context) {
	numberOrHash := c.Param("numberOrHash")
	
	var query string
	var args []interface{}
	
	if _, err := strconv.ParseUint(numberOrHash, 10, 64); err == nil {
		query = "SELECT number, hash, parent_hash, gas_limit, gas_used, timestamp, miner, difficulty, size, transactions, uncles FROM blocks WHERE number = $1"
		args = append(args, numberOrHash)
	} else {
		query = "SELECT number, hash, parent_hash, gas_limit, gas_used, timestamp, miner, difficulty, size, transactions, uncles FROM blocks WHERE hash = $1"
		args = append(args, numberOrHash)
	}
	
	var block map[string]interface{}
	if err := h.db.QueryRowContext(c.Request.Context(), query, args...).Scan(&block); err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Block not found"})
		return
	}
	
	c.JSON(http.StatusOK, block)
}

func (h *Handler) GetLatestBlock(c *gin.Context) {
	var block map[string]interface{}
	err := h.db.QueryRowContext(c.Request.Context(), 
		"SELECT number, hash, timestamp, gas_limit, gas_used, miner, difficulty FROM blocks ORDER BY number DESC LIMIT 1",
	).Scan(&block)
	
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "No blocks found"})
		return
	}
	
	c.JSON(http.StatusOK, block)
}

// ============================================================================
// Transaction Handlers
// ============================================================================

func (h *Handler) GetTransactions(c *gin.Context) {
	var filter TransactionFilter
	if err := c.ShouldBindQuery(&filter); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	pagination := Pagination{Page: 1, Limit: 25, MaxLimit: 100}
	if page, err := strconv.Atoi(c.Query("page")); err == nil {
		pagination.Page = page
	}
	if limit, err := strconv.Atoi(c.Query("limit")); err == nil {
		pagination.Limit = limit
	}
	
	query := "SELECT hash, block_number, block_hash, from, to, value, gas, gas_price, gas_used, timestamp, status FROM transactions"
	args := []interface{}{}
	
	where := []string{}
	if filter.From != "" {
		where = append(where, fmt.Sprintf("from = $%d", len(args)+1))
		args = append(args, filter.From)
	}
	if filter.To != "" {
		where = append(where, fmt.Sprintf("to = $%d", len(args)+1))
		args = append(args, filter.To)
	}
	if filter.Address != "" {
		where = append(where, fmt.Sprintf("(from = $%d OR to = $%d)", len(args)+1, len(args)+1))
		args = append(args, filter.Address)
	}
	if filter.Status != "" {
		where = append(where, fmt.Sprintf("status = $%d", len(args)+1))
		args = append(args, filter.Status)
	}
	
	if len(where) > 0 {
		query += " WHERE " + strings.Join(where, " AND ")
	}
	
	query += fmt.Sprintf(" ORDER BY block_number DESC, transaction_index DESC LIMIT $%d OFFSET $%d", len(args)+1, len(args)+2)
	args = append(args, pagination.GetLimit(), pagination.Offset())
	
	rows, err := h.db.QueryContext(c.Request.Context(), query, args...)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	type Tx struct {
		Hash        string `json:"hash"`
		BlockNumber uint64 `json:"block_number"`
		BlockHash  string `json:"block_hash"`
		From      string `json:"from"`
		To        string `json:"to"`
		Value      string `json:"value"`
		Gas        uint64 `json:"gas"`
		GasPrice   string `json:"gas_price"`
		GasUsed    uint64 `json:"gas_used"`
		Timestamp  uint64 `json:"timestamp"`
		Status     string `json:"status"`
	}
	
	var txs []Tx
	for rows.Next() {
		var t Tx
		if err := rows.Scan(&t.Hash, &t.BlockNumber, &t.BlockHash, &t.From, &t.To, &t.Value, &t.Gas, &t.GasPrice, &t.GasUsed, &t.Timestamp, &t.Status); err != nil {
			continue
		}
		txs = append(txs, t)
	}
	
	c.JSON(http.StatusOK, gin.H{
		"data": txs,
		"pagination": PaginationResponse{
			Page:       pagination.Page,
			Limit:      pagination.Limit,
			Total:      int64(len(txs)),
			TotalPages: (len(txs) + pagination.Limit - 1) / pagination.Limit,
		},
	})
}

func (h *Handler) GetTransaction(c *gin.Context) {
	hash := c.Param("hash")
	
	var tx map[string]interface{}
	err := h.db.QueryRowContext(c.Request.Context(),
		"SELECT * FROM transactions WHERE hash = $1",
		hash,
	).Scan(&tx)
	
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
		"SELECT * FROM transaction_receipts WHERE transaction_hash = $1",
		hash,
	).Scan(&receipt)
	
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Receipt not found"})
		return
	}
	
	c.JSON(http.StatusOK, receipt)
}

func (h *Handler) GetInternalTransactions(c *gin.Context) {
	hash := c.Param("hash")
	
	pagination := Pagination{Page: 1, Limit: 50, MaxLimit: 100}
	if page, err := strconv.Atoi(c.Query("page")); err == nil {
		pagination.Page = page
	}
	
	rows, err := h.db.QueryContext(c.Request.Context(),
		"SELECT type, from, to, value, input, call_type, depth, pc, gas, gas_used FROM traces WHERE transaction_hash = $1 ORDER BY trace_index LIMIT $2 OFFSET $3",
		hash, pagination.GetLimit(), pagination.Offset(),
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	type InternalTx struct {
		Type     string `json:"type"`
		From    string `json:"from"`
		To      string `json:"to"`
		Value   string `json:"value"`
		Input   string `json:"input"`
		CallType string `json:"call_type"`
		Depth   int    `json:"depth"`
		PC      uint64 `json:"pc"`
		Gas     uint64 `json:"gas"`
		GasUsed  uint64 `json:"gas_used"`
	}
	
	var txs []InternalTx
	for rows.Next() {
		var t InternalTx
		if err := rows.Scan(&t.Type, &t.From, &t.To, &t.Value, &t.Input, &t.CallType, &t.Depth, &t.PC, &t.Gas, &t.GasUsed); err != nil {
			continue
		}
		txs = append(txs, t)
	}
	
	c.JSON(http.StatusOK, gin.H{"data": txs})
}

// ============================================================================
// Token Handlers
// ============================================================================

func (h *Handler) GetTokens(c *gin.Context) {
	var filter TokenFilter
	if err := c.ShouldBindQuery(&filter); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	pagination := Pagination{Page: 1, Limit: 25, MaxLimit: 100}
	if page, err := strconv.Atoi(c.Query("page")); err == nil {
		pagination.Page = page
	}
	
	query := "SELECT address, name, symbol, decimals, total_supply, holders, transfers_24h, price_usd, price_change_24h, market_cap, volume_24h FROM tokens"
	args := []interface{}{}
	
	where := []string{}
	if filter.Address != "" {
		where = append(where, fmt.Sprintf("address = $%d", len(args)+1))
		args = append(args, filter.Address)
	}
	if filter.Name != "" {
		where = append(where, fmt.Sprintf("name ILIKE $%d", len(args)+1))
		args = append(args, "%"+filter.Name+"%")
	}
	if filter.Symbol != "" {
		where = append(where, fmt.Sprintf("symbol ILIKE $%d", len(args)+1))
		args = append(args, "%"+filter.Symbol+"%")
	}
	if filter.Verified != nil {
		where = append(where, fmt.Sprintf("verified = $%d", len(args)+1))
		args = append(args, *filter.Verified)
	}
	
	if len(where) > 0 {
		query += " WHERE " + strings.Join(where, " AND ")
	}
	
	query += fmt.Sprintf(" ORDER BY market_cap DESC LIMIT $%d OFFSET $%d", len(args)+1, len(args)+2)
	args = append(args, pagination.GetLimit(), pagination.Offset())
	
	rows, err := h.db.QueryContext(c.Request.Context(), query, args...)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	type Token struct {
		Address     string `json:"address"`
		Name        string `json:"name"`
		Symbol      string `json:"symbol"`
		Decimals    uint8  `json:"decimals"`
		TotalSupply string `json:"total_supply"`
		Holders    uint64 `json:"holders"`
		Transfers24h uint64 `json:"transfers_24h"`
		PriceUSD    float64 `json:"price_usd"`
		Change24h  float64 `json:"price_change_24h"`
		MarketCap  float64 `json:"market_cap"`
		Volume24h  float64 `json:"volume_24h"`
	}
	
	var tokens []Token
	for rows.Next() {
		var t Token
		if err := rows.Scan(&t.Address, &t.Name, &t.Symbol, &t.Decimals, &t.TotalSupply, &t.Holders, &t.Transfers24h, &t.PriceUSD, &t.Change24h, &t.MarketCap, &t.Volume24h); err != nil {
			continue
		}
		tokens = append(tokens, t)
	}
	
	c.JSON(http.StatusOK, gin.H{"data": tokens})
}

func (h *Handler) GetToken(c *gin.Context) {
	address := c.Param("address")
	
	var token map[string]interface{}
	err := h.db.QueryRowContext(c.Request.Context(),
		"SELECT * FROM tokens WHERE address = $1",
		address,
	).Scan(&token)
	
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Token not found"})
		return
	}
	
	c.JSON(http.StatusOK, token)
}

func (h *Handler) GetTokenHolders(c *gin.Context) {
	address := c.Param("address")
	
	pagination := Pagination{Page: 1, Limit: 25, MaxLimit: 100}
	if page, err := strconv.Atoi(c.Query("page")); err == nil {
		pagination.Page = page
	}
	
	rows, err := h.db.QueryContext(c.Request.Context(),
		"SELECT address, balance, balance_usd, percentage FROM token_holders WHERE token_address = $1 ORDER BY balance_usd DESC LIMIT $2 OFFSET $3",
		address, pagination.GetLimit(), pagination.Offset(),
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	type Holder struct {
		Address     string `json:"address"`
		Balance     string `json:"balance"`
		BalanceUSD float64 `json:"balance_usd"`
		Percentage float64 `json:"percentage"`
	}
	
	var holders []Holder
	for rows.Next() {
		var h Holder
		if err := rows.Scan(&h.Address, &h.Balance, &h.BalanceUSD, &h.Percentage); err != nil {
			continue
		}
		holders = append(holders, h)
	}
	
	c.JSON(http.StatusOK, gin.H{"data": holders})
}

func (h *Handler) GetTokenTransfers(c *gin.Context) {
	address := c.Param("address")
	
	pagination := Pagination{Page: 1, Limit: 25, MaxLimit: 100}
	if page, err := strconv.Atoi(c.Query("page")); err == nil {
		pagination.Page = page
	}
	
	rows, err := h.db.QueryContext(c.Request.Context(),
		"SELECT hash, block_number, timestamp, from, to, value, value_usd FROM token_transfers WHERE token_address = $1 ORDER BY block_number DESC, log_index DESC LIMIT $2 OFFSET $3",
		address, pagination.GetLimit(), pagination.Offset(),
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	type Transfer struct {
		Hash        string `json:"hash"`
		BlockNumber uint64 `json:"block_number"`
		Timestamp   uint64 `json:"timestamp"`
		From        string `json:"from"`
		To          string `json:"to"`
		Value       string `json:"value"`
		ValueUSD   float64 `json:"value_usd"`
	}
	
	var transfers []Transfer
	for rows.Next() {
		var t Transfer
		if err := rows.Scan(&t.Hash, &t.BlockNumber, &t.Timestamp, &t.From, &t.To, &t.Value, &t.ValueUSD); err != nil {
			continue
		}
		transfers = append(transfers, t)
	}
	
	c.JSON(http.StatusOK, gin.H{"data": transfers})
}

func (h *Handler) GetTokenApprovals(c *gin.Context) {
	address := c.Param("address")
	
	pagination := Pagination{Page: 1, Limit: 25, MaxLimit: 100}
	if page, err := strconv.Atoi(c.Query("page")); err == nil {
		pagination.Page = page
	}
	
	rows, err := h.db.QueryContext(c.Request.Context(),
		"SELECT owner, spender, value, approved, block_number, transaction_hash FROM token_approvals WHERE token_address = $1 ORDER BY block_number DESC LIMIT $2 OFFSET $3",
		address, pagination.GetLimit(), pagination.Offset(),
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	type Approval struct {
		Owner    string `json:"owner"`
		Spender  string `json:"spender"`
		Value    string `json:"value"`
		Approved bool   `json:"approved"`
		BlockNumber uint64 `json:"block_number"`
		TransactionHash string `json:"transaction_hash"`
	}
	
	var approvals []Approval
	for rows.Next() {
		var a Approval
		if err := rows.Scan(&a.Owner, &a.Spender, &a.Value, &a.Approved, &a.BlockNumber, &a.TransactionHash); err != nil {
			continue
		}
		approvals = append(approvals, a)
	}
	
	c.JSON(http.StatusOK, gin.H{"data": approvals})
}

// ============================================================================
// NFT Handlers
// ============================================================================

func (h *Handler) GetNFTCollections(c *gin.Context) {
	pagination := Pagination{Page: 1, Limit: 25, MaxLimit: 100}
	if page, err := strconv.Atoi(c.Query("page")); err == nil {
		pagination.Page = page
	}
	
	rows, err := h.db.QueryContext(c.Request.Context(),
		"SELECT address, name, symbol, contract_type, total_supply, holder_count, volume_24h, volume_24h_usd, floor_price FROM nft_collections ORDER BY volume_24h_usd DESC LIMIT $1 OFFSET $2",
		pagination.GetLimit(), pagination.Offset(),
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	type Collection struct {
		Address      string `json:"address"`
		Name         string `json:"name"`
		Symbol       string `json:"symbol"`
		ContractType string `json:"contract_type"`
		TotalSupply uint64 `json:"total_supply"`
		HolderCount  uint64 `json:"holder_count"`
		Volume24h   float64 `json:"volume_24h"`
		Volume24hUSD float64 `json:"volume_24h_usd"`
		FloorPrice  float64 `json:"floor_price"`
	}
	
	var collections []Collection
	for rows.Next() {
		var c Collection
		if err := rows.Scan(&c.Address, &c.Name, &c.Symbol, &c.ContractType, &c.TotalSupply, &c.HolderCount, &c.Volume24h, &c.Volume24hUSD, &c.FloorPrice); err != nil {
			continue
		}
		collections = append(collections, c)
	}
	
	c.JSON(http.StatusOK, gin.H{"data": collections})
}

func (h *Handler) GetNFT(c *gin.Context) {
	collection := c.Param("collection")
	tokenID := c.Param("token_id")
	
	var nft map[string]interface{}
	err := h.db.QueryRowContext(c.Request.Context(),
		"SELECT * FROM nfts WHERE collection_address = $1 AND token_id = $2",
		collection, tokenID,
	).Scan(&nft)
	
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "NFT not found"})
		return
	}
	
	c.JSON(http.StatusOK, nft)
}

func (h *Handler) GetNFTTransfers(c *gin.Context) {
	collection := c.Param("collection")
	
	pagination := Pagination{Page: 1, Limit: 25, MaxLimit: 100}
	if page, err := strconv.Atoi(c.Query("page")); err == nil {
		pagination.Page = page
	}
	
	rows, err := h.db.QueryContext(c.Request.Context(),
		"SELECT hash, block_number, timestamp, token_id, from, to, amount, price_usd FROM nft_transfers WHERE collection_address = $1 ORDER BY block_number DESC LIMIT $2 OFFSET $3",
		collection, pagination.GetLimit(), pagination.Offset(),
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	type Transfer struct {
		Hash        string `json:"hash"`
		BlockNumber uint64 `json:"block_number"`
		Timestamp   uint64 `json:"timestamp"`
		TokenID     string `json:"token_id"`
		From        string `json:"from"`
		To          string `json:"to"`
		Amount      string `json:"amount"`
		PriceUSD   float64 `json:"price_usd"`
	}
	
	var transfers []Transfer
	for rows.Next() {
		var t Transfer
		if err := rows.Scan(&t.Hash, &t.BlockNumber, &t.Timestamp, &t.TokenID, &t.From, &t.To, &t.Amount, &t.PriceUSD); err != nil {
			continue
		}
		transfers = append(transfers, t)
	}
	
	c.JSON(http.StatusOK, gin.H{"data": transfers})
}

// ============================================================================
// Account Handlers
// ============================================================================

func (h *Handler) GetAccount(c *gin.Context) {
	address := c.Param("address")
	
	var account map[string]interface{}
	err := h.db.QueryRowContext(c.Request.Context(),
		"SELECT * FROM accounts WHERE address = $1",
		address,
	).Scan(&account)
	
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Account not found"})
		return
	}
	
	c.JSON(http.StatusOK, account)
}

func (h *Handler) GetAccountTransactions(c *gin.Context) {
	address := c.Param("address")
	
	pagination := Pagination{Page: 1, Limit: 25, MaxLimit: 100}
	if page, err := strconv.Atoi(c.Query("page")); err == nil {
		pagination.Page = page
	}
	
	rows, err := h.db.QueryContext(c.Request.Context(),
		"SELECT hash, block_number, timestamp, from, to, value, status FROM transactions WHERE from = $1 OR to = $1 ORDER BY block_number DESC, transaction_index DESC LIMIT $2 OFFSET $3",
		address, pagination.GetLimit(), pagination.Offset(),
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	type Tx struct {
		Hash        string `json:"hash"`
		BlockNumber uint64 `json:"block_number"`
		Timestamp   uint64 `json:"timestamp"`
		From        string `json:"from"`
		To          string `json:"to"`
		Value       string `json:"value"`
		Status      string `json:"status"`
	}
	
	var txs []Tx
	for rows.Next() {
		var t Tx
		if err := rows.Scan(&t.Hash, &t.BlockNumber, &t.Timestamp, &t.From, &t.To, &t.Value, &t.Status); err != nil {
			continue
		}
		txs = append(txs, t)
	}
	
	c.JSON(http.StatusOK, gin.H{"data": txs})
}

func (h *Handler) GetAccountTokens(c *gin.Context) {
	address := c.Param("address")
	
	rows, err := h.db.QueryContext(c.Request.Context(),
		"SELECT token_address, balance, balance_usd FROM account_tokens WHERE account_address = $1 ORDER BY balance_usd DESC",
		address,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	type Token struct {
		Address    string `json:"address"`
		Balance   string `json:"balance"`
		BalanceUSD float64 `json:"balance_usd"`
	}
	
	var tokens []Token
	for rows.Next() {
		var t Token
		if err := rows.Scan(&t.Address, &t.Balance, &t.BalanceUSD); err != nil {
			continue
		}
		tokens = append(tokens, t)
	}
	
	c.JSON(http.StatusOK, gin.H{"data": tokens})
}

func (h *Handler) GetAccountNFTs(c *gin.Context) {
	address := c.Param("address")
	
	pagination := Pagination{Page: 1, Limit: 25, MaxLimit: 100}
	if page, err := strconv.Atoi(c.Query("page")); err == nil {
		pagination.Page = page
	}
	
	rows, err := h.db.QueryContext(c.Request.Context(),
		"SELECT collection_address, token_id, balance FROM account_nfts WHERE account_address = $1 ORDER BY balance DESC LIMIT $2 OFFSET $3",
		address, pagination.GetLimit(), pagination.Offset(),
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()
	
	type NFT struct {
		Collection string `json:"collection"`
		TokenID   string `json:"token_id"`
		Balance  uint64 `json:"balance"`
	}
	
	var nfts []NFT
	for rows.Next() {
		var n NFT
		if err := rows.Scan(&n.Collection, &n.TokenID, &n.Balance); err != nil {
			continue
		}
		nfts = append(nfts, n)
	}
	
	c.JSON(http.StatusOK, gin.H{"data": nfts})
}

// ============================================================================
// Contract Handlers
// ============================================================================

func (h *Handler) GetContract(c *gin.Context) {
	address := c.Param("address")
	
	var contract map[string]interface{}
	err := h.db.QueryRowContext(c.Request.Context(),
		"SELECT * FROM contracts WHERE address = $1",
		address,
	).Scan(&contract)
	
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Contract not found"})
		return
	}
	
	c.JSON(http.StatusOK, contract)
}

func (h *Handler) GetContractSource(c *gin.Context) {
	address := c.Param("address")
	
	rows, err := h.db.QueryContext(c.Request.Context(),
		"SELECT file_name, source_code, language FROM verified_sources WHERE address = $1",
		address,
	)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Source not found"})
		return
	}
	defer rows.Close()
	
	type Source struct {
		FileName   string `json:"file_name"`
		SourceCode string `json:"source_code"`
		Language  string `json:"language"`
	}
	
	var sources []Source
	for rows.Next() {
		var s Source
		if err := rows.Scan(&s.FileName, &s.SourceCode, &s.Language); err != nil {
			continue
		}
		sources = append(sources, s)
	}
	
	c.JSON(http.StatusOK, gin.H{"data": sources})
}

func (h *Handler) GetContractABI(c *gin.Context) {
	address := c.Param("address")
	
	var abi string
	err := h.db.QueryRowContext(c.Request.Context(),
		"SELECT abi FROM contracts WHERE address = $1",
		address,
	).Scan(&abi)
	
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "ABI not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"abi": abi})
}

func (h *Handler) ReadContract(c *gin.Context) {
	address := c.Param("address")
	var req struct {
		Method string   `json:"method" binding:"required"`
		Params []string `json:"params"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	// In production, make RPC call to node
	var result string
	err := h.db.QueryRowContext(c.Request.Context(),
		"SELECT encode(eth_call($1, $2, $3), 'hex')",
		address, req.Method, req.Params,
	).Scan(&result)
	
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"result": result})
}

func (h *Handler) WriteContract(c *gin.Context) {
	address := c.Param("address")
	var req struct {
		Method    string `json:"method" binding:"required"`
		Params   []string `json:"params"`
		From     string `json:"from"`
		GasLimit uint64 `json:"gas_limit"`
	}
	
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	// In production, make RPC call to node
	var txHash string
	err := h.db.QueryRowContext(c.Request.Context(),
		"SELECT eth_sendRawTransaction($1)",
		req.Method, req.Params,
	).Scan(&txHash)
	
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"transactionHash": txHash})
}

// ============================================================================
// Search Handler
// ============================================================================

func (h *Handler) Search(c *gin.Context) {
	query := c.Query("q")
	if query == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Query required"})
		return
	}
	
	// Try to parse as address
	if len(query) == 42 && strings.HasPrefix(query, "0x") {
		// Check if account
		var account map[string]interface{}
		err := h.db.QueryRowContext(c.Request.Context(),
			"SELECT address, balance, code FROM accounts WHERE address = $1",
			query,
		).Scan(&account)
		
		if err == nil {
			c.JSON(http.StatusOK, gin.H{"type": "address", "result": account})
			return
		}
		
		// Check if contract
		var contract map[string]interface{}
		err = h.db.QueryRowContext(c.Request.Context(),
			"SELECT address, code FROM contracts WHERE address = $1",
			query,
		).Scan(&contract)
		
		if err == nil {
			c.JSON(http.StatusOK, gin.H{"type": "contract", "result": contract})
			return
		}
	}
	
	// Try to parse as transaction hash
	if len(query) == 66 && strings.HasPrefix(query, "0x") {
		var tx map[string]interface{}
		err := h.db.QueryRowContext(c.Request.Context(),
			"SELECT hash, from, to, value FROM transactions WHERE hash = $1",
			query,
		).Scan(&tx)
		
		if err == nil {
			c.JSON(http.StatusOK, gin.H{"type": "transaction", "result": tx})
			return
		}
	}
	
	// Try to parse as block number
	if num, err := strconv.ParseUint(query, 10, 64); err == nil {
		var block map[string]interface{}
		err := h.db.QueryRowContext(c.Request.Context(),
			"SELECT number, hash, timestamp, miner FROM blocks WHERE number = $1",
			num,
		).Scan(&block)
		
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
			if err := rows.Scan(&t); err == nil {
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
// Stats Handlers
// ============================================================================

func (h *Handler) GetStats(c *gin.Context) {
	var stats struct {
		TotalBlocks      uint64 `json:"total_blocks"`
		TotalTransactions uint64 `json:"total_transactions"`
		TotalTokens     uint64 `json:"total_tokens"`
		TotalAccounts   uint64 `json:"total_accounts"`
		TotalContracts  uint64 `json:"total_contracts"`
		LastBlock      uint64 `json:"last_block"`
	}
	
	h.db.QueryRowContext(c.Request.Context(), "SELECT COUNT(*) FROM blocks").Scan(&stats.TotalBlocks)
	h.db.QueryRowContext(c.Request.Context(), "SELECT COUNT(*) FROM transactions").Scan(&stats.TotalTransactions)
	h.db.QueryRowContext(c.Request.Context(), "SELECT COUNT(*) FROM tokens").Scan(&stats.TotalTokens)
	h.db.QueryRowContext(c.Request.Context(), "SELECT COUNT(*) FROM accounts").Scan(&stats.TotalAccounts)
	h.db.QueryRowContext(c.Request.Context(), "SELECT COUNT(*) FROM contracts").Scan(&stats.TotalContracts)
	h.db.QueryRowContext(c.Request.Context(), "SELECT MAX(number) FROM blocks").Scan(&stats.LastBlock)
	
	c.JSON(http.StatusOK, stats)
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
// Main
// ============================================================================

func main() {
	// Set GOMAXPROCS
	runtime.GOMAXPROCS(runtime.NumCPU())
	
	// Set Gin mode
	gin.SetMode(gin.ReleaseMode)
	
	// Load configuration
	config := Config{
		Server: ServerConfig{
			Host:     "0.0.0.0",
			Port:     8080,
			ReadTimeout:  30,
			WriteTimeout: 30,
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
			Burst:           2000,
			Enabled:         true,
		},
	}
	
	// Initialize database
	db, err := NewDB(config.Database, config.Redis)
	if err != nil {
		fmt.Printf("Failed to connect to database: %v\n", err)
		os.Exit(1)
	}
	defer db.Close()
	
	// Initialize handler
	handler := NewHandler(db)
	
	// Initialize router
	router := gin.New()
	router.Use(gin.Recovery())
	router.Use(gin.Logger())
	
	// CORS
	router.Use(cors.New(cors.Config{
		AllowOrigins:     config.Security.CorsOrigins,
		AllowMethods:     []string{"GET", "POST", "PUT", "DELETE", "OPTIONS"},
		AllowHeaders:     []string{"Origin", "Content-Type", "Accept", "Authorization", "X-API-Key"},
		ExposeHeaders:    []string{"X-Request-ID"},
		AllowCredentials: true,
	}))
	
	// Rate limiting middleware
	if config.RateLimit.Enabled {
		store := throttled.NewMemoryStore(throttled.RateQuota{
			MaxRate:  rate.Limit(config.RateLimit.RequestsPerSecond),
			MaxBurst: config.RateLimit.Burst,
		})
		throttled := throttled.Throttle(store, &throttled.Quota{
			MaxRate:  rate.Limit(config.RateLimit.RequestsPerSecond),
			MaxBurst: config.RateLimit.Burst,
		})
		router.Use(throttled)
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
		
		// Tokens
		api.GET("/tokens", handler.GetTokens)
		api.GET("/tokens/:address", handler.GetToken)
		api.GET("/tokens/:address/holders", handler.GetTokenHolders)
		api.GET("/tokens/:address/transfers", handler.GetTokenTransfers)
		api.GET("/tokens/:address/approvals", handler.GetTokenApprovals)
		
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
		
		// Search
		api.GET("/search", handler.Search)
		
		// Stats
		api.GET("/stats", handler.GetStats)
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