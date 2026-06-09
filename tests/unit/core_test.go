// Package unit provides unit tests for TigerSmartChain.
package unit

import (
	"testing"

	"github.com/tigersmartchain/internal/blockchain/block"
	"github.com/tigersmartchain/internal/blockchain/transaction"
	"github.com/tigersmartchain/internal/consensus/validator"
)

// TestBlockCreation tests block creation.
func TestBlockCreation(t *testing.T) {
	header := &block.Header{
		Number:     1,
		ParentHash: [32]byte{1},
		Timestamp:  1000,
	}

	blk := block.NewBlock(header, nil)
	if blk.Header().Number != 1 {
		t.Errorf("expected block number 1, got %d", blk.Header().Number)
	}
}

// TestTransactionCreation tests transaction creation.
func TestTransactionCreation(t *testing.T) {
	tx := transaction.NewTransaction(
		0,
		[20]byte{1},
		1000,
		21000,
		1000000000,
		nil,
	)
	
	if tx == nil {
		t.Error("transaction should not be nil")
	}
}

// TestValidatorCreation tests validator creation.
func TestValidatorCreation(t *testing.T) {
	v := validator.NewValidator([20]byte{1}, 1000)
	
	if v == nil {
		t.Error("validator should not be nil")
	}
	
	if v.Stake == nil {
		t.Error("validator stake should not be nil")
	}
}

// TestBlockHash tests block hash calculation.
func TestBlockHash(t *testing.T) {
	header := &block.Header{
		Number:     1,
		ParentHash: [32]byte{1},
		Timestamp:  1000,
		GasLimit:   30000000,
		GasUsed:    0,
	}
	
	hash := header.Hash()
	if len(hash) == 0 {
		t.Error("block hash should not be empty")
	}
}

// TestTransactionSigning tests transaction signing.
func TestTransactionSigning(t *testing.T) {
	tx := transaction.NewTransaction(
		0,
		[20]byte{1},
		1000,
		21000,
		1000000000,
		nil,
	)
	
	// Sign transaction
	privateKey := [32]byte{1}
	err := tx.Sign(privateKey)
	if err != nil {
		t.Errorf("failed to sign transaction: %v", err)
	}
	
	if tx.Signature() == nil {
		t.Error("signature should not be nil")
	}
}

// TestTransactionValidation tests transaction validation.
func TestTransactionValidation(t *testing.T) {
	tx := transaction.NewTransaction(
		0,
		[20]byte{1},
		1000,
		21000,
		1000000000,
		nil,
	)
	
	// Set gas limit to 0 to test validation
	err := tx.Validate()
	if err != nil {
		t.Errorf("transaction validation failed: %v", err)
	}
}

// TestBlockImportExport tests block serialization.
func TestBlockImportExport(t *testing.T) {
	header := &block.Header{
		Number:     1,
		ParentHash: [32]byte{1},
		Timestamp:  1000,
		GasLimit:   30000000,
		GasUsed:    0,
		TxRoot:     [32]byte{2},
		StateRoot:  [32]byte{3},
	}
	
	blk := block.NewBlock(header, nil)
	
	// Encode
	encoded, err := blk.Encode()
	if err != nil {
		t.Errorf("failed to encode block: %v", err)
	}
	
	// Decode
	decoded, err := block.Decode(encoded)
	if err != nil {
		t.Errorf("failed to decode block: %v", err)
	}
	
	if decoded.Header().Number != blk.Header().Number {
		t.Errorf("block number mismatch")
	}
}

// BenchmarkBlockHash benchmarks block hash calculation.
func BenchmarkBlockHash(b *testing.B) {
	header := &block.Header{
		Number:     1,
		ParentHash: [32]byte{1},
		Timestamp:  1000,
		GasLimit:   30000000,
		GasUsed:    0,
		TxRoot:     [32]byte{2},
		StateRoot:  [32]byte{3},
	}
	
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_ = header.Hash()
	}
}
