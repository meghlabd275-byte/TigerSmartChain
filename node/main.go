// Package node provides TigerSmartChain blockchain node implementation.
// This is the main entry point for running a blockchain node.
package node

import (
	"context"
	"fmt"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/tigersmartchain/internal/blockchain/chain"
	"github.com/tigersmartchain/internal/blockchain/genesis"
	"github.com/tigersmartchain/internal/blockchain/mempool"
	"github.com/tigersmartchain/internal/blockchain/state"
	"github.com/tigersmartchain/internal/consensus/posa"
	"github.com/tigersmartchain/internal/evm/execution-engine"
	"github.com/tigersmartchain/internal/metrics/prometheus"
	"github.com/tigersmartchain/internal/network/p2p"
	"github.com/tigersmartchain/internal/rpc/json-rpc"
	"github.com/tigersmartchain/internal/storage/leveldb"
)

// Config holds node configuration.
type Config struct {
	// Network settings
	NetworkID   uint64
	ChainID     uint64
	Network    string

	// P2P settings
	ListenAddr  string
	BootNodes  []string
	MaxPeers   int

	// RPC settings
	RPCEnabled  bool
	RPCAddr    string
	RPCCORSHost string

	// State settings
	DataDir    string
	CacheSize int

	// Consensus settings
	ValidatorAddr string
	ValidatorKey  string
	EpochLength  uint64
	SlotDuration time.Duration

	// Metrics settings
	MetricsEnabled bool
	MetricsAddr   string

	// Genesis
	GenesisFile string
}

// TigerNode represents a TigerSmartChain blockchain node.
type TigerNode struct {
	config *Config

	// Core components
	chain      *chain.Chain
	stateDB    *state.StateDB
	mempool    *mempool.TxPool
	engine     *executionengine.Engine
	consensus  *posa.PoSA

	// Network components
	p2pServer *p2p.Server

	// RPC server
	rpcServer *jsonrpc.Server

	// Metrics
	metrics *prometheus.Metrics

	// Context
	ctx    context.Context
	cancel context.CancelFunc

	// Status
	isRunning bool
}

// NewTigerNode creates a new TigerNode instance.
func NewTigerNode(config *Config) (*TigerNode, error) {
	ctx, cancel := context.WithCancel(context.Background())

	node := &TigerNode{
		config:  config,
		ctx:    ctx,
		cancel: cancel,
	}

	if err := node.initialize(); err != nil {
		cancel()
		return nil, err
	}

	return node, nil
}

// initialize initializes all node components.
func (n *TigerNode) initialize() error {
	// Initialize storage
	storage, err := leveldb.NewLevelDB(n.config.DataDir, n.config.CacheSize)
	if err != nil {
		return fmt.Errorf("failed to initialize storage: %w", err)
	}

	// Initialize state database
	stateDB, err := state.NewStateDB(storage)
	if err != nil {
		return fmt.Errorf("failed to initialize state DB: %w", err)
	}
	n.stateDB = stateDB

	// Initialize chain
	chainConfig := &chain.Config{
		NetworkID: n.config.NetworkID,
		ChainID:   n.config.ChainID,
	}
	chain, err := chain.NewChain(chainConfig, storage)
	if err != nil {
		return fmt.Errorf("failed to initialize chain: %w", err)
	}
	n.chain = chain

	// Initialize transaction pool
	mempoolConfig := &mempool.Config{
		MaxSize:       4096,
		MaxPerAccount: 128,
		PriceBump:    10,
	}
	mempool := mempool.NewTxPool(mempoolConfig)
	n.mempool = mempool

	// Initialize EVM execution engine
	engineConfig := &executionengine.Config{
		ChainConfig: chainConfig,
		StateDB:    stateDB,
	}
	engine, err := executionengine.NewEngine(engineConfig)
	if err != nil {
		return fmt.Errorf("failed to initialize EVM engine: %w", err)
	}
	n.engine = engine

	// Initialize PoSA consensus
	consensusConfig := &posa.Config{
		ChainID:      n.config.ChainID,
		EpochLength: n.config.EpochLength,
		SlotDuration: n.config.SlotDuration,
	}
	consensus, err := posa.NewPoSA(consensusConfig, chain, stateDB)
	if err != nil {
		return fmt.Errorf("failed to initialize consensus: %w", err)
	}
	n.consensus = consensus

	// Initialize P2P server
	p2pConfig := &p2p.Config{
		ListenAddr:  n.config.ListenAddr,
		MaxPeers:   n.config.MaxPeers,
		BootNodes: n.config.BootNodes,
	}
	p2pServer, err := p2p.NewServer(p2pConfig, chain, mempool, consensus)
	if err != nil {
		return fmt.Errorf("failed to initialize P2P server: %w", err)
	}
	n.p2pServer = p2pServer

	// Initialize RPC server
	if n.config.RPCEnabled {
		rpcConfig := &jsonrpc.Config{
			Address:   n.config.RPCAddr,
			CORSHost: n.config.RPCCORSHost,
			Chain:    chain,
			StateDB:  stateDB,
			Mempool:  mempool,
			Engine:  engine,
			Consensus: consensus,
		}
		rpcServer, err := jsonrpc.NewServer(rpcConfig)
		if err != nil {
			return fmt.Errorf("failed to initialize RPC server: %w", err)
		}
		n.rpcServer = rpcServer
	}

	// Initialize metrics
	if n.config.MetricsEnabled {
		metrics, err := prometheus.NewMetrics(n.config.MetricsAddr)
		if err != nil {
			return fmt.Errorf("failed to initialize metrics: %w", err)
		}
		n.metrics = metrics
	}

	return nil
}

// Start starts the node and all its components.
func (n *TigerNode) Start() error {
	if n.isRunning {
		return fmt.Errorf("node already running")
	}

	fmt.Println("Starting TigerSmartChain node...")

	// Start P2P server
	if err := n.p2pServer.Start(); err != nil {
		return fmt.Errorf("failed to start P2P server: %w", err)
	}
	fmt.Println("P2P server started")

	// Start consensus
	if err := n.consensus.Start(); err != nil {
		return fmt.Errorf("failed to start consensus: %w", err)
	}
	fmt.Println("Consensus engine started")

	// Start RPC server
	if n.rpcServer != nil {
		if err := n.rpcServer.Start(); err != nil {
			return fmt.Errorf("failed to start RPC server: %w", err)
		}
		fmt.Println("RPC server started")
	}

	// Start metrics
	if n.metrics != nil {
		if err := n.metrics.Start(); err != nil {
			return fmt.Errorf("failed to start metrics: %w", err)
		}
		fmt.Println("Metrics server started")
	}

	n.isRunning = true
	fmt.Println("TigerSmartChain node started successfully")

	return nil
}

// Stop stops the node and all its components.
func (n *TigerNode) Stop() error {
	if !n.isRunning {
		return fmt.Errorf("node not running")
	}

	fmt.Println("Stopping TigerSmartChain node...")

	// Cancel context to signal all components to stop
	n.cancel()

	// Stop RPC server
	if n.rpcServer != nil {
		n.rpcServer.Stop()
	}

	// Stop P2P server
	n.p2pServer.Stop()

	// Stop consensus
	n.consensus.Stop()

	// Stop metrics
	if n.metrics != nil {
		n.metrics.Stop()
	}

	// Close state database
	if n.stateDB != nil {
		n.stateDB.Close()
	}

	n.isRunning = false
	fmt.Println("TigerSmartChain node stopped")

	return nil
}

// Run runs the node until it's stopped.
func (n *TigerNode) Run() error {
	if err := n.Start(); err != nil {
		return err
	}

	// Wait for interrupt signal
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, os.Interrupt, syscall.SIGTERM)

	<-sigCh

	return n.Stop()
}

// GetChain returns the chain instance.
func (n *TigerNode) GetChain() *chain.Chain {
	return n.chain
}

// GetStateDB returns the state database instance.
func (n *TigerNode) GetStateDB() *state.StateDB {
	return n.stateDB
}

// GetMempool returns the transaction pool instance.
func (n *TigerNode) GetMempool() *mempool.TxPool {
	return n.mempool
}

// GetConsensus returns the consensus engine instance.
func (n *TigerNode) GetConsensus() *posa.PoSA {
	return n.consensus
}

// GetP2PServer returns the P2P server instance.
func (n *TigerNode) GetP2PServer() *p2p.Server {
	return n.p2pServer
}

// GetRPCServer returns the RPC server instance.
func (n *TigerNode) GetRPCServer() *jsonrpc.Server {
	return n.rpcServer
}

// IsRunning returns whether the node is running.
func (n *TigerNode) IsRunning() bool {
	return n.isRunning
}

// DefaultConfig returns default node configuration.
func DefaultConfig() *Config {
	return &Config{
		NetworkID:    1,
		ChainID:     1,
		Network:    "mainnet",
		ListenAddr: ":30303",
		BootNodes: nil,
		MaxPeers:   50,
		RPCEnabled: true,
		RPCAddr:   ":8545",
		DataDir:   "./data",
		CacheSize: 1024,
		EpochLength: 200,
		SlotDuration: 3 * time.Second,
		MetricsEnabled: false,
		MetricsAddr: ":9090",
	}
}

// LoadGenesis loads genesis configuration from file.
func LoadGenesis(filename string) (*genesis.Genesis, error) {
	if filename == "" {
		return genesis.DefaultGenesis(), nil
	}
	return genesis.LoadGenesis(filename)
}