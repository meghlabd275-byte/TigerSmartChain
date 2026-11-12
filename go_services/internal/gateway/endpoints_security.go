package gateway

import (
	"net/http"

	"github.com/gin-gonic/gin"
)

// Handlers for the indexed tables that had no route. Each table is written by an
// indexer service and read here, so an explorer page has a real source instead
// of generating rows in the browser.

// ==================== WHALE ACTIVITY ====================

// GetWhales lists large transfers ordered by value.
func (h *Handler) GetWhales(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `
		SELECT id, transaction_hash, from_address, to_address, value, value_usd,
		       token_address, block_number, whale_type, created_at
		FROM whale_transactions
		ORDER BY value DESC, id DESC LIMIT $1
	`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM whale_transactions`)
	respondList(c, rows, int(total))
}

// GetWhaleStats aggregates whale, MEV and flash-loan activity over the last
// 24 hours. Each figure is a SQL sum over the indexed tables; there is no
// estimate, so an unindexed chain reports zero rather than a plausible number.
func (h *Handler) GetWhaleStats(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `
		SELECT
			(SELECT COUNT(*) FROM whale_transactions) AS whale_transfer_count,
			(SELECT COALESCE(SUM(value_usd), 0) FROM whale_transactions
			 WHERE created_at >= NOW() - INTERVAL '24 hours') AS whale_volume_24h,
			(SELECT COALESCE(SUM(profit_usd), 0) FROM mev_bundles
			 WHERE inserted_at >= NOW() - INTERVAL '24 hours') AS mev_profit_24h,
			(SELECT COUNT(*) FROM mev_bundles
			 WHERE inserted_at >= NOW() - INTERVAL '24 hours') AS mev_bundle_count_24h,
			(SELECT COALESCE(SUM(profit_usd), 0) FROM flash_loans
			 WHERE created_at >= NOW() - INTERVAL '24 hours') AS flash_profit_24h,
			(SELECT COUNT(*) FROM flash_loans
			 WHERE created_at >= NOW() - INTERVAL '24 hours') AS flash_loan_count_24h
	`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

// GetWhalesByAddress lists whale transfers where the address is either side.
func (h *Handler) GetWhalesByAddress(c *gin.Context) {
	addr := c.Param("address")
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `
		SELECT id, transaction_hash, from_address, to_address, value, value_usd,
		       token_address, block_number, whale_type, created_at
		FROM whale_transactions
		WHERE from_address = $1 OR to_address = $1
		ORDER BY block_number DESC, id DESC LIMIT $2
	`, addr, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// ==================== SECURITY ====================

// GetSecurityStats counts the indexed threat records by kind.
func (h *Handler) GetSecurityStats(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `
		SELECT
			(SELECT count(*) FROM malicious_contracts WHERE is_active) AS malicious_contracts,
			(SELECT count(*) FROM phishing_urls WHERE is_active) AS phishing_urls,
			(SELECT count(*) FROM scam_tokens WHERE is_active) AS scam_tokens
	`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

// GetMaliciousContracts lists flagged contract addresses.
func (h *Handler) GetMaliciousContracts(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `
		SELECT id, address, threat_type, confidence, description, source,
		       first_seen, last_seen, is_active
		FROM malicious_contracts WHERE is_active
		ORDER BY last_seen DESC, id DESC LIMIT $1
	`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM malicious_contracts WHERE is_active`)
	respondList(c, rows, int(total))
}

// GetPhishingURLs lists flagged phishing URLs.
func (h *Handler) GetPhishingURLs(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `
		SELECT id, url, domain, target_addresses, description, source,
		       first_seen, last_seen, is_active
		FROM phishing_urls WHERE is_active
		ORDER BY last_seen DESC, id DESC LIMIT $1
	`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM phishing_urls WHERE is_active`)
	respondList(c, rows, int(total))
}

// GetScamTokens lists flagged token contracts.
func (h *Handler) GetScamTokens(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `
		SELECT id, address, name, symbol, threat_type, confidence, description,
		       source, first_seen, is_active
		FROM scam_tokens WHERE is_active
		ORDER BY first_seen DESC, id DESC LIMIT $1
	`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM scam_tokens WHERE is_active`)
	respondList(c, rows, int(total))
}

// GetSecurityCheck reports every threat record for one address, so a caller can
// decide whether to warn before interacting with it.
func (h *Handler) GetSecurityCheck(c *gin.Context) {
	addr := c.Param("address")
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()

	malicious, err := h.queryRows(ctx, `
		SELECT address, threat_type, confidence, description, source
		FROM malicious_contracts WHERE address = $1 AND is_active
	`, addr)
	if err != nil {
		dbError(c, err)
		return
	}
	scams, err := h.queryRows(ctx, `
		SELECT address, name, symbol, threat_type, confidence, description
		FROM scam_tokens WHERE address = $1 AND is_active
	`, addr)
	if err != nil {
		dbError(c, err)
		return
	}

	// Build a fresh slice so the append cannot write into malicious's backing
	// array, which is still referenced above.
	threats := make([]map[string]interface{}, 0, len(malicious)+len(scams))
	threats = append(threats, malicious...)
	threats = append(threats, scams...)
	c.JSON(http.StatusOK, gin.H{
		"address":      addr,
		"is_flagged":   len(threats) > 0,
		"threats":      threats,
		"threat_count": len(threats),
	})
}

// ==================== STAKING ====================

// GetStakingDelegations lists delegation records.
func (h *Handler) GetStakingDelegations(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `
		SELECT id, delegator, validator, amount, rewards, lock_start_time,
		       lock_end_time, is_compound, is_withdrawn, block_number
		FROM stakings ORDER BY block_number DESC, id DESC LIMIT $1
	`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM stakings`)
	respondList(c, rows, int(total))
}

// GetStakingByDelegator lists the delegations made by one address.
func (h *Handler) GetStakingByDelegator(c *gin.Context) {
	addr := c.Param("address")
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `
		SELECT id, delegator, validator, amount, rewards, lock_start_time,
		       lock_end_time, is_compound, is_withdrawn, block_number
		FROM stakings WHERE delegator = $1
		ORDER BY block_number DESC, id DESC LIMIT $2
	`, addr, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// ==================== FLASH LOANS ====================

// GetFlashLoans lists indexed flash loans.
func (h *Handler) GetFlashLoans(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `
		SELECT id, transaction_hash, block_number, borrower, provider, asset,
		       amount, fee, profit_usd
		FROM flash_loans ORDER BY block_number DESC, id DESC LIMIT $1
	`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM flash_loans`)
	respondList(c, rows, int(total))
}

// ==================== BRIDGE ====================

// GetBridgeTransfers lists cross-chain transfers.
func (h *Handler) GetBridgeTransfers(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `
		SELECT id, transfer_id, source_chain_id, dest_chain_id, sender, recipient,
		       token_address, amount, amount_usd, status, direction, fee,
		       block_number, created_at, updated_at
		FROM bridge_transfers ORDER BY block_number DESC, id DESC LIMIT $1
	`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM bridge_transfers`)
	respondList(c, rows, int(total))
}

// GetBridgeTransfer returns one transfer by its bridge transfer id.
func (h *Handler) GetBridgeTransfer(c *gin.Context) {
	transferID := c.Param("id")
	if transferID == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing transfer id"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `
		SELECT id, transfer_id, source_chain_id, dest_chain_id, sender, recipient,
		       token_address, amount, amount_usd, status, direction, fee,
		       nonce, message_hash, confirmation_time, completion_time, block_number
		FROM bridge_transfers WHERE transfer_id = $1
	`, transferID)
	if err != nil {
		dbError(c, err)
		return
	}
	if row == nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "transfer not found"})
		return
	}
	c.JSON(http.StatusOK, row)
}

// ==================== TOKEN APPROVALS ====================

// riskCase is the shared SQL expression that classifies a spender. It reads the
// threat tables rather than guessing, so a spender is only marked dangerous
// when the security indexer has actually flagged it. Unlimited allowances are
// treated as a warning because they let a spender drain the balance later.
const riskCase = `
	CASE
		WHEN a.spender IN (SELECT address FROM malicious_contracts WHERE is_active)
		  OR a.spender IN (SELECT address FROM scam_tokens WHERE is_active)
		THEN 'danger'
		WHEN a.value >= 115792089237316195423570985008687907853269984665640564039457584007913129639935
		THEN 'warning'
		ELSE 'safe'
	END`

// GetApprovals lists token approvals across the chain with a real risk label.
func (h *Handler) GetApprovals(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 100)
	offset := paramOffset(c)
	rows, err := h.queryRows(ctx, `
		SELECT a.id, a.token_address, a.owner, a.spender, a.value,
		       a.transaction_hash, a.block_number, a.log_index, a.created_at,
		       `+riskCase+` AS risk
		FROM token_approvals a
		ORDER BY a.block_number DESC, a.log_index DESC
		LIMIT $1 OFFSET $2
	`, limit, offset)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM token_approvals`)
	respondList(c, rows, int(total))
}

// GetApprovalsByOwner lists the approvals one owner has granted.
func (h *Handler) GetApprovalsByOwner(c *gin.Context) {
	owner := c.Param("address")
	if owner == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 100)
	offset := paramOffset(c)
	rows, err := h.queryRows(ctx, `
		SELECT a.id, a.token_address, a.owner, a.spender, a.value,
		       a.transaction_hash, a.block_number, a.log_index, a.created_at,
		       `+riskCase+` AS risk
		FROM token_approvals a
		WHERE a.owner = $1
		ORDER BY a.block_number DESC, a.log_index DESC
		LIMIT $2 OFFSET $3
	`, owner, limit, offset)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM token_approvals WHERE owner = $1`, owner)
	respondList(c, rows, int(total))
}

// GetApprovalsBySpender lists the allowances granted to one spender. This is
// the view a user needs to see who can move their tokens.
func (h *Handler) GetApprovalsBySpender(c *gin.Context) {
	spender := c.Param("address")
	if spender == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 100)
	offset := paramOffset(c)
	rows, err := h.queryRows(ctx, `
		SELECT a.id, a.token_address, a.owner, a.spender, a.value,
		       a.transaction_hash, a.block_number, a.log_index, a.created_at,
		       `+riskCase+` AS risk
		FROM token_approvals a
		WHERE a.spender = $1
		ORDER BY a.block_number DESC, a.log_index DESC
		LIMIT $2 OFFSET $3
	`, spender, limit, offset)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM token_approvals WHERE spender = $1`, spender)
	respondList(c, rows, int(total))
}

// GetApprovalStats counts approvals by risk band, so the approvals page can
// show a summary without loading every row.
func (h *Handler) GetApprovalStats(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `
		SELECT
			COUNT(*) AS total,
			COUNT(*) FILTER (WHERE `+riskCase+` = 'danger') AS danger,
			COUNT(*) FILTER (WHERE `+riskCase+` = 'warning') AS warning,
			COUNT(*) FILTER (WHERE `+riskCase+` = 'safe') AS safe
		FROM token_approvals a
	`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

// ==================== NFT APPROVALS ====================
// GetNFTApprovals lists NFT approval events.
func (h *Handler) GetNFTApprovals(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `
		SELECT id, token_address, owner, approved, token_id, transaction_hash,
		       block_number, log_index
		FROM nft_approvals ORDER BY block_number DESC, log_index DESC LIMIT $1
	`, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM nft_approvals`)
	respondList(c, rows, int(total))
}

// GetNFTApprovalsByOwner lists approvals granted by one owner.
func (h *Handler) GetNFTApprovalsByOwner(c *gin.Context) {
	addr := c.Param("address")
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	rows, err := h.queryRows(ctx, `
		SELECT id, token_address, owner, approved, token_id, transaction_hash,
		       block_number, log_index
		FROM nft_approvals WHERE owner = $1
		ORDER BY block_number DESC, log_index DESC LIMIT $2
	`, addr, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}
