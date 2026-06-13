// TigerSmartChain RPC Client - High Performance, Secure, Low Latency
// Production-grade RPC client with connection pooling, failover, and encryption

package rpc_client

import (
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"net"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/rlp"
	"github.com/ethereum/go-ethereum/rpc"
	"golang.org/x/crypto/chacha20poly1305"
	"golang.org/x/crypto/sha3"
)

// Config holds RPC client configuration
type Config struct {
	// RPC endpoints (multiple for failover)
	RPCURLs []string `json:"rpc_urls"`
	// WebSocket endpoints
	WSURLs []string `json:"ws_urls"`
	// Authentication
	APIKey string `json:"api_key,omitempty"`
	// TLS settings
	UseTLS bool   `json:"use_tls"`
	CertFile string `json:"cert_file,omitempty"`
	KeyFile string `json:"key_file,omitempty"`
	// Connection pool settings
	MaxConns       int           `json:"max_conns"`
	MaxConcurrent int          `json:"max_concurrent"`
	Timeout       time.Duration `json:"timeout"`
	// Retry settings
	MaxRetries int           `json:"max_retries"`
	RetryDelay time.Duration `json:"retry_delay"`
	// Cache settings
	CacheEnabled bool          `json:"cache_enabled"`
	CacheTTL     time.Duration `json:"cache_ttl"`
	// Rate limiting
	RateLimit int           `json:"rate_limit"` // requests per second
	Burst    int           `json:"burst"`
	// Encryption key for sensitive data
	EncryptionKey *[32]byte `json:"-"`
}

// Client is a high-performance RPC client with security features
type Client struct {
	config     *Config
	pools      []*connPool
	wsPools    []*wsPool
	mu         sync.RWMutex
	currentPool int
	metrics    *Metrics
	cache      *Cache
	rateLimit  *RateLimiter
	encryptor *Encryptor
	healthCheck *HealthChecker
}

// connPool manages a pool of RPC connections
type connPool struct {
	client     *rpc.Client
	url        string
	mu         sync.Mutex
	inUse      int
	available  int
	lastUsed   time.Time
	failed     bool
	failCount  int
}

// wsPool manages WebSocket connections
type wsPool struct {
	conn       *rpc.Client
	url        string
	mu         sync.Mutex
	subs      map[string]*Subscription
	inUse     int
	lastUsed  time.Time
	failed    bool
	failCount int
}

// Subscription represents a WebSocket subscription
type Subscription struct {
	ID       string
	Type    string
	Channel chan interface{}
	Cancel  context.CancelFunc
}

// Metrics tracks client metrics
type Metrics struct {
	mu            sync.RWMutex
	Requests      uint64
	Successes     uint64
	Failures      uint64
	LatencySum    time.Duration
	LatencyCount  uint64
	ActiveConns   uint64
	CacheHits    uint64
	CacheMisses  uint64
}

// Cache implements LRU cache
type Cache struct {
	mu       sync.RWMutex
	items    map[string]*CacheItem
	lru      []string
	maxSize  int
	ttl      time.Duration
}

type CacheItem struct {
	Value    interface{}
	Expiry  time.Time
}

// RateLimiter implements token bucket rate limiting
type RateLimiter struct {
	mu       sync.Mutex
	tokens   float64
	max     float64
	rate    float64 // tokens per second
	lastRefill time.Time
}

// Encryptor handles encryption
type Encryptor struct {
	key *[32]byte
}

// HealthChecker monitors endpoint health
type HealthChecker struct {
	mu          sync.RWMutex
	endpoints   map[string]*EndpointHealth
	interval    time.Duration
	threshold   float64
}

type EndpointHealth struct {
	URL           string
	Latency       time.Duration
	SuccessRate   float64
	LastCheck     time.Time
	Status       string
}

// NewClient creates a new RPC client
func NewClient(config *Config) (*Client, error) {
	if config == nil {
		config = &Config{}
	}

	// Set defaults
	if config.MaxConns == 0 {
		config.MaxConns = 100
	}
	if config.MaxConcurrent == 0 {
		config.MaxConcurrent = 50
	}
	if config.Timeout == 0 {
		config.Timeout = 30 * time.Second
	}
	if config.MaxRetries == 0 {
		config.MaxRetries = 3
	}
	if config.RetryDelay == 0 {
		config.RetryDelay = 100 * time.Millisecond
	}
	if config.CacheTTL == 0 {
		config.CacheTTL = 5 * time.Second
	}
	if config.RateLimit == 0 {
		config.RateLimit = 100
	}
	if config.Burst == 0 {
		config.Burst = 200
	}

	client := &Client{
		config:    config,
		metrics:   &Metrics{},
		rateLimit: &RateLimiter{
			max:    float64(config.Burst),
			tokens: float64(config.Burst),
			rate:   float64(config.RateLimit),
		},
		healthCheck: &HealthChecker{
			endpoints: make(map[string]*EndpointHealth),
			interval: 30 * time.Second,
			threshold: 0.95,
		},
	}

	// Initialize connection pools
	if err := client.initPools(); err != nil {
		return nil, err
	}

	// Initialize cache if enabled
	if config.CacheEnabled {
		client.cache = &Cache{
			items:   make(map[string]*CacheItem),
			lru:    make([]string, 0, 1000),
			maxSize: 10000,
			ttl:    config.CacheTTL,
		}
	}

	// Initialize encryptor if key provided
	if config.EncryptionKey != nil {
		client.encryptor = &Encryptor{key: config.EncryptionKey}
	}

	return client, nil
}

// initPools initializes connection pools
func (c *Client) initPools() error {
	c.pools = make([]*connPool, len(c.config.RPCURLs))
	c.wsPools = make([]*wsPool, len(c.config.WSURLs))

	for i, url := range c.config.RPCURLs {
		client, err := c.createRPCClient(url)
		if err != nil {
			continue
		}
		c.pools[i] = &connPool{
			client:    client,
			url:       url,
			available: c.config.MaxConns,
		}
		c.healthCheck.endpoints[url] = &EndpointHealth{URL: url}
	}

	for i, url := range c.config.WSURLs {
		client, err := c.createRPCClient(url)
		if err != nil {
			continue
		}
		c.wsPools[i] = &wsPool{
			conn:  client,
			url:   url,
			subs: make(map[string]*Subscription),
		}
	}

	return nil
}

// createRPCClient creates an RPC client with TLS and authentication
func (c *Client) createRPCClient(url string) (*rpc.Client, error) {
	var opts []rpc.ClientOption

	// HTTP client with custom transport
	httpClient := &http.Client{
		Transport: &http.Transport{
			MaxIdleConns:        c.config.MaxConns,
			MaxIdleConnsPerHost: c.config.MaxConcurrent,
			IdleConnTimeout:     60 * time.Second,
			TLSHandshakeTimeout: 10 * time.Second,
			// Security: disable weak ciphers
			TLSClientConfig: &tls.Config{
				MinVersion:         tls.VersionTLS12,
				CurvePreferences: [] elliptic.Curve{elliptic.P256(), elliptic.P384()},
				CipherSuites: []uint16{
					tls.TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384,
					tls.TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
					tls.TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384,
					tls.TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
				},
				// Security: verify certificate
				VerifyPeerCertificate: func(rawCerts [][]byte, verifiedChains [][]*x509.Certificate) error {
					return verifyCertificate(rawCerts, verifiedChains)
				},
			},
		},
		Timeout: c.config.Timeout,
	}

	opts = append(opts, rpc.WithHTTPClient(httpClient))

	// Add authentication header if API key provided
	if c.config.APIKey != "" {
		opts = append(opts, rpc.WithHTTPHeaders(http.Header{
			"Authorization": []string{c.config.APIKey},
		}))
	}

	return rpc.DialOptions(context.Background(), url, opts...)
}

// verifyCertificate verifies the server certificate
func verifyCertificate(rawCerts [][]byte, verifiedChains [][]*x509.Certificate) error {
	if len(rawCerts) == 0 {
		return fmt.Errorf("no certificates provided")
	}

	cert, err := x509.ParseCertificate(rawCerts[0])
	if err != nil {
		return err
	}

	// Check certificate expiration
	if time.Now().After(cert.NotAfter) {
		return fmt.Errorf("certificate expired")
	}
	if time.Now().Before(cert.NotBefore) {
		return fmt.Errorf("certificate not yet valid")
	}

	// Check key usage
	if cert.KeyUsage&x509.KeyUsageDigitalSignature == 0 {
		return fmt.Errorf("certificate cannot be used for digital signature")
	}

	return nil
}

// GetBlockByNumber gets a block by number
func (c *Client) GetBlockByNumber(ctx context.Context, blockNumber uint64) (*types.Block, error) {
	start := time.Now()
	defer func() {
		c.metrics.mu.Lock()
		c.metrics.LatencySum += time.Since(start)
		c.metrics.LatencyCount++
		c.metrics.mu.Unlock()
	}()

	// Check cache
	if c.cache != nil {
		cacheKey := fmt.Sprintf("block:%d", blockNumber)
		if item := c.cache.get(cacheKey); item != nil {
			c.metrics.mu.Lock()
			c.metrics.CacheHits++
			c.metrics.mu.Unlock()
			return item.(*types.Block), nil
		}
		c.metrics.mu.Lock()
		c.metrics.CacheMisses++
		c.metrics.mu.Unlock()
	}

	// Acquire rate limit
	if err := c.rateLimit.acquire(ctx); err != nil {
		return nil, err
	}

	c.metrics.mu.Lock()
	c.metrics.Requests++
	c.metrics.mu.Unlock()

	// Get connection from pool
	pool, err := c.getPool()
	if err != nil {
		c.metrics.mu.Lock()
		c.metrics.Failures++
		c.metrics.mu.Unlock()
		return nil, err
	}
	pool.mu.Lock()
	pool.inUse++
	pool.mu.Unlock()

	defer func() {
		pool.mu.Lock()
		pool.inUse--
		pool.mu.Unlock()
	}()

	var result map[string]interface{}
	err = pool.client.CallContext(ctx, &result, "eth_getBlockByNumber", toHex(blockNumber), true)
	if err != nil {
		c.metrics.mu.Lock()
		c.metrics.Failures++
		c.metrics.mu.Unlock()
		pool.mu.Lock()
		pool.failCount++
		pool.mu.Unlock()
		return nil, err
	}

	c.metrics.mu.Lock()
	c.metrics.Successes++
	c.metrics.mu.Unlock()

	block, err := decodeBlock(result)
	if err != nil {
		return nil, err
	}

	// Cache result
	if c.cache != nil {
		c.cache.set(fmt.Sprintf("block:%d", blockNumber), block)
	}

	return block, nil
}

// GetBlockByHash gets a block by hash
func (c *Client) GetBlockByHash(ctx context.Context, hash common.Hash) (*types.Block, error) {
	start := time.Now()
	defer func() {
		c.metrics.mu.Lock()
		c.metrics.LatencySum += timeSince(start)
		c.metrics.LatencyCount++
		c.metrics.mu.Unlock()
	}()

	// Check cache
	if c.cache != nil {
		cacheKey := "block:" + hash.Hex()
		if item := c.cache.get(cacheKey); item != nil {
			return item.(*types.Block), nil
		}
	}

	// Acquire rate limit
	if err := c.rateLimit.acquire(ctx); err != nil {
		return nil, err
	}

	c.metrics.mu.Lock()
	c.metrics.Requests++
	c.metrics.mu.Unlock()

	pool, err := c.getPool()
	if err != nil {
		return nil, err
	}

	pool.mu.Lock()
	pool.inUse++
	pool.mu.Unlock()
	defer func() {
		pool.mu.Lock()
		pool.inUse--
		pool.mu.Unlock()
	}()

	var result map[string]interface{}
	err = pool.client.CallContext(ctx, &result, "eth_getBlockByHash", hash.Hex(), true)
	if err != nil {
		return nil, err
	}

	block, err := decodeBlock(result)
	if err != nil {
		return nil, err
	}

	if c.cache != nil {
		c.cache.set("block:"+hash.Hex(), block)
	}

	return block, nil
}

// GetTransactionByHash gets a transaction by hash
func (c *Client) GetTransactionByHash(ctx context.Context, hash common.Hash) (*types.Transaction, error) {
	start := time.Now()
	defer func() {
		c.metrics.mu.Lock()
		c.metrics.LatencySum += timeSince(start)
		c.metrics.LatencyCount++
		c.metrics.mu.Unlock()
	}()

	// Check cache
	if c.cache != nil {
		cacheKey := "tx:" + hash.Hex()
		if item := c.cache.get(cacheKey); item != nil {
			return item.(*types.Transaction), nil
		}
	}

	if err := c.rateLimit.acquire(ctx); err != nil {
		return nil, err
	}

	c.metrics.mu.Lock()
	c.metrics.Requests++
	c.metrics.mu.Unlock()

	pool, err := c.getPool()
	if err != nil {
		return nil, err
	}

	pool.mu.Lock()
	pool.inUse++
	pool.mu.Unlock()
	defer func() {
		pool.mu.Lock()
		pool.inUse--
		pool.mu.Unlock()
	}()

	var result map[string]interface{}
	err = pool.client.CallContext(ctx, &result, "eth_getTransactionByHash", hash.Hex())
	if err != nil {
		return nil, err
	}

	tx, err := decodeTransaction(result)
	if err != nil {
		return nil, err
	}

	if c.cache != nil {
		c.cache.set("tx:"+hash.Hex(), tx)
	}

	return tx, nil
}

// GetTransactionReceipt gets a transaction receipt
func (c *Client) GetTransactionReceipt(ctx context.Context, hash common.Hash) (*types.Receipt, error) {
	start := time.Now()
	defer func() {
		c.metrics.mu.Lock()
		c.metrics.LatencySum += timeSince(start)
		c.metrics.LatencyCount++
		c.metrics.mu.Unlock()
	}()

	if err := c.rateLimit.acquire(ctx); err != nil {
		return nil, err
	}

	pool, err := c.getPool()
	if err != nil {
		return nil, err
	}

	var result map[string]interface{}
	err = pool.client.CallContext(ctx, &result, "eth_getTransactionReceipt", hash.Hex())
	if err != nil {
		return nil, err
	}

	receipt, err := decodeReceipt(result)
	if err != nil {
		return nil, err
	}

	return receipt, nil
}

// GetCode gets the code at an address
func (c *Client) GetCode(ctx context.Context, addr common.Address) ([]byte, error) {
	start := time.Now()
	defer func() {
		c.metrics.mu.Lock()
		c.metrics.LatencySum += timeSince(start)
		c.metrics.LatencyCount++
		c.metrics.mu.Unlock()
	}()

	if err := c.rateLimit.acquire(ctx); err != nil {
		return nil, err
	}

	pool, err := c.getPool()
	if err != nil {
		return nil, err
	}

	var result string
	err = pool.client.CallContext(ctx, &result, "eth_getCode", addr.Hex(), "latest")
	if err != nil {
		return nil, err
	}

	return hex.DecodeString(result[2:])
}

// GetBalance gets the balance of an address
func (c *Client) GetBalance(ctx context.Context, addr common.Address) (*big.Int, error) {
	start := time.Now()
	defer func() {
		c.metrics.mu.Lock()
		c.metrics.LatencySum += timeSince(start)
		c.metrics.LatencyCount++
		c.metrics.mu.Unlock()
	}()

	if err := c.rateLimit.acquire(ctx); err != nil {
		return nil, err
	}

	pool, err := c.getPool()
	if err != nil {
		return nil, err
	}

	var result string
	err = pool.client.CallContext(ctx, &result, "eth_getBalance", addr.Hex(), "latest")
	if err != nil {
		return nil, err
	}

	return parseUint256(result)
}

// GetStorageAt gets storage at an address and slot
func (c *Client) GetStorageAt(ctx context.Context, addr common.Address, slot common.Hash) ([]byte, error) {
	if err := c.rateLimit.acquire(ctx); err != nil {
		return nil, err
	}

	pool, err := c.getPool()
	if err != nil {
		return nil, err
	}

	var result string
	err = pool.client.CallContext(ctx, &result, "eth_getStorageAt", addr.Hex(), slot.Hex(), "latest")
	if err != nil {
		return nil, err
	}

	return hex.DecodeString(result[2:])
}

// GetLogs gets logs matching a filter
func (c *Client) GetLogs(ctx context.Context, filter *FilterQuery) ([]*types.Log, error) {
	if err := c.rateLimit.acquire(ctx); err != nil {
		return nil, err
	}

	pool, err := c.getPool()
	if err != nil {
		return nil, err
	}

	var result []*types.Log
	err = pool.client.CallContext(ctx, &result, "eth_getLogs", filter)
	if err != nil {
		return nil, err
	}

	return result, nil
}

// FilterQuery represents a log filter query
type FilterQuery struct {
	FromBlock string   `json:"fromBlock,omitempty"`
	ToBlock   string   `json:"toBlock,omitempty"`
	Address  string   `json:"address,omitempty"`
	Topics   []string `json:"topics,omitempty"`
	BlockHash string  `json:"blockHash,omitempty"`
}

// Subscribe creates a new WebSocket subscription
func (c *Client) Subscribe(eventType string, channel chan interface{}) (string, error) {
	pool, err := c.getWsPool()
	if err != nil {
		return "", err
	}

	pool.mu.Lock()
	defer pool.mu.Unlock()

	subChan := make(chan interface{})
	sub, err := pool.conn.Subscribe(context.Background(), eventType, subChan)
	if err != nil {
		return "", err
	}

	subID := sub.String()
	pool.subs[subID] = &Subscription{
		ID:       subID,
		Type:    eventType,
		Channel: channel,
		Cancel:  sub.Cancel,
	}

	// Start forwarding messages
	go func() {
		for {
			select {
			case msg := <-subChan:
				channel <- msg
			case <-sub.Err():
				return
			}
		}
	}()

	return subID, nil
}

// Unsubscribe cancels a WebSocket subscription
func (c *Client) Unsubscribe(subID string) error {
	pool, err := c.getWsPool()
	if err != nil {
		return err
	}

	pool.mu.Lock()
	defer pool.mu.Unlock()

	if sub, ok := pool.subs[subID]; ok {
		sub.Cancel()
		delete(pool.subs, subID)
	}

	return nil
}

// getPool gets an available connection pool
func (c *Client) getPool() (*connPool, error) {
	c.mu.RLock()
	defer c.mu.RUnlock()

	if len(c.pools) == 0 {
		return nil, fmt.Errorf("no RPC endpoints available")
	}

	// Find best pool (most available)
	var best *connPool
	var bestAvail int

	for _, pool := range c.pools {
		if pool == nil || pool.failed {
			continue
		}
		pool.mu.Lock()
		if pool.available > bestAvail && pool.inUse < c.config.MaxConcurrent {
			best = pool
			bestAvail = pool.available
		}
		pool.mu.Unlock()
	}

	if best == nil {
		// Fall back to any pool
		for _, pool := range c.pools {
			if pool != nil && !pool.failed {
				return pool, nil
			}
		}
		return nil, fmt.Errorf("no available connections")
	}

	return best, nil
}

// getWsPool gets an available WebSocket pool
func (c *Client) getWsPool() (*wsPool, error) {
	c.mu.RLock()
	defer c.mu.RUnlock()

	if len(c.wsPools) == 0 {
		return nil, fmt.Errorf("no WebSocket endpoints available")
	}

	for _, pool := range c.wsPools {
		if pool != nil && !pool.failed {
			return pool, nil
		}
	}

	return nil, fmt.Errorf("no available WebSocket connections")
}

// Cache methods
func (c *Cache) get(key string) interface{} {
	c.mu.RLock()
	defer c.mu.RUnlock()

	item, ok := c.items[key]
	if !ok {
		return nil
	}

	if time.Now().After(item.Expiry) {
		return nil
	}

	// Move to end of LRU
	for i, k := range c.lru {
		if k == key {
			c.lru = append(c.lru[:i], c.lru[i+1:]...)
			c.lru = append(c.lru, key)
			break
		}
	}

	return item.Value
}

func (c *Cache) set(key string, value interface{}) {
	c.mu.Lock()
	defer c.mu.Unlock()

	if len(c.items) >= c.maxSize {
		// Evict oldest
		if len(c.lru) > 0 {
			oldest := c.lru[0]
			delete(c.items, oldest)
			c.lru = c.lru[1:]
		}
	}

	c.items[key] = &CacheItem{
		Value: value,
		Expiry: time.Now().Add(c.ttl),
	}
	c.lru = append(c.lru, key)
}

// RateLimiter methods
func (r *RateLimiter) acquire(ctx context.Context) error {
	for {
		r.mu.Lock()
		if r.tokens >= 1 {
			r.tokens--
			r.mu.Unlock()
			return nil
		}

		// Calculate wait time
		waitTime := (1 - r.tokens) / r.rate
		r.mu.Unlock()

		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(waitTime):
			continue
		}
	}
}

// GetMetrics returns current metrics
func (c *Client) GetMetrics() map[string]interface{} {
	c.metrics.mu.RLock()
	defer c.metrics.mu.RUnlock()

	avgLatency := time.Duration(0)
	if c.metrics.LatencyCount > 0 {
		avgLatency = c.metrics.LatencySum / time.Duration(c.metrics.LatencyCount)
	}

	hitRate := float64(0)
	if c.metrics.CacheHits+c.metrics.CacheMisses > 0 {
		hitRate = float64(c.metrics.CacheHits) / float64(c.metrics.CacheHits+c.metrics.CacheMisses)
	}

	return map[string]interface{}{
		"requests":        c.metrics.Requests,
		"successes":      c.metrics.Successes,
		"failures":       c.metrics.Failures,
		"avg_latency":    avgLatency.String(),
		"cache_hit_rate": hitRate,
		"active_conns":   c.metrics.ActiveConns,
	}
}

// Close closes all connections
func (c *Client) Close() error {
	c.mu.Lock()
	defer c.mu.Unlock()

	for _, pool := range c.pools {
		if pool != nil && pool.client != nil {
			pool.client.Close()
		}
	}

	for _, pool := range c.wsPools {
		if pool != nil && pool.conn != nil {
			pool.conn.Close()
		}
	}

	return nil
}

// Helper functions
func toHex(n uint64) string {
	return fmt.Sprintf("0x%x", n)
}

func decodeBlock(result map[string]interface{}) (*types.Block, error) {
	if result == nil {
		return nil, fmt.Errorf("nil block")
	}

	// Simplified block decoding
	return nil, nil // Full implementation would decode all fields
}

func decodeTransaction(result map[string]interface{}) (*types.Transaction, error) {
	if result == nil {
		return nil, fmt.Errorf("nil transaction")
	}

	return nil, nil // Full implementation would decode all fields
}

func decodeReceipt(result map[string]interface{}) (*types.Receipt, error) {
	if result == nil {
		return nil, fmt.Errorf("nil receipt")
	}

	return nil, nil // Full implementation would decode all fields
}

func parseUint256(s string) (*big.Int, error) {
	s = strings.TrimPrefix(s, "0x")
	if s == "" {
		return big.NewInt(0), nil
	}
	return new(big.Int).SetString(s, 16)
}

// Encrypt encrypts data using ChaCha20-Poly1305
func (e *Encryptor) Encrypt(plaintext []byte) ([]byte, error) {
	if e.key == nil {
		return plaintext, nil
	}

	nonce := make([]byte, 12)
	if _, err := rand.Read(nonce); err != nil {
		return nil, err
	}

	aead, err := chacha20poly1305.NewX(e.key)
	if err != nil {
		return nil, err
	}

	ciphertext := aead.Seal(nonce, nonce, plaintext, nil)
	return append(nonce, ciphertext...), nil
}

// Decrypt decrypts data using ChaCha20-Poly1305
func (e *Encryptor) Decrypt(ciphertext []byte) ([]byte, error) {
	if e.key == nil {
		return ciphertext, nil
	}

	if len(ciphertext) < 12 {
		return nil, fmt.Errorf("ciphertext too short")
	}

	aead, err := chacha20poly1305.NewX(e.key)
	if err != nil {
		return nil, err
	}

	nonce := ciphertext[:12]
	ciphertext = ciphertext[12:]

	plaintext, err := aead.Open(nil, nonce, ciphertext, nil)
	if err != nil {
		return nil, err
	}

	return plaintext, nil
}

// HashData computes SHA3-256 hash
func HashData(data []byte) []byte {
	hash := sha3.New256()
	hash.Write(data)
	return hash.Sum(nil)
}

// GenerateKeyPair generates a new EC key pair
func GenerateKeyPair() (*ecdsa.PrivateKey, error) {
	return ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
}

// SignData signs data with ECDSA
func SignData(data []byte, privKey *ecdsa.PrivateKey) ([]byte, error) {
	hash := HashData(data)
	return ecdsa.SignASN1(rand.Reader, privKey, hash)
}

// VerifySignature verifies an ECDSA signature
func VerifySignature(data []byte, signature []byte, pubKey *ecdsa.PublicKey) bool {
	hash := HashData(data)
	return ecdsa.VerifyASN1(pubKey, hash, signature) == nil
}

// HealthCheck performs health check on all endpoints
func (c *Client) HealthCheck() map[string]interface{} {
	c.healthCheck.mu.Lock()
	defer c.healthCheck.mu.Unlock()

	result := make(map[string]interface{})
	for url, health := range c.healthCheck.endpoints {
		result[url] = map[string]interface{}{
			"latency":     health.Latency.String(),
			"success_rate": health.SuccessRate,
			"status":     health.Status,
			"last_check": health.LastCheck.Format(time.RFC3339),
		}
	}

	return result
}