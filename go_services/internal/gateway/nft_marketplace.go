package gateway

import (
	"net/http"
	"strings"

	"github.com/gin-gonic/gin"
)

// NFT sales come from the nft_sales table, which the metadata service fills
// from marketplace events. The marketplace page previously built its "recent
// sales" list by generating random token ids, random buyer and seller
// addresses and random transaction hashes in the browser, so every figure it
// showed was invented. These handlers read the indexed rows.

// GetNFTSales returns recent sales, optionally scoped to one collection.
//
// GET /nfts/:address/sales
// GET /nfts/sales
func (h *Handler) GetNFTSales(c *gin.Context) {
	ctx := c.Request.Context()
	collection := c.Param("address")
	limit := paramInt(c, "limit", 50)
	offset := paramOffset(c)

	var (
		rows  []map[string]interface{}
		err   error
		total int64
	)

	const columns = `id, token_address, token_id, seller, buyer, price, payment_token,
	                 marketplace, transaction_hash, block_number, log_index, block_timestamp`

	if collection != "" {
		rows, err = h.queryRows(ctx, `SELECT `+columns+` FROM nft_sales
			WHERE token_address = $1 ORDER BY block_number DESC, log_index DESC LIMIT $2 OFFSET $3`,
			collection, limit, offset)
		if err != nil {
			dbError(c, err)
			return
		}
		total, _ = h.countQuery(ctx, `SELECT count(*) FROM nft_sales WHERE token_address = $1`, collection)
	} else {
		rows, err = h.queryRows(ctx, `SELECT `+columns+` FROM nft_sales
			ORDER BY block_number DESC, log_index DESC LIMIT $1 OFFSET $2`, limit, offset)
		if err != nil {
			dbError(c, err)
			return
		}
		total, _ = h.countQuery(ctx, `SELECT count(*) FROM nft_sales`)
	}

	respondList(c, rows, int(total))
}

// GetNFTMarketplaceStats summarises trading across every indexed collection.
//
// GET /nfts/marketplace/stats
func (h *Handler) GetNFTMarketplaceStats(c *gin.Context) {
	ctx := c.Request.Context()

	row, err := h.queryOne(ctx, `
		SELECT
			COALESCE(SUM(volume_24h), 0)            AS volume_24h,
			COALESCE(SUM(floor_price * holders_count), 0) AS market_cap,
			COALESCE(AVG(floor_price), 0)           AS average_floor,
			COUNT(*)                                AS collections
		FROM nft_collections
		WHERE is_spam = false
	`)
	if err != nil {
		dbError(c, err)
		return
	}

	sales, err := h.queryOne(ctx, `
		SELECT
			COUNT(*)                            AS sales_24h,
			COALESCE(SUM(price), 0)             AS volume_24h_eth,
			COALESCE(AVG(price), 0)             AS average_price
		FROM nft_sales
		WHERE block_timestamp >= NOW() - INTERVAL '24 hours'
	`)
	if err != nil {
		dbError(c, err)
		return
	}

	listings, err := h.queryOne(ctx, `SELECT count(*) AS active_listings FROM nft_owners`)
	if err != nil {
		dbError(c, err)
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"totalVolume24h": rowValue(row, "volume_24h"),
		"marketCap":      rowValue(row, "market_cap"),
		"averageFloor":   rowValue(row, "average_floor"),
		"collections":    rowValue(row, "collections"),
		"totalSales24h":  rowValue(sales, "sales_24h"),
		"salesVolume24h": rowValue(sales, "volume_24h_eth"),
		"averagePrice":   rowValue(sales, "average_price"),
		"activeListings": rowValue(listings, "active_listings"),
	})
}

// GetNFTMarketplaceCollections lists collections with the trading columns the
// marketplace grid renders, sorted by 24h volume.
//
// GET /nfts/marketplace/collections
func (h *Handler) GetNFTMarketplaceCollections(c *gin.Context) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", 50)
	offset := paramOffset(c)

	rows, err := h.queryRows(ctx, `
		SELECT c.address, c.name, c.symbol, c.contract_type, c.total_supply,
		       c.holders_count, c.transfers_count, c.floor_price,
		       c.floor_price_change_24h, c.volume_24h, c.volume_change_24h,
		       c.average_price_24h, c.market_cap, c.description,
		       c.image_url, c.banner_url, c.social, c.is_verified,
		       COALESCE(s.sales_24h, 0) AS sales_24h
		FROM nft_collections c
		LEFT JOIN LATERAL (
			SELECT COUNT(*) AS sales_24h
			FROM nft_sales
			WHERE token_address = c.address
			  AND block_timestamp >= NOW() - INTERVAL '24 hours'
		) s ON TRUE
		WHERE c.is_spam = false
		ORDER BY c.volume_24h DESC NULLS LAST, c.floor_price DESC NULLS LAST
		LIMIT $1 OFFSET $2
	`, limit, offset)
	if err != nil {
		dbError(c, err)
		return
	}
	total, _ := h.countQuery(ctx, `SELECT count(*) FROM nft_collections WHERE is_spam = false`)
	respondList(c, rows, int(total))
}

// GetNFTCollectionSalesSeries returns per-day sale counts and volume for one
// collection, which is what the marketplace chart plots.
//
// GET /nfts/:address/sales-series
func (h *Handler) GetNFTCollectionSalesSeries(c *gin.Context) {
	ctx := c.Request.Context()
	collection := c.Param("address")
	if strings.TrimSpace(collection) == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "collection address is required"})
		return
	}
	limit := paramInt(c, "limit", 30)

	rows, err := h.queryRows(ctx, `
		SELECT to_char(block_timestamp, 'YYYY-MM-DD') AS date,
		       COUNT(*)                               AS sales,
		       COALESCE(SUM(price), 0)                AS volume
		FROM nft_sales
		WHERE token_address = $1
		GROUP BY 1
		ORDER BY 1 DESC
		LIMIT $2
	`, collection, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}

// GetNFTCollectionHoldersSeries reports the holder count per day, taken from
// nft_holders rather than invented.
//
// GET /nfts/:address/holders-series
func (h *Handler) GetNFTCollectionHoldersSeries(c *gin.Context) {
	ctx := c.Request.Context()
	collection := c.Param("address")
	if strings.TrimSpace(collection) == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "collection address is required"})
		return
	}
	limit := paramInt(c, "limit", 30)

	rows, err := h.queryRows(ctx, `
		SELECT to_char(created_at, 'YYYY-MM-DD') AS date,
		       COUNT(DISTINCT address)           AS holders
		FROM nft_holders
		WHERE collection_address = $1
		GROUP BY 1
		ORDER BY 1 DESC
		LIMIT $2
	`, collection, limit)
	if err != nil {
		dbError(c, err)
		return
	}
	respondList(c, rows, len(rows))
}
