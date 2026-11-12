package gateway

import (
	"net/http"

	"github.com/gin-gonic/gin"
)

// Token transfer feed.
//
// The transfer tracker page used to build 200 rows in the browser with
// Math.random: random token, random sender and receiver, random value, random
// gas, a 2% chance of "failed", and block numbers counting up from a fixed
// 35,000,000 that does not correspond to this chain. It also listed eight
// hardcoded tokens, every one of them from BNB Chain, with their real prices and
// holder counts attached.
//
// This reads the token_transfers table instead. Amounts stay as raw wei strings;
// turning them into a human figure needs the token's decimals, which the client
// has from the tokens table.

// GetRecentTokenTransfers returns the newest ERC-20 transfers chain-wide.
//
// GET /transfers
func (h *Handler) GetRecentTokenTransfers(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	offset := paramOffset(c)

	// The join gives each row the token's symbol and decimals so the client can
	// format the amount without a second round trip per row.
	rows, err := h.queryRows(ctx, `
		SELECT
			tt.id,
			tt.token_address,
			tt.from_address,
			tt.to_address,
			tt.value,
			tt.transaction_hash,
			tt.block_number,
			tt.log_index,
			t.symbol AS token_symbol,
			t.name AS token_name,
			t.decimals AS token_decimals,
			t.is_verified AS token_verified,
			b.timestamp AS block_timestamp
		FROM token_transfers tt
		LEFT JOIN tokens t ON t.address = tt.token_address
		LEFT JOIN blocks b ON b.number = tt.block_number
		ORDER BY tt.block_number DESC, tt.log_index DESC
		LIMIT $1 OFFSET $2
	`, limit, offset)
	if err != nil {
		dbError(c, err)
		return
	}

	total, _ := h.countQuery(ctx, `SELECT count(*) FROM token_transfers`)
	respondList(c, rows, int(total))
}

// GetTokenTransferStats returns totals over a recent window. The page's summary
// cards were computed from the 200 generated rows, which made them a summary of
// nothing.
//
// GET /transfers/stats
func (h *Handler) GetTokenTransferStats(c *gin.Context) {
	ctx := c.Request.Context()
	since := nowSeconds() - 24*60*60

	row, err := h.queryOne(ctx, `
		SELECT
			COUNT(*) AS total_transfers,
			COUNT(DISTINCT tt.from_address) AS unique_senders,
			COUNT(DISTINCT tt.to_address) AS unique_receivers,
			COUNT(DISTINCT tt.token_address) AS distinct_tokens,
			COALESCE(MIN(tt.block_number), 0) AS first_block,
			COALESCE(MAX(tt.block_number), 0) AS last_block
		FROM token_transfers tt
		LEFT JOIN blocks b ON b.number = tt.block_number
		WHERE b.timestamp >= $1
	`, since)
	if err != nil {
		dbError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"window":          "24h",
		"totalTransfers":  rowValue(row, "total_transfers"),
		"uniqueSenders":   rowValue(row, "unique_senders"),
		"uniqueReceivers": rowValue(row, "unique_receivers"),
		"distinctTokens":  rowValue(row, "distinct_tokens"),
		"firstBlock":      rowValue(row, "first_block"),
		"lastBlock":       rowValue(row, "last_block"),
	})
}

// GetTokenTransferSeries returns per-hour transfer counts so the page can plot
// real activity. It returns counts rather than a dollar volume: this chain's
// indexed data has no price for most tokens, and inventing a dollar figure from
// a wei amount would repeat the mistake the page used to make.
//
// GET /transfers/series
func (h *Handler) GetTokenTransferSeries(c *gin.Context) {
	ctx := c.Request.Context()
	hours := paramInt(c, "hours", 24)

	rows, err := h.queryRows(ctx, `
		SELECT
			date_trunc('hour', b.timestamp) AS bucket,
			COUNT(*) AS transfer_count,
			COUNT(DISTINCT tt.token_address) AS token_count
		FROM token_transfers tt
		JOIN blocks b ON b.number = tt.block_number
		WHERE b.timestamp >= to_timestamp($1)
		GROUP BY bucket
		ORDER BY bucket ASC
	`, nowSeconds()-int64(hours)*3600)
	if err != nil {
		dbError(c, err)
		return
	}

	respondList(c, rows, len(rows))
}
