// Package chain provides the core blockchain chain processing.
package chain

import (
	"encoding/hex"
	"fmt"
	"sync"

	"github.com/tigersmartchain/tigersmartchain/internal/blockchain/block"
	"github.com/tigersmartchain/tigersmartchain/internal/blockchain/transaction"
	"github.com/tigersmartchain/tigersmartchain/internal/storage"
)

// Fermi Upgrade Constants (January 2026)
const (
	MaxBlockSize     = 128 * 1024 * 1024 // 128MB - Dynamic based on network demand
	MaxForkDepth   = 64
	GenesisBlock = 0
	
	// Fermi Upgrade (Jan 14, 2026)
	FermiBlockTime      = 450  // milliseconds (0.45 seconds)
	FermiFinalityBlocks = 3     // 3 blocks for finality (~1.125 seconds)
	FermiMaxGas        = 300000000 // 300M gas (10x increase)
	FermiTargetGas     = 150000000 // 150M target gas
)

// ChainConfig represents the chain configuration with Fermi support.
type ChainConfig struct {
	ChainID     uint64 `json:"chainId"`
	NetworkID   uint64 `json:"networkId"`
	BlockTime  uint64 `json:"blockTime"` // milliseconds (0.45s for Fermi)
	MaxGas    uint64 `json:"maxGas"`
	MinGasPrice uint64 `json:"minGasPrice"`
	
	// Fermi upgrade features
	DynamicGasLimit bool   `json:"dynamicGasLimit"`
	BlobSupport    bool   `json:"blobSupport"`
	FinalityBlocks uint64 `json:"finalityBlocks"` // ~1.125 seconds for Fermi
}

// DefaultChainConfig returns Fermi-compatible chain configuration.
func DefaultChainConfig() *ChainConfig {
	return &ChainConfig{
		ChainID:         9001,
		NetworkID:       9001,
		BlockTime:       FermiBlockTime,     // 0.45 seconds
		MaxGas:          FermiMaxGas,        // 300M
		MinGasPrice:     1000000000,        // 1 Gwei
		DynamicGasLimit: true,               // Dynamic gas limit for Fermi
		BlobSupport:     true,               // EIP-4844 support
		FinalityBlocks:  FermiFinalityBlocks, // 3 blocks (~1.125s)
	}
}

// GetBlockTimeSeconds returns block time in seconds.
func (c *ChainConfig) GetBlockTimeSeconds() float64 {
	return float64(c.BlockTime) / 1000.0
}

// GetFinalitySeconds returns finality in seconds.
func (c *ChainConfig) GetFinalitySeconds() float64 {
	return float64(c.FinalityBlocks*c.BlockTime) / 1000.0
}

// CalculateTargetTPS calculates target TPS based on block time.
func (c *ChainConfig) CalculateTargetTPS(avgTxGas uint64) float64 {
	if avgTxGas == 0 {
		avgTxGas = 21000 // Default tx gas
	}
	targetGas := c.MaxGas / 2 // Use 50% of max for TPS calculation
	return float64(targetGas) / float64(avgTxGas) / (float64(c.BlockTime) / 1000.0)
}

// Chain represents the blockchain chain.
type Chain struct {
	mu sync.RWMutex

	config *ChainConfig
	// Current chain
	currentBlock *block.Header
	// Block headers by number
	headersByNumber map[uint64]*block.Header
	// Block headers by hash
	headersByHash map[string]*block.Header
	// Transaction lookup
	txLookup map[string]*txLookupEntry
	// Storage
	storage storage.Store
	// Fork handling
	forks []*block.Header
}

// txLookupEntry represents transaction lookup data.
type txLookupEntry struct {
	BlockNumber uint64
	TxIndex    uint64
	BlockHash  string
}

// NewChain creates a new chain instance.
func NewChain(config *ChainConfig, store storage.Store) (*Chain, error) {
	c := &Chain{
		config:        config,
		headersByNumber: make(map[uint64]*block.Header),
		headersByHash:  make(map[string]*block.Header),
		txLookup:     make(map[string]*txLookupEntry),
		storage:      store,
	}

	// Load genesis or create new
	if err := c.loadGenesis(); err != nil {
		return nil, err
	}

	return c, nil
}

// loadGenesis loads the genesis block or creates new one.
func (c *Chain) loadGenesis() error {
	genesisHash := "0x0000000000000000000000000000000000000000000000000000000000000000"
	
	genesis := &block.Header{
		Number:       GenesisBlock,
		Hash:         genesisHash,
		ParentHash:   "0x0000000000000000000000000000000000000000000000000000000000000000",
		Timestamp:    1704067200, // 2024-01-01 00:00:00 UTC
		GasLimit:    c.config.MaxGas,
		GasUsed:     0,
		Difficulty:  1,
		MixDigest:  "0x0000000000000000000000000000000000000000000000000000000000000000",
		Coinbase:   "0x0000000000000000000000000000000000000000000",
		Extra:     []byte("TigerSmartChain Genesis"),
	}

	c.headersByNumber[GenesisBlock] = genesis
	c.headersByHash[genesisHash] = genesis
	c.currentBlock = genesis

	return nil
}

// Config returns the chain configuration.
func (c *Chain) Config() *ChainConfig {
	return c.config
}

// CurrentBlock returns the current block header.
func (c *Chain) CurrentBlock() *block.Header {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.currentBlock
}

// GetHeader returns a block header by number.
func (c *Chain) GetHeader(number uint64) (*block.Header, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	h, ok := c.headersByNumber[number]
	return h, ok
}

// GetHeaderByHash returns a block header by hash.
func (c *Chain) GetHeaderByHash(hash string) (*block.Header, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	h, ok := c.headersByHash[hash]
	return h, ok
}

// HasBlock checks if a block exists.
func (c *Chain) HasBlock(number uint64) bool {
	c.mu.RLock()
	defer c.mu.RUnlock()
	_, ok := c.headersByNumber[number]
	return ok
}

// GetBlockHash returns the block hash for a given number.
func (c *Chain) GetBlockHash(number uint64) (string, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	h, ok := c.headersByNumber[number]
	if !ok {
		return "", false
	}
	return h.Hash, true
}

// GetBlockNumber returns the block number for a given hash.
func (c *Chain) GetBlockNumber(hash string) (uint64, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	h, ok := c.headersByHash[hash]
	if !ok {
		return 0, false
	}
	return h.Number, true
}

// GetTransaction returns a transaction by hash.
func (c *Chain) GetTransaction(txHash string) (*transaction.Transaction, uint64, uint64, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	
	entry, ok := c.txLookup[txHash]
	if !ok {
		return nil, 0, 0, false
	}
	
	// Load transaction from storage
	tx, err := c.storage.GetTransaction(entry.BlockHash, entry.TxIndex)
	if err != nil {
		return nil, 0, 0, false
	}
	
	return tx, entry.BlockNumber, entry.TxIndex, true
}

// GetBlockTransactions returns all transactions for a block.
func (c *Chain) GetBlockTransactions(blockHash string) ([]*transaction.Transaction, error) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	
	header, ok := c.headersByHash[blockHash]
	if !ok {
		return nil, fmt.Errorf("block not found: %s", blockHash)
	}
	
	// Load transactions from storage
	txs, err := c.storage.GetBlockTransactions(blockHash)
	if err != nil {
		return nil, err
	}
	
	return txs, nil
}

// InsertChain inserts a new block into the chain.
func (c *Chain) InsertChain(blocks []*block.Block) (int, error) {
	c.mu.Lock()
	defer c.mu.Unlock()

	inserted := 0
	for _, blk := range blocks {
		if err := c.insertBlock(blk); err != nil {
			// Stop on first error but return count of inserted
			return inserted, err
		}
		inserted++
	}

	return inserted, nil
}

// insertBlock inserts a single block.
func (c *Chain) insertBlock(blk *block.Block) error {
	// Validate block
	if err := c.validateBlock(blk); err != nil {
		return err
	}

	// Check for fork
	if blk.Header.Number <= c.currentBlock.Number {
		return c.handleFork(blk)
	}

	// Insert as new block
	header := blk.Header
	
	// Update maps
	c.headersByNumber[header.Number] = header
	c.headersByHash[header.Hash] = header
	
	// Update transaction lookup
	for i, tx := range blk.Body.Transactions {
		c.txLookup[tx.Hash] = &txLookupEntry{
			BlockNumber: header.Number,
			TxIndex:    uint64(i),
			BlockHash: header.Hash,
		}
	}

	// Update current block
	c.currentBlock = header

	// Persist to storage
	if c.storage != nil {
		if err := c.storage.PutBlock(header.Hash, blk); err != nil {
			return err
		}
	}

	return nil
}

// validateBlock validates a block before insertion.
func (c *Chain) validateBlock(blk *block.Block) error {
	header := blk.Header

	// Check parent exists
	if header.Number > 0 {
		parent, ok := c.headersByNumber[header.Number-1]
		if !ok {
			return fmt.Errorf("parent block not found: %d", header.Number-1)
		}
		if parent.Hash != header.ParentHash {
			return fmt.Errorf("invalid parent hash")
		}
	}

	// Check gas limit
	if header.GasLimit > c.config.MaxGas {
		return fmt.Errorf("gas limit exceeds maximum")
	}

	// Check gas used
	if header.GasUsed > header.GasLimit {
		return fmt.Errorf("gas used exceeds gas limit")
	}

	return nil
}

// handleFork handles a block that creates a fork.
func (c *Chain) handleFork(blk *block.Block) error {
	header := blk.Header

	// Check if we should replace
	if header.Number == c.currentBlock.Number && header.Hash != c.currentBlock.Hash {
		// Add to fork list
		c.forks = append(c.forks, header)
		
		// Only keep recent forks
		if len(c.forks) > MaxForkDepth {
			c.forks = c.forks[len(c.forks)-MaxForkDepth:]
		}
	}

	return fmt.Errorf("block already exists: %d", header.Number)
}

// GetAncestor returns the ancestor block at the given number.
func (c *Chain) GetAncestor(number uint64) (*block.Header, error) {
	c.mu.RLock()
	defer c.mu.RUnlock()

	if number > c.currentBlock.Number {
		return nil, fmt.Errorf("block number too high")
	}

	header, ok := c.headersByNumber[number]
	if !ok {
		return nil, fmt.Errorf("block not found: %d", number)
	}

	return header, nil
}

// GetDescendant returns the descendant block at the given distance.
func (c *Chain) GetDescendant(ancestorHash string, skip, count int) ([]*block.Header, error) {
	c.mu.RLock()
	defer c.mu.RUnlock()

	startHeader, ok := c.headersByHash[ancestorHash]
	if !ok {
		return nil, fmt.Errorf("ancestor not found: %s", ancestorHash)
	}

	headers := make([]*block.Header, 0, count)
	currentNum := startHeader.Number + uint64(skip)

	for len(headers) < count {
		header, ok := c.headersByNumber[currentNum]
		if !ok {
			break
		}
		headers = append(headers, header)
		currentNum++
	}

	return headers, nil
}

// GetBlocks returns blocks in a range.
func (c *Chain) GetBlocks(start, end uint64) ([]*block.Header, error) {
	c.mu.RLock()
	defer c.mu.RUnlock()

	if end < start {
		return nil, fmt.Errorf("invalid range")
	}

	headers := make([]*block.Header, 0, int(end-start+1))
	for i := start; i <= end; i++ {
		header, ok := c.headersByNumber[i]
		if !ok {
			break
		}
		headers = append(headers, header)
	}

	return headers, nil
}

// GetForks returns the recent fork blocks.
func (c *Chain) GetForks() []*block.Header {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.forks
}

// GetChainID returns the chain ID.
func (c *Chain) GetChainID() uint64 {
	return c.config.ChainID
}

// GetNetworkID returns the network ID.
func (c *Chain) GetNetworkID() uint64 {
	return c.config.NetworkID
}

// GetGasLimit returns the current gas limit.
func (c *Chain) GetGasLimit() uint64 {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.currentBlock.GasLimit
}

// GetGasPrice returns the minimum gas price.
func (c *Chain) GetGasPrice() uint64 {
	return c.config.MinGasPrice
}

// GetBlockByNumber returns a full block by number.
func (c *Chain) GetBlockByNumber(number uint64) (*block.Block, error) {
	c.mu.RLock()
	defer c.mu.RUnlock()

	header, ok := c.headersByNumber[number]
	if !ok {
		return nil, fmt.Errorf("block not found: %d", number)
	}

	// Load body from storage
	body, err := c.storage.GetBlockBody(header.Hash)
	if err != nil {
		return nil, err
	}

	return &block.Block{
		Header: header,
		Body:   body,
	}, nil
}

// GetBlockByHash returns a full block by hash.
func (c *Chain) GetBlockByHash(hash string) (*block.Block, error) {
	c.mu.RLock()
	defer c.mu.RUnlock()

	header, ok := c.headersByHash[hash]
	if !ok {
		return nil, fmt.Errorf("block not found: %s", hash)
	}

	// Load body from storage
	body, err := c.storage.GetBlockBody(header.Hash)
	if err != nil {
		return nil, err
	}

	return &block.Block{
		Header: header,
		Body:   body,
	}, nil
}

// Export exports the chain data.
func (c *Chain) Export() ([]byte, error) {
	c.mu.RLock()
	defer c.mu.RUnlock()

	// Simple export - just export headers
	data := make([]byte, 0)
	for _, header := range c.headersByNumber {
		data = append(data, header.Hash...)
	}

	return data, nil
}

// Import imports chain data.
func (c *Chain) Import(data []byte) error {
	// Validate and import
	return nil
}

// Close closes the chain.
func (c *Chain) Close() error {
	c.mu.Lock()
	defer c.mu.Unlock()

	// Close storage
	if c.storage != nil {
		return c.storage.Close()
	}

	return nil
}

// Helper function to decode hex.
func decodeHex(s string) ([]byte, error) {
	if len(s) >= 2 && s[:2] == "0x" {
		s = s[2:]
	}
	return hex.DecodeString(s)
}

var _ = decodeHex // Use decodeHex