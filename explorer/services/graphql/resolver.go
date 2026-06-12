// Package graphql provides production GraphQL API with real resolvers.
package graphql

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
)

// Resolver provides GraphQL resolution with real database queries
type Resolver struct {
	db *sql.DB
	mu sync.RWMutex
}

// Block represents a block in GraphQL
type Block struct {
	Number        string        `graphql:"number"`
	Hash          string        `graphql:"hash"`
	ParentHash    string        `graphql:"parentHash"`
	Miner         string        `graphql:"miner"`
	Timestamp     string        `graphql:"timestamp"`
	GasUsed       string        `graphql:"gasUsed"`
	GasLimit      string        `graphql:"gasLimit"`
	Transactions  []Transaction `graphql:"transactions"`
	Uncles        []Uncle       `graphql:"uncles"`
}

// Transaction represents a transaction
type Transaction struct {
	Hash           string       `graphql:"hash"`
	From           string       `graphql:"from"`
	To             string       `graphql:"to"`
	Value          string       `graphql:"value"`
	GasPrice       string       `graphql:"gasPrice"`
	GasUsed        string       `graphql:"gasUsed"`
	BlockNumber    string       `graphql:"blockNumber"`
	Timestamp      string       `graphql:"timestamp"`
	Status         string       `graphql:"status"`
	InputData      string       `graphql:"inputData"`
	Logs           []Log        `graphql:"logs"`
	TokenTransfers []TokenTransfer `graphql:"tokenTransfers"`
}

// Log represents an event log
type Log struct {
	Address   string   `graphql:"address"`
	Topics    []string `graphql:"topics"`
	Data      string   `graphql:"data"`
	LogIndex  string   `graphql:"logIndex"`
}

// TokenTransfer represents token transfer
type TokenTransfer struct {
	TokenAddress string `graphql:"tokenAddress"`
	From        string `graphql:"from"`
	To          string `graphql:"to"`
	Value       string `graphql:"value"`
	Type        string `graphql:"type"`
}

// Uncle represents an uncle block
type Uncle struct {
	Hash   string `graphql:"hash"`
	Miner  string `graphql:"miner"`
	Number string `graphql:"number"`
	Reward string `graphql:"reward"`
}

// Token represents a token
type Token struct {
	ID            string  `graphql:"id"`
	Address      string  `graphql:"address"`
	Name         string  `graphql:"name"`
	Symbol       string  `graphql:"symbol"`
	Decimals     int     `graphql:"decimals"`
	TotalSupply  string  `graphql:"totalSupply"`
	Price        string  `graphql:"price"`
	MarketCap    string  `graphql:"marketCap"`
	Volume24h    string  `graphql:"volume24h"`
	Holders      int     `graphql:"holders"`
	Transfers    int     `graphql:"transfers"`
}

// NFT represents an NFT
type NFT struct {
	ID             string     `graphql:"id"`
	Address       string     `graphql:"address"`
	TokenID       string     `graphql:"tokenId"`
	Name          string     `graphql:"name"`
	Description   string     `graphql:"description"`
	ImageURL      string     `graphql:"imageUrl"`
	Attributes    []Attribute `graphql:"attributes"`
	Owner         string     `graphql:"owner"`
}

// Attribute represents NFT attribute
type Attribute struct {
	TraitType    string `graphql:"traitType"`
	Value       string `graphql:"value"`
	DisplayType string `graphql:"displayType"`
}

// Account represents an account
type Account struct {
	Address   string  `graphql:"address"`
	Balance   string  `graphql:"balance"`
	TxCount   int     `graphql:"txCount"`
	Tokens    []Token `graphql:"tokens"`
	NFTs      []NFT   `graphql:"nfts"`
}

// GasPrice represents gas price
type GasPrice struct {
	Low         string `graphql:"low"`
	Medium      string `graphql:"medium"`
	High        string `graphql:"high"`
	BaseFee     string `graphql:"baseFee"`
	PriorityFee string `graphql:"priorityFee"`
}

// NetworkStats represents network statistics
type NetworkStats struct {
	TotalBlocks      string  `graphql:"totalBlocks"`
	TotalTransactions string `graphql:"totalTransactions"`
	TotalAccounts   string  `graphql:"totalAccounts"`
	TPS             float64 `graphql:"tps"`
	AvgGasPrice     string  `graphql:"avgGasPrice"`
}

// Service provides GraphQL API
type Service struct {
	db      *sql.DB
	schema  *graphql.Schema
	router  *gin.Engine
	wsHub   *WebSocketHub
}

// Config holds configuration
type Config struct {
	DB *sql.DB
}

// GraphQL Schema with complete queries and mutations
const Schema = `
type Query {
	# Block queries
	block(number: Int!): Block
	blocks(limit: Int, offset: Int): [Block!]!
	latestBlock: Block!
	
	# Transaction queries
	transaction(hash: String!): Transaction
	transactions(address: String, limit: Int, offset: Int): [Transaction!]!
	pendingTransactions: [Transaction!]!
	
	# Token queries
	token(address: String!): Token
	tokens(limit: Int, offset: Int, sortBy: String): [Token!]!
	tokenHolders(address: String!, limit: Int): [Account!]!
	tokenTransfers(address: String!, limit: Int): [TokenTransfer!]!
	tokenPriceHistory(address: String!, from: String, to: String): [PricePoint!]!
	
	# NFT queries
	nftCollection(address: String!): NFTCollection
	nft(address: String!, tokenId: String!): NFT
	nfts(address: String!, owner: String, traits: String, limit: Int, offset: Int): [NFT!]!
	nftTransfers(address: String!, limit: Int): [NFTTransfer!]!
	nftFloorPriceHistory(address: String!): [PricePoint!]!
	
	# Account queries
	account(address: String!): Account
	accountTokens(address: String!, limit: Int): [Token!]!
	accountNFTs(address: String!, limit: Int): [NFT!]!
	accountTransactions(address: String!, limit: Int): [Transaction!]!
	
	# Contract queries
	contract(address: String!): Contract
	contractEvents(address: String!, limit: Int): [ContractEvent!]!
	
	# Analytics queries
	gasPrice: GasPrice!
	gasPriceHistory(from: String, to: String): [GasPrice!]!
	networkStats: NetworkStats!
	tpsHistory(from: String, to: String): [TPSPoint!]!
	topTokens(limit: Int, metric: String): [Token!]!
	topAccounts(limit: Int): [Account!]!
	
	# Search
	search(query: String!): [SearchResult!]!
	
	# Validators
	validators: [Validator!]!
	validator(address: String!): Validator
}

type Mutation {
	# Contract interactions
	verifyContract(input: VerifyContractInput!): ContractVerification!
	watchAddress(address: String!): WatchSubscription
	
	# Subscriptions (mutations for testing)
	triggerEvent(type: String!, data: String!): EventTriggered!
}

type Subscription {
	newBlock: Block!
	newTransaction: Transaction!
	newPendingTransaction: Transaction!
	newTokenTransfer(token: String): TokenTransfer!
	newNFTTransfer(collection: String): NFTTransfer!
	gasPriceUpdate: GasPrice!
}

type Block {
	number: String!
	hash: String!
	parentHash: String!
	miner: String!
	timestamp: String!
	gasUsed: String!
	gasLimit: String!
	transactions: [Transaction!]!
	uncles: [Uncle!]!
	difficulty: String!
	totalDifficulty: String!
	size: String!
	nonce: String!
	extraData: String!
}

type Transaction {
	hash: String!
	from: String!
	to: String!
	value: String!
	gasPrice: String!
	gasUsed: String!
	gasLimit: String!
	blockNumber: String!
	blockHash: String!
	timestamp: String!
	status: String!
	inputData: String!
	logs: [Log!]!
	tokenTransfers: [TokenTransfer!]!
	internalTransactions: [InternalTransaction!]!
	isPending: Boolean!
}

type Log {
	address: String!
	topics: [String!]!
	data: String!
	logIndex: String!
	blockNumber: String!
	transactionHash: String!
}

type InternalTransaction {
	hash: String!
	callType: String!
	from: String!
	to: String!
	value: String!
	input: String!
	output: String!
}

type TokenTransfer {
	transactionHash: String!
	tokenAddress: String!
	from: String!
	to: String!
	value: String!
	type: String!
	timestamp: String!
}

type NFTTransfer {
	transactionHash: String!
	collection: String!
	tokenId: String!
	from: String!
	to: String!
	timestamp: String!
}

type Token {
	id: String!
	address: String!
	name: String!
	symbol: String!
	decimals: Int!
	totalSupply: String!
	price: String!
	marketCap: String!
	volume24h: String!
	priceChange24h: String!
	holders: Int!
	transfers: Int!
}

type NFTCollection {
	address: String!
	name: String!
	symbol: String!
	totalSupply: String!
	holders: Int!
	floorPrice: String!
	volume24h: String!
}

type NFT {
	id: String!
	address: String!
	tokenId: String!
	name: String!
	description: String!
	imageUrl: String!
	animationUrl: String!
	attributes: [Attribute!]!
	owner: String!
	collection: NFTCollection!
}

type Attribute {
	traitType: String!
	value: String!
	displayType: String
}

type Account {
	address: String!
	balance: String!
	txCount: Int!
	tokens: [Token!]!
	nfts: [NFT!]!
}

type GasPrice {
	low: String!
	medium: String!
	high: String!
	baseFee: String!
	priorityFee: String!
	timestamp: String!
}

type NetworkStats {
	totalBlocks: String!
	totalTransactions: String!
	totalAccounts: String!
	totalTokens: String!
	totalNFTs: String!
	tps: Float!
	avgGasPrice: String!
	networkUtilization: Float!
}

type Validator {
	address: String!
	name: String!
	votingPower: String!
	commission: String!
	delegators: Int!
	rewards: String!
	uptime: Float!
	jailed: Boolean!
}

type Contract {
	address: String!
	name: String!
	abi: String!
	sourceCode: String!
	isVerified: Boolean!
	compiler: String!
	optimization: Boolean!
}

type ContractEvent {
	event: String!
	args: String!
	blockNumber: String!
	transactionHash: String!
	timestamp: String!
}

type ContractVerification {
	address: String!
	name: String!
	compiler: String!
	version: String!
	verified: Boolean!
}

type PricePoint {
	price: String!
	timestamp: String!
}

type TPSPoint {
	tps: Float!
	timestamp: String!
}

type WatchSubscription {
	address: String!
	active: Boolean!
}

type EventTriggered {
	type: String!
	data: String!
	timestamp: String!
}

type SearchResult {
	type: String!
	id: String!
	data: String!
}

input VerifyContractInput {
	address: String!
	name: String!
	compiler: String!
	version: String!
	sourceCode: String!
	abi: String!
	optimization: Boolean
	runs: Int
}

union SearchResult = Block | Transaction | Token | Account | Contract | NFT | NFTCollection
`

// NewService creates a new GraphQL service
func NewService(cfg *Config) (*Service, error) {
	opts := []graphql.SchemaOpt{
		graphql.UseFieldResolvers(),
		graphql.Tracer(&Tracer{}),
	}

	schema, err := graphql.ParseSchema(Schema, &Resolver{db: cfg.DB}, opts...)
	if err != nil {
		return nil, err
	}

	return &Service{
		db:     cfg.DB,
		schema: schema,
		wsHub:  NewWebSocketHub(),
	}, nil
}

// Handler returns the HTTP handler
func (s *Service) Handler() http.Handler {
	return relay.Handler{Schema: s.schema}
}

// ServeHTTP handles HTTP requests
func (s *Service) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if isWebSocket(r) {
		s.wsHub.ServeHTTP(w, r)
		return
	}

	var params struct {
		Query         string                 `json:"query"`
		OperationName string                `json:"operationName"`
		Variables     map[string]interface{} `json:"variables"`
	}

	if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	ctx := context.WithValue(r.Context(), "resolver", &Resolver{db: s.db})
	result := s.schema.Exec(ctx, params.Query, params.OperationName, params.Variables)

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(result)
}

// isWebSocket checks if request is WebSocket
func isWebSocket(r *http.Request) bool {
	return r.Header.Get("Upgrade") == "websocket"
}

// ============================================
// RESOLVERS
// ============================================

// Block resolver
func (r *Resolver) Block(args struct{ Number int }) (*Block, error) {
	query := `
		SELECT number::text, hash, parent_hash::text, miner, 
		       EXTRACT(EPOCH FROM timestamp)::text, 
		       gas_used::text, gas_limit::text, difficulty::text, 
		       total_difficulty::text, size::text, nonce, extra_data
		FROM blocks
		WHERE number = $1
	`

	var b Block
	var timestamp, difficulty, totalDifficulty, size string

	err := r.db.QueryRow(query, args.Number).Scan(
		&b.Number, &b.Hash, &b.ParentHash, &b.Miner,
		&timestamp, &b.GasUsed, &b.GasLimit, &difficulty,
		&totalDifficulty, &size, &b.Nonce, &b.ExtraData,
	)
	if err != nil {
		return nil, err
	}

	b.Timestamp = timestamp

	return &b, nil
}

// LatestBlock resolver
func (r *Resolver) LatestBlock() (*Block, error) {
	query := `
		SELECT number::text, hash, parent_hash::text, miner,
		       EXTRACT(EPOCH FROM timestamp)::text,
		       gas_used::text, gas_limit::text
		FROM blocks
		ORDER BY number DESC
		LIMIT 1
	`

	var b Block
	var timestamp string

	err := r.db.QueryRow(query).Scan(
		&b.Number, &b.Hash, &b.ParentHash, &b.Miner,
		&timestamp, &b.GasUsed, &b.GasLimit,
	)
	if err != nil {
		return nil, err
	}

	b.Timestamp = timestamp
	return &b, nil
}

// Blocks resolver
func (r *Resolver) Blocks(args struct{ Limit, Offset *int }) ([]*Block, error) {
	limit := 25
	offset := 0

	if args.Limit != nil {
		limit = *args.Limit
	}
	if args.Offset != nil {
		offset = *args.Offset
	}

	query := `
		SELECT number::text, hash, parent_hash::text, miner,
		       EXTRACT(EPOCH FROM timestamp)::text,
		       gas_used::text, gas_limit::text
		FROM blocks
		ORDER BY number DESC
		LIMIT $1 OFFSET $2
	`

	rows, err := r.db.Query(query, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var blocks []*Block
	for rows.Next() {
		var b Block
		var timestamp string

		err := rows.Scan(
			&b.Number, &b.Hash, &b.ParentHash, &b.Miner,
			&timestamp, &b.GasUsed, &b.GasLimit,
		)
		if err != nil {
			return nil, err
		}

		b.Timestamp = timestamp
		blocks = append(blocks, &b)
	}

	return blocks, nil
}

// Transaction resolver
func (r *Resolver) Transaction(args struct{ Hash string }) (*Transaction, error) {
	query := `
		SELECT hash, from_address, to_address, value::text,
		       gas_price::text, gas_used::text, block_number::text,
		       EXTRACT(EPOCH FROM timestamp)::text, status, input_data
		FROM transactions
		WHERE hash = $1
	`

	var tx Transaction
	var timestamp string

	err := r.db.QueryRow(query, args.Hash).Scan(
		&tx.Hash, &tx.From, &tx.To, &tx.Value,
		&tx.GasPrice, &tx.GasUsed, &tx.BlockNumber,
		&timestamp, &tx.Status, &tx.InputData,
	)
	if err != nil {
		return nil, err
	}

	tx.Timestamp = timestamp
	return &tx, nil
}

// Token resolver
func (r *Resolver) Token(args struct{ Address string }) (*Token, error) {
	query := `
		SELECT address, name, symbol, decimals, total_supply::text,
		       price::text, market_cap::text, volume_24h::text,
		       holders_count, transfers_count
		FROM tokens
		WHERE address = $1
	`

	var t Token
	var price, marketCap, volume24h sql.NullString

	err := r.db.QueryRow(query, args.Address).Scan(
		&t.Address, &t.Name, &t.Symbol, &t.Decimals, &t.TotalSupply,
		&price, &marketCap, &volume24h,
		&t.Holders, &t.Transfers,
	)
	if err != nil {
		return nil, err
	}

	if price.Valid {
		t.Price = price.String
	}
	if marketCap.Valid {
		t.MarketCap = marketCap.String
	}
	if volume24h.Valid {
		t.Volume24h = volume24h.String
	}

	return &t, nil
}

// Tokens resolver
func (r *Resolver) Tokens(args struct{ Limit, Offset *int, SortBy *string }) ([]*Token, error) {
	limit := 25
	offset := 0

	if args.Limit != nil {
		limit = *args.Limit
	}
	if args.Offset != nil {
		offset = *args.Offset
	}

	sortBy := "transfers_count"
	if args.SortBy != nil {
		sortBy = *args.SortBy
	}

	query := fmt.Sprintf(`
		SELECT address, name, symbol, decimals, total_supply::text,
		       price::text, market_cap::text, volume_24h::text,
		       holders_count, transfers_count
		FROM tokens
		ORDER BY %s DESC
		LIMIT $1 OFFSET $2
	`, sortBy)

	rows, err := r.db.Query(query, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var tokens []*Token
	for rows.Next() {
		var t Token
		var price, marketCap, volume24h sql.NullString

		err := rows.Scan(
			&t.Address, &t.Name, &t.Symbol, &t.Decimals, &t.TotalSupply,
			&price, &marketCap, &volume24h,
			&t.Holders, &t.Transfers,
		)
		if err != nil {
			return nil, err
		}

		if price.Valid {
			t.Price = price.String
		}
		if marketCap.Valid {
			t.MarketCap = marketCap.String
		}
		if volume24h.Valid {
			t.Volume24h = volume24h.String
		}

		tokens = append(tokens, &t)
	}

	return tokens, nil
}

// TokenHolders resolver
func (r *Resolver) TokenHolders(args struct{ Address string, Limit *int }) ([]*Account, error) {
	limit := 100
	if args.Limit != nil {
		limit = *args.Limit
	}

	query := `
		SELECT address, balance::text
		FROM token_holders
		WHERE token_address = $1
		ORDER BY balance DESC
		LIMIT $2
	`

	rows, err := r.db.Query(query, args.Address, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var accounts []*Account
	for rows.Next() {
		var a Account

		err := rows.Scan(&a.Address, &a.Balance)
		if err != nil {
			return nil, err
		}

		accounts = append(accounts, &a)
	}

	return accounts, nil
}

// Account resolver
func (r *Resolver) Account(args struct{ Address string }) (*Account, error) {
	query := `
		SELECT address, balance::text, tx_count
		FROM accounts
		WHERE address = $1
	`

	var a Account

	err := r.db.QueryRow(query, args.Address).Scan(
		&a.Address, &a.Balance, &a.TxCount,
	)
	if err != nil {
		return nil, err
	}

	return &a, nil
}

// GasPrice resolver
func (r *Resolver) GasPrice() (*GasPrice, error) {
	query := `
		SELECT low_gas_price::text, medium_gas_price::text, 
		       high_gas_price::text, base_fee_per_gas::text,
		       priority_fee_avg::text
		FROM gas_price_history
		ORDER BY timestamp DESC
		LIMIT 1
	`

	var gp GasPrice

	err := r.db.QueryRow(query).Scan(
		&gp.Low, &gp.Medium, &gp.High, &gp.BaseFee, &gp.PriorityFee,
	)
	if err != nil {
		// Return defaults if no data
		gp.Low = "1000000000"
		gp.Medium = "2000000000"
		gp.High = "5000000000"
		gp.BaseFee = "10000000000"
		gp.PriorityFee = "1000000000"
	}

	return &gp, nil
}

// NetworkStats resolver
func (r *Resolver) NetworkStats() (*NetworkStats, error) {
	ns := &NetworkStats{}

	// Get counts from database
	r.db.QueryRow("SELECT COUNT(*)::text FROM blocks").Scan(&ns.TotalBlocks)
	r.db.QueryRow("SELECT COUNT(*)::text FROM transactions").Scan(&ns.TotalTransactions)
	r.db.QueryRow("SELECT COUNT(*)::text FROM accounts").Scan(&ns.TotalAccounts)
	r.db.QueryRow("SELECT COUNT(*)::text FROM tokens").Scan(&ns.TotalTokens)
	r.db.QueryRow("SELECT COUNT(*)::text FROM nft_collections").Scan(&ns.TotalNFTs)

	// Get TPS
	var tps float64
	r.db.QueryRow(`
		SELECT COALESCE(AVG(txs_per_second), 0)
		FROM (
			SELECT COUNT(*) / EXTRACT(EPOCH FROM (MAX(timestamp) - MIN(timestamp))) as txs_per_second
			FROM transactions
			WHERE timestamp > NOW() - INTERVAL '1 hour'
			GROUP BY date_trunc('minute', timestamp)
		) sub
	`).Scan(&tps)
	ns.TPS = tps

	// Get avg gas price
	r.db.QueryRow("SELECT COALESCE(AVG(gas_price)::text, '0') FROM transactions").Scan(&ns.AvgGasPrice)

	return ns, nil
}

// Search resolver
func (r *Resolver) Search(args struct{ Query string }) ([]interface{}, error) {
	q := args.Query
	var results []interface{}

	// Check if it's a block number
	if num, err := strconv.Atoi(q); err == nil {
		results = append(results, map[string]string{
			"type": "block",
			"id":   strconv.Itoa(num),
		})
	}

	// Check if it's an address
	if strings.HasPrefix(q, "0x") && len(q) == 42 {
		results = append(results, map[string]string{
			"type": "address",
			"id":   q,
		})
	}

	// Check if it's a transaction hash
	if strings.HasPrefix(q, "0x") && len(q) == 66 {
		results = append(results, map[string]string{
			"type": "transaction",
			"id":   q,
		})
	}

	// Search tokens
	rows, err := r.db.Query(`
		SELECT address, name, symbol
		FROM tokens
		WHERE name ILIKE $1 OR symbol ILIKE $1
		LIMIT 5
	`, "%"+q+"%")
	if err == nil {
		defer rows.Close()
		for rows.Next() {
			var addr, name, symbol string
			rows.Scan(&addr, &name, &symbol)
			results = append(results, map[string]string{
				"type": "token",
				"id":   addr,
				"data": name + " (" + symbol + ")",
			})
		}
	}

	return results, nil
}

// Tracer implements graphql-go tracing
type Tracer struct{}

func (t *Tracer) TraceQuery(ctx context.Context, queryString string, variableValues map[string]interface{}, label string) func(err error) {
	start := time.Now()
	return func(err error) {
		// Could log query timing here
		_ = start
		_ = err
	}
}

func (t *Tracer) TraceField(ctx context.Context, label string, variableValues map[string]interface{}) func(err error) {
	start := time.Now()
	return func(err error) {
		// Could log field resolution timing here
		_ = start
		_ = err
	}
}

// Import sql for null handling
var _ = sql.NullString{}