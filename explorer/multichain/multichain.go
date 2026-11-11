// Multi-chain Support API
// Production-grade multi-chain explorer support

package multichain

import (
	"bytes"
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
)

// =============================================================================
// TYPES
// =============================================================================

type Chain struct {
	ID             int    `json:"id"`
	Name           string `json:"name"`
	Symbol         string `json:"symbol"`
	ChainID        int64  `json:"chainId"`
	RPCURL         string `json:"rpcUrl"`
	ExplorerURL    string `json:"explorerUrl"`
	Currency       string `json:"currency"`
	CurrencySymbol string `json:"currencySymbol"`
	Decimals       int    `json:"decimals"`
	BlockTime      int    `json:"blockTime"`
	IsTestnet      bool   `json:"isTestnet"`
	LogoURL        string `json:"logoUrl"`
	Color          string `json:"color"`
}

type ChainStats struct {
	ChainID     int64   `json:"chainId"`
	TotalBlocks uint64  `json:"totalBlocks"`
	TotalTXs    uint64  `json:"totalTransactions"`
	TPS         float64 `json:"tps"`
	GasPrice    uint64  `json:"gasPrice"`
	LastBlock   uint64  `json:"lastBlock"`
}

type MultiChainConfig struct {
	DB        *sql.DB
	CacheTTL  time.Duration
	MaxChains int
}

type Server struct {
	cfg    *MultiChainConfig
	chains map[int64]*Chain
	stats  map[int64]*ChainStats
	mu     sync.RWMutex
	client *http.Client
}

// =============================================================================
// DEFAULT CHAINS
// =============================================================================

// defaultChains seeds the registry with the well-known public networks. Rows in
// chain_configs override these entries when a database is configured.
var defaultChains = map[int64]*Chain{
	6666:     {ID: 6666, Name: "TigerSmartChain", Symbol: "TGR", ChainID: 6666, RPCURL: "http://127.0.0.1:8545", ExplorerURL: "https://tigerscan.io", Currency: "tgr", CurrencySymbol: "TGR", Decimals: 18, BlockTime: 1, IsTestnet: false, Color: "#ff7a00"},
	1:        {ID: 1, Name: "Ethereum", Symbol: "ETH", ChainID: 1, RPCURL: "https://eth.llamarpc.com", ExplorerURL: "https://etherscan.io", Currency: "ether", CurrencySymbol: "ETH", Decimals: 18, BlockTime: 12, IsTestnet: false, Color: "#627eea"},
	56:       {ID: 56, Name: "BNB Smart Chain", Symbol: "BNB", ChainID: 56, RPCURL: "https://bsc-dataseed.binance.org", ExplorerURL: "https://bscscan.com", Currency: "bnb", CurrencySymbol: "BNB", Decimals: 18, BlockTime: 3, IsTestnet: false, Color: "#f3ba2f"},
	137:      {ID: 137, Name: "Polygon", Symbol: "POL", ChainID: 137, RPCURL: "https://polygon-rpc.com", ExplorerURL: "https://polygonscan.com", Currency: "pol", CurrencySymbol: "POL", Decimals: 18, BlockTime: 2, IsTestnet: false, Color: "#8247e5"},
	42161:    {ID: 42161, Name: "Arbitrum One", Symbol: "ETH", ChainID: 42161, RPCURL: "https://arb1.arbitrum.io/rpc", ExplorerURL: "https://arbiscan.io", Currency: "ether", CurrencySymbol: "ETH", Decimals: 18, BlockTime: 1, IsTestnet: false, Color: "#28a0f0"},
	10:       {ID: 10, Name: "OP Mainnet", Symbol: "ETH", ChainID: 10, RPCURL: "https://mainnet.optimism.io", ExplorerURL: "https://optimistic.etherscan.io", Currency: "ether", CurrencySymbol: "ETH", Decimals: 18, BlockTime: 2, IsTestnet: false, Color: "#ff0420"},
	8453:     {ID: 8453, Name: "Base", Symbol: "ETH", ChainID: 8453, RPCURL: "https://mainnet.base.org", ExplorerURL: "https://basescan.org", Currency: "ether", CurrencySymbol: "ETH", Decimals: 18, BlockTime: 2, IsTestnet: false, Color: "#0052ff"},
	43114:    {ID: 43114, Name: "Avalanche C-Chain", Symbol: "AVAX", ChainID: 43114, RPCURL: "https://api.avax.network/ext/bc/C/rpc", ExplorerURL: "https://snowtrace.io", Currency: "avax", CurrencySymbol: "AVAX", Decimals: 18, BlockTime: 2, IsTestnet: false, Color: "#e84142"},
	11155111: {ID: 11155111, Name: "Sepolia", Symbol: "ETH", ChainID: 11155111, RPCURL: "https://rpc.sepolia.org", ExplorerURL: "https://sepolia.etherscan.io", Currency: "ether", CurrencySymbol: "ETH", Decimals: 18, BlockTime: 12, IsTestnet: true, Color: "#627eea"},
}

// =============================================================================
// SERVER
// =============================================================================

func NewServer(cfg *MultiChainConfig) *Server {
	if cfg == nil {
		cfg = &MultiChainConfig{}
	}
	if cfg.CacheTTL == 0 {
		cfg.CacheTTL = 30 * time.Second
	}

	srv := &Server{
		cfg:    cfg,
		chains: make(map[int64]*Chain),
		stats:  make(map[int64]*ChainStats),
		client: &http.Client{Timeout: 15 * time.Second},
	}

	for id, chain := range defaultChains {
		srv.chains[id] = chain
	}

	if cfg.DB != nil {
		if err := srv.loadChainsFromDB(context.Background()); err != nil {
			fmt.Printf("multichain: loading chain registry: %v\n", err)
		}
	}

	go srv.updateStatsLoop()

	return srv
}

// loadChainsFromDB overlays the persisted registry on the built-in defaults.
func (s *Server) loadChainsFromDB(ctx context.Context) error {
	rows, err := s.cfg.DB.QueryContext(ctx,
		`SELECT chain_id, chain_name, symbol, COALESCE(rpc_url, ''), COALESCE(explorer_url, '')
		 FROM chain_configs WHERE is_active = TRUE`)
	if err != nil {
		return err
	}
	defer rows.Close()

	for rows.Next() {
		var chainID int64
		var name, symbol, rpcURL, explorerURL string
		if err := rows.Scan(&chainID, &name, &symbol, &rpcURL, &explorerURL); err != nil {
			return err
		}

		chain := s.chains[chainID]
		if chain == nil {
			chain = &Chain{ID: int(chainID), ChainID: chainID, Decimals: 18}
			s.chains[chainID] = chain
		}
		chain.Name = name
		chain.Symbol = symbol
		chain.CurrencySymbol = symbol
		if rpcURL != "" {
			chain.RPCURL = rpcURL
		}
		if explorerURL != "" {
			chain.ExplorerURL = explorerURL
		}
	}
	return rows.Err()
}

func (s *Server) updateStatsLoop() {
	ticker := time.NewTicker(s.cfg.CacheTTL)
	defer ticker.Stop()

	s.updateAllStats()
	for range ticker.C {
		s.updateAllStats()
	}
}

func (s *Server) updateAllStats() {
	s.mu.RLock()
	ids := make([]int64, 0, len(s.chains))
	for id, chain := range s.chains {
		if chain.RPCURL != "" {
			ids = append(ids, id)
		}
	}
	s.mu.RUnlock()

	for _, id := range ids {
		go s.updateChainStats(id)
	}
}

// rpcCall performs a single JSON-RPC request against the given endpoint.
func (s *Server) rpcCall(ctx context.Context, url, method string, params ...interface{}) (json.RawMessage, error) {
	body, err := json.Marshal(map[string]interface{}{
		"jsonrpc": "2.0",
		"id":      1,
		"method":  method,
		"params":  params,
	})
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := s.client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	raw, err := io.ReadAll(io.LimitReader(resp.Body, 8<<20))
	if err != nil {
		return nil, err
	}

	var out struct {
		Result json.RawMessage `json:"result"`
		Error  *struct {
			Message string `json:"message"`
		} `json:"error"`
	}
	if err := json.Unmarshal(raw, &out); err != nil {
		return nil, fmt.Errorf("decode %s response: %w", method, err)
	}
	if out.Error != nil {
		return nil, fmt.Errorf("%s: %s", method, out.Error.Message)
	}
	return out.Result, nil
}

func (s *Server) rpcUint64(ctx context.Context, url, method string, params ...interface{}) (uint64, error) {
	raw, err := s.rpcCall(ctx, url, method, params...)
	if err != nil {
		return 0, err
	}
	var hexStr string
	if err := json.Unmarshal(raw, &hexStr); err != nil {
		return 0, fmt.Errorf("%s: unexpected result %s", method, raw)
	}
	return parseHexUint64(hexStr)
}

func parseHexUint64(s string) (uint64, error) {
	if len(s) > 2 && (s[:2] == "0x" || s[:2] == "0X") {
		s = s[2:]
	}
	if s == "" {
		return 0, nil
	}
	return strconv.ParseUint(s, 16, 64)
}

// rpcBlock is the subset of a block needed to derive chain statistics.
type rpcBlock struct {
	Number       string   `json:"number"`
	Timestamp    string   `json:"timestamp"`
	Transactions []string `json:"transactions"`
}

// updateChainStats derives live statistics from the chain's own RPC endpoint.
// Nothing is synthesised: when the endpoint is unreachable the previous sample
// is kept and the failure is logged.
func (s *Server) updateChainStats(chainID int64) {
	s.mu.RLock()
	chain := s.chains[chainID]
	s.mu.RUnlock()
	if chain == nil || chain.RPCURL == "" {
		return
	}

	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()

	height, err := s.rpcUint64(ctx, chain.RPCURL, "eth_blockNumber")
	if err != nil {
		fmt.Printf("multichain: %s eth_blockNumber: %v\n", chain.Name, err)
		return
	}

	gasPrice, err := s.rpcUint64(ctx, chain.RPCURL, "eth_gasPrice")
	if err != nil {
		fmt.Printf("multichain: %s eth_gasPrice: %v\n", chain.Name, err)
		return
	}

	stats := &ChainStats{
		ChainID:     chainID,
		TotalBlocks: height,
		LastBlock:   height,
		GasPrice:    gasPrice,
	}

	// Throughput is measured across the two most recent blocks so the figure
	// reflects the chain rather than one quiet block.
	if height > 0 {
		latest, err := s.fetchBlock(ctx, chain.RPCURL, height)
		if err != nil {
			fmt.Printf("multichain: %s latest block: %v\n", chain.Name, err)
			return
		}
		if prev, err := s.fetchBlock(ctx, chain.RPCURL, height-1); err == nil {
			latestTime, tErr1 := parseHexUint64(latest.Timestamp)
			prevTime, tErr2 := parseHexUint64(prev.Timestamp)
			if tErr1 == nil && tErr2 == nil && latestTime > prevTime {
				blockTime := latestTime - prevTime
				txCount := uint64(len(latest.Transactions) + len(prev.Transactions))
				stats.TPS = float64(txCount) / float64(blockTime)
				stats.TotalTXs = txCount
			}
		}
	}

	s.mu.Lock()
	s.stats[chainID] = stats
	s.mu.Unlock()
}

func (s *Server) fetchBlock(ctx context.Context, url string, number uint64) (*rpcBlock, error) {
	raw, err := s.rpcCall(ctx, url, "eth_getBlockByNumber", fmt.Sprintf("0x%x", number), false)
	if err != nil {
		return nil, err
	}
	var block rpcBlock
	if err := json.Unmarshal(raw, &block); err != nil {
		return nil, err
	}
	return &block, nil
}

// =============================================================================
// API HANDLERS
// =============================================================================

func (s *Server) GetChains(c *gin.Context) {
	includeTestnet := c.Query("testnet") == "true"

	s.mu.RLock()
	chains := make([]*Chain, 0, len(s.chains))
	for _, chain := range s.chains {
		if includeTestnet || !chain.IsTestnet {
			chains = append(chains, chain)
		}
	}
	s.mu.RUnlock()

	c.JSON(http.StatusOK, gin.H{"chains": chains, "total": len(chains)})
}

func (s *Server) GetChain(c *gin.Context) {
	chainID, err := strconv.ParseInt(c.Param("chainId"), 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid chain ID"})
		return
	}

	s.mu.RLock()
	chain, ok := s.chains[chainID]
	s.mu.RUnlock()

	if !ok {
		c.JSON(http.StatusNotFound, gin.H{"error": "Chain not found"})
		return
	}

	c.JSON(http.StatusOK, chain)
}

func (s *Server) GetChainStats(c *gin.Context) {
	chainID, err := strconv.ParseInt(c.Param("chainId"), 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid chain ID"})
		return
	}

	s.mu.RLock()
	stats, ok := s.stats[chainID]
	chain := s.chains[chainID]
	s.mu.RUnlock()

	if chain == nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Chain not found"})
		return
	}
	if !ok {
		c.JSON(http.StatusServiceUnavailable, gin.H{
			"error":   "no statistics available for this chain",
			"chainId": chainID,
		})
		return
	}

	c.JSON(http.StatusOK, stats)
}

func (s *Server) GetCrossChainTX(c *gin.Context) {
	txHash := c.Param("txHash")
	if s.cfg.DB == nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": "database not configured"})
		return
	}

	var (
		sourceChain int64
		destChain   int64
		fromAddr    string
		toAddr      string
		tokenAddr   string
		amount      string
		fee         sql.NullString
		status      string
		blockNumber int64
		createdAt   time.Time
		confirmedAt sql.NullTime
	)

	err := s.cfg.DB.QueryRowContext(c.Request.Context(),
		`SELECT source_chain_id, dest_chain_id, from_address, to_address, token_address,
		        amount, fee, status, block_number, created_at, confirmed_at
		 FROM cross_chain_transfers WHERE transaction_hash = $1`,
		txHash,
	).Scan(&sourceChain, &destChain, &fromAddr, &toAddr, &tokenAddr, &amount, &fee,
		&status, &blockNumber, &createdAt, &confirmedAt)

	if err == sql.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{"error": "cross-chain transfer not found"})
		return
	}
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	result := gin.H{
		"hash":        txHash,
		"sourceChain": sourceChain,
		"destChain":   destChain,
		"sender":      fromAddr,
		"receiver":    toAddr,
		"token":       tokenAddr,
		"amount":      amount,
		"status":      status,
		"blockNumber": blockNumber,
		"createdAt":   createdAt,
	}
	if fee.Valid {
		result["fee"] = fee.String
	}
	if confirmedAt.Valid {
		result["confirmedAt"] = confirmedAt.Time
	}

	c.JSON(http.StatusOK, result)
}

func (s *Server) AddChain(c *gin.Context) {
	var req struct {
		Name           string `json:"name" binding:"required"`
		Symbol         string `json:"symbol" binding:"required"`
		ChainID        int64  `json:"chainId" binding:"required"`
		RPCURL         string `json:"rpcUrl" binding:"required"`
		ExplorerURL    string `json:"explorerUrl"`
		Currency       string `json:"currency"`
		CurrencySymbol string `json:"currencySymbol"`
		Decimals       int    `json:"decimals"`
		BlockTime      int    `json:"blockTime"`
		IsTestnet      bool   `json:"isTestnet"`
	}

	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	s.mu.RLock()
	_, exists := s.chains[req.ChainID]
	s.mu.RUnlock()
	if exists {
		c.JSON(http.StatusConflict, gin.H{"error": "Chain already exists"})
		return
	}

	if s.cfg.MaxChains > 0 && len(s.chains) >= s.cfg.MaxChains {
		c.JSON(http.StatusConflict, gin.H{"error": "chain registry is full"})
		return
	}

	chain := &Chain{
		ID:             len(s.chains) + 1,
		Name:           req.Name,
		Symbol:         req.Symbol,
		ChainID:        req.ChainID,
		RPCURL:         req.RPCURL,
		ExplorerURL:    req.ExplorerURL,
		Currency:       req.Currency,
		CurrencySymbol: req.CurrencySymbol,
		Decimals:       req.Decimals,
		BlockTime:      req.BlockTime,
		IsTestnet:      req.IsTestnet,
	}

	if s.cfg.DB != nil {
		_, err := s.cfg.DB.ExecContext(c.Request.Context(),
			`INSERT INTO chain_configs (chain_id, chain_name, symbol, rpc_url, explorer_url, is_active)
			 VALUES ($1, $2, $3, $4, $5, TRUE)
			 ON CONFLICT (chain_id) DO UPDATE SET
			     chain_name = EXCLUDED.chain_name,
			     symbol = EXCLUDED.symbol,
			     rpc_url = EXCLUDED.rpc_url,
			     explorer_url = EXCLUDED.explorer_url,
			     is_active = TRUE`,
			chain.ChainID, chain.Name, chain.Symbol, chain.RPCURL, chain.ExplorerURL,
		)
		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
	}

	s.mu.Lock()
	s.chains[req.ChainID] = chain
	s.mu.Unlock()

	go s.updateChainStats(req.ChainID)

	c.JSON(http.StatusCreated, chain)
}

// =============================================================================
// ROUTES
// =============================================================================

func (s *Server) RegisterRoutes(r *gin.Engine) {
	api := r.Group("/api/v1")
	{
		api.GET("/chains", s.GetChains)
		api.GET("/chains/:chainId", s.GetChain)
		api.GET("/chains/:chainId/stats", s.GetChainStats)
		api.GET("/cross-chain/tx/:txHash", s.GetCrossChainTX)
		api.POST("/admin/chains", s.AddChain)
	}
}
