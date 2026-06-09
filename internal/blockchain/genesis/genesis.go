// Package genesis provides genesis block configuration for TigerSmartChain.
package genesis

import (
	"encoding/json"
	"os"
	"fmt"
	"math/big"

	"github.com/tigersmartchain/tigersmartchain/internal/blockchain/block"
	"github.com/tigersmartchain/tigersmartchain/pkg/crypto"
	"github.com/tigersmartchain/tigersmartchain/pkg/types"
)

// Genesis represents the genesis block configuration.
type Genesis struct {
	// Config is the chain configuration
	Config *ChainConfig `json:"config"`

	// Alloc is the initial account allocations
	Alloc Allocation `json:"alloc"`

	// Validators is the list of initial validators
	Validators []types.Address `json:"validators"`

	// Timestamp is the genesis timestamp
	Timestamp uint64 `json:"timestamp"`
}

// ChainConfig is the chain configuration in genesis.
type ChainConfig struct {
	// ChainID is the chain ID
	ChainID uint64 `json:"chainId"`

	// ChainName is the chain name
	ChainName string `json:"chainName"`

	// Symbol is the native coin symbol
	Symbol string `json:"symbol"`

	// Decimals is the native coin decimals
	Decimals uint8 `json:"decimals"`

	// BlockTime is the target block time in seconds
	BlockTime uint64 `json:"blockTime"`

	// MaxGasLimit is the maximum block gas limit
	MaxGasLimit uint64 `json:"maxGasLimit"`

	// MinGasLimit is the minimum block gas limit
	MinGasLimit uint64 `json:"minGasLimit"`

	// NetworkID is the network ID
	NetworkID uint64 `json:"networkId"`

	// EIP150Block is the EIP-150 fork block
	EIP150Block uint64 `json:"eip150Block"`

	// EIP155Block is the EIP-155 fork block
	EIP155Block uint64 `json:"eip155Block"`

	// EIP158Block is the EIP-158 fork block
	EIP158Block uint64 `json:"eip158Block"`

	// ByzantiumBlock is the Byzantium fork block
	ByzantiumBlock uint64 `json:"byzantiumBlock"`

	// ConstantinopleBlock is the Constantinople fork block
	ConstantinopleBlock uint64 `json:"constantinopleBlock"`

	// PetersburgBlock is the Petersburg fork block
	PetersburgBlock uint64 `json:"petersburgBlock"`

	// IstanbulBlock is the Istanbul fork block
	IstanbulBlock uint64 `json:"istanbulBlock"`

	// EIP1559Block is the EIP-1559 (London) fork block
	EIP1559Block uint64 `json:"eip1559Block"`

	// MergeBlock is the Merge fork block
	MergeBlock uint64 `json:"mergeBlock"`
}

// Allocation represents the initial account allocations.
type Allocation map[types.Address]AccountAlloc

// AccountAlloc represents the account allocation.
type AccountAlloc struct {
	// Balance is the initial balance in wei
	Balance string `json:"balance"`

	// Code is the contract code
	Code string `json:"code,omitempty"`

	// Storage is the contract storage
	Storage map[string]string `json:"storage,omitempty"`

	// Nonce is the account nonce
	Nonce uint64 `json:"nonce,omitempty"`
}

// DefaultGenesis returns the default mainnet genesis.
func DefaultGenesis() *Genesis {
	return &Genesis{
		Config: DefaultChainConfig(),
		Alloc:  DefaultAllocation(),
		Validators: []types.Address{
			types.HexToAddress("0x..."), // Add validator addresses
		},
		Timestamp: 1699507200, // 2023-11-09 00:00:00 UTC
	}
}

// TestnetGenesis returns the default testnet genesis.
func TestnetGenesis() *Genesis {
	return &Genesis{
		Config: TestnetChainConfig(),
		Alloc:  TestnetAllocation(),
		Validators: []types.Address{
			types.HexToAddress("0x..."),
		},
		Timestamp: 1699507200,
	}
}

// DevGenesis returns the development genesis.
func DevGenesis() *Genesis {
	return &Genesis{
		Config: DevChainConfig(),
		Alloc:  DevAllocation(),
		Validators: []types.Address{
			types.HexToAddress("0x..."),
		},
		Timestamp: 0,
	}
}

// DefaultChainConfig returns the default mainnet chain config.
func DefaultChainConfig() *ChainConfig {
	return &ChainConfig{
		ChainID:            9001,
		ChainName:          "TigerSmartChain",
		Symbol:             "TGR",
		Decimals:          18,
		BlockTime:          3,
		MaxGasLimit:       30000000,
		MinGasLimit:       5000000,
		NetworkID:         9001,
		EIP150Block:       0,
		EIP155Block:       0,
		EIP158Block:       0,
		ByzantiumBlock:    0,
		ConstantinopleBlock: 0,
		PetersburgBlock:    0,
		IstanbulBlock:    0,
		EIP1559Block:     0,
		MergeBlock:       0,
	}
}

// TestnetChainConfig returns the testnet chain config.
func TestnetChainConfig() *ChainConfig {
	return &ChainConfig{
		ChainID:            9002,
		ChainName:          "TigerSmartChain Testnet",
		Symbol:             "TGR",
		Decimals:          18,
		BlockTime:          3,
		MaxGasLimit:       30000000,
		MinGasLimit:       5000000,
		NetworkID:         9002,
		EIP150Block:       0,
		EIP155Block:       0,
		EIP158Block:       0,
		ByzantiumBlock:    0,
		ConstantinopleBlock: 0,
		PetersburgBlock:    0,
		IstanbulBlock:    0,
		EIP1559Block:     0,
		MergeBlock:       0,
	}
}

// DevChainConfig returns the development chain config.
func DevChainConfig() *ChainConfig {
	return &ChainConfig{
		ChainID:            9001,
		ChainName:          "TigerSmartChain Dev",
		Symbol:             "TGR",
		Decimals:          18,
		BlockTime:          3,
		MaxGasLimit:       30000000,
		MinGasLimit:       5000000,
		NetworkID:         9001,
		EIP150Block:       0,
		EIP155Block:       0,
		EIP158Block:       0,
		ByzantiumBlock:    0,
		ConstantinopleBlock: 0,
		PetersburgBlock:    0,
		IstanbulBlock:    0,
		EIP1559Block:     0,
		MergeBlock:       0,
	}
}

// DefaultAllocation returns the default mainnet allocation.
func DefaultAllocation() Allocation {
	alloc := make(Allocation)

	// Add initial TGR holders (example addresses)
	// These would be the initial token distribution
	alloc[types.HexToAddress("0x0000000000000000000000000000000000000001")] = AccountAlloc{
		Balance: "0",
		Nonce:   0,
	}

	return alloc
}

// TestnetAllocation returns the testnet allocation.
func TestnetAllocation() Allocation {
	return DefaultAllocation()
}

// DevAllocation returns the development allocation.
func DevAllocation() Allocation {
	alloc := make(Allocation)

	// Add some initial balance for development
	devAddr := types.HexToAddress("0xdeaddeaddeaddeaddeaddeaddeaddeaddeaddead")
	alloc[devAddr] = AccountAlloc{
		Balance: "100000000000000000000000", // 100,000 TGR
		Nonce:   0,
	}

	return alloc
}

// ToChainConfig converts genesis ChainConfig to types.ChainConfig.
func (g *Genesis) ToChainConfig() *types.ChainConfig {
	return &types.ChainConfig{
		ChainID:     g.Config.ChainID,
		ChainName:   g.Config.ChainName,
		Symbol:      g.Config.Symbol,
		Decimals:    g.Config.Decimals,
		BlockTime:   g.Config.BlockTime,
		MaxGasLimit: g.Config.MaxGasLimit,
		MinGasLimit: g.Config.MinGasLimit,
		NetworkID:   g.Config.NetworkID,
	}
}

// Hash returns the genesis block hash.
func (g *Genesis) Hash() (crypto.Hash, error) {
	data, err := json.Marshal(g)
	if err != nil {
		return crypto.Hash{}, err
	}
	return crypto.Keccak256Hash(data), nil
}

// ToBlock creates the genesis block.
func (g *Genesis) ToBlock() (*block.Block, error) {
	// Create genesis header
	header := &block.Header{
		Number:     big.NewInt(0),
		GasLimit:   g.Config.MaxGasLimit,
		Time:       g.Timestamp,
		Coinbase:   types.Address{},
		Difficulty: big.NewInt(1),
		Root:       crypto.Keccak256Hash([]byte{}),
		TxHash:     crypto.Keccak256Hash([]byte{}),
	}

	// Create genesis block
	return block.NewBlock(header, []interface{}{}), nil
}

// Load loads genesis from a file.
func Load(filename string) (*Genesis, error) {
	data, err := os.ReadFile(filename)
	if err != nil {
		return nil, err
	}

	var genesis Genesis
	if err := json.Unmarshal(data, &genesis); err != nil {
		return nil, err
	}

	return &genesis, nil
}

// Save saves genesis to a file.
func Save(g *Genesis, filename string) error {
	data, err := json.MarshalIndent(g, "", "  ")
	if err != nil {
		return err
	}

	return os.WriteFile(filename, data, 0644)
}

// MustLoad loads genesis from a file and panics on error.
func MustLoad(filename string) *Genesis {
	g, err := Load(filename)
	if err != nil {
		panic(err)
	}
	return g
}

// String returns a string representation of genesis.
func (g *Genesis) String() string {
	return fmt.Sprintf("Genesis{chainID: %d, validators: %d, alloc: %d}",
		g.Config.ChainID, len(g.Validators), len(g.Alloc))
}