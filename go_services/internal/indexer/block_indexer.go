package indexer

import (
	"context"
	"fmt"
	"log"
	"sync"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/tigersmartchain/go_services/internal/rpc"
)

// BlockIndexer indexes blockchain blocks and transactions
type BlockIndexer struct {
	rpcClient  *rpc.BSCClient
	db         Database
	workers    int
	startBlock uint64
	endBlock   uint64
	isRunning  bool
	mu         sync.RWMutex
}

// NewBlockIndexer creates a new block indexer
func NewBlockIndexer(rpcURL, wsURL string, db Database, workers int) (*BlockIndexer, error) {
	client, err := rpc.NewBSCClient(rpcURL, wsURL)
	if err != nil {
		return nil, err
	}

	return &BlockIndexer{
		rpcClient:  client,
		db:         db,
		workers:    workers,
		startBlock: 0,
	}, nil
}

// Start begins indexing from a specific block
func (bi *BlockIndexer) Start(ctx context.Context, startBlock uint64) error {
	bi.mu.Lock()
	bi.isRunning = true
	bi.startBlock = startBlock
	bi.mu.Unlock()

	log.Printf("Starting block indexer from block %d", startBlock)

	// Get latest block
	latestBlock, err := bi.rpcClient.GetBlockNumber(ctx)
	if err != nil {
		return fmt.Errorf("failed to get latest block: %w", err)
	}

	bi.endBlock = latestBlock

	// Process blocks in batches
	bi.processBlockRange(ctx, startBlock, latestBlock)

	return nil
}

// Stop stops the indexer
func (bi *BlockIndexer) Stop() {
	bi.mu.Lock()
	bi.isRunning = false
	bi.mu.Unlock()
	log.Println("Block indexer stopped")
}

// processBlockRange processes a range of blocks
func (bi *BlockIndexer) processBlockRange(ctx context.Context, start, end uint64) {
	blockChan := make(chan uint64, bi.workers)
	resultChan := make(chan error, bi.workers)

	// Start workers
	for i := 0; i < bi.workers; i++ {
		go bi.worker(ctx, blockChan, resultChan)
	}

	// Send blocks to workers
	go func() {
		for blockNum := start; blockNum <= end; blockNum++ {
			bi.mu.RLock()
			if !bi.isRunning {
				bi.mu.RUnlock()
				break
			}
			bi.mu.RUnlock()
			blockChan <- blockNum
		}
		close(blockChan)
	}()

	// Wait for results
	for i := uint64(0); i < (end - start + 1); i++ {
		err := <-resultChan
		if err != nil {
			log.Printf("Error processing block: %v", err)
		}
	}
}

// worker processes blocks from the channel
func (bi *BlockIndexer) worker(ctx context.Context, blockChan <-chan uint64, resultChan chan<- error) {
	for blockNum := range blockChan {
		err := bi.processBlock(ctx, blockNum)
		resultChan <- err
	}
}

// processBlock indexes a single block
func (bi *BlockIndexer) processBlock(ctx context.Context, blockNum uint64) error {
	// Get block
	block, err := bi.rpcClient.GetBlockByNumber(ctx, blockNum)
	if err != nil {
		return fmt.Errorf("failed to get block %d: %w", blockNum, err)
	}

	// Save block to database
	err = bi.db.SaveBlock(ctx, convertBlock(block))
	if err != nil {
		return fmt.Errorf("failed to save block %d: %w", blockNum, err)
	}

	// Process transactions
	for _, tx := range block.Transactions() {
		err = bi.processTransaction(ctx, tx, blockNum)
		if err != nil {
			log.Printf("Failed to process tx %s: %v", tx.Hash().Hex(), err)
		}
	}

	log.Printf("Indexed block %d with %d transactions", blockNum, len(block.Transactions()))
	return nil
}

// processTransaction indexes a transaction
func (bi *BlockIndexer) processTransaction(ctx context.Context, tx *types.Transaction, blockNum uint64) error {
	receipt, err := bi.rpcClient.GetTransactionReceipt(ctx, tx.Hash())
	if err != nil {
		return fmt.Errorf("failed to get receipt: %w", err)
	}

	// Save transaction
	err = bi.db.SaveTransaction(ctx, convertTransaction(tx, receipt, blockNum))
	if err != nil {
		return fmt.Errorf("failed to save transaction: %w", err)
	}

	// Save logs
	for _, lg := range receipt.Logs {
		err = bi.db.SaveLog(ctx, convertLog(lg, blockNum))
		if err != nil {
			log.Printf("Failed to save log: %v", err)
		}
	}

	return nil
}

// SubscribeToNewBlocks subscribes to new block headers
func (bi *BlockIndexer) SubscribeToNewBlocks(ctx context.Context) error {
	headerChan := make(chan *types.Header)

	sub, err := bi.rpcClient.SubscribeNewHead(ctx, headerChan)
	if err != nil {
		return fmt.Errorf("failed to subscribe: %w", err)
	}
	defer sub.Unsubscribe()

	log.Println("Subscribed to new blocks")

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case header := <-headerChan:
			err := bi.processBlock(ctx, header.Number.Uint64())
			if err != nil {
				log.Printf("Failed to process new block: %v", err)
			}
		}
	}
}

// convertBlock converts ETH block to our format
func convertBlock(block *types.Block) map[string]interface{} {
	return map[string]interface{}{
		"number":            block.Number().Uint64(),
		"hash":              block.Hash().Hex(),
		"parent_hash":       block.ParentHash().Hex(),
		"nonce":             block.Nonce(),
		"sha3_uncles":       block.UncleHash().Hex(),
		"logs_bloom":        block.Bloom().Bytes(),
		"transactions_root": block.TxHash().Hex(),
		"state_root":        block.Root().Hex(),
		"receipts_root":     block.ReceiptHash().Hex(),
		"miner":             block.Coinbase().Hex(),
		"difficulty":        block.Difficulty().String(),
		"extra_data":        block.Extra(),
		"size":              block.Size(),
		"gas_limit":         block.GasLimit(),
		"gas_used":          block.GasUsed(),
		"timestamp":         block.Time(),
		"transactions":      len(block.Transactions()),
		"uncles":            len(block.Uncles()),
	}
}

// convertTransaction converts ETH transaction to our format
func convertTransaction(tx *types.Transaction, receipt *types.Receipt, blockNum uint64) map[string]interface{} {
	to := ""
	if tx.To() != nil {
		to = tx.To().Hex()
	}

	return map[string]interface{}{
		"hash":              tx.Hash().Hex(),
		"block_number":      blockNum,
		"block_hash":        receipt.BlockHash.Hex(),
		"transaction_index": receipt.TransactionIndex,
		"to":                to,
		"value":             tx.Value().String(),
		"gas_price":         tx.GasPrice().String(),
		"gas":               tx.Gas(),
		"nonce":             tx.Nonce(),
		"input":             common.Bytes2Hex(tx.Data()),
		"tx_type":           tx.Type(),
		"status":            receipt.Status,
		"gas_used":          receipt.GasUsed,
		"logs_count":        len(receipt.Logs),
		"logs_bloom":        receipt.Bloom.Bytes(),
	}
}

// convertLog converts ETH log to our format
func convertLog(lg *types.Log, blockNum uint64) map[string]interface{} {
	topics := make([]string, len(lg.Topics))
	for i, topic := range lg.Topics {
		topics[i] = topic.Hex()
	}

	return map[string]interface{}{
		"address":          lg.Address.Hex(),
		"topics":           topics,
		"data":             common.Bytes2Hex(lg.Data),
		"block_number":     blockNum,
		"transaction_hash": lg.TxHash.Hex(),
		"log_index":        lg.Index,
	}
}

// GetIndexedBlockRange returns the current indexed block range
func (bi *BlockIndexer) GetIndexedBlockRange() (start, end uint64) {
	bi.mu.RLock()
	defer bi.mu.RUnlock()
	return bi.startBlock, bi.endBlock
}

// IsRunning returns whether the indexer is running
func (bi *BlockIndexer) IsRunning() bool {
	bi.mu.RLock()
	defer bi.mu.RUnlock()
	return bi.isRunning
}

// Database interface for storage operations
type Database interface {
	SaveBlock(ctx context.Context, block map[string]interface{}) error
	SaveTransaction(ctx context.Context, tx map[string]interface{}) error
	SaveLog(ctx context.Context, log map[string]interface{}) error
}
