package indexer

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"math/big"
	"strings"
	"sync"
	"time"

	"github.com/ethereum/go-ethereum"
	"github.com/ethereum/go-ethereum/common"
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
		block.Nonce().Hex(),
		block.GasLimit(),
		block.GasUsed(),
		block.Time(),
		block.Coinbase().Hex(),
		block.Size().Int64(),
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

func (i *Indexer) indexTransaction(tx *ethereum.Transaction, block *ethereum.types.Block) error {
	receipt, err := i.ethClient.TransactionReceipt(context.Background(), tx.Hash())
	if err != nil {
		return fmt.Errorf("failed to get receipt: %w", err)
	}

	var toAddr string
	if tx.To() != nil {
		toAddr = tx.To().Hex()
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
		tx.From().Hex(),
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
	for idx, log := range receipt.Logs {
		topics := make([]string, len(log.Topics))
		for j, topic := range log.Topics {
			topics[j] = topic.Hex()
		}

		_, err = i.pool.Exec(context.Background(), `
			INSERT INTO logs (address, topics, data, block_number, transaction_hash, log_index)
			VALUES ($1, $2, $3, $4, $5, $6)
		`,
			log.Address.Hex(),
			topics,
			common.Bytes2Hex(log.Data),
			log.BlockNumber,
			log.TransactionHash.Hex(),
			idx,
		)

		if err != nil {
			log.Printf("Failed to insert log: %v", err)
		}

		// Check for token transfers
		if len(log.Topics) >= 3 {
			// ERC20 Transfer: Transfer(address,address,uint256)
			if log.Topics[0].Hex() == "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef" {
				i.processERC20Transfer(log)
			}
			// ERC721 Transfer
			if log.Topics[0].Hex() == "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef" {
				i.processERC721Transfer(log)
			}
		}
	}

	return nil
}

func (i *Indexer) processERC20Transfer(log *ethereum.types.Log) {
	if len(log.Topics) < 3 {
		return
	}

	from := "0x" + strings.TrimPrefix(log.Topics[1].Hex(), "0x000000000000000000000000")
	to := "0x" + strings.TrimPrefix(log.Topics[2].Hex(), "0x000000000000000000000000")

	value := new(big.Int).SetBytes(log.Data).String()

	_, err := i.pool.Exec(context.Background(), `
		INSERT INTO token_transfers (token_address, from_address, to_address, value, transaction_hash, block_number, log_index)
		VALUES ($1, $2, $3, $4, $5, $6, $7)
		ON CONFLICT DO NOTHING
	`,
		log.Address.Hex(),
		from,
		to,
		value,
		log.TransactionHash.Hex(),
		log.BlockNumber,
		log.Index,
	)

	if err != nil {
		log.Printf("Failed to process ERC20 transfer: %v", err)
	}
}

func (i *Indexer) processERC721Transfer(log *ethereum.types.Log) {
	if len(log.Topics) < 4 {
		return
	}

	from := "0x" + strings.TrimPrefix(log.Topics[1].Hex(), "0x000000000000000000000000")
	to := "0x" + strings.TrimPrefix(log.Topics[2].Hex(), "0x000000000000000000000000")
	tokenId := new(big.Int).SetBytes(log.Data[:32]).String()

	_, err := i.pool.Exec(context.Background(), `
		INSERT INTO nft_transfers (collection_address, token_id, from_address, to_address, transaction_hash, block_number, log_index)
		VALUES ($1, $2, $3, $4, $5, $6, $7)
		ON CONFLICT DO NOTHING
	`,
		log.Address.Hex(),
		tokenId,
		from,
		to,
		log.TransactionHash.Hex(),
		log.BlockNumber,
		log.Index,
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

func (i *Indexer) getInternalTransactions(txHash string) ([]InternalTxTrace, error) {
	// Would call debug_traceTransaction RPC
	// This is a placeholder
	return []InternalTxTrace{}, nil
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
