// Package graphql provides GraphQL API server for TigerScan.
package graphql

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
)

// Resolver provides GraphQL resolution
type Resolver struct {
	db *sql.DB
}

// Block represents a block in GraphQL
type Block struct {
	Number        string `graphql:"number"`
	Hash         string `graphql:"hash"`
	ParentHash   string `graphql:"parentHash"`
	Miner        string `graphql:"miner"`
	Timestamp    string `graphql:"timestamp"`
	Transactions []Transaction `graphql:"transactions"`
}

// Transaction represents a transaction in GraphQL
type Transaction struct {
	Hash       string `graphql:"hash"`
	From      string `graphql:"from"`
	To        string `graphql:"to"`
	Value     string `graphql:"value"`
	GasPrice  string `graphql:"gasPrice"`
	GasUsed   string `graphql:"gasUsed"`
	Timestamp string `graphql:"timestamp"`
	Status    string `graphql:"status"`
}

// Token represents a token in GraphQL
type Token struct {
	ID          string `graphql:"id"`
	Address    string `graphql:"address"`
	Name       string `graphql:"name"`
	Symbol     string `graphql:"symbol"`
	Decimals   int    `graphql:"decimals"`
	TotalSupply string `graphql:"totalSupply"`
	Price      string `graphql:"price"`
	MarketCap  string `graphql:"marketCap"`
}

// Account represents an account in GraphQL
type Account struct {
	Address    string `graphql:"address"`
	Balance    string `graphql:"balance"`
	TxCount   int    `graphql:"txCount"`
	Tokens    []Token `graphql:"tokens"`
}

// Schema is the GraphQL schema
const Schema = `
type Query {
	block(number: Int!): Block
	blocks(limit: Int, offset: Int): [Block!]!
	latestBlock: Block!
	
	transaction(hash: String!): Transaction
	transactions(address: String, limit: Int, offset: Int): [Transaction!]!
	
	token(address: String!): Token
	tokens(limit: Int, offset: Int): [Token!]!
	tokenHolders(address: String!, limit: Int): [Account!]!
	tokenTransfers(address: String!, limit: Int): [Transaction!]!
	
	account(address: String!): Account
	search(query: String!): [SearchResult!]!
	
	gasPrice: GasPrice!
	networkStats: NetworkStats!
}

type Mutation {
	verifyContract(address: String!, source: String!, compiler: String!, version: String!): ContractVerification
	watchAddress(address: String!): WatchSubscription
}

type Subscription {
	newBlock: Block!
	newTransaction(address: String): Transaction!
	newTransactionHash: String!
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
}

type Uncle {
	hash: String!
	miner: String!
	number: String!
	reward: String!
}

type Transaction {
	hash: String!
	from: String!
	to: String!
	value: String!
	gasPrice: String!
	gasUsed: String!
	blockNumber: String!
	timestamp: String!
	status: String!
	inputData: String!
	logs: [Log!]!
	internalTransactions: [InternalTransaction!]!
}

type Log {
	address: String!
	topics: [String!]!
	data: String!
	logIndex: String!
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
	holders: Int!
	transfers: Int!
}

type Account {
	address: String!
	balance: String!
	txCount: Int!
	tokens: [Token!]!
	nfts: [NFT!]!
}

type NFT {
	id: String!
	address: String!
	tokenId: String!
	name: String!
	description: String!
	imageUrl: String!
	owner: String!
	attributes: [Attribute!]!
}

type Attribute {
	traitType: String!
	value: String!
	displayType: String
}

type GasPrice {
	low: String!
	medium: String!
	high: String!
	baseFee: String!
	priorityFee: String!
}

type NetworkStats {
	totalBlocks: String!
	totalTransactions: String!
	totalAccounts: String!
	tps: Float!
	avgGasPrice: String!
}

type ContractVerification {
	address: String!
	name: String!
	compiler: String!
	version: String!
	verified: Boolean!
}

type WatchSubscription {
	address: String!
	active: Boolean!
}

union SearchResult = Block | Transaction | Token | Account | Contract

type Contract {
	address: String!
	name: String!
	abi: String!
	sourceCode: String!
}
`

// Service provides GraphQL API
type Service struct {
	db        *sql.DB
	schema    *graphql.Schema
	router   *gin.Engine
	wsHub   *WebSocketHub
}

// Config holds service configuration
type Config struct {
	DB       *sql.DB
}

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
		db:      cfg.DB,
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

// isWebSocket checks if request is a WebSocket upgrade
func isWebSocket(r *http.Request) bool {
	return r.Header.Get("Upgrade") == "websocket"
}

// Block resolver
func (r *Resolver) Block(args struct{ Number int }) (*Block, error) {
	return &Block{
		Number:      strconv.Itoa(args.Number),
		Hash:        "0x0",
		ParentHash:  "0x0",
		Miner:       "0x0",
		Timestamp:  strconv.FormatInt(time.Now().Unix(), 10),
	}, nil
}

// LatestBlock resolver
func (r *Resolver) LatestBlock() (*Block, error) {
	return &Block{
		Number:     "1",
		Hash:      "0x0",
		Timestamp: strconv.FormatInt(time.Now().Unix(), 10),
	}, nil
}

// Transaction resolver
func (r *Resolver) Transaction(args struct{ Hash string }) (*Transaction, error) {
	return &Transaction{
		Hash:      args.Hash,
		From:     "0x0",
		To:       "0x0",
		Value:    "0",
		Status:   "0x1",
	}, nil
}

// Token resolver
func (r *Resolver) Token(args struct{ Address string }) (*Token, error) {
	return &Token{
		ID:          args.Address,
		Address:    args.Address,
		Name:       "Token",
		Symbol:     "TKN",
		Decimals:   18,
		TotalSupply: "1000000000",
	}, nil
}

// Tokens resolver
func (r *Resolver) Tokens(args struct{ Limit, Offset *int }) ([]*Token, error) {
	limit := 10
	if args.Limit != nil {
		limit = *args.Limit
	}
	
	tokens := make([]*Token, limit)
	for i := 0; i < limit; i++ {
		tokens[i] = &Token{
			ID:        strconv.Itoa(i),
			Address:   "0x0",
			Name:     "Token",
			Symbol:   "TKN",
			Decimals: 18,
		}
	}
	
	return tokens, nil
}

// Account resolver
func (r *Resolver) Account(args struct{ Address string }) (*Account, error) {
	return &Account{
		Address:  args.Address,
		Balance: "0",
		TxCount: 0,
	}, nil
}

// GasPrice resolver
func (r *Resolver) GasPrice() (map[string]string, error) {
	return map[string]string{
		"low":    "1000000000",
		"medium": "2000000000",
		"high":  "5000000000",
		"base":  "10000000000",
	}, nil
}

// NetworkStats resolver
func (r *Resolver) NetworkStats() (map[string]interface{}, error) {
	return map[string]interface{}{
		"totalBlocks":      "1000000",
		"totalTransactions": "5000000",
		"totalAccounts":   "100000",
		"tps":            15.5,
		"avgGasPrice":    "2000000000",
	}, nil
}

// Search resolver
func (r *Resolver) Search(args struct{ Query string }) ([]interface{}, error) {
	return []interface{}{
		map[string]string{"address": args.Query},
	}, nil
}

// Tracer implements graphql-go tracing
type Tracer struct{}

func (t *Tracer) TraceQuery(ctx context.Context, queryString string, variableValues map[string]interface{}, label string) func(err error) {
	return func(err error) {}
}

func (t *Tracer) TraceField(ctx context.Context, label string, variableValues map[string]interface{}) func(err error) {
	return func(err error) {}
}

var _ = fmt.Sprintf // Use fmt
var _ = strconv.Atoi // Use strconv