// Package opcodes provides additional EVM opcodes for Fermi upgrade.
package opcodes

import (
	"fmt"
)

// =============================================================================
// EIP-5656: MCOPY - Memory Copy
// =============================================================================

// MCopy implements EIP-5656 memory copy instruction.
// Opcode: 0x5E
// Gas cost: 3 + 3*word_count
type MCopy struct{}

const (
	// Opcode number for MCOPY
	OpMCopy = 0x5E
	
	// Base gas cost
	mcopyBaseGas = 3
	
	// Gas per word
	mcopyWordGas = 3
)

// RequiredGas returns the gas required for MCOPY.
func (m *MCopy) RequiredGas(stack *Stack) uint64 {
	if len(stack.data) < 3 {
		return 0
	}
	
	// Get memory expansion cost
	memSize := stack.peek(2).Uint64()
	wordCount := (memSize + 31) / 32
	
	return mcopyBaseGas + wordCount*mcopyWordGas
}

// Execute performs the MCOPY operation.
func (m *MCopy) Execute(evm *EVM) error {
	if len(evm.Stack.data) < 3 {
		return fmt.Errorf("MCOPY requires 3 stack items")
	}
	
	destOffset := evm.Stack.pop().Uint64()
	srcOffset := evm.Stack.pop().Uint64()
	length := evm.Stack.pop().Uint64()
	
	// Expand memory if needed
	newMemSize := destOffset + length
	if newMemSize > uint64(len(evm.Memory)) {
		evm.Memory = append(evm.Memory, make([]byte, newMemSize-uint64(len(evm.Memory)))...
	}
	
	srcEnd := srcOffset + length
	if srcEnd > uint64(len(evm.Memory)) {
		evm.Memory = append(evm.Memory, make([]byte, srcEnd-uint64(len(evm.Memory)))...
	}
	
	// Copy memory
	copy(evm.Memory[destOffset:destOffset+length], evm.Memory[srcOffset:srcEnd])
	
	return nil
}

// =============================================================================
// EIP-1153: Transient Storage
// =============================================================================

// TransientStorage implements EIP-1153 transient storage.
// This provides non-persistent storage that lives only within a transaction.
// Opcodes: TLOAD (0x5C), TSTORE (0x5D)
type TransientStorage struct {
	storage map[TransientKey][]byte
}

type TransientKey struct {
	Address string
	Key     [32]byte
}

// NewTransientStorage creates a new transient storage.
func NewTransientStorage() *TransientStorage {
	return &TransientStorage{
		storage: make(map[TransientKey][]byte),
	}
}

// TLoad implements EIP-1153 TLOAD opcode.
// Opcode: 0x5C
type TLoad struct{}

const OpTLoad = 0x5C

// RequiredGas returns the gas required for TLOAD.
func (t *TLoad) RequiredGas(stack *Stack) uint64 {
	return 100 // Warm storage read
}

// Execute performs the TLOAD operation.
func (t *TLoad) Execute(evm *EVM) error {
	if len(evm.Stack.data) < 1 {
		return fmt.Errorf("TLOAD requires 1 stack item")
	}
	
	key := evm.Stack.pop()
	key32 := key.Bytes32()
	
	tsKey := TransientKey{
		Address: evm.Caller.String(),
		Key:     key32,
	}
	
	value := evm.TransientStorage.storage[tsKey]
	if value == nil {
		evm.Stack.push(NewZero())
	} else {
		evm.Stack.push(NewFromBytes(value))
	}
	
	return nil
}

// TStore implements EIP-1153 TSTORE opcode.
// Opcode: 0x5D
type TStore struct{}

const OpTStore = 0x5D

// RequiredGas returns the gas required for TSTORE.
func (t *TStore) RequiredGas(stack *Stack) uint64 {
	return 100 // Warm storage write
}

// Execute performs the TSTORE operation.
func (t *TStore) Execute(evm *EVM) error {
	if len(evm.Stack.data) < 2 {
		return fmt.Errorf("TSTORE requires 2 stack items")
	}
	
	key := evm.Stack.pop()
	key32 := key.Bytes32()
	value := evm.Stack.pop()
	
	tsKey := TransientKey{
		Address: evm.Caller.String(),
		Key:     key32,
	}
	
	if value.IsZero() {
		delete(evm.TransientStorage.storage, tsKey)
	} else {
		evm.TransientStorage.storage[tsKey] = value.Bytes()
	}
	
	return nil
}

// =============================================================================
// EIP-6780: SELFDESTRUCT Changes
// =============================================================================

// SelfDestruct behavior after EIP-6780:
// - SELFDESTRUCT only works in the same transaction that creates the contract
// - Otherwise, it just burns all gas and does nothing

// SelfDestruct implements the modified SELFDESTRUCT opcode.
type SelfDestruct struct{}

const OpSelfDestruct = 0xFF

// RequiredGas returns the gas required for SELFDESTRUCT.
func (s *SelfDestruct) RequiredGas(stack *Stack) uint64 {
	return 5000 // EIP-150 gas pricing
}

// Execute performs the SELFDESTRUCT operation with EIP-6780 rules.
func (s *SelfDestruct) Execute(evm *EVM) error {
	if len(evm.Stack.data) < 1 {
		return fmt.Errorf("SELFDESTRUCT requires 1 stack item")
	}
	
	beneficiary := evm.Stack.pop()
	
	// EIP-6780: Only works in same transaction as creation
	if evm.Contract.Created() {
		// Transfer balance
		balance := evm.StateDB.GetBalance(evm.Contract.Address())
		evm.StateDB.AddBalance(beneficiary.Bytes20(), balance)
		
		// Mark contract as destructed
		evm.Contract.MarkDestructed()
	} else {
		// EIP-6780: Just burn all gas, do nothing
		// This is a breaking change that prevents funds from being locked
		evm.Gas = 0
	}
	
	return nil
}

// =============================================================================
// EIP-4844: Blob Transaction Support
// =============================================================================

// BlobHash computes the versioned hash for blob commitments.
// This is used in EIP-4844 blob transactions.
type BlobHash struct{}

const OpBlobHash = 0x49

// BlobHash computes the blob transaction hash.
// Formula: blob_hash(version, index, commitments, proofs)
// Returns: keccak256(commitment)
func ComputeBlobHash(version byte, index uint64, commitment []byte) []byte {
	// Implementation would use proper SHA256 or keccak
	// For now, return mock hash
	return []byte{version, byte(index), 0x01, 0x02}
}

// RequiredGas returns the gas required for BLOBHASH.
func (b *BlobHash) RequiredGas(stack *Stack) uint64 {
	return 3 // Low gas cost
}

// Execute performs the BLOBHASH operation.
func (b *BlobHash) Execute(evm *EVM) error {
	if len(evm.Stack.data) < 1 {
		return fmt.Errorf("BLOBHASH requires 1 stack item")
	}
	
	index := evm.Stack.pop().Uint64()
	
	// Get blob hash at index
	blobHash := evm.GetBlobHash(index)
	evm.Stack.push(NewFromBytes(blobHash))
	
	return nil
}

// =============================================================================
// Opcode Registry
// =============================================================================

// Opcode represents an EVM opcode.
type Opcode interface {
	Execute(evm *EVM) error
	RequiredGas(stack *Stack) uint64
}

// Stack represents the EVM stack.
type Stack struct {
	data []*BigInt
}

type BigInt struct {
	val uint64
}

func NewZero() *BigInt {
	return &BigInt{val: 0}
}

func NewFromBytes(b []byte) *BigInt {
	if len(b) == 0 {
		return NewZero()
	}
	// Simplified - just return length as "hash"
	return &BigInt{val: uint64(len(b))}
}

func (b *BigInt) Uint64() uint64 {
	return b.val
}

func (b *BigInt) Bytes() []byte {
	return []byte{byte(b.val)}
}

func (b *BigInt) Bytes32() [32]byte {
	var res [32]byte
	res[0] = byte(b.val)
	return res
}

func (b *BigInt) IsZero() bool {
	return b.val == 0
}

// EVM minimal interface for opcode execution
type EVM struct {
	Stack          *Stack
	Memory         []byte
	Gas            uint64
	Caller         Address
	Contract       *Contract
	StateDB        StateDB
	TransientStorage *TransientStorage
}

type Address struct {
	bytes [20]byte
}

func (a Address) String() string {
	return fmt.Sprintf("0x%x", a.bytes[:])
}

func (a Address) Bytes20() []byte {
	return a.bytes[:]
}

type Contract struct {
	Address     Address
	Creator    Address
	destructed bool
}

func (c *Contract) Created() bool {
	return c.Creator.bytes == c.Address.bytes
}

func (c *Contract) MarkDestructed() {
	c.destructed = true
}

type StateDB interface {
	GetBalance(addr []byte) uint64
	AddBalance(addr []byte, amount uint64)
}

// GetBlobHash returns blob hash at given index.
func (e *EVM) GetBlobHash(index uint64) []byte {
	// In production, retrieve from blob pool
	return []byte{0x01, 0x02, 0x03}
}

var _ = fmt.Sprintf // Use fmt
