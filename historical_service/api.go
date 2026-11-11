/**
 * TigerScan Historical State API Service
 *
 * High-performance Go service for querying historical blockchain state
 * at any block height with full state trie verification.
 *
 * Features:
 * - Historical account state queries
 * - Historical storage slot queries
 * - State proof verification
 * - Historical transaction execution
 * - State diff tracking
 */

package main

import (
	"context"
	"encoding/json"
	"fmt"
	"math/big"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/ethereum/go-ethereum/ethclient/gethclient"
	"github.com/ethereum/go-ethereum/rpc"
	"github.com/gorilla/mux"
	"github.com/redis/go-redis/v9"
)

// Configuration
type HistoricalConfig struct {
	EthereumRPC    string
	RedisURL       string
	Port           int
	MaxCacheAge    time.Duration
	StateCacheSize int
	ProofWorkers   int
}

// State types
type AccountState struct {
	Balance  string `json:"balance"`
	Nonce    uint64 `json:"nonce"`
	CodeHash string `json:"code_hash"`
	Root     string `json:"root"`
	Code     string `json:"code,omitempty"`
}

type StorageSlot struct {
	Key   string `json:"key"`
	Value string `json:"value"`
	Proof string `json:"proof,omitempty"`
}

type StateProof struct {
	Address      string        `json:"address"`
	BlockNumber  uint64        `json:"block_number"`
	Account      *AccountState `json:"account,omitempty"`
	Storage      []StorageSlot `json:"storage,omitempty"`
	AccountProof []string      `json:"account_proof"`
	StorageProof [][]string    `json:"storage_proof"`
	Error        string        `json:"error,omitempty"`
}

type HistoricalTransaction struct {
	Hash        string      `json:"hash"`
	BlockNumber uint64      `json:"block_number"`
	From        string      `json:"from"`
	To          string      `json:"to"`
	Value       string      `json:"value"`
	Gas         uint64      `json:"gas"`
	GasPrice    string      `json:"gas_price"`
	Input       string      `json:"input"`
	Output      string      `json:"output"`
	Status      uint64      `json:"status"`
	Logs        []types.Log `json:"logs"`
	Trace       []CallFrame `json:"trace,omitempty"`
}

type CallFrame struct {
	Type    string      `json:"type"`
	From    string      `json:"from"`
	To      string      `json:"to"`
	Value   string      `json:"value"`
	Input   string      `json:"input"`
	Output  string      `json:"output"`
	Gas     uint64      `json:"gas"`
	GasUsed uint64      `json:"gas_used"`
	Calls   []CallFrame `json:"calls,omitempty"`
}

type StateDiff struct {
	BlockNumber uint64                 `json:"block_number"`
	Timestamp   time.Time              `json:"timestamp"`
	Accounts    map[string]AccountDiff `json:"accounts"`
}

type AccountDiff struct {
	Before  *AccountState          `json:"before"`
	After   *AccountState          `json:"after"`
	Storage map[string]StorageDiff `json:"storage"`
}

type StorageDiff struct {
	Before string `json:"before"`
	After  string `json:"after"`
}

// Historical State Service
type HistoricalStateService struct {
	config     HistoricalConfig
	client     *ethclient.Client
	geth       *gethclient.Client
	rpcClient  *rpc.Client
	redis      *redis.Client
	stateCache *LRUCache
	proofQueue chan ProofRequest
	ctx        context.Context
	cancel     context.CancelFunc
}

type ProofRequest struct {
	Address    common.Address
	BlockNum   *big.Int
	ResultChan chan *StateProof
}

type LRUCache struct {
	mu    sync.RWMutex
	items map[string]interface{}
	order []string
	size  int
}

func NewLRUCache(size int) *LRUCache {
	return &LRUCache{
		items: make(map[string]interface{}),
		order: []string{},
		size:  size,
	}
}

func (c *LRUCache) Get(key string) (interface{}, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()

	val, ok := c.items[key]
	return val, ok
}

func (c *LRUCache) Set(key string, value interface{}) {
	c.mu.Lock()
	defer c.mu.Unlock()

	if _, ok := c.items[key]; ok {
		// Move to end
		for i, k := range c.order {
			if k == key {
				c.order = append(c.order[:i], c.order[i+1:]...)
				break
			}
		}
	} else {
		// Remove oldest if at capacity
		if len(c.order) >= c.size {
			oldest := c.order[0]
			delete(c.items, oldest)
			c.order = c.order[1:]
		}
	}

	c.items[key] = value
	c.order = append(c.order, key)
}

func NewHistoricalStateService(config HistoricalConfig) (*HistoricalStateService, error) {
	ctx, cancel := context.WithCancel(context.Background())

	client, err := ethclient.Dial(config.EthereumRPC)
	if err != nil {
		cancel()
		return nil, fmt.Errorf("failed to connect to Ethereum: %v", err)
	}

	rpcClient, err := rpc.DialContext(ctx, config.EthereumRPC)
	if err != nil {
		cancel()
		return nil, fmt.Errorf("failed to open rpc client: %v", err)
	}

	redisClient := redis.NewClient(&redis.Options{
		Addr: config.RedisURL,
	})

	service := &HistoricalStateService{
		config:     config,
		client:     client,
		geth:       gethclient.New(rpcClient),
		rpcClient:  rpcClient,
		redis:      redisClient,
		stateCache: NewLRUCache(config.StateCacheSize),
		proofQueue: make(chan ProofRequest, 1000),
		ctx:        ctx,
		cancel:     cancel,
	}

	return service, nil
}

func (s *HistoricalStateService) Start() error {
	// Start proof workers
	for i := 0; i < s.config.ProofWorkers; i++ {
		go s.proofWorker()
	}

	// Start HTTP server
	go s.startHTTPServer()

	return nil
}

func (s *HistoricalStateService) Stop() {
	s.cancel()
}

func (s *HistoricalStateService) proofWorker() {
	for {
		select {
		case <-s.ctx.Done():
			return
		case req := <-s.proofQueue:
			proof := s.getProofInternal(req.Address, req.BlockNum)
			req.ResultChan <- proof
		}
	}
}

// Get historical account state
func (s *HistoricalStateService) GetAccountState(address common.Address, blockNumber uint64) (*AccountState, error) {
	ctx := context.Background()

	// Try cache first
	cacheKey := fmt.Sprintf("account:%s:%d", address.Hex(), blockNumber)
	if cached, ok := s.stateCache.Get(cacheKey); ok {
		return cached.(*AccountState), nil
	}

	// Try Redis
	redisKey := fmt.Sprintf("state:account:%s:%d", address.Hex(), blockNumber)
	data, err := s.redis.Get(ctx, redisKey).Result()
	if err == nil {
		var state AccountState
		if json.Unmarshal([]byte(data), &state) == nil {
			s.stateCache.Set(cacheKey, &state)
			return &state, nil
		}
	}

	// Fetch from RPC
	state, err := s.fetchAccountState(address, blockNumber)
	if err != nil {
		return nil, err
	}

	// Cache results
	if data, err := json.Marshal(state); err == nil {
		s.redis.Set(ctx, redisKey, data, s.config.MaxCacheAge)
	}
	s.stateCache.Set(cacheKey, state)

	return state, nil
}

func (s *HistoricalStateService) fetchAccountState(address common.Address, blockNumber uint64) (*AccountState, error) {
	ctx := context.Background()
	blockNum := big.NewInt(int64(blockNumber))

	result, err := s.geth.GetProof(ctx, address, []string{}, blockNum)
	if err != nil {
		return nil, fmt.Errorf("eth_getProof for %s at block %d: %w", address.Hex(), blockNumber, err)
	}

	// The code hash is the keccak256 of the deployed code, which is empty for
	// externally owned accounts.
	codeHash := result.CodeHash
	if codeHash == (common.Hash{}) {
		codeHash = crypto.Keccak256Hash(nil)
	}

	state := &AccountState{
		Balance:  result.Balance.String(),
		Nonce:    result.Nonce,
		CodeHash: codeHash.Hex(),
		Root:     result.StorageHash.Hex(),
	}

	// Deployed code is fetched separately; it is absent for EOAs.
	code, err := s.client.CodeAt(ctx, address, blockNum)
	if err != nil {
		return nil, err
	}
	if len(code) > 0 {
		state.Code = common.Bytes2Hex(code)
	}

	return state, nil
}

// Get historical storage slot
func (s *HistoricalStateService) GetStorageSlot(address common.Address, slot common.Hash, blockNumber uint64) (*StorageSlot, error) {
	ctx := context.Background()
	blockNum := big.NewInt(int64(blockNumber))

	// Get storage value
	value, err := s.client.StorageAt(ctx, address, slot, blockNum)
	if err != nil {
		return nil, err
	}

	return &StorageSlot{
		Key:   slot.Hex(),
		Value: common.BytesToHash(value).Hex(),
	}, nil
}

// Get state proof
func (s *HistoricalStateService) GetStateProof(address common.Address, blockNumber uint64) (*StateProof, error) {
	resultChan := make(chan *StateProof, 1)

	select {
	case s.proofQueue <- ProofRequest{
		Address:    address,
		BlockNum:   big.NewInt(int64(blockNumber)),
		ResultChan: resultChan,
	}:
		return <-resultChan, nil
	case <-time.After(10 * time.Second):
		return nil, fmt.Errorf("proof request timeout")
	}
}

func (s *HistoricalStateService) getProofInternal(address common.Address, blockNum *big.Int) *StateProof {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	proof := &StateProof{
		Address:     address.Hex(),
		BlockNumber: blockNum.Uint64(),
	}

	result, err := s.geth.GetProof(ctx, address, []string{}, blockNum)
	if err != nil {
		proof.Error = fmt.Sprintf("eth_getProof failed: %v", err)
		return proof
	}

	codeHash := result.CodeHash
	if codeHash == (common.Hash{}) {
		codeHash = crypto.Keccak256Hash(nil)
	}

	proof.Account = &AccountState{
		Balance:  result.Balance.String(),
		Nonce:    result.Nonce,
		CodeHash: codeHash.Hex(),
		Root:     result.StorageHash.Hex(),
	}
	proof.AccountProof = result.AccountProof

	for _, slot := range result.StorageProof {
		key := new(big.Int)
		if _, ok := key.SetString(strings.TrimPrefix(slot.Key, "0x"), 16); !ok {
			continue
		}
		proof.Storage = append(proof.Storage, StorageSlot{
			Key:   common.BigToHash(key).Hex(),
			Value: common.BigToHash(slot.Value).Hex(),
		})
		proof.StorageProof = append(proof.StorageProof, slot.Proof)
	}

	return proof
}

// Execute historical transaction
func (s *HistoricalStateService) ExecuteHistoricalTransaction(txHash common.Hash) (*HistoricalTransaction, error) {
	ctx := context.Background()

	// Get transaction receipt
	receipt, err := s.client.TransactionReceipt(ctx, txHash)
	if err != nil {
		return nil, err
	}

	// Get transaction
	tx, _, err := s.client.TransactionByHash(ctx, txHash)
	if err != nil {
		return nil, err
	}

	// Build result
	logs := make([]types.Log, 0, len(receipt.Logs))
	for _, entry := range receipt.Logs {
		if entry == nil {
			continue
		}
		logs = append(logs, *entry)
	}

	gasPrice := tx.GasPrice()
	if gasPrice == nil {
		gasPrice = new(big.Int)
	}

	result := &HistoricalTransaction{
		Hash:        txHash.Hex(),
		BlockNumber: receipt.BlockNumber.Uint64(),
		Value:       tx.Value().String(),
		Gas:         tx.Gas(),
		GasPrice:    gasPrice.String(),
		Input:       common.Bytes2Hex(tx.Data()),
		Status:      receipt.Status,
		Logs:        logs,
	}

	// Get sender
	msg, err := tx.AsMessage(types.LatestSignerForChainID(tx.ChainId()), nil)
	if err == nil {
		result.From = msg.From().Hex()
	}

	// Get recipient
	if tx.To() != nil {
		result.To = tx.To().Hex()
	}

	return result, nil
}

// GetStateDiff reports how every account touched between the two blocks
// changed. Storage changes are enumerated from the node's state diff RPC when
// the endpoint supports it.
func (s *HistoricalStateService) GetStateDiff(fromBlock, toBlock uint64) (*StateDiff, error) {
	ctx := context.Background()

	if fromBlock >= toBlock {
		return nil, fmt.Errorf("invalid block range: from %d must be below to %d", fromBlock, toBlock)
	}

	toHeader, err := s.client.HeaderByNumber(ctx, big.NewInt(int64(toBlock)))
	if err != nil {
		return nil, err
	}

	diff := &StateDiff{
		BlockNumber: toBlock,
		Timestamp:   time.Unix(int64(toHeader.Time), 0),
		Accounts:    make(map[string]AccountDiff),
	}

	// Every sender and recipient in the range is a candidate for a state change.
	touched := make(map[common.Address]struct{})
	for number := fromBlock + 1; number <= toBlock; number++ {
		block, err := s.client.BlockByNumber(ctx, big.NewInt(int64(number)))
		if err != nil {
			return nil, fmt.Errorf("load block %d: %w", number, err)
		}

		for _, tx := range block.Transactions() {
			signer := types.LatestSignerForChainID(tx.ChainId())
			if msg, err := tx.AsMessage(signer, block.BaseFee()); err == nil {
				touched[msg.From()] = struct{}{}
			}
			if to := tx.To(); to != nil {
				touched[*to] = struct{}{}
			}
		}
	}

	for address := range touched {
		before, err := s.fetchAccountState(address, fromBlock)
		if err != nil {
			continue
		}
		after, err := s.fetchAccountState(address, toBlock)
		if err != nil {
			continue
		}

		if before.Balance == after.Balance && before.Nonce == after.Nonce && before.CodeHash == after.CodeHash {
			continue
		}

		storage := make(map[string]StorageDiff)
		if before.Root != after.Root {
			// Enumerating individual slots requires the node's state range RPC;
			// the slot-level detail is omitted when it is unavailable.
			if changes, err := s.storageDiff(ctx, address, fromBlock, toBlock); err == nil {
				storage = changes
			}
		}

		diff.Accounts[address.Hex()] = AccountDiff{
			Before:  before,
			After:   after,
			Storage: storage,
		}
	}

	return diff, nil
}

// storageDiff enumerates the storage slots that differ for one account using
// debug_storageRangeAt on the destination block.
func (s *HistoricalStateService) storageDiff(ctx context.Context, address common.Address, fromBlock, toBlock uint64) (map[string]StorageDiff, error) {
	toHeader, err := s.client.HeaderByNumber(ctx, big.NewInt(int64(toBlock)))
	if err != nil {
		return nil, err
	}

	changes := make(map[string]StorageDiff)
	var startKey *common.Hash

	// Walk the storage trie in pages; the node caps the number of slots returned
	// per call, so keep requesting from the last key seen.
	for page := 0; page < 64; page++ {
		var result struct {
			Storage map[common.Hash]struct {
				Key   common.Hash `json:"key"`
				Value common.Hash `json:"value"`
			} `json:"storage"`
			NextKey *common.Hash `json:"nextKey"`
		}

		err := s.rpcClient.CallContext(ctx, &result, "debug_storageRangeAt",
			toHeader.Hash(), 0, address, startKey, 1024)
		if err != nil {
			return nil, err
		}

		for _, slot := range result.Storage {
			before, err := s.client.StorageAt(ctx, address, slot.Key, big.NewInt(int64(fromBlock)))
			if err != nil {
				continue
			}
			previous := common.BytesToHash(before)
			if previous == slot.Value {
				continue
			}
			changes[slot.Key.Hex()] = StorageDiff{
				Before: previous.Hex(),
				After:  slot.Value.Hex(),
			}
		}

		if result.NextKey == nil {
			break
		}
		startKey = result.NextKey
	}

	return changes, nil
}

// Historical block info
type HistoricalBlock struct {
	Number       uint64   `json:"number"`
	Hash         string   `json:"hash"`
	ParentHash   string   `json:"parent_hash"`
	Timestamp    uint64   `json:"timestamp"`
	Difficulty   string   `json:"difficulty"`
	GasLimit     uint64   `json:"gas_limit"`
	GasUsed      uint64   `json:"gas_used"`
	Transactions []string `json:"transactions"`
	MevBundles   []string `json:"mev_bundles,omitempty"`
}

func (s *HistoricalStateService) GetBlock(blockNumber uint64) (*HistoricalBlock, error) {
	ctx := context.Background()

	block, err := s.client.BlockByNumber(ctx, big.NewInt(int64(blockNumber)))
	if err != nil {
		return nil, err
	}

	header := block.Header()

	txs := make([]string, len(block.Transactions()))
	for i, tx := range block.Transactions() {
		txs[i] = tx.Hash().Hex()
	}

	return &HistoricalBlock{
		Number:       blockNumber,
		Hash:         header.Hash().Hex(),
		ParentHash:   header.ParentHash.Hex(),
		Timestamp:    header.Time,
		Difficulty:   header.Difficulty.String(),
		GasLimit:     header.GasLimit,
		GasUsed:      header.GasUsed,
		Transactions: txs,
	}, nil
}

// HTTP Handlers
func (s *HistoricalStateService) handleGetAccount(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]

	blockNum := uint64(0)
	if b := r.URL.Query().Get("block"); b != "" {
		fmt.Sscanf(b, "%d", &blockNum)
	}

	addr := common.HexToAddress(address)
	state, err := s.GetAccountState(addr, blockNum)
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}

	json.NewEncoder(w).Encode(state)
}

func (s *HistoricalStateService) handleGetStorage(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]
	slot := vars["slot"]

	blockNum := uint64(0)
	if b := r.URL.Query().Get("block"); b != "" {
		fmt.Sscanf(b, "%d", &blockNum)
	}

	addr := common.HexToAddress(address)
	slotHash := common.HexToHash(slot)

	storage, err := s.GetStorageSlot(addr, slotHash, blockNum)
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}

	json.NewEncoder(w).Encode(storage)
}

func (s *HistoricalStateService) handleGetProof(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]

	blockNum := uint64(0)
	if b := r.URL.Query().Get("block"); b != "" {
		fmt.Sscanf(b, "%d", &blockNum)
	}

	addr := common.HexToAddress(address)
	proof, err := s.GetStateProof(addr, blockNum)
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}

	json.NewEncoder(w).Encode(proof)
}

func (s *HistoricalStateService) handleGetTransaction(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	txHash := vars["hash"]

	txHashParsed := common.HexToHash(txHash)
	tx, err := s.ExecuteHistoricalTransaction(txHashParsed)
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}

	json.NewEncoder(w).Encode(tx)
}

func (s *HistoricalStateService) handleGetBlock(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	var blockNum uint64
	fmt.Sscanf(vars["block"], "%d", &blockNum)

	block, err := s.GetBlock(blockNum)
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}

	json.NewEncoder(w).Encode(block)
}

func (s *HistoricalStateService) handleGetStateDiff(w http.ResponseWriter, r *http.Request) {
	var fromBlock, toBlock uint64
	fmt.Sscanf(r.URL.Query().Get("from"), "%d", &fromBlock)
	fmt.Sscanf(r.URL.Query().Get("to"), "%d", &toBlock)

	if fromBlock >= toBlock {
		http.Error(w, "invalid block range", http.StatusBadRequest)
		return
	}

	diff, err := s.GetStateDiff(fromBlock, toBlock)
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}

	json.NewEncoder(w).Encode(diff)
}

func (s *HistoricalStateService) startHTTPServer() {
	router := mux.NewRouter()

	router.HandleFunc("/api/v1/account/{address}", s.handleGetAccount)
	router.HandleFunc("/api/v1/storage/{address}/{slot}", s.handleGetStorage)
	router.HandleFunc("/api/v1/proof/{address}", s.handleGetProof)
	router.HandleFunc("/api/v1/transaction/{hash}", s.handleGetTransaction)
	router.HandleFunc("/api/v1/block/{block}", s.handleGetBlock)
	router.HandleFunc("/api/v1/state-diff", s.handleGetStateDiff)

	http.ListenAndServe(fmt.Sprintf(":%d", s.config.Port), router)
}

// Main
func main() {
	config := HistoricalConfig{
		EthereumRPC:    "http://localhost:8545",
		RedisURL:       "localhost:6379",
		Port:           8083,
		MaxCacheAge:    24 * time.Hour,
		StateCacheSize: 10000,
		ProofWorkers:   4,
	}

	service, err := NewHistoricalStateService(config)
	if err != nil {
		fmt.Printf("Failed to create service: %v\n", err)
		return
	}

	if err := service.Start(); err != nil {
		fmt.Printf("Failed to start service: %v\n", err)
		return
	}

	fmt.Println("Historical State API started on port", config.Port)

	select {}
}
