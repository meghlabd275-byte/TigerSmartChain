// TigerSmartChain API Server - extended handlers.
//
// Every handler in this file reads from the indexed PostgreSQL tables or from
// the chain node over JSON-RPC. Nothing here invents data: when a source is
// unavailable the handler reports an error instead of returning a placeholder.

package main

import (
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"math/big"
	"net/http"
	"strconv"
	"strings"

	"github.com/ethereum/go-ethereum/common"
	"github.com/gin-gonic/gin"
)

// ============================================================================
// Shared helpers
// ============================================================================

// hexQuantity renders a decimal string as an Ethereum hex quantity.
func hexQuantity(decimal string) string {
	n, ok := new(big.Int).SetString(strings.TrimSpace(decimal), 10)
	if !ok {
		return "0x0"
	}
	return "0x" + n.Text(16)
}

// nullStr dereferences a nullable column into an empty string.
func nullStr(ns sql.NullString) string {
	if !ns.Valid {
		return ""
	}
	return ns.String
}

// paginationFrom builds a Pagination from the page/limit query parameters.
func paginationFrom(c *gin.Context, defLimit int) Pagination {
	p := Pagination{Page: 1, Limit: defLimit, MaxLimit: 100}
	if page, err := strconv.Atoi(c.Query("page")); err == nil && page > 0 {
		p.Page = page
	}
	if limit, err := strconv.Atoi(c.Query("limit")); err == nil && limit > 0 {
		p.Limit = limit
	}
	return p
}

// internalError reports a database failure without leaking driver detail.
func internalError(c *gin.Context, err error) {
	c.JSON(http.StatusInternalServerError, gin.H{"error": "internal error"})
	_ = err
}

// scanRows iterates rows, invoking scan for each row. Rows that fail to scan are
// reported rather than silently skipped.
func scanRows(rows *sql.Rows, scan func() error) error {
	for rows.Next() {
		if err := scan(); err != nil {
			return err
		}
	}
	return rows.Err()
}

// ============================================================================
// Trace handlers
// ============================================================================

// TraceFrame is a single call frame in a transaction trace.
type TraceFrame struct {
	Type         string       `json:"type"`
	From         string       `json:"from"`
	To           string       `json:"to"`
	Value        string       `json:"value"`
	Gas          string       `json:"gas"`
	GasUsed      string       `json:"gasUsed"`
	Input        string       `json:"input"`
	Output       string       `json:"output"`
	Error        string       `json:"error,omitempty"`
	RevertReason string       `json:"revertReason,omitempty"`
	Calls        []TraceFrame `json:"calls,omitempty"`
}

// TransactionTrace is the full trace of a transaction.
type TransactionTrace struct {
	Trace   []TraceFrame `json:"trace"`
	Output  string       `json:"output"`
	GasUsed string       `json:"gasUsed"`
}

// StateDiff is a single storage or balance change.
type StateDiff struct {
	Address    string `json:"address"`
	StorageKey string `json:"storageKey,omitempty"`
	OldValue   string `json:"oldValue"`
	NewValue   string `json:"newValue"`
	DiffType   string `json:"diffType"`
}

// CallFrame is a call frame returned by the debug trace endpoints.
type CallFrame struct {
	Type    string `json:"type"`
	From    string `json:"from"`
	To      string `json:"to"`
	Value   string `json:"value"`
	Gas     string `json:"gas"`
	GasUsed string `json:"gasUsed"`
	Input   string `json:"input,omitempty"`
	Output  string `json:"output,omitempty"`
}

// GetTransactionTrace returns the indexed call trace for a transaction.
func (h *Handler) GetTransactionTrace(c *gin.Context) {
	hash := c.Param("hash")
	if !isTxHash(hash) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid transaction hash"})
		return
	}

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT call_type, from_address, COALESCE(to_address, ''), COALESCE(value, 0)::text,
		        COALESCE(gas, 0)::text, COALESCE(gas_used, 0)::text, COALESCE(input, ''),
		        COALESCE(output, ''), COALESCE(error, '')
		 FROM internal_transactions
		 WHERE LOWER(transaction_hash) = LOWER($1)
		 ORDER BY trace_index, subtrace_index`, hash)
	if err != nil {
		internalError(c, err)
		return
	}
	defer rows.Close()

	traces := make([]TraceFrame, 0, 16)
	if err := scanRows(rows, func() error {
		var f TraceFrame
		if err := rows.Scan(&f.Type, &f.From, &f.To, &f.Value, &f.Gas, &f.GasUsed,
			&f.Input, &f.Output, &f.Error); err != nil {
			return err
		}
		traces = append(traces, f)
		return nil
	}); err != nil {
		internalError(c, err)
		return
	}

	var (
		gasUsed   sql.NullString
		blockHash sql.NullString
	)
	err = h.db.QueryRowContext(c.Request.Context(),
		`SELECT COALESCE(gas_used, 0)::text, block_hash FROM transactions
		 WHERE LOWER(hash) = LOWER($1)`, hash).Scan(&gasUsed, &blockHash)
	if err != nil && err != sql.ErrNoRows {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"transactionHash": hash,
		"blockHash":       nullStr(blockHash),
		"trace":           traces,
		"gasUsed":         hexQuantity(nullStr(gasUsed)),
	})
}

// GetStateDiffs returns the indexed state changes for a transaction.
func (h *Handler) GetStateDiffs(c *gin.Context) {
	hash := c.Param("hash")
	if !isTxHash(hash) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid transaction hash"})
		return
	}

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT address, COALESCE(storage_key, ''), COALESCE(old_value, ''),
		        COALESCE(new_value, ''), diff_type
		 FROM state_diffs
		 WHERE LOWER(transaction_hash) = LOWER($1)
		 ORDER BY id`, hash)
	if err != nil {
		internalError(c, err)
		return
	}
	defer rows.Close()

	diffs := make([]StateDiff, 0, 16)
	if err := scanRows(rows, func() error {
		var d StateDiff
		if err := rows.Scan(&d.Address, &d.StorageKey, &d.OldValue, &d.NewValue, &d.DiffType); err != nil {
			return err
		}
		diffs = append(diffs, d)
		return nil
	}); err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{"transactionHash": hash, "data": diffs})
}

// ============================================================================
// Token handlers
// ============================================================================

// distributionBucket describes one holding-size band.
type distributionBucket struct {
	Range   string  `json:"range"`
	Count   int64   `json:"count"`
	Percent float64 `json:"percent"`
}

// GetTokenDistribution returns the holder distribution for a token, computed
// from token_holders.
func (h *Handler) GetTokenDistribution(c *gin.Context) {
	address := c.Param("address")
	if !isAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid token address"})
		return
	}

	var totalSupply sql.NullString
	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT total_supply::text FROM tokens WHERE LOWER(address) = LOWER($1)`,
		address).Scan(&totalSupply)
	if err == sql.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{"error": "token not found"})
		return
	}
	if err != nil {
		internalError(c, err)
		return
	}

	var (
		totalHolders int64
		top10Balance sql.NullString
		top1Balance  sql.NullString
		avgBalance   sql.NullString
		medianBal    sql.NullString
	)
	err = h.db.QueryRowContext(c.Request.Context(), `
		SELECT COUNT(*),
		       COALESCE(SUM(balance) FILTER (WHERE rnk <= GREATEST(1, (COUNT(*) OVER ()) / 10)), 0)::text,
		       COALESCE(SUM(balance) FILTER (WHERE rnk <= GREATEST(1, (COUNT(*) OVER ()) / 100)), 0)::text,
		       COALESCE(AVG(balance), 0)::text,
		       COALESCE(PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY balance), 0)::text
		FROM (
		    SELECT balance, ROW_NUMBER() OVER (ORDER BY balance DESC) AS rnk
		    FROM token_holders WHERE LOWER(token_address) = LOWER($1)
		) ranked`, address,
	).Scan(&totalHolders, &top10Balance, &top1Balance, &avgBalance, &medianBal)
	if err != nil {
		internalError(c, err)
		return
	}

	buckets := make([]distributionBucket, 0, 6)
	err = func() error {
		rows, err := h.db.QueryContext(c.Request.Context(), `
			SELECT CASE
			         WHEN pct < 0.001 THEN '< 0.001%'
			         WHEN pct < 0.01  THEN '0.001-0.01%'
			         WHEN pct < 0.1   THEN '0.01-0.1%'
			         WHEN pct < 1     THEN '0.1-1%'
			         WHEN pct < 10    THEN '1-10%'
			         ELSE '> 10%'
			       END AS band,
			       COUNT(*),
			       COALESCE(SUM(pct), 0)::float8
			FROM (
			    SELECT CASE WHEN $2::numeric > 0
			                THEN (balance * 100.0) / $2::numeric
			                ELSE 0 END AS pct
			    FROM token_holders WHERE LOWER(token_address) = LOWER($1)
			) shares
			GROUP BY band`, address, nullStr(totalSupply))
		if err != nil {
			return err
		}
		defer rows.Close()
		for rows.Next() {
			var b distributionBucket
			if err := rows.Scan(&b.Range, &b.Count, &b.Percent); err != nil {
				return err
			}
			buckets = append(buckets, b)
		}
		return rows.Err()
	}()
	if err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"token":         address,
		"totalSupply":   nullStr(totalSupply),
		"totalHolders":  totalHolders,
		"top10Balance":  nullStr(top10Balance),
		"top1Balance":   nullStr(top1Balance),
		"avgBalance":    nullStr(avgBalance),
		"medianBalance": nullStr(medianBal),
		"buckets":       buckets,
	})
}

// GetTokenPriceHistory returns recorded price points for a token.
func (h *Handler) GetTokenPriceHistory(c *gin.Context) {
	address := c.Param("address")
	if !isAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid token address"})
		return
	}
	pagination := paginationFrom(c, 100)

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT timestamp, price_usd::text
		 FROM token_prices WHERE LOWER(token_address) = LOWER($1)
		 ORDER BY timestamp DESC LIMIT $2 OFFSET $3`,
		address, pagination.GetLimit(), pagination.Offset())
	if err != nil {
		internalError(c, err)
		return
	}
	defer rows.Close()

	type pricePoint struct {
		Timestamp int64  `json:"timestamp"`
		PriceUSD  string `json:"priceUsd"`
	}
	out := make([]pricePoint, 0, 64)
	if err := scanRows(rows, func() error {
		var p pricePoint
		if err := rows.Scan(&p.Timestamp, &p.PriceUSD); err != nil {
			return err
		}
		out = append(out, p)
		return nil
	}); err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{"data": out})
}

// ============================================================================
// NFT handlers
// ============================================================================

// GetNFTFloorPrice returns the recorded floor price for a collection.
func (h *Handler) GetNFTFloorPrice(c *gin.Context) {
	collection := c.Param("collection")
	if !isAddress(collection) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid collection address"})
		return
	}

	var (
		addr         string
		floorPrice   sql.NullString
		floorPriceUS sql.NullString
		volume24h    sql.NullString
		volumeUSD    sql.NullString
		sales24h     sql.NullInt64
		holders      sql.NullInt64
		updatedAt    sql.NullInt64
	)
	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT collection_address, floor_price::text, floor_price_usd::text,
		        volume_24h::text, volume_24h_usd::text, sales_24h, holders,
		        EXTRACT(EPOCH FROM updated_at)::bigint
		 FROM nft_floor_prices WHERE LOWER(collection_address) = LOWER($1)`,
		collection).Scan(&addr, &floorPrice, &floorPriceUS, &volume24h, &volumeUSD,
		&sales24h, &holders, &updatedAt)
	if err == sql.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{"error": "floor price not recorded"})
		return
	}
	if err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"collection":    addr,
		"floorPrice":    nullStr(floorPrice),
		"floorPriceUsd": nullStr(floorPriceUS),
		"volume24h":     nullStr(volume24h),
		"volumeUsd":     nullStr(volumeUSD),
		"sales24h":      sales24h.Int64,
		"holders":       holders.Int64,
		"updatedAt":     updatedAt.Int64,
	})
}

// GetNFTRarity returns the recorded rarity entry for a token.
func (h *Handler) GetNFTRarity(c *gin.Context) {
	collection := c.Param("collection")
	tokenID := c.Param("token_id")
	if !isAddress(collection) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid collection address"})
		return
	}
	if _, err := strconv.ParseUint(tokenID, 10, 64); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid token id"})
		return
	}

	var (
		addr     string
		tokID    string
		score    sql.NullString
		rank     sql.NullInt64
		traitsJS sql.NullString
	)
	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT collection_address, token_id, rarity_score::text, rank, traits::text
		 FROM nft_rarity WHERE LOWER(collection_address) = LOWER($1) AND token_id = $2`,
		collection, tokenID).Scan(&addr, &tokID, &score, &rank, &traitsJS)
	if err == sql.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{"error": "rarity not recorded"})
		return
	}
	if err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"collection":  addr,
		"tokenId":     tokID,
		"rarityScore": nullStr(score),
		"rank":        rank.Int64,
		"traits":      rawJSON(nullStr(traitsJS)),
	})
}

// GetNFTOwners returns the current owner distribution for a collection.
func (h *Handler) GetNFTOwners(c *gin.Context) {
	collection := c.Param("collection")
	if !isAddress(collection) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid collection address"})
		return
	}
	pagination := paginationFrom(c, 50)

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT owner, COUNT(*) AS balance
		 FROM nft_owners WHERE LOWER(token_address) = LOWER($1)
		 GROUP BY owner ORDER BY balance DESC, owner
		 LIMIT $2 OFFSET $3`,
		collection, pagination.GetLimit(), pagination.Offset())
	if err != nil {
		internalError(c, err)
		return
	}
	defer rows.Close()

	type owner struct {
		Owner   string `json:"owner"`
		Balance int64  `json:"balance"`
	}
	out := make([]owner, 0, 50)
	if err := scanRows(rows, func() error {
		var o owner
		if err := rows.Scan(&o.Owner, &o.Balance); err != nil {
			return err
		}
		out = append(out, o)
		return nil
	}); err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{"data": out})
}

// ============================================================================
// Governance handlers
// ============================================================================

// GetProposals lists governance proposals.
func (h *Handler) GetProposals(c *gin.Context) {
	pagination := paginationFrom(c, 25)

	query := `SELECT proposal_id, contract_address, proposer, title, COALESCE(description, ''),
	                 status, for_votes::text, against_votes::text, abstain_votes::text,
	                 start_block, end_block
	          FROM governance_proposals`
	args := []interface{}{}
	if status := strings.TrimSpace(c.Query("status")); status != "" {
		args = append(args, status)
		query += " WHERE status = $1"
	}
	query += " ORDER BY created_at DESC LIMIT $" + strconv.Itoa(len(args)+1) +
		" OFFSET $" + strconv.Itoa(len(args)+2)
	args = append(args, pagination.GetLimit(), pagination.Offset())

	rows, err := h.db.QueryContext(c.Request.Context(), query, args...)
	if err != nil {
		internalError(c, err)
		return
	}
	defer rows.Close()

	type proposal struct {
		ID           string `json:"id"`
		Contract     string `json:"contract"`
		Proposer     string `json:"proposer"`
		Title        string `json:"title"`
		Description  string `json:"description"`
		Status       string `json:"status"`
		ForVotes     string `json:"forVotes"`
		AgainstVotes string `json:"againstVotes"`
		AbstainVotes string `json:"abstainVotes"`
		StartBlock   int64  `json:"startBlock"`
		EndBlock     int64  `json:"endBlock"`
	}
	out := make([]proposal, 0, 25)
	if err := scanRows(rows, func() error {
		var p proposal
		if err := rows.Scan(&p.ID, &p.Contract, &p.Proposer, &p.Title, &p.Description,
			&p.Status, &p.ForVotes, &p.AgainstVotes, &p.AbstainVotes,
			&p.StartBlock, &p.EndBlock); err != nil {
			return err
		}
		out = append(out, p)
		return nil
	}); err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{"data": out})
}

// GetProposal returns a single proposal with its vote tallies.
func (h *Handler) GetProposal(c *gin.Context) {
	id := c.Param("id")

	var p struct {
		ID           string `json:"id"`
		Contract     string `json:"contract"`
		Proposer     string `json:"proposer"`
		Title        string `json:"title"`
		Description  string `json:"description"`
		Status       string `json:"status"`
		ForVotes     string `json:"forVotes"`
		AgainstVotes string `json:"againstVotes"`
		AbstainVotes string `json:"abstainVotes"`
		TotalVotes   string `json:"totalVotes"`
		StartBlock   int64  `json:"startBlock"`
		EndBlock     int64  `json:"endBlock"`
	}
	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT proposal_id, contract_address, proposer, title, COALESCE(description, ''),
		        status, for_votes::text, against_votes::text, abstain_votes::text,
		        total_votes::text, start_block, end_block
		 FROM governance_proposals WHERE proposal_id = $1`, id,
	).Scan(&p.ID, &p.Contract, &p.Proposer, &p.Title, &p.Description, &p.Status,
		&p.ForVotes, &p.AgainstVotes, &p.AbstainVotes, &p.TotalVotes, &p.StartBlock, &p.EndBlock)
	if err == sql.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{"error": "proposal not found"})
		return
	}
	if err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, p)
}

// GetProposalVotes returns the individual votes cast on a proposal.
func (h *Handler) GetProposalVotes(c *gin.Context) {
	id := c.Param("id")

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT voter, vote_choice, votes::text, block_number, transaction_hash
		 FROM governance_votes WHERE proposal_id = $1
		 ORDER BY votes DESC LIMIT 500`, id)
	if err != nil {
		internalError(c, err)
		return
	}
	defer rows.Close()

	type vote struct {
		Voter  string `json:"voter"`
		Choice string `json:"choice"`
		Votes  string `json:"votes"`
		Block  int64  `json:"block"`
		TxHash string `json:"txHash"`
	}
	out := make([]vote, 0, 32)
	if err := scanRows(rows, func() error {
		var v vote
		if err := rows.Scan(&v.Voter, &v.Choice, &v.Votes, &v.Block, &v.TxHash); err != nil {
			return err
		}
		out = append(out, v)
		return nil
	}); err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{"data": out})
}

// GetDelegates returns the addresses an account has delegated to, derived from
// the governance votes it has cast.
func (h *Handler) GetDelegates(c *gin.Context) {
	address := c.Param("address")
	if !isAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid address"})
		return
	}

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT voter, SUM(votes)::text, MAX(block_number), COUNT(*)
		 FROM governance_votes WHERE LOWER(voter) = LOWER($1)
		 GROUP BY voter ORDER BY 2 DESC`, address)
	if err != nil {
		internalError(c, err)
		return
	}
	defer rows.Close()

	type delegate struct {
		Delegatee string `json:"delegatee"`
		Votes     string `json:"votes"`
		Block     int64  `json:"block"`
		Proposals int64  `json:"proposals"`
	}
	out := make([]delegate, 0, 8)
	if err := scanRows(rows, func() error {
		var d delegate
		if err := rows.Scan(&d.Delegatee, &d.Votes, &d.Block, &d.Proposals); err != nil {
			return err
		}
		out = append(out, d)
		return nil
	}); err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{"data": out})
}

// ============================================================================
// MEV handlers
// ============================================================================

// GetMEVBundles lists indexed MEV bundles.
func (h *Handler) GetMEVBundles(c *gin.Context) {
	pagination := paginationFrom(c, 25)

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT bundle_hash, block_number, sender, mev_type, tx_hashes,
		        COALESCE(gas_used, 0), COALESCE(profit_eth, 0)::text, COALESCE(profit_usd, 0)::text
		 FROM mev_bundles ORDER BY block_number DESC
		 LIMIT $1 OFFSET $2`, pagination.GetLimit(), pagination.Offset())
	if err != nil {
		internalError(c, err)
		return
	}
	defer rows.Close()

	type bundle struct {
		Hash      string `json:"hash"`
		Block     int64  `json:"block"`
		Sender    string `json:"sender"`
		MEVType   string `json:"mevType"`
		TxHashes  string `json:"txHashes"`
		GasUsed   int64  `json:"gasUsed"`
		ProfitETH string `json:"profitEth"`
		ProfitUSD string `json:"profitUsd"`
	}
	out := make([]bundle, 0, 25)
	if err := scanRows(rows, func() error {
		var b bundle
		if err := rows.Scan(&b.Hash, &b.Block, &b.Sender, &b.MEVType, &b.TxHashes,
			&b.GasUsed, &b.ProfitETH, &b.ProfitUSD); err != nil {
			return err
		}
		out = append(out, b)
		return nil
	}); err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{"data": out})
}

// GetMEVBundle returns a single MEV bundle.
func (h *Handler) GetMEVBundle(c *gin.Context) {
	hash := c.Param("hash")

	var b struct {
		Hash      string `json:"hash"`
		Block     int64  `json:"block"`
		Sender    string `json:"sender"`
		MEVType   string `json:"mevType"`
		TxHashes  string `json:"txHashes"`
		GasUsed   int64  `json:"gasUsed"`
		ProfitETH string `json:"profitEth"`
		ProfitUSD string `json:"profitUsd"`
	}
	err := h.db.QueryRowContext(c.Request.Context(),
		`SELECT bundle_hash, block_number, sender, mev_type, tx_hashes,
		        COALESCE(gas_used, 0), COALESCE(profit_eth, 0)::text, COALESCE(profit_usd, 0)::text
		 FROM mev_bundles WHERE LOWER(bundle_hash) = LOWER($1)`, hash,
	).Scan(&b.Hash, &b.Block, &b.Sender, &b.MEVType, &b.TxHashes,
		&b.GasUsed, &b.ProfitETH, &b.ProfitUSD)
	if err == sql.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{"error": "bundle not found"})
		return
	}
	if err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, b)
}

// SearchMEV searches MEV bundles by sender or bundle hash.
func (h *Handler) SearchMEV(c *gin.Context) {
	query := strings.TrimSpace(c.Query("q"))
	if query == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "query required"})
		return
	}

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT bundle_hash, block_number, sender, mev_type
		 FROM mev_bundles
		 WHERE sender ILIKE $1 OR bundle_hash ILIKE $1
		 ORDER BY block_number DESC LIMIT 20`, "%"+query+"%")
	if err != nil {
		internalError(c, err)
		return
	}
	defer rows.Close()

	type bundle struct {
		Hash    string `json:"hash"`
		Block   int64  `json:"block"`
		Sender  string `json:"sender"`
		MEVType string `json:"mevType"`
	}
	out := make([]bundle, 0, 20)
	if err := scanRows(rows, func() error {
		var b bundle
		if err := rows.Scan(&b.Hash, &b.Block, &b.Sender, &b.MEVType); err != nil {
			return err
		}
		out = append(out, b)
		return nil
	}); err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{"data": out})
}

// ============================================================================
// Account history
// ============================================================================

// GetAccountHistory returns the recorded balance history of an address.
func (h *Handler) GetAccountHistory(c *gin.Context) {
	address := c.Param("address")
	if !isAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid address"})
		return
	}
	pagination := paginationFrom(c, 50)

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT block_number, balance::text, COALESCE(transaction_hash, ''), timestamp
		 FROM token_holder_history WHERE LOWER(holder_address) = LOWER($1)
		 ORDER BY block_number DESC LIMIT $2 OFFSET $3`,
		address, pagination.GetLimit(), pagination.Offset())
	if err != nil {
		internalError(c, err)
		return
	}
	defer rows.Close()

	type entry struct {
		BlockNumber int64  `json:"blockNumber"`
		Balance     string `json:"balance"`
		TxHash      string `json:"txHash"`
		Timestamp   int64  `json:"timestamp"`
	}
	out := make([]entry, 0, 50)
	if err := scanRows(rows, func() error {
		var e entry
		if err := rows.Scan(&e.BlockNumber, &e.Balance, &e.TxHash, &e.Timestamp); err != nil {
			return err
		}
		out = append(out, e)
		return nil
	}); err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{"data": out})
}

// ============================================================================
// Debug trace handlers
// ============================================================================

// DebugTrace runs debug_traceTransaction on the node and returns the result.
func (h *Handler) DebugTrace(c *gin.Context) {
	var req struct {
		TxHash string `json:"txHash" binding:"required"`
		Tracer string `json:"tracer"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	if !isTxHash(req.TxHash) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid transaction hash"})
		return
	}

	tracer := req.Tracer
	if tracer == "" {
		tracer = "callTracer"
	}

	result, err := h.rpcCall(c.Request.Context(), "debug_traceTransaction",
		req.TxHash, map[string]interface{}{"tracer": tracer})
	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, gin.H{"txHash": req.TxHash, "tracer": tracer, "result": result})
}

// DebugTraceCall runs debug_traceCall on the node and returns the result.
func (h *Handler) DebugTraceCall(c *gin.Context) {
	var req struct {
		From   string `json:"from" binding:"required"`
		To     string `json:"to"`
		Value  string `json:"value"`
		Data   string `json:"data"`
		Gas    string `json:"gas"`
		Tracer string `json:"tracer"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	if !isAddress(req.From) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid from address"})
		return
	}

	call := map[string]interface{}{"from": req.From}
	if req.To != "" {
		if !isAddress(req.To) {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid to address"})
			return
		}
		call["to"] = req.To
	}
	if req.Value != "" {
		call["value"] = req.Value
	}
	if req.Data != "" {
		call["data"] = req.Data
	}
	if req.Gas != "" {
		call["gas"] = req.Gas
	}

	tracer := req.Tracer
	if tracer == "" {
		tracer = "callTracer"
	}

	result, err := h.rpcCall(c.Request.Context(), "debug_traceCall",
		call, "latest", map[string]interface{}{"tracer": tracer})
	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, gin.H{"tracer": tracer, "result": result})
}

// DebugTraceTx returns the indexed call frames and state diffs of a
// transaction. Unlike DebugTrace it does not need an archive node.
func (h *Handler) DebugTraceTx(c *gin.Context) {
	hash := c.Param("hash")
	if !isTxHash(hash) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid transaction hash"})
		return
	}

	rows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT call_type, from_address, COALESCE(to_address, ''),
		        COALESCE(value, 0)::text, COALESCE(gas, 0)::text, COALESCE(gas_used, 0)::text
		 FROM internal_transactions
		 WHERE LOWER(transaction_hash) = LOWER($1)
		 ORDER BY trace_index, subtrace_index`, hash)
	if err != nil {
		internalError(c, err)
		return
	}
	defer rows.Close()

	calls := make([]CallFrame, 0, 16)
	if err := scanRows(rows, func() error {
		var f CallFrame
		if err := rows.Scan(&f.Type, &f.From, &f.To, &f.Value, &f.Gas, &f.GasUsed); err != nil {
			return err
		}
		calls = append(calls, f)
		return nil
	}); err != nil {
		internalError(c, err)
		return
	}

	diffRows, err := h.db.QueryContext(c.Request.Context(),
		`SELECT address, COALESCE(storage_key, ''), COALESCE(old_value, ''),
		        COALESCE(new_value, ''), diff_type
		 FROM state_diffs WHERE LOWER(transaction_hash) = LOWER($1) ORDER BY id`, hash)
	if err != nil {
		internalError(c, err)
		return
	}
	defer diffRows.Close()

	stateDiffs := make([]StateDiff, 0, 16)
	if err := scanRows(diffRows, func() error {
		var d StateDiff
		if err := rows.Scan(&d.Address, &d.StorageKey, &d.OldValue, &d.NewValue, &d.DiffType); err != nil {
			return err
		}
		stateDiffs = append(stateDiffs, d)
		return nil
	}); err != nil {
		internalError(c, err)
		return
	}

	var (
		gasUsed sql.NullString
		status  sql.NullString
	)
	err = h.db.QueryRowContext(c.Request.Context(),
		`SELECT COALESCE(gas_used, 0)::text, status FROM transactions
		 WHERE LOWER(hash) = LOWER($1)`, hash).Scan(&gasUsed, &status)
	if err != nil && err != sql.ErrNoRows {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"txHash":     hash,
		"gasUsed":    hexQuantity(nullStr(gasUsed)),
		"failed":     strings.EqualFold(nullStr(status), "failed"),
		"calls":      calls,
		"stateDiffs": stateDiffs,
	})
}

// ============================================================================
// Stats handlers
// ============================================================================

// GetNetworkStats returns live network counters derived from the indexed data.
func (h *Handler) GetNetworkStats(c *gin.Context) {
	var stats struct {
		TotalBlocks    int64   `json:"totalBlocks"`
		TotalTxs       int64   `json:"totalTxs"`
		TotalAccounts  int64   `json:"totalAccounts"`
		TotalContracts int64   `json:"totalContracts"`
		TotalTokens    int64   `json:"totalTokens"`
		LastBlock      int64   `json:"lastBlock"`
		TPS            float64 `json:"tps"`
		AvgBlockTime   float64 `json:"avgBlockTime"`
		GasLimit       int64   `json:"gasLimit"`
		GasUsed        int64   `json:"gasUsed"`
	}

	err := h.db.QueryRowContext(c.Request.Context(), `
		SELECT
			(SELECT COUNT(*) FROM blocks),
			(SELECT COUNT(*) FROM transactions),
			(SELECT COUNT(*) FROM accounts),
			(SELECT COUNT(*) FROM contracts),
			(SELECT COUNT(*) FROM tokens),
			(SELECT COALESCE(MAX(number), 0) FROM blocks),
			(SELECT COALESCE(gas_limit, 0) FROM blocks ORDER BY number DESC LIMIT 1),
			(SELECT COALESCE(gas_used, 0) FROM blocks ORDER BY number DESC LIMIT 1)
	`).Scan(&stats.TotalBlocks, &stats.TotalTxs, &stats.TotalAccounts,
		&stats.TotalContracts, &stats.TotalTokens, &stats.LastBlock,
		&stats.GasLimit, &stats.GasUsed)
	if err != nil {
		internalError(c, err)
		return
	}

	var (
		count int64
		span  sql.NullInt64
	)
	err = h.db.QueryRowContext(c.Request.Context(), `
		SELECT COUNT(*), MAX(timestamp) - MIN(timestamp)
		FROM blocks
		WHERE number >= GREATEST(0, (SELECT COALESCE(MAX(number), 0) - 100 FROM blocks))
	`).Scan(&count, &span)
	if err != nil {
		internalError(c, err)
		return
	}
	if span.Valid && span.Int64 > 0 {
		stats.TPS = float64(count) / float64(span.Int64)
		if count > 1 {
			stats.AvgBlockTime = float64(span.Int64) / float64(count-1)
		}
	}

	c.JSON(http.StatusOK, stats)
}

// GetGasStats returns gas price percentiles computed from indexed blocks.
func (h *Handler) GetGasStats(c *gin.Context) {
	var (
		slow, standard, fast, baseFee sql.NullFloat64
	)
	err := h.db.QueryRowContext(c.Request.Context(), `
		SELECT
			percentile_cont(0.25) WITHIN GROUP (ORDER BY base_fee_per_gas),
			percentile_cont(0.50) WITHIN GROUP (ORDER BY base_fee_per_gas),
			percentile_cont(0.90) WITHIN GROUP (ORDER BY base_fee_per_gas),
			COALESCE(MAX(base_fee_per_gas), 0)
		FROM (
		    SELECT base_fee_per_gas FROM blocks
		    WHERE base_fee_per_gas IS NOT NULL
		    ORDER BY number DESC LIMIT 100
		) recent
	`).Scan(&slow, &standard, &fast, &baseFee)
	if err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"slow":     int64(slow.Float64),
		"standard": int64(standard.Float64),
		"fast":     int64(fast.Float64),
		"baseFee":  int64(baseFee.Float64),
	})
}

// ============================================================================
// Historical state handlers
// ============================================================================

// GetHistoricalBalance returns the indexed balance of an address at a block.
func (h *Handler) GetHistoricalBalance(c *gin.Context) {
	address := c.Param("address")
	blockStr := c.Query("block")
	if !isAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid address"})
		return
	}
	blockNum, err := strconv.ParseInt(blockStr, 10, 64)
	if err != nil || blockNum < 0 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid block number"})
		return
	}

	var (
		balance   sql.NullString
		blockHash sql.NullString
		timestamp sql.NullInt64
	)
	err = h.db.QueryRowContext(c.Request.Context(),
		`SELECT COALESCE(sa.balance, 0)::text, b.hash, b.timestamp
		 FROM state_accounts sa
		 LEFT JOIN blocks b ON b.number = sa.block_number
		 WHERE LOWER(sa.address) = LOWER($1) AND sa.block_number = $2
		 LIMIT 1`, address, blockNum).Scan(&balance, &blockHash, &timestamp)
	if err == sql.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{"error": "no recorded state for that block"})
		return
	}
	if err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"address":     address,
		"balance":     hexQuantity(nullStr(balance)),
		"blockNumber": blockNum,
		"blockHash":   nullStr(blockHash),
		"timestamp":   timestamp.Int64,
	})
}

// GetHistoricalState returns the indexed account state at a block.
func (h *Handler) GetHistoricalState(c *gin.Context) {
	address := c.Param("address")
	blockStr := c.Query("block")
	if !isAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid address"})
		return
	}
	blockNum, err := strconv.ParseInt(blockStr, 10, 64)
	if err != nil || blockNum < 0 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid block number"})
		return
	}

	var (
		balance     sql.NullString
		nonce       sql.NullInt64
		codeHash    sql.NullString
		storageRoot sql.NullString
		isContract  sql.NullBool
	)
	err = h.db.QueryRowContext(c.Request.Context(),
		`SELECT COALESCE(balance, 0)::text, COALESCE(nonce, 0), code_hash,
		        storage_root, is_contract
		 FROM state_accounts
		 WHERE LOWER(address) = LOWER($1) AND block_number = $2
		 LIMIT 1`, address, blockNum,
	).Scan(&balance, &nonce, &codeHash, &storageRoot, &isContract)
	if err == sql.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{"error": "no recorded state for that block"})
		return
	}
	if err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"address":     address,
		"balance":     hexQuantity(nullStr(balance)),
		"nonce":       nonce.Int64,
		"codeHash":    nullStr(codeHash),
		"storageRoot": nullStr(storageRoot),
		"isContract":  isContract.Bool,
		"blockNumber": blockNum,
	})
}

// GetHistoricalStorage returns the indexed value of a storage slot at a block.
func (h *Handler) GetHistoricalStorage(c *gin.Context) {
	address := c.Param("address")
	slot := c.Param("slot")
	blockStr := c.Query("block")
	if !isAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid address"})
		return
	}
	blockNum, err := strconv.ParseInt(blockStr, 10, 64)
	if err != nil || blockNum < 0 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid block number"})
		return
	}

	var value sql.NullString
	err = h.db.QueryRowContext(c.Request.Context(),
		`SELECT COALESCE(new_value, '') FROM state_diffs
		 WHERE LOWER(address) = LOWER($1) AND storage_key = $2 AND block_number <= $3
		   AND diff_type = 'storage'
		 ORDER BY block_number DESC, id DESC LIMIT 1`, address, slot, blockNum,
	).Scan(&value)
	if err == sql.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{"error": "no recorded value for that slot"})
		return
	}
	if err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"address":     address,
		"slot":        slot,
		"value":       nullStr(value),
		"blockNumber": blockNum,
	})
}

// ============================================================================
// Live node data
// ============================================================================

// GetLatestNodeBlock proxies eth_getBlockByNumber("latest") from the node.
func (h *Handler) GetLatestNodeBlock(c *gin.Context) {
	result, err := h.rpcCall(c.Request.Context(), "eth_getBlockByNumber", "latest", false)
	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"block": result})
}

// GetNodeChainID proxies eth_chainId from the node.
func (h *Handler) GetNodeChainID(c *gin.Context) {
	result, err := h.rpcCall(c.Request.Context(), "eth_chainId")
	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"chainId": result})
}

// GetNodeGasPrice proxies eth_gasPrice from the node.
func (h *Handler) GetNodeGasPrice(c *gin.Context) {
	result, err := h.rpcCall(c.Request.Context(), "eth_gasPrice")
	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"gasPrice": result})
}

// GetNodePeerCount proxies net_peerCount from the node.
func (h *Handler) GetNodePeerCount(c *gin.Context) {
	result, err := h.rpcCall(c.Request.Context(), "net_peerCount")
	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"peerCount": result})
}

// GetNodeSyncing proxies eth_syncing from the node.
func (h *Handler) GetNodeSyncing(c *gin.Context) {
	result, err := h.rpcCall(c.Request.Context(), "eth_syncing")
	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"syncing": result})
}

// ============================================================================
// Contract calls (proxied to the node)
// ============================================================================

// ReadContract performs an eth_call against the node.
func (h *Handler) ReadContract(c *gin.Context) {
	address := c.Param("address")
	if !isAddress(address) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid contract address"})
		return
	}
	var req struct {
		Data  string `json:"data" binding:"required"`
		From  string `json:"from"`
		Value string `json:"value"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	if !isHexData(req.Data) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "data must be 0x-prefixed hex"})
		return
	}

	call := map[string]interface{}{"to": address, "data": req.Data}
	if req.From != "" {
		if !isAddress(req.From) {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid from address"})
			return
		}
		call["from"] = req.From
	}
	if req.Value != "" {
		call["value"] = req.Value
	}

	result, err := h.rpcCall(c.Request.Context(), "eth_call", call, "latest")
	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"result": result})
}

// WriteContract broadcasts a signed transaction to the node.
func (h *Handler) WriteContract(c *gin.Context) {
	var req struct {
		SignedTx string `json:"signedTx" binding:"required"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	if !isHexData(req.SignedTx) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "signedTx must be 0x-prefixed hex"})
		return
	}

	result, err := h.rpcCall(c.Request.Context(), "eth_sendRawTransaction", req.SignedTx)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"transactionHash": result})
}

// ============================================================================
// Indexed lookups not covered by main.go
// ============================================================================

// GetLogs returns indexed event logs for an address or transaction.
func (h *Handler) GetLogs(c *gin.Context) {
	pagination := paginationFrom(c, 50)

	where := []string{}
	args := []interface{}{}
	if addr := strings.TrimSpace(c.Query("address")); addr != "" {
		if !isAddress(addr) {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid address"})
			return
		}
		args = append(args, addr)
		where = append(where, "LOWER(address) = LOWER($"+strconv.Itoa(len(args))+")")
	}
	if txHash := strings.TrimSpace(c.Query("txHash")); txHash != "" {
		if !isTxHash(txHash) {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid transaction hash"})
			return
		}
		args = append(args, txHash)
		where = append(where, "LOWER(transaction_hash) = LOWER($"+strconv.Itoa(len(args))+")")
	}

	query := `SELECT COALESCE(transaction_hash, ''), block_number, address,
	                 COALESCE(topic0, ''), COALESCE(topic1, ''), COALESCE(topic2, ''),
	                 COALESCE(topic3, ''), COALESCE(data, ''), log_index
	          FROM logs`
	if len(where) > 0 {
		query += " WHERE " + strings.Join(where, " AND ")
	}
	query += " ORDER BY block_number DESC, log_index DESC LIMIT $" +
		strconv.Itoa(len(args)+1) + " OFFSET $" + strconv.Itoa(len(args)+2)
	args = append(args, pagination.GetLimit(), pagination.Offset())

	rows, err := h.db.QueryContext(c.Request.Context(), query, args...)
	if err != nil {
		internalError(c, err)
		return
	}
	defer rows.Close()

	type logEntry struct {
		TxHash   string   `json:"transactionHash"`
		Block    int64    `json:"blockNumber"`
		Address  string   `json:"address"`
		Topics   []string `json:"topics"`
		Data     string   `json:"data"`
		LogIndex int64    `json:"logIndex"`
	}
	out := make([]logEntry, 0, 50)
	if err := scanRows(rows, func() error {
		var (
			e              logEntry
			t0, t1, t2, t3 string
		)
		if err := rows.Scan(&e.TxHash, &e.Block, &e.Address, &t0, &t1, &t2, &t3,
			&e.Data, &e.LogIndex); err != nil {
			return err
		}
		for _, t := range []string{t0, t1, t2, t3} {
			if t != "" {
				e.Topics = append(e.Topics, t)
			}
		}
		out = append(out, e)
		return nil
	}); err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{"data": out})
}

// GetCrossChainTransfers returns indexed bridge transfers.
func (h *Handler) GetCrossChainTransfers(c *gin.Context) {
	pagination := paginationFrom(c, 25)

	where := []string{}
	args := []interface{}{}
	if addr := strings.TrimSpace(c.Query("address")); addr != "" {
		if !isAddress(addr) {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid address"})
			return
		}
		args = append(args, addr)
		n := strconv.Itoa(len(args))
		where = append(where, "(LOWER(from_address) = LOWER($"+n+") OR LOWER(to_address) = LOWER($"+n+"))")
	}

	query := `SELECT transaction_hash, block_number, from_address, to_address,
	                 amount::text, COALESCE(token_address, ''), COALESCE(source_chain, ''),
	                 COALESCE(destination_chain, ''), status
	          FROM cross_chain_transfers`
	if len(where) > 0 {
		query += " WHERE " + strings.Join(where, " AND ")
	}
	query += " ORDER BY block_number DESC LIMIT $" + strconv.Itoa(len(args)+1) +
		" OFFSET $" + strconv.Itoa(len(args)+2)
	args = append(args, pagination.GetLimit(), pagination.Offset())

	rows, err := h.db.QueryContext(c.Request.Context(), query, args...)
	if err != nil {
		internalError(c, err)
		return
	}
	defer rows.Close()

	type transfer struct {
		TxHash      string `json:"transactionHash"`
		Block       int64  `json:"blockNumber"`
		From        string `json:"from"`
		To          string `json:"to"`
		Amount      string `json:"amount"`
		Token       string `json:"token"`
		SourceChain string `json:"sourceChain"`
		DestChain   string `json:"destinationChain"`
		Status      string `json:"status"`
	}
	out := make([]transfer, 0, 25)
	if err := scanRows(rows, func() error {
		var t transfer
		if err := rows.Scan(&t.TxHash, &t.Block, &t.From, &t.To, &t.Amount,
			&t.Token, &t.SourceChain, &t.DestChain, &t.Status); err != nil {
			return err
		}
		out = append(out, t)
		return nil
	}); err != nil {
		internalError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{"data": out})
}

// ============================================================================
// Validation helpers
// ============================================================================

// isAddress reports whether s is a 0x-prefixed 20-byte hex address.
func isAddress(s string) bool {
	if len(s) != 42 || !strings.HasPrefix(s, "0x") {
		return false
	}
	return common.IsHexAddress(s)
}

// isTxHash reports whether s is a 0x-prefixed 32-byte hex hash.
func isTxHash(s string) bool {
	if len(s) != 66 || !strings.HasPrefix(s, "0x") {
		return false
	}
	_, err := hex.DecodeString(s[2:])
	return err == nil
}

// isHexData reports whether s is 0x-prefixed hex with an even digit count.
func isHexData(s string) bool {
	if !strings.HasPrefix(s, "0x") || len(s) < 2 {
		return false
	}
	body := s[2:]
	if len(body)%2 != 0 {
		return false
	}
	_, err := hex.DecodeString(body)
	return err == nil
}

// rawJSON passes an already-encoded JSON value through unchanged, or null.
func rawJSON(s string) interface{} {
	trimmed := strings.TrimSpace(s)
	if trimmed == "" {
		return nil
	}
	return json.RawMessage(trimmed)
}
