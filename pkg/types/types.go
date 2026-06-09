// Package types provides common type definitions for TigerSmartChain.
package types

import (
	"math/big"

	"github.com/ethereum/go-ethereum/common"
)

// Address represents a 20-byte Ethereum-style address.
type Address = common.Address

// HashToAddress converts a hash to an address.
func HashToAddress(h Hash) Address {
	b := make([]byte, len(h))
	copy(b, h[:])
	if len(b) >= 20 {
		b = b[:20]
	}
	return common.BytesToAddress(b)
}

// HexToAddress converts a hex string to an address.
func HexToAddress(s string) Address {
	return common.HexToAddress(s)
}

// Hash represents a 32-byte cryptographic hash.
type Hash = common.Hash

// Bloom represents a bloom filter.
type Bloom [256]byte

// Chain configuration constants
const (
	ChainID    = 9001
	ChainName  = "TigerSmartChain"
	Ticker    = "Tiger"
	Symbol    = "TGR"
	Decimals   = 18
	BlockTime = 3
	MaxGasLimit = 30000000
	MinGasLimit = 5000000
)

// ChainConfig contains the chain configuration.
type ChainConfig struct {
	ChainID     uint64
	ChainName   string
	Symbol     string
	Decimals   uint8
	BlockTime  uint64
	MaxGasLimit uint64
	MinGasLimit uint64
	NetworkID  uint64
}

func DefaultChainConfig() *ChainConfig {
	return &ChainConfig{
		ChainID:     ChainID,
		ChainName:   ChainName,
		Symbol:     Symbol,
		Decimals:   Decimals,
		BlockTime:  BlockTime,
		MaxGasLimit: MaxGasLimit,
		MinGasLimit: MinGasLimit,
		NetworkID:  ChainID,
	}
}

func (c *ChainConfig) String() string {
	return c.ChainName
}

// NativeCoinInfo returns information about the native coin.
type NativeCoinInfo struct {
	Name     string
	Symbol  string
	Decimals uint8
	Supply  *big.Int
}

func TGRInfo() *NativeCoinInfo {
	return &NativeCoinInfo{
		Name:     "Tiger Coin",
		Symbol:  Symbol,
		Decimals: Decimals,
		Supply:  new(big.Int).Mul(big.NewInt(1e18), big.NewInt(1e9)),
	}
}