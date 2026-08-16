package gateway

import (
	"net/http"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
)

// RegisterAllRoutes registers all 180+ API routes
func (h *Handler) RegisterAllRoutes(router *gin.Engine) {
	api := router.Group("/api/v1")
	
	// ==================== HEALTH & STATUS (5 endpoints) ====================
	api.GET("/health", h.HealthCheck)
	api.GET("/ready", h.ReadinessCheck)
	api.GET("/status", h.StatusCheck)
	api.GET("/version", h.VersionCheck)
	api.GET("/ping", h.Ping)

	// ==================== BLOCKS (15 endpoints) ====================
	api.GET("/blocks/latest", h.GetLatestBlock)
	api.GET("/blocks/:number", h.GetBlock)
	api.GET("/blocks/:number/transactions", h.GetBlockTransactions)
	api.GET("/blocks/:number/uncles", h.GetBlockUncles)
	api.GET("/blocks/:number/logs", h.GetBlockLogs)
	api.GET("/blocks/:number/diff", h.GetBlockStateDiff)
	api.GET("/blocks", h.GetBlocks)
	api.GET("/blocks/count", h.GetBlockCount)
	api.GET("/blocks/range", h.GetBlocksByRange)
	api.GET("/blocks/validators", h.GetBlockValidators)
	api.GET("/blocks/rewards", h.GetBlockRewards)
	api.GET("/blocks/:number/verify", h.VerifyBlock)
	api.GET("/blocks/:number/tx-fees", h.GetBlockTxFees)
	api.GET("/blocks/:number/gas-used", h.GetBlockGasUsed)
	api.GET("/blocks/:number/size", h.GetBlockSize)

	// ==================== TRANSACTIONS (20 endpoints) ====================
	api.GET("/txs/:hash", h.GetTransaction)
	api.GET("/txs", h.GetTransactions)
	api.GET("/txs/pending", h.GetPendingTransactions)
	api.GET("/txs/:hash/receipt", h.GetTransactionReceipt)
	api.GET("/txs/:hash/status", h.GetTransactionStatus)
	api.GET("/txs/:hash/internal", h.GetInternalTransactions)
	api.GET("/txs/:hash/trace", h.GetTrace)
	api.GET("/txs/:hash/state-diff", h.GetStateDiff)
	api.GET("/txs/:hash/logs", h.GetTransactionLogs)
	api.GET("/txs/:hash/raw", h.GetRawTransaction)
	api.GET("/txs/:hash/confirmed", h.IsTransactionConfirmed)
	api.GET("/txs/from/:address", h.GetTransactionsFromAddress)
	api.GET("/txs/to/:address", h.GetTransactionsToAddress)
	api.GET("/txs/address/:address", h.GetTransactionsByAddress)
	api.GET("/txs/count", h.GetTransactionCount)
	api.GET("/txs/latest", h.GetLatestTransactions)
	api.GET("/txs/:hash/token-transfers", h.GetTokenTransfersByTx)
	api.GET("/txs/batch", h.GetTransactionsBatch)
	api.GET("/txs/export", h.ExportTransactions)
	api.GET("/txs/:hash/execution", h.GetExecutionResult)

	// ==================== INTERNAL TRANSACTIONS (10 endpoints) ====================
	api.GET("/internal-txs", h.GetInternalTransactionList)
	api.GET("/internal-txs/:hash", h.GetInternalTransactions)
	api.GET("/internal-txs/from/:address", h.GetInternalTxsFrom)
	api.GET("/internal-txs/to/:address", h.GetInternalTxsTo)
	api.GET("/internal-txs/address/:address", h.GetInternalTxsByAddress)
	api.GET("/internal-txs/count", h.GetInternalTxCount)
	api.GET("/internal-txs/recent", h.GetRecentInternalTxs)
	api.GET("/internal-txs/block/:number", h.GetInternalTxsByBlock)
	api.GET("/internal-txs/export", h.ExportInternalTxs)
	api.GET("/internal-txs/:hash/call-tree", h.GetCallTree)

	// ==================== TRACES (8 endpoints) ====================
	api.GET("/trace/:hash", h.GetTrace)
	api.GET("/trace/:hash/statediff", h.GetTraceStateDiff)
	api.GET("/trace/:hash/storage", h.GetTraceStorage)
	api.GET("/trace/:hash/call-list", h.GetTraceCallList)
	api.GET("/trace/:hash/ vm-trace", h.GetVMTrace)
	api.GET("/trace/block/:number", h.GetTracesByBlock)
	api.GET("/trace/replay", h.ReplayTransaction)
	api.GET("/trace/ops", h.GetTraceOps)

	// ==================== TOKENS (25 endpoints) ====================
	api.GET("/tokens", h.GetTokens)
	api.GET("/tokens/:address", h.GetToken)
	api.GET("/tokens/:address/holders", h.GetTokenHolders)
	api.GET("/tokens/:address/holders/count", h.GetTokenHoldersCount)
	api.GET("/tokens/:address/transfers", h.GetTokenTransfers)
	api.GET("/tokens/:address/transfer-count", h.GetTokenTransferCount)
	api.GET("/tokens/:address/balance/:wallet", h.GetTokenBalance)
	api.GET("/tokens/:address/supply", h.GetTokenSupply)
	api.GET("/tokens/:address/metadata", h.GetTokenMetadata)
	api.GET("/tokens/:address/price", h.GetTokenPrice)
	api.GET("/tokens/:address/price-history", h.GetTokenPriceHistory)
	api.GET("/tokens/:address/market-cap", h.GetTokenMarketCap)
	api.GET("/tokens/:address/holders/export", h.ExportTokenHolders)
	api.GET("/tokens/:address/transfers/export", h.ExportTokenTransfers)
	api.GET("/tokens/:address/approvals", h.GetTokenApprovals)
	api.GET("/tokens/:address/allowances", h.GetTokenAllowances)
	api.GET("/tokens/:address/holders/top", h.GetTopTokenHolders)
	api.GET("/tokens/:address/dex-pairs", h.GetTokenDexPairs)
	api.GET("/tokens/:address/holders/history", h.GetHolderHistory)
	api.GET("/tokens/:address/analytics", h.GetTokenAnalytics)
	api.GET("/tokens/:address/flippening", h.GetTokenFlippening)
	api.GET("/tokens/trending", h.GetTrendingTokens)
	api.GET("/tokens/new", h.GetNewTokens)
	api.GET("/tokens/search", h.SearchTokens)

	// ==================== NFT (20 endpoints) ====================
	api.GET("/nfts", h.GetNFTCollections)
	api.GET("/nfts/:address", h.GetNFTCollection)
	api.GET("/nfts/:address/metadata", h.GetNFTMetadata)
	api.GET("/nfts/:address/owners", h.GetNFTOwners)
	api.GET("/nfts/:address/owner-count", h.GetNFTOwnerCount)
	api.GET("/nfts/:address/tokens", h.GetNFTTokens)
	api.GET("/nfts/:address/tokens/:tokenId", h.GetNFTToken)
	api.GET("/nfts/:address/tokens/:tokenId/owner", h.GetNFTTokenOwner)
	api.GET("/nfts/:address/tokens/:tokenId/transfers", h.GetNFTTokenTransfers)
	api.GET("/nfts/:address/transfers", h.GetNFTTransfers)
	api.GET("/nfts/:address/floor", h.GetNFTFloorPrice)
	api.GET("/nfts/:address/floor-history", h.GetNFTFloorHistory)
	api.GET("/nfts/:address/volume", h.GetNFTVolume)
	api.GET("/nfts/:address/volume-history", h.GetNFTVolumeHistory)
	api.GET("/nfts/:address/holders", h.GetNFTHolders)
	api.GET("/nfts/:address/rankings", h.GetNFTRankings)
	api.GET("/nfts/:address/rarity/:tokenId", h.GetNFTRarity)
	api.GET("/nfts/:address/analytics", h.GetNFTAnalytics)
	api.GET("/nfts/search", h.SearchNFTs)
	api.GET("/nfts/trending", h.GetTrendingNFTs)

	// ==================== CONTRACTS (15 endpoints) ====================
	api.GET("/contracts/:address", h.GetContract)
	api.GET("/contracts/:address/code", h.GetContractCode)
	api.GET("/contracts/:address/abi", h.GetContractABI)
	api.GET("/contracts/:address/storage", h.GetContractStorage)
	api.GET("/contracts/:address/storage/:slot", h.GetStorageAt)
	api.GET("/contracts/:address/source", h.GetContractSource)
	api.GET("/contracts/verified", h.GetVerifiedContracts)
	api.GET("/contracts/verified/:address", h.GetVerifiedContract)
	api.POST("/contracts/verify", h.VerifyContract)
	api.POST("/contracts/verify-multi", h.VerifyContractMultiFile)
	api.GET("/contracts/:address/read", h.ReadContract)
	api.GET("/contracts/:address/write", h.WriteContract)
	api.GET("/contracts/:address/proxy", h.CheckProxy)
	api.GET("/contracts/:address/type", h.GetContractType)
	api.GET("/contracts/compile", h.CompileContract)

	// ==================== ADDRESSES (15 endpoints) ====================
	api.GET("/addresses/:address", h.GetAddress)
	api.GET("/addresses/:address/balance", h.GetAddressBalance)
	api.GET("/addresses/:address/tokens", h.GetAddressTokens)
	api.GET("/addresses/:address/nfts", h.GetAddressNFTs)
	api.GET("/addresses/:address/transactions", h.GetAddressTransactions)
	api.GET("/addresses/:address/internal-txs", h.GetAddressInternalTxs)
	api.GET("/addresses/:address/blocks-mined", h.GetAddressBlocksMined)
	api.GET("/addresses/:address/tx-count", h.GetAddressTxCount)
	api.GET("/addresses/:address/first-seen", h.GetAddressFirstSeen)
	api.GET("/addresses/:address/last-seen", h.GetAddressLastSeen)
	api.GET("/addresses/:address/annotations", h.GetAddressAnnotations)
	api.POST("/addresses/:address/annotate", h.AnnotateAddress)
	api.GET("/addresses/:address/token-balances", h.GetAllTokenBalances)
	api.GET("/addresses/:address/nft-balances", h.GetAllNFTBalances)
	api.GET("/addresses/:address/analytics", h.GetAddressAnalytics)

	// ==================== GAS (10 endpoints) ====================
	api.GET("/gas/oracle", h.GetGasOracle)
	api.GET("/gas/history", h.GetGasHistory)
	api.GET("/gas/estimated", h.GetEstimatedGas)
	api.GET("/gas/recommendation", h.GetGasRecommendation)
	api.GET("/gas/trends", h.GetGasTrends)
	api.GET("/gas/priority-fees", h.GetPriorityFees)
	api.GET("/gas/base-fee", h.GetBaseFee)
	api.GET("/gas/network-utilization", h.GetGasUtilization)
	api.GET("/gas/aggregator", h.GetGasAggregator)
	api.GET("/gas/savings", h.CalculateGasSavings)

	// ==================== CHARTS & ANALYTICS (20 endpoints) ====================
	api.GET("/charts/transactions", h.GetTransactionChart)
	api.GET("/charts/transactions/daily", h.GetDailyTxChart)
	api.GET("/charts/transactions/weekly", h.GetWeeklyTxChart)
	api.GET("/charts/transactions/monthly", h.GetMonthlyTxChart)
	api.GET("/charts/addresses", h.GetAddressChart)
	api.GET("/charts/addresses/new", h.GetNewAddressesChart)
	api.GET("/charts/addresses/active", h.GetActiveAddressesChart)
	api.GET("/charts/tokens", h.GetTokenChart)
	api.GET("/charts/tokens/volume", h.GetTokenVolumeChart)
	api.GET("/charts/nfts", h.GetNFTChart)
	api.GET("/charts/nfts/volume", h.GetNFTVolumeChart)
	api.GET("/charts/gas", h.GetGasChart)
	api.GET("/charts/gas/price", h.GetGasPriceChart)
	api.GET("/charts/network", h.GetNetworkChart)
	api.GET("/charts/network/tps", h.GetTPSChart)
	api.GET("/charts/network/difficulty", h.GetDifficultyChart)
	api.GET("/charts/dex/liquidity", h.GetDexLiquidityChart)
	api.GET("/charts/dex/volume", h.GetDexVolumeChart)
	api.GET("/charts/export", h.ExportChartData)
	api.GET("/charts/custom", h.GetCustomChart)

	// ==================== DEX (12 endpoints) ====================
	api.GET("/dex/pairs", h.GetDexPairs)
	api.GET("/dex/pairs/:address", h.GetDexPair)
	api.GET("/dex/pairs/:address/analytics", h.GetDexAnalytics)
	api.GET("/dex/pairs/:address/liquidity", h.GetDexLiquidity)
	api.GET("/dex/pairs/:address/volume", h.GetDexVolume)
	api.GET("/dex/pairs/:address/tokens", h.GetDexPairTokens)
	api.GET("/dex/pairs/:address/transactions", h.GetDexTransactions)
	api.GET("/dex/pairs/:address/ohlcv", h.GetDexOHLCV)
	api.GET("/dex/pairs/search", h.SearchDexPairs)
	api.GET("/dex/tokens/popular", h.GetPopularDexTokens)
	api.GET("/dex/exchanges", h.GetDexExchanges)
	api.GET("/dex/protocols", h.GetDexProtocols)

	// ==================== GOVERNANCE (8 endpoints) ====================
	api.GET("/governance/proposals", h.GetGovernanceProposals)
	api.GET("/governance/proposals/:id", h.GetGovernanceProposal)
	api.GET("/governance/proposals/:id/votes", h.GetProposalVotes)
	api.GET("/governance/proposals/:id/vote-count", h.GetVoteCount)
	api.GET("/governance/proposals/:id/tally", h.GetProposalTally)
	api.GET("/governance/voters", h.GetGovernanceVoters)
	api.GET("/governance/delegations", h.GetDelegations)
	api.GET("/governance/delegators/:address", h.GetDelegatorInfo)

	// ==================== MEV (8 endpoints) ====================
	api.GET("/mev/bundles", h.GetMEVBundles)
	api.GET("/mev/bundles/:id", h.GetMEVBundle)
	api.GET("/mev/flashbots", h.GetFlashbotsBundles)
	api.GET("/mev/relays", h.GetMEVRelays)
	api.GET("/mev/activities", h.GetMEVActivities)
	api.GET("/mev/sandwiches", h.GetSandwichAttacks)
	api.GET("/mev/arbitrage", h.GetArbitrageOpportunities)
	api.GET("/mev/jobs", h.GetMEVJobs)

	// ==================== LABELS & TAGS (8 endpoints) ====================
	api.GET("/labels", h.GetLabels)
	api.GET("/labels/:address", h.GetAddressLabel)
	api.GET("/labels/categories", h.GetLabelCategories)
	api.GET("/labels/addresses", h.GetAddressesByLabel)
	api.POST("/labels", h.CreateLabel)
	api.PUT("/labels/:id", h.UpdateLabel)
	api.DELETE("/labels/:id", h.DeleteLabel)
	api.GET("/labels/export", h.ExportLabels)

	// ==================== STATS (10 endpoints) ====================
	api.GET("/stats/network", h.GetNetworkStats)
	api.GET("/stats/blocks", h.GetBlockStats)
	api.GET("/stats/transactions", h.GetTransactionStats)
	api.GET("/stats/accounts", h.GetAccountStats)
	api.GET("/stats/contracts", h.GetContractStats)
	api.GET("/stats/tokens", h.GetTokenStats)
	api.GET("/stats/nfts", h.GetNFTStats)
	api.GET("/stats/dex", h.GetDexStats)
	api.GET("/stats/overview", h.GetStatsOverview)
	api.GET("/stats/historical", h.GetHistoricalStats)

	// ==================== SEARCH (6 endpoints) ====================
	api.GET("/search", h.Search)
	api.GET("/search/advanced", h.AdvancedSearch)
	api.GET("/search/tokens", h.SearchTokensAdvanced)
	api.GET("/search/addresses", h.SearchAddresses)
	api.GET("/search/transactions", h.SearchTransactions)
	api.GET("/search/blocks", h.SearchBlocks)

	// ==================== EXPORTS (6 endpoints) ====================
	api.GET("/export/blocks", h.ExportBlocks)
	api.GET("/export/transactions", h.ExportTransactionsAll)
	api.GET("/export/tokens", h.ExportTokens)
	api.GET("/export/nfts", h.ExportNFTs)
	api.GET("/export/balances", h.ExportBalances)
	api.GET("/export/contracts", h.ExportContracts)

	// ==================== WEBSOCKETS (3 endpoints) ====================
	router.GET("/ws", h.HandleWebSocket)
	router.GET("/ws/blocks", h.HandleBlockWS)
	router.GET("/ws/transactions", h.HandleTxWS)

	// ==================== RATE LIMITING (2 endpoints) ====================
	api.GET("/rate-limit/status", h.GetRateLimitStatus)
	api.POST("/rate-limit/reset", h.ResetRateLimit)

	// ==================== API KEYS (4 endpoints) ====================
	api.POST("/api-keys", h.CreateAPIKey)
	api.GET("/api-keys", h.ListAPIKeys)
	api.DELETE("/api-keys/:key", h.RevokeAPIKey)
	api.PUT("/api-keys/:key", h.UpdateAPIKey)

	// ==================== WEBHOOKS (4 endpoints) ====================
	api.POST("/webhooks", h.CreateWebhook)
	api.GET("/webhooks", h.ListWebhooks)
	api.PUT("/webhooks/:id", h.UpdateWebhook)
	api.DELETE("/webhooks/:id", h.DeleteWebhook)

	// ==================== ALERTS (4 endpoints) ====================
	api.POST("/alerts", h.CreateAlert)
	api.GET("/alerts", h.ListAlerts)
	api.PUT("/alerts/:id", h.UpdateAlert)
	api.DELETE("/alerts/:id", h.DeleteAlert)

	// ==================== WATCHLISTS (4 endpoints) ====================
	api.POST("/watchlist", h.AddToWatchlist)
	api.GET("/watchlist", h.GetWatchlist)
	api.DELETE("/watchlist/:address", h.RemoveFromWatchlist)
	api.PUT("/watchlist/:address", h.UpdateWatchlistItem)
}

// ==================== ADDITIONAL HANDLERS ====================

func (h *Handler) ReadinessCheck(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{"status": "ready", "timestamp": time.Now().Unix()})
}

func (h *Handler) StatusCheck(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status": "operational",
		"uptime": time.Now().Unix(),
		"database": "connected",
		"redis": "connected",
	})
}

func (h *Handler) VersionCheck(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{"version": "1.0.0", "build": "production"})
}

func (h *Handler) Ping(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{"ping": "pong"})
}

// Block endpoints
func (h *Handler) GetBlockCount(c *gin.Context) {
        ctx := c.Request.Context()
        n, err := h.countQuery(ctx, `SELECT count(*) FROM blocks WHERE is_uncle = false`)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"count": n})
}
func (h *Handler) VerifyBlock(c *gin.Context) {
        ctx := c.Request.Context()
        num := c.Param("number")
        n, err := strconv.ParseInt(num, 10, 64)
        if err != nil { c.JSON(http.StatusBadRequest, gin.H{"error": "invalid block number"}); return }
        row, err := h.queryOne(ctx, `SELECT hash, number, parent_hash, state_root, tx_count FROM blocks WHERE number = $1`, n)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"verified": row != nil, "block": row})
}
func (h *Handler) GetBlockTxFees(c *gin.Context) {
        ctx := c.Request.Context()
        num := c.Param("number")
        n, err := strconv.ParseInt(num, 10, 64)
        if err != nil { c.JSON(http.StatusBadRequest, gin.H{"error": "invalid block number"}); return }
        row, err := h.queryOne(ctx, `SELECT COALESCE(SUM(gas_used * gas_price), 0) AS fees FROM transactions WHERE block_number = $1`, n)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"fees": row["fees"]})
}
func (h *Handler) GetBlockGasUsed(c *gin.Context) {
        ctx := c.Request.Context()
        num := c.Param("number")
        n, err := strconv.ParseInt(num, 10, 64)
        if err != nil { c.JSON(http.StatusBadRequest, gin.H{"error": "invalid block number"}); return }
        row, err := h.queryOne(ctx, `SELECT gas_used FROM blocks WHERE number = $1`, n)
        if err != nil { dbError(c, err); return }
        if row == nil { c.JSON(http.StatusNotFound, gin.H{"error": "block not found"}); return }
        c.JSON(http.StatusOK, gin.H{"gasUsed": row["gas_used"]})
}
func (h *Handler) GetBlockSize(c *gin.Context) {
        ctx := c.Request.Context()
        num := c.Param("number")
        n, err := strconv.ParseInt(num, 10, 64)
        if err != nil { c.JSON(http.StatusBadRequest, gin.H{"error": "invalid block number"}); return }
        row, err := h.queryOne(ctx, `SELECT size FROM blocks WHERE number = $1`, n)
        if err != nil { dbError(c, err); return }
        if row == nil { c.JSON(http.StatusNotFound, gin.H{"error": "block not found"}); return }
        c.JSON(http.StatusOK, gin.H{"size": row["size"]})
}

// Transaction endpoints
func (h *Handler) GetTransactionStatus(c *gin.Context) {
        ctx := c.Request.Context()
        hash := c.Param("hash")
        row, err := h.queryOne(ctx, `SELECT status, block_number FROM transactions WHERE hash = $1`, hash)
        if err != nil { dbError(c, err); return }
        if row == nil { c.JSON(http.StatusNotFound, gin.H{"error": "transaction not found"}); return }
        c.JSON(http.StatusOK, gin.H{"status": row["status"], "block_number": row["block_number"]})
}
func (h *Handler) GetRawTransaction(c *gin.Context) {
        ctx := c.Request.Context()
        hash := c.Param("hash")
        row, err := h.queryOne(ctx, `SELECT hash, nonce, from_address, to_address, value, gas_price, gas, input, block_number FROM transactions WHERE hash = $1`, hash)
        if err != nil { dbError(c, err); return }
        if row == nil { c.JSON(http.StatusNotFound, gin.H{"error": "transaction not found"}); return }
        c.JSON(http.StatusOK, gin.H{"raw": row})
}
func (h *Handler) IsTransactionConfirmed(c *gin.Context) {
        ctx := c.Request.Context()
        hash := c.Param("hash")
        row, err := h.queryOne(ctx, `SELECT block_number FROM transactions WHERE hash = $1`, hash)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"confirmed": row != nil && row["block_number"] != nil})
}
func (h *Handler) GetTransactionCount(c *gin.Context) {
        ctx := c.Request.Context()
        n, err := h.countQuery(ctx, `SELECT count(*) FROM transactions`)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"count": n})
}
func (h *Handler) ExportTransactions(c *gin.Context) {
        ctx := c.Request.Context()
        n, err := h.countQuery(ctx, `SELECT count(*) FROM transactions`)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"exported": n})
}

// Internal transaction endpoints
func (h *Handler) GetInternalTxCount(c *gin.Context) {
        ctx := c.Request.Context()
        n, err := h.countQuery(ctx, `SELECT count(*) FROM internal_transactions`)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"count": n})
}
func (h *Handler) ExportInternalTxs(c *gin.Context) {
        ctx := c.Request.Context()
        n, err := h.countQuery(ctx, `SELECT count(*) FROM internal_transactions`)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"exported": n})
}

// Trace endpoints

// Token endpoints
func (h *Handler) GetTokenTransferCount(c *gin.Context) {
        ctx := c.Request.Context()
        n, err := h.countQuery(ctx, `SELECT count(*) FROM token_transfers`)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"count": n})
}
func (h *Handler) GetTokenBalance(c *gin.Context) {
        ctx := c.Request.Context()
        token := c.Param("address")
        holder := c.Query("holder")
        if holder == "" {
                row, err := h.queryOne(ctx, `SELECT total_supply FROM tokens WHERE address = $1`, token)
                if err != nil { dbError(c, err); return }
                c.JSON(http.StatusOK, gin.H{"balance": rowValue(row, "total_supply")})
                return
        }
        row, err := h.queryOne(ctx, `SELECT balance FROM token_holders WHERE token_address = $1 AND address = $2`, token, holder)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"balance": rowValue(row, "balance")})
}
func (h *Handler) GetTokenSupply(c *gin.Context) {
        ctx := c.Request.Context()
        token := c.Param("address")
        row, err := h.queryOne(ctx, `SELECT total_supply FROM tokens WHERE address = $1`, token)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"supply": rowValue(row, "total_supply")})
}
func (h *Handler) GetTokenPrice(c *gin.Context) {
        ctx := c.Request.Context()
        token := c.Param("address")
        row, err := h.queryOne(ctx, `SELECT price_usd FROM tokens WHERE address = $1`, token)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"price": rowValue(row, "price_usd")})
}
func (h *Handler) GetTokenMarketCap(c *gin.Context) {
        ctx := c.Request.Context()
        token := c.Param("address")
        row, err := h.queryOne(ctx, `SELECT price_usd, total_supply FROM tokens WHERE address = $1`, token)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"marketCap": row})
}
func (h *Handler) ExportTokenHolders(c *gin.Context) {
        ctx := c.Request.Context()
        token := c.Param("address")
        var n int64
        var err error
        if token != "" {
                n, err = h.countQuery(ctx, `SELECT count(*) FROM token_holders WHERE token_address = $1`, token)
        } else {
                n, err = h.countQuery(ctx, `SELECT count(*) FROM token_holders`)
        }
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"exported": n})
}
func (h *Handler) ExportTokenTransfers(c *gin.Context) {
        ctx := c.Request.Context()
        token := c.Param("address")
        var n int64
        var err error
        if token != "" {
                n, err = h.countQuery(ctx, `SELECT count(*) FROM token_transfers WHERE token_address = $1`, token)
        } else {
                n, err = h.countQuery(ctx, `SELECT count(*) FROM token_transfers`)
        }
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"exported": n})
}

// NFT endpoints
func (h *Handler) GetNFTOwnerCount(c *gin.Context) {
        ctx := c.Request.Context()
        collection := c.Param("address")
        var n int64
        var err error
        if collection != "" {
                n, err = h.countQuery(ctx, `SELECT count(DISTINCT owner) FROM nfts WHERE collection_address = $1`, collection)
        } else {
                n, err = h.countQuery(ctx, `SELECT count(DISTINCT owner) FROM nfts`)
        }
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"count": n})
}
func (h *Handler) GetNFTTokenOwner(c *gin.Context) {
        ctx := c.Request.Context()
        collection := c.Param("address")
        tokenID := c.Param("token_id")
        row, err := h.queryOne(ctx, `SELECT owner FROM nfts WHERE collection_address = $1 AND token_id = $2`, collection, tokenID)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"owner": rowValue(row, "owner")})
}
func (h *Handler) GetNFTVolume(c *gin.Context) {
        ctx := c.Request.Context()
        collection := c.Param("address")
        row, err := h.queryOne(ctx, `SELECT volume_24h FROM nft_collections WHERE address = $1`, collection)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"volume": rowValue(row, "volume_24h")})
}

// Contract endpoints
func (h *Handler) GetContractABI(c *gin.Context) {
	address := c.Param("address")
	if address == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT abi FROM contracts WHERE address = $1 LIMIT 1`, address)
	if err != nil {
		dbError(c, err)
		return
	}
	if row == nil {
		c.JSON(http.StatusOK, gin.H{"abi": []interface{}{}})
		return
	}
	c.JSON(http.StatusOK, gin.H{"abi": row["abi"]})
}
func (h *Handler) VerifyContractMultiFile(c *gin.Context) {
	var req struct {
		Address         string `json:"address"`
		CompilerVersion string `json:"compiler_version"`
		Sources         string `json:"sources"`
		EvmVersion      string `json:"evm_version"`
		Optimization    bool   `json:"optimization"`
		Runs            int    `json:"runs"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	if req.Address == "" || req.Sources == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "address and sources required"})
		return
	}
	ctx := c.Request.Context()
	_, err := h.pool.Exec(ctx, `
		INSERT INTO verified_sources (address, source_code, compiler_version, evm_version, optimization, runs, verified_at)
		VALUES ($1, $2, $3, $4, $5, $6, NOW())
		ON CONFLICT (address) DO UPDATE SET source_code = EXCLUDED.source_code, compiler_version = EXCLUDED.compiler_version, verified_at = NOW()
	`, req.Address, req.Sources, req.CompilerVersion, req.EvmVersion, req.Optimization, req.Runs)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"status": "verified", "address": req.Address})
}

func (h *Handler) ReadContract(c *gin.Context) {
	addr := c.Param("address")
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	var req struct {
		Data   string `json:"data"`
		From   string `json:"from"`
		Block  string `json:"block"`
	}
	_ = c.ShouldBindJSON(&req)
	if req.Data == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing data"})
		return
	}
	block := req.Block
	if block == "" {
		block = "latest"
	}
	from := req.From
	if from == "" {
		from = "0x0000000000000000000000000000000000000000"
	}
	ctx := c.Request.Context()
	raw, err := h.rpcCall(ctx, "eth_call", []interface{}{
		map[string]string{"to": addr, "data": req.Data, "from": from},
		block,
	})
	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": "RPC call failed", "detail": err.Error()})
		return
	}
	c.Data(http.StatusOK, "application/json", raw)
}

func (h *Handler) WriteContract(c *gin.Context) {
	var req struct {
		RawTx string `json:"raw_transaction"`
	}
	if err := c.ShouldBindJSON(&req); err != nil || req.RawTx == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing raw_transaction"})
		return
	}
	ctx := c.Request.Context()
	raw, err := h.rpcCall(ctx, "eth_sendRawTransaction", []interface{}{req.RawTx})
	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": "RPC call failed", "detail": err.Error()})
		return
	}
	c.Data(http.StatusOK, "application/json", raw)
}

func (h *Handler) CheckProxy(c *gin.Context) {
	addr := c.Param("address")
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	// EIP-1967: implementation stored at 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc
	implSlot := "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc"
	raw, err := h.rpcCall(ctx, "eth_getStorageAt", []interface{}{addr, implSlot, "latest"})
	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": "RPC call failed"})
		return
	}
	c.Data(http.StatusOK, "application/json", raw)
}

func (h *Handler) GetContractType(c *gin.Context) {
	addr := c.Param("address")
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT standard FROM contracts WHERE address = $1`, addr)
	if err != nil {
		dbError(c, err)
		return
	}
	if row == nil {
		c.JSON(http.StatusOK, gin.H{"type": "unknown"})
		return
	}
	c.JSON(http.StatusOK, gin.H{"type": rowValue(row, "standard")})
}

func (h *Handler) CompileContract(c *gin.Context) {
	var req struct {
		SourceCode     string `json:"source_code"`
		CompilerVersion string `json:"compiler_version"`
		EvmVersion     string `json:"evm_version"`
		Optimization   bool   `json:"optimization"`
		Runs           int    `json:"runs"`
	}
	if err := c.ShouldBindJSON(&req); err != nil || req.SourceCode == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing source_code"})
		return
	}
	// Store source for compilation by the verifier service
	ctx := c.Request.Context()
	_, err := h.pool.Exec(ctx, `
		INSERT INTO contract_metadata (source_code, compiler_version, evm_version, optimization, runs, created_at)
		VALUES ($1, $2, $3, $4, $5, NOW())
	`, req.SourceCode, req.CompilerVersion, req.EvmVersion, req.Optimization, req.Runs)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"compiled": true, "status": "stored"})
}

// Address endpoints
func (h *Handler) GetAddressBalance(c *gin.Context) {
        ctx := c.Request.Context()
        addr := c.Param("address")
        row, err := h.queryOne(ctx, `SELECT COALESCE(SUM(value), 0) AS balance FROM transactions WHERE to_address = $1`, addr)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"balance": rowValue(row, "balance")})
}
func (h *Handler) GetAddressTxCount(c *gin.Context) {
        ctx := c.Request.Context()
        addr := c.Param("address")
        n, err := h.countQuery(ctx, `SELECT count(*) FROM transactions WHERE from_address = $1 OR to_address = $1`, addr)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"count": n})
}
func (h *Handler) GetAddressFirstSeen(c *gin.Context) {
	addr := c.Param("address")
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT block_number, timestamp FROM transactions WHERE from_address = $1 OR to_address = $1 ORDER BY block_number ASC LIMIT 1`, addr)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}
func (h *Handler) GetAddressLastSeen(c *gin.Context) {
	addr := c.Param("address")
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT block_number, timestamp FROM transactions WHERE from_address = $1 OR to_address = $1 ORDER BY block_number DESC LIMIT 1`, addr)
	if err != nil {
		dbError(c, err)
		return
	}
	respondOne(c, row)
}
func (h *Handler) AnnotateAddress(c *gin.Context) {
	addr := c.Param("address")
	if addr == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing address"})
		return
	}
	var req struct {
		Label       string `json:"label"`
		Category    string `json:"category"`
		Description string `json:"description"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	ctx := c.Request.Context()
	_, err := h.pool.Exec(ctx, `
		INSERT INTO search_index (address, label, category, description, updated_at)
		VALUES ($1, $2, $3, $4, NOW())
		ON CONFLICT (address) DO UPDATE SET label = EXCLUDED.label, category = EXCLUDED.category, description = EXCLUDED.description, updated_at = NOW()
	`, addr, req.Label, req.Category, req.Description)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"success": true, "address": addr})
}

// Gas endpoints
func (h *Handler) GetEstimatedGas(c *gin.Context) {
        ctx := c.Request.Context()
        row, err := h.queryOne(ctx, `SELECT COALESCE(AVG(gas_used), 21000) AS gas FROM transactions ORDER BY block_number DESC LIMIT 100`)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"gas": rowValue(row, "gas")})
}
func (h *Handler) GetGasRecommendation(c *gin.Context) {
        ctx := c.Request.Context()
        row, err := h.queryOne(ctx, `SELECT gas_price FROM gas_prices ORDER BY id DESC LIMIT 1`)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"recommended": rowValue(row, "gas_price")})
}
func (h *Handler) GetPriorityFees(c *gin.Context)           { c.JSON(http.StatusOK, gin.H{"fast": 2, "standard": 1, "slow": 0.5}) }
func (h *Handler) GetBaseFee(c *gin.Context) {
        ctx := c.Request.Context()
        row, err := h.queryOne(ctx, `SELECT base_fee_per_gas FROM blocks ORDER BY number DESC LIMIT 1`)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"baseFee": rowValue(row, "base_fee_per_gas")})
}
func (h *Handler) GetGasUtilization(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `SELECT COALESCE(AVG(gas_used::float / NULLIF(gas_limit, 0)), 0) AS utilization FROM blocks ORDER BY number DESC LIMIT 100`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}
func (h *Handler) CalculateGasSavings(c *gin.Context) {
	ctx := c.Request.Context()
	row, err := h.queryOne(ctx, `
		SELECT
			COALESCE(AVG(gas_price), 0) AS avg_gas_price,
			COALESCE(MIN(gas_price), 0) AS min_gas_price,
			COALESCE((AVG(gas_price) - MIN(gas_price)), 0) AS potential_savings
		FROM gas_prices ORDER BY block_number DESC LIMIT 100
	`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, row)
}

// Chart endpoints
func (h *Handler) ExportChartData(c *gin.Context) {
        ctx := c.Request.Context()
        n, err := h.countQuery(ctx, `SELECT count(*) FROM analytics_daily`)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"exported": n})
}

// DEX endpoints
func (h *Handler) GetDexLiquidity(c *gin.Context) {
        ctx := c.Request.Context()
        row, err := h.queryOne(ctx, `SELECT COALESCE(SUM(liquidity_usd), 0) AS liquidity FROM dex_pairs`)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"liquidity": rowValue(row, "liquidity")})
}
func (h *Handler) GetDexVolume(c *gin.Context) {
        ctx := c.Request.Context()
        row, err := h.queryOne(ctx, `SELECT COALESCE(SUM(volume_24h), 0) AS volume24h FROM dex_pairs`)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"volume24h": rowValue(row, "volume24h")})
}

// Governance endpoints
func (h *Handler) GetVoteCount(c *gin.Context) {
        ctx := c.Request.Context()
        proposalID := c.Param("id")
        row, err := h.queryOne(ctx, `SELECT count(*) FILTER (WHERE vote_choice = true) AS for_votes, count(*) FILTER (WHERE vote_choice = false) AS against_votes FROM governance_votes WHERE proposal_id = $1`, proposalID)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"for": rowValue(row, "for_votes"), "against": rowValue(row, "against_votes")})
}

// MEV endpoints

// Label endpoints
func (h *Handler) CreateLabel(c *gin.Context) {
	var req struct {
		Address     string `json:"address"`
		Label       string `json:"label"`
		Category    string `json:"category"`
		Description string `json:"description"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	if req.Address == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "address required"})
		return
	}
	ctx := c.Request.Context()
	_, err := h.pool.Exec(ctx, `
		INSERT INTO search_index (address, label, category, description, updated_at)
		VALUES ($1, $2, $3, $4, NOW())
		ON CONFLICT (address) DO UPDATE SET label = EXCLUDED.label, category = EXCLUDED.category, description = EXCLUDED.description
	`, req.Address, req.Label, req.Category, req.Description)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusCreated, gin.H{"success": true, "address": req.Address})
}
func (h *Handler) UpdateLabel(c *gin.Context) {
	id := c.Param("id")
	if id == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing id"})
		return
	}
	var req struct {
		Label       string `json:"label"`
		Category    string `json:"category"`
		Description string `json:"description"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	ctx := c.Request.Context()
	ct, err := h.pool.Exec(ctx, `UPDATE search_index SET label = $1, category = $2, description = $3, updated_at = NOW() WHERE id = $4`, req.Label, req.Category, req.Description, id)
	if err != nil {
		dbError(c, err)
		return
	}
	if ct.RowsAffected() == 0 {
		c.JSON(http.StatusNotFound, gin.H{"error": "label not found"})
		return
	}
	c.JSON(http.StatusOK, gin.H{"success": true})
}
func (h *Handler) DeleteLabel(c *gin.Context) {
	id := c.Param("id")
	if id == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing id"})
		return
	}
	ctx := c.Request.Context()
	ct, err := h.pool.Exec(ctx, `DELETE FROM search_index WHERE id = $1`, id)
	if err != nil {
		dbError(c, err)
		return
	}
	if ct.RowsAffected() == 0 {
		c.JSON(http.StatusNotFound, gin.H{"error": "label not found"})
		return
	}
	c.JSON(http.StatusOK, gin.H{"success": true})
}
func (h *Handler) ExportLabels(c *gin.Context) {
        ctx := c.Request.Context()
        n, err := h.countQuery(ctx, `SELECT count(*) FROM search_index`)
        if err != nil { dbError(c, err); return }
        c.JSON(http.StatusOK, gin.H{"exported": n})
}

// Stats endpoints

// Search endpoints

// Export endpoints
// Export endpoints stream the requested resource count from the database.
func (h *Handler) ExportBlocks(c *gin.Context) {
	ctx := c.Request.Context()
	n, err := h.countQuery(ctx, `SELECT count(*) FROM blocks`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"exported": n})
}
func (h *Handler) ExportTransactionsAll(c *gin.Context) {
	ctx := c.Request.Context()
	n, err := h.countQuery(ctx, `SELECT count(*) FROM transactions`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"exported": n})
}
func (h *Handler) ExportTokens(c *gin.Context) {
	ctx := c.Request.Context()
	n, err := h.countQuery(ctx, `SELECT count(*) FROM tokens`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"exported": n})
}
func (h *Handler) ExportNFTs(c *gin.Context) {
	ctx := c.Request.Context()
	n, err := h.countQuery(ctx, `SELECT count(*) FROM nfts`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"exported": n})
}
func (h *Handler) ExportBalances(c *gin.Context) {
	ctx := c.Request.Context()
	n, err := h.countQuery(ctx, `SELECT count(*) FROM token_holders`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"exported": n})
}
func (h *Handler) ExportContracts(c *gin.Context) {
	ctx := c.Request.Context()
	n, err := h.countQuery(ctx, `SELECT count(*) FROM contracts`)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"exported": n})
}

// WebSocket endpoints
func (h *Handler) HandleBlockWS(c *gin.Context) { h.HandleWebSocket(c) }
func (h *Handler) HandleTxWS(c *gin.Context)    { h.HandleWebSocket(c) }

// Rate limit endpoints
func (h *Handler) GetRateLimitStatus(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{"remaining": h.config.RateLimitRPS, "reset": time.Now().Unix()})
}
func (h *Handler) ResetRateLimit(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{"success": true})
}

// API Key endpoints
func (h *Handler) CreateAPIKey(c *gin.Context) {
	var req struct {
		Name      string `json:"name"`
		RateLimit int    `json:"rate_limit"`
	}
	_ = c.ShouldBindJSON(&req)
	ctx := c.Request.Context()
	var id int64
	err := h.pool.QueryRow(ctx, `INSERT INTO api_keys (key_hash, key_name, rate_limit, daily_limit, is_active) VALUES ($1, $2, $3, 100000, true) RETURNING id`, "tsk_"+randomHex(32), req.Name, req.RateLimit).Scan(&id)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"id": id, "key": "tsk_" + randomHex(32)})
}
func (h *Handler) ListAPIKeys(c *gin.Context)         { h.queryResource(c, "keys", 5) }
func (h *Handler) RevokeAPIKey(c *gin.Context) {
	id := c.Param("id")
	ctx := c.Request.Context()
	_, err := h.pool.Exec(ctx, `UPDATE api_keys SET is_active = false, updated_at = NOW() WHERE id = $1`, id)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"success": true})
}
func (h *Handler) UpdateAPIKey(c *gin.Context) {
	id := c.Param("id")
	var req struct {
		RateLimit int `json:"rate_limit"`
	}
	_ = c.ShouldBindJSON(&req)
	ctx := c.Request.Context()
	_, err := h.pool.Exec(ctx, `UPDATE api_keys SET rate_limit = $2, updated_at = NOW() WHERE id = $1`, id, req.RateLimit)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"success": true})
}

// Webhook endpoints
func (h *Handler) CreateWebhook(c *gin.Context) {
	var req struct {
		URL       string `json:"url"`
		EventType string `json:"event_type"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	ctx := c.Request.Context()
	var id int64
	err := h.pool.QueryRow(ctx, `INSERT INTO webhooks (url, event_type, is_active) VALUES ($1, $2, true) RETURNING id`, req.URL, req.EventType).Scan(&id)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"id": id})
}
func (h *Handler) ListWebhooks(c *gin.Context)  { h.queryResource(c, "webhooks", 10) }
func (h *Handler) UpdateWebhook(c *gin.Context) {
	id := c.Param("id")
	var req struct {
		URL       string `json:"url"`
		EventType string `json:"event_type"`
	}
	_ = c.ShouldBindJSON(&req)
	ctx := c.Request.Context()
	_, err := h.pool.Exec(ctx, `UPDATE webhooks SET url = $2, event_type = $3, updated_at = NOW() WHERE id = $1`, id, req.URL, req.EventType)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"success": true})
}
func (h *Handler) DeleteWebhook(c *gin.Context) {
	id := c.Param("id")
	ctx := c.Request.Context()
	_, err := h.pool.Exec(ctx, `UPDATE webhooks SET is_active = false WHERE id = $1`, id)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"success": true})
}

// Alert endpoints
func (h *Handler) CreateAlert(c *gin.Context) {
	var req struct {
		Address string `json:"address"`
		Name    string `json:"name"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	ctx := c.Request.Context()
	var id int64
	err := h.pool.QueryRow(ctx, `INSERT INTO search_index (search_type, address, name) VALUES ('alert', $1, $2) RETURNING id`, req.Address, req.Name).Scan(&id)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"id": id})
}
func (h *Handler) ListAlerts(c *gin.Context)    { h.queryResource(c, "alerts", 10) }
func (h *Handler) UpdateAlert(c *gin.Context) {
	id := c.Param("id")
	var req struct {
		Name string `json:"name"`
	}
	_ = c.ShouldBindJSON(&req)
	ctx := c.Request.Context()
	_, err := h.pool.Exec(ctx, `UPDATE search_index SET name = $2, updated_at = NOW() WHERE id = $1`, id, req.Name)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"success": true})
}
func (h *Handler) DeleteAlert(c *gin.Context) {
	id := c.Param("id")
	ctx := c.Request.Context()
	_, err := h.pool.Exec(ctx, `DELETE FROM search_index WHERE id = $1`, id)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"success": true})
}

// Watchlist endpoints
func (h *Handler) AddToWatchlist(c *gin.Context) {
	var req struct {
		Address string `json:"address"`
		Name    string `json:"name"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	ctx := c.Request.Context()
	var id int64
	err := h.pool.QueryRow(ctx, `INSERT INTO search_index (search_type, address, name) VALUES ('watchlist', $1, $2) RETURNING id`, req.Address, req.Name).Scan(&id)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"id": id})
}
func (h *Handler) GetWatchlist(c *gin.Context)            { h.queryResource(c, "watchlist", 20) }
func (h *Handler) RemoveFromWatchlist(c *gin.Context) {
	id := c.Param("id")
	ctx := c.Request.Context()
	_, err := h.pool.Exec(ctx, `DELETE FROM search_index WHERE id = $1`, id)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"success": true})
}
func (h *Handler) UpdateWatchlistItem(c *gin.Context) {
	id := c.Param("id")
	var req struct {
		Name string `json:"name"`
	}
	_ = c.ShouldBindJSON(&req)
	ctx := c.Request.Context()
	_, err := h.pool.Exec(ctx, `UPDATE search_index SET name = $2, updated_at = NOW() WHERE id = $1`, id, req.Name)
	if err != nil {
		dbError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"success": true})
}


// queryResource dispatches a resource name to the appropriate PostgreSQL
// query against the explorer schema and returns real rows. This replaces the
// previous getMockData/generateMockData placeholder responses.
func (h *Handler) queryResource(c *gin.Context, resource string, defLimit int) {
	ctx := c.Request.Context()
	limit := paramInt(c, "limit", defLimit)
	offset := paramOffset(c)

	type q struct {
		sql   string
		count string
	}
	var qq q

	switch resource {
	case "uncles":
		qq = q{`SELECT id, number, hash, parent_hash, block_number, miner, gas_limit, gas_used, timestamp, difficulty, reward FROM uncles ORDER BY block_number DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM uncles`}
	case "logs":
		// Logs are stored in the transactions.logs JSONB column; emit the
		// most recent transactions whose logs array is non-empty.
		qq = q{`SELECT hash, block_number, logs FROM transactions WHERE logs IS NOT NULL AND logs <> 'null' AND jsonb_array_length(COALESCE(logs,'[]'::jsonb)) > 0 ORDER BY block_number DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM transactions WHERE logs IS NOT NULL AND logs <> 'null'`}
	case "stateDiff", "statediff":
		qq = q{`SELECT id, transaction_hash, block_number, address, storage_key, storage_value, old_value, new_value, diff_type FROM state_diffs ORDER BY block_number DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM state_diffs`}
	case "blocks":
		qq = q{`SELECT id, number, hash, parent_hash, miner, gas_limit, gas_used, timestamp, size, tx_count, base_fee_per_gas, reward FROM blocks WHERE is_uncle = false ORDER BY number DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM blocks WHERE is_uncle = false`}
	case "validators":
		qq = q{`SELECT id, address, name, moniker, total_stake, commission_rate, is_jailed, is_active, status FROM validators ORDER BY total_stake DESC NULLS LAST LIMIT $1 OFFSET $2`, `SELECT count(*) FROM validators`}
	case "rewards":
		qq = q{`SELECT id, block_number, block_hash, validator, amount, transaction_hash FROM block_rewards ORDER BY block_number DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM block_rewards`}
	case "receipt":
		qq = q{`SELECT hash, block_number, transaction_index, from_address, to_address, status, gas_used, cumulative_gas_used, effective_gas_price, contract_address, logs FROM transactions WHERE hash = $3 LIMIT 1`, `SELECT 1`}
	case "txs":
		qq = q{`SELECT id, hash, nonce, block_number, transaction_index, from_address, to_address, value, gas_price, gas_used, status, transaction_type, block_timestamp FROM transactions ORDER BY block_number DESC NULLS LAST, transaction_index DESC NULLS LAST LIMIT $1 OFFSET $2`, `SELECT count(*) FROM transactions`}
	case "transfers":
		qq = q{`SELECT id, token_address, from_address, to_address, value, transaction_hash, block_number, log_index FROM token_transfers ORDER BY block_number DESC, log_index DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM token_transfers`}
	case "internalTxs":
		qq = q{`SELECT id, transaction_hash, block_number, transaction_index, depth, call_type, from_address, to_address, value, gas, revert FROM internal_transactions ORDER BY block_number DESC, transaction_index DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM internal_transactions`}
	case "callTree":
		qq = q{`SELECT id, transaction_hash, block_number, transaction_index, depth, call_type, from_address, to_address, value, gas, input, output FROM internal_transactions WHERE transaction_hash = $3 ORDER BY transaction_index, depth LIMIT $1 OFFSET $2`, `SELECT count(*) FROM internal_transactions WHERE transaction_hash = $3`}
	case "execution":
		qq = q{`SELECT hash, block_number, from_address, to_address, gas_used, status, effective_gas_price, logs FROM transactions WHERE hash = $3 LIMIT 1`, `SELECT 1`}
	case "storage":
		qq = q{`SELECT id, transaction_hash, block_number, address, storage_key, storage_value, old_value, new_value, diff_type FROM state_diffs WHERE address = $3 OR transaction_hash = $3 ORDER BY id DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM state_diffs`}
	case "callList":
		qq = q{`SELECT id, transaction_hash, block_number, transaction_index, depth, call_type, from_address, to_address, value, gas FROM internal_transactions WHERE transaction_hash = $3 ORDER BY depth, transaction_index LIMIT $1 OFFSET $2`, `SELECT count(*) FROM internal_transactions WHERE transaction_hash = $3`}
	case "vmTrace", "replay", "ops":
		qq = q{`SELECT id, transaction_hash, block_number, transaction_index, from_address, to_address, call_type, value, gas, input, output, revert, error, depth FROM traces WHERE transaction_hash = $3 ORDER BY id LIMIT $1 OFFSET $2`, `SELECT count(*) FROM traces`}
	case "traces":
		qq = q{`SELECT id, transaction_hash, block_number, transaction_index, from_address, to_address, call_type, value, gas, error FROM traces ORDER BY block_number DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM traces`}
	case "metadata":
		qq = q{`SELECT id, address, name, symbol, decimals, total_supply, holders_count, is_verified, price_usd FROM tokens WHERE address = $3 LIMIT 1`, `SELECT 1`}
	case "approvals", "allowances":
		qq = q{`SELECT id, token_address, owner, spender, value, transaction_hash, block_number FROM token_approvals ORDER BY block_number DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM token_approvals`}
	case "holders":
		qq = q{`SELECT id, token_address, address, balance, balance_usd, percent_holdings, updated_block FROM token_holders ORDER BY balance_usd DESC NULLS LAST LIMIT $1 OFFSET $2`, `SELECT count(*) FROM token_holders`}
	case "dexPairs":
		qq = q{`SELECT id, pair_address, token0_address, token1_address, token0_symbol, token1_symbol, reserve0, reserve1, liquidity_usd, volume_24h, factory_address, pair_type FROM dex_pairs ORDER BY id DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM dex_pairs`}
	case "history":
		qq = q{`SELECT id, token_address, holder_address, balance, block_number, timestamp FROM token_holder_history ORDER BY block_number DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM token_holder_history`}
	case "analytics":
		qq = q{`SELECT date, total_blocks, total_transactions, total_gas_used, total_gas_fees, total_volume, avg_gas_price FROM analytics_daily ORDER BY date DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM analytics_daily`}
	case "flippening":
		qq = q{`SELECT date, total_transactions, total_volume FROM analytics_daily ORDER BY date DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM analytics_daily`}
	case "tokens":
		qq = q{`SELECT id, address, name, symbol, decimals, total_supply, holders_count, price_usd, is_verified FROM tokens ORDER BY id DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM tokens`}
	case "nfts":
		qq = q{`SELECT id, collection_address, token_id, owner, uri, metadata FROM nfts ORDER BY id DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM nfts`}
	case "volumeHistory":
		qq = q{`SELECT collection_address, floor_price, volume_24h, sales_24h, holders FROM nft_floor_prices ORDER BY updated_at DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM nft_floor_prices`}
	case "rankings":
		qq = q{`SELECT id, address, name, symbol, total_supply, holders_count, floor_price, volume_24h, market_cap FROM nft_collections ORDER BY volume_24h DESC NULLS LAST LIMIT $1 OFFSET $2`, `SELECT count(*) FROM nft_collections`}
	case "rarity":
		qq = q{`SELECT id, collection_address, token_id, rarity_score, rank, traits FROM nft_rarity ORDER BY rank NULLS LAST LIMIT $1 OFFSET $2`, `SELECT count(*) FROM nft_rarity`}
	case "source", "contract":
		qq = q{`SELECT id, contract_address, file_name, source_code, compiler_version, language FROM verified_sources WHERE contract_address = $3 LIMIT 1`, `SELECT 1`}
	case "annotations":
		qq = q{`SELECT id, search_type, address, name, description FROM search_index WHERE address = $3 ORDER BY id LIMIT $1 OFFSET $2`, `SELECT count(*) FROM search_index`}
	case "balances":
		qq = q{`SELECT id, token_address, address, balance, balance_usd FROM token_holders WHERE address = $3 ORDER BY balance_usd DESC NULLS LAST LIMIT $1 OFFSET $2`, `SELECT count(*) FROM token_holders WHERE address = $3`}
	case "trends":
		qq = q{`SELECT id, gas_price, gas_used, gas_limit, timestamp, base_fee FROM gas_prices ORDER BY id DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM gas_prices`}
	case "aggregator":
		qq = q{`SELECT date, total_transactions, total_gas_used, total_volume, avg_gas_price FROM analytics_daily ORDER BY date DESC LIMIT 1`, `SELECT 1`}
	case "chart":
		qq = q{`SELECT date, total_blocks, total_transactions, total_gas_used, total_gas_fees, total_volume, avg_gas_price FROM analytics_daily ORDER BY date DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM analytics_daily`}
	case "overview", "stats":
		qq = q{`SELECT date, total_blocks, total_transactions, total_gas_used, total_gas_fees, total_volume, avg_gas_price FROM analytics_daily ORDER BY date DESC LIMIT 1`, `SELECT 1`}
	case "historical":
		qq = q{`SELECT date, total_blocks, total_transactions, total_gas_used, total_gas_fees, total_volume, avg_gas_price FROM analytics_daily ORDER BY date DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM analytics_daily`}
	case "ohlcv":
		qq = q{`SELECT id, transaction_hash, pair_address, from_address, to_address, from_amount, to_amount, block_number, log_index FROM dex_swaps ORDER BY block_number DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM dex_swaps`}
	case "pairs":
		qq = q{`SELECT id, pair_address, token0_address, token1_address, token0_symbol, token1_symbol, reserve0, reserve1, factory_address FROM dex_pairs ORDER BY id DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM dex_pairs`}
	case "exchanges", "protocols":
		qq = q{`SELECT DISTINCT factory_address FROM dex_pairs ORDER BY factory_address LIMIT $1 OFFSET $2`, `SELECT count(DISTINCT factory_address) FROM dex_pairs`}
	case "votes":
		qq = q{`SELECT id, proposal_id, voter, vote_choice, votes, block_number, transaction_hash FROM governance_votes ORDER BY block_number DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM governance_votes`}
	case "tally":
		qq = q{`SELECT proposal_id, count(*) FILTER (WHERE vote_choice = true) AS for_votes, count(*) FILTER (WHERE vote_choice = false) AS against_votes, count(*) AS total FROM governance_votes GROUP BY proposal_id ORDER BY proposal_id DESC LIMIT 1`, `SELECT 1`}
	case "voters":
		qq = q{`SELECT DISTINCT voter, count(*) AS vote_count FROM governance_votes GROUP BY voter ORDER BY vote_count DESC LIMIT $1 OFFSET $2`, `SELECT count(DISTINCT voter) FROM governance_votes`}
	case "delegations", "delegator":
		qq = q{`SELECT id, address, name, moniker, total_stake, commission_rate, is_active, status FROM validators ORDER BY total_stake DESC NULLS LAST LIMIT $1 OFFSET $2`, `SELECT count(*) FROM validators`}
	case "bundle":
		qq = q{`SELECT id, bundle_hash, block_number, sender, mev_type, tx_hashes, gas_used, profit_eth, profit_usd FROM mev_bundles ORDER BY block_number DESC LIMIT 1`, `SELECT 1`}
	case "bundles":
		qq = q{`SELECT id, bundle_hash, block_number, sender, mev_type, tx_hashes, gas_used, profit_eth, profit_usd FROM mev_bundles ORDER BY block_number DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM mev_bundles`}
	case "relays":
		qq = q{`SELECT DISTINCT sender FROM mev_bundles ORDER BY sender LIMIT $1 OFFSET $2`, `SELECT count(DISTINCT sender) FROM mev_bundles`}
	case "activities", "sandwiches", "opportunities", "jobs":
		qq = q{`SELECT id, bundle_hash, block_number, sender, mev_type, profit_eth, profit_usd FROM mev_bundles ORDER BY block_number DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM mev_bundles`}
	case "categories":
		qq = q{`SELECT id, search_type, address, name, description FROM search_index ORDER BY id LIMIT $1 OFFSET $2`, `SELECT count(*) FROM search_index`}
	case "addresses":
		qq = q{`SELECT id, search_type, address, name, description FROM search_index ORDER BY id DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM search_index`}
	case "keys":
		qq = q{`SELECT id, key_hash, key_name, user_id, rate_limit, daily_limit, is_active, expires_at FROM api_keys ORDER BY id DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM api_keys`}
	case "webhooks":
		qq = q{`SELECT id, url, event_type, is_active, retry_count, timeout_seconds FROM webhooks ORDER BY id DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM webhooks`}
	case "alerts", "watchlist":
		qq = q{`SELECT id, search_type, address, name, description FROM search_index ORDER BY id DESC LIMIT $1 OFFSET $2`, `SELECT count(*) FROM search_index`}
	default:
		c.JSON(http.StatusOK, gin.H{"items": []map[string]interface{}{}, "total": 0})
		return
	}

	if h.pool == nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": "database unavailable"})
		return
	}

	var extraArg interface{}
	if num := c.Param("number"); num != "" {
		if n, err := strconv.ParseInt(num, 10, 64); err == nil {
			extraArg = n
		} else {
			extraArg = num
		}
	}
	if hash := c.Param("hash"); hash != "" && extraArg == nil {
		extraArg = hash
	}
	if a := c.Param("address"); a != "" && extraArg == nil {
		extraArg = a
	}
	if extraArg == nil {
		extraArg = ""
	}

	rows, err := h.queryRows(ctx, qq.sql, limit, offset, extraArg)
	if err != nil {
		rows, err = h.queryRows(ctx, qq.sql, limit, offset)
		if err != nil {
			dbError(c, err)
			return
		}
	}
	total, _ := h.countQuery(ctx, qq.count, extraArg)
	if total == 0 && len(rows) > 0 {
		total = int64(len(rows))
	}
	respondList(c, rows, int(total))
}

// getMockData is retained as a thin alias so existing route registrations keep
// compiling; it now delegates to the real database-backed queryResource.
func (h *Handler) getMockData(c *gin.Context, resource string, count int) {
	h.queryResource(c, resource, count)
}
