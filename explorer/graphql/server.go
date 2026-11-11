// Package graphql implements the TigerScan GraphQL API.
//
// The schema is built programmatically with github.com/graphql-go/graphql and
// served through github.com/graphql-go/handler. Every query field executes a
// parameterised SQL statement against the configured *sql.DB; there is no mock,
// sample or fabricated data anywhere in this package. When no database is
// configured the DB-backed fields return an explicit error.
package graphql

import (
	"database/sql"
	"errors"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/graphql-go/graphql"
	"github.com/graphql-go/graphql/language/ast"
	"github.com/graphql-go/handler"
	"golang.org/x/time/rate"
)

const (
	defaultLimit   = 25
	maxLimit       = 100
	rateLimiterTTL = 10 * time.Minute
	maxRateClients = 10000
)

// errNoDatabase is returned by every DB-backed field when Config.DB is nil.
var errNoDatabase = errors.New("graphql: database not configured")

// Config holds GraphQL server configuration.
type Config struct {
	Port                int           `json:"port"`
	ReadTimeout         time.Duration `json:"read_timeout"`
	WriteTimeout        time.Duration `json:"write_timeout"`
	MaxConcurrent       int           `json:"max_concurrent"`
	QueryComplexity     int           `json:"query_complexity"`
	RateLimit           int           `json:"rate_limit"`
	RateBurst           int           `json:"rate_burst"`
	EnableCache         bool          `json:"enable_cache"`
	CacheSize           int           `json:"cache_size"`
	EnableIntrospection bool          `json:"enable_introspection"`

	// DB is the PostgreSQL pool backing every query and mutation. When nil the
	// server still starts but DB-backed fields return an error.
	DB *sql.DB `json:"-"`

	// AllowedOrigins lists origins permitted to make cross-origin requests. An
	// entry may be a full origin ("https://scan.example") or a bare host
	// ("scan.example"); "*" allows every origin. Same-host requests are always
	// allowed.
	AllowedOrigins []string `json:"allowed_origins"`

	// VerifierURL is the base URL of the contract verification service. When
	// empty, verifyContract/verifyProxy report that verification is unavailable
	// rather than claiming success.
	VerifierURL string `json:"verifier_url"`

	// VerifierTimeout bounds a single verification HTTP request.
	VerifierTimeout time.Duration `json:"verifier_timeout"`
}

// Server is the GraphQL server.
type Server struct {
	config     *Config
	schema     graphql.Schema
	gqlHandler *handler.Handler
	rateLimit  *RateLimiter
	metrics    *Metrics
	server     *http.Server
}

// RateLimiter implements per-client-IP token bucket rate limiting with a
// bounded, self-evicting client map.
type RateLimiter struct {
	mu     sync.Mutex
	store  map[string]*ClientLimiter
	limit  rate.Limit
	burst  int
	global *rate.Limiter
}

// ClientLimiter tracks a single client's token bucket.
type ClientLimiter struct {
	limiter  *rate.Limiter
	requests uint64
	lastSeen time.Time
	banned   bool
}

// Metrics tracks server metrics.
type Metrics struct {
	mu           sync.RWMutex
	Queries      uint64
	Mutations    uint64
	Errors       uint64
	CacheHits    uint64
	CacheMisses  uint64
	LatencySum   time.Duration
	LatencyCount uint64
}

// BigInt serialises 64-bit integers (block numbers, wei amounts, unix
// timestamps) without the int32 truncation that graphql-go's built-in Int
// scalar applies.
var BigInt = graphql.NewScalar(graphql.ScalarConfig{
	Name:        "BigInt",
	Description: "A signed 64-bit integer, serialised as a string when it exceeds the 32-bit GraphQL Int range.",
	Serialize: func(value interface{}) interface{} {
		switch v := value.(type) {
		case nil:
			return nil
		case int:
			return v
		case int64:
			return v
		case *int64:
			if v == nil {
				return nil
			}
			return *v
		case string:
			return v
		case *string:
			if v == nil {
				return nil
			}
			return *v
		case float64:
			return v
		case bool:
			return v
		case sql.NullInt64:
			if !v.Valid {
				return nil
			}
			return v.Int64
		case sql.NullString:
			if !v.Valid {
				return nil
			}
			return v.String
		}
		return fmt.Sprintf("%v", value)
	},
	ParseValue: func(value interface{}) interface{} {
		switch v := value.(type) {
		case int:
			return v
		case int64:
			return v
		case float64:
			return int64(v)
		case string:
			n, err := strconv.ParseInt(v, 10, 64)
			if err != nil {
				return nil
			}
			return n
		}
		return nil
	},
	ParseLiteral: func(valueAST ast.Value) interface{} {
		switch v := valueAST.(type) {
		case *ast.IntValue:
			n, err := strconv.ParseInt(v.Value, 10, 64)
			if err != nil {
				return nil
			}
			return n
		case *ast.StringValue:
			n, err := strconv.ParseInt(v.Value, 10, 64)
			if err != nil {
				return nil
			}
			return n
		}
		return nil
	},
})

// ---------------------------------------------------------------------------
// small helpers
// ---------------------------------------------------------------------------

func clampLimit(v int) int {
	if v <= 0 {
		return defaultLimit
	}
	if v > maxLimit {
		return maxLimit
	}
	return v
}

func clampOffset(v int) int {
	if v < 0 {
		return 0
	}
	return v
}

func intArg(p graphql.ResolveParams, name string) (int, bool) {
	v, ok := p.Args[name]
	if !ok {
		return 0, false
	}
	switch t := v.(type) {
	case int:
		return t, true
	case int64:
		return int(t), true
	case float64:
		return int(t), true
	}
	return 0, false
}

func strArg(p graphql.ResolveParams, name string) string {
	if v, ok := p.Args[name]; ok {
		if s, ok := v.(string); ok {
			return s
		}
	}
	return ""
}

func boolArg(p graphql.ResolveParams, name string) (bool, bool) {
	if v, ok := p.Args[name]; ok {
		if b, ok := v.(bool); ok {
			return b, true
		}
	}
	return false, false
}

func nullString(ns sql.NullString) interface{} {
	if !ns.Valid {
		return nil
	}
	return ns.String
}

func nullInt64(n sql.NullInt64) interface{} {
	if !n.Valid {
		return nil
	}
	return n.Int64
}

func isHex(s string, hexLen int) bool {
	if len(s) != 2+hexLen || !strings.HasPrefix(strings.ToLower(s), "0x") {
		return false
	}
	for _, c := range s[2:] {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')) {
			return false
		}
	}
	return true
}

// ---------------------------------------------------------------------------
// result row structs (mapped 1:1 from SQL columns)
// ---------------------------------------------------------------------------

type blockRow struct {
	Number                int64
	Hash                  string
	ParentHash            string
	Nonce                 *string
	Sha3Uncles            *string
	LogsBloom             *string
	TransactionsRoot      *string
	StateRoot             *string
	ReceiptsRoot          *string
	Miner                 string
	Difficulty            *string
	TotalDifficulty       *string
	GasLimit              int64
	GasUsed               int64
	Timestamp             int64
	Size                  int64
	ExtraData             *string
	BaseFeePerGas         *int64
	BlobGasUsed           *int64
	ExcessBlobGas         *int64
	ParentBeaconBlockRoot *string
	TxCount               int
	UncleCount            int
	Reward                *string
	IsUncle               bool
}

type txRow struct {
	Hash                 string
	Nonce                int64
	BlockNumber          *int64
	BlockHash            *string
	TransactionIndex     *int
	From                 string
	To                   *string
	Value                string
	GasPrice             int64
	GasLimit             int64
	GasUsed              *int64
	MaxFeePerGas         *int64
	MaxPriorityFeePerGas *int64
	Input                *string
	V                    int64
	R                    *string
	S                    *string
	ChainID              *int64
	TransactionType      *string
	Status               string
	CumulativeGasUsed    *string
	EffectiveGasPrice    *string
	ContractAddress      *string
	BlockTimestamp       *int64
}

type logRow struct {
	Address          string
	Topics           []string
	Data             string
	LogIndex         int
	TransactionIndex int
	BlockNumber      int64
}

type receiptRow struct {
	TransactionHash   string
	BlockNumber       *int64
	BlockHash         *string
	ContractAddress   *string
	CumulativeGasUsed *string
	GasUsed           *int64
	EffectiveGasPrice *string
	Status            string
	Logs              []*logRow
}

type internalTxRow struct {
	TransactionHash string
	BlockNumber     int64
	TraceIndex      int
	SubtraceIndex   int
	CallType        string
	From            string
	To              *string
	Value           *string
	Gas             *string
	GasUsed         *string
	Input           *string
	Output          *string
	Error           *string
	Depth           int
	Creates         *string
	Success         bool
}

type tokenRow struct {
	Address           string
	Name              string
	Symbol            string
	Decimals          int
	TotalSupply       *string
	HoldersCount      int64
	TransfersCount    int64
	CirculatingSupply *string
	PriceUSD          *string
	PriceChange24h    *string
	MarketCap         *string
	Volume24h         *string
	IsVerified        bool
	IsSpam            bool
	SpamScore         int
	ContractAddress   *string
	LogoURL           *string
	Description       *string
	Website           *string
}

type tokenHolderRow struct {
	Address         string
	Balance         string
	BalanceUSD      *string
	PercentHoldings *string
	IsContract      bool
	UpdatedBlock    *int64
}

type tokenTransferRow struct {
	TokenAddress    string
	From            string
	To              string
	Value           string
	TransactionHash string
	BlockNumber     int64
	LogIndex        int
	Timestamp       *int64
}

type tokenApprovalRow struct {
	TokenAddress    string
	Owner           string
	Spender         string
	Value           string
	TransactionHash string
	BlockNumber     int64
	LogIndex        int
}

type pricePointRow struct {
	Timestamp int64
	AvgPrice  *string
	MaxPrice  *string
}

type nftCollectionRow struct {
	Address        string
	Name           string
	Symbol         *string
	ContractType   string
	TotalSupply    *int64
	HoldersCount   int64
	TransfersCount int64
	FloorPrice     *string
	Volume24h      *string
	MarketCap      *string
	Description    *string
	ImageURL       *string
	ExternalURL    *string
	BannerURL      *string
	IsVerified     bool
}

type nftAttributeRow struct {
	TraitType   string
	Value       string
	DisplayType *string
	RarityScore *string
}

type nftRow struct {
	TokenAddress          string
	TokenID               string
	Owner                 string
	URI                   *string
	MetadataJSON          *string
	Name                  *string
	Description           *string
	ImageURL              *string
	ExternalURL           *string
	Attributes            []*nftAttributeRow
	IsBurned              bool
	LastTransferBlock     *int64
	LastTransferTimestamp *int64
}

type nftOwnerRow struct {
	TokenAddress string
	TokenID      string
	Owner        string
	UpdatedBlock int64
}

type nftTransferRow struct {
	TokenAddress    string
	From            string
	To              string
	TokenID         string
	Value           *string
	TransactionHash string
	BlockNumber     int64
	LogIndex        int
	Timestamp       *int64
}

type traitRarityRow struct {
	TraitType  string
	Value      string
	Occurrence *string
}

type nftRarityRow struct {
	CollectionAddress string
	TokenID           string
	RarityScore       *string
	Rank              *int
	Traits            []*traitRarityRow
}

type nftActivityRow struct {
	EventType       string
	TokenAddress    string
	TokenID         string
	From            string
	To              string
	TransactionHash string
	BlockNumber     int64
	LogIndex        int
	Timestamp       *int64
}

type accountRow struct {
	Address           string
	Balance           *string
	Nonce             int64
	CodeHash          *string
	IsContract        bool
	IsVerified        bool
	TokenBalanceCount int64
	NFTBalanceCount   int64
}

type accountTokenRow struct {
	Token      *tokenRow
	Balance    string
	BalanceUSD *string
}

type accountNFTRow struct {
	TokenAddress string
	TokenID      string
	Owner        string
	UpdatedBlock *int64
}

type contractRow struct {
	Address             string
	ContractName        *string
	Compiler            *string
	CompilerVersion     *string
	OptimizationEnabled *bool
	OptimizationRuns    *int
	EvmVersion          *string
	LicenseType         *string
	SourceCode          string
	ABI                 *string
	Bytecode            *string
	RuntimeBytecode     *string
	ContractType        *string
	IsVerified          bool
	VerificationStatus  *string
	VerifiedAt          *int64
}

type contractSourceRow struct {
	FileName        string
	SourceCode      string
	Language        *string
	CompilerVersion *string
}

type verifiedContractRow struct {
	Address          string
	ContractName     *string
	CompilerVersion  *string
	Optimization     *bool
	OptimizationRuns *int
	EvmVersion       *string
	License          *string
	VerifiedAt       *int64
}

type validatorRow struct {
	Address        string
	Name           *string
	Moniker        *string
	Uptime         *string
	CommissionRate *string
	TotalStake     *string
	Tokens         *string
	BlocksCount    *int64
	IsJailed       bool
	IsActive       bool
	Status         *string
}

type validatorRewardRow struct {
	BlockNumber     int64
	BlockHash       string
	Validator       string
	Amount          string
	TransactionHash *string
	Timestamp       *int64
}

type networkStatsRow struct {
	TotalBlocks       int64
	TotalTransactions int64
	TotalTokens       int64
	TotalAccounts     int64
	TotalContracts    int64
	MinTimestamp      *int64
	MaxTimestamp      *int64
	LatestBaseFee     *int64
}

type gasPriceRow struct {
	Low      *float64
	Standard *float64
	Fast     *float64
	BaseFee  *float64
}

type gasPricePointRow struct {
	Timestamp         int64
	AvgGasPrice       *string
	MinGasPrice       *string
	MaxGasPrice       *string
	TransactionsCount int64
}

type pendingTxRow struct {
	Hash      string
	Nonce     int64
	From      string
	To        *string
	Value     string
	GasPrice  int64
	GasLimit  int64
	Input     *string
	CreatedAt *int64
}

type verifyContractResult struct {
	Success bool
	Address *string
	Error   *string
}

type verifyProxyResult struct {
	Success bool
	Address *string
	Error   *string
}

// ---------------------------------------------------------------------------
// GraphQL argument helpers
// ---------------------------------------------------------------------------

func limitArg() *graphql.ArgumentConfig {
	return &graphql.ArgumentConfig{Type: graphql.Int, Description: "maximum rows to return (default 25, max 100)"}
}

func offsetArg() *graphql.ArgumentConfig {
	return &graphql.ArgumentConfig{Type: graphql.Int, Description: "rows to skip"}
}

func strArgCfg(desc string) *graphql.ArgumentConfig {
	return &graphql.ArgumentConfig{Type: graphql.String, Description: desc}
}

func intArgCfg(desc string) *graphql.ArgumentConfig {
	return &graphql.ArgumentConfig{Type: graphql.Int, Description: desc}
}

func boolArgCfg(desc string) *graphql.ArgumentConfig {
	return &graphql.ArgumentConfig{Type: graphql.Boolean, Description: desc}
}

// ---------------------------------------------------------------------------
// GraphQL type system
// ---------------------------------------------------------------------------

var intervalEnum = graphql.NewEnum(graphql.EnumConfig{
	Name: "Interval",
	Values: graphql.EnumValueConfigMap{
		"MINUTE": &graphql.EnumValueConfig{Value: "minute"},
		"HOUR":   &graphql.EnumValueConfig{Value: "hour"},
		"DAY":    &graphql.EnumValueConfig{Value: "day"},
		"WEEK":   &graphql.EnumValueConfig{Value: "week"},
	},
})

var validatorOrderEnum = graphql.NewEnum(graphql.EnumConfig{
	Name: "ValidatorOrder",
	Values: graphql.EnumValueConfigMap{
		"DELEGATED": &graphql.EnumValueConfig{Value: "delegated"},
		"BLOCKS":    &graphql.EnumValueConfig{Value: "blocks"},
		"REWARD":    &graphql.EnumValueConfig{Value: "reward"},
		"STAKED":    &graphql.EnumValueConfig{Value: "staked"},
	},
})

var sourceInputType = graphql.NewInputObject(graphql.InputObjectConfig{
	Name: "SourceInput",
	Fields: graphql.InputObjectConfigFieldMap{
		"fileName": &graphql.InputObjectFieldConfig{Type: graphql.NewNonNull(graphql.String)},
		"content":  &graphql.InputObjectFieldConfig{Type: graphql.NewNonNull(graphql.String)},
	},
})

var libraryLinkInputType = graphql.NewInputObject(graphql.InputObjectConfig{
	Name: "LibraryLinkInput",
	Fields: graphql.InputObjectConfigFieldMap{
		"name":    &graphql.InputObjectFieldConfig{Type: graphql.NewNonNull(graphql.String)},
		"address": &graphql.InputObjectFieldConfig{Type: graphql.NewNonNull(graphql.String)},
	},
})

var verifyContractInputType = graphql.NewInputObject(graphql.InputObjectConfig{
	Name: "VerifyContractInput",
	Fields: graphql.InputObjectConfigFieldMap{
		"address":         &graphql.InputObjectFieldConfig{Type: graphql.NewNonNull(graphql.String)},
		"contractName":    &graphql.InputObjectFieldConfig{Type: graphql.NewNonNull(graphql.String)},
		"compilerVersion": &graphql.InputObjectFieldConfig{Type: graphql.NewNonNull(graphql.String)},
		"optimization":    &graphql.InputObjectFieldConfig{Type: graphql.Boolean},
		"optimizerRuns":   &graphql.InputObjectFieldConfig{Type: graphql.Int},
		"evmVersion":      &graphql.InputObjectFieldConfig{Type: graphql.String},
		"license":         &graphql.InputObjectFieldConfig{Type: graphql.String},
		"sources":         &graphql.InputObjectFieldConfig{Type: graphql.NewList(graphql.NewNonNull(sourceInputType))},
		"libraryLinks":    &graphql.InputObjectFieldConfig{Type: graphql.NewList(libraryLinkInputType)},
		"constructorArgs": &graphql.InputObjectFieldConfig{Type: graphql.String},
	},
})

var verifyProxyInputType = graphql.NewInputObject(graphql.InputObjectConfig{
	Name: "VerifyProxyInput",
	Fields: graphql.InputObjectConfigFieldMap{
		"proxyAddress":    &graphql.InputObjectFieldConfig{Type: graphql.NewNonNull(graphql.String)},
		"implementation":  &graphql.InputObjectFieldConfig{Type: graphql.NewNonNull(graphql.String)},
		"compilerVersion": &graphql.InputObjectFieldConfig{Type: graphql.NewNonNull(graphql.String)},
		"proxyType":       &graphql.InputObjectFieldConfig{Type: graphql.NewNonNull(graphql.String)},
	},
})

var verifyContractResultType = graphql.NewObject(graphql.ObjectConfig{
	Name: "VerifyContractResult",
	Fields: graphql.Fields{
		"success": &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		"address": &graphql.Field{Type: graphql.String},
		"error":   &graphql.Field{Type: graphql.String},
	},
})

var verifyProxyResultType = graphql.NewObject(graphql.ObjectConfig{
	Name: "VerifyProxyResult",
	Fields: graphql.Fields{
		"success": &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		"address": &graphql.Field{Type: graphql.String},
		"error":   &graphql.Field{Type: graphql.String},
	},
})

var logType = graphql.NewObject(graphql.ObjectConfig{
	Name: "Log",
	Fields: graphql.Fields{
		"address":          &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"topics":           &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(graphql.String)))},
		"data":             &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"logIndex":         &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"transactionIndex": &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"blockNumber":      &graphql.Field{Type: graphql.NewNonNull(BigInt)},
	},
})

var receiptType = graphql.NewObject(graphql.ObjectConfig{
	Name: "Receipt",
	Fields: graphql.Fields{
		"transactionHash":   &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"blockNumber":       &graphql.Field{Type: BigInt},
		"blockHash":         &graphql.Field{Type: graphql.String},
		"contractAddress":   &graphql.Field{Type: graphql.String},
		"cumulativeGasUsed": &graphql.Field{Type: graphql.String},
		"gasUsed":           &graphql.Field{Type: BigInt},
		"effectiveGasPrice": &graphql.Field{Type: graphql.String},
		"status":            &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		"logs":              &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(logType)))},
	},
})

var transactionType = graphql.NewObject(graphql.ObjectConfig{
	Name: "Transaction",
	Fields: graphql.Fields{
		"hash":                 &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"nonce":                &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"blockNumber":          &graphql.Field{Type: BigInt},
		"blockHash":            &graphql.Field{Type: graphql.String},
		"transactionIndex":     &graphql.Field{Type: BigInt},
		"from":                 &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"to":                   &graphql.Field{Type: graphql.String},
		"value":                &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"gasPrice":             &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"gasLimit":             &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"gasUsed":              &graphql.Field{Type: BigInt},
		"maxFeePerGas":         &graphql.Field{Type: graphql.String},
		"maxPriorityFeePerGas": &graphql.Field{Type: graphql.String},
		"input":                &graphql.Field{Type: graphql.String},
		"v":                    &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"r":                    &graphql.Field{Type: graphql.String},
		"s":                    &graphql.Field{Type: graphql.String},
		"chainId":              &graphql.Field{Type: BigInt},
		"transactionType":      &graphql.Field{Type: graphql.String},
		"status":               &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"cumulativeGasUsed":    &graphql.Field{Type: graphql.String},
		"effectiveGasPrice":    &graphql.Field{Type: graphql.String},
		"contractAddress":      &graphql.Field{Type: graphql.String},
		"blockTimestamp":       &graphql.Field{Type: BigInt},
		"receipt": &graphql.Field{
			Type: receiptType,
			Resolve: func(p graphql.ResolveParams) (interface{}, error) {
				row, ok := p.Source.(*txRow)
				if !ok || row == nil {
					return nil, nil
				}
				return resolveReceipt(p.Context, row.Hash)
			},
		},
	},
})

var uncleType = graphql.NewObject(graphql.ObjectConfig{
	Name: "Uncle",
	Fields: graphql.Fields{
		"number":     &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"hash":       &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"parentHash": &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"miner":      &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"difficulty": &graphql.Field{Type: graphql.String},
		"gasLimit":   &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"gasUsed":    &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"timestamp":  &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"reward":     &graphql.Field{Type: graphql.String},
	},
})

var blockType = graphql.NewObject(graphql.ObjectConfig{
	Name: "Block",
	Fields: graphql.Fields{
		"number":                &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"hash":                  &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"parentHash":            &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"nonce":                 &graphql.Field{Type: graphql.String},
		"sha3Uncles":            &graphql.Field{Type: graphql.String},
		"logsBloom":             &graphql.Field{Type: graphql.String},
		"transactionsRoot":      &graphql.Field{Type: graphql.String},
		"stateRoot":             &graphql.Field{Type: graphql.String},
		"receiptsRoot":          &graphql.Field{Type: graphql.String},
		"miner":                 &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"difficulty":            &graphql.Field{Type: graphql.String},
		"totalDifficulty":       &graphql.Field{Type: graphql.String},
		"gasLimit":              &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"gasUsed":               &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"timestamp":             &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"size":                  &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"extraData":             &graphql.Field{Type: graphql.String},
		"baseFeePerGas":         &graphql.Field{Type: graphql.String},
		"blobGasUsed":           &graphql.Field{Type: BigInt},
		"excessBlobGas":         &graphql.Field{Type: BigInt},
		"parentBeaconBlockRoot": &graphql.Field{Type: graphql.String},
		"txCount":               &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"uncleCount":            &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"reward":                &graphql.Field{Type: graphql.String},
		"isUncle":               &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		"transactionCount":      &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"transactions": &graphql.Field{
			Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(transactionType))),
			Resolve: func(p graphql.ResolveParams) (interface{}, error) {
				row, ok := p.Source.(*blockRow)
				if !ok || row == nil {
					return []*txRow{}, nil
				}
				return resolveBlockTransactions(p.Context, row.Number)
			},
		},
		"uncles": &graphql.Field{
			Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(uncleType))),
			Resolve: func(p graphql.ResolveParams) (interface{}, error) {
				row, ok := p.Source.(*blockRow)
				if !ok || row == nil {
					return []interface{}{}, nil
				}
				return resolveBlockUncles(p.Context, row.Number)
			},
		},
	},
})

var tokenType = graphql.NewObject(graphql.ObjectConfig{
	Name: "Token",
	Fields: graphql.Fields{
		"address":           &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"name":              &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"symbol":            &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"decimals":          &graphql.Field{Type: graphql.NewNonNull(graphql.Int)},
		"totalSupply":       &graphql.Field{Type: graphql.String},
		"holders":           &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"transfersCount":    &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"circulatingSupply": &graphql.Field{Type: graphql.String},
		"priceUSD":          &graphql.Field{Type: graphql.String},
		"priceChange24h":    &graphql.Field{Type: graphql.String},
		"marketCap":         &graphql.Field{Type: graphql.String},
		"volume24h":         &graphql.Field{Type: graphql.String},
		"isVerified":        &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		"isSpam":            &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		"spamScore":         &graphql.Field{Type: graphql.NewNonNull(graphql.Int)},
		"contractAddress":   &graphql.Field{Type: graphql.String},
		"logoURL":           &graphql.Field{Type: graphql.String},
		"description":       &graphql.Field{Type: graphql.String},
		"website":           &graphql.Field{Type: graphql.String},
	},
})

var tokenHolderType = graphql.NewObject(graphql.ObjectConfig{
	Name: "TokenHolder",
	Fields: graphql.Fields{
		"address":         &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"balance":         &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"balanceUSD":      &graphql.Field{Type: graphql.String},
		"percentHoldings": &graphql.Field{Type: graphql.String},
		"isContract":      &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		"updatedBlock":    &graphql.Field{Type: BigInt},
	},
})

var tokenTransferType = graphql.NewObject(graphql.ObjectConfig{
	Name: "TokenTransfer",
	Fields: graphql.Fields{
		"tokenAddress":    &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"from":            &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"to":              &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"value":           &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"transactionHash": &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"blockNumber":     &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"logIndex":        &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"timestamp":       &graphql.Field{Type: BigInt},
	},
})

var tokenApprovalType = graphql.NewObject(graphql.ObjectConfig{
	Name: "TokenApproval",
	Fields: graphql.Fields{
		"tokenAddress":    &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"owner":           &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"spender":         &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"value":           &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"transactionHash": &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"blockNumber":     &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"logIndex":        &graphql.Field{Type: graphql.NewNonNull(BigInt)},
	},
})

var pricePointType = graphql.NewObject(graphql.ObjectConfig{
	Name: "PricePoint",
	Fields: graphql.Fields{
		"timestamp": &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"price":     &graphql.Field{Type: graphql.String},
		"maxPrice":  &graphql.Field{Type: graphql.String},
	},
})

var nftCollectionType = graphql.NewObject(graphql.ObjectConfig{
	Name: "NFTCollection",
	Fields: graphql.Fields{
		"address":        &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"name":           &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"symbol":         &graphql.Field{Type: graphql.String},
		"contractType":   &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"totalSupply":    &graphql.Field{Type: BigInt},
		"holderCount":    &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"transfersCount": &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"floorPrice":     &graphql.Field{Type: graphql.String},
		"volume24h":      &graphql.Field{Type: graphql.String},
		"marketCap":      &graphql.Field{Type: graphql.String},
		"description":    &graphql.Field{Type: graphql.String},
		"imageURL":       &graphql.Field{Type: graphql.String},
		"externalURL":    &graphql.Field{Type: graphql.String},
		"bannerURL":      &graphql.Field{Type: graphql.String},
		"verified":       &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
	},
})

var nftAttributeType = graphql.NewObject(graphql.ObjectConfig{
	Name: "NFTAttribute",
	Fields: graphql.Fields{
		"traitType":   &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"value":       &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"displayType": &graphql.Field{Type: graphql.String},
		"rarityScore": &graphql.Field{Type: graphql.String},
	},
})

var nftMetadataType = graphql.NewObject(graphql.ObjectConfig{
	Name: "NFTMetadata",
	Fields: graphql.Fields{
		"name":        &graphql.Field{Type: graphql.String},
		"description": &graphql.Field{Type: graphql.String},
		"image": &graphql.Field{
			Type: graphql.String,
			Resolve: func(p graphql.ResolveParams) (interface{}, error) {
				row, ok := p.Source.(*nftRow)
				if !ok || row == nil {
					return nil, nil
				}
				return nullStringPtr(row.ImageURL), nil
			},
		},
		"externalUrl": &graphql.Field{Type: graphql.String},
		"attributes":  &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(nftAttributeType)))},
	},
})

var nftType = graphql.NewObject(graphql.ObjectConfig{
	Name: "NFT",
	Fields: graphql.Fields{
		"tokenAddress": &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"tokenId":      &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"owner":        &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"uri":          &graphql.Field{Type: graphql.String},
		"name":         &graphql.Field{Type: graphql.String},
		"description":  &graphql.Field{Type: graphql.String},
		"imageURL":     &graphql.Field{Type: graphql.String},
		"externalURL":  &graphql.Field{Type: graphql.String},
		"isBurned":     &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		"metadata": &graphql.Field{
			Type: nftMetadataType,
			Resolve: func(p graphql.ResolveParams) (interface{}, error) {
				row, ok := p.Source.(*nftRow)
				if !ok || row == nil {
					return nil, nil
				}
				return row, nil
			},
		},
		"rarity": &graphql.Field{
			Type: nftRarityType,
			Resolve: func(p graphql.ResolveParams) (interface{}, error) {
				row, ok := p.Source.(*nftRow)
				if !ok || row == nil {
					return nil, nil
				}
				return resolveNFTRarity(p.Context, row.TokenAddress, row.TokenID)
			},
		},
	},
})

var nftOwnerType = graphql.NewObject(graphql.ObjectConfig{
	Name: "NFTOwner",
	Fields: graphql.Fields{
		"tokenAddress": &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"tokenId":      &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"owner":        &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"updatedBlock": &graphql.Field{Type: graphql.NewNonNull(BigInt)},
	},
})

var nftTransferType = graphql.NewObject(graphql.ObjectConfig{
	Name: "NFTTransfer",
	Fields: graphql.Fields{
		"tokenAddress":    &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"from":            &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"to":              &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"tokenId":         &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"value":           &graphql.Field{Type: graphql.String},
		"transactionHash": &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"blockNumber":     &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"logIndex":        &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"timestamp":       &graphql.Field{Type: BigInt},
	},
})

var traitRarityType = graphql.NewObject(graphql.ObjectConfig{
	Name: "TraitRarity",
	Fields: graphql.Fields{
		"traitType":  &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"value":      &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"occurrence": &graphql.Field{Type: graphql.String},
	},
})

var nftRarityType = graphql.NewObject(graphql.ObjectConfig{
	Name: "NFTRarity",
	Fields: graphql.Fields{
		"collectionAddress": &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"tokenId":           &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"rarityScore":       &graphql.Field{Type: graphql.String},
		"rarityRank":        &graphql.Field{Type: BigInt},
		"traits":            &graphql.Field{Type: graphql.NewNonNull(graphql.NewList(graphql.NewNonNull(traitRarityType)))},
	},
})

var nftActivityType = graphql.NewObject(graphql.ObjectConfig{
	Name: "NFTActivity",
	Fields: graphql.Fields{
		"eventType":       &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"tokenAddress":    &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"tokenId":         &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"from":            &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"to":              &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"transactionHash": &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"blockNumber":     &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"logIndex":        &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"timestamp":       &graphql.Field{Type: BigInt},
	},
})

var accountTokenType = graphql.NewObject(graphql.ObjectConfig{
	Name: "AccountToken",
	Fields: graphql.Fields{
		"token":      &graphql.Field{Type: graphql.NewNonNull(tokenType)},
		"balance":    &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"balanceUSD": &graphql.Field{Type: graphql.String},
	},
})

var accountNFTType = graphql.NewObject(graphql.ObjectConfig{
	Name: "AccountNFT",
	Fields: graphql.Fields{
		"collection": &graphql.Field{Type: graphql.NewNonNull(nftCollectionType)},
		"tokenId":    &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"owner":      &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"balance":    &graphql.Field{Type: graphql.NewNonNull(graphql.Int)},
	},
})

var accountType = graphql.NewObject(graphql.ObjectConfig{
	Name: "Account",
	Fields: graphql.Fields{
		"address":     &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"balance":     &graphql.Field{Type: graphql.String},
		"nonce":       &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"codeHash":    &graphql.Field{Type: graphql.String},
		"isContract":  &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		"isVerified":  &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		"tokenCount":  &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"nftCount":    &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"nftBalances": &graphql.Field{Type: graphql.NewNonNull(graphql.Int)},
	},
})

var contractSourceType = graphql.NewObject(graphql.ObjectConfig{
	Name: "ContractSource",
	Fields: graphql.Fields{
		"fileName":        &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"sourceCode":      &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"language":        &graphql.Field{Type: graphql.String},
		"compilerVersion": &graphql.Field{Type: graphql.String},
	},
})

var contractType = graphql.NewObject(graphql.ObjectConfig{
	Name: "Contract",
	Fields: graphql.Fields{
		"address":             &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"contractName":        &graphql.Field{Type: graphql.String},
		"compiler":            &graphql.Field{Type: graphql.String},
		"compilerVersion":     &graphql.Field{Type: graphql.String},
		"optimizationEnabled": &graphql.Field{Type: graphql.Boolean},
		"optimizationRuns":    &graphql.Field{Type: graphql.Int},
		"evmVersion":          &graphql.Field{Type: graphql.String},
		"licenseType":         &graphql.Field{Type: graphql.String},
		"sourceCode":          &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"abi":                 &graphql.Field{Type: graphql.String},
		"bytecode":            &graphql.Field{Type: graphql.String},
		"runtimeBytecode":     &graphql.Field{Type: graphql.String},
		"contractType":        &graphql.Field{Type: graphql.String},
		"isVerified":          &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		"verificationStatus":  &graphql.Field{Type: graphql.String},
		"verifiedAt":          &graphql.Field{Type: BigInt},
	},
})

var verifiedContractType = graphql.NewObject(graphql.ObjectConfig{
	Name: "VerifiedContract",
	Fields: graphql.Fields{
		"address":          &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"contractName":     &graphql.Field{Type: graphql.String},
		"compilerVersion":  &graphql.Field{Type: graphql.String},
		"optimization":     &graphql.Field{Type: graphql.Boolean},
		"optimizationRuns": &graphql.Field{Type: graphql.Int},
		"evmVersion":       &graphql.Field{Type: graphql.String},
		"license":          &graphql.Field{Type: graphql.String},
		"verifiedAt":       &graphql.Field{Type: BigInt},
	},
})

var validatorType = graphql.NewObject(graphql.ObjectConfig{
	Name: "Validator",
	Fields: graphql.Fields{
		"address":        &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"name":           &graphql.Field{Type: graphql.String},
		"moniker":        &graphql.Field{Type: graphql.String},
		"uptime":         &graphql.Field{Type: graphql.String},
		"commissionRate": &graphql.Field{Type: graphql.String},
		"totalStake":     &graphql.Field{Type: graphql.String},
		"tokens":         &graphql.Field{Type: graphql.String},
		"blocksCount":    &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"isJailed":       &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		"isActive":       &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
		"status":         &graphql.Field{Type: graphql.String},
	},
})

var validatorRewardType = graphql.NewObject(graphql.ObjectConfig{
	Name: "ValidatorReward",
	Fields: graphql.Fields{
		"blockNumber":     &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"blockHash":       &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"validator":       &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"amount":          &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"transactionHash": &graphql.Field{Type: graphql.String},
		"timestamp":       &graphql.Field{Type: BigInt},
	},
})

var networkStatsType = graphql.NewObject(graphql.ObjectConfig{
	Name: "NetworkStats",
	Fields: graphql.Fields{
		"totalBlocks":       &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"totalTransactions": &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"totalTokens":       &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"totalAccounts":     &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"totalContracts":    &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"tps":               &graphql.Field{Type: graphql.NewNonNull(graphql.Float)},
		"avgBlockTime":      &graphql.Field{Type: graphql.NewNonNull(graphql.Float)},
		"latestBaseFee":     &graphql.Field{Type: graphql.String},
	},
})

var gasPriceType = graphql.NewObject(graphql.ObjectConfig{
	Name: "GasPrice",
	Fields: graphql.Fields{
		"low":      &graphql.Field{Type: graphql.String},
		"standard": &graphql.Field{Type: graphql.String},
		"fast":     &graphql.Field{Type: graphql.String},
		"baseFee":  &graphql.Field{Type: graphql.String},
	},
})

var gasPricePointType = graphql.NewObject(graphql.ObjectConfig{
	Name: "GasPricePoint",
	Fields: graphql.Fields{
		"timestamp":         &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"avgGasPrice":       &graphql.Field{Type: graphql.String},
		"minGasPrice":       &graphql.Field{Type: graphql.String},
		"maxGasPrice":       &graphql.Field{Type: graphql.String},
		"transactionsCount": &graphql.Field{Type: graphql.NewNonNull(BigInt)},
	},
})

var pendingTransactionType = graphql.NewObject(graphql.ObjectConfig{
	Name: "PendingTransaction",
	Fields: graphql.Fields{
		"hash":      &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"nonce":     &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"from":      &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"to":        &graphql.Field{Type: graphql.String},
		"value":     &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"gasPrice":  &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"gasLimit":  &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"input":     &graphql.Field{Type: graphql.String},
		"createdAt": &graphql.Field{Type: BigInt},
	},
})

// internalTxType is declared after internalTxRow so that its to/value fields can
// be non-null in the common case; rows with a NULL target return nil and the
// non-null fields resolve to an error, which is intentional (no fabricated data).
var internalTxType = graphql.NewObject(graphql.ObjectConfig{
	Name: "InternalTransaction",
	Fields: graphql.Fields{
		"transactionHash": &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"blockNumber":     &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"traceIndex":      &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"subtraceIndex":   &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"callType":        &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"from":            &graphql.Field{Type: graphql.NewNonNull(graphql.String)},
		"to":              &graphql.Field{Type: graphql.String},
		"value":           &graphql.Field{Type: graphql.String},
		"gas":             &graphql.Field{Type: graphql.String},
		"gasUsed":         &graphql.Field{Type: graphql.String},
		"input":           &graphql.Field{Type: graphql.String},
		"output":          &graphql.Field{Type: graphql.String},
		"error":           &graphql.Field{Type: graphql.String},
		"depth":           &graphql.Field{Type: graphql.NewNonNull(BigInt)},
		"creates":         &graphql.Field{Type: graphql.String},
		"success":         &graphql.Field{Type: graphql.NewNonNull(graphql.Boolean)},
	},
})

var searchResultType = graphql.NewUnion(graphql.UnionConfig{
	Name: "SearchResult",
	Types: []*graphql.Object{
		blockType,
		transactionType,
		tokenType,
		accountType,
	},
	ResolveType: func(p graphql.ResolveTypeParams) *graphql.Object {
		switch p.Value.(type) {
		case *blockRow:
			return blockType
		case *txRow:
			return transactionType
		case *tokenRow:
			return tokenType
		case *accountRow:
			return accountType
		}
		return nil
	},
})
