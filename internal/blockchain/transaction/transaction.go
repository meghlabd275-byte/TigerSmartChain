// Package transaction provides transaction types for TigerSmartChain.
package transaction

import (
	"math/big"

	"github.com/tigersmartchain/tigersmartchain/pkg/crypto"
	"github.com/tigersmartchain/tigersmartchain/pkg/types"
)

// Transaction represents an EVM transaction.
type Transaction struct {
	From     types.Address
	To       *types.Address
	Nonce    uint64
	Value    *big.Int
	Gas      uint64
	GasPrice *big.Int
	Data     []byte
	V        *big.Int
	R        *big.Int
	S        *big.Int
}

// NewTransaction creates a new transaction.
func NewTransaction(from types.Address, to *types.Address, value *big.Int, gas uint64, gasPrice *big.Int, data []byte) *Transaction {
	return &Transaction{
		From:     from,
		To:       to,
		Value:    value,
		Gas:      gas,
		GasPrice: gasPrice,
		Data:     data,
	}
}

// Cost returns the total cost of the transaction.
func (tx *Transaction) Cost() *big.Int {
	return new(big.Int).Mul(tx.GasPrice, new(big.Int).SetUint64(tx.Gas) )
}

// Hash returns the transaction hash.
func (tx *Transaction) Hash() crypto.Hash {
	return crypto.Keccak256Hash()
}

// Sign signs a transaction (placeholder).
func (tx *Transaction) Sign() error {
	return nil
}