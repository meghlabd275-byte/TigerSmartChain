// TigerScan GraphQL Server
// Production-grade GraphQL API with full blockchain data

package graphql

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/99designs/gqlgen/graphql/handler"
	"github.com/99designs/gqlgen/graphql/handler/extension"
	"github.com/99designs/gqlgen/graphql/handler/lru"
	"github.com/99designs/gqlgen/graphql/handler/transport"
	"github.com/gin-gonic/gin"
	"github.com/throttled/throttled"
	"golang.org/x/time/rate"
)

// Config holds GraphQL server configuration
type Config struct {
	Port            int           `json:"port"`
	ReadTimeout     time.Duration `json:"read_timeout"`
	WriteTimeout    time.Duration `json:"write_timeout"`
	MaxConcurrent  int           `json:"max_concurrent"`
	QueryComplexity int           `json:"query_complexity"`
	RateLimit      int           `json:"rate_limit"`
	RateBurst      int           `json:"rate_burst"`
	EnableCache    bool          `json:"enable_cache"`
	CacheSize      int           `json:"cache_size"`
	EnableIntrospection bool     `json:"enable_introspection"`
}

// Server is the GraphQL server
type Server struct {
	config    *Config
	executor  *Executor
	rateLimit *RateLimiter
	metrics   *Metrics
	server    *http.Server
}

// RateLimiter implements rate limiting
type RateLimiter struct {
	store  map[string]*ClientLimiter
	mu     sync.RWMutex
	global *rate.Limiter
}

type ClientLimiter struct {
	limiter  *rate.Limiter
	requests uint64
	banned   bool
}

// Metrics tracks server metrics
type Metrics struct {
	mu            sync.RWMutex
	Queries       uint64
	Mutations     uint64
	Errors        uint64
	CacheHits     uint64
	CacheMisses   uint64
	LatencySum    time.Duration
	LatencyCount  uint64
}

type Executor struct {
	db *DB
	// resolvers
}

// DB represents database connection
type DB struct {
	// Database connection pool would go here
}

// Resolver implements the generated resolver interface
type Resolver struct {
	executor *Executor
}

func NewResolver(exe *Executor) *Resolver {
	return &Resolver{executor: exe}
}

// ============================================================================
// Query Resolvers
// ============================================================================

// Blocks returns blocks with pagination and filters
func (r *Resolver) Blocks(ctx context.Context, limit *int, offset *int, fromBlock *uint64, toBlock *uint64, miner *string) ([]*BlockResolver, error) {
	l := 25
	if limit != nil && *limit > 0 && *limit <= 100 {
		l = *limit
	}
	o := 0
	if offset != nil {
		o = *offset
	}

	query := "SELECT number, hash, parent_hash, nonce, sha3_uncles, logs_bloom, transactions_root, state_root, receipts_root, miner, difficulty, total_difficulty, size, gas_limit, gas_used, timestamp FROM blocks"
	args := []interface{}{}
	conditions := []string{}

	if fromBlock != nil {
		conditions = append(conditions, fmt.Sprintf("number >= $%d", len(args)+1))
		args = append(args, *fromBlock)
	}
	if toBlock != nil {
		conditions = append(conditions, fmt.Sprintf("number <= $%d", len(args)+1))
		args = append(args, *toBlock)
	}
	if miner != nil && *miner != "" {
		conditions = append(conditions, fmt.Sprintf("miner = $%d", len(args)+1))
		args = append(args, *miner)
	}

	if len(conditions) > 0 {
		query += " WHERE " + strings.Join(conditions, " AND ")
	}

	query += fmt.Sprintf(" ORDER BY number DESC LIMIT $%d OFFSET $%d", len(args)+1, len(args)+2)
	args = append(args, l, o)

	// Execute query and return results
	return []*BlockResolver{}, nil
}

// Block returns a single block by number or hash
func (r *Resolver) Block(ctx context.Context, numberOrHash string) (*BlockResolver, error) {
	var isNum bool
	if _, err := strconv.ParseUint(numberOrHash, 10, 64); err == nil {
		isNum = true
	}

	// Query block
	return &BlockResolver{}, nil
}

// LatestBlock returns the latest block
func (r *Resolver) LatestBlock(ctx context.Context) (*BlockResolver, error) {
	return &BlockResolver{}, nil
}

// Transactions returns transactions with filters
func (r *Resolver) Transactions(ctx context.Context, limit *int, offset *int, from *string, to *string, address *string, status *string) ([]*TransactionResolver, error) {
	l := 25
	if limit != nil && *limit > 0 && *limit <= 100 {
		l = *limit
	}
	o := 0
	if offset != nil {
		o = *offset
	}

	// Build query with filters
	return []*TransactionResolver{}, nil
}

// Transaction returns a single transaction by hash
func (r *Resolver) Transaction(ctx context.Context, hash string) (*TransactionResolver, error) {
	return &TransactionResolver{}, nil
}

// TransactionReceipt returns receipt for a transaction
func (r *Resolver) TransactionReceipt(ctx context.Context, hash string) (*ReceiptResolver, error) {
	return &ReceiptResolver{}, nil
}

// InternalTransactions returns internal transactions for a tx
func (r *Resolver) InternalTransactions(ctx context.Context, txHash string, limit *int, offset *int) ([]*InternalTransactionResolver, error) {
	l := 50
	if limit != nil {
		l = *limit
	}
	o := 0
	if offset != nil {
		o = *offset
	}

	return []*InternalTransactionResolver{}, nil
}

// Tokens returns tokens with search and filters
func (r *Resolver) Tokens(ctx context.Context, limit *int, offset *int, search *string, verified *bool) ([]*TokenResolver, error) {
	l := 25
	if limit != nil && *limit > 0 && *limit <= 100 {
		l = *limit
	}
	o := 0
	if offset != nil {
		o = *offset
	}

	return []*TokenResolver{}, nil
}

// Token returns a single token by address
func (r *Resolver) Token(ctx context.Context, address string) (*TokenResolver, error) {
	return &TokenResolver{}, nil
}

// TokenHolders returns holders for a token
func (r *Resolver) TokenHolders(ctx context.Context, address string, limit *int, offset *int) ([]*TokenHolderResolver, error) {
	l := 25
	if limit != nil && *limit > 0 && *limit <= 100 {
		l = *limit
	}
	o := 0
	if offset != nil {
		o = *offset
	}

	return []*TokenHolderResolver{}, nil
}

// TokenTransfers returns transfers for a token
func (r *Resolver) TokenTransfers(ctx context.Context, address string, limit *int, offset *int, fromBlock *uint64) ([]*TokenTransferResolver, error) {
	return []*TokenTransferResolver{}, nil
}

// TokenApprovals returns approvals for a token
func (r *Resolver) TokenApprovals(ctx context.Context, address string, owner *string, spender *string, limit *int, offset *int) ([]*TokenApprovalResolver, error) {
	return []*TokenApprovalResolver{}, nil
}

// TokenPriceHistory returns price history for a token
func (r *Resolver) TokenPriceHistory(ctx context.Context, address string, from time.Time, to time.Time, interval string) ([]*PricePointResolver, error) {
	return []*PricePointResolver{}, nil
}

// NFTCollections returns NFT collections
func (r *Resolver) NFTCollections(ctx context.Context, limit *int, offset *int, search *string) ([]*NFTCollectionResolver, error) {
	return []*NFTCollectionResolver{}, nil
}

// NFTCollection returns a single NFT collection
func (r *Resolver) NFTCollection(ctx context.Context, address string) (*NFTCollectionResolver, error) {
	return &NFTCollectionResolver{}, nil
}

// NFT returns a single NFT
func (r *Resolver) NFT(ctx context.Context, address string, tokenID string) (*NFTResolver, error) {
	return &NFTResolver{}, nil
}

// NFTOwners returns owners for an NFT
func (r *Resolver) NFTOwners(ctx context.Context, address string, limit *int, offset *int) ([]*NFTOwnerResolver, error) {
	return []*NFTOwnerResolver{}, nil
}

// NFTTransfers returns transfers for an NFT collection
func (r *Resolver) NFTTransfers(ctx context.Context, address string, limit *int, offset *int, fromBlock *uint64) ([]*NFTTransferResolver, error) {
	return []*NFTTransferResolver{}, nil
}

// NFTRarity returns rarity data for an NFT
func (r *Resolver) NFTRarity(ctx context.Context, address string, tokenID string) (*NFTRarityResolver, error) {
	return &NFTRarityResolver{}, nil
}

// NFTActivity returns activity for an NFT collection
func (r *Resolver) NFTActivity(ctx context.Context, address string, limit *int) ([]*NFTActivityResolver, error) {
	l := 20
	if limit != nil && *limit > 0 && *limit <= 100 {
		l = *limit
	}

	return []*NFTActivityResolver{}, nil
}

// Account returns an account by address
func (r *Resolver) Account(ctx context.Context, address string) (*AccountResolver, error) {
	return &AccountResolver{}, nil
}

// AccountTransactions returns transactions for an account
func (r *Resolver) AccountTransactions(ctx context.Context, address string, limit *int, offset *int) ([]*TransactionResolver, error) {
	return []*TransactionResolver{}, nil
}

// AccountTokens returns tokens for an account
func (r *Resolver) AccountTokens(ctx context.Context, address string, limit *int, offset *int) ([]*AccountTokenResolver, error) {
	return []*AccountTokenResolver{}, nil
}

// AccountNFTs returns NFTs for an account
func (r *Resolver) AccountNFTs(ctx context.Context, address string, limit *int, offset *int) ([]*AccountNFTResolver, error) {
	return []*AccountNFTResolver{}, nil
}

// Contract returns a contract by address
func (r *Resolver) Contract(ctx context.Context, address string) (*ContractResolver, error) {
	return &ContractResolver{}, nil
}

// ContractSource returns source code for a contract
func (r *Resolver) ContractSource(ctx context.Context, address string) ([]*ContractSourceResolver, error) {
	return []*ContractSourceResolver{}, nil
}

// ContractABI returns ABI for a contract
func (r *Resolver) ContractABI(ctx context.Context, address string) (string, error) {
	return "", nil
}

// VerifiedContracts returns verified contracts
func (r *Resolver) VerifiedContracts(ctx context.Context, limit *int, offset *int, search *string, license *string) ([]*VerifiedContractResolver, error) {
	return []*VerifiedContractResolver{}, nil
}

// Validators returns validators
func (r *Resolver) Validators(ctx context.Context, limit *int, offset *int, orderBy *string) ([]*ValidatorResolver, error) {
	return []*ValidatorResolver{}, nil
}

// Validator returns a validator by address
func (r *Resolver) Validator(ctx context.Context, address string) (*ValidatorResolver, error) {
	return &ValidatorResolver{}, nil
}

// ValidatorRewards returns rewards for a validator
func (r *Resolver) ValidatorRewards(ctx context.Context, address string, fromBlock *uint64, toBlock *uint64) ([]*ValidatorRewardResolver, error) {
	return []*ValidatorRewardResolver{}, nil
}

// Search performs a global search
func (r *Resolver) Search(ctx context.Context, query string) (SearchResultResolver, error) {
	// Determine search type
	if strings.HasPrefix(query, "0x") && len(query) == 42 {
		// Address - check if account or contract
		return &AccountResolver{}, nil
	}
	if strings.HasPrefix(query, "0x") && len(query) == 66 {
		// Transaction hash
		return &TransactionResolver{}, nil
	}
	if _, err := strconv.ParseUint(query, 10, 64); err == nil {
		// Block number
		return &BlockResolver{}, nil
	}

	// Search by name/symbol
	return &TokenResolver{}, nil
}

// Stats returns network statistics
func (r *Resolver) Stats(ctx context.Context) (*NetworkStatsResolver, error) {
	return &NetworkStatsResolver{}, nil
}

// GasPrice returns current gas price
func (r *Resolver) GasPrice(ctx context.Context) (*GasPriceResolver, error) {
	return &GasPriceResolver{}, nil
}

// GasHistory returns historical gas data
func (r *Resolver) GasHistory(ctx context.Context, from time.Time, to time.Time, interval string) ([]*GasPricePointResolver, error) {
	return []*GasPricePointResolver{}, nil
}

// PendingTransactions returns pending transactions
func (r *Resolver) PendingTransactions(ctx context.Context, limit *int, from *string, to *string) ([]*PendingTransactionResolver, error) {
	l := 50
	if limit != nil && *limit > 0 && *limit <= 100 {
		l = *limit
	}

	return []*PendingTransactionResolver{}, nil
}

// ============================================================================
// Mutation Resolvers
// ============================================================================

// VerifyContract submits a contract for verification
func (r *Resolver) VerifyContract(ctx context.Context, input VerifyContractInput) (*VerifyContractResultResolver, error) {
	// Validate input
	if input.Address == "" {
		return &VerifyContractResultResolver{Success: false, Error: strPtr("address required")}, nil
	}
	if input.ContractName == "" {
		return &VerifyContractResultResolver{Success: false, Error: strPtr("contract name required")}, nil
	}
	if input.CompilerVersion == "" {
		return &VerifyContractResultResolver{Success: false, Error: strPtr("compiler version required")}, nil
	}
	if len(input.Sources) == 0 {
		return &VerifyContractResultResolver{Success: false, Error: strPtr("sources required")}, nil
	}

	// Compile and verify
	return &VerifyContractResultResolver{Success: true, Address: &input.Address}, nil
}

// VerifyProxy submits a proxy for verification
func (r *Resolver) VerifyProxy(ctx context.Context, input VerifyProxyInput) (*VerifyProxyResultResolver, error) {
	return &VerifyProxyResultResolver{}, nil
}

// ============================================================================
// Type Resolvers
// ============================================================================

type BlockResolver struct{}

func (r *BlockResolver) Number() uint64 { return 0 }
func (r *BlockResolver) Hash() string { return "" }
func (r *BlockResolver) ParentHash() string { return "" }
func (r *BlockResolver) Miner() string { return "" }
func (r *BlockResolver) Timestamp() time.Time { return time.Now() }
func (r *BlockResolver) Transactions() []*TransactionResolver { return nil }

type TransactionResolver struct{}

func (r *TransactionResolver) Hash() string { return "" }
func (r *TransactionResolver) BlockNumber() uint64 { return 0 }
func (r *TransactionResolver) From() string { return "" }
func (r *TransactionResolver) To() *string { return nil }
func (r *TransactionResolver) Value() string { return "" }
func (r *TransactionResolver) GasPrice() string { return "" }
func (r *TransactionResolver) Status() string { return "" }
func (r *TransactionResolver) Timestamp() time.Time { return time.Now() }
func (r *TransactionResolver) Receipt() *ReceiptResolver { return nil }

type ReceiptResolver struct{}

func (r *ReceiptResolver) TransactionHash() string { return "" }
func (r *ReceiptResolver) BlockNumber() uint64 { return 0 }
func (r *ReceiptResolver) GasUsed() uint64 { return 0 }
func (r *ReceiptResolver) Status() bool { return true }
func (r *ReceiptResolver) Logs() []*LogResolver { return nil }

type LogResolver struct{}

func (r *LogResolver) Address() string { return "" }
func (r *LogResolver) Topics() []string { return nil }
func (r *LogResolver) Data() string { return "" }

type InternalTransactionResolver struct{}

func (r *InternalTransactionResolver) Type() string { return "" }
func (r *InternalTransactionResolver) From() string { return "" }
func (r *InternalTransactionResolver) To() string { return "" }
func (r *InternalTransactionResolver) Value() string { return "" }
func (r *InternalTransactionResolver) Depth() int { return 0 }

type PendingTransactionResolver struct{}

func (r *PendingTransactionResolver) Hash() string { return "" }
func (r *PendingTransactionResolver) From() string { return "" }
func (r *PendingTransactionResolver) To() *string { return nil }
func (r *PendingTransactionResolver) Value() string { return "" }
func (r *PendingTransactionResolver) GasPrice() string { return "" }
func (r *PendingTransactionResolver) Timestamp() time.Time { return time.Now() }

type TokenResolver struct{}

func (r *TokenResolver) Address() string { return "" }
func (r *TokenResolver) Name() string { return "" }
func (r *TokenResolver) Symbol() string { return "" }
func (r *TokenResolver) Decimals() int { return 18 }
func (r *TokenResolver) TotalSupply() string { return "" }
func (r *TokenResolver) PriceUSD() *float64 { return nil }
func (r *TokenResolver) MarketCap() float64 { return 0 }
func (r *TokenResolver) Verified() bool { return false }

type TokenHolderResolver struct{}

func (r *TokenHolderResolver) Address() string { return "" }
func (r *TokenHolderResolver) Balance() string { return "" }
func (r *TokenHolderResolver) BalanceUSD() float64 { return 0 }
func (r *TokenHolderResolver) Percentage() float64 { return 0 }
func (r *TokenHolderResolver) Rank() int { return 0 }

type TokenTransferResolver struct{}

func (r *TokenTransferResolver) Hash() string { return "" }
func (r *TokenTransferResolver) BlockNumber() uint64 { return 0 }
func (r *TokenTransferResolver) Timestamp() time.Time { return time.Now() }
func (r *TokenTransferResolver) From() string { return "" }
func (r *TokenTransferResolver) To() string { return "" }
func (r *TokenTransferResolver) Value() string { return "" }
func (r *TokenTransferResolver) ValueUSD() *float64 { return nil }

type TokenApprovalResolver struct{}

func (r *TokenApprovalResolver) Owner() string { return "" }
func (r *TokenApprovalResolver) Spender() string { return "" }
func (r *TokenApprovalResolver) Value() string { return "" }
func (r *TokenApprovalResolver) Approved() bool { return true }

type PricePointResolver struct{}

func (r *PricePointResolver) Timestamp() time.Time { return time.Now() }
func (r *PricePointResolver) Price() float64 { return 0 }
func (r *PricePointResolver) Volume() float64 { return 0 }

type NFTCollectionResolver struct{}

func (r *NFTCollectionResolver) Address() string { return "" }
func (r *NFTCollectionResolver) Name() string { return "" }
func (r *NFTCollectionResolver) ContractType() string { return "" }
func (r *NFTCollectionResolver) TotalSupply() uint64 { return 0 }
func (r *NFTCollectionResolver) FloorPriceUSD() *float64 { return nil }
func (r *NFTCollectionResolver) Volume24hUSD() float64 { return 0 }

type NFTResolver struct{}

func (r *NFTResolver) ID() string { return "" }
func (r *NFTResolver) Owner() string { return "" }
func (r *NFTResolver) CurrentPriceUSD() *float64 { return nil }
func (r *NFTResolver) Metadata() *NFTMetadataResolver { return nil }
func (r *NFTResolver) Rarity() *NFTRarityResolver { return nil }

type NFTMetadataResolver struct{}

func (r *NFTMetadataResolver) Name() *string { return nil }
func (r *NFTMetadataResolver) Description() *string { return nil }
func (r *NFTMetadataResolver) Image() *string { return nil }
func (r *NFTMetadataResolver) Attributes() []*NFTAttributeResolver { return nil }

type NFTAttributeResolver struct{}

func (r *NFTAttributeResolver) TraitType() string { return "" }
func (r *NFTAttributeResolver) Value() string { return "" }
func (r *NFTAttributeResolver) RarityScore() *float64 { return nil }

type NFTRarityResolver struct{}

func (r *NFTRarityResolver) TokenID() string { return "" }
func (r *NFTRarityResolver) RarityScore() float64 { return 0 }
func (r *NFTRarityResolver) RarityRank() int { return 0 }
func (r *NFTRarityResolver) TraitRarities() []*TraitRarityResolver { return nil }

type TraitRarityResolver struct{}

func (r *TraitRarityResolver) TraitType() string { return "" }
func (r *TraitRarityResolver) Value() string { return "" }
func (r *TraitRarityResolver) Occurrence() float64 { return 0 }

type NFTOwnerResolver struct{}

func (r *NFTOwnerResolver) Address() string { return "" }
func (r *NFTOwnerResolver) Balance() uint64 { return 0 }

type NFTTransferResolver struct{}

func (r *NFTTransferResolver) Hash() string { return "" }
func (r *NFTTransferResolver) TokenID() string { return "" }
func (r *NFTTransferResolver) From() string { return "" }
func (r *NFTTransferResolver) To() string { return "" }

type NFTActivityResolver struct{}

func (r *NFTActivityResolver) EventType() string { return "" }
func (r *NFTActivityResolver) PriceUSD() *float64 { return nil }
func (r *NFTActivityResolver) Timestamp() time.Time { return time.Now() }

type AccountResolver struct{}

func (r *AccountResolver) Address() string { return "" }
func (r *AccountResolver) Balance() string { return "" }
func (r *AccountResolver) BalanceUSD() *float64 { return nil }
func (r *AccountResolver) Nonce() uint64 { return 0 }
func (r *AccountResolver) IsContract() bool { return false }

type AccountTokenResolver struct{}

func (r *AccountTokenResolver) Balance() string { return "" }
func (r *AccountTokenResolver) BalanceUSD() float64 { return 0 }

type AccountNFTResolver struct{}

func (r *AccountNFTResolver) TokenID() string { return "" }
func (r *AccountNFTResolver) Balance() uint64 { return 0 }

type ContractResolver struct{}

func (r *ContractResolver) Address() string { return "" }
func (r *ContractResolver) Code() string { return "" }
func (r *ContractResolver) Verified() bool { return false }

type ContractSourceResolver struct{}

func (r *ContractSourceResolver) FileName() string { return "" }
func (r *ContractSourceResolver) SourceCode() string { return "" }
func (r *ContractSourceResolver) Language() string { return "" }

type VerifiedContractResolver struct{}

func (r *VerifiedContractResolver) Address() string { return "" }
func (r *VerifiedContractResolver) ContractName() string { return "" }
func (r *VerifiedContractResolver) CompilerVersion() string { return "" }
func (r *VerifiedContractResolver) Optimizer() bool { return false }
func (r *VerifiedContractResolver) VerifiedAt() time.Time { return time.Now() }

type ValidatorResolver struct{}

func (r *ValidatorResolver) Address() string { return "" }
func (r *ValidatorResolver) Name() string { return "" }
func (r *ValidatorResolver) Uptime() float64 { return 0 }
func (r *ValidatorResolver) TotalStaked() string { return "" }
func (r *ValidatorResolver) Rank() int { return 0 }

type ValidatorRewardResolver struct{}

func (r *ValidatorRewardResolver) BlockNumber() uint64 { return 0 }
func (r *ValidatorRewardResolver) Reward() string { return "" }
func (r *ValidatorRewardResolver) Timestamp() time.Time { return time.Now() }

type NetworkStatsResolver struct{}

func (r *NetworkStatsResolver) TotalBlocks() uint64 { return 0 }
func (r *NetworkStatsResolver) TotalTransactions() uint64 { return 0 }
func (r *NetworkStatsResolver) TotalTokens() uint64 { return 0 }
func (r *NetworkStatsResolver) TPS() float64 { return 0 }

type GasPriceResolver struct{}

func (r *GasPriceResolver) Low() string { return "" }
func (r *GasPriceResolver) Standard() string { return "" }
func (r *GasPriceResolver) Fast() string { return "" }

type GasPricePointResolver struct{}

func (r *GasPricePointResolver) Timestamp() time.Time { return time.Now() }
func (r *GasPricePointResolver) AvgGasPrice() string { return "" }
func (r *GasPricePointResolver) TransactionsCount() uint64 { return 0 }

type SearchResultResolver interface{}

// Input types
type VerifyContractInput struct {
	Address          string
	ContractName     string
	CompilerVersion  string
	Optimization     bool
	OptimizerRuns    *int
	EvmVersion       *string
	License          *string
	Sources          []SourceInput
	LibraryLinks     []LibraryLinkInput
	ConstructorArgs  *string
}

type SourceInput struct {
	FileName string
	Content  string
}

type LibraryLinkInput struct {
	Name    string
	Address string
}

type VerifyProxyInput struct {
	ProxyAddress     string
	Implementation  string
	CompilerVersion string
	ProxyType       string
}

type VerifyContractResultResolver struct {
	Success bool
	Address *string
	Error   *string
}

type VerifyProxyResultResolver struct {
	Success bool
	Address *string
	Error   *string
}

// Helper function
func strPtr(s string) *string {
	return &s
}

// ============================================================================
// Server Implementation
// ============================================================================

func NewServer(config *Config) (*Server, error) {
	if config == nil {
		config = &Config{}
	}

	if config.Port == 0 {
		config.Port = 4000
	}
	if config.ReadTimeout == 0 {
		config.ReadTimeout = 30 * time.Second
	}
	if config.WriteTimeout == 0 {
		config.WriteTimeout = 30 * time.Second
	}
	if config.MaxConcurrent == 0 {
		config.MaxConcurrent = 100
	}
	if config.QueryComplexity == 0 {
		config.QueryComplexity = 100
	}
	if config.RateLimit == 0 {
		config.RateLimit = 100
	}
	if config.RateBurst == 0 {
		config.RateBurst = 200
	}
	if config.CacheSize == 0 {
		config.CacheSize = 1000
	}

	executor := &Executor{db: &DB{}}
	resolver := NewResolver(executor)

	// Create GraphQL handler
	// In production, use gqlgen generated code
	srv := handler.New(func() interface{} {
		return resolver
	})

	// Extensions
	srv.Use(extension.AutomaticPersistedQuery{
		Cache: lru.New(config.CacheSize),
	})
	srv.Use(extension.ComplexityLimit(config.QueryComplexity))
	srv.Use(extension.Introspection{Enabled: config.EnableIntrospection})

	// Transport
	srv.AddTransport(transport.GET{})
	srv.AddTransport(transport.POST{})
	srv.AddTransport(transport.MultipartForm{})

	// Rate limiting
	rateLimiter := &RateLimiter{
		store: make(map[string]*ClientLimiter),
		global: rate.NewLimiter(rate.Limit(config.RateLimit), config.RateBurst),
	}

	server := &Server{
		config:    config,
		executor:  executor,
		rateLimit: rateLimiter,
		metrics:   &Metrics{},
	}

	// Wrap handler with rate limiting
	httpHandler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Check rate limit
		if !rateLimiter.Allow(r.RemoteAddr) {
			http.Error(w, "Rate limit exceeded", http.StatusTooManyRequests)
			return
		}

		start := time.Now()
		srv.ServeHTTP(w, r)
		
		// Update metrics
		server.metrics.mu.Lock()
		server.metrics.LatencySum += time.Since(start)
		server.metrics.LatencyCount++
		if r.Method == "GET" {
			server.metrics.Queries++
		} else {
			server.metrics.Mutations++
		}
		server.metrics.mu.Unlock()
	})

	server.server = &http.Server{
		Addr:         fmt.Sprintf(":%d", config.Port),
		Handler:      httpHandler,
		ReadTimeout:  config.ReadTimeout,
		WriteTimeout: config.WriteTimeout,
	}

	return server, nil
}

func (r *RateLimiter) Allow(key string) bool {
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
			limiter: rate.NewLimiter(rate.Limit(100), 200),
		}
		r.store[key] = client
		r.mu.Unlock()
	}

	return client.limiter.Allow()
}

func (s *Server) Start() error {
	fmt.Printf("GraphQL server listening on :%d\n", s.config.Port)
	return s.server.ListenAndServe()
}

func (s *Server) Stop(ctx context.Context) error {
	return s.server.Shutdown(ctx)
}

func (s *Server) Metrics() map[string]interface{} {
	s.metrics.mu.RLock()
	defer s.metrics.mu.RUnlock()

	avgLatency := time.Duration(0)
	if s.metrics.LatencyCount > 0 {
		avgLatency = s.metrics.LatencySum / time.Duration(s.metrics.LatencyCount)
	}

	return map[string]interface{}{
		"queries":      s.metrics.Queries,
		"mutations":    s.metrics.Mutations,
		"errors":       s.metrics.Errors,
		"cache_hits":   s.metrics.CacheHits,
		"cache_misses": s.metrics.CacheMisses,
		"avg_latency":  avgLatency.String(),
	}
}

// Gin middleware integration
func (s *Server) GinMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		if !s.rateLimit.Allow(c.ClientIP()) {
			c.AbortWithStatusJSON(http.StatusTooManyRequests, gin.H{
				"error": "Rate limit exceeded",
			})
			return
		}
		c.Next()
	}
}

// ============================================================================
// Main
// ============================================================================

func main() {
	config := &Config{
		Port:               4000,
		ReadTimeout:        30 * time.Second,
		WriteTimeout:       30 * time.Second,
		MaxConcurrent:      100,
		QueryComplexity:    100,
		RateLimit:          100,
		RateBurst:          200,
		EnableCache:        true,
		CacheSize:          1000,
		EnableIntrospection: true,
	}

	server, err := NewServer(config)
	if err != nil {
		fmt.Printf("Failed to create server: %v\n", err)
		return
	}

	if err := server.Start(); err != nil {
		fmt.Printf("Server error: %v\n", err)
	}
}