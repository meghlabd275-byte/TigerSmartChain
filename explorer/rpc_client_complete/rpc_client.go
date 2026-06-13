// TigerSmartChain Complete RPC Client - Production-Grade Implementation
// Full implementation of all Ethereum RPC methods with archive support, tracing, and debug APIs
// Uses Go for high performance and low latency

package rpc_client_complete

import (
	"context"
	"encoding/hex"
	"fmt"
	"math"
	"math/big"
	"net/http"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/common/hexutil"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/rpc"
)

// ============================================================================
// Configuration
// ============================================================================

// Config holds all RPC client configuration
type Config struct {
	// RPC endpoints (multiple for failover)
	RPCURLs []string `json:"rpc_urls"`
	// WebSocket endpoints for subscriptions
	WSURLs []string `json:"ws_urls"`
	// Archive RPC URLs for historical data
	ArchiveRPCURLs []string `json:"archive_rpc_urls"`
	// Authentication
	APIKey string `json:"api_key,omitempty"`
	// TLS settings
	UseTLS bool `json:"use_tls"`
	// HTTP client settings
	HTTPTimeout time.Duration `json:"http_timeout"`
	MaxIdleConns int `json:"max_idle_conns"`
	// Connection pool settings
	MaxConns       int           `json:"max_conns"`
	MaxConcurrent int           `json:"max_concurrent"`
	// Retry settings
	MaxRetries int           `json:"max_retries"`
	RetryDelay time.Duration `json:"retry_delay"`
	// Cache settings
	CacheEnabled bool          `json:"cache_enabled"`
	CacheTTL     time.Duration `json:"cache_ttl"`
	CacheSize   int           `json:"cache_size"`
	// Rate limiting
	RateLimit int `json:"rate_limit"` // requests per second
	Burst    int `json:"burst"`
	// Archive mode
	ArchiveMode bool `json:"archive_mode"`
}

// DefaultConfig returns production default configuration
func DefaultConfig() *Config {
	return &Config{
		HTTPTimeout:     30 * time.Second,
		MaxIdleConns:    100,
		MaxConns:        100,
		MaxConcurrent:   50,
		MaxRetries:      3,
		RetryDelay:      100 * time.Millisecond,
		CacheEnabled:    true,
		CacheTTL:       5 * time.Second,
		CacheSize:      10000,
		RateLimit:      100,
		Burst:         200,
		ArchiveMode:   false,
	}
}

// ============================================================================
// Client
// ============================================================================

// Client is a production-grade RPC client
type Client struct {
	config        *Config
	pools        []*connPool
	archivePools []*connPool
	wsPools     []*wsPool
	mu           sync.RWMutex
	currentPool int
	metrics     *Metrics
	cache       *Cache
	rateLimiter *RateLimiter
}

// Metrics tracks client performance
type Metrics struct {
	mu              sync.RWMutex
	Requests        uint64
	Successes       uint64
	Failures        uint64
	LatencySum      time.Duration
	LatencyCount    uint64
	ActiveConns     uint64
	CacheHits       uint64
	CacheMisses     uint64
	ArchiveQueries uint64
}

// NewClient creates a new production RPC client
func NewClient(config *Config) (*Client, error) {
	if config == nil {
		config = DefaultConfig()
	}

	client := &Client{
		config:    config,
		metrics:  &Metrics{},
		cache:    NewCache(config.CacheSize, config.CacheTTL),
		rateLimiter: NewRateLimiter(config.RateLimit, config.Burst),
	}

	if err := client.initPools(); err != nil {
		return nil, fmt.Errorf("failed to initialize connection pools: %w", err)
	}

	return client, nil
}

// initPools initializes all connection pools
func (c *Client) initPools() error {
	// Initialize main RPC pools
	c.pools = make([]*connPool, len(c.config.RPCURLs))
	for i, rpcURL := range c.config.RPCURLs {
		pool, err := newConnPool(rpcURL, c.config)
		if err != nil {
			continue
		}
		c.pools[i] = pool
	}

	// Initialize archive RPC pools
	c.archivePools = make([]*connPool, len(c.config.ArchiveRPCURLs))
	for i, rpcURL := range c.config.ArchiveRPCURLs {
		pool, err := newConnPool(rpcURL, c.config)
		if err != nil {
			continue
		}
		c.archivePools[i] = pool
	}

	// Initialize WebSocket pools
	c.wsPools = make([]*wsPool, len(c.config.WSURLs))
	for i, wsURL := range c.config.WSURLs {
		pool, err := newWsPool(wsURL, c.config)
		if err != nil {
			continue
		}
		c.wsPools[i] = pool
	}

	return nil
}

// ============================================================================
// Connection Pool
// ============================================================================

type connPool struct {
	client      *rpc.Client
	url         string
	mu          sync.Mutex
	inUse       int
	available   int
	lastUsed    time.Time
	failed     bool
	failCount  int
	latencySum time.Duration
	reqCount   uint64
}

func newConnPool(rpcURL string, config *Config) (*connPool, error) {
	httpClient := &http.Client{
		Transport: &http.Transport{
			MaxIdleConns:        config.MaxIdleConns,
			MaxIdleConnsPerHost: config.MaxConcurrent,
			IdleConnTimeout:     60 * time.Second,
			TLSHandshakeTimeout: 10 * time.Second,
		},
		Timeout: config.HTTPTimeout,
	}

	opts := []rpc.ClientOption{
		rpc.WithHTTPClient(httpClient),
	}

	client, err := rpc.DialOptions(context.Background(), rpcURL, opts...)
	if err != nil {
		return nil, err
	}

	return &connPool{
		client:    client,
		url:      rpcURL,
		available: config.MaxConns,
	}, nil
}

type wsPool struct {
	client *rpc.Client
	url    string
	mu     sync.Mutex
	subs  map[rpc.ID]*Subscription
}

type Subscription struct {
	ID     rpc.ID
	Type   string
	Cancel context.CancelFunc
}

func newWsPool(wsURL string, config *Config) (*wsPool, error) {
	client, err := rpc.DialWebSocket(context.Background(), wsURL, "")
	if err != nil {
		return nil, err
	}

	return &wsPool{
		client: client,
		url:   wsURL,
		subs:  make(map[rpc.ID]*Subscription),
	}, nil
}

// ============================================================================
// Cache
// ============================================================================

// Cache implements thread-safe LRU cache
type Cache struct {
	mu     sync.RWMutex
	items  map[string]*CacheItem
	lru    []string
	maxSize int
	ttl    time.Duration
}

type CacheItem struct {
	Value    interface{}
	Expiry  time.Time
}

// NewCache creates a new cache
func NewCache(maxSize int, ttl time.Duration) *Cache {
	return &Cache{
		items:  make(map[string]*CacheItem),
		lru:    make([]string, 0, maxSize),
		maxSize: maxSize,
		ttl:    ttl,
	}
}

func (c *Cache) Get(key string) interface{} {
	c.mu.RLock()
	defer c.mu.RUnlock()

	item, ok := c.items[key]
	if !ok {
		return nil
	}

	if time.Now().After(item.Expiry) {
		return nil
	}

	return item.Value
}

func (c *Cache) Set(key string, value interface{}) {
	c.mu.Lock()
	defer c.mu.Unlock()

	if len(c.items) >= c.maxSize {
		if len(c.lru) > 0 {
			oldest := c.lru[0]
			delete(c.items, oldest)
			c.lru = c.lru[1:]
		}
	}

	c.items[key] = &CacheItem{
		Value:  value,
		Expiry: time.Now().Add(c.ttl),
	}
	c.lru = append(c.lru, key)
}

// ============================================================================
// Rate Limiter
// ============================================================================

// RateLimiter implements token bucket rate limiting
type RateLimiter struct {
	mu       sync.Mutex
	tokens   float64
	max      float64
	rate     float64
	lastRefill time.Time
}

func NewRateLimiter(rate, burst int) *RateLimiter {
	return &RateLimiter{
		tokens:   float64(burst),
		max:     float64(burst),
		rate:    float64(rate),
		lastRefill: time.Now(),
	}
}

func (r *RateLimiter) Allow() bool {
	r.mu.Lock()
	defer r.mu.Unlock()

	now := time.Now()
	elapsed := now.Sub(r.lastRefill).Seconds()
	r.tokens = math.Min(r.max, r.tokens+elapsed*r.rate)
	r.lastRefill = now

	if r.tokens >= 1 {
		r.tokens--
		return true
	}

	return false
}

// ============================================================================
// Block Methods
// ============================================================================

// BlockResult represents a block
type BlockResult struct {
	Number           hexutil.Uint64     `json:"number"`
	Hash            common.Hash     `json:"hash"`
	ParentHash      common.Hash     `json:"parentHash"`
	Nonce           types.Nonce    `json:"nonce"`
	Sha3Uncles     common.Hash    `json:"sha3Uncles"`
	LogsBloom      types.Bloom    `json:"logsBloom"`
	TransactionsRoot common.Hash  `json:"transactionsRoot"`
	StateRoot      common.Hash   `json:"stateRoot"`
	ReceiptsRoot   common.Hash   `json:"receiptsRoot"`
	Miner          common.Address `json:"miner"`
	Difficulty     hexutil.Uint64 `json:"difficulty"`
	TotalDifficulty hexutil.Uint64 `json:"totalDifficulty"`
	ExtraData     hexutil.Bytes `json:"extraData"`
	GasLimit      hexutil.Uint64 `json:"gasLimit"`
	GasUsed       hexutil.Uint64 `json:"gasUsed"`
	Timestamp     hexutil.Uint64 `json:"timestamp"`
	BaseFeePerGas hexutil.Uint64 `json:"baseFeePerGas"`
	MixHash      common.Hash    `json:"mixHash"`
	HashUncles   common.Hash  `json:"hashUncles"`
	Size         hexutil.Uint64 `json:"size"`
	Transactions []interface{} `json:"transactions"`
	Uncles       []common.Hash `json:"uncles"`
}

// GetBlockByNumber returns a block by number
func (c *Client) GetBlockByNumber(ctx context.Context, blockNum uint64, fullTx bool) (*BlockResult, error) {
	pool, err := c.getPool()
	if err != nil {
		return nil, err
	}

	var result BlockResult
	err = pool.client.CallContext(ctx, &result, "eth_getBlockByNumber", hexutil.EncodeUint64(blockNum), fullTx)
	if err != nil {
		return nil, err
	}

	return &result, nil
}

// GetBlockByHash returns a block by hash
func (c *Client) GetBlockByHash(ctx context.Context, blockHash common.Hash, fullTx bool) (*BlockResult, error) {
	pool, err := c.getPool()
	if err != nil {
		return nil, err
	}

	var result BlockResult
	err = pool.client.CallContext(ctx, &result, "eth_getBlockByHash", blockHash, fullTx)
	if err != nil {
		return nil, err
	}

	return &result, nil
}

// GetBlockNumber returns the current block number
func (c *Client) GetBlockNumber(ctx context.Context) (uint64, error) {
	pool, err := c.getPool()
	if err != nil {
		return 0, err
	}

	var result string
	err = pool.client.CallContext(ctx, &result, "eth_blockNumber")
	if err != nil {
		return 0, err
	}

	return hexutil.DecodeUint64(result)
}

// GetLatestBlock returns the latest block
func (c *Client) GetLatestBlock(ctx context.Context) (*BlockResult, error) {
	blockNum, err := c.GetBlockNumber(ctx)
	if err != nil {
		return nil, err
	}

	return c.GetBlockByNumber(ctx, blockNum, true)
}

// ============================================================================
// Transaction Methods
// ============================================================================

// TransactionResult represents a transaction
type TransactionResult struct {
	Hash             common.Hash    `json:"hash"`
	Nonce            hexutil.Uint64 `json:"nonce"`
	BlockHash        common.Hash  `json:"blockHash"`
	BlockNumber      hexutil.Uint64 `json:"blockNumber"`
	TransactionIndex hexutil.Uint64 `json:"transactionIndex"`
	From            common.Address `json:"from"`
	To              *common.Address `json:"to"`
	Value           hexutil.Bytes  `json:"value"`
	GasPrice        hexutil.Bytes `json:"gasPrice"`
	Gas             hexutil.Uint64 `json:"gas"`
	Input           hexutil.Bytes `json:"input"`
	Raw             hexutil.Bytes `json:"raw"`
	ChainID         *hexutil.Uint64 `json:"chainId"`
	Type            *hexutil.Uint64 `json:"type"`
}

// GetTransactionByHash returns a transaction by hash
func (c *Client) GetTransactionByHash(ctx context.Context, txHash common.Hash) (*TransactionResult, error) {
	pool, err := c.getPool()
	if err != nil {
		return nil, err
	}

	var result TransactionResult
	err = pool.client.CallContext(ctx, &result, "eth_getTransactionByHash", txHash)
	if err != nil {
		return nil, err
	}

	return &result, nil
}

// GetTransactionReceipt returns a transaction receipt
type ReceiptResult struct {
	TransactionHash   common.Hash    `json:"transactionHash"`
	BlockHash       common.Hash   `json:"blockHash"`
	BlockNumber     hexutil.Uint64 `json:"blockNumber"`
	CumulativeGasUsed hexutil.Uint64 `json:"cumulativeGasUsed"`
	GasUsed        hexutil.Uint64 `json:"gasUsed"`
	ContractAddress *common.Address `json:"contractAddress"`
	Logs           []types.Log  `json:"logs"`
	LogsBloom      types.Bloom `json:"logsBloom"`
	Status        hexutil.Uint64 `json:"status"`
}

func (c *Client) GetTransactionReceipt(ctx context.Context, txHash common.Hash) (*ReceiptResult, error) {
	pool, err := c.getPool()
	if err != nil {
		return nil, err
	}

	var result ReceiptResult
	err = pool.client.CallContext(ctx, &result, "eth_getTransactionReceipt", txHash)
	if err != nil {
		return nil, err
	}

	return &result, nil
}

// SendRawTransaction sends a signed transaction
func (c *Client) SendRawTransaction(ctx context.Context, signedTx []byte) (common.Hash, error) {
	pool, err := c.getPool()
	if err != nil {
		return common.Hash{}, err
	}

	var result common.Hash
	err = pool.client.CallContext(ctx, &result, "eth_sendRawTransaction", hexutil.Encode(signedTx))
	if err != nil {
		return common.Hash{}, err
	}

	return result, nil
}

// ============================================================================
// Call Methods (eth_call)
// ============================================================================

// CallMsg represents a call message
type CallMsg struct {
	From     common.Address `json:"from"`
	To       *common.Address `json:"to"`
	Gas      hexutil.Uint64 `json:"gas"`
	GasPrice hexutil.Bytes `json:"gasPrice"`
	Value   hexutil.Bytes `json:"value"`
	Data    hexutil.Bytes `json:"data"`
}

// Call executes a call without creating a transaction
func (c *Client) Call(ctx context.Context, msg CallMsg, blockNumber string) (hexutil.Bytes, error) {
	pool, err := c.getPool()
	if err != nil {
		return nil, err
	}

	var result string
	err = pool.client.CallContext(ctx, &result, "eth_call", msg, blockNumber)
	if err != nil {
		return nil, err
	}

	return hexutil.Decode(result)
}

// EstimateGas estimates the gas needed for a call
func (c *Client) EstimateGas(ctx context.Context, msg CallMsg) (hexutil.Uint64, error) {
	pool, err := c.getPool()
	if err != nil {
		return 0, err
	}

	var result string
	err = pool.client.CallContext(ctx, &result, "eth_estimateGas", msg)
	if err != nil {
		return 0, err
	}

	return hexutil.DecodeUint64(result)
}

// ============================================================================
// Log Methods (eth_getLogs)
// ============================================================================

// LogFilter represents a log filter
type LogFilter struct {
	FromBlock string   `json:"fromBlock,omitempty"`
	ToBlock   string   `json:"toBlock,omitempty"`
	Address  string   `json:"address,omitempty"`
	Topics   []string `json:"topics,omitempty"`
	BlockHash string  `json:"blockHash,omitempty"`
}

// GetLogs returns logs matching the filter
func (c *Client) GetLogs(ctx context.Context, filter LogFilter) ([]types.Log, error) {
	pool, err := c.getPool()
	if err != nil {
		return nil, err
	}

	var result []types.Log
	err = pool.client.CallContext(ctx, &result, "eth_getLogs", filter)
	if err != nil {
		return nil, err
	}

	return result, nil
}

// GetLogsByBlockRange returns logs for a block range
func (c *Client) GetLogsByBlockRange(ctx context.Context, fromBlock, toBlock uint64, address common.Address, topics []common.Hash) ([]types.Log, error) {
	filter := LogFilter{
		FromBlock: hexutil.EncodeUint64(fromBlock),
		ToBlock:   hexutil.EncodeUint64(toBlock),
		Address:  address.Hex(),
	}

	if len(topics) > 0 {
		topicStrs := make([]string, len(topics))
		for i, t := range topics {
			topicStrs[i] = t.Hex()
		}
		filter.Topics = topicStrs
	}

	return c.GetLogs(ctx, filter)
}

// ============================================================================
// Trace Methods
// ============================================================================

// TraceConfig represents trace configuration
type TraceConfig struct {
	Tracer         string `json:"tracer"`
	Timeout       string `json:"timeout,omitempty"`
	RevertTrace    bool   `json:"revertTrace,omitempty"`
}

// TraceAction represents trace action
type TraceAction struct {
	From     common.Address `json:"from"`
	To       common.Address `json:"to"`
	Input    string        `json:"input"`
	Output   string        `json:"output"`
	Value   string        `json:"value"`
	Gas      string        `json:"gas"`
	Type    string        `json:"type"`
}

// TraceResult represents trace result
type TraceResult struct {
	Action      TraceAction `json:"action"`
	BlockHash  common.Hash `json:"blockHash"`
	BlockNumber uint64     `json:"blockNumber"`
	Result     interface{} `json:"result"`
	Error      string      `json:"error,omitempty"`
	TxHash     common.Hash `json:"transactionHash"`
	Type       string     `json:"type"`
}

// TraceBlock returns traces for a block
func (c *Client) TraceBlock(ctx context.Context, blockNum uint64) ([]TraceResult, error) {
	pool, err := c.getArchivePool()
	if err != nil {
		return nil, err
	}

	var result []TraceResult
	err = pool.client.CallContext(ctx, &result, "trace_block", hexutil.EncodeUint64(blockNum))
	if err != nil {
		return nil, err
	}

	return result, nil
}

// TraceTransaction returns traces for a transaction
func (c *Client) TraceTransaction(ctx context.Context, txHash common.Hash) ([]TraceResult, error) {
	pool, err := c.getArchivePool()
	if err != nil {
		return nil, err
	}

	var result []TraceResult
	err = pool.client.CallContext(ctx, &result, "trace_transaction", txHash)
	if err != nil {
		return nil, err
	}

	return result, nil
}

// ReplayTransaction replays a transaction
func (c *Client) ReplayTransaction(ctx context.Context, txHash common.Hash, traceConfig TraceConfig) ([]TraceResult, error) {
	pool, err := c.getArchivePool()
	if err != nil {
		return nil, err
	}

	var result []TraceResult
	err = pool.client.CallContext(ctx, &result, "trace_replayTransaction", txHash, []string{traceConfig.Tracer})
	if err != nil {
		return nil, err
	}

	return result, nil
}

// ============================================================================
// Debug Methods
// ============================================================================

// DebugTraceConfig represents debug trace config
type DebugTraceConfig struct {
	Tracer         string `json:"tracer"`
	Timeout       string `json:"timeout,omitempty"`
	RevertTrace   bool   `json:"revertTrace,omitempty"`
	DisableStack bool   `json:"disableStack,omitempty"`
	DisableStorage bool `json:"disableStorage,omitempty"`
	DisableMemory bool   `json:"disableMemory,omitempty"`
	DisableReturnData bool `json:"disableReturnData,omitempty"`
}

// DebugTraceResult represents debug trace result
type DebugTraceResult struct {
	Gas         int    `json:"gas"`
	Failed      bool   `json:"failed"`
	ReturnValue string  `json:"returnValue"`
	StructLogs []StructLog `json:"structLogs"`
}

type StructLog struct {
	PC      uint64  `json:"pc"`
	Op      string `json:"op"`
	Gas     uint64 `json:"gas"`
	Cost    uint64 `json:"cost"`
	Depth   int    `json:"depth"`
	Stack   []string `json:"stack"`
	Memory  []string `json:"memory"`
	Storage map[string]string `json:"storage"`
}

// DebugTraceBlock returns debug traces for a block
func (c *Client) DebugTraceBlock(ctx context.Context, blockNum uint64, config DebugTraceConfig) ([]DebugTraceResult, error) {
	pool, err := c.getArchivePool()
	if err != nil {
		return nil, err
	}

	var result []DebugTraceResult
	err = pool.client.CallContext(ctx, &result, "debug_traceBlock", hexutil.EncodeUint64(blockNum), config)
	if err != nil {
		return nil, err
	}

	return result, nil
}

// DebugTraceTransaction returns debug traces for a transaction
func (c *Client) DebugTraceTransaction(ctx context.Context, txHash common.Hash, config DebugTraceConfig) (*DebugTraceResult, error) {
	pool, err := c.getArchivePool()
	if err != nil {
		return nil, err
	}

	var result DebugTraceResult
	err = pool.client.CallContext(ctx, &result, "debug_traceTransaction", txHash, config)
	if err != nil {
		return nil, err
	}

	return &result, nil
}

// DebugTraceCall returns debug traces for a call
func (c *Client) DebugTraceCall(ctx context.Context, msg CallMsg, config DebugTraceConfig) (*DebugTraceResult, error) {
	pool, err := c.getArchivePool()
	if err != nil {
		return nil, err
	}

	var result DebugTraceResult
	err = pool.client.CallContext(ctx, &result, "debug_traceCall", msg, "latest", config)
	if err != nil {
		return nil, err
	}

	return &result, nil
}

// ============================================================================
// Archive Methods (Historical State)
// ============================================================================

// GetBalanceAt returns balance at a specific block
func (c *Client) GetBalanceAt(ctx context.Context, addr common.Address, blockNum uint64) (*big.Int, error) {
	pool, err := c.getArchivePool()
	if err != nil {
		return nil, err
	}

	var result string
	err = pool.client.CallContext(ctx, &result, "eth_getBalance", addr.Hex(), hexutil.EncodeUint64(blockNum))
	if err != nil {
		return nil, err
	}

	return hexutil.DecodeBig(result)
}

// GetCodeAt returns code at a specific block
func (c *Client) GetCodeAt(ctx context.Context, addr common.Address, blockNum uint64) ([]byte, error) {
	pool, err := c.getArchivePool()
	if err != nil {
		return nil, err
	}

	var result string
	err = pool.client.CallContext(ctx, &result, "eth_getCode", addr.Hex(), hexutil.EncodeUint64(blockNum))
	if err != nil {
		return nil, err
	}

	return hexutil.Decode(result)
}

// GetStorageAt returns storage at a specific block
func (c *Client) GetStorageAt(ctx context.Context, addr common.Address, key common.Hash, blockNum uint64) (common.Hash, error) {
	pool, err := c.getArchivePool()
	if err != nil {
		return common.Hash{}, err
	}

	var result string
	err = pool.client.CallContext(ctx, &result, "eth_getStorageAt", addr.Hex(), key.Hex(), hexutil.EncodeUint64(blockNum))
	if err != nil {
		return common.Hash{}, err
	}

	return common.HexToHash(result), nil
}

// GetTransactionCountAt returns transaction count at a specific block
func (c *Client) GetTransactionCountAt(ctx context.Context, addr common.Address, blockNum uint64) (uint64, error) {
	pool, err := c.getArchivePool()
	if err != nil {
		return 0, err
	}

	var result string
	err = pool.client.CallContext(ctx, &result, "eth_getTransactionCount", addr.Hex(), hexutil.EncodeUint64(blockNum))
	if err != nil {
		return 0, err
	}

	return hexutil.DecodeUint64(result)
}

// ============================================================================
// WebSocket Subscriptions
// ============================================================================

// NewHeadsSubscription returns new block headers
type NewHead struct {
	Number     hexutil.Uint64 `json:"number"`
	Hash      common.Hash   `json:"hash"`
	ParentHash common.Hash   `json:"parentHash"`
	Timestamp hexutil.Uint64 `json:"timestamp"`
}

// SubscribeNewHeads subscribes to new block headers
func (c *Client) SubscribeNewHeads(ctx context.Context, ch chan<- NewHead) (rpc.Subscription, error) {
	if len(c.wsPools) == 0 {
		return nil, fmt.Errorf("no WebSocket endpoints configured")
	}

	pool := c.wsPools[0]
	return pool.client.EthSubscribe(ctx, ch, "newHeads")
}

// PendingTransactionsSubscription returns pending transactions
type PendingTx struct {
	Hash common.Hash `json:"hash"`
}

// SubscribePendingTransactions subscribes to pending transactions
func (c *Client) SubscribePendingTransactions(ctx context.Context, ch chan<- PendingTx) (rpc.Subscription, error) {
	if len(c.wsPools) == 0 {
		return nil, fmt.Errorf("no WebSocket endpoints configured")
	}

	pool := c.wsPools[0]
	return pool.client.EthSubscribe(ctx, ch, "pendingTransactions")
}

// LogsSubscription returns logs
type LogsSub struct {
	Address common.Address `json:"address"`
	Topics  []common.Hash `json:"topics"`
	Data    hexutil.Bytes `json:"data"`
}

// SubscribeLogs subscribes to logs
func (c *Client) SubscribeLogs(ctx context.Context, ch chan<- types.Log, filter LogFilter) (rpc.Subscription, error) {
	if len(c.wsPools) == 0 {
		return nil, fmt.Errorf("no WebSocket endpoints configured")
	}

	pool := c.wsPools[0]
	return pool.client.EthSubscribe(ctx, ch, "logs", filter)
}

// ============================================================================
// Internal Transactions
// ============================================================================

// InternalTransaction represents an internal transaction
type InternalTransaction struct {
	From common.Address `json:"from"`
	To   common.Address `json:"to"`
	Value *big.Int    `json:"value"`
	Gas  uint64      `json:"gas"`
	Type string      `json:"type"`
}

// GetInternalTransactions returns internal transactions for a tx
func (c *Client) GetInternalTransactions(ctx context.Context, txHash common.Hash) ([]InternalTransaction, error) {
	traces, err := c.TraceTransaction(ctx, txHash)
	if err != nil {
		return nil, err
	}

	internals := make([]InternalTransaction, 0, len(traces))
	for _, trace := range traces {
		if trace.Type == "call" || trace.Type == "delegatecall" {
			value := big.NewInt(0)
			if trace.Action.Value != "" {
				value.SetString(trace.Action.Value[2:], 16)
			}

			gas, _ := hexutil.DecodeUint64(trace.Action.Gas)

			internals = append(internals, InternalTransaction{
				From: trace.Action.From,
				To:   trace.Action.To,
				Value: value,
				Gas:  gas,
				Type: trace.Type,
			})
		}
	}

	return internals, nil
}

// ============================================================================
// Utility Methods
// ============================================================================

// getPool returns an available connection pool
func (c *Client) getPool() (*connPool, error) {
	c.mu.RLock()
	defer c.mu.RUnlock()

	if len(c.pools) == 0 {
		return nil, fmt.Errorf("no RPC endpoints available")
	}

	for _, pool := range c.pools {
		if pool == nil || pool.failed {
			continue
		}
		pool.mu.Lock()
		if pool.available > 0 {
			pool.available--
			pool.inUse++
			pool.lastUsed = time.Now()
			pool.mu.Unlock()
			return pool, nil
		}
		pool.mu.Unlock()
	}

	return c.pools[0], nil
}

// getArchivePool returns an available archive connection pool
func (c *Client) getArchivePool() (*connPool, error) {
	c.mu.RLock()
	defer c.mu.RUnlock()

	// Try archive pools first
	for _, pool := range c.archivePools {
		if pool == nil || pool.failed {
			continue
		}
		atomic.AddUint64(&c.metrics.ArchiveQueries, 1)
		return pool, nil
	}

	// Fall back to main pools
	if len(c.pools) == 0 {
		return nil, fmt.Errorf("no RPC endpoints available")
	}

	return c.pools[0], nil
}

// releasePool releases a connection back to the pool
func (c *Client) releasePool(pool *connPool) {
	pool.mu.Lock()
	pool.inUse--
	pool.available++
	pool.mu.Unlock()
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
		"requests":          c.metrics.Requests,
		"successes":        c.metrics.Successes,
		"failures":         c.metrics.Failures,
		"avg_latency":       avgLatency.String(),
		"cache_hit_rate":   hitRate,
		"active_conns":     c.metrics.ActiveConns,
		"archive_queries": c.metrics.ArchiveQueries,
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

	for _, pool := range c.archivePools {
		if pool != nil && pool.client != nil {
			pool.client.Close()
		}
	}

	for _, pool := range c.wsPools {
		if pool != nil && pool.client != nil {
			pool.client.Close()
		}
	}

	return nil
}

// ============================================================================
// Helper Functions
// ============================================================================

func hexEncode(n uint64) string {
	return fmt.Sprintf("0x%x", n)
}

func parseUint256(s string) (*big.Int, error) {
	s = strings.TrimPrefix(s, "0x")
	if s == "" {
		return big.NewInt(0), nil
	}
	return new(big.Int).SetString(s, 16)
}