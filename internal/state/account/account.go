// Package account provides account state management.
package account

import (
	"encoding/hex"
	"fmt"
	"math/big"
	"sync"

	"github.com/ethereum/go-ethereum/rlp"
	"github.com/tigersmartchain/tigersmartchain/internal/crypto"
)

// Account represents an Ethereum account.
type Account struct {
	Nonce    uint64
	Balance  *big.Int
	Root     []byte // Merkle root of storage trie
	CodeHash []byte // Hash of contract code
}

// Empty returns if account is empty.
func (a *Account) Empty() bool {
	return a.Nonce == 0 && a.Balance.Sign() == 0 && len(a.CodeHash) == 0
}

// StateObject represents the account state.
type StateObject struct {
	address   string
	nonce    uint64
	balance  *big.Int
	codeHash []byte
	storage  map[string][]byte
	code    []byte
	dirty   bool

	// Original values
	originNonce    uint64
	originBalance *big.Int
}

// NewStateObject creates a new state object.
func NewStateObject(address string) *StateObject {
	return &StateObject{
		address:  address,
		balance: new(big.Int),
		storage: make(map[string][]byte),
	}
}

// Address returns the account address.
func (s *StateObject) Address() string {
	return s.address
}

// Balance returns the account balance.
func (s *StateObject) Balance() *big.Int {
	return s.balance
}

// Nonce returns the account nonce.
func (s *StateObject) Nonce() uint64 {
	return s.nonce
}

// Code returns the contract code.
func (s *StateObject) Code() []byte {
	return s.code
}

// CodeHash returns the code hash.
func (s *StateObject) CodeHash() []byte {
	if len(s.codeHash) == 0 && len(s.code) > 0 {
		s.codeHash = crypto.Keccak256(s.code)
	}
	return s.codeHash
}

// SetBalance sets the account balance.
func (s *StateObject) SetBalance(balance *big.Int) {
	s.dirty = true
	s.balance.Set(balance)
}

// AddBalance adds to the account balance.
func (s *StateObject) AddBalance(amount *big.Int) {
	if amount.Sign() == 0 {
		return
	}
	s.dirty = true
	s.balance.Add(s.balance, amount)
}

// SubBalance subtracts from the account balance.
func (s *StateObject) SubBalance(amount *big.Int) {
	if amount.Sign() == 0 {
		return
	}
	if s.balance.Cmp(amount) < 0 {
		panic("insufficient balance")
	}
	s.dirty = true
	s.balance.Sub(s.balance, amount)
}

// SetNonce sets the account nonce.
func (s *StateObject) SetNonce(nonce uint64) {
	s.dirty = true
	s.nonce = nonce
}

// SetCode sets the contract code.
func (s *StateObject) SetCode(code []byte) {
	s.dirty = true
	s.code = code
	s.codeHash = crypto.Keccak256(code)
}

// SetStorage sets a storage value.
func (s *StateObject) SetStorage(key, value []byte) {
	s.dirty = true
	s.storage[hex.EncodeToString(key)] = value
}

// GetStorage returns a storage value.
func (s *StateObject) GetStorage(key []byte) []byte {
	return s.storage[hex.EncodeToString(key)]
}

// IsContract returns if the account is a contract.
func (s *StateObject) IsContract() bool {
	return len(s.code) > 0
}

// IsDirty returns if the account has been modified.
func (s *StateObject) IsDirty() bool {
	return s.dirty
}

// Commit commits the state changes.
func (s *StateObject) Commit() error {
	s.dirty = false
	return nil
}

// StateStore represents the state storage interface.
type StateStore interface {
	GetStateObject(addr string) (*StateObject, bool)
	SetStateObject(obj *StateObject)
	DeleteStateObject(addr string)
	Commit() error
}

// AccountState represents the account state management.
type AccountState struct {
	mu sync.RWMutex
	// Account objects
	objects map[string]*StateObject
	// Storage backend
	store StateStore
}

// NewAccountState creates a new account state.
func NewAccountState() *AccountState {
	return &AccountState{
		objects: make(map[string]*StateObject),
	}
}

// GetOrNewStateObject returns or creates a state object.
func (as *AccountState) GetOrNewStateObject(addr string) *StateObject {
	as.mu.Lock()
	defer as.mu.Unlock()

	obj, ok := as.objects[addr]
	if !ok {
		obj = NewStateObject(addr)
		as.objects[addr] = obj
	}

	return obj
}

// GetStateObject returns a state object.
func (as *AccountState) GetStateObject(addr string) (*StateObject, bool) {
	as.mu.RLock()
	defer as.mu.RUnlock()

	obj, ok := as.objects[addr]
	return obj, ok
}

// GetAccount returns account info.
func (as *AccountState) GetAccount(addr string) (*Account, bool) {
	as.mu.RLock()
	defer as.mu.RUnlock()

	obj, ok := as.objects[addr]
	if !ok {
		return nil, false
	}

	return &Account{
		Nonce:    obj.nonce,
		Balance:  new(big.Int).Set(obj.balance),
		CodeHash: obj.CodeHash(),
	}, true
}

// GetBalance returns account balance.
func (as *AccountState) GetBalance(addr string) *big.Int {
	as.mu.RLock()
	defer as.mu.RUnlock()

	obj, ok := as.objects[addr]
	if !ok {
		return new(big.Int)
	}
	return new(big.Int).Set(obj.balance)
}

// GetNonce returns account nonce.
func (as *AccountState) GetNonce(addr string) uint64 {
	as.mu.RLock()
	defer as.mu.RUnlock()

	obj, ok := as.objects[addr]
	if !ok {
		return 0
	}
	return obj.nonce
}

// GetCode returns contract code.
func (as *AccountState) GetCode(addr string) []byte {
	as.mu.RLock()
	defer as.mu.RUnlock()

	obj, ok := as.objects[addr]
	if !ok {
		return nil
	}
	return obj.code
}

// GetCodeHash returns code hash.
func (as *AccountState) GetCodeHash(addr string) []byte {
	as.mu.RLock()
	defer as.mu.RUnlock()

	obj, ok := as.objects[addr]
	if !ok {
		return nil
	}
	return obj.CodeHash()
}

// GetStorage returns storage value.
func (as *AccountState) GetStorage(addr string, key []byte) []byte {
	as.mu.RLock()
	defer as.mu.RUnlock()

	obj, ok := as.objects[addr]
	if !ok {
		return nil
	}
	return obj.GetStorage(key)
}

// AddBalance adds balance to an account.
func (as *AccountState) AddBalance(addr string, amount *big.Int) error {
	if amount.Sign() <= 0 {
		return fmt.Errorf("invalid amount")
	}

	obj := as.GetOrNewStateObject(addr)
	obj.AddBalance(amount)
	return nil
}

// SubBalance subtracts balance from an account.
func (as *AccountState) SubBalance(addr string, amount *big.Int) error {
	if amount.Sign() <= 0 {
		return fmt.Errorf("invalid amount")
	}

	obj := as.GetOrNewStateObject(addr)
	if obj.Balance().Cmp(amount) < 0 {
		return fmt.Errorf("insufficient balance")
	}
	obj.SubBalance(amount)
	return nil
}

// Transfer transfers balance between accounts.
func (as *AccountState) Transfer(from, to string, amount *big.Int) error {
	if amount.Sign() <= 0 {
		return fmt.Errorf("invalid amount")
	}

	// Subtract from sender
	fromObj := as.GetOrNewStateObject(from)
	if fromObj.Balance().Cmp(amount) < 0 {
		return fmt.Errorf("insufficient balance")
	}
	fromObj.SubBalance(amount)

	// Add to receiver
	toObj := as.GetOrNewStateObject(to)
	toObj.AddBalance(amount)

	return nil
}

// SetNonce sets account nonce.
func (as *AccountState) SetNonce(addr string, nonce uint64) {
	obj := as.GetOrNewStateObject(addr)
	obj.SetNonce(nonce)
}

// IncrementNonce increments account nonce.
func (as *AccountState) IncrementNonce(addr string) {
	obj := as.GetOrNewStateObject(addr)
	obj.SetNonce(obj.Nonce() + 1)
}

// SetCode sets contract code.
func (as *AccountState) SetCode(addr string, code []byte) {
	obj := as.GetOrNewStateObject(addr)
	obj.SetCode(code)
}

// SetStorage sets storage value.
func (as *AccountState) SetStorage(addr string, key, value []byte) {
	obj := as.GetOrNewStateObject(addr)
	obj.SetStorage(key, value)
}

// CreateAccount creates a new account.
func (as *AccountState) CreateAccount(addr string) *StateObject {
	as.mu.Lock()
	defer as.mu.Unlock()

	if obj, ok := as.objects[addr]; ok {
		return obj
	}

	obj := NewStateObject(addr)
	as.objects[addr] = obj
	return obj
}

// DeleteAccount deletes an account.
func (as *AccountState) DeleteAccount(addr string) {
	as.mu.Lock()
	defer as.mu.Unlock()
	delete(as.objects, addr)
}

// Exists checks if account exists.
func (as *AccountState) Exists(addr string) bool {
	as.mu.RLock()
	defer as.mu.RUnlock()

	_, ok := as.objects[addr]
	return ok
}

// GetAccounts returns all accounts.
func (as *AccountState) GetAccounts() []*StateObject {
	as.mu.RLock()
	defer as.mu.RUnlock()

	accounts := make([]*StateObject, 0, len(as.objects))
	for _, obj := range as.objects {
		accounts = append(accounts, obj)
	}
	return accounts
}

// Commit commits all state changes.
func (as *AccountState) Commit() error {
	as.mu.Lock()
	defer as.mu.Unlock()

	for _, obj := range as.objects {
		if obj.IsDirty() {
			if err := obj.Commit(); err != nil {
				return err
			}
		}
	}
	return nil
}

// Snapshot creates a state snapshot.
func (as *AccountState) Snapshot() ([]byte, error) {
	as.mu.RLock()
	defer as.mu.RUnlock()

	// Encode all accounts
	data, err := rlp.EncodeTo(as.objects)
	if err != nil {
		return nil, err
	}

	return crypto.Keccak256(data), nil
}

// Restore restores state from snapshot.
func (as *AccountState) Restore(data []byte) error {
	as.mu.Lock()
	defer as.mu.Unlock()

	// Decode accounts
	objects := make(map[string]*StateObject)
	if err := rlp.DecodeBytes(data, &objects); err != nil {
		return err
	}

	as.objects = objects
	return nil
}

// GetAccountCount returns the number of accounts.
func (as *AccountState) GetAccountCount() int {
	as.mu.RLock()
	defer as.mu.RUnlock()
	return len(as.objects)
}

// Iterate iterates over all accounts.
func (as *AccountState) Iterate(fn func(*StateObject) bool) {
	as.mu.RLock()
	defer as.mu.RUnlock()

	for _, obj := range as.objects {
		if !fn(obj) {
			break
		}
	}
}

var _ = fmt.Sprintf("") // Use fmt