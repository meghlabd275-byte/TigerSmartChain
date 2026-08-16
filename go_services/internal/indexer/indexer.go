package indexer

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"math/big"
	"net/http"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds indexer configuration
type Config struct {
	RPCURL          string
	WSURL           string
	DatabaseURL     string
	RedisURL        string
	StartBlock      uint64
	BatchSize       uint64
	Confirmations   uint64
	Workers         int
	RateLimit       int
}

// Indexer is the main indexer
type Indexer struct {
	config      Config
	ethClient   *ethclient.Client
	pool        *pgxpool.Pool
	redis       *redis.Client
	currentBlock uint64
	mu          sync.RWMutex
}

// LoadConfig loads configuration from environment
func LoadConfig() Config {
	return Config{
		RPCURL:        getEnv("RPC_URL", "https://bsc-dataseed1.binance.org"),
		WSURL:         getEnv("WS_URL", "wss://bsc-ws.noderadio.cn"),
		DatabaseURL:   getEnv("DATABASE_URL", "postgres://postgres:postgres@localhost:5432/tigerscan"),
		RedisURL:     getEnv("REDIS_URL", "localhost:6379"),
		StartBlock:   0,
		BatchSize:    100,
		Confirmations: 12,
		Workers:      10,
		RateLimit:    100,
	}
}

// NewIndexer creates a new indexer
func NewIndexer(config Config) *Indexer {
	// Connect to Ethereum node
	client, err := ethclient.Dial(config.RPCURL)
	if err != nil {
		log.Fatalf("Failed to connect to Ethereum node: %v", err)
	}

	// Connect to PostgreSQL
	pool, err := pgxpool.New(context.Background(), config.DatabaseURL)
	if err != nil {
		log.Fatalf("Failed to connect to database: %v", err)
	}

	// Connect to Redis
	rdb := redis.NewClient(&redis.Options{
		Addr: config.RedisURL,
	})

	// Get current block number
	header, err := client.HeaderByNumber(context.Background(), nil)
	if err != nil {
		log.Fatalf("Failed to get latest block: %v", err)
	}

	return &Indexer{
		config:      config,
		ethClient:   client,
		pool:        pool,
		redis:       rdb,
		currentBlock: header.Number.Uint64(),
	}
}

// StartBlockIndexer indexes blocks
func (i *Indexer) StartBlockIndexer(ctx context.Context) error {
	log.Println("Starting block indexer...")

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
		}

		// Get last indexed block from Redis
		lastBlock := i.getLastIndexedBlock("blocks")

		// Process blocks in batches
		for blockNum := lastBlock + 1; blockNum <= i.currentBlock-i.config.Confirmations; blockNum++ {
			if err := i.indexBlock(blockNum); err != nil {
				log.Printf("Error indexing block %d: %v", blockNum, err)
				time.Sleep(time.Second)
				continue
			}

			i.setLastIndexedBlock("blocks", blockNum)

			// Update current block every 100 blocks
			if blockNum%100 == 0 {
				log.Printf("Indexed blocks up to %d", blockNum)
			}
		}

		// Update current block
		header, err := i.ethClient.HeaderByNumber(ctx, nil)
		if err == nil {
			i.mu.Lock()
			i.currentBlock = header.Number.Uint64()
			i.mu.Unlock()
		}

		time.Sleep(12 * time.Second) // BSC block time
	}
}

func (i *Indexer) indexBlock(blockNum uint64) error {
	block, err := i.ethClient.BlockByNumber(context.Background(), big.NewInt(int64(blockNum)))
	if err != nil {
		return fmt.Errorf("failed to get block: %w", err)
	}

	// Insert block into database
	_, err = i.pool.Exec(context.Background(), `
		INSERT INTO blocks (number, hash, parent_hash, nonce, gas_limit, gas_used, timestamp, miner, size, base_fee_per_gas, transactions_count)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
		ON CONFLICT (number) DO NOTHING
	`,
		block.Number().Uint64(),
		block.Hash().Hex(),
		block.ParentHash().Hex(),
		fmt.Sprintf("0x%x", block.Nonce()),
		block.GasLimit(),
		block.GasUsed(),
		block.Time(),
		block.Coinbase().Hex(),
		int64(block.Size()),
		block.BaseFee(),
		len(block.Transactions()),
	)

	if err != nil {
		return fmt.Errorf("failed to insert block: %w", err)
	}

	// Index transactions
	for _, tx := range block.Transactions() {
		if err := i.indexTransaction(tx, block); err != nil {
			log.Printf("Error indexing transaction %s: %v", tx.Hash().Hex(), err)
		}
	}

	return nil
}

func (i *Indexer) indexTransaction(tx *types.Transaction, block *types.Block) error {
	receipt, err := i.ethClient.TransactionReceipt(context.Background(), tx.Hash())
	if err != nil {
		return fmt.Errorf("failed to get receipt: %w", err)
	}

	var toAddr string
	if tx.To() != nil {
		toAddr = tx.To().Hex()
	}

	// Derive sender address from the transaction signature
	fromAddr := ""
	chainID := big.NewInt(56) // BSC chain ID
	if i.ethClient != nil {
		if cid, err := i.ethClient.ChainID(context.Background()); err == nil {
			chainID = cid
		}
	}
	signer := types.NewLondonSigner(chainID)
	if sender, err := types.Sender(signer, tx); err == nil {
		fromAddr = sender.Hex()
	}

	// Determine transaction status
	status := "pending"
	if receipt != nil {
		if receipt.Status == 1 {
			status = "success"
		} else {
			status = "failure"
		}
	}

	// Insert transaction
	_, err = i.pool.Exec(context.Background(), `
		INSERT INTO transactions (hash, block_number, block_hash, from_address, to_address, value, gas_price, gas, nonce, input, tx_type, status)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
		ON CONFLICT (hash) DO NOTHING
	`,
		tx.Hash().Hex(),
		receipt.BlockNumber.Uint64(),
		receipt.BlockHash.Hex(),
		fromAddr,
		toAddr,
		tx.Value().String(),
		tx.GasPrice().String(),
		tx.Gas(),
		tx.Nonce(),
		common.Bytes2Hex(tx.Data()),
		int(tx.Type()),
		status,
	)

	if err != nil {
		return fmt.Errorf("failed to insert transaction: %w", err)
	}

	// Insert logs
	for idx, lg := range receipt.Logs {
		topics := make([]string, len(lg.Topics))
		for j, topic := range lg.Topics {
			topics[j] = topic.Hex()
		}

		_, err = i.pool.Exec(context.Background(), `
			INSERT INTO logs (address, topics, data, block_number, transaction_hash, log_index)
			VALUES ($1, $2, $3, $4, $5, $6)
		`,
			lg.Address.Hex(),
			topics,
			common.Bytes2Hex(lg.Data),
			lg.BlockNumber,
			lg.TxHash.Hex(),
			idx,
		)

		if err != nil {
			log.Printf("Failed to insert log: %v", err)
		}

		// Check for token transfers
		if len(lg.Topics) >= 3 {
			// ERC20 Transfer: Transfer(address,address,uint256)
			if lg.Topics[0].Hex() == "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef" {
				i.processERC20Transfer(lg)
			}
			// ERC721 Transfer
			if lg.Topics[0].Hex() == "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef" {
				i.processERC721Transfer(lg)
			}
		}
	}

	return nil
}

func (i *Indexer) processERC20Transfer(lg *types.Log) {
	if len(lg.Topics) < 3 {
		return
	}

	from := "0x" + strings.TrimPrefix(lg.Topics[1].Hex(), "0x000000000000000000000000")
	to := "0x" + strings.TrimPrefix(lg.Topics[2].Hex(), "0x000000000000000000000000")

	value := new(big.Int).SetBytes(lg.Data).String()

	_, err := i.pool.Exec(context.Background(), `
		INSERT INTO token_transfers (token_address, from_address, to_address, value, transaction_hash, block_number, log_index)
		VALUES ($1, $2, $3, $4, $5, $6, $7)
		ON CONFLICT DO NOTHING
	`,
		lg.Address.Hex(),
		from,
		to,
		value,
		lg.TxHash.Hex(),
		lg.BlockNumber,
		lg.Index,
	)

	if err != nil {
		log.Printf("Failed to process ERC20 transfer: %v", err)
	}
}

func (i *Indexer) processERC721Transfer(lg *types.Log) {
	if len(lg.Topics) < 4 {
		return
	}

	from := "0x" + strings.TrimPrefix(lg.Topics[1].Hex(), "0x000000000000000000000000")
	to := "0x" + strings.TrimPrefix(lg.Topics[2].Hex(), "0x000000000000000000000000")
	tokenId := new(big.Int).SetBytes(lg.Data[:32]).String()

	_, err := i.pool.Exec(context.Background(), `
		INSERT INTO nft_transfers (collection_address, token_id, from_address, to_address, transaction_hash, block_number, log_index)
		VALUES ($1, $2, $3, $4, $5, $6, $7)
		ON CONFLICT DO NOTHING
	`,
		lg.Address.Hex(),
		tokenId,
		from,
		to,
		lg.TxHash.Hex(),
		lg.BlockNumber,
		lg.Index,
	)

	if err != nil {
		log.Printf("Failed to process ERC721 transfer: %v", err)
	}
}

// StartTransactionIndexer indexes transactions
func (i *Indexer) StartTransactionIndexer(ctx context.Context) error {
	log.Println("Transaction indexer uses block indexer")
	return nil
}

// StartTokenIndexer indexes tokens
func (i *Indexer) StartTokenIndexer(ctx context.Context) error {
	log.Println("Starting token indexer...")
	
	// Get known token addresses
	tokenAddresses := i.getKnownTokenAddresses()
	
	for _, addr := range tokenAddresses {
		if err := i.indexToken(common.HexToAddress(addr)); err != nil {
			log.Printf("Error indexing token %s: %v", addr, err)
		}
	}
	
	return nil
}

func (i *Indexer) indexToken(addr common.Address) error {
	// Query token contract for metadata
	// In production, would call token contract methods
	
	log.Printf("Indexing token: %s", addr.Hex())
	return nil
}

// StartNFTIndexer indexes NFTs
func (i *Indexer) StartNFTIndexer(ctx context.Context) error {
	log.Println("Starting NFT indexer...")
	return nil
}

// StartInternalTxIndexer indexes internal transactions
func (i *Indexer) StartInternalTxIndexer(ctx context.Context) error {
	log.Println("Starting internal transaction indexer...")

	// Get last indexed block
	lastBlock := i.getLastIndexedBlock("internal_txs")

	for blockNum := lastBlock + 1; blockNum <= i.currentBlock-i.config.Confirmations; blockNum++ {
		block, err := i.ethClient.BlockByNumber(ctx, big.NewInt(int64(blockNum)))
		if err != nil {
			continue
		}

		for _, tx := range block.Transactions() {
			// Get internal transactions using debug_traceTransaction
			traces, err := i.getInternalTransactions(tx.Hash().Hex())
			if err != nil {
				continue
			}

			for _, trace := range traces {
				i.saveInternalTransaction(tx.Hash().Hex(), blockNum, trace)
			}
		}

		i.setLastIndexedBlock("internal_txs", blockNum)
	}

	return nil
}

type InternalTxTrace struct {
	From  string `json:"from"`
	To    string `json:"to"`
	Value string `json:"value"`
	Call  string `json:"callType"`
	Gas   string `json:"gas"`
}

// traceCallResult represents a single call within a debug_traceTransaction trace
type traceCallResult struct {
	Type    string                 `json:"type"`
	From    string                 `json:"from"`
	To      string                 `json:"to"`
	Value   string                 `json:"value"`
	Gas     string                 `json:"gas"`
	GasUsed string                 `json:"gasUsed"`
	Error   string                 `json:"error"`
	Calls   []traceCallResult      `json:"calls"`
	Output  string                 `json:"output"`
}

// traceResult represents the top-level result of debug_traceTransaction
type traceResult struct {
	Type         string            `json:"type"`
	From         string            `json:"from"`
	To           string            `json:"to"`
	Value        string            `json:"value"`
	Gas          string            `json:"gas"`
	GasUsed      string            `json:"gasUsed"`
	Input        string            `json:"input"`
	Output       string            `json:"output"`
	Calls        []traceCallResult `json:"calls"`
}

// rpcRequest is a JSON-RPC 2.0 request envelope
type rpcRequest struct {
	Jsonrpc string        `json:"jsonrpc"`
	Method  string        `json:"method"`
	Params  []interface{} `json:"params"`
	Id      int           `json:"id"`
}

// getInternalTransactions calls debug_traceTransaction on the RPC node
// and extracts all internal calls (CALL, CALLCODE, DELEGATECALL, STATICCALL, CREATE, CREATE2)
func (i *Indexer) getInternalTransactions(txHash string) ([]InternalTxTrace, error) {
	reqBody := rpcRequest{
		Jsonrpc: "2.0",
		Method:  "debug_traceTransaction",
		Params:  []interface{}{txHash, map[string]interface{}{"tracer": "callTracer"}},
		Id:      1,
	}

	jsonData, err := json.Marshal(reqBody)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal trace request: %w", err)
	}

	httpReq, err := http.NewRequest("POST", i.config.RPCURL, bytes.NewBuffer(jsonData))
	if err != nil {
		return nil, fmt.Errorf("failed to create trace request: %w", err)
	}
	httpReq.Header.Set("Content-Type", "application/json")

	client := &http.Client{Timeout: 30 * time.Second}
	resp, err := client.Do(httpReq)
	if err != nil {
		return nil, fmt.Errorf("failed to send trace request: %w", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("failed to read trace response: %w", err)
	}

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("trace request failed with status %d: %s", resp.StatusCode, string(body))
	}

	// Parse JSON-RPC response
	var rpcResp struct {
		Result traceResult `json:"result"`
		Error  *struct {
			Code    int    `json:"code"`
			Message string `json:"message"`
		} `json:"error"`
	}
	if err := json.Unmarshal(body, &rpcResp); err != nil {
		return nil, fmt.Errorf("failed to parse trace response: %w", err)
	}
	if rpcResp.Error != nil {
		// debug_traceTransaction might not be supported by all RPC providers
		log.Printf("debug_traceTransaction error for %s: code=%d msg=%s", txHash, rpcResp.Error.Code, rpcResp.Error.Message)
		return nil, fmt.Errorf("trace error: %s", rpcResp.Error.Message)
	}

	// Extract internal calls recursively
	var traces []InternalTxTrace
	collectCalls(rpcResp.Result.Calls, &traces)

	return traces, nil
}

// collectCalls recursively extracts internal calls from the trace
func collectCalls(calls []traceCallResult, traces *[]InternalTxTrace) {
	for _, call := range calls {
		// Only include actual internal calls (not the top-level transaction)
		if call.Type == "CALL" || call.Type == "CALLCODE" ||
			call.Type == "DELEGATECALL" || call.Type == "STATICCALL" ||
			call.Type == "CREATE" || call.Type == "CREATE2" {
			*traces = append(*traces, InternalTxTrace{
				From:  call.From,
				To:    call.To,
				Value: call.Value,
				Call:  strings.ToLower(call.Type),
				Gas:   call.Gas,
			})
		}
		// Recurse into nested calls
		if len(call.Calls) > 0 {
			collectCalls(call.Calls, traces)
		}
	}
}

func (i *Indexer) saveInternalTransaction(txHash string, blockNum uint64, trace InternalTxTrace) {
	_, err := i.pool.Exec(context.Background(), `
		INSERT INTO traces (transaction_hash, block_number, from_address, to_address, value, call_type, gas)
		VALUES ($1, $2, $3, $4, $5, $6, $7)
		ON CONFLICT DO NOTHING
	`,
		txHash,
		blockNum,
		trace.From,
		trace.To,
		trace.Value,
		trace.Call,
		trace.Gas,
	)

	if err != nil {
		log.Printf("Failed to save internal transaction: %v", err)
	}
}

func (i *Indexer) getLastIndexedBlock(key string) uint64 {
	ctx := context.Background()
	val, err := i.redis.Get(ctx, "indexer:last_block:"+key).Result()
	if err != nil {
		return i.config.StartBlock
	}

	var block uint64
	json.Unmarshal([]byte(val), &block)
	return block
}

func (i *Indexer) setLastIndexedBlock(key string, block uint64) {
	ctx := context.Background()
	val, _ := json.Marshal(block)
	i.redis.Set(ctx, "indexer:last_block:"+key, val, 0)
}

func (i *Indexer) getKnownTokenAddresses() []string {
	// Common BSC tokens
	return []string{
		"0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173b095c", // WBNB
		"0x55d398326f99059fF775485246999027B3197955", // USDT
		"0xe9e7CEA3DedcA5984780Bafc599bD69ADd087D56", // BUSD
		"0x8Ba1f109551bD432803012645Ac136ddd64DBA72", // BNB
		"0x10ed43c718714eb63d5aa57b78b54704e256024e", // CAKE
	}
}

func getEnv(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}
