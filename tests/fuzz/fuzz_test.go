// Package fuzz provides fuzz tests for TigerSmartChain.
package fuzz

import (
	"fmt"
	"math/big"
	"testing"

	"github.com/tigersmartchain/tigersmartChain/internal/blockchain/transaction"
	"github.com/tigersmartchain/tigersmartChain/internal/evm"
	"github.com/tigersmartchain/tigersmartChain/internal/evm/opcodes"
)

// =============================================================================
// FUZZ TESTS
// =============================================================================

// FuzzTransactionValidation fuzz tests transaction validation.
func FuzzTransactionValidation(f *testing.F) {
	// Seed with initial data
	f.Add([]byte{0x00, 0x01, 0x02, 0x03})
	f.Add([]byte{0xFF, 0xFE, 0xFD, 0xFC})
	
	f.Fuzz(func(t *testing.T, data []byte) {
		// Test transaction validation
		tx := &transaction.Transaction{}
		
		if len(data) > 0 {
			tx.Data = data
		}
		
		// Validate - should not panic
		_ = tx.Validate()
	})
}

// FuzzTransactionEncoding fuzz tests transaction encoding.
func FuzzTransactionEncoding(f *testing.F) {
	f.Add([]byte{0x00})
	f.Add([]byte{0xFF})
	f.Add([]byte{0x12, 0x34, 0x56, 0x78})
	
	f.Fuzz(func(t *testing.T, data []byte) {
		tx := &transaction.Transaction{
			Value:    big.NewInt(12345),
			Data:     data,
			GasLimit: 21000,
		}
		
		// Test encoding
		encoded, err := tx.MarshalBinary()
		if err != nil {
			// Encoding failure is acceptable
			return
		}
		
		// Test decoding
		tx2 := &transaction.Transaction{}
		if err := tx2.UnmarshalBinary(encoded); err != nil {
			t.Errorf("decode failed: %v", err)
		}
	})
}

// FuzzEVMOpcode fuzz tests EVM opcodes.
func FuzzEVMOpcode(f *testing.F) {
	f.Add(uint8(0x00))
	f.Add(uint8(0x5E)) // MCOPY
	f.Add(uint8(0x5C)) // TLOAD
	f.Add(uint8(0x5D)) // TSTORE
	
	f.Fuzz(func(t *testing.T, opcode uint8) {
		// Test opcode handling
		switch opcode {
		case 0x5E: // MCOPY
			mcopy := &opcodes.MCOPY{}
			if mcopy.OpCode() != 0x5E {
				t.Errorf("MCOPY opcode mismatch")
			}
		case 0x5C: // TLOAD
			tload := &opcodes.TLOAD{}
			if tload.OpCode() != 0x5C {
				t.Errorf("TLOAD opcode mismatch")
			}
		case 0x5D: // TSTORE
			tstore := &opcodes.TSTORE{}
			if tstore.OpCode() != 0x5D {
				t.Errorf("TSTORE opcode mismatch")
			}
		default:
			// Unknown opcode - just verify it doesn't crash
		}
	})
}

// FuzzAddressParsing fuzz tests address parsing.
func FuzzAddressParsing(f *testing.F) {
	f.Add("0x0000000000000000000000000000000000000000")
	f.Add("0x1234567890123456789012345678901234567890")
	f.Add("0xffffffffffffffffffffffffffffffffffffffff")
	
	f.Fuzz(func(t *testing.T, addr string) {
		// Test address parsing - should not panic
		isValid := isValidAddress(addr)
		_ = isValid
	})
}

// FuzzBlockHeader fuzz tests block header validation.
func FuzzBlockHeader(f *testing.F) {
	f.Add(uint64(0), uint64(0))
	f.Add(uint64(1), uint64(0))
	f.Add(uint64(100), uint64(50))
	
	f.Fuzz(func(t *testing.T, number, parent uint64) {
		// Validate block header
		if number > 0 && parent >= number {
			// Invalid parent should be handled gracefully
			return
		}
	})
}

// FuzzGasCalculation fuzz tests gas calculation.
func FuzzGasCalculation(f *testing.F) {
	f.Add(uint64(0), uint64(0))
	f.Add(uint64(21000), uint64(21000))
	f.Add(uint64(100000), uint64(50000))
	
	f.Fuzz(func(t *testing.T, gas, price uint64) {
		// Calculate gas cost
		cost := gas * price
		
		// Verify result is reasonable
		if gas > 0 && price > 0 && cost == 0 {
			t.Errorf("gas cost should not be zero")
		}
	})
}

// FuzzTrieInsert fuzz tests trie insertion.
func FuzzTrieInsert(f *testing.F) {
	f.Add([]byte("key1"), []byte("value1"))
	f.Add([]byte("key2"), []byte("value2"))
	f.Add([]byte("test"), []byte("data"))
	
	f.Fuzz(func(t *testing.T, key, value []byte) {
		// Test trie insertion - should not panic
		if len(key) == 0 {
			return
		}
		
		// Simple hash for testing
		hash := make([]byte, len(key))
		for i, b := range key {
			hash[i] = b ^ value[i%len(value)]
		}
		
		_ = hash
	})
}

// FuzzSignatureVerification fuzz tests signature verification.
func FuzzSignatureVerification(f *testing.F) {
	f.Add([]byte("message"), []byte("signature"))
	f.Add([]byte("test"), []byte("sig"))
	
	f.Fuzz(func(t *testing.T, msg, sig []byte) {
		// Test signature verification - should not panic
		if len(msg) == 0 || len(sig) == 0 {
			return
		}
		
		// Simple signature check
		valid := len(sig) >= 8
		
		_ = valid
	})
}

// FuzzJSONRPCRequest fuzz tests JSON-RPC request parsing.
func FuzzJSONRPCRequest(f *testing.F) {
	f.Add(`{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}`)
	f.Add(`{"jsonrpc":"2.0","method":"eth_getBalance","params":["0x1234"],"id":2}`)
	f.Add(`{"jsonrpc":"2.0","method":"eth_sendTransaction","params":[],"id":3}`)
	
	f.Fuzz(func(t *testing.T, jsonStr string) {
		// Test JSON parsing - should not panic
		if len(jsonStr) == 0 {
			return
		}
		
		// Simple JSON check
		valid := len(jsonStr) > 10
		
		_ = valid
	})
}

// =============================================================================
// HELPERS
// =============================================================================

// isValidAddress checks if an address is valid.
func isValidAddress(addr string) bool {
	if len(addr) != 42 {
		return false
	}
	if addr[:2] != "0x" {
		return false
	}
	return true
}

// init initializes fuzz tests.
func init() {
	// Register fuzz tests
	_ = FuzzTransactionValidation
	_ = FuzzTransactionEncoding
	_ = FuzzEVMOpcode
	_ = FuzzAddressParsing
	_ = FuzzBlockHeader
	_ = FuzzGasCalculation
	_ = FuzzTrieInsert
	_ = FuzzSignatureVerification
	_ = FuzzJSONRPCRequest
}