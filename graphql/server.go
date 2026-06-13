// Package graphql provides GraphQL API for TigerScan
// Built with Go for high performance
package graphql

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
	"github.com/99designs/gqlgen/graphql"
	"github.com/99designs/gqlgen/graphql/handler"
	"github.com/99designs/gqlgen/graphql/handler/extension"
	"github.com/99designs/gqlgen/graphql/handler/lru"
	"github.com/99designs/gqlgen/graphql/handler/transport"
)

// Config holds GraphQL server configuration
type Config struct {
	Port           string
	DBURL          string
	RedisURL       string
	MaxComplexity int
	CacheEnabled  bool
}

// Server represents the GraphQL server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
}

// NewServer creates a new GraphQL server
func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	
	// Database connection
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	
	// Redis connection
	rdb := redis.NewClient(&redis.Options{
		Addr:     cfg.RedisURL,
		Password: "",
		DB:       1, // Use separate DB for GraphQL cache
	})
	
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	
	return &Server{
		cfg:   cfg,
		pool:  pool,
		redis: rdb,
	}, nil
}

// Start starts the GraphQL server
func (s *Server) Start() error {
	// Create GraphQL executable schema
	exec := s.createExecutableSchema()
	
	// Create GraphQL handler
	h := handler.New(exec)
	
	// Use extensions
	h.Use(extension.AutomaticPersistedQueryCache{
		Cache: lru.New(100),
	})
	h.Use(extension.Introspection{})
	h.Use(extension.ComplexityLimit(s.cfg.MaxComplexity))
	
	// WebSocket transport
	h.AddTransport(transport.WebSocket{
		Upgrader: transport.Upgrader{
			CheckOrigin: func(r *http.Request) bool {
				return true // In production, implement proper origin checking
			},
		},
	})
	
	// HTTP transport
	h.AddTransport(transport.GET{})
	h.AddTransport(transport.POST{})
	h.AddTransport(transport.MultipartForm{})
	
	// GraphQL endpoint
	http.HandleFunc("/graphql", func(w http.ResponseWriter, r *http.Request) {
		h.ServeHTTP(w, r)
	})
	
	// GraphiQL endpoint (for development)
	http.HandleFunc("/graphiql", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/html")
		w.Write([]byte(`<!DOCTYPE html>
<html>
<head>
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/graphiql/graphiql.min.css"/>
<script src="https://cdn.jsdelivr.net/npm/graphiql/graphiql.min.js"></script>
</head>
<body style="margin: 0;">
<div id="graphiql" style="height: 100vh;"></div>
<script>
graphiql.configure({
  endpoint: '/graphql',
  subscriptionsEndpoint: 'ws://localhost:8080/ws',
});
</script>
</body>
</html>`))
	})
	
	addr := fmt.Sprintf(":%s", s.cfg.Port)
	return http.ListenAndServe(addr, nil)
}

// createExecutableSchema creates the GraphQL executable schema
func (s *Server) createExecutableSchema() graphql.ExecutableSchema {
	resolver := &Resolver{
		pool:  s.pool,
		redis: s.redis,
	}
	
	// Simple schema for now - in production use gqlgen generated code
	return &SimpleSchema{resolver: resolver}
}

// Resolver handles GraphQL resolver functions
type Resolver struct {
	pool  *pgxpool.Pool
	redis *redis.Client
}

// Query resolvers
func (r *Resolver) Blocks(ctx context.Context, args struct {
	First   int
	After   *string
}) ([]*Block, error) {
	query := `
		SELECT number, hash, parent_hash, timestamp, tx_count, gas_used, miner, size, gas_limit
		FROM blocks
		ORDER BY number DESC
		LIMIT $1
	`
	
	rows, err := r.pool.Query(ctx, query, args.First)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var blocks []*Block
	for rows.Next() {
		var b Block
		if err := rows.Scan(&b.Number, &b.Hash, &b.ParentHash, &b.Timestamp, 
			&b.TxCount, &b.GasUsed, &b.Miner, &b.Size, &b.GasLimit); err != nil {
			return nil, err
		}
		blocks = append(blocks, &b)
	}
	
	return blocks, nil
}

func (r *Resolver) Block(ctx context.Context, args struct {
	Number *int
	Hash   *string
}) (*Block, error) {
	var query string
	var arg interface{}
	
	if args.Number != nil {
		query = `SELECT number, hash, parent_hash, timestamp, tx_count, gas_used, miner, size, gas_limit 
			FROM blocks WHERE number = $1`
		arg = *args.Number
	} else {
		query = `SELECT number, hash, parent_hash, timestamp, tx_count, gas_used, miner, size, gas_limit 
			FROM blocks WHERE hash = $1`
		arg = *args.Hash
	}
	
	var b Block
	err := r.pool.QueryRow(ctx, query, arg).Scan(
		&b.Number, &b.Hash, &b.ParentHash, &b.Timestamp,
		&b.TxCount, &b.GasUsed, &b.Miner, &b.Size, &b.GasLimit,
	)
	if err != nil {
		return nil, err
	}
	
	return &b, nil
}

func (r *Resolver) Transactions(ctx context.Context, args struct {
	First int
	From  *string
	To    *string
}) ([]*Transaction, error) {
	query := `
		SELECT hash, block_number, from_address, to_address, value, gas_price, gas_used, status, timestamp
		FROM transactions
		WHERE 1=1
	`
	
	if args.From != nil {
		query += fmt.Sprintf(" AND from_address = '%s'", *args.From)
	}
	if args.To != nil {
		query += fmt.Sprintf(" AND to_address = '%s'", *args.To)
	}
	
	query += fmt.Sprintf(" ORDER BY timestamp DESC LIMIT %d", args.First)
	
	rows, err := r.pool.Query(ctx, query)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var txs []*Transaction
	for rows.Next() {
		var tx Transaction
		if err := rows.Scan(&tx.Hash, &tx.BlockNumber, &tx.From, &tx.To, 
			&tx.Value, &tx.GasPrice, &tx.GasUsed, &tx.Status, &tx.Timestamp); err != nil {
			return nil, err
		}
		txs = append(txs, &tx)
	}
	
	return txs, nil
}

func (r *Resolver) Transaction(ctx context.Context, args struct {
	Hash string
}) (*Transaction, error) {
	var tx Transaction
	err := r.pool.QueryRow(ctx, `
		SELECT hash, block_number, from_address, to_address, value, gas_price, gas_used, status, timestamp, input_data
		FROM transactions WHERE hash = $1`,
		args.Hash,
	).Scan(
		&tx.Hash, &tx.BlockNumber, &tx.From, &tx.To,
		&tx.Value, &tx.GasPrice, &tx.GasUsed, &tx.Status, &tx.Timestamp, &tx.Input,
	)
	if err != nil {
		return nil, err
	}
	
	return &tx, nil
}

func (r *Resolver) Tokens(ctx context.Context, args struct {
	First int
	Search *string
}) ([]*Token, error) {
	query := `
		SELECT address, name, symbol, decimals, total_supply, holders_count, transfers_count, price
		FROM tokens
		WHERE is_active = true
	`
	
	if args.Search != nil {
		query += fmt.Sprintf(" AND (name ILIKE '%%%s%%' OR symbol ILIKE '%%%s%%')", *args.Search, *args.Search)
	}
	
	query += fmt.Sprintf(" ORDER BY transfers_count DESC LIMIT %d", args.First)
	
	rows, err := r.pool.Query(ctx, query)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var tokens []*Token
	for rows.Next() {
		var t Token
		if err := rows.Scan(&t.Address, &t.Name, &t.Symbol, &t.Decimals, 
			&t.TotalSupply, &t.HoldersCount, &t.TransfersCount, &t.Price); err != nil {
			return nil, err
		}
		tokens = append(tokens, &t)
	}
	
	return tokens, nil
}

func (r *Resolver) Token(ctx context.Context, args struct {
	Address string
}) (*Token, error) {
	var t Token
	err := r.pool.QueryRow(ctx, `
		SELECT address, name, symbol, decimals, total_supply, holders_count, transfers_count, price, market_cap, volume_24h
		FROM tokens WHERE address = $1`,
		args.Address,
	).Scan(
		&t.Address, &t.Name, &t.Symbol, &t.Decimals,
		&t.TotalSupply, &t.HoldersCount, &t.TransfersCount, &t.Price,
		&t.MarketCap, &t.Volume24h,
	)
	if err != nil {
		return nil, err
	}
	
	return &t, nil
}

func (r *Resolver) NFTs(ctx context.Context, args struct {
	First      int
	Collection *string
}) ([]*NFT, error) {
	query := `
		SELECT address, token_id, name, owner, collection_address, image_url, attributes
		FROM nfts
		WHERE is_active = true
	`
	
	if args.Collection != nil {
		query += fmt.Sprintf(" AND collection_address = '%s'", *args.Collection)
	}
	
	query += fmt.Sprintf(" ORDER BY token_id DESC LIMIT %d", args.First)
	
	rows, err := r.pool.Query(ctx, query)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var nfts []*NFT
	for rows.Next() {
		var n NFT
		var attrs json.RawMessage
		if err := rows.Scan(&n.Address, &n.TokenID, &n.Name, &n.Owner, 
			&n.CollectionAddress, &n.ImageURL, &attrs); err != nil {
			return nil, err
		}
		json.Unmarshal(attrs, &n.Attributes)
		nfts = append(nfts, &n)
	}
	
	return nfts, nil
}

func (r *Resolver) Accounts(ctx context.Context, args struct {
	Address string
}) (*Account, error) {
	var acc Account
	err := r.pool.QueryRow(ctx, `
		SELECT address, balance, nonce, code_hash, is_contract, is_verified
		FROM accounts WHERE address = $1`,
		args.Address,
	).Scan(
		&acc.Address, &acc.Balance, &acc.Nonce, &acc.CodeHash, 
		&acc.IsContract, &acc.IsVerified,
	)
	if err != nil {
		return nil, err
	}
	
	// Get token holdings
	tokenRows, err := r.pool.Query(ctx, `
		SELECT th.token_address, t.name, t.symbol, th.balance
		FROM token_holders th
		JOIN tokens t ON t.address = th.token_address
		WHERE th.address = $1
		ORDER BY th.balance DESC
		LIMIT 50`,
		args.Address,
	)
	if err == nil {
		defer tokenRows.Close()
		for tokenRows.Next() {
			var h TokenHolder
			tokenRows.Scan(&h.TokenAddress, &h.TokenName, &h.TokenSymbol, &h.Balance)
			acc.TokenHoldings = append(acc.TokenHoldings, h)
		}
	}
	
	return &acc, nil
}

func (r *Resolver) Validators(ctx context.Context, args struct {
	First int
	Active  *bool
}) ([]*Validator, error) {
	query := `SELECT address, name, commission_rate, total_stake, blocks_proposed, uptime, is_active
		FROM validators WHERE 1=1`
	
	if args.Active != nil {
		query += fmt.Sprintf(" AND is_active = %v", *args.Active)
	}
	
	query += fmt.Sprintf(" ORDER BY total_stake DESC LIMIT %d", args.First)
	
	rows, err := r.pool.Query(ctx, query)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var validators []*Validator
	for rows.Next() {
		var v Validator
		if err := rows.Scan(&v.Address, &v.Name, &v.CommissionRate, 
			&v.TotalStake, &v.BlocksProposed, &v.Uptime, &v.IsActive); err != nil {
			return nil, err
		}
		validators = append(validators, &v)
	}
	
	return validators, nil
}

func (r *Resolver) Search(ctx context.Context, args struct {
	Query string
}) (*SearchResult, error) {
	result := &SearchResult{}
	
	// Search blocks
	err := r.pool.QueryRow(ctx, `SELECT COUNT(*) FROM blocks WHERE hash = $1`, args.Query).Scan(&result.BlocksCount)
	if err == nil && result.BlocksCount > 0 {
		result.Blocks = append(result.Blocks, args.Query)
	}
	
	// Search transactions
	err = r.pool.QueryRow(ctx, `SELECT COUNT(*) FROM transactions WHERE hash = $1`, args.Query).Scan(&result.TransactionsCount)
	if err == nil && result.TransactionsCount > 0 {
		result.Transactions = append(result.Transactions, args.Query)
	}
	
	// Search tokens
	err = r.pool.QueryRow(ctx, `SELECT COUNT(*) FROM tokens WHERE address = $1`, args.Query).Scan(&result.TokensCount)
	if err == nil && result.TokensCount > 0 {
		result.Tokens = append(result.Tokens, args.Query)
	}
	
	// Search accounts
	err = r.pool.QueryRow(ctx, `SELECT COUNT(*) FROM accounts WHERE address = $1`, args.Query).Scan(&result.AccountsCount)
	if err == nil && result.AccountsCount > 0 {
		result.Accounts = append(result.Accounts, args.Query)
	}
	
	return result, nil
}

func (r *Resolver) ChainStats(ctx context.Context) (*ChainStats, error) {
	var stats ChainStats
	err := r.pool.QueryRow(ctx, `
		SELECT 
			(SELECT COUNT(*) FROM blocks) as total_blocks,
			(SELECT COUNT(*) FROM transactions) as total_transactions,
			(SELECT COUNT(*) FROM accounts) as total_addresses,
			(SELECT MAX(number) FROM blocks) as latest_block,
			(SELECT AVG(gas_price) FROM transactions WHERE timestamp > $1) as avg_gas_price
		FROM blocks LIMIT 1`,
		time.Now().Unix()-86400,
	).Scan(
		&stats.TotalBlocks, &stats.TotalTransactions, &stats.TotalAddresses,
		&stats.LatestBlock, &stats.AvgGasPrice,
	)
	if err != nil {
		return nil, err
	}
	
	// Get TPS (last hour)
	var txsLastHour int
	r.pool.QueryRow(ctx, `SELECT COUNT(*) FROM transactions WHERE timestamp > $1`, time.Now().Unix()-3600).Scan(&txsLastHour)
	stats.TPS = float64(txsLastHour) / 3600.0
	
	return &stats, nil
}

// SimpleSchema is a simple GraphQL schema implementation
type SimpleSchema struct {
	resolver *Resolver
}

func (s *SimpleSchema) Query(ctx context.Context, op *graphql.Operation) (graphql.ResponsePath, error) {
	// Simplified - in production use gqlgen generated code
	return nil, nil
}

// GraphQL Types
type Block struct {
	Number      int     `json:"number"`
	Hash        string  `json:"hash"`
	ParentHash  string  `json:"parentHash"`
	Timestamp   int     `json:"timestamp"`
	TxCount     int     `json:"txCount"`
	GasUsed     uint64  `json:"gasUsed"`
	Miner       string  `json:"miner"`
	Size       int     `json:"size"`
	GasLimit    uint64  `json:"gasLimit"`
}

type Transaction struct {
	Hash         string  `json:"hash"`
	BlockNumber *int    `json:"blockNumber"`
	From        string  `json:"from"`
	To          string  `json:"to"`
	Value       string  `json:"value"`
	GasPrice    uint64  `json:"gasPrice"`
	GasUsed     uint64  `json:"gasUsed"`
	Status      string  `json:"status"`
	Timestamp   int     `json:"timestamp"`
	Input       string  `json:"input"`
}

type Token struct {
	Address         string  `json:"address"`
	Name            string  `json:"name"`
	Symbol          string  `json:"symbol"`
	Decimals        int     `json:"decimals"`
	TotalSupply     string  `json:"totalSupply"`
	HoldersCount    int     `json:"holdersCount"`
	TransfersCount  int     `json:"transfersCount"`
	Price           string  `json:"price"`
	MarketCap      string  `json:"marketCap"`
	Volume24h      string  `json:"volume24h"`
}

type NFT struct {
	Address            string          `json:"address"`
	TokenID            string          `json:"tokenId"`
	Name              string          `json:"name"`
	Owner             string          `json:"owner"`
	CollectionAddress string          `json:"collectionAddress"`
	ImageURL          string          `json:"imageUrl"`
	Attributes        json.RawMessage `json:"attributes"`
}

type Account struct {
	Address         string        `json:"address"`
	Balance        string        `json:"balance"`
	Nonce          int           `json:"nonce"`
	CodeHash       string        `json:"codeHash"`
	IsContract     bool          `json:"isContract"`
	IsVerified    bool          `json:"isVerified"`
	TokenHoldings  []TokenHolder `json:"tokenHoldings"`
}

type TokenHolder struct {
	TokenAddress string `json:"tokenAddress"`
	TokenName   string `json:"tokenName"`
	TokenSymbol string `json:"tokenSymbol"`
	Balance    string `json:"balance"`
}

type Validator struct {
	Address         string  `json:"address"`
	Name           string  `json:"name"`
	CommissionRate float64 `json:"commissionRate"`
	TotalStake     string  `json:"totalStake"`
	BlocksProposed int    `json:"blocksProposed"`
	Uptime         float64 `json:"uptime"`
	IsActive       bool    `json:"isActive"`
}

type SearchResult struct {
	Blocks        []string `json:"blocks"`
	Transactions []string `json:"transactions"`
	Tokens       []string `json:"tokens"`
	Accounts     []string `json:"accounts"`
	BlocksCount       int `json:"blocksCount"`
	TransactionsCount int `json:"transactionsCount"`
	TokensCount       int `json:"tokensCount"`
	AccountsCount    int `json:"accountsCount"`
}

type ChainStats struct {
	TotalBlocks     int     `json:"totalBlocks"`
	TotalTransactions int   `json:"totalTransactions"`
	TotalAddresses int     `json:"totalAddresses"`
	LatestBlock    int     `json:"latestBlock"`
	AvgGasPrice    uint64  `json:"avgGasPrice"`
	TPS            float64 `json:"tps"`
}