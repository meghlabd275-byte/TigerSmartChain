package gateway

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/go-redis/redis/v8"
	"github.com/gorilla/websocket"
	"github.com/jackc/pgx/v5/pgxpool"
)

var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool { return true },
}

// Config holds configuration
type Config struct {
	Port           string
	RedisURL       string
	DatabaseURL    string
	RPCHTTPURL     string
	RateLimitRPS   int
	RateLimitBurst int
}

// Handler handles HTTP requests
type Handler struct {
	config Config
	redis  *redis.Client
	rpcURL string
	pool   *pgxpool.Pool
}

// NewHandler creates a new handler
func NewHandler(config Config) *Handler {
	// Initialize Redis client
	rdb := redis.NewClient(&redis.Options{
		Addr:     config.RedisURL,
		Password: "",
		DB:       0,
	})

	// Initialize PostgreSQL connection pool (advanced database, replaces sqlite).
	var pool *pgxpool.Pool
	if config.DatabaseURL != "" {
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		if p, err := pgxpool.New(ctx, config.DatabaseURL); err != nil {
			log.Printf("[gateway] failed to connect to postgres: %v", err)
		} else {
			pool = p
		}
	}

	return &Handler{
		config: config,
		redis:  rdb,
		rpcURL: config.RPCHTTPURL,
		pool:   pool,
	}
}

// HealthCheck returns health status
func (h *Handler) HealthCheck(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status":    "ok",
		"timestamp": time.Now().Unix(),
	})
}

// GetLatestBlock returns the latest block from the database (with Redis cache).
func (h *Handler) GetLatestBlock(c *gin.Context) {
	ctx := context.Background()
	cached, err := h.redis.Get(ctx, "block:latest").Result()
	if err == nil {
		var block map[string]interface{}
		if json.Unmarshal([]byte(cached), &block) == nil {
			c.JSON(http.StatusOK, block)
			return
		}
	}
	block, err := h.queryOne(ctx, `SELECT id, number, hash, parent_hash, miner, gas_limit, gas_used, timestamp, size, tx_count, base_fee_per_gas, reward FROM blocks WHERE is_uncle = false ORDER BY number DESC LIMIT 1`)
	if err != nil || block == nil {
		// Fall back to RPC chain head if DB has no blocks yet.
		res, rerr := h.rpcCall(ctx, "eth_blockNumber", nil)
		if rerr != nil {
			c.JSON(http.StatusServiceUnavailable, gin.H{"error": rerr.Error()})
			return
		}
		c.JSON(http.StatusOK, gin.H{"number": string(res)})
		return
	}
	if data, err := json.Marshal(block); err == nil {
		h.redis.Set(ctx, "block:latest", data, 10*time.Second)
	}
	c.JSON(http.StatusOK, block)
}

// GetBlock returns a block by number from the database (with Redis cache).
func (h *Handler) GetBlock(c *gin.Context) {
	numberStr := c.Param("number")
	blockNum, err := strconv.ParseInt(numberStr, 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid block number"})
		return
	}
	ctx := context.Background()
	cacheKey := fmt.Sprintf("block:%d", blockNum)
	cached, err := h.redis.Get(ctx, cacheKey).Result()
	if err == nil {
		var block map[string]interface{}
		if json.Unmarshal([]byte(cached), &block) == nil {
			c.JSON(http.StatusOK, block)
			return
		}
	}
	block, err := h.queryOne(ctx, `SELECT id, number, hash, parent_hash, nonce, sha3_uncles, miner, gas_limit, gas_used, timestamp, size, base_fee_per_gas, tx_count, uncle_count, reward FROM blocks WHERE number = $1 LIMIT 1`, blockNum)
	if err != nil {
		dbError(c, err)
		return
	}
	if block == nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "block not found"})
		return
	}
	if data, err := json.Marshal(block); err == nil {
		h.redis.Set(ctx, cacheKey, data, 30*time.Second)
	}
	c.JSON(http.StatusOK, block)
}

// GetBlockTransactions returns transactions for a specific block.
func (h *Handler) GetBlockTransactions(c *gin.Context) {
	numberStr := c.Param("number")
	blockNum, err := strconv.ParseInt(numberStr, 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid block number"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `SELECT id, hash, nonce, from_address, to_address, value, gas_price, gas_used, status, transaction_index FROM transactions WHERE block_number = $1 ORDER BY transaction_index LIMIT $2`, blockNum, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM transactions WHERE block_number = $1`, blockNum)
	respondList(c, rows, int(total))
}

// GetBlocks returns a paginated list of blocks.
func (h *Handler) GetBlocks(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 25)
	offset := paramOffset(c)
	rows, err := h.queryRows(ctx, `SELECT id, number, hash, parent_hash, miner, gas_limit, gas_used, timestamp, size, tx_count, base_fee_per_gas FROM blocks WHERE is_uncle = false ORDER BY number DESC LIMIT $1 OFFSET $2`, limit, offset)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM blocks WHERE is_uncle = false`)
	respondList(c, rows, int(total))
}

// GetTransaction returns a transaction by hash (DB + Redis cache).
func (h *Handler) GetTransaction(c *gin.Context) {
	hash := c.Param("hash")
	ctx := context.Background()
	cached, err := h.redis.Get(ctx, "tx:"+hash).Result()
	if err == nil {
		var tx map[string]interface{}
		if json.Unmarshal([]byte(cached), &tx) == nil {
			c.JSON(http.StatusOK, tx)
			return
		}
	}
	tx, err := h.queryOne(ctx, `SELECT id, hash, nonce, block_number, block_hash, transaction_index, from_address, to_address, value, gas_price, gas_limit, gas_used, max_fee_per_gas, max_priority_fee_per_gas, input, status, transaction_type, contract_address, effective_gas_price FROM transactions WHERE hash = $1 LIMIT 1`, hash)
	if err != nil {
		dbError(c, err)
		return
	}
	if tx == nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "transaction not found"})
		return
	}
	if data, err := json.Marshal(tx); err == nil {
		h.redis.Set(ctx, "tx:"+hash, data, 30*time.Second)
	}
	c.JSON(http.StatusOK, tx)
}

// GetTransactions returns paginated transactions.
func (h *Handler) GetTransactions(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 25)
	offset := paramOffset(c)
	rows, err := h.queryRows(ctx, `SELECT id, hash, nonce, block_number, transaction_index, from_address, to_address, value, gas_price, gas_used, status, transaction_type FROM transactions ORDER BY block_number DESC NULLS LAST, transaction_index DESC NULLS LAST LIMIT $1 OFFSET $2`, limit, offset)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM transactions`)
	respondList(c, rows, int(total))
}

// GetPendingTransactions returns pending transactions from the DB.
func (h *Handler) GetPendingTransactions(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `SELECT id, hash, from_address, to_address, value, gas_price, gas_limit, nonce, input FROM pending_transactions ORDER BY id DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM pending_transactions`)
	respondList(c, rows, int(total))
}

// GetInternalTransactions returns internal transactions list.
func (h *Handler) GetInternalTransactions(c *gin.Context) {
	h.GetInternalTransactionList(c)
}

// Token endpoints
func (h *Handler) GetTokens(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 25)
	offset := paramOffset(c)
	rows, err := h.queryRows(ctx, `SELECT id, address, name, symbol, decimals, total_supply, holders_count, transfers_count, price_usd, is_verified FROM tokens ORDER BY id DESC LIMIT $1 OFFSET $2`, limit, offset)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM tokens`)
	respondList(c, rows, int(total))
}

func (h *Handler) GetToken(c *gin.Context) {
	address := c.Param("address")
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT id, address, name, symbol, decimals, total_supply, holders_count, transfers_count, circulating_supply, price_usd, price_change_24h, market_cap, volume_24h, is_verified, contract_address FROM tokens WHERE address = $1 LIMIT 1`, address)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

func (h *Handler) GetTokenHolders(c *gin.Context) {
	address := c.Param("address")
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `SELECT id, token_address, address, balance, balance_usd, percent_holdings, updated_block FROM token_holders WHERE token_address = $1 ORDER BY balance DESC LIMIT $2`, address, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM token_holders WHERE token_address = $1`, address)
	respondList(c, rows, int(total))
}

func (h *Handler) GetTokenTransfers(c *gin.Context) {
	address := c.Param("address")
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `SELECT id, token_address, from_address, to_address, value, transaction_hash, block_number, log_index FROM token_transfers WHERE token_address = $1 ORDER BY block_number DESC, log_index DESC LIMIT $2`, address, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM token_transfers WHERE token_address = $1`, address)
	respondList(c, rows, int(total))
}

// NFT endpoints
func (h *Handler) GetNFTCollections(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 25)
	offset := paramOffset(c)
	rows, err := h.queryRows(ctx, `SELECT id, address, name, symbol, contract_type, total_supply, holders_count, floor_price, volume_24h, market_cap, is_verified FROM nft_collections ORDER BY id DESC LIMIT $1 OFFSET $2`, limit, offset)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM nft_collections`)
	respondList(c, rows, int(total))
}

func (h *Handler) GetNFTCollection(c *gin.Context) {
	address := c.Param("address")
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT id, address, name, symbol, contract_type, total_supply, holders_count, transfers_count, floor_price, volume_24h, market_cap, description, image_url FROM nft_collections WHERE address = $1 LIMIT 1`, address)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

func (h *Handler) GetNFTToken(c *gin.Context) {
	collection := c.Param("address")
	tokenID := c.Param("token_id")
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT id, collection_address, token_id, owner, uri, metadata FROM nfts WHERE collection_address = $1 AND token_id = $2 LIMIT 1`, collection, tokenID)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

func (h *Handler) GetNFTTransfers(c *gin.Context) {
	collection := c.Param("address")
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `SELECT id, token_address, token_id, from_address, to_address, transaction_hash, block_number FROM nft_transfers WHERE token_address = $1 ORDER BY block_number DESC LIMIT $2`, collection, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM nft_transfers WHERE token_address = $1`, collection)
	respondList(c, rows, int(total))
}

// Contract endpoints
func (h *Handler) GetContract(c *gin.Context) {
	address := c.Param("address")
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT id, address, contract_name, compiler, compiler_version, optimization_enabled, optimization_runs, evm_version, license_type, source_code, abi, bytecode, runtime_bytecode, contract_type, is_verified, verification_status, verified_at FROM contracts WHERE address = $1 LIMIT 1`, address)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

func (h *Handler) GetContractCode(c *gin.Context) {
	address := c.Param("address")
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT address, bytecode, runtime_bytecode FROM contracts WHERE address = $1 LIMIT 1`, address)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

func (h *Handler) GetStorageAt(c *gin.Context) {
	address := c.Param("address")
	key := c.DefaultQuery("key", "")
	ctx := c.Request.Context()
	if key != "" {
		row, err := h.queryOne(ctx, `SELECT storage_key, storage_value FROM state_diffs WHERE address = $1 AND storage_key = $2 ORDER BY block_number DESC LIMIT 1`, address, key)
		if err != nil {
			dbError(c, err)
			return
		}
		respondOne(c, row)
		return
	}
	rows, err := h.queryRows(ctx, `SELECT storage_key, storage_value FROM state_diffs WHERE address = $1 ORDER BY block_number DESC LIMIT 50`, address)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"address": address, "storage": rows})
}

func (h *Handler) VerifyContract(c *gin.Context) {
	// Contract verification is performed by the dedicated verifier service;
	// here we persist the verification request status.
	address := c.Param("address")
	ctx := c.Request.Context()
	_, err := h.pool.Exec(ctx, `UPDATE contracts SET verification_status = 'pending', updated_at = NOW() WHERE address = $1`, address)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"success": true, "message": "verification queued", "address": address})
}

// Address endpoints
func (h *Handler) GetAddress(c *gin.Context) {
	address := c.Param("address")
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT id, address, balance, nonce, code_hash, is_contract, is_verified, token_balance_count, nft_balance_count FROM accounts WHERE address = $1 LIMIT 1`, address)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

func (h *Handler) GetAddressTokens(c *gin.Context) {
	address := c.Param("address")
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `SELECT id, token_address, address, balance, balance_usd FROM token_holders WHERE address = $1 ORDER BY balance_usd DESC NULLS LAST LIMIT $2`, address, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetAddressNFTs(c *gin.Context) {
	address := c.Param("address")
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `SELECT id, token_address, token_id, owner, updated_block FROM nft_owners WHERE owner = $1 ORDER BY updated_block DESC LIMIT $2`, address, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// Analytics endpoints
func (h *Handler) GetNetworkStats(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT date, total_blocks, total_transactions, total_gas_used, total_gas_fees, total_volume, avg_gas_price, avg_block_time, new_contracts, new_tokens, new_nfts FROM analytics_daily ORDER BY date DESC LIMIT 1`)
	if err != nil || row == nil {
		c.JSON(http.StatusOK, gin.H{"error": "no stats available"})
		return
	}
	c.JSON(http.StatusOK, row)
}

func (h *Handler) GetTransactionChart(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 30)
	rows, err := h.queryRows(ctx, `SELECT date, total_transactions, total_gas_used, total_gas_fees FROM analytics_daily ORDER BY date DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, rows)
}

func (h *Handler) GetAddressChart(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 30)
	rows, err := h.queryRows(ctx, `SELECT date, total_blocks, total_transactions, active_addresses FROM analytics_daily ORDER BY date DESC LIMIT $1`, limit)
	if err != nil {
		// active_addresses may not exist; retry without it.
		rows, err = h.queryRows(ctx, `SELECT date, total_blocks, total_transactions FROM analytics_daily ORDER BY date DESC LIMIT $1`, limit)
		if err != nil {
			dbError(c, err)
			return
		}
	}
	c.JSON(http.StatusOK, rows)
}

func (h *Handler) GetGasOracle(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT gas_price, gas_used, gas_limit, base_fee FROM gas_prices ORDER BY id DESC LIMIT 1`)
	if err != nil || row == nil {
		c.JSON(http.StatusOK, gin.H{"slow": "0", "standard": "0", "fast": "0", "baseFee": "0"})
		return
	}
	gp, _ := row["gas_price"].(int64)
	c.JSON(http.StatusOK, gin.H{
		"slow":     fmt.Sprintf("%d", gp),
		"standard": fmt.Sprintf("%d", gp),
		"fast":     fmt.Sprintf("%d", gp*2),
		"baseFee":  fmt.Sprintf("%v", row["base_fee"]),
	})
}

// Search endpoints
func (h *Handler) Search(c *gin.Context) {
	h.AdvancedSearch(c)
}

// DEX endpoints
func (h *Handler) GetDexPairs(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 25)
	offset := paramOffset(c)
	rows, err := h.queryRows(ctx, `SELECT id, pair_address, token0_address, token1_address, token0_symbol, token1_symbol, reserve0, reserve1, liquidity_usd, volume_24h, factory_address FROM dex_pairs ORDER BY id DESC LIMIT $1 OFFSET $2`, limit, offset)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM dex_pairs`)
	respondList(c, rows, int(total))
}

func (h *Handler) GetDexPair(c *gin.Context) {
	address := c.Param("address")
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT id, pair_address, token0_address, token1_address, token0_symbol, token1_symbol, reserve0, reserve1, liquidity_usd, volume_24h, factory_address, pair_type FROM dex_pairs WHERE pair_address = $1 LIMIT 1`, address)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

// Governance endpoints
// Governance endpoints


// WebSocket handler
func (h *Handler) HandleWebSocket(c *gin.Context) {
	conn, err := upgrader.Upgrade(c.Writer, c.Request, nil)
	if err != nil {
		log.Println("Upgrade error:", err)
		return
	}
	defer conn.Close()

	log.Println("WebSocket client connected")

	for {
		messageType, message, err := conn.ReadMessage()
		if err != nil {
			break
		}
		
		if messageType == websocket.TextMessage {
			// Echo back for now
			conn.WriteMessage(websocket.TextMessage, message)
		}
	}

	log.Println("WebSocket client disconnected")
}

// LoadConfig loads configuration from environment
func LoadConfig() Config {
	return Config{
		Port:           getEnv("PORT", "8080"),
		RedisURL:       getEnv("REDIS_URL", "localhost:6379"),
		DatabaseURL:    getEnv("DATABASE_URL", "postgres://localhost:5432/tigerscan"),
		RPCHTTPURL:     getEnv("RPC_HTTP_URL", "https://bsc-dataseed1.binance.org"),
		RateLimitRPS:   100,
		RateLimitBurst: 200,
	}
}

func getEnv(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}

