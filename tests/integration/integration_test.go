// Package integration provides integration tests for TigerSmartChain.
package integration

import (
	"context"
	"fmt"
	"math/big"
	"sync"
	"testing"
	"time"

	"github.com/tigersmartchain/tigersmartChain/internal/blockchain"
	"github.com/tigersmartchain/tigersmartChain/internal/blockchain/chain"
	"github.com/tigersmartchain/tigersmartChain/internal/blockchain/mempool"
	"github.com/tigersmartchain/tigersmartChain/internal/blockchain/transaction"
	"github.com/tigersmartchain/tigersmartChain/internal/consensus"
	"github.com/tigersmartchain/tigersmartChain/internal/consensus/validator"
	"github.com/tigersmartchain/tigersmartChain/internal/evm"
	"github.com/tigersmartchain/tigersmartChain/internal/state"
)

// =============================================================================
// TEST CLUSTERS
// =============================================================================

// TestCluster represents a test cluster of nodes.
type TestCluster struct {
	t *testing.T

	nodes []*TestNode
	config *ClusterConfig

	mu sync.RWMutex
}

// ClusterConfig holds cluster configuration.
type ClusterConfig struct {
	// Number of validator nodes
	ValidatorCount int

	// Number of full nodes
	FullNodeCount int

	// Block time in milliseconds
	BlockTime uint64

	// Max gas
	MaxGas uint64

	// Enable fast sync
	FastSync bool

	// Network latency
	NetworkLatency time.Duration
}

// TestNode represents a test node.
type TestNode struct {
	id int

	// Components
	chain *blockchain.Chain
	mempool *mempool.TxPool
	evm *evm.EVM
	stateDB state.Database
	validatorMgr *validator.Manager

	// Status
	running bool
	height uint64

	// P2P
	peers map[int]*TestNode
}

// NewTestCluster creates a new test cluster.
func NewTestCluster(t *testing.T, config *ClusterConfig) *TestCluster {
	if config == nil {
		config = &ClusterConfig{
			ValidatorCount: 4,
			FullNodeCount: 2,
			BlockTime: 450,
			MaxGas: 300000000,
			FastSync: true,
		}
	}

	cluster := &TestCluster{
		t:     t,
		config: config,
		nodes: make([]*TestNode, 0),
	}

	// Create nodes
	for i := 0; i < config.ValidatorCount+config.FullNodeCount; i++ {
		node := cluster.createNode(i)
		cluster.nodes = append(cluster.nodes, node)
	}

	// Connect nodes
	cluster.connectNodes()

	return cluster
}

// createNode creates a new test node.
func (c *TestCluster) createNode(id int) *TestNode {
	// Create chain config
	chainConfig := chain.DefaultChainConfig()
	chainConfig.BlockTime = c.config.BlockTime
	chainConfig.MaxGas = c.config.MaxGas

	// Create chain
	blockchainChain, err := blockchain.NewChain(chainConfig, nil)
	if err != nil {
		c.t.Fatalf("failed to create chain: %v", err)
	}

	// Create mempool
	memPool := mempool.NewTxPool(&mempool.Config{
		MaxSize:    4096,
		MaxPerAccount: 128,
		PriceBump:  10,
	})

	// Create state DB
	stateDB := state.NewDatabase(nil)

	// Create EVM
	evmEngine := evm.NewEVM(blockchainChain, stateDB)

	// Create validator manager
	validatorMgr := validator.NewManager()

	node := &TestNode{
		id:         id,
		chain:     blockchainChain,
		mempool:   memPool,
		evm:      evmEngine,
		stateDB:  stateDB,
		validatorMgr: validatorMgr,
		running: false,
		height:  0,
		peers:   make(map[int]*TestNode),
	}

	return node
}

// connectNodes connects all nodes in the cluster.
func (c *TestCluster) connectNodes() {
	for i, node := range c.nodes {
		for j, peer := range c.nodes {
			if i != j {
				node.peers[j] = peer
			}
		}
	}
}

// Start starts all nodes in the cluster.
func (c *TestCluster) Start() {
	for _, node := range c.nodes {
		if err := node.Start(); err != nil {
			c.t.Fatalf("failed to start node: %v", err)
		}
	}

	// Wait for initial block
	c.waitForBlock(1, 10*time.Second)
}

// Stop stops all nodes.
func (c *TestCluster) Stop() {
	for _, node := range c.nodes {
		node.Stop()
	}
}

// waitForBlock waits for a specific block.
func (c *TestCluster) waitForBlock(target uint64, timeout time.Duration) {
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()

	for {
		select {
		case <-ctx.Done():
			c.t.Fatalf("timeout waiting for block %d", target)
		default:
			if c.getLatestBlock() >= target {
				return
			}
			time.Sleep(10 * time.Millisecond)
		}
	}
}

// getLatestBlock returns the latest block number.
func (c *TestCluster) getLatestBlock() uint64 {
	var max uint64
	for _, node := range c.nodes {
		if node.height > max {
			max = node.height
		}
	}
	return max
}

// Start starts a node.
func (n *TestNode) Start() error {
	n.running = true
	return nil
}

// Stop stops a node.
func (n *TestNode) Stop() {
	n.running = false
}

// =============================================================================
// INTEGRATION TESTS
// =============================================================================

// TestBlockProduction tests block production.
func TestBlockProduction(t *testing.T) {
	cluster := NewTestCluster(t, nil)
	defer cluster.Stop()

	cluster.Start()

	// Wait for multiple blocks
	time.Sleep(2 * time.Second)
	
	height := cluster.getLatestBlock()
	if height < 1 {
		t.Error("no blocks produced")
	}
}

// TestTransactionPool tests transaction pool.
func TestTransactionPool(t *testing.T) {
	cluster := NewTestCluster(t, nil)
	defer cluster.Stop()

	cluster.Start()

	// Create transaction
	tx := &transaction.Transaction{
		To:     "0x1234567890123456789012345678901234567890",
		Value:  big.NewInt(1000000),
		Data:   []byte{},
		GasLimit: 21000,
	}

	// Add to mempool
	node := cluster.nodes[0]
	if err := node.mempool.AddTransaction(tx); err != nil {
		t.Fatalf("failed to add transaction: %v", err)
	}

	// Wait for block
	cluster.waitForBlock(1, 5*time.Second)

	if node.mempool.PendingCount() != 0 {
		t.Log("transaction included in block")
	}
}

// TestConsensus tests consensus.
func TestConsensus(t *testing.T) {
	cluster := NewTestCluster(t, nil)
	defer cluster.Stop()

	cluster.Start()

	// Add validators
	node := cluster.nodes[0]
	
	v := &validator.Validator{
		Address: "0x1234567890123456789012345678901234567890",
		Stake:  big.NewInt(100000000000000000000),
		Active: true,
	}
	
	if err := node.validatorMgr.AddValidator(v); err != nil {
		t.Fatalf("failed to add validator: %v", err)
	}

	// Get validators
	validators := node.validatorMgr.GetActiveValidators()
	if len(validators) < 1 {
		t.Error("no active validators")
	}
}

// TestStateSync tests state synchronization.
func TestStateSync(t *testing.T) {
	cluster := NewTestCluster(t, &ClusterConfig{
		ValidatorCount: 4,
		FullNodeCount: 2,
		FastSync:    true,
	})
	defer cluster.Stop()

	cluster.Start()

	// Wait for blocks
	cluster.waitForBlock(3, 10*time.Second)

	// Verify all nodes have same height
	height := cluster.getLatestBlock()
	for i, node := range cluster.nodes {
		if node.height != height {
			t.Errorf("node %d height mismatch: got %d, want %d", i, node.height, height)
		}
	}
}

// TestEVMBasics tests EVM basic operations.
func TestEVMBasics(t *testing.T) {
	cluster := NewTestCluster(t, nil)
	defer cluster.Stop()

	cluster.Start()

	node := cluster.nodes[0]
	
	// Test EVM execution
	result, err := node.evm.Execute([]byte{})
	if err != nil {
		t.Logf("EVM execution: %v", err)
	}
	
	_ = result
}

// =============================================================================
// BENCHMARKS
// =============================================================================

// BenchmarkBlockProduction benchmarks block production.
func BenchmarkBlockProduction(b *testing.B) {
	cluster := NewTestCluster(&testing.T{}, nil)
	defer cluster.Stop()

	cluster.Start()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		cluster.waitForBlock(uint64(i+1), 5*time.Second)
	}
}

// BenchmarkTransactionThroughput benchmarks transaction throughput.
func BenchmarkTransactionThroughput(b *testing.B) {
	cluster := NewTestCluster(&testing.T{}, nil)
	defer cluster.Stop()

	cluster.Start()

	node := cluster.nodes[0]
	
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		tx := &transaction.Transaction{
			To:     fmt.Sprintf("0x%x", i),
			Value:  big.NewInt(int64(i)),
			Data:   []byte{},
			GasLimit: 21000,
		}
		node.mempool.AddTransaction(tx)
	}
}

// =============================================================================
// HELPERS
// =============================================================================

// generateTestAddress generates a test address.
func generateTestAddress(id int) string {
	return fmt.Sprintf("0x%x", id)
}

// generateTestKey generates a test private key.
func generateTestKey(id int) string {
	return fmt.Sprintf("0x%x", id)
}

// init initializes tests.
func init() {
	// Verify cluster creation
	_ = NewTestCluster
}