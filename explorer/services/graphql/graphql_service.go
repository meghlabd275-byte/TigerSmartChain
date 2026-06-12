// Package graphql provides GraphQL API for TigerScan blockchain explorer
// with complete schema for blocks, transactions, tokens, NFTs, validators
package graphql

import (
	"context"
	"fmt"
	"math/big"
	"net/http"
	"strconv"
	"time"

	"github.com/99designs/gqlgen/graphql"
	"github.com/99designs/gqlgen/graphql/handler"
	"github.com/99designs/gqlgen/graphql/handler/transport"
	"github.com/gin-gonic/gin"
)

// =============================================================================
// SCHEMA DEFINITIONS
// =============================================================================

// GraphQL Schema for TigerScan
const typeDefs = `
scalar BigInt
scalar JSON
scalar DateTime

type Query {
	# Blocks
	block(number: BigInt!): Block
	blocks(limit: Int, offset: Int, fromBlock: BigInt, toBlock: BigInt): [Block!]!
	latestBlock: Block
	
	# Transactions
	transaction(hash: String!): Transaction
	transactions(address: String, block: BigInt, limit: Int, offset: Int): [Transaction!]!
	pendingTransactions(address: String, limit: Int): [Transaction!]!
	
	# Accounts
	account(address: String!): Account
	accounts(limit: Int, offset: Int, hasCode: Boolean): [Account!]!
	balanceHistory(address: String!, fromBlock: BigInt, toBlock: BigInt): [BalancePoint!]!
	
	# Tokens
	token(address: String!): Token
	tokens(type: String, verified: Boolean, limit: Int, offset: Int, sortBy: String): [Token!]!
	tokenHolders(tokenAddress: String!, limit: Int, offset: Int): [TokenHolder!]!
	tokenTransfers(tokenAddress: String, from: String, to: String, limit: Int, offset: Int): [TokenTransfer!]!
	
	# NFTs
	nftCollection(address: String!): NFTCollection
	nftCollections(limit: Int, offset: Int): [NFTCollection!]!
	nft(collectionAddress: String!, tokenId: String!): NFT
	nfts(collectionAddress: String!, owner: String, limit: Int, offset: Int): [NFT!]!
	nftTransfers(collectionAddress: String, from: String, to: String, limit: Int, offset: Int): [NFTTransfer!]!
	
	# Validators
	validator(address: String!): Validator
	validators(active: Boolean, limit: Int, offset: Int): [Validator!]!
	delegations(validatorAddress: String!, limit: Int): [Delegation!]!
	
	# Network
	networkStats: NetworkStats!
	gasTracker: GasTracker!
	tpsHistory(from: DateTime, to: DateTime, interval: String): [TPSPoint!]!
	
	# Search
	search(query: String!, limit: Int): [SearchResult!]!
	
	# Contracts
	contract(address: String!): Contract
	contractABI(address: String!): String
	
	# Logs
	logs(address: String, topic0: String, topic1: String, topic2: String, topic3: String, fromBlock: BigInt, toBlock: BigInt, limit: Int): [Log!]!
	
	# Internal Transactions
	internalTransactions(transactionHash: String!): [InternalTransaction!]!
}

type Mutation {
	# Contract Verification
	verifyContract(input: VerifyContractInput!): VerificationResult!
	
	# Subscriptions (for real-time)
	subscribe(channel: String!): Subscription!
	unsubscribe(channel: String!): Boolean!
}

type Subscription {
	newBlock: Block!
	newTransaction: Transaction!
	newPendingTransaction: Transaction!
	newTokenTransfer: TokenTransfer!
	newNFTTransfer: NFTTransfer!
}

# Block
type Block {
	number: BigInt!
	hash: String!
	parentHash: String!
	timestamp: DateTime!
	miner: String!
	gasUsed: BigInt!
	gasLimit: BigInt!
	txCount: Int!
	transactions: [Transaction!]!
	uncles: [String!]!
	baseFeePerGas: BigInt
	reward: BigInt
	size: Int!
}

# Transaction
type Transaction {
	hash: String!
	block: Block
	blockNumber: BigInt!
	from: Account!
	to: Account
	value: BigInt!
	gasPrice: BigInt!
	gasUsed: BigInt
	gasLimit: BigInt!
	nonce: BigInt!
	inputData: String
	status: Boolean!
	fee: BigInt
	timestamp: DateTime!
	logs: [Log!]!
	internalTransactions: [InternalTransaction!]!
	tokenTransfers: [TokenTransfer!]!
}

# Account
type Account {
	address: String!
	balance: BigInt!
	nonce: BigInt!
	codeHash: String
	isContract: Boolean!
	isVerified: Boolean
	contractInfo: Contract
	transactions: [Transaction!]!
	tokenBalances: [TokenBalance!]!
	nftBalances: [NFTBalance!]!
	firstSeen: Block
	lastSeen: Block
}

type TokenBalance {
	token: Token!
	balance: BigInt!
	valueUSD: Float
}

type NFTBalance {
	nft: NFT!
	count: Int!
}

# Token
type Token {
	address: String!
	name: String!
	symbol: String!
	decimals: Int!
	totalSupply: BigInt!
	type: String!
	isVerified: Boolean!
	holderCount: BigInt!
	transferCount: BigInt!
	priceUSD: Float
	volume24hUSD: Float
	marketCapUSD: Float
	holders(limit: Int, offset: Int): [TokenHolder!]!
	transfers(limit: Int, offset: Int): [TokenTransfer!]!
}

type TokenHolder {
	address: String!
	balance: BigInt!
	balanceUSD: Float
	rank: Int!
}

type TokenTransfer {
	transaction: Transaction!
	token: Token!
	from: Account!
	to: Account!
	value: BigInt!
	timestamp: DateTime!
}

# NFT
type NFTCollection {
	address: String!
	name: String!
	symbol: String
	type: String!
	totalSupply: BigInt!
	holderCount: BigInt!
	floorPriceUSD: Float
	volume24hUSD: Float
	volume7dUSD: Float
	imageURL: String
	description: String
	nfts(limit: Int, offset: Int): [NFT!]!
}

type NFT {
	collection: NFTCollection!
	tokenId: String!
	owner: Account!
	name: String
	description: String
	imageURL: String
	animationURL: String
	attributes: [NFTAttribute!]
	transfers(limit: Int): [NFTTransfer!]!
}

type NFTAttribute {
	traitType: String!
	value: String!
	rarity: Float
}

type NFTTransfer {
	transaction: Transaction!
	collection: NFTCollection!
	tokenId: String!
	from: Account!
	to: Account!
	amount: BigInt
	timestamp: DateTime!
	priceUSD: Float
}

# Validator
type Validator {
	address: String!
	name: String
	totalStake: BigInt!
	selfStake: BigInt!
	delegatorCount: BigInt!
	commission: Int!
	uptime: Float!
	blocksProposed: BigInt!
	blocksMissed: BigInt!
	rewardsAccumulated: BigInt!
	isActive: Boolean!
	delegations(limit: Int): [Delegation!]!
}

type Delegation {
	delegator: Account!
	validator: Validator!
	amount: BigInt!
	rewards: BigInt!
}

# Network Stats
type NetworkStats {
	totalBlocks: BigInt!
	totalTransactions: BigInt!
	totalAddresses: BigInt!
	totalContracts: BigInt!
	totalTokens: BigInt!
	tps: Float!
	avgBlockTime: Float!
	avgGasPrice: BigInt!
	difficulty: BigInt!
}

type GasTracker {
	low: BigInt!
	medium: BigInt!
	high: BigInt!
	baseFee: BigInt!
	updatedAt: DateTime!
}

type TPSPoint {
	timestamp: DateTime!
	value: Float!
	blockNumber: BigInt!
}

type BalancePoint {
	block: Block!
	balance: BigInt!
	timestamp: DateTime!
}

# Contract
type Contract {
	address: String!
	name: String!
	compilerVersion: String!
	optimization: Boolean!
	optimizerRuns: Int
	evmVersion: String
	license: String
	verifiedAt: DateTime
	sourceCode: String
	abi: String
	isProxy: Boolean
	implementation: Contract
}

# Logs
type Log {
	transaction: Transaction!
	address: String!
	topics: [String!]!
	data: String!
	logIndex: Int!
}

type InternalTransaction {
	transaction: Transaction!
	from: Account!
	to: Account!
	value: BigInt!
	callType: String!
	depth: Int!
	result: String
}

# Search
type SearchResult {
	type: String!
	id: String!
	data: JSON
}

# Verification
input VerifyContractInput {
	address: String!
	name: String!
	compilerVersion: String!
	sourceCode: String!
	optimization: Boolean
	optimizerRuns: Int
	evmVersion: String
	license: String
	constructorArgs: String
	libraries: JSON
}

type VerificationResult {
	success: Boolean!
	address: String!
	message: String
}
`

// =============================================================================
// RESOLVERS
// =============================================================================

// Resolver implements the GraphQL resolver
type Resolver struct {
	db            interface{} // Would be *postgresdb.DB
	ethClient    interface{} // Would be *ethclient.Client
}

// NewResolver creates a new resolver
func NewResolver(db interface{}, ethClient interface{}) *Resolver {
	return &Resolver{
		db:         db,
		ethClient: ethClient,
	}
}

// =============================================================================
// BLOCK RESOLVERS
// =============================================================================

// Block resolves block by number
func (r *Resolver) Block(ctx context.Context, number *big.Int) (*BlockResolver, error) {
	return &BlockResolver{number: number.Uint64()}, nil
}

// Blocks resolves blocks with filters
func (r *Resolver) Blocks(ctx context.Context, limit, offset *int, fromBlock, toBlock *big.Int) ([]*BlockResolver, error) {
	l := 50
	if limit != nil {
		l = *limit
	}
	
	// Would query database
	blocks := make([]*BlockResolver, 0, l)
	return blocks, nil
}

// LatestBlock resolves the latest block
func (r *Resolver) LatestBlock(ctx context.Context) (*BlockResolver, error) {
	return &BlockResolver{}, nil
}

// BlockResolver wraps block data
type BlockResolver struct {
	number   uint64
	hash     string
	timestamp time.Time
}

// Number returns block number
func (b *BlockResolver) Number() graphql.BigInt {
	return graphql.BigInt(b.number)
}

// Hash returns block hash
func (b *BlockResolver) Hash() string {
	return b.hash
}

// Timestamp returns block timestamp
func (b *BlockResolver) Timestamp() graphql.DateTime {
	return graphql.DateTime{Time: b.timestamp}
}

// Miner returns block miner
func (b *BlockResolver) Miner() string {
	return "0x0000000000000000000000000000000000000000"
}

// GasUsed returns gas used
func (b *BlockResolver) GasUsed() graphql.BigInt {
	return graphql.BigInt(15000000)
}

// GasLimit returns gas limit
func (b *BlockResolver) GasLimit() graphql.BigInt {
	return graphql.BigInt(30000000)
}

// TxCount returns transaction count
func (b *BlockResolver) TxCount() int {
	return 100
}

// Transactions returns block transactions
func (b *BlockResolver) Transactions(ctx context.Context) ([]*TransactionResolver, error) {
	return []*TransactionResolver{}, nil
}

// =============================================================================
// TRANSACTION RESOLVERS
// =============================================================================

// Transaction resolves transaction by hash
func (r *Resolver) Transaction(ctx context.Context, hash string) (*TransactionResolver, error) {
	return &TransactionResolver{hash: hash}, nil
}

// Transactions resolves transactions with filters
func (r *Resolver) Transactions(ctx context.Context, address *string, block *big.Int, limit, offset *int) ([]*TransactionResolver, error) {
	return []*TransactionResolver{}, nil
}

// TransactionResolver wraps transaction data
type TransactionResolver struct {
	hash        string
	blockNumber uint64
	from       string
	to         string
	value      *big.Int
	gasUsed    uint64
	status     bool
	timestamp  time.Time
}

// Hash returns transaction hash
func (t *TransactionResolver) Hash() string {
	return t.hash
}

// BlockNumber returns block number
func (t *TransactionResolver) BlockNumber() graphql.BigInt {
	return graphql.BigInt(t.blockNumber)
}

// Value returns transaction value
func (t *TransactionResolver) Value() graphql.BigInt {
	if t.value == nil {
		return graphql.BigInt(0)
	}
	return graphql.BigInt(t.value.Int64())
}

// Status returns transaction status
func (t *TransactionResolver) Status() bool {
	return t.status
}

// Timestamp returns transaction timestamp
func (t *TransactionResolver) Timestamp() graphql.DateTime {
	return graphql.DateTime{Time: t.timestamp}
}

// From returns from address
func (t *TransactionResolver) From() *AccountResolver {
	return &AccountResolver{address: t.from}
}

// To returns to address
func (t *TransactionResolver) To() *AccountResolver {
	if t.to == "" {
		return nil
	}
	return &AccountResolver{address: t.to}
}

// GasPrice returns gas price
func (t *TransactionResolver) GasPrice() graphql.BigInt {
	return graphql.BigInt(2000000000)
}

// GasUsed returns gas used
func (t *TransactionResolver) GasUsed() graphql.BigInt {
	return graphql.BigInt(t.gasUsed)
}

// GasLimit returns gas limit
func (t *TransactionResolver) GasLimit() graphql.BigInt {
	return graphql.BigInt(21000)
}

// Nonce returns nonce
func (t *TransactionResolver) Nonce() graphql.BigInt {
	return graphql.BigInt(0)
}

// InputData returns input data
func (t *TransactionResolver) InputData() string {
	return "0x"
}

// Fee returns transaction fee
func (t *TransactionResolver) Fee() graphql.BigInt {
	return graphql.BigInt(int64(t.gasUsed) * 2000000000)
}

// =============================================================================
// ACCOUNT RESOLVERS
// =============================================================================

// Account resolves account by address
func (r *Resolver) Account(ctx context.Context, address string) (*AccountResolver, error) {
	return &AccountResolver{address: address}, nil
}

// AccountResolver wraps account data
type AccountResolver struct {
	address    string
	balance    *big.Int
	nonce     uint64
	isContract bool
}

// Address returns account address
func (a *AccountResolver) Address() string {
	return a.address
}

// Balance returns account balance
func (a *AccountResolver) Balance() graphql.BigInt {
	if a.balance == nil {
		return graphql.BigInt(0)
	}
	return graphql.BigInt(a.balance.Int64())
}

// Nonce returns account nonce
func (a *AccountResolver) Nonce() graphql.BigInt {
	return graphql.BigInt(a.nonce)
}

// IsContract returns if account is a contract
func (a *AccountResolver) IsContract() bool {
	return a.isContract
}

// =============================================================================
// TOKEN RESOLVERS
// =============================================================================

// Token resolves token by address
func (r *Resolver) Token(ctx context.Context, address string) (*TokenResolver, error) {
	return &TokenResolver{address: address}, nil
}

// TokenResolver wraps token data
type TokenResolver struct {
	address        string
	name           string
	symbol         string
	decimals       int
	totalSupply    *big.Int
	holderCount    int64
	transferCount  int64
	priceUSD       float64
	volume24hUSD   float64
	marketCapUSD   float64
}

// Address returns token address
func (t *TokenResolver) Address() string {
	return t.address
}

// Name returns token name
func (t *TokenResolver) Name() string {
	return t.name
}

// Symbol returns token symbol
func (t *TokenResolver) Symbol() string {
	return t.symbol
}

// Decimals returns token decimals
func (t *TokenResolver) Decimals() int {
	return t.decimals
}

// TotalSupply returns total supply
func (t *TokenResolver) TotalSupply() graphql.BigInt {
	if t.totalSupply == nil {
		return graphql.BigInt(0)
	}
	return graphql.BigInt(t.totalSupply.Int64())
}

// HolderCount returns holder count
func (t *TokenResolver) HolderCount() graphql.BigInt {
	return graphql.BigInt(t.holderCount)
}

// TransferCount returns transfer count
func (t *TokenResolver) TransferCount() graphql.BigInt {
	return graphql.BigInt(t.transferCount)
}

// PriceUSD returns price in USD
func (t *TokenResolver) PriceUSD() float64 {
	return t.priceUSD
}

// Volume24hUSD returns 24h volume in USD
func (t *TokenResolver) Volume24hUSD() float64 {
	return t.volume24hUSD
}

// MarketCapUSD returns market cap in USD
func (t *TokenResolver) MarketCapUSD() float64 {
	return t.marketCapUSD
}

// =============================================================================
// VALIDATOR RESOLVERS
// =============================================================================

// Validator resolves validator by address
func (r *Resolver) Validator(ctx context.Context, address string) (*ValidatorResolver, error) {
	return &ValidatorResolver{address: address}, nil
}

// ValidatorResolver wraps validator data
type ValidatorResolver struct {
	address            string
	name              string
	totalStake        *big.Int
	delegatorCount    int64
	commission        int
	uptime            float64
	blocksProposed    int64
	rewardsAccumulated *big.Int
	isActive          bool
}

// Address returns validator address
func (v *ValidatorResolver) Address() string {
	return v.address
}

// Name returns validator name
func (v *ValidatorResolver) Name() string {
	return v.name
}

// TotalStake returns total stake
func (v *ValidatorResolver) TotalStake() graphql.BigInt {
	if v.totalStake == nil {
		return graphql.BigInt(0)
	}
	return graphql.BigInt(v.totalStake.Int64())
}

// DelegatorCount returns delegator count
func (v *ValidatorResolver) DelegatorCount() graphql.BigInt {
	return graphql.BigInt(v.delegatorCount)
}

// Commission returns commission rate
func (v *ValidatorResolver) Commission() int {
	return v.commission
}

// Uptime returns uptime percentage
func (v *ValidatorResolver) Uptime() float64 {
	return v.uptime
}

// BlocksProposed returns blocks proposed
func (v *ValidatorResolver) BlocksProposed() graphql.BigInt {
	return graphql.BigInt(v.blocksProposed)
}

// RewardsAccumulated returns accumulated rewards
func (v *ValidatorResolver) RewardsAccumulated() graphql.BigInt {
	if v.rewardsAccumulated == nil {
		return graphql.BigInt(0)
	}
	return graphql.BigInt(v.rewardsAccumulated.Int64())
}

// IsActive returns if validator is active
func (v *ValidatorResolver) IsActive() bool {
	return v.isActive
}

// =============================================================================
// NETWORK STATS RESOLVERS
// =============================================================================

// NetworkStats resolves network statistics
func (r *Resolver) NetworkStats(ctx context.Context) (*NetworkStatsResolver, error) {
	return &NetworkStatsResolver{
		totalBlocks:     1000000,
		totalTransactions: 5000000,
		totalAddresses:  100000,
		tps:            45.5,
		avgBlockTime:   3.0,
		avgGasPrice:    2000000000,
	}, nil
}

// NetworkStatsResolver wraps network stats
type NetworkStatsResolver struct {
	totalBlocks      int64
	totalTransactions int64
	totalAddresses   int64
	totalContracts   int64
	totalTokens     int64
	tps             float64
	avgBlockTime    float64
	avgGasPrice     int64
}

// TotalBlocks returns total blocks
func (n *NetworkStatsResolver) TotalBlocks() graphql.BigInt {
	return graphql.BigInt(n.totalBlocks)
}

// TotalTransactions returns total transactions
func (n *NetworkStatsResolver) TotalTransactions() graphql.BigInt {
	return graphql.BigInt(n.totalTransactions)
}

// TotalAddresses returns total addresses
func (n *NetworkStatsResolver) TotalAddresses() graphql.BigInt {
	return graphql.BigInt(n.totalAddresses)
}

// TPS returns transactions per second
func (n *NetworkStatsResolver) TPS() float64 {
	return n.tps
}

// AvgBlockTime returns average block time in seconds
func (n *NetworkStatsResolver) AvgBlockTime() float64 {
	return n.avgBlockTime
}

// AvgGasPrice returns average gas price
func (n *NetworkStatsResolver) AvgGasPrice() graphql.BigInt {
	return graphql.BigInt(n.avgGasPrice)
}

// =============================================================================
// GAS TRACKER RESOLVER
// =============================================================================

// GasTracker resolves gas tracker
func (r *Resolver) GasTracker(ctx context.Context) (*GasTrackerResolver, error) {
	return &GasTrackerResolver{
		low:     1000000000,
		medium:  2000000000,
		high:    5000000000,
		baseFee: 10000000000,
	}, nil
}

// GasTrackerResolver wraps gas prices
type GasTrackerResolver struct {
	low     int64
	medium  int64
	high    int64
	baseFee int64
}

// Low returns low gas price
func (g *GasTrackerResolver) Low() graphql.BigInt {
	return graphql.BigInt(g.low)
}

// Medium returns medium gas price
func (g *GasTrackerResolver) Medium() graphql.BigInt {
	return graphql.BigInt(g.medium)
}

// High returns high gas price
func (g *GasTrackerResolver) High() graphql.BigInt {
	return graphql.BigInt(g.high)
}

// BaseFee returns base fee
func (g *GasTrackerResolver) BaseFee() graphql.BigInt {
	return graphql.BigInt(g.baseFee)
}

// =============================================================================
// SEARCH RESOLVER
// =============================================================================

// Search resolves search results
func (r *Resolver) Search(ctx context.Context, query string, limit *int) ([]*SearchResultResolver, error) {
	l := 10
	if limit != nil {
		l = *limit
	}
	
	// Would search database
	return []*SearchResultResolver{}, nil
}

// SearchResultResolver wraps search result
type SearchResultResolver struct {
	resultType string
	id        string
}

// Type returns result type
func (s *SearchResultResolver) Type() string {
	return s.resultType
}

// ID returns result ID
func (s *SearchResultResolver) ID() string {
	return s.id
}

// =============================================================================
// GQLGEN SERVER
// =============================================================================

// NewGraphQLServer creates a new GraphQL server
func NewGraphQLServer(resolver *Resolver) http.Handler {
	// Using gqlgen would require code generation
	// For now, return a basic handler
	
	// This is a simplified version - production would use gqlgen generated code
	return &GraphQLHandler{resolver: resolver}
}

// GraphQLHandler is a basic GraphQL handler
type GraphQLHandler struct {
	resolver *Resolver
}

func (h *GraphQLHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	// Parse query
	query := r.URL.Query().Get("query")
	if query == "" {
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"errors":[{"message":"No query provided"}]}`))
		return
	}
	
	// Execute query (simplified)
	w.Header().Set("Content-Type", "application/json")
	w.Write([]byte(`{"data":{}}`))
}

// GinHandler returns a Gin handler for GraphQL
func GinHandler(resolver *Resolver) gin.HandlerFunc {
	server := NewGraphQLServer(resolver)
	
	return func(c *gin.Context) {
		server.ServeHTTP(c.Writer, c.Request)
	}
}

// =============================================================================
// EXAMPLE USAGE
// =============================================================================

/*
func main() {
	// Create resolver
	resolver := NewResolver(nil, nil)
	
	// Setup Gin
	r := gin.Default()
	r.POST("/graphql", graphql.GinHandler(resolver))
	r.GET("/graphql", graphql.GinHandler(resolver))
	
	// Alternative: use http
	http.Handle("/graphql", NewGraphQLServer(resolver))
	
	log.Fatal(http.ListenAndServe(":8080", nil))
}
*/

// =============================================================================
// ADDITIONAL RESOLVER INTERFACES (for gqlgen)
// =============================================================================

// TokenHolder returns token holder
func (r *Resolver) TokenHolder(ctx context.Context, tokenAddress string) (*TokenHolderResolver, error) {
	return &TokenHolderResolver{}, nil
}

// TokenTransfer returns token transfer
func (r *Resolver) TokenTransfer(ctx context.Context, tokenAddress string) (*TokenTransferResolver, error) {
	return &TokenTransferResolver{}, nil
}

// TokenHolderResolver wraps token holder
type TokenHolderResolver struct {
	address   string
	balance   *big.Int
	balanceUSD float64
	rank      int
}

// Address returns holder address
func (t *TokenHolderResolver) Address() string {
	return t.address
}

// Balance returns holder balance
func (t *TokenHolderResolver) Balance() graphql.BigInt {
	if t.balance == nil {
		return graphql.BigInt(0)
	}
	return graphql.BigInt(t.balance.Int64())
}

// BalanceUSD returns balance in USD
func (t *TokenHolderResolver) BalanceUSD() float64 {
	return t.balanceUSD
}

// Rank returns holder rank
func (t *TokenHolderResolver) Rank() int {
	return t.rank
}

// TokenTransferResolver wraps token transfer
type TokenTransferResolver struct {
	from     string
	to       string
	value    *big.Int
	timestamp time.Time
}

// From returns from address
func (t *TokenTransferResolver) From() *AccountResolver {
	return &AccountResolver{address: t.from}
}

// To returns to address
func (t *TokenTransferResolver) To() *AccountResolver {
	return &AccountResolver{address: t.to}
}

// Value returns transfer value
func (t *TokenTransferResolver) Value() graphql.BigInt {
	if t.value == nil {
		return graphql.BigInt(0)
	}
	return graphql.BigInt(t.value.Int64())
}

// Timestamp returns timestamp
func (t *TokenTransferResolver) Timestamp() graphql.DateTime {
	return graphql.DateTime{Time: t.timestamp}
}

// BalancePointResolver wraps balance history point
type BalancePointResolver struct {
	balance   *big.Int
	timestamp time.Time
}

// Balance returns balance
func (b *BalancePointResolver) Balance() graphql.BigInt {
	if b.balance == nil {
		return graphql.BigInt(0)
	}
	return graphql.BigInt(b.balance.Int64())
}

// Timestamp returns timestamp
func (b *BalancePointResolver) Timestamp() graphql.DateTime {
	return graphql.DateTime{Time: b.timestamp}
}

// TPSPointResolver wraps TPS data point
type TPSPointResolver struct {
	value       float64
	blockNumber uint64
	timestamp   time.Time
}

// Value returns TPS value
func (t *TPSPointResolver) Value() float64 {
	return t.value
}

// BlockNumber returns block number
func (t *TPSPointResolver) BlockNumber() graphql.BigInt {
	return graphql.BigInt(t.blockNumber)
}

// Timestamp returns timestamp
func (t *TPSPointResolver) Timestamp() graphql.DateTime {
	return graphql.DateTime{Time: t.timestamp}
}

// ContractResolver wraps contract data
type ContractResolver struct {
	address          string
	name            string
	compilerVersion string
	optimization    bool
	optimizerRuns   int
	evmVersion     string
	license        string
	verifiedAt     time.Time
	sourceCode     string
	abi            string
	isProxy        bool
}

// Address returns contract address
func (c *ContractResolver) Address() string {
	return c.address
}

// Name returns contract name
func (c *ContractResolver) Name() string {
	return c.name
}

// CompilerVersion returns compiler version
func (c *ContractResolver) CompilerVersion() string {
	return c.compilerVersion
}

// Optimization returns if optimization is enabled
func (c *ContractResolver) Optimization() bool {
	return c.optimization
}

// OptimizerRuns returns optimizer runs
func (c *ContractResolver) OptimizerRuns() int {
	return c.optimizerRuns
}

// EvmVersion returns EVM version
func (c *ContractResolver) EvmVersion() string {
	return c.evmVersion
}

// License returns license type
func (c *ContractResolver) License() string {
	return c.license
}

// VerifiedAt returns verification timestamp
func (c *ContractResolver) VerifiedAt() *graphql.DateTime {
	if c.verifiedAt.IsZero() {
		return nil
	}
	dt := graphql.DateTime{Time: c.verifiedAt}
	return &dt
}

// SourceCode returns source code
func (c *ContractResolver) SourceCode() string {
	return c.sourceCode
}

// Abi returns contract ABI
func (c *ContractResolver) Abi() string {
	return c.abi
}

// IsProxy returns if contract is a proxy
func (c *ContractResolver) IsProxy() bool {
	return c.isProxy
}

// NFTResolver wraps NFT data
type NFTResolver struct {
	collectionAddress string
	tokenId          string
	owner           string
	name            string
	description    string
	imageURL        string
}

// Collection returns collection
func (n *NFTResolver) Collection() *NFTCollectionResolver {
	return &NFTCollectionResolver{address: n.collectionAddress}
}

// TokenId returns token ID
func (n *NFTResolver) TokenId() string {
	return n.tokenId
}

// Owner returns owner
func (n *NFTResolver) Owner() *AccountResolver {
	return &AccountResolver{address: n.owner}
}

// Name returns name
func (n *NFTResolver) Name() *string {
	if n.name == "" {
		return nil
	}
	return &n.name
}

// Description returns description
func (n *NFTResolver) Description() *string {
	if n.description == "" {
		return nil
	}
	return &n.description
}

// ImageURL returns image URL
func (n *NFTResolver) ImageURL() *string {
	if n.imageURL == "" {
		return nil
	}
	return &n.imageURL
}

// NFTCollectionResolver wraps NFT collection
type NFTCollectionResolver struct {
	address      string
	name        string
	symbol      string
	totalSupply int64
	holderCount int64
	floorPrice  float64
	volume24h   float64
	imageURL    string
}

// Address returns collection address
func (n *NFTCollectionResolver) Address() string {
	return n.address
}

// Name returns collection name
func (n *NFTCollectionResolver) Name() string {
	return n.name
}

// Symbol returns symbol
func (n *NFTCollectionResolver) Symbol() *string {
	if n.symbol == "" {
		return nil
	}
	return &n.symbol
}

// TotalSupply returns total supply
func (n *NFTCollectionResolver) TotalSupply() graphql.BigInt {
	return graphql.BigInt(n.totalSupply)
}

// HolderCount returns holder count
func (n *NFTCollectionResolver) HolderCount() graphql.BigInt {
	return graphql.BigInt(n.holderCount)
}

// FloorPriceUSD returns floor price in USD
func (n *NFTCollectionResolver) FloorPriceUSD() *float64 {
	if n.floorPrice == 0 {
		return nil
	}
	return &n.floorPrice
}

// Volume24hUSD returns 24h volume in USD
func (n *NFTCollectionResolver) Volume24hUSD() *float64 {
	if n.volume24h == 0 {
		return nil
	}
	return &n.volume24h
}

// ImageURL returns image URL
func (n *NFTCollectionResolver) ImageURL() *string {
	if n.imageURL == "" {
		return nil
	}
	return &n.imageURL
}

// DelegationResolver wraps delegation
type DelegationResolver struct {
	delegatorAddress string
	validatorAddress string
	amount          *big.Int
}

// Delegator returns delegator
func (d *DelegationResolver) Delegator() *AccountResolver {
	return &AccountResolver{address: d.delegatorAddress}
}

// Validator returns validator
func (d *DelegationResolver) Validator() *ValidatorResolver {
	return &ValidatorResolver{address: d.validatorAddress}
}

// Amount returns delegation amount
func (d *DelegationResolver) Amount() graphql.BigInt {
	if d.amount == nil {
		return graphql.BigInt(0)
	}
	return graphql.BigInt(d.amount.Int64())
}

// LogResolver wraps log
type LogResolver struct {
	address   string
	topics    []string
	data      string
	logIndex int
}

// Address returns log address
func (l *LogResolver) Address() string {
	return l.address
}

// Topics returns log topics
func (l *LogResolver) Topics() []string {
	return l.topics
}

// Data returns log data
func (l *LogResolver) Data() string {
	return l.data
}

// LogIndex returns log index
func (l *LogResolver) LogIndex() int {
	return l.logIndex
}

// InternalTransactionResolver wraps internal transaction
type InternalTransactionResolver struct {
	from     string
	to       string
	value    *big.Int
	callType string
	depth    int
}

// From returns from address
func (i *InternalTransactionResolver) From() *AccountResolver {
	return &AccountResolver{address: i.from}
}

// To returns to address
func (i *InternalTransactionResolver) To() *AccountResolver {
	return &AccountResolver{address: i.to}
}

// Value returns value
func (i *InternalTransactionResolver) Value() graphql.BigInt {
	if i.value == nil {
		return graphql.BigInt(0)
	}
	return graphql.BigInt(i.value.Int64())
}

// CallType returns call type
func (i *InternalTransactionResolver) CallType() string {
	return i.callType
}

// Depth returns depth
func (i *InternalTransactionResolver) Depth() int {
	return i.depth
}

// VerificationResultResolver wraps verification result
type VerificationResultResolver struct {
	success  bool
	address  string
	message  string
}

// Success returns success status
func (v *VerificationResultResolver) Success() bool {
	return v.success
}

// Address returns address
func (v *VerificationResultResolver) Address() string {
	return v.address
}

// Message returns message
func (v *VerificationResultResolver) Message() *string {
	if v.message == "" {
		return nil
	}
	return &v.message
}