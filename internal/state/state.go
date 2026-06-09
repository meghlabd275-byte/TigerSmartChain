// Package state provides state management for TigerSmartChain.
package state

import (
	"fmt"
	"math/big"
	"sync"

	"github.com/tigersmartchain/tigersmartchain/internal/state/trie"
	"github.com/tigersmartchain/tigersmartchain/pkg/crypto"
	"github.com/tigersmartchain/tigersmartchain/pkg/types"
)

// StateDB represents the state database.
type StateDB struct {
	mu sync.RWMutex

	// db is the underlying database
	db *trie.StateTrie

	// accounts is the in-memory account cache
	accounts map[types.Address]*StateObject

	// code cache
	code map[types.Address][]byte
}

// StateObject represents a state account.
type StateObject struct {
	// Address is the account address
	Address types.Address

	// Balance is the account balance
	Balance *big.Int

	// Nonce is the account nonce
	Nonce uint64

	// CodeHash is the hash of the contract code
	CodeHash crypto.Hash

	// Code is the contract code
	Code []byte

	// Storage is the contract storage
	Storage map[crypto.Hash]crypto.Hash
}

// NewStateDB creates a new state database.
func NewStateDB(root crypto.Hash) (*StateDB, error) {
	db := &StateDB{
		accounts: make(map[types.Address]*StateObject),
		code:    make(map[types.Address][]byte),
	}

	// Initialize the state trie
	t, err := trie.NewStateTrie(root)
	if err != nil {
		return nil, err
	}
	db.db = t

	return db, nil
}

// GetAccount returns the state object for an address.
func (s *StateDB) GetAccount(addr types.Address) *StateObject {
	s.mu.RLock()
	defer s.mu.RUnlock()

	return s.accounts[addr]
}

// GetOrNewStateObject returns the state object for an address, creating if it doesn't exist.
func (s *StateDB) GetOrNewStateObject(addr types.Address) *StateObject {
	s.mu.Lock()
	defer s.mu.Unlock()

	obj, ok := s.accounts[addr]
	if !ok {
		obj = &StateObject{
			Address: addr,
			Balance: big.NewInt(0),
			Storage: make(map[crypto.Hash]crypto.Hash),
		}
		s.accounts[addr] = obj
	}

	return obj
}

// CreateAccount creates a new account.
func (s *StateDB) CreateAccount(addr types.Address) *StateObject {
	s.mu.Lock()
	defer s.mu.Unlock()

	obj := &StateObject{
		Address: addr,
		Balance: big.NewInt(0),
		Nonce:  0,
		Storage: make(map[crypto.Hash]crypto.Hash),
	}
	s.accounts[addr] = obj

	return obj
}

// DeleteAccount deletes an account.
func (s *StateDB) DeleteAccount(addr types.Address) {
	s.mu.Lock()
	defer s.mu.Unlock()

	delete(s.accounts, addr)
}

// GetBalance returns the balance of an account.
func (s *StateDB) GetBalance(addr types.Address) *big.Int {
	obj := s.GetAccount(addr)
	if obj == nil {
		return big.NewInt(0)
	}
	return new(big.Int).Set(obj.Balance)
}

// SetBalance sets the balance of an account.
func (s *StateDB) SetBalance(addr types.Address, balance *big.Int) {
	obj := s.GetOrNewStateObject(addr)
	obj.Balance = new(big.Int).Set(balance)
}

// GetNonce returns the nonce of an account.
func (s *StateDB) GetNonce(addr types.Address) uint64 {
	obj := s.GetAccount(addr)
	if obj == nil {
		return 0
	}
	return obj.Nonce
}

// SetNonce sets the nonce of an account.
func (s *StateDB) SetNonce(addr types.Address, nonce uint64) {
	obj := s.GetOrNewStateObject(addr)
	obj.Nonce = nonce
}

// GetCode returns the code of an account.
func (s *StateDB) GetCode(addr types.Address) []byte {
	s.mu.RLock()
	defer s.mu.RUnlock()

	// Check cache first
	if code, ok := s.code[addr]; ok {
		return code
	}

	// Get from state
	obj := s.accounts[addr]
	if obj != nil && len(obj.Code) > 0 {
		return obj.Code
	}

	return nil
}

// SetCode sets the code of an account.
func (s *StateDB) SetCode(addr types.Address, code []byte) {
	obj := s.GetOrNewStateObject(addr)
	obj.Code = code
	obj.CodeHash = crypto.Keccak256Hash(code)

	s.mu.Lock()
	s.code[addr] = code
	s.mu.Unlock()
}

// GetCodeHash returns the code hash of an account.
func (s *StateDB) GetCodeHash(addr types.Address) crypto.Hash {
	obj := s.GetAccount(addr)
	if obj == nil {
		return crypto.Hash{}
	}
	return obj.CodeHash
}

// GetState returns a storage slot.
func (s *StateDB) GetState(addr types.Address, key crypto.Hash) crypto.Hash {
	obj := s.GetAccount(addr)
	if obj == nil {
		return crypto.Hash{}
	}
	return obj.Storage[key]
}

// SetState sets a storage slot.
func (s *StateDB) SetState(addr types.Address, key, value crypto.Hash) {
	obj := s.GetOrNewStateObject(addr)
	obj.Storage[key] = value
}

// GetCommittedState returns a committed storage slot.
func (s *StateDB) GetCommittedState(addr types.Address, key crypto.Hash) crypto.Hash {
	// This would get from the underlying trie
	return crypto.Hash{}
}

// AddBalance adds balance to an account.
func (s *StateDB) AddBalance(addr types.Address, amount *big.Int) {
	obj := s.GetOrNewStateObject(addr)
	obj.Balance.Add(obj.Balance, amount)
}

// SubBalance subtracts balance from an account.
func (s *StateDB) SubBalance(addr types.Address, amount *big.Int) {
	obj := s.GetOrNewStateObject(addr)
	obj.Balance.Sub(obj.Balance, amount)
}

// SetStorage sets the entire storage map.
func (s *StateDB) SetStorage(addr types.Address, storage map[crypto.Hash]crypto.Hash) {
	obj := s.GetOrNewStateObject(addr)
	obj.Storage = storage
}

// Prepare resets the journal for a new block.
func (s *StateDB) Prepare() {
	// This would prepare for a new block
}

// Finalize finalizes the state.
func (s *StateDB) Finalize() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Write all changes to the trie
	for addr, obj := range s.accounts {
		if err := s.db.UpdateAccount(addr, obj); err != nil {
			return err
		}
	}

	return nil
}

// Root returns the state root.
func (s *StateDB) Root() crypto.Hash {
	return s.db.Root()
}

// Reset resets the state to a new root.
func (s *StateDB) Reset(root crypto.Hash) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Create a new trie
	t, err := trie.NewStateTrie(root)
	if err != nil {
		return err
	}
	s.db = t

	// Clear caches
	s.accounts = make(map[types.Address]*StateObject)
	s.code = make(map[types.Address][]byte)

	return nil
}

// RevertToSnapshot reverts to a snapshot.
func (s *StateDB) RevertToSnapshot(revid int64) error {
	// This would revert to a snapshot
	return fmt.Errorf("not implemented")
}

// Snapshot creates a snapshot.
func (s *StateDB) Snapshot() int64 {
	return 0
}

// GetOrNewStateObjectWithNonce gets or creates a state object with nonce.
func (s *StateDB) GetOrNewStateObjectWithNonce(addr types.Address, nonce uint64) *StateObject {
	obj := s.GetOrNewStateObject(addr)
	if obj.Nonce == 0 {
		obj.Nonce = nonce
	}
	return obj
}

// ForEachStorage iterates over all storage slots.
func (s *StateDB) ForEachStorage(addr types.Address, callback func(key, value crypto.Hash) bool) {
	obj := s.GetAccount(addr)
	if obj == nil {
		return
	}

	for key, value := range obj.Storage {
		if !callback(key, value) {
			return
		}
	}
}

// SubNonce subtracts from the account nonce.
func (s *StateDB) SubNonce(addr types.Address, delta uint64) {
	obj := s.GetOrNewStateObject(addr)
	if obj.Nonce < delta {
		obj.Nonce = 0
	} else {
		obj.Nonce -= delta
	}
}

// IsContract returns true if the address is a contract.
func (s *StateDB) IsContract(addr types.Address) bool {
	code := s.GetCode(addr)
	return len(code) > 0
}

// String returns a string representation of the state object.
func (so *StateObject) String() string {
	return fmt.Sprintf("StateObject{addr: %s, balance: %s, nonce: %d}",
		so.Address.Hex(), so.Balance.String(), so.Nonce)
}