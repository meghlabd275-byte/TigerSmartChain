package gateway

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
)

// GetInternalTransactionList returns paginated internal transactions from the
// database, optionally filtered by address.
func (h *Handler) GetInternalTransactionList(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 25)
	offset := paramOffset(c)
	address := c.Query("address")

	var rows []map[string]interface{}
	var err error
	if address != "" {
		rows, err = h.queryRows(ctx, `SELECT id, transaction_hash, block_number, transaction_index, depth, call_type, from_address, to_address, value, gas, revert FROM internal_transactions WHERE from_address = $1 OR to_address = $1 ORDER BY block_number DESC, transaction_index DESC LIMIT $2 OFFSET $3`, address, limit, offset)
	} else {
		rows, err = h.queryRows(ctx, `SELECT id, transaction_hash, block_number, transaction_index, depth, call_type, from_address, to_address, value, gas, revert FROM internal_transactions ORDER BY block_number DESC, transaction_index DESC LIMIT $1 OFFSET $2`, limit, offset)
	}
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM internal_transactions`)
	respondList(c, rows, int(total))
}

// GetTrace returns trace data for a transaction, sourced from the traces
// table with Redis caching.
func (h *Handler) GetTrace(c *gin.Context) {
	txHash := c.Param("hash")
	ctx := c.Request.Context()
	cacheKey := "trace:" + txHash

	if cached, err := h.redis.Get(ctx, cacheKey).Result(); err == nil {
		var trace map[string]interface{}
		if json.Unmarshal([]byte(cached), &trace) == nil {
			c.JSON(http.StatusOK, trace)
			return
		}
	}

	rows, err := h.queryRows(ctx, `SELECT id, transaction_hash, block_number, transaction_index, from_address, to_address, call_type, value, gas, input, output, revert, error, depth FROM traces WHERE transaction_hash = $1 ORDER BY id`, txHash)
	if err != nil {
		dbError(c, err)
		return
	}
	trace := map[string]interface{}{
		"transactionHash": txHash,
		"failed":          false,
		"returnValue":     "0x",
		"structLogs":      rows,
	}
	if len(rows) == 0 {
		trace["structLogs"] = []map[string]interface{}{}
	}
	if data, err := json.Marshal(trace); err == nil {
		h.redis.Set(ctx, cacheKey, data, 60*time.Second)
	}
	c.JSON(http.StatusOK, trace)
}

// GetStateDiff returns state diff for a transaction from the state_diffs table.
func (h *Handler) GetStateDiff(c *gin.Context) {
	txHash := c.Param("hash")
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, `SELECT id, block_number, address, storage_key, storage_value, old_value, new_value, diff_type FROM state_diffs WHERE transaction_hash = $1 ORDER BY id`, txHash)
	if err != nil {
		dbError(c, err)
		return
	}
	if rows == nil {
		rows = []map[string]interface{}{}
	}
	c.JSON(http.StatusOK, gin.H{"transactionHash": txHash, "diffs": rows})
}

// GetTokenHoldersCount returns the real holder count for a token.
func (h *Handler) GetTokenHoldersCount(c *gin.Context) {
	address := c.Param("address")
	ctx := c.Request.Context()
	cacheKey := "token:holders:count:" + address
	if cached, err := h.redis.Get(ctx, cacheKey).Result(); err == nil {
		c.JSON(http.StatusOK, gin.H{"count": cached})
		return
	}
	total, err := h.countQuery(ctx, `SELECT count(*) FROM token_holders WHERE token_address = $1`, address)
	if err != nil {
		dbError(c, err)
		return
	}
	h.redis.Set(ctx, cacheKey, fmt.Sprintf("%d", total), 5*time.Minute)
	c.JSON(http.StatusOK, gin.H{"count": total})
}

// GetNFTFloorPrice returns floor price data from the nft_floor_prices table.
func (h *Handler) GetNFTFloorPrice(c *gin.Context) {
	address := c.Param("address")
	ctx := c.Request.Context()
	cacheKey := "nft:floor:" + address
	if cached, err := h.redis.Get(ctx, cacheKey).Result(); err == nil {
		var result map[string]interface{}
		if json.Unmarshal([]byte(cached), &result) == nil {
			c.JSON(http.StatusOK, result)
			return
		}
	}
	row, err := h.queryOne(ctx, `SELECT collection_address, floor_price, floor_price_usd, volume_24h, volume_24h_usd, sales_24h, holders FROM nft_floor_prices WHERE collection_address = $1 LIMIT 1`, address)
	if err != nil {
		dbError(c, err)
		return
	}
	if row == nil {
		c.JSON(http.StatusOK, gin.H{"collection_address": address, "floor_price": 0, "volume_24h": 0})
		return
	}
	if data, err := json.Marshal(row); err == nil {
		h.redis.Set(ctx, cacheKey, data, 5*time.Minute)
	}
	c.JSON(http.StatusOK, row)
}

// GetNFTFloorHistory returns floor-price history for a collection.
func (h *Handler) GetNFTFloorHistory(c *gin.Context) {
	address := c.Param("address")
	limit := paramInt(c, "limit", 30)
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, `SELECT collection_address, floor_price, floor_price_usd, volume_24h, sales_24h, updated_at FROM nft_floor_prices WHERE collection_address = $1 ORDER BY updated_at DESC LIMIT $2`, address, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// GetContractStorage returns storage slots for a contract address.
func (h *Handler) GetContractStorage(c *gin.Context) {
	address := c.Param("address")
	limit := paramInt(c, "limit", 50)
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, `SELECT id, transaction_hash, block_number, storage_key, storage_value, old_value, new_value, diff_type FROM state_diffs WHERE address = $1 ORDER BY block_number DESC LIMIT $2`, address, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// GetNFTOwners returns owners of an NFT collection from the nft_owners table.
func (h *Handler) GetNFTOwners(c *gin.Context) {
	address := c.Param("address")
	limit := paramInt(c, "limit", 25)
	offset := paramOffset(c)
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, `SELECT id, token_address, token_id, owner, updated_block FROM nft_owners WHERE token_address = $1 ORDER BY updated_block DESC LIMIT $2 OFFSET $3`, address, limit, offset)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM nft_owners WHERE token_address = $1`, address)
	respondList(c, rows, int(total))
}

// GetGovernanceProposals returns proposals from the governance_proposals table.
func (h *Handler) GetGovernanceProposals(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 25)
	offset := paramOffset(c)
	status := c.Query("status")

	var rows []map[string]interface{}
	var err error
	if status != "" {
		rows, err = h.queryRows(ctx, `SELECT id, proposal_id, contract_address, proposer, title, description, status, for_votes, against_votes, abstain_votes, total_votes, start_block, end_block, created_at FROM governance_proposals WHERE status = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3`, status, limit, offset)
	} else {
		rows, err = h.queryRows(ctx, `SELECT id, proposal_id, contract_address, proposer, title, description, status, for_votes, against_votes, abstain_votes, total_votes, start_block, end_block, created_at FROM governance_proposals ORDER BY created_at DESC LIMIT $1 OFFSET $2`, limit, offset)
	}
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM governance_proposals`)
	respondList(c, rows, int(total))
}

// GetGovernanceProposal returns a single proposal by id.
func (h *Handler) GetGovernanceProposal(c *gin.Context) {
	idStr := c.Param("id")
	id, err := strconv.ParseInt(idStr, 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid proposal id"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT id, proposal_id, contract_address, proposer, title, description, status, for_votes, against_votes, abstain_votes, total_votes, start_block, end_block, created_at FROM governance_proposals WHERE id = $1 OR proposal_id = $1 LIMIT 1`, id)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

// GetMEVBundles returns MEV bundles from the mev_bundles table.
func (h *Handler) GetMEVBundles(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 25)
	offset := paramOffset(c)
	rows, err := h.queryRows(ctx, `SELECT id, bundle_hash, block_number, sender, mev_type, tx_hashes, gas_used, profit_eth, profit_usd FROM mev_bundles ORDER BY block_number DESC LIMIT $1 OFFSET $2`, limit, offset)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM mev_bundles`)
	respondList(c, rows, int(total))
}

// GetVerifiedContracts returns verified contracts from the contracts table.
func (h *Handler) GetVerifiedContracts(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 25)
	offset := paramOffset(c)
	rows, err := h.queryRows(ctx, `SELECT id, address, contract_name, compiler, compiler_version, optimization_enabled, optimization_runs, evm_version, license_type, contract_type, verified_at FROM contracts WHERE is_verified = true ORDER BY verified_at DESC NULLS LAST LIMIT $1 OFFSET $2`, limit, offset)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM contracts WHERE is_verified = true`)
	respondList(c, rows, int(total))
}

// GetLabels returns address labels grouped by category from search_index.
func (h *Handler) GetLabels(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `SELECT id, search_type, address, name, description FROM search_index ORDER BY id DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	groups := map[string][]map[string]interface{}{}
	for _, r := range rows {
		cat, _ := r["search_type"].(string)
		if cat == "" {
			cat = "other"
		}
		groups[cat] = append(groups[cat], r)
	}
	out := make([]map[string]interface{}, 0, len(groups))
	for cat, items := range groups {
		out = append(out, map[string]interface{}{"category": cat, "labels": items})
	}
	c.JSON(http.StatusOK, out)
}

// GetAddressLabel returns the label for a specific address.
func (h *Handler) GetAddressLabel(c *gin.Context) {
	address := c.Param("address")
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT id, search_type, address, name, description FROM search_index WHERE address = $1 LIMIT 1`, address)
	if err != nil {
		dbError(c, err)
		return
	}
	if row == nil {
		c.JSON(http.StatusOK, gin.H{"address": address, "label": nil})
		return
	}
	c.JSON(http.StatusOK, gin.H{"address": address, "label": row["name"], "category": row["search_type"]})
}

// AdvancedSearch searches the search_index table by query string and type.
func (h *Handler) AdvancedSearch(c *gin.Context) {
	q := c.Query("q")
	searchType := c.Query("type")
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 25)

	var rows []map[string]interface{}
	var err error
	switch {
	case q != "" && searchType != "":
		rows, err = h.queryRows(ctx, `SELECT id, search_type, address, hash, number, name, description FROM search_index WHERE search_type = $1 AND (address ILIKE $2 OR name ILIKE $2 OR description ILIKE $2) ORDER BY id DESC LIMIT $3`, searchType, "%"+q+"%", limit)
	case q != "":
		rows, err = h.queryRows(ctx, `SELECT id, search_type, address, hash, number, name, description FROM search_index WHERE address ILIKE $1 OR name ILIKE $1 OR description ILIKE $1 ORDER BY id DESC LIMIT $2`, "%"+q+"%", limit)
	case searchType != "":
		rows, err = h.queryRows(ctx, `SELECT id, search_type, address, hash, number, name, description FROM search_index WHERE search_type = $1 ORDER BY id DESC LIMIT $2`, searchType, limit)
	default:
		rows, err = h.queryRows(ctx, `SELECT id, search_type, address, hash, number, name, description FROM search_index ORDER BY id DESC LIMIT $1`, limit)
	}
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"results": rows, "query": q, "type": searchType})
}

// GetDexAnalytics returns analytics for a DEX pair from dex_pairs.
func (h *Handler) GetDexAnalytics(c *gin.Context) {
	pairAddress := c.Param("address")
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT id, pair_address, token0_address, token1_address, token0_symbol, token1_symbol, reserve0, reserve1, total_supply, liquidity_usd, volume_24h, volume_change_24h, fee_24h, factory_address, pair_type FROM dex_pairs WHERE pair_address = $1 LIMIT 1`, pairAddress)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}

// GetGasHistory returns gas price history from the gas_prices table.
func (h *Handler) GetGasHistory(c *gin.Context) {
	limit := paramInt(c, "limit", 24)
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, `SELECT id, gas_price, gas_used, gas_limit, timestamp, base_fee FROM gas_prices ORDER BY id DESC LIMIT $1`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"timeframe": c.DefaultQuery("timeframe", "24h"), "history": rows})
}

// GetTokenPriceHistory returns price history from the token_prices table.
func (h *Handler) GetTokenPriceHistory(c *gin.Context) {
	address := c.Param("address")
	limit := paramInt(c, "limit", 30)
	ctx := c.Request.Context()
	rows, err := h.queryRows(ctx, `SELECT id, token_address, price_usd, timestamp, source FROM token_prices WHERE token_address = $1 ORDER BY timestamp DESC LIMIT $2`, address, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"token": address, "timeframe": c.DefaultQuery("timeframe", "24h"), "history": rows})
}
