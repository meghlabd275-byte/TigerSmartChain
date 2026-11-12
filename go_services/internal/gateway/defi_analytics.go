package gateway

import (
	"net/http"

	"github.com/gin-gonic/gin"
)

// DeFi analytics.
//
// The DeFi dashboard previously invented all of it: ten protocols (Aave,
// Compound, Uniswap, Curve, MakerDAO, Yearn, Convex, SushiSwap, Lido, Rocket
// Pool) with their real mainnet TVL, volume, fee and user figures; eight pools
// with APYs; six lending markets with supply and borrow rates; a 30-day TVL
// series built with Math.random; and a "TVL by Chain" table quoting Ethereum,
// Arbitrum, Optimism, Polygon, Base and Avalanche. None of those protocols run
// on this chain, and TigerScan has no way to measure another chain's TVL.
//
// This exposes the DeFi tables the indexer actually maintains: defi_tvl,
// defi_tvl_history, lending_rates and dex_pairs.

// GetDeFiProtocols returns the tracked protocols with their current TVL.
//
// GET /defi/protocols
func (h *Handler) GetDeFiProtocols(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 25)

	rows, err := h.queryRows(ctx, `
		SELECT protocol, tvl, tvl_change_24h, volume_24h, updated_at
		FROM defi_tvl
		ORDER BY tvl DESC NULLS LAST
		LIMIT $1
	`, limit)
	if err != nil {
		dbError(c, err)
		return
	}

	total, _ := h.countQuery(ctx, `SELECT count(*) FROM defi_tvl`)
	respondList(c, rows, int(total))
}

// GetDeFiTVLHistory returns the daily TVL series across every tracked protocol,
// summed per day. The dashboard's chart was previously 30 days of Math.random
// walking from an invented 85 billion starting point.
//
// GET /defi/tvl-history
func (h *Handler) GetDeFiTVLHistory(c *gin.Context) {
	ctx := c.Request.Context()
	days := paramInt(c, "days", 30)

	rows, err := h.queryRows(ctx, `
		SELECT
			date,
			SUM(tvl) AS tvl,
			COUNT(DISTINCT protocol) AS protocol_count
		FROM defi_tvl_history
		WHERE date >= CURRENT_DATE - ($1::int * INTERVAL '1 day')
		GROUP BY date
		ORDER BY date ASC
	`, days)
	if err != nil {
		dbError(c, err)
		return
	}

	respondList(c, rows, len(rows))
}

// GetDeFiLendingRates returns the tracked money-market rates.
//
// GET /defi/lending
func (h *Handler) GetDeFiLendingRates(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)

	rows, err := h.queryRows(ctx, `
		SELECT protocol, token, supply_rate, borrow_rate, utilization,
		       total_supply, total_borrow, updated_at
		FROM lending_rates
		ORDER BY total_supply DESC NULLS LAST
		LIMIT $1
	`, limit)
	if err != nil {
		dbError(c, err)
		return
	}

	total, _ := h.countQuery(ctx, `SELECT count(*) FROM lending_rates`)
	respondList(c, rows, int(total))
}

// GetDeFiPools returns the deepest DEX pairs together with an annualised fee
// yield.
//
// The fee yield is fee_24h / liquidity_usd * 365. It is the standard
// fees-over-TVL figure and it is derived entirely from indexed columns, so it
// is a measurement rather than an invented APY. It assumes all fees accrue to
// liquidity providers, which is what the pair's own accounting records, and it
// is labelled as a fee yield in the UI for that reason.
//
// GET /defi/pools
func (h *Handler) GetDeFiPools(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 25)

	rows, err := h.queryRows(ctx, `
		SELECT
			pair_address,
			token0_symbol,
			token1_symbol,
			token0_address,
			token1_address,
			liquidity_usd,
			volume_24h,
			volume_change_24h,
			fee_24h,
			pair_type,
			CASE
				WHEN liquidity_usd > 0 THEN ROUND(fee_24h / liquidity_usd * 365 * 100, 4)
				ELSE NULL
			END AS fee_yield_annual_pct
		FROM dex_pairs
		WHERE liquidity_usd IS NOT NULL
		ORDER BY liquidity_usd DESC
		LIMIT $1
	`, limit)
	if err != nil {
		dbError(c, err)
		return
	}

	total, _ := h.countQuery(ctx, `SELECT count(*) FROM dex_pairs`)
	respondList(c, rows, int(total))
}

// GetDeFiSummary returns the headline figures the dashboard shows at the top:
// protocol TVL and its 24 hour change, DEX liquidity and volume, and the pool
// and market counts.
//
// GET /defi/summary
func (h *Handler) GetDeFiSummary(c *gin.Context) {
	ctx := c.Request.Context()

	row, err := h.queryOne(ctx, `
		SELECT
			(SELECT COALESCE(SUM(tvl), 0) FROM defi_tvl) AS protocol_tvl,
			(SELECT COUNT(*) FROM defi_tvl) AS protocol_count,
			(SELECT COALESCE(SUM(volume_24h), 0) FROM defi_tvl) AS protocol_volume_24h,
			(SELECT COALESCE(SUM(liquidity_usd), 0) FROM dex_pairs) AS dex_liquidity,
			(SELECT COALESCE(SUM(volume_24h), 0) FROM dex_pairs) AS dex_volume_24h,
			(SELECT COALESCE(SUM(fee_24h), 0) FROM dex_pairs) AS dex_fees_24h,
			(SELECT COUNT(*) FROM dex_pairs) AS pair_count,
			(SELECT COUNT(*) FROM lending_rates) AS market_count
	`)
	if err != nil {
		dbError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"protocolTvl":        rowValue(row, "protocol_tvl"),
		"protocolCount":      rowValue(row, "protocol_count"),
		"protocolVolume24h":  rowValue(row, "protocol_volume_24h"),
		"dexLiquidity":       rowValue(row, "dex_liquidity"),
		"dexVolume24h":       rowValue(row, "dex_volume_24h"),
		"dexFees24h":         rowValue(row, "dex_fees_24h"),
		"pairCount":          rowValue(row, "pair_count"),
		"lendingMarketCount": rowValue(row, "market_count"),
	})
}
