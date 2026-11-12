package gateway

import (
	"math"
	"net/http"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
)

// Gas network statistics.
//
// The gas tracker page previously built 24 hours of price history, a two-hour
// forecast and a cross-chain comparison table in the browser with Math.random.
// The comparison table was the worst of it: it printed invented gas prices,
// volumes and 24h changes for Ethereum, Base, Arbitrum, Optimism and Polygon on
// TigerScan, which reads as if TigerScan measured those chains.
//
// Everything below is computed from the two tables that actually hold this
// chain's fee data: blocks (gas_used, gas_limit, base_fee_per_gas) and
// gas_prices (the samples the tracker service writes).

// gasPriceToGwei converts wei to gwei, rounded to two decimals, as a string so
// the client can display it without re-deriving the scale.
func gasPriceToGwei(wei float64) string {
	gwei := wei / 1e9
	return strconv.FormatFloat(math.Round(gwei*100)/100, 'f', 2, 64)
}

// GetGasNetworkStats returns current fees, averages, volatility and the latest
// block's fill, all derived from indexed data.
//
// GET /gas/network-stats
func (h *Handler) GetGasNetworkStats(c *gin.Context) {
	ctx := c.Request.Context()

	prices, err := h.queryOne(ctx, `
		SELECT
			COALESCE(PERCENTILE_CONT(0.25) WITHIN GROUP (ORDER BY gas_price), 0) AS slow,
			COALESCE(PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY gas_price), 0) AS standard,
			COALESCE(PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY gas_price), 0) AS fast,
			COALESCE(STDDEV_POP(gas_price), 0) AS volatility,
			COUNT(*) AS samples
		FROM (
			SELECT gas_price FROM gas_prices ORDER BY timestamp DESC LIMIT 100
		) recent
	`)
	if err != nil {
		dbError(c, err)
		return
	}

	averages, err := h.queryOne(ctx, `
		SELECT
			COALESCE(AVG(gas_price) FILTER (WHERE timestamp >= $1), 0) AS average_24h,
			COALESCE(AVG(gas_price) FILTER (WHERE timestamp >= $2), 0) AS average_7d,
			COALESCE(AVG(gas_price) FILTER (WHERE timestamp >= $3), 0) AS first_half,
			COALESCE(AVG(gas_price) FILTER (WHERE timestamp >= $1), 0) AS second_half,
			COUNT(*) AS total_samples
		FROM gas_prices
		WHERE timestamp >= $2
	`, nowSeconds()-24*60*60, nowSeconds()-7*24*60*60, nowSeconds()-12*60*60)
	if err != nil {
		dbError(c, err)
		return
	}

	// Trend compares the last 12 hours against the 12 before it. Two averages
	// over equal windows is a weaker signal than a fitted slope but it is not
	// sensitive to a single outlier sample, which a slope would be.
	trend := "stable"
	first, second := asFloat(rowValue(averages, "first_half")), asFloat(rowValue(averages, "second_half"))
	if first > 0 {
		ratio := second / first
		switch {
		case ratio > 1.1:
			trend = "up"
		case ratio < 0.9:
			trend = "down"
		}
	}

	block, err := h.queryOne(ctx, `
		SELECT number, gas_used, gas_limit, base_fee_per_gas, timestamp
		FROM blocks ORDER BY number DESC LIMIT 1
	`)
	if err != nil {
		dbError(c, err)
		return
	}

	gasUsed := asFloat(rowValue(block, "gas_used"))
	gasLimit := asFloat(rowValue(block, "gas_limit"))
	utilization := 0.0
	if gasLimit > 0 {
		utilization = math.Round(gasUsed/gasLimit*10000) / 100
	}

	c.JSON(http.StatusOK, gin.H{
		"current": gin.H{
			"slow":     gasPriceToGwei(asFloat(rowValue(prices, "slow"))),
			"standard": gasPriceToGwei(asFloat(rowValue(prices, "standard"))),
			"fast":     gasPriceToGwei(asFloat(rowValue(prices, "fast"))),
		},
		"average24h": gasPriceToGwei(asFloat(rowValue(averages, "average_24h"))),
		"average7d":  gasPriceToGwei(asFloat(rowValue(averages, "average_7d"))),
		"volatility": gasPriceToGwei(asFloat(rowValue(prices, "volatility"))),
		"trend":      trend,
		"samples":    rowValue(prices, "samples"),
		"latestBlock": gin.H{
			"number":      rowValue(block, "number"),
			"gasUsed":     rowValue(block, "gas_used"),
			"gasLimit":    rowValue(block, "gas_limit"),
			"baseFee":     gasPriceToGwei(asFloat(rowValue(block, "base_fee_per_gas"))),
			"utilization": utilization,
			"timestamp":   rowValue(block, "timestamp"),
		},
	})
}

// GetGasPriceHistorySeries returns sampled gas prices in a shape a chart can
// plot directly, with the wei value converted to gwei server-side.
//
// GET /gas/price-series
func (h *Handler) GetGasPriceHistorySeries(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 288)

	rows, err := h.queryRows(ctx, `
		SELECT timestamp, gas_price, base_fee, priority_fee, gas_used, gas_limit
		FROM gas_prices
		ORDER BY timestamp DESC
		LIMIT $1
	`, limit)
	if err != nil {
		dbError(c, err)
		return
	}

	series := make([]gin.H, 0, len(rows))
	for _, row := range rows {
		series = append(series, gin.H{
			"timestamp":   rowValue(row, "timestamp"),
			"gasPrice":    gasPriceToGwei(asFloat(rowValue(row, "gas_price"))),
			"baseFee":     gasPriceToGwei(asFloat(rowValue(row, "base_fee"))),
			"priorityFee": gasPriceToGwei(asFloat(rowValue(row, "priority_fee"))),
			"gasUsed":     rowValue(row, "gas_used"),
			"gasLimit":    rowValue(row, "gas_limit"),
		})
	}

	// Oldest first so the chart reads left to right without the client sorting.
	for i, j := 0, len(series)-1; i < j; i, j = i+1, j-1 {
		series[i], series[j] = series[j], series[i]
	}

	c.JSON(http.StatusOK, gin.H{"items": series, "total": len(series)})
}

// asFloat coerces the numeric types pgx can return for a column into float64.
func asFloat(v interface{}) float64 {
	switch value := v.(type) {
	case nil:
		return 0
	case float64:
		return value
	case float32:
		return float64(value)
	case int64:
		return float64(value)
	case int32:
		return float64(value)
	case int:
		return float64(value)
	case string:
		f, err := strconv.ParseFloat(value, 64)
		if err != nil {
			return 0
		}
		return f
	default:
		return 0
	}
}

func nowSeconds() int64 {
	return time.Now().Unix()
}
