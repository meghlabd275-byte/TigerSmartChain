// Package opcodes provides EVM opcode implementations.
// This file implements EIP-1153 Transient Storage Opcodes (TLOAD and TSTORE).
package opcodes

import (
	"fmt"

	"github.com/tigersmartchain/tigersmartchain/internal/evm/gas-meter"
	"github.com/tigersmartchain/tigersmartchain/internal/evm/memory"
	"github.com/tigersmartchain/tigersmartchain/internal/evm/stack"
)

// TransientStorage provides in-memory transient storage for EIP-1153.
// Transient storage is cleared after each transaction, unlike persistent storage.
type TransientStorage struct {
	// Storage is a map of address -> key -> value
	storage map[string]map[string][]byte
}

// NewTransientStorage creates a new transient storage instance.
func NewTransientStorage() *TransientStorage {
	return &TransientStorage{
		storage: make(map[string]map[string][]byte),
	}
}

// Get retrieves a value from transient storage.
func (ts *TransientStorage) Get(addr, key string) ([]byte, bool) {
	if storage, ok := ts.storage[addr]; ok {
		val, ok := storage[key]
		return val, ok
	}
	return nil, false
}

// Set sets a value in transient storage.
func (ts *TransientStorage) Set(addr, key string, value []byte) {
	if _, ok := ts.storage[addr]; !ok {
		ts.storage[addr] = make(map[string][]byte)
	}
	ts.storage[addr][key] = value
}

// Clear clears all transient storage (called after transaction).
func (ts *TransientStorage) Clear() {
	ts.storage = make(map[string]map[string][]byte)
}

// TLOAD implements the TLOAD opcode (EIP-1153).
// TLOAD loads a value from transient storage.
// Gas: 100 gas (warm) or 1000 gas (cold)
type TLOAD struct{}

// OpName returns the opcode name.
func (op *TLOAD) OpName() string {
	return "TLOAD"
}

// IsDynamic returns whether the opcode is dynamic.
func (op *TLOAD) IsDynamic() bool {
	return true
}

// Execute executes the TLOAD opcode.
func (op *TLOAD) Execute(evm *EVM, stack *stack.Stack, memory *memory.Memory, gasMeter *gas-meter.GasMeter) error {
	// Pop key from stack
	key := stack.Pop()
	
	// Get address from current context
	addr := evm.CurrentContract().Address()
	
	// Check if access is warm or cold
	keyStr := string(key)
	_, isWarm := evm.TransientStorage().Get(addr, keyStr)
	
	// Calculate gas
	if isWarm {
		gasMeter.UseGas(100, "TLOAD-warm")
	} else {
		gasMeter.UseGas(1000, "TLOAD-cold")
		// Mark as warm
		evm.TransientStorage().Set(addr, keyStr, []byte{})
	}
	
	// Get value from transient storage
	value, _ := evm.TransientStorage().Get(addr, keyStr)
	if value == nil {
		value = make([]byte, 32)
	}
	
	// Push value to stack
	stack.Push(value)
	
	return nil
}

// TSTORE implements the TSTORE opcode (EIP-1153).
// TSTORE stores a value in transient storage.
// Gas: 100 gas (warm) or 10000 gas (cold)
type TSTORE struct{}

// OpName returns the opcode name.
func (op *TSTORE) OpName() string {
	return "TSTORE"
}

// IsDynamic returns whether the opcode is dynamic.
func (op *TSTORE) IsDynamic() bool {
	return true
}

// Execute executes the TSTORE opcode.
func (op *TSTORE) Execute(evm *EVM, stack *stack.Stack, memory *memory.Memory, gasMeter *gas-meter.GasMeter) error {
	// Pop key and value from stack
	key := stack.Pop()
	value := stack.Pop()
	
	// Get address from current context
	addr := evm.CurrentContract().Address()
	
	// Check if key exists
	keyStr := string(key)
	_, exists := evm.TransientStorage().Get(addr, keyStr)
	
	// Calculate gas
	if exists {
		gasMeter.UseGas(100, "TSTORE-warm")
	} else {
		gasMeter.UseGas(10000, "TSTORE-cold")
	}
	
	// Cannot store to static call
	if evm.CurrentContract().IsStatic() {
		return fmt.Errorf("cannot SSTORE in static call")
	}
	
	// Store value in transient storage
	evm.TransientStorage().Set(addr, keyStr, value)
	
	return nil
}

// MCOPY implements the MCOPY opcode (EIP-5656).
// MCOPY copies memory areas.
// Gas: 3 gas per word + memory expansion
type MCOPY struct{}

// OpName returns the opcode name.
func (op *MCOPY) OpName() string {
	return "MCOPY"
}

// IsDynamic returns whether the opcode is dynamic.
func (op *MCOPY) IsDynamic() bool {
	return true
}

// Execute executes the MCOPY opcode.
func (op *MCOPY) Execute(evm *EVM, stack *stack.Stack, memory *memory.Memory, gasMeter *gas_meter.GasMeter) error {
	// Pop destOffset, sourceOffset, length from stack
	destOffset := stack.Pop()
	sourceOffset := stack.Pop()
	length := stack.Pop()
	
	// Calculate word count for gas (rounded up)
	wordCount := (length + 31) / 32
	gasMeter.UseGas(3*wordCount, "MCOPY")
	
	// Expand memory if needed
	memory.Ensure(length.Add(destOffset, length))
	memory.Ensure(length.Add(sourceOffset, length))
	
	// Copy memory
	memory.Copy(destOffset, sourceOffset, length)
	
	return nil
}