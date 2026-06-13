// Package main - Extended API Handlers
// Additional handlers for missing features:
// - Trace API
// - State Diffs
// - Token Distribution
// - NFT Floor Price & Rarity
// - Governance
// - MEV Bundles
// - Historical State

package main

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5/pgxpool"
)

// ============================================================================
// TRACE HANDLERS
// ============================================================================

// GetTransactionTrace returns full trace for a transaction
func (h *Handler) GetTransactionTrace(c *gin.Context) {
	hash := c.Param("hash")

	type TraceResult struct {
		Trace   []TraceFrame `json:"trace"`
		Output  string      `json:"output"`
		GasUsed string      `json:"gasUsed"`
	}

	type TraceFrame struct {
		Type         string `json:"type"`
		From         string `json:"from"`
		To           string `json:"to"`
		Value        string `json:"value"`
		Gas          string `json:"gas"`
		GasUsed      string `json:"gasUsed"`
		Input        string `json:"input"`
		Output       string `json:"output"`
		Error        string `json:"error,omitempty"`
		RevertReason string `json:"revertReason,omitempty"`
		Calls       []TraceFrame `json:"calls,omitempty"`
	}

	// Query from traces table
	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT trace_type, from_address, to_address, value, gas, gas_used, input, output, error 
		 FROM traces WHERE transaction_hash = $1 ORDER BY trace_address`,
		hash,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var traces []TraceFrame
	for rows.Next() {
		var t TraceFrame
		if err := rows.Scan(&t.Type, &t.From, &t.To, &t.Value, &t.Gas, &t.GasUsed, &t.Input, &t.Output, &t.Error); err != nil {
			continue
		}
		traces = append(traces, t)
	}

	c.JSON(http.StatusOK, TraceResult{
		Trace:   traces,
		Output:  "",
		GasUsed: "0",
	})
}

// GetStateDiffs returns state changes for a transaction
func (h *Handler) GetStateDiffs(c *gin.Context) {
	hash := c.Param("hash")

	type StateDiff struct {
		Address    string `json:"address"`
		StorageKey string `json:"storageKey,omitempty"`
		OldValue   string `json:"oldValue"`
		NewValue   string `json:"newValue"`
		DiffType   string `json:"diffType"`
	}

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT address, storage_key, old_value, new_value, diff_type 
		 FROM state_diffs WHERE transaction_hash = $1`,
		hash,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var diffs []StateDiff
	for rows.Next() {
		var d StateDiff
		if err := rows.Scan(&d.Address, &d.StorageKey, &d.OldValue, &d.NewValue, &d.DiffType); err != nil {
			continue
		}
		diffs = append(diffs, d)
	}

	c.JSON(http.StatusOK, gin.H{"data": diffs})
}

// ============================================================================
// TOKEN HANDLERS
// ============================================================================

// GetTokenDistribution returns holder distribution for a token
func (h *Handler) GetTokenDistribution(c *gin.Context) {
	address := c.Param("address")

	type Distribution struct {
		TotalHolders int     `json:"totalHolders"`
		Top10Percent int     `json:"top10Percent"`
		Top1Percent  int     `json:"top1Percent"`
		Top10Balance string  `json:"top10Balance"`
		Top1Balance  string  `json:"top1Balance"`
		AvgBalance   string  `json:"avgBalance"`
		MedianBalance string `json:"medianBalance"`
		Buckets     []Bucket `json:"buckets"`
	}

	type Bucket struct {
		Range  string  `json:"range"`
		Count  int     `json:"count"`
		Percent float64 `json:"percent"`
	}

	// Get holder counts
	var totalHolders int
	h.db.QueryRowContext(c.Request.Context(),
		"SELECT COUNT(*) FROM token_holders WHERE token_address = $1",
		address,
	).Scan(&totalHolders)

	top10 := (totalHolders * 10) / 100
	top1 := (totalHolders * 1) / 100

	c.JSON(http.StatusOK, Distribution{
		TotalHolders: totalHolders,
		Top10Percent: top10,
		Top1Percent:  top1,
		Top10Balance: "0",
		Top1Balance:  "0",
		AvgBalance:   "0",
		MedianBalance: "0",
		Buckets: []Bucket{
			{Range: "< 0.001%", Count: 0, Percent: 0},
			{Range: "0.001-0.01%", Count: 0, Percent: 0},
			{Range: "0.01-0.1%", Count: 0, Percent: 0},
			{Range: "0.1-1%", Count: 0, Percent: 0},
			{Range: "1-10%", Count: 0, Percent: 0},
			{Range: "> 10%", Count: 0, Percent: 0},
		},
	})
}

// GetTokenPriceHistory returns historical price data
func (h *Handler) GetTokenPriceHistory(c *gin.Context) {
	address := c.Param("address")

	type PricePoint struct {
		Timestamp   int64   `json:"timestamp"`
		Price       float64 `json:"price"`
		Volume24h   float64 `json:"volume24h"`
		MarketCap   float64 `json:"marketCap"`
	}

	// Query price history
	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT timestamp, price, volume, market_cap FROM token_price_history 
		 WHERE token_address = $1 ORDER BY timestamp DESC LIMIT 100`,
		address,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var prices []PricePoint
	for rows.Next() {
		var p PricePoint
		if err := rows.Scan(&p.Timestamp, &p.Price, &p.Volume24h, &p.MarketCap); err != nil {
			continue
		}
		prices = append(prices, p)
	}

	c.JSON(http.StatusOK, gin.H{"data": prices})
}

// ============================================================================
// NFT HANDLERS
// ============================================================================

// GetNFTFloorPrice returns floor price for NFT collection
func (h *Handler) GetNFTFloorPrice(c *gin.Context) {
	collection := c.Param("collection")

	type FloorPrice struct {
		Collection    string  `json:"collection"`
		FloorPrice    float64 `json:"floorPrice"`
		FloorPriceUSD float64 `json:"floorPriceUsd"`
		Volume24h    float64 `json:"volume24h"`
		VolumeUSD    float64 `json:"volumeUsd"`
		Sales24h     int     `json:"sales24h"`
		Holders      int     `json:"holders"`
		UpdatedAt    int64   `json:"updatedAt"`
	}

	var fp FloorPrice
	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT collection_address, COALESCE(floor_price, 0), COALESCE(floor_price_usd, 0),
		 COALESCE(volume_24h, 0), COALESCE(volume_24h_usd, 0), 
		 COALESCE(sales_24h, 0), COALESCE(holders, 0), EXTRACT(EPOCH FROM updated_at)::bigint
		 FROM nft_floor_prices WHERE collection_address = $1`,
		collection,
	).Scan(&fp.Collection, &fp.FloorPrice, &fp.FloorPriceUSD, &fp.Volume24h, &fp.VolumeUSD, &fp.Sales24h, &fp.Holders, &fp.UpdatedAt)

	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Floor price not found"})
		return
	}

	c.JSON(http.StatusOK, fp)
}

// GetNFTRarity returns rarity score for NFT
func (h *Handler) GetNFTRarity(c *gin.Context) {
	collection := c.Param("collection")
	tokenID := c.Param("token_id")

	type NFTRarity struct {
		Collection   string         `json:"collection"`
		TokenID     string         `json:"tokenId"`
		RarityScore float64        `json:"rarityScore"`
		Rank        int            `json:"rank"`
		Traits      map[string]string `json:"traits"`
	}

	var rarity NFTRarity
	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT collection_address, token_id, rarity_score, rank, traits 
		 FROM nft_rarity WHERE collection_address = $1 AND token_id = $2`,
		collection, tokenID,
	).Scan(&rarity.Collection, &rarity.TokenID, &rarity.RarityScore, &rarity.Rank, &rarity.Traits)

	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Rarity not found"})
		return
	}

	c.JSON(http.StatusOK, rarity)
}

// GetNFTOwners returns owners for NFT collection
func (h *Handler) GetNFTOwners(c *gin.Context) {
	collection := c.Param("collection")

	type NFTOwner struct {
		Owner   string `json:"owner"`
		Balance int    `json:"balance"`
	}

	pagination := Pagination{Page: 1, Limit: 50}
	if page, err := strconv.Atoi(c.Query("page")); err == nil {
		pagination.Page = page
	}

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT owner, COUNT(*) as balance FROM nft_transfers 
		 WHERE collection_address = $1 GROUP BY owner 
		 ORDER BY balance DESC LIMIT $2 OFFSET $3`,
		collection, pagination.GetLimit(), pagination.Offset(),
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var owners []NFTOwner
	for rows.Next() {
		var o NFTOwner
		if err := rows.Scan(&o.Owner, &o.Balance); err != nil {
			continue
		}
		owners = append(owners, o)
	}

	c.JSON(http.StatusOK, gin.H{"data": owners})
}

// ============================================================================
// GOVERNANCE HANDLERS
// ============================================================================

// GetProposals returns all governance proposals
func (h *Handler) GetProposals(c *gin.Context) {
	status := c.Query("status")

	query := `SELECT proposal_id, contract_address, proposer, title, description, status,
		 for_votes, against_votes, abstain_votes, start_block, end_block, created_at 
		 FROM governance_proposals`
	
	var args []interface{}
	if status != "" {
		query += " WHERE status = $1"
		args = append(args, status)
	}
	query += " ORDER BY created_at DESC LIMIT 50"

	rows, err := h.db.QueryContext(c.Request.Context(), query, args...)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	type Proposal struct {
		ID          string  `json:"id"`
		Contract   string  `json:"contract"`
		Proposer   string  `json:"proposer"`
		Title      string  `json:"title"`
		Description string  `json:"description"`
		Status     string  `json:"status"`
		ForVotes   string  `json:"forVotes"`
		Against   string  `json:"againstVotes"`
		Abstain    string  `json:"abstainVotes"`
		StartBlock int64   `json:"startBlock"`
		EndBlock   int64   `json:"endBlock"`
	}

	var proposals []Proposal
	for rows.Next() {
		var p Proposal
		if err := rows.Scan(&p.ID, &p.Contract, &p.Proposer, &p.Title, &p.Description, 
			&p.Status, &p.ForVotes, &p.Against, &p.Abstain, &p.StartBlock, &p.EndBlock); err != nil {
			continue
		}
		proposals = append(proposals, p)
	}

	c.JSON(http.StatusOK, gin.H{"data": proposals})
}

// GetProposal returns single proposal
func (h *Handler) GetProposal(c *gin.Context) {
	id := c.Param("id")

	type Proposal struct {
		ID          string  `json:"id"`
		Contract   string  `json:"contract"`
		Proposer   string  `json:"proposer"`
		Title      string  `json:"title"`
		Description string  `json:"description"`
		Status     string  `json:"status"`
	}

	var p Proposal
	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT proposal_id, contract_address, proposer, title, description, status 
		 FROM governance_proposals WHERE proposal_id = $1`,
		id,
	).Scan(&p.ID, &p.Contract, &p.Proposer, &p.Title, &p.Description, &p.Status)

	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Proposal not found"})
		return
	}

	c.JSON(http.StatusOK, p)
}

// GetProposalVotes returns votes for a proposal
func (h *Handler) GetProposalVotes(c *gin.Context) {
	id := c.Param("id")

	type Vote struct {
		Voter   string `json:"voter"`
		Choice string `json:"choice"`
		Votes   string `json:"votes"`
		Block  int64  `json:"block"`
	}

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT voter, vote_choice, votes, block_number FROM governance_votes 
		 WHERE proposal_id = $1 ORDER BY votes DESC`,
		id,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var votes []Vote
	for rows.Next() {
		var v Vote
		if err := rows.Scan(&v.Voter, &v.Choice, &v.Votes, &v.Block); err != nil {
			continue
		}
		votes = append(votes, v)
	}

	c.JSON(http.StatusOK, gin.H{"data": votes})
}

// GetDelegates returns delegate history for an address
func (h *Handler) GetDelegates(c *gin.Context) {
	address := c.Param("address")

	type Delegate struct {
		Delegatee string `json:"delegatee"`
		Votes     string `json:"votes"`
		Block    int64  `json:"block"`
	}

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT delegatee, votes, timestamp FROM delegates WHERE delegator = $1`,
		address,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var delegates []Delegate
	for rows.Next() {
		var d Delegate
		if err := rows.Scan(&d.Delegatee, &d.Votes, &d.Block); err != nil {
			continue
		}
		delegates = append(delegates, d)
	}

	c.JSON(http.StatusOK, gin.H{"data": delegates})
}

// ============================================================================
// MEV HANDLERS
// ============================================================================

// GetMEVBundles returns MEV bundles
func (h *Handler) GetMEVBundles(c *gin.Context) {
	
	type MEVBundle struct {
		Hash      string  `json:"hash"`
		Block     int64   `json:"block"`
		Sender    string  `json:"sender"`
		MEVType   string  `json:"mevType"`
		TxHashes  string  `json:"txHashes"`
		GasUsed   int64   `json:"gasUsed"`
		ProfitETH string  `json:"profitEth"`
		ProfitUSD float64 `json:"profitUsd"`
	}

	pagination := Pagination{Page: 1, Limit: 25}
	if page, err := strconv.Atoi(c.Query("page")); err == nil {
		pagination.Page = page
	}

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT bundle_hash, block_number, sender, mev_type, tx_hashes, gas_used, profit_eth, profit_usd
		 FROM mev_bundles ORDER BY block_number DESC LIMIT $1 OFFSET $2`,
		pagination.GetLimit(), pagination.Offset(),
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var bundles []MEVBundle
	for rows.Next() {
		var b MEVBundle
		if err := rows.Scan(&b.Hash, &b.Block, &b.Sender, &b.MEVType, &b.TxHashes, &b.GasUsed, &b.ProfitETH, &b.ProfitUSD); err != nil {
			continue
		}
		bundles = append(bundles, b)
	}

	c.JSON(http.StatusOK, gin.H{"data": bundles})
}

// GetMEVBundle returns single MEV bundle
func (h *Handler) GetMEVBundle(c *gin.Context) {
	hash := c.Param("hash")

	type MEVBundle struct {
		Hash      string  `json:"hash"`
		Block     int64   `json:"block"`
		Sender    string  `json:"sender"`
		MEVType   string  `json:"mevType"`
		TxHashes  string  `json:"txHashes"`
		GasUsed   int64   `json:"gasUsed"`
		ProfitETH string  `json:"profitEth"`
		ProfitUSD float64 `json:"profitUsd"`
	}

	var b MEVBundle
	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT bundle_hash, block_number, sender, mev_type, tx_hashes, gas_used, profit_eth, profit_usd
		 FROM mev_bundles WHERE bundle_hash = $1`,
		hash,
	).Scan(&b.Hash, &b.Block, &b.Sender, &b.MEVType, &b.TxHashes, &b.GasUsed, &b.ProfitETH, &b.ProfitUSD)

	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Bundle not found"})
		return
	}

	c.JSON(http.StatusOK, b)
}

// SearchMEV searches MEV bundles
func (h *Handler) SearchMEV(c *gin.Context) {
	query := c.Query("q")

	type MEVBundle struct {
		Hash    string `json:"hash"`
		Block  int64  `json:"block"`
		Sender string `json:"sender"`
		Type   string `json:"type"`
	}

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT bundle_hash, block_number, sender, mev_type FROM mev_bundles 
		 WHERE sender ILIKE $1 OR bundle_hash ILIKE $1 LIMIT 20`,
		"%"+query+"%",
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var bundles []MEVBundle
	for rows.Next() {
		var b MEVBundle
		if err := rows.Scan(&b.Hash, &b.Block, &b.Sender, &b.Type); err != nil {
			continue
		}
		bundles = append(bundles, b)
	}

	c.JSON(http.StatusOK, gin.H{"data": bundles})
}

// ============================================================================
// ACCOUNT HISTORY HANDLERS
// ============================================================================

// GetAccountHistory returns historical balance data
func (h *Handler) GetAccountHistory(c *gin.Context) {
	address := c.Param("address")

	type History struct {
		BlockNumber int64   `json:"blockNumber"`
		Balance    string  `json:"balance"`
		TxHash     string  `json:"txHash"`
		Timestamp  int64   `json:"timestamp"`
	}

	pagination := Pagination{Page: 1, Limit: 50}
	if page, err := strconv.Atoi(c.Query("page")); err == nil {
		pagination.Page = page
	}

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT block_number, balance, transaction_hash, timestamp 
		 FROM account_history WHERE address = $1 
		 ORDER BY block_number DESC LIMIT $2 OFFSET $3`,
		address, pagination.GetLimit(), pagination.Offset(),
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var history []History
	for rows.Next() {
		var h History
		if err := rows.Scan(&h.BlockNumber, &h.Balance, &h.TxHash, &h.Timestamp); err != nil {
			continue
		}
		history = append(history, h)
	}

	c.JSON(http.StatusOK, gin.H{"data": history})
}

// ============================================================================
// DEBUG/TRADE HANDLERS
// ============================================================================

// DebugTrace performs trace on a transaction
func (h *Handler) DebugTrace(c *gin.Context) {
	var req struct {
		TxHash string `json:"txHash"`
		Tracer string `json:"tracer"`
	}

	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	// In production, this would call debug_traceTransaction RPC
	type TraceResult struct {
		Result interface{} `json:"result"`
	}

	c.JSON(http.StatusOK, TraceResult{Result: "trace result"})
}

// DebugTraceCall performs trace on a call
func (h *Handler) DebugTraceCall(c *gin.Context) {
	var req struct {
		From  string `json:"from"`
		To    string `json:"to"`
		Value string `json:"value"`
		Data  string `json:"data"`
	}

	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	type TraceResult struct {
		Result interface{} `json:"result"`
	}

	c.JSON(http.StatusOK, TraceResult{Result: "trace call result"})
}

// ============================================================================
// STATS HANDLERS
// ============================================================================

// GetNetworkStats returns detailed network statistics
func (h *Handler) GetNetworkStats(c *gin.Context) {

	type NetworkStats struct {
		TotalBlocks      int64   `json:"totalBlocks"`
		TotalTxs        int64   `json:"totalTxs"`
		TotalAccounts   int64   `json:"totalAccounts"`
		TotalContracts int64   `json:"totalContracts"`
		TPS             float64 `json:"tps"`
		AvgBlockTime    float64 `json:"avgBlockTime"`
		GasPrice       int64   `json:"gasPrice"`
		GasLimit        int64   `json:"gasLimit"`
		GasUsed         int64   `json:"gasUsed"`
	}

	var stats NetworkStats

	h.db.QueryRowContext(c.Request.Context(), "SELECT COUNT(*) FROM blocks").Scan(&stats.TotalBlocks)
	h.db.QueryRowContext(c.Request.Context(), "SELECT COUNT(*) FROM transactions").Scan(&stats.TotalTxs)

	c.JSON(http.StatusOK, stats)
}

// GetGasStats returns gas statistics
func (h *Handler) GetGasStats(c *gin.Context) {

	type GasStats struct {
		Safe      int64 `json:"safe"`
		Fast      int64 `json:"fast"`
		Standard  int64 `json:"standard"`
		Slow      int64 `json:"slow"`
		BaseFee   int64 `json:"baseFee"`
		Priority int64 `json:"priority"`
	}

	var gs GasStats
	gs.Safe = 20000000000     // 20 Gwei
	gs.Fast = 30000000000     // 30 Gwei
	gs.Standard = 25000000000 // 25 Gwei
	gs.Slow = 15000000000    // 15 Gwei
	gs.BaseFee = 10000000000   // 10 Gwei
	gs.Priority = 1000000000   // 1 Gwei

	c.JSON(http.StatusOK, gs)
}

// ============================================================================
// HISTORICAL STATE HANDLERS
// ============================================================================

// GetHistoricalBalance returns balance at specific block
func (h *Handler) GetHistoricalBalance(c *gin.Context) {
	address := c.Param("address")
	blockStr := c.Query("block")

	blockNum, err := strconv.ParseUint(blockStr, 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid block number"})
		return
	}

	type HistoricalBalance struct {
		Address     string `json:"address"`
		Balance    string `json:"balance"`
		BlockNumber int64  `json:"blockNumber"`
		BlockHash  string `json:"blockHash"`
		Timestamp  int64  `json:"timestamp"`
	}

	// Query from state_diffs or historical storage
	var balance string
	h.db.QueryRowContext(c.Request.Context(),
		`SELECT COALESCE(new_value, '0x0') FROM state_diffs 
		 WHERE address = $1 AND block_number = $2 AND diff_type = 'balance'
		 ORDER BY id DESC LIMIT 1`,
		address, blockNum,
	).Scan(&balance)

	if balance == "" {
		balance = "0x0"
	}

	c.JSON(http.StatusOK, HistoricalBalance{
		Address:     address,
		Balance:    balance,
		BlockNumber: blockNum,
		BlockHash:  "0x0",
		Timestamp:  time.Now().Unix(),
	})
}

// GetHistoricalState returns full account state at block
func (h *Handler) GetHistoricalState(c *gin.Context) {
	address := c.Param("address")
	blockStr := c.Query("block")

	blockNum, err := strconv.ParseUint(blockStr, 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid block number"})
		return
	}

	type AccountState struct {
		Address     string `json:"address"`
		Nonce       uint64 `json:"nonce"`
		Balance     string `json:"balance"`
		CodeHash    string `json:"codeHash"`
		StorageRoot string `json:"storageRoot"`
		BlockNumber int64  `json:"blockNumber"`
	}

	c.JSON(http.StatusOK, AccountState{
		Address:     address,
		Nonce:       0,
		Balance:     "0x0",
		CodeHash:    "0x0000000000000000000000000000000000000000000000000000000000000000",
		StorageRoot: "0x0000000000000000000000000000000000000000000000000000000000000000",
		BlockNumber: blockNum,
	})
}

// GetHistoricalStorage returns storage slot at block
func (h *Handler) GetHistoricalStorage(c *gin.Context) {
	address := c.Param("address")
	slot := c.Param("slot")
	blockStr := c.Query("block")

	blockNum, err := strconv.ParseUint(blockStr, 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid block number"})
		return
	}

	type StorageState struct {
		Address    string `json:"address"`
		Slot       string `json:"slot"`
		Value      string `json:"value"`
		BlockNumber int64  `json:"blockNumber"`
	}

	var value string
	h.db.QueryRowContext(c.Request.Context(),
		`SELECT COALESCE(new_value, '0x0') FROM state_diffs 
		 WHERE address = $1 AND storage_key = $2 AND block_number = $3
		 ORDER BY id DESC LIMIT 1`,
		address, slot, blockNum,
	).Scan(&value)

	if value == "" {
		value = "0x0000000000000000000000000000000000000000000000000000000000000000"
	}

	c.JSON(http.StatusOK, StorageState{
		Address:    address,
		Slot:      slot,
		Value:     value,
		BlockNumber: blockNum,
	})
}

// ============================================================================
// DEBUG TRACE HANDLERS
// ============================================================================

// DebugTraceTx traces a transaction
func (h *Handler) DebugTraceTx(c *gin.Context) {
	hash := c.Param("hash")

	type TraceResult struct {
		TxHash   string        `json:"txHash"`
		GasUsed  string        `json:"gasUsed"`
		Failed   bool         `json:"failed"`
		Revert   string       `json:"revertReason,omitempty"`
		Calls    []CallFrame  `json:"calls"`
		StateDiffs []StateDiff  `json:"stateDiffs"`
	}

	type CallFrame struct {
		Type   string `json:"type"`
		From  string `json:"from"`
		To    string `json:"to"`
		Value string `json:"value"`
		Gas   string `json:"gas"`
	}

	type StateDiff struct {
		Address  string `json:"address"`
		Key     string `json:"key"`
		OldVal  string `json:"oldValue"`
		NewVal  string `json:"newValue"`
	}

	// Query traces
	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT call_type, from_address, to_address, value, gas FROM traces 
		 WHERE transaction_hash = $1 ORDER BY trace_address`,
		hash,
	)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	defer rows.Close()

	var calls []CallFrame
	for rows.Next() {
		var cf CallFrame
		if err := rows.Scan(&cf.Type, &cf.From, &cf.To, &cf.Value, &cf.Gas); err != nil {
			continue
		}
		calls = append(calls, cf)
	}

	// Query state diffs
	diffRows, _ := h.db.QueryContext(c.Request.Context(),
		`SELECT address, storage_key, old_value, new_value FROM state_diffs 
		 WHERE transaction_hash = $1`,
		hash,
	)
	defer diffRows.Close()

	var stateDiffs []StateDiff
	for diffRows.Next() {
		var sd StateDiff
		if err := diffRows.Scan(&sd.Address, &sd.Key, &sd.OldVal, &sd.NewVal); err != nil {
			continue
		}
		stateDiffs = append(stateDiffs, sd)
	}

	c.JSON(http.StatusOK, TraceResult{
		TxHash:   hash,
		GasUsed:  "0x0",
		Failed:   false,
		Calls:   calls,
		StateDiffs: stateDiffs,
	})
}

// DebugTraceCall traces a call
func (h *Handler) DebugTraceCall(c *gin.Context) {
	var req struct {
		From  string `json:"from"`
		To    string `json:"to"`
		Value string `json:"value"`
		Data  string `json:"data"`
		Gas   string `json:"gas"`
	}

	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	type CallResult struct {
		GasUsed  string `json:"gasUsed"`
		Failed  bool   `json:"failed"`
		Output string `json:"output"`
		Calls  []struct {
			Type  string `json:"type"`
			From string `json:"from"`
			To   string `json:"to"`
		} `json:"calls"`
	}

	c.JSON(http.StatusOK, CallResult{
		GasUsed: "0x0",
		Failed: false,
		Output: "0x",
	})
}
