package main

import (
	"context"
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"math/big"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/go-redis/redis/v8"
	"github.com/golang-jwt/jwt/v5"
	"github.com/jackc/pgx/v4"
	"golang.org/x/time/rate"
)

// ============================================
// Advanced API Configuration
// ============================================

type Config struct {
	Port           string
	DatabaseURL    string
	RedisURL       string
	JWTSecret      string
	RateLimitRPS   float64
	RateLimitBurst int
	CacheTTL       time.Duration
}

var (
	cfg         *Config
	db          *pgx.Conn
	redisClient *redis.Client
	rateLimiter *rate.Limiter
)

// ============================================
// Cursor-based Pagination
// ============================================

type Cursor struct {
	BlockNumber uint64 `json:"block_number"`
	Index       uint64 `json:"index"`
	Hash        string `json:"hash"`
}

type PageInfo struct {
	HasNextPage     bool   `json:"hasNextPage"`
	HasPreviousPage bool   `json:"hasPreviousPage"`
	StartCursor     string `json:"startCursor"`
	EndCursor       string `json:"endCursor"`
}

type PaginatedResponse struct {
	Data       interface{} `json:"data"`
	PageInfo   PageInfo    `json:"pageInfo"`
	TotalCount int64       `json:"totalCount"`
}

// encodeCursor renders a cursor as URL-safe base64 so it survives a round-trip
// through a query string.
func encodeCursor(c *Cursor) string {
	data := fmt.Sprintf("%d:%d:%s", c.BlockNumber, c.Index, c.Hash)
	return base64.RawURLEncoding.EncodeToString([]byte(data))
}

// decodeCursor reverses encodeCursor and rejects malformed input rather than
// silently paginating from the wrong position.
func decodeCursor(encoded string) (*Cursor, error) {
	data, err := base64.RawURLEncoding.DecodeString(encoded)
	if err != nil {
		return nil, fmt.Errorf("cursor is not valid base64: %w", err)
	}

	parts := strings.SplitN(string(data), ":", 3)
	if len(parts) != 3 {
		return nil, fmt.Errorf("cursor has %d fields, want 3", len(parts))
	}

	number, err := strconv.ParseUint(parts[0], 10, 64)
	if err != nil {
		return nil, fmt.Errorf("cursor block number: %w", err)
	}
	index, err := strconv.ParseUint(parts[1], 10, 64)
	if err != nil {
		return nil, fmt.Errorf("cursor index: %w", err)
	}

	return &Cursor{BlockNumber: number, Index: index, Hash: parts[2]}, nil
}

// ============================================
// Advanced API Handlers
// ============================================

// Get blocks with cursor pagination
func GetBlocks(c *gin.Context) {
	// Parse pagination params
	limit := 50
	if l := c.Query("limit"); l != "" {
		if parsed, err := strconv.Atoi(l); err == nil && parsed > 0 && parsed <= 100 {
			limit = parsed
		}
	}

	cursor := c.Query("cursor")
	direction := c.DefaultQuery("direction", "next") // next or previous

	// Build query
	var query string
	var args []interface{}

	if cursor == "" {
		if direction == "next" {
			query = `SELECT number, hash, parent_hash, miner, gas_limit, gas_used, timestamp, size, tx_count
				FROM blocks ORDER BY number DESC LIMIT $1`
			args = []interface{}{limit + 1}
		} else {
			query = `SELECT number, hash, parent_hash, miner, gas_limit, gas_used, timestamp, size, tx_count
				FROM blocks ORDER BY number ASC LIMIT $1`
			args = []interface{}{limit + 1}
		}
	} else {
		// Decode cursor
		cur, err := decodeCursor(cursor)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid cursor"})
			return
		}

		if direction == "next" {
			query = `SELECT number, hash, parent_hash, miner, gas_limit, gas_used, timestamp, size, tx_count
				FROM blocks WHERE number < $1 ORDER BY number DESC LIMIT $2`
			args = []interface{}{cur.BlockNumber, limit + 1}
		} else {
			query = `SELECT number, hash, parent_hash, miner, gas_limit, gas_used, timestamp, size, tx_count
				FROM blocks WHERE number > $1 ORDER BY number ASC LIMIT $2`
			args = []interface{}{cur.BlockNumber, limit + 1}
		}
	}

	// Execute query
	rows, err := db.Query(context.Background(), query, args...)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var blocks []map[string]interface{}
	for rows.Next() {
		var number, gasLimit, gasUsed, timestamp, size, txCount uint64
		var hash, parentHash, miner string

		err := rows.Scan(&number, &hash, &parentHash, &miner, &gasLimit, &gasUsed, &timestamp, &size, &txCount)
		if err != nil {
			continue
		}

		blocks = append(blocks, map[string]interface{}{
			"number":           number,
			"hash":             hash,
			"parentHash":       parentHash,
			"miner":            miner,
			"gasLimit":         gasLimit,
			"gasUsed":          gasUsed,
			"timestamp":        timestamp,
			"size":             size,
			"transactionCount": txCount,
		})
	}

	// Check if more pages exist
	hasMore := len(blocks) > limit
	if hasMore {
		blocks = blocks[:limit]
	}

	// Build response
	pageInfo := PageInfo{
		HasNextPage:     hasMore,
		HasPreviousPage: cursor != "",
	}

	if len(blocks) > 0 {
		first := blocks[0]
		last := blocks[len(blocks)-1]
		pageInfo.StartCursor = encodeCursor(&Cursor{
			BlockNumber: first["number"].(uint64),
			Hash:        first["hash"].(string),
		})
		pageInfo.EndCursor = encodeCursor(&Cursor{
			BlockNumber: last["number"].(uint64),
			Hash:        last["hash"].(string),
		})
	}

	// Get total count
	var total int64
	db.QueryRow(context.Background(), "SELECT COUNT(*) FROM blocks").Scan(&total)

	c.JSON(http.StatusOK, PaginatedResponse{
		Data:       blocks,
		PageInfo:   pageInfo,
		TotalCount: total,
	})
}

// Get transactions with advanced filtering
func GetTransactions(c *gin.Context) {
	limit := 50
	if l := c.Query("limit"); l != "" {
		if parsed, err := strconv.Atoi(l); err == nil && parsed > 0 && parsed <= 100 {
			limit = parsed
		}
	}

	// Build filters
	filters := []string{"1=1"}
	args := []interface{}{}
	argNum := 1

	// Address filter (from or to)
	if addr := c.Query("address"); addr != "" {
		filters = append(filters, fmt.Sprintf("(from_address = $%d OR to_address = $%d)", argNum, argNum))
		args = append(args, strings.ToLower(addr))
		argNum++
	}

	// Block range
	if from := c.Query("fromBlock"); from != "" {
		if block, err := strconv.ParseUint(from, 10, 64); err == nil {
			filters = append(filters, fmt.Sprintf("block_number >= $%d", argNum))
			args = append(args, block)
			argNum++
		}
	}

	if to := c.Query("toBlock"); to != "" {
		if block, err := strconv.ParseUint(to, 10, 64); err == nil {
			filters = append(filters, fmt.Sprintf("block_number <= $%d", argNum))
			args = append(args, block)
			argNum++
		}
	}

	// Value filter
	if minVal := c.Query("minValue"); minVal != "" {
		if val, ok := new(big.Int).SetString(minVal, 10); ok {
			filters = append(filters, fmt.Sprintf("value >= $%d", argNum))
			args = append(args, val.String())
			argNum++
		}
	}

	// Status filter
	if status := c.Query("status"); status != "" {
		filters = append(filters, fmt.Sprintf("status = $%d", argNum))
		args = append(args, status == "success")
		argNum++
	}

	// Method ID filter
	if methodID := c.Query("methodID"); methodID != "" {
		filters = append(filters, fmt.Sprintf("LEFT(input, 10) = $%d", argNum))
		args = append(args, methodID)
		argNum++
	}

	// Build query
	whereClause := strings.Join(filters, " AND ")
	query := fmt.Sprintf(`
		SELECT hash, block_number, block_hash, timestamp, from_address, to_address, 
		       value, gas_price, gas_used, input, status
		FROM transactions 
		WHERE %s 
		ORDER BY block_number DESC, hash
		LIMIT $%d
	`, whereClause, argNum)
	args = append(args, limit+1)

	rows, err := db.Query(context.Background(), query, args...)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var txs []map[string]interface{}
	for rows.Next() {
		var hash, from, to, input, blockHash string
		var blockNumber, timestamp, gasPrice, gasUsed uint64
		var value *big.Int
		var status bool

		err := rows.Scan(&hash, &blockNumber, &blockHash, &timestamp, &from, &to, &value, &gasPrice, &gasUsed, &input, &status)
		if err != nil {
			continue
		}

		txs = append(txs, map[string]interface{}{
			"hash":        hash,
			"blockNumber": blockNumber,
			"blockHash":   blockHash,
			"timestamp":   timestamp,
			"from":        from,
			"to":          to,
			"value":       value.String(),
			"gasPrice":    gasPrice,
			"gasUsed":     gasUsed,
			"input":       input,
			"status":      status,
		})
	}

	hasMore := len(txs) > limit
	if hasMore {
		txs = txs[:limit]
	}

	c.JSON(http.StatusOK, gin.H{
		"data": txs,
		"pageInfo": gin.H{
			"hasNextPage": hasMore,
		},
	})
}

// Get token transfers
func GetTokenTransfers(c *gin.Context) {
	limit := 50
	if l := c.Query("limit"); l != "" {
		if parsed, err := strconv.Atoi(l); err == nil && parsed > 0 && parsed <= 100 {
			limit = parsed
		}
	}

	filters := []string{"1=1"}
	args := []interface{}{}
	argNum := 1

	// Token address
	if token := c.Query("token"); token != "" {
		filters = append(filters, fmt.Sprintf("token_address = $%d", argNum))
		args = append(args, strings.ToLower(token))
		argNum++
	}

	// Address
	if addr := c.Query("address"); addr != "" {
		filters = append(filters, fmt.Sprintf("(from_address = $%d OR to_address = $%d)", argNum, argNum))
		args = append(args, strings.ToLower(addr))
		argNum++
	}

	whereClause := strings.Join(filters, " AND ")
	query := fmt.Sprintf(`
		SELECT transaction_hash, block_number, timestamp, from_address, to_address, 
		       token_address, value, log_index
		FROM token_transfers 
		WHERE %s 
		ORDER BY block_number DESC, log_index DESC
		LIMIT $%d
	`, whereClause, argNum)
	args = append(args, limit+1)

	rows, err := db.Query(context.Background(), query, args...)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var transfers []map[string]interface{}
	for rows.Next() {
		var txHash, from, to, tokenAddr string
		var blockNumber, timestamp, logIndex uint64
		var value *big.Int

		err := rows.Scan(&txHash, &blockNumber, &timestamp, &from, &to, &tokenAddr, &value, &logIndex)
		if err != nil {
			continue
		}

		transfers = append(transfers, map[string]interface{}{
			"hash":         txHash,
			"blockNumber":  blockNumber,
			"timestamp":    timestamp,
			"from":         from,
			"to":           to,
			"tokenAddress": tokenAddr,
			"value":        value.String(),
			"logIndex":     logIndex,
		})
	}

	hasMore := len(transfers) > limit
	if hasMore {
		transfers = transfers[:limit]
	}

	c.JSON(http.StatusOK, gin.H{
		"data":     transfers,
		"pageInfo": gin.H{"hasNextPage": hasMore},
	})
}

// Get contract ABI
func GetContractABI(c *gin.Context) {
	address := c.Param("address")

	// Check cache first
	cacheKey := "contract:abi:" + address
	if cached, err := redisClient.Get(context.Background(), cacheKey).Result(); err == nil {
		c.JSON(http.StatusOK, gin.H{"abi": cached})
		return
	}

	// Query database
	var abi string
	err := db.QueryRow(context.Background(),
		"SELECT abi FROM contracts WHERE address = $1",
		strings.ToLower(address),
	).Scan(&abi)

	if err == pgx.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{"error": "Contract not found"})
		return
	}

	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	// Cache result
	redisClient.Set(context.Background(), cacheKey, abi, time.Hour)

	c.JSON(http.StatusOK, gin.H{"abi": abi})
}

// Batch query handler
func BatchQuery(c *gin.Context) {
	var requests []map[string]interface{}
	if err := c.ShouldBindJSON(&requests); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	// Limit batch size
	if len(requests) > 10 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Maximum 10 requests per batch"})
		return
	}

	results := make([]interface{}, len(requests))

	for i, req := range requests {
		queryType, _ := req["type"].(string)

		switch queryType {
		case "block":
			if num, ok := req["blockNumber"].(float64); ok {
				results[i] = getBlockByNumber(uint64(num))
			}
		case "transaction":
			if hash, ok := req["hash"].(string); ok {
				results[i] = getTransactionByHash(hash)
			}
		case "address":
			if addr, ok := req["address"].(string); ok {
				results[i] = getAddressInfo(addr)
			}
		}
	}

	c.JSON(http.StatusOK, gin.H{"results": results})
}

// Helper functions
func getBlockByNumber(num uint64) map[string]interface{} {
	var hash, parentHash, miner string
	var gasLimit, gasUsed, timestamp, size, txCount uint64

	err := db.QueryRow(context.Background(), `
		SELECT hash, parent_hash, miner, gas_limit, gas_used, timestamp, size, tx_count
		FROM blocks WHERE number = $1
	`, num).Scan(&hash, &parentHash, &miner, &gasLimit, &gasUsed, &timestamp, &size, &txCount)

	if err != nil {
		return nil
	}

	return map[string]interface{}{
		"number":     num,
		"hash":       hash,
		"parentHash": parentHash,
		"miner":      miner,
		"gasLimit":   gasLimit,
		"gasUsed":    gasUsed,
		"timestamp":  timestamp,
		"size":       size,
		"txCount":    txCount,
	}
}

func getTransactionByHash(hash string) map[string]interface{} {
	var from, to, input, blockHash string
	var blockNumber, timestamp, gasPrice, gasUsed uint64
	var value *big.Int
	var status bool

	err := db.QueryRow(context.Background(), `
		SELECT from_address, to_address, value, gas_price, gas_used, input, status, block_number, block_hash, timestamp
		FROM transactions WHERE hash = $1
	`, hash).Scan(&from, &to, &value, &gasPrice, &gasUsed, &input, &status, &blockNumber, &blockHash, &timestamp)

	if err != nil {
		return nil
	}

	return map[string]interface{}{
		"hash":        hash,
		"from":        from,
		"to":          to,
		"value":       value.String(),
		"gasPrice":    gasPrice,
		"gasUsed":     gasUsed,
		"input":       input,
		"status":      status,
		"blockNumber": blockNumber,
		"blockHash":   blockHash,
		"timestamp":   timestamp,
	}
}

func getAddressInfo(address string) map[string]interface{} {
	var balance *big.Int
	var nonce uint64
	var codeHash string
	var isContract bool

	err := db.QueryRow(context.Background(), `
		SELECT balance, nonce, code_hash, is_contract 
		FROM addresses WHERE address = $1
	`, strings.ToLower(address)).Scan(&balance, &nonce, &codeHash, &isContract)

	if err != nil {
		return nil
	}

	return map[string]interface{}{
		"address":    address,
		"balance":    balance.String(),
		"nonce":      nonce,
		"codeHash":   codeHash,
		"isContract": isContract,
	}
}

// GetBlockByNumber returns a single block together with the transactions it
// contains.
func GetBlockByNumber(c *gin.Context) {
	number, err := strconv.ParseInt(c.Param("number"), 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "block number must be an integer"})
		return
	}

	ctx := context.Background()

	var (
		blockNumber, gasLimit, gasUsed, timestamp, size int64
		txCount                                         int
		hash, parentHash, miner                         string
	)
	err = db.QueryRow(ctx, `
		SELECT number, hash, parent_hash, miner, gas_limit, gas_used, timestamp, size, tx_count
		FROM blocks WHERE number = $1`, number,
	).Scan(&blockNumber, &hash, &parentHash, &miner, &gasLimit, &gasUsed, &timestamp, &size, &txCount)
	if err == pgx.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{"error": "block not found"})
		return
	}
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "query failed"})
		return
	}

	txs, err := fetchTransactions(ctx, "block_number = $1", number, 100)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "query failed"})
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"number":           blockNumber,
		"hash":             hash,
		"parentHash":       parentHash,
		"miner":            miner,
		"gasLimit":         gasLimit,
		"gasUsed":          gasUsed,
		"timestamp":        timestamp,
		"size":             size,
		"transactionCount": txCount,
		"transactions":     txs,
	})
}

// GetTransactionByHash returns a single transaction.
func GetTransactionByHash(c *gin.Context) {
	hash := strings.ToLower(c.Param("hash"))
	if !isHash(hash) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid transaction hash"})
		return
	}

	tx, err := fetchTransactions(context.Background(), "hash = $1", hash, 1)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "query failed"})
		return
	}
	if len(tx) == 0 {
		c.JSON(http.StatusNotFound, gin.H{"error": "transaction not found"})
		return
	}

	c.JSON(http.StatusOK, tx[0])
}

// Search resolves a free-text query against block numbers, hashes, addresses
// and token metadata.
func Search(c *gin.Context) {
	q := strings.TrimSpace(c.Query("q"))
	if q == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "query parameter q is required"})
		return
	}

	limit := 20
	if l := c.Query("limit"); l != "" {
		if parsed, err := strconv.Atoi(l); err == nil && parsed > 0 && parsed <= 100 {
			limit = parsed
		}
	}

	ctx := context.Background()
	results := make([]map[string]interface{}, 0, limit)
	lower := strings.ToLower(q)

	if number, err := strconv.ParseInt(q, 10, 64); err == nil {
		rows, err := db.Query(ctx, `SELECT number, hash FROM blocks WHERE number = $1`, number)
		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": "query failed"})
			return
		}
		defer rows.Close()
		for rows.Next() {
			var n int64
			var h string
			if err := rows.Scan(&n, &h); err != nil {
				continue
			}
			results = append(results, map[string]interface{}{"type": "block", "number": n, "hash": h})
		}
	}

	if isHash(lower) {
		rows, err := db.Query(ctx, `SELECT hash, block_number FROM transactions WHERE hash = $1`, lower)
		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": "query failed"})
			return
		}
		defer rows.Close()
		for rows.Next() {
			var h string
			var bn int64
			if err := rows.Scan(&h, &bn); err != nil {
				continue
			}
			results = append(results, map[string]interface{}{"type": "transaction", "hash": h, "blockNumber": bn})
		}
	}

	if isAddress(lower) {
		rows, err := db.Query(ctx, `
			SELECT address, is_contract FROM accounts WHERE address = $1`, lower)
		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": "query failed"})
			return
		}
		defer rows.Close()
		for rows.Next() {
			var addr string
			var isContract bool
			if err := rows.Scan(&addr, &isContract); err != nil {
				continue
			}
			results = append(results, map[string]interface{}{"type": "address", "address": addr, "isContract": isContract})
		}
	}

	rows, err := db.Query(ctx, `
		SELECT address, name, symbol FROM tokens
		WHERE name ILIKE '%' || $1 || '%' OR symbol ILIKE '%' || $1 || '%'
		LIMIT $2`, q, limit)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "query failed"})
		return
	}
	defer rows.Close()
	for rows.Next() {
		var address, name, symbol string
		if err := rows.Scan(&address, &name, &symbol); err != nil {
			continue
		}
		results = append(results, map[string]interface{}{"type": "token", "address": address, "name": name, "symbol": symbol})
	}

	if len(results) > limit {
		results = results[:limit]
	}

	c.JSON(http.StatusOK, gin.H{"query": q, "count": len(results), "results": results})
}

// GetBalanceHistory returns the indexed native-balance history of an address.
func GetBalanceHistory(c *gin.Context) {
	address := strings.ToLower(c.Query("address"))
	if !isAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "valid address query parameter is required"})
		return
	}

	limit := 100
	if l := c.Query("limit"); l != "" {
		if parsed, err := strconv.Atoi(l); err == nil && parsed > 0 && parsed <= 1000 {
			limit = parsed
		}
	}

	rows, err := db.Query(context.Background(), `
		SELECT sa.block_number, sa.balance, COALESCE(b.timestamp, 0)
		FROM state_accounts sa
		LEFT JOIN blocks b ON b.number = sa.block_number
		WHERE sa.address = $1
		ORDER BY sa.block_number DESC
		LIMIT $2`, address, limit)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "query failed"})
		return
	}
	defer rows.Close()

	type balancePoint struct {
		BlockNumber int64  `json:"blockNumber"`
		Balance     string `json:"balance"`
		Timestamp   int64  `json:"timestamp"`
	}
	points := make([]balancePoint, 0, limit)
	for rows.Next() {
		var p balancePoint
		if err := rows.Scan(&p.BlockNumber, &p.Balance, &p.Timestamp); err != nil {
			continue
		}
		points = append(points, p)
	}

	c.JSON(http.StatusOK, gin.H{"address": address, "history": points})
}

// GetNonceHistory returns the indexed nonce history of an address, which is
// what transaction-count charts are built from.
func GetNonceHistory(c *gin.Context) {
	address := strings.ToLower(c.Query("address"))
	if !isAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "valid address query parameter is required"})
		return
	}

	limit := 100
	if l := c.Query("limit"); l != "" {
		if parsed, err := strconv.Atoi(l); err == nil && parsed > 0 && parsed <= 1000 {
			limit = parsed
		}
	}

	rows, err := db.Query(context.Background(), `
		SELECT sa.block_number, COALESCE(sa.nonce, 0), COALESCE(b.timestamp, 0)
		FROM state_accounts sa
		LEFT JOIN blocks b ON b.number = sa.block_number
		WHERE sa.address = $1
		ORDER BY sa.block_number DESC
		LIMIT $2`, address, limit)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "query failed"})
		return
	}
	defer rows.Close()

	type noncePoint struct {
		BlockNumber int64 `json:"blockNumber"`
		Nonce       int64 `json:"nonce"`
		Timestamp   int64 `json:"timestamp"`
	}
	points := make([]noncePoint, 0, limit)
	for rows.Next() {
		var p noncePoint
		if err := rows.Scan(&p.BlockNumber, &p.Nonce, &p.Timestamp); err != nil {
			continue
		}
		points = append(points, p)
	}

	c.JSON(http.StatusOK, gin.H{"address": address, "history": points})
}

// fetchTransactions loads transactions matching a caller-controlled WHERE
// clause whose single placeholder is the supplied argument.
func fetchTransactions(ctx context.Context, where string, arg interface{}, limit int) ([]map[string]interface{}, error) {
	rows, err := db.Query(ctx, `
		SELECT hash, block_number, transaction_index, from_address, COALESCE(to_address, ''),
		       value, gas_price, COALESCE(gas_used, 0), status, COALESCE(contract_address, '')
		FROM transactions WHERE `+where+`
		ORDER BY block_number DESC, transaction_index ASC
		LIMIT `+strconv.Itoa(limit), arg)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make([]map[string]interface{}, 0, limit)
	for rows.Next() {
		var (
			hash, from, to, status, contractAddress string
			blockNumber, value, gasPrice, gasUsed   int64
			txIndex                                 int
		)
		if err := rows.Scan(&hash, &blockNumber, &txIndex, &from, &to, &value, &gasPrice, &gasUsed, &status, &contractAddress); err != nil {
			continue
		}
		out = append(out, map[string]interface{}{
			"hash":             hash,
			"blockNumber":      blockNumber,
			"transactionIndex": txIndex,
			"from":             from,
			"to":               to,
			"value":            value,
			"gasPrice":         gasPrice,
			"gasUsed":          gasUsed,
			"status":           status,
			"contractAddress":  contractAddress,
		})
	}
	return out, nil
}

// isHash reports whether s is a 0x-prefixed 32-byte hex string.
func isHash(s string) bool {
	if len(s) != 66 || !strings.HasPrefix(s, "0x") {
		return false
	}
	_, err := hex.DecodeString(s[2:])
	return err == nil
}

// isAddress reports whether s is a 0x-prefixed 20-byte hex string.
func isAddress(s string) bool {
	if len(s) != 42 || !strings.HasPrefix(s, "0x") {
		return false
	}
	_, err := hex.DecodeString(s[2:])
	return err == nil
}

// ============================================
// Rate Limiting Middleware
// ============================================

func RateLimiter() gin.HandlerFunc {
	return func(c *gin.Context) {
		if !rateLimiter.Allow() {
			c.JSON(http.StatusTooManyRequests, gin.H{
				"error":      "Rate limit exceeded",
				"retryAfter": 60,
			})
			c.Abort()
			return
		}
		c.Next()
	}
}

// ============================================
// JWT Authentication Middleware
// ============================================

func JWTAuth() gin.HandlerFunc {
	return func(c *gin.Context) {
		token := c.GetHeader("Authorization")
		if token == "" {
			c.JSON(http.StatusUnauthorized, gin.H{"error": "Authorization header required"})
			c.Abort()
			return
		}

		// Remove "Bearer " prefix
		token = strings.TrimPrefix(token, "Bearer ")

		claims, err := validateJWT(token)
		if err != nil {
			c.JSON(http.StatusUnauthorized, gin.H{"error": "invalid token"})
			c.Abort()
			return
		}

		if sub, err := claims.Claims.GetSubject(); err == nil {
			c.Set("subject", sub)
		}

		c.Next()
	}
}

// validateJWT verifies the HMAC-SHA256 signature and the standard claims of a
// bearer token using the configured secret.
func validateJWT(tokenString string) (*jwt.Token, error) {
	if cfg == nil || cfg.JWTSecret == "" {
		return nil, fmt.Errorf("JWT secret is not configured")
	}

	token, err := jwt.Parse(tokenString, func(t *jwt.Token) (interface{}, error) {
		if _, ok := t.Method.(*jwt.SigningMethodHMAC); !ok {
			return nil, fmt.Errorf("unexpected signing method %v", t.Header["alg"])
		}
		return []byte(cfg.JWTSecret), nil
	}, jwt.WithValidMethods([]string{"HS256", "HS384", "HS512"}))
	if err != nil {
		return nil, err
	}
	if !token.Valid {
		return nil, fmt.Errorf("token is not valid")
	}
	return token, nil
}

// ============================================
// Main
// ============================================

func main() {
	// Initialize configuration
	cfg = &Config{
		Port:           getEnv("PORT", "8080"),
		DatabaseURL:    getEnv("DATABASE_URL", "postgres://localhost:5432/tigersmartchain"),
		RedisURL:       getEnv("REDIS_URL", "redis://localhost:6379"),
		JWTSecret:      getEnv("JWT_SECRET", "secret"),
		RateLimitRPS:   100,
		RateLimitBurst: 200,
		CacheTTL:       time.Minute * 5,
	}

	// Initialize database
	var err error
	db, err = pgx.Connect(context.Background(), cfg.DatabaseURL)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Database connection failed: %v\n", err)
		os.Exit(1)
	}
	defer db.Close(context.Background())

	// Initialize Redis
	redisClient = redis.NewClient(&redis.Options{
		Addr:     strings.TrimPrefix(cfg.RedisURL, "redis://"),
		Password: "",
		DB:       0,
	})

	// Initialize rate limiter
	rateLimiter = rate.NewLimiter(rate.Limit(cfg.RateLimitRPS), cfg.RateLimitBurst)

	// Setup Gin
	gin.SetMode(gin.ReleaseMode)
	r := gin.New()
	r.Use(gin.Recovery())
	r.Use(gin.Logger())

	// Public routes
	public := r.Group("/api/v1")
	public.Use(RateLimiter())
	{
		// Blocks
		public.GET("/blocks", GetBlocks)
		public.GET("/blocks/:number", GetBlockByNumber)

		// Transactions
		public.GET("/transactions", GetTransactions)
		public.GET("/transactions/:hash", GetTransactionByHash)

		// Token transfers
		public.GET("/token/transfers", GetTokenTransfers)

		// Contracts
		public.GET("/contracts/:address", GetContractABI)

		// Search
		public.GET("/search", Search)
	}

	// Authenticated routes
	authenticated := r.Group("/api/v1")
	authenticated.Use(RateLimiter())
	authenticated.Use(JWTAuth())
	{
		// Batch queries
		authenticated.POST("/batch", BatchQuery)

		// Historical data
		authenticated.GET("/history/balance", GetBalanceHistory)
		authenticated.GET("/history/nonce", GetNonceHistory)
	}

	// Health check
	r.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{
			"status":    "healthy",
			"timestamp": time.Now().Unix(),
		})
	})

	fmt.Printf("Starting API server on port %s\n", cfg.Port)
	r.Run(":" + cfg.Port)
}

func getEnv(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}
