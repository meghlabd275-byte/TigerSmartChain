package gateway

// This file contains real database-backed implementations for all
// endpoints that were previously returning mock data via getMockData().
// Each handler queries PostgreSQL (and RPC where appropriate) and returns
// real data. Empty results return [] not null.

import (
	"net/http"
	"strconv"

	"github.com/gin-gonic/gin"
)

// ==================== BLOCK ENDPOINTS ====================

func (h *Handler) GetBlockUncles(c *gin.Context) {
	h.listByBlockNumber(c, `SELECT * FROM uncles WHERE block_number = $1 ORDER BY id DESC LIMIT $2`)
}

func (h *Handler) GetBlockLogs(c *gin.Context) {
	h.listByBlockNumber(c, `SELECT * FROM logs WHERE block_number = $1 ORDER BY log_index LIMIT $2`)
}

func (h *Handler) GetBlockStateDiff(c *gin.Context) {
	h.listByBlockNumber(c, `SELECT * FROM state_diffs WHERE block_number = $1 ORDER BY id LIMIT $2`)
}

func (h *Handler) GetBlocksByRange(c *gin.Context) {
	startStr := c.Query("start")
	endStr := c.Query("end")
	start, err := strconv.ParseInt(startStr, 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid start block"})
		return
	}
	end, err := strconv.ParseInt(endStr, 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid end block"})
		return
	}
	limit := paramInt(c, "limit", 100)
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, `SELECT * FROM blocks WHERE number >= $1 AND number <= $2 ORDER BY number DESC LIMIT $3`, start, end, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetBlockValidators(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `SELECT * FROM validators ORDER BY total_staked DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetBlockRewards(c *gin.Context) {
	numStr := c.Param("number")
	num, err := strconv.ParseInt(numStr, 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid block number"})
		return
	}
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, `SELECT * FROM block_rewards WHERE block_number = $1 LIMIT 1`, num)
	if err != nil {
		dbError(c, err)
		return
	}
	if len(rows) == 0 {
		c.JSON(http.StatusNotFound, gin.H{"error": "no rewards found"})
		return
	}
	c.JSON(http.StatusOK, rows[0])
}

// ==================== TRANSACTION ENDPOINTS ====================

func (h *Handler) GetTransactionReceipt(c *gin.Context) {
	hash := c.Param("hash")
	if hash == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing hash"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT hash, block_number, block_hash, transaction_index, from_address, to_address, status, gas_used, cumulative_gas_used, effective_gas_price, contract_address, logs_bloom FROM transactions WHERE hash = $1`, hash)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

func (h *Handler) GetTransactionLogs(c *gin.Context) {
	h.listByTxHash(c, `SELECT * FROM logs WHERE transaction_hash = $1 ORDER BY log_index LIMIT $2`)
}

func (h *Handler) GetTransactionsFromAddress(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT * FROM transactions WHERE from_address = $1 ORDER BY block_number DESC LIMIT $2`)
}

func (h *Handler) GetTransactionsToAddress(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT * FROM transactions WHERE to_address = $1 ORDER BY block_number DESC LIMIT $2`)
}

func (h *Handler) GetTransactionsByAddress(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT * FROM transactions WHERE from_address = $1 OR to_address = $1 ORDER BY block_number DESC LIMIT $2`)
}

func (h *Handler) GetLatestTransactions(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT * FROM transactions ORDER BY block_number DESC, transaction_index DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetTokenTransfersByTx(c *gin.Context) {
	h.listByTxHash(c, `SELECT * FROM token_transfers WHERE transaction_hash = $1 ORDER BY id LIMIT $2`)
}

func (h *Handler) GetTransactionsBatch(c *gin.Context) {
	hashes := c.QueryArray("hashes")
	if len(hashes) == 0 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "no hashes provided"})
		return
	}
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, `SELECT * FROM transactions WHERE hash = ANY($1)`, hashes)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetExecutionResult(c *gin.Context) {
	hash := c.Param("hash")
	if hash == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing hash"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT hash, status, gas_used, effective_gas_price, contract_address FROM transactions WHERE hash = $1`, hash)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

// ==================== INTERNAL TRANSACTION ENDPOINTS ====================

func (h *Handler) GetInternalTxsFrom(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT * FROM internal_transactions WHERE from_address = $1 ORDER BY block_number DESC LIMIT $2`)
}

func (h *Handler) GetInternalTxsTo(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT * FROM internal_transactions WHERE to_address = $1 ORDER BY block_number DESC LIMIT $2`)
}

func (h *Handler) GetInternalTxsByAddress(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT * FROM internal_transactions WHERE from_address = $1 OR to_address = $1 ORDER BY block_number DESC LIMIT $2`)
}

func (h *Handler) GetRecentInternalTxs(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT * FROM internal_transactions ORDER BY block_number DESC, id DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetInternalTxsByBlock(c *gin.Context) {
	h.listByBlockNumber(c, `SELECT * FROM internal_transactions WHERE block_number = $1 ORDER BY trace_index LIMIT $2`)
}

func (h *Handler) GetCallTree(c *gin.Context) {
	h.listByTxHash(c, `SELECT * FROM traces WHERE transaction_hash = $1 ORDER BY depth, trace_index LIMIT $2`)
}

// ==================== TRACE ENDPOINTS ====================

func (h *Handler) GetTraceStateDiff(c *gin.Context) {
	h.listByTxHash(c, `SELECT * FROM state_diffs WHERE transaction_hash = $1 ORDER BY id LIMIT $2`)
}

func (h *Handler) GetTraceStorage(c *gin.Context) {
	h.listByTxHash(c, `SELECT address, slot, value FROM traces WHERE transaction_hash = $1 AND call_type = 'STATICCALL' LIMIT $2`)
}

func (h *Handler) GetTraceCallList(c *gin.Context) {
	h.listByTxHash(c, `SELECT id, from_address, to_address, call_type, value, gas, depth FROM traces WHERE transaction_hash = $1 ORDER BY depth LIMIT $2`)
}

func (h *Handler) GetVMTrace(c *gin.Context) {
	hash := c.Param("hash")
	if hash == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing hash"})
		return
	}
	ctx := c.Request.Context()
	raw, err := h.rpcCall(ctx, "debug_traceTransaction", []interface{}{hash})
	if err != nil {
		c.JSON(http.StatusOK, gin.H{"trace": nil, "note": "debug RPC unavailable"})
		return
	}
	c.Data(http.StatusOK, "application/json", raw)
}

func (h *Handler) GetTracesByBlock(c *gin.Context) {
	h.listByBlockNumber(c, `SELECT * FROM traces WHERE block_number = $1 ORDER BY transaction_index, trace_index LIMIT $2`)
}

func (h *Handler) ReplayTransaction(c *gin.Context) {
	hash := c.Param("hash")
	if hash == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing hash"})
		return
	}
	ctx := c.Request.Context()
	raw, err := h.rpcCall(ctx, "trace_replayTransaction", []interface{}{hash, []string{"trace", "stateDiff"}})
	if err != nil {
		c.JSON(http.StatusOK, gin.H{"result": nil, "note": "trace RPC unavailable"})
		return
	}
	c.Data(http.StatusOK, "application/json", raw)
}

func (h *Handler) GetTraceOps(c *gin.Context) {
	h.listByTxHash(c, `SELECT id, call_type, from_address, to_address, value, gas, depth, input FROM traces WHERE transaction_hash = $1 LIMIT $2`)
}

// ==================== TOKEN ENDPOINTS ====================

func (h *Handler) GetTokenMetadata(c *gin.Context) {
	token := c.Param("address")
	if token == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT address, name, symbol, decimals, total_supply, logo_uri, website FROM tokens WHERE address = $1`, token)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

func (h *Handler) GetTokenApprovals(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT * FROM token_approvals WHERE token_address = $1 ORDER BY id DESC LIMIT $2`)
}

func (h *Handler) GetTokenAllowances(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT * FROM token_approvals WHERE token_address = $1 AND amount > 0 ORDER BY id DESC LIMIT $2`)
}

func (h *Handler) GetTopTokenHolders(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT address, balance FROM token_holders WHERE token_address = $1 ORDER BY balance DESC LIMIT $2`)
}

func (h *Handler) GetTokenDexPairs(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT * FROM dex_pairs WHERE token0_address = $1 OR token1_address = $1 LIMIT $2`)
}

func (h *Handler) GetHolderHistory(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT * FROM token_holder_history WHERE token_address = $1 ORDER BY block_number DESC LIMIT $2`)
}

func (h *Handler) GetTokenAnalytics(c *gin.Context) {
	token := c.Param("address")
	if token == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `
		SELECT
			COUNT(DISTINCT th.address) AS holder_count,
			COALESCE(SUM(tt.value), 0) AS transfer_volume,
			COUNT(tt.id) AS transfer_count
		FROM tokens t
		LEFT JOIN token_holders th ON th.token_address = t.address
		LEFT JOIN token_transfers tt ON tt.token_address = t.address
		WHERE t.address = $1
	`, token)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

func (h *Handler) GetTokenFlippening(c *gin.Context) {
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, `SELECT address, name, symbol, market_cap_usd FROM tokens WHERE market_cap_usd IS NOT NULL ORDER BY market_cap_usd DESC LIMIT 10`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"ranking": rows})
}

func (h *Handler) GetTrendingTokens(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT address, name, symbol, price_usd, volume_24h FROM tokens WHERE volume_24h IS NOT NULL ORDER BY volume_24h DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetNewTokens(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT address, name, symbol, created_at FROM tokens ORDER BY created_at DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) SearchTokens(c *gin.Context) {
	q := c.Query("q")
	if q == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing query"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 10)
	rows, err := h.queryRows(ctx, `SELECT address, name, symbol FROM tokens WHERE name ILIKE '%' || $1 || '%' OR symbol ILIKE '%' || $1 || '%' LIMIT $2`, q, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// ==================== NFT ENDPOINTS ====================

func (h *Handler) GetNFTMetadata(c *gin.Context) {
	collection := c.Param("address")
	if collection == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT address, name, symbol, total_supply, logo_uri, website, discord, twitter FROM nft_collections WHERE address = $1`, collection)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

func (h *Handler) GetNFTTokens(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT collection_address, token_id, owner, uri FROM nfts WHERE collection_address = $1 ORDER BY token_id LIMIT $2`)
}

func (h *Handler) GetNFTTokenTransfers(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT * FROM nft_transfers WHERE collection_address = $1 ORDER BY block_number DESC LIMIT $2`)
}

func (h *Handler) GetNFTVolumeHistory(c *gin.Context) {
	ctx := c.Request.Context()
	collection := c.Param("address")
	limit := paramInt(c, "limit", 30)
	var rows []map[string]interface{}
	var err error
	if collection != "" {
		rows, err = h.queryRows(ctx, `SELECT * FROM analytics_daily WHERE entity_type = 'nft' AND entity_address = $1 ORDER BY date DESC LIMIT $2`, collection, limit)
	} else {
		rows, err = h.queryRows(ctx, `SELECT * FROM analytics_daily WHERE entity_type = 'nft' ORDER BY date DESC LIMIT $1`, limit)
	}
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetNFTHolders(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT owner, COUNT(*) AS token_count FROM nfts WHERE collection_address = $1 GROUP BY owner ORDER BY token_count DESC LIMIT $2`)
}

func (h *Handler) GetNFTRankings(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 100)
	rows, err := h.queryRows(ctx, `SELECT address, name, total_supply, volume_24h FROM nft_collections ORDER BY volume_24h DESC NULLS LAST LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetNFTRarity(c *gin.Context) {
	collection := c.Param("address")
	tokenID := c.Param("token_id")
	if collection == "" || tokenID == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address or token_id"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT * FROM nft_rarity WHERE collection_address = $1 AND token_id = $2`, collection, tokenID)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

func (h *Handler) GetNFTAnalytics(c *gin.Context) {
	collection := c.Param("address")
	if collection == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `
		SELECT
			COUNT(DISTINCT owner) AS holder_count,
			COUNT(*) AS token_count,
			COALESCE(volume_24h, 0) AS volume_24h
		FROM nft_collections nc
		LEFT JOIN nfts n ON n.collection_address = nc.address
		WHERE nc.address = $1
		GROUP BY nc.address, nc.volume_24h
	`, collection)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

func (h *Handler) SearchNFTs(c *gin.Context) {
	q := c.Query("q")
	if q == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing query"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT address, name, symbol FROM nft_collections WHERE name ILIKE '%' || $1 || '%' OR symbol ILIKE '%' || $1 || '%' LIMIT $2`, q, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetTrendingNFTs(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT address, name, symbol, volume_24h FROM nft_collections WHERE volume_24h IS NOT NULL ORDER BY volume_24h DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// ==================== CONTRACT ENDPOINTS ====================

func (h *Handler) GetContractSource(c *gin.Context) {
	addr := c.Param("address")
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT address, source_code, compiler_version, abi, contract_name FROM verified_sources WHERE address = $1`, addr)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

func (h *Handler) GetVerifiedContract(c *gin.Context) {
	addr := c.Param("address")
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT address, contract_name, compiler_version, verified_at, license_type FROM verified_sources WHERE address = $1`, addr)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

// ==================== ADDRESS ENDPOINTS ====================

func (h *Handler) GetAddressTransactions(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT * FROM transactions WHERE from_address = $1 OR to_address = $1 ORDER BY block_number DESC LIMIT $2`)
}

func (h *Handler) GetAddressInternalTxs(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT * FROM internal_transactions WHERE from_address = $1 OR to_address = $1 ORDER BY block_number DESC LIMIT $2`)
}

func (h *Handler) GetAddressBlocksMined(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT number, hash, timestamp, gas_used, gas_limit FROM blocks WHERE miner = $1 ORDER BY number DESC LIMIT $2`)
}

func (h *Handler) GetAddressAnnotations(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT label, category, description FROM search_index WHERE address = $1 LIMIT $2`)
}

func (h *Handler) GetAllTokenBalances(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT token_address, balance FROM token_holders WHERE address = $1 ORDER BY balance DESC LIMIT $2`)
}

func (h *Handler) GetAllNFTBalances(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT collection_address, COUNT(*) AS token_count FROM nfts WHERE owner = $1 GROUP BY collection_address LIMIT $2`)
}

func (h *Handler) GetAddressAnalytics(c *gin.Context) {
	addr := c.Param("address")
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `
		SELECT
			COUNT(*) FILTER (WHERE from_address = $1) AS txs_sent,
			COUNT(*) FILTER (WHERE to_address = $1) AS txs_received,
			COALESCE(SUM(value) FILTER (WHERE from_address = $1), 0) AS total_sent,
			COALESCE(SUM(value) FILTER (WHERE to_address = $1), 0) AS total_received
		FROM transactions
		WHERE from_address = $1 OR to_address = $1
	`, addr)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

// ==================== GAS ENDPOINTS ====================

func (h *Handler) GetGasTrends(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 24)
	rows, err := h.queryRows(ctx, `SELECT block_number, gas_price, base_fee_per_gas, timestamp FROM gas_prices ORDER BY block_number DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetGasAggregator(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `
		SELECT
			MIN(gas_price) AS min_gas_price,
			MAX(gas_price) AS max_gas_price,
			AVG(gas_price) AS avg_gas_price,
			PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY gas_price) AS median_gas_price
		FROM gas_prices
	`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

// ==================== CHART ENDPOINTS ====================

func (h *Handler) dailyChart(c *gin.Context, metric string, days int) {
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, `
		SELECT date, $1::text AS metric, value FROM analytics_daily
		WHERE metric = $1 AND date >= NOW() - ($2 || ' days')::INTERVAL
		ORDER BY date
	`, metric, strconv.Itoa(days))
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetDailyTxChart(c *gin.Context)        { h.dailyChart(c, "daily_tx", 30) }
func (h *Handler) GetWeeklyTxChart(c *gin.Context)      { h.dailyChart(c, "weekly_tx", 90) }
func (h *Handler) GetMonthlyTxChart(c *gin.Context)      { h.dailyChart(c, "monthly_tx", 365) }
func (h *Handler) GetNewAddressesChart(c *gin.Context)   { h.dailyChart(c, "new_addresses", 30) }
func (h *Handler) GetActiveAddressesChart(c *gin.Context) { h.dailyChart(c, "active_addresses", 30) }
func (h *Handler) GetTokenChart(c *gin.Context)          { h.dailyChart(c, "token_price", 30) }
func (h *Handler) GetTokenVolumeChart(c *gin.Context)    { h.dailyChart(c, "token_volume", 30) }
func (h *Handler) GetNFTChart(c *gin.Context)            { h.dailyChart(c, "nft_floor", 30) }
func (h *Handler) GetNFTVolumeChart(c *gin.Context)      { h.dailyChart(c, "nft_volume", 30) }
func (h *Handler) GetGasChart(c *gin.Context)             { h.dailyChart(c, "gas_used", 30) }
func (h *Handler) GetGasPriceChart(c *gin.Context)       { h.dailyChart(c, "gas_price", 30) }
func (h *Handler) GetNetworkChart(c *gin.Context)        { h.dailyChart(c, "network_usage", 30) }
func (h *Handler) GetTPSChart(c *gin.Context)             { h.dailyChart(c, "tps", 30) }
func (h *Handler) GetDifficultyChart(c *gin.Context)      { h.dailyChart(c, "difficulty", 30) }
func (h *Handler) GetDexLiquidityChart(c *gin.Context)   { h.dailyChart(c, "dex_liquidity", 30) }
func (h *Handler) GetDexVolumeChart(c *gin.Context)      { h.dailyChart(c, "dex_volume", 30) }

func (h *Handler) GetCustomChart(c *gin.Context) {
	metric := c.Query("metric")
	if metric == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing metric parameter"})
		return
	}
	days := paramInt(c, "days", 30)
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, `SELECT date, value FROM analytics_daily WHERE metric = $1 AND date >= NOW() - ($2 || ' days')::INTERVAL ORDER BY date`, metric, strconv.Itoa(days))
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// ==================== DEX ENDPOINTS ====================

func (h *Handler) GetDexPairTokens(c *gin.Context) {
	pair := c.Param("address")
	if pair == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT token0_address, token1_address, token0_symbol, token1_symbol FROM dex_pairs WHERE address = $1`, pair)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

func (h *Handler) GetDexTransactions(c *gin.Context) {
	h.listByAddress(c, "address", `SELECT * FROM dex_swaps WHERE pair_address = $1 ORDER BY block_number DESC LIMIT $2`)
}

func (h *Handler) GetDexOHLCV(c *gin.Context) {
	ctx := c.Request.Context()
	pair := c.Param("address")
	limit := paramInt(c, "limit", 100)
	var rows []map[string]interface{}
	var err error
	if pair != "" {
		rows, err = h.queryRows(ctx, `SELECT * FROM token_prices WHERE token_address = $1 ORDER BY timestamp DESC LIMIT $2`, pair, limit)
	} else {
		rows, err = h.queryRows(ctx, `SELECT * FROM token_prices ORDER BY timestamp DESC LIMIT $1`, limit)
	}
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) SearchDexPairs(c *gin.Context) {
	q := c.Query("q")
	if q == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing query"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 10)
	rows, err := h.queryRows(ctx, `SELECT * FROM dex_pairs WHERE token0_symbol ILIKE '%' || $1 || '%' OR token1_symbol ILIKE '%' || $1 || '%' LIMIT $2`, q, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetPopularDexTokens(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT token0_address, token0_symbol, COUNT(*) AS swap_count FROM dex_swaps GROUP BY token0_address, token0_symbol ORDER BY swap_count DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetDexExchanges(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 10)
	rows, err := h.queryRows(ctx, `SELECT DISTINCT factory_address FROM dex_pairs LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetDexProtocols(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 5)
	rows, err := h.queryRows(ctx, `SELECT protocol_name, COALESCE(SUM(tvl_usd), 0) AS tvl FROM defi_tvl GROUP BY protocol_name ORDER BY tvl DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// ==================== GOVERNANCE ENDPOINTS ====================

func (h *Handler) GetProposalVotes(c *gin.Context) {
	proposalID := c.Param("id")
	if proposalID == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing proposal id"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 100)
	rows, err := h.queryRows(ctx, `SELECT * FROM governance_votes WHERE proposal_id = $1 ORDER BY id LIMIT $2`, proposalID, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetProposalTally(c *gin.Context) {
	proposalID := c.Param("id")
	if proposalID == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing proposal id"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `
		SELECT
			COUNT(*) FILTER (WHERE vote_choice = true) AS for_votes,
			COUNT(*) FILTER (WHERE vote_choice = false) AS against_votes,
			COUNT(*) FILTER (WHERE vote_choice IS NULL) AS abstain_votes
		FROM governance_votes WHERE proposal_id = $1
	`, proposalID)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

func (h *Handler) GetGovernanceVoters(c *gin.Context) {
	proposalID := c.Param("id")
	if proposalID == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing proposal id"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `SELECT voter, vote_choice, voting_power FROM governance_votes WHERE proposal_id = $1 ORDER BY voting_power DESC LIMIT $2`, proposalID, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetDelegations(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT delegator, delegatee, voting_power FROM governance_votes WHERE delegator IS NOT NULL ORDER BY voting_power DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetDelegatorInfo(c *gin.Context) {
	addr := c.Param("address")
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT delegator, delegatee, voting_power FROM governance_votes WHERE delegator = $1 ORDER BY voting_power DESC LIMIT 1`, addr)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

// ==================== MEV ENDPOINTS ====================

func (h *Handler) GetMEVBundle(c *gin.Context) {
	bundleID := c.Param("id")
	if bundleID == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing id"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT * FROM mev_bundles WHERE id = $1`, bundleID)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

func (h *Handler) GetFlashbotsBundles(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT * FROM mev_bundles ORDER BY block_number DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetMEVRelays(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 5)
	rows, err := h.queryRows(ctx, `SELECT DISTINCT relay_url FROM mev_bundles WHERE relay_url IS NOT NULL LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetMEVActivities(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `SELECT * FROM mev_bundles ORDER BY block_number DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetSandwichAttacks(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT * FROM mev_bundles WHERE mev_type = 'sandwich' ORDER BY block_number DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetArbitrageOpportunities(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 10)
	rows, err := h.queryRows(ctx, `SELECT * FROM mev_bundles WHERE mev_type = 'arbitrage' ORDER BY block_number DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetMEVJobs(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT * FROM mev_bundles WHERE mev_type = 'backrun' ORDER BY block_number DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// ==================== LABEL ENDPOINTS ====================

func (h *Handler) GetLabelCategories(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 10)
	rows, err := h.queryRows(ctx, `SELECT DISTINCT category FROM search_index WHERE category IS NOT NULL LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) GetAddressesByLabel(c *gin.Context) {
	category := c.Param("category")
	if category == "" {
		category = c.Query("category")
	}
	if category == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing category"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `SELECT address, label, description FROM search_index WHERE category = $1 LIMIT $2`, category, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// ==================== STATS ENDPOINTS ====================

func (h *Handler) GetBlockStats(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT COUNT(*) AS total_blocks, MAX(number) AS latest_block, AVG(gas_used) AS avg_gas_used FROM blocks`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

func (h *Handler) GetTransactionStats(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT COUNT(*) AS total_txs, COUNT(*) FILTER (WHERE status = 1) AS successful_txs, COUNT(*) FILTER (WHERE status = 0) AS failed_txs, AVG(gas_used) AS avg_gas FROM transactions`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

func (h *Handler) GetAccountStats(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT COUNT(DISTINCT from_address) AS unique_senders, COUNT(DISTINCT to_address) AS unique_recipients FROM transactions`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

func (h *Handler) GetContractStats(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT COUNT(*) AS total_contracts, COUNT(*) FILTER (WHERE is_verified = true) AS verified_contracts FROM contracts`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

func (h *Handler) GetTokenStats(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT COUNT(*) AS total_tokens, COUNT(*) FILTER (WHERE price_usd IS NOT NULL) AS priced_tokens FROM tokens`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

func (h *Handler) GetNFTStats(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT COUNT(*) AS total_collections, COUNT(*) FILTER (WHERE volume_24h IS NOT NULL) AS active_collections FROM nft_collections`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

func (h *Handler) GetDexStats(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT COUNT(*) AS total_pairs, COALESCE(SUM(liquidity_usd), 0) AS total_liquidity, COALESCE(SUM(volume_24h), 0) AS total_volume FROM dex_pairs`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

func (h *Handler) GetStatsOverview(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `
		SELECT
			(SELECT COUNT(*) FROM blocks) AS total_blocks,
			(SELECT COUNT(*) FROM transactions) AS total_transactions,
			(SELECT COUNT(*) FROM tokens) AS total_tokens,
			(SELECT COUNT(*) FROM nft_collections) AS total_nfts,
			(SELECT COUNT(*) FROM contracts) AS total_contracts,
			(SELECT MAX(number) FROM blocks) AS latest_block
	`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

func (h *Handler) GetHistoricalStats(c *gin.Context) {
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, `SELECT date, metric, value FROM analytics_daily WHERE date >= NOW() - INTERVAL '365 days' ORDER BY date`)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// ==================== SEARCH ENDPOINTS ====================

func (h *Handler) SearchTokensAdvanced(c *gin.Context) {
	q := c.Query("q")
	if q == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing query"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT address, name, symbol, total_supply, price_usd FROM tokens WHERE name ILIKE '%' || $1 || '%' OR symbol ILIKE '%' || $1 || '%' LIMIT $2`, q, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) SearchAddresses(c *gin.Context) {
	q := c.Query("q")
	if q == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing query"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT address, label, category FROM search_index WHERE address ILIKE '%' || $1 || '%' OR label ILIKE '%' || $1 || '%' LIMIT $2`, q, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) SearchTransactions(c *gin.Context) {
	q := c.Query("q")
	if q == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing query"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT * FROM transactions WHERE hash ILIKE '%' || $1 || '%' OR from_address ILIKE '%' || $1 || '%' OR to_address ILIKE '%' || $1 || '%' LIMIT $2`, q, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

func (h *Handler) SearchBlocks(c *gin.Context) {
	q := c.Query("q")
	if q == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing query"})
		return
	}
	num, err := strconv.ParseInt(q, 10, 64)
	if err != nil {
		ctx := c.Request.Context()
		limit := paramInt(c, "limit", 20)
		rows, qerr := h.queryRows(ctx, `SELECT * FROM blocks WHERE hash ILIKE '%' || $1 || '%' LIMIT $2`, q, limit)
		if qerr != nil {
			dbError(c, qerr)
			return
		}
		respondList(c, rows, len(rows))
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 20)
	rows, err := h.queryRows(ctx, `SELECT * FROM blocks WHERE number = $1 LIMIT $2`, num, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}
