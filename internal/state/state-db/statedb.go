// Package statedb provides persistent state database.
package statedb

import (
	"encoding/binary"
	"fmt"
	"sync"

	"github.com/tigersmartchain/tigersmartchain/internal/state/account"
	"github.com/tigersmartchain/tigersmartchain/internal/state/trie"
	"github.com/tigersmartchain/tigersmartchain/internal/storage"
)

// StateDB represents the persistent state database.
type StateDB struct {
	mu sync.RWMutex

	db        storage.Store
	accountDB *account.AccountState
	trie      *trie.Trie

	// Snapshots
	snapshots     [][]byte
	snapshotIndex int

	// Preimages
	preimages map[string][]byte
}

// NewStateDB creates a new state database.
func NewStateDB(store storage.Store) (*StateDB, error) {
	db := &StateDB{
		db:          store,
		accountDB:   account.NewAccountState(),
		trie:       trie.NewTrie(),
		preimages:  make(map[string][]byte),
		snapshots:   make([][]byte, 0),
		snapshotIndex: -1,
	}

	// Load existing state
	if err := db.loadState(); err != nil {
		return nil, err
	}

	return db, nil
}

// loadState loads state from storage.
func (sdb *StateDB) loadState() error {
	// Try to load root from storage
	root, err := sdb.db.Get([]byte("state_root"))
	if err != nil {
		// No existing state
		return nil
	}

	if len(root) > 0 {
		if err := sdb.trie.Unmarshal(root); err != nil {
			return err
		}
	}

	return nil
}

// GetAccount returns account info.
func (sdb *StateDB) GetAccount(addr string) (*account.Account, bool) {
	return sdb.accountDB.GetAccount(addr)
}

// GetBalance returns account balance.
func (sdb *StateDB) GetBalance(addr string) interface{} {
	return sdb.accountDB.GetBalance(addr)
}

// GetNonce returns account nonce.
func (sdb *StateDB) GetNonce(addr string) uint64 {
	return sdb.accountDB.GetNonce(addr)
}

// GetCode returns contract code.
func (sdb *StateDB) GetCode(addr string) []byte {
	return sdb.accountDB.GetCode(addr)
}

// GetCodeHash returns code hash.
func (sdb *StateDB) GetCodeHash(addr string) []byte {
	return sdb.accountDB.GetCodeHash(addr)
}

// GetStorage returns storage value.
func (sdb *StateDB) GetStorage(addr string, key []byte) []byte {
	return sdb.accountDB.GetStorage(addr, key)
}

// AddBalance adds balance to an account.
func (sdb *StateDB) AddBalance(addr string, amount interface{}) error {
	amountInt := toBigInt(amount)
	return sdb.accountDB.AddBalance(addr, amountInt)
}

// SubBalance subtracts balance from an account.
func (sdb *StateDB) SubBalance(addr string, amount interface{}) error {
	amountInt := toBigInt(amount)
	return sdb.accountDB.SubBalance(addr, amountInt)
}

// Transfer transfers balance between accounts.
func (sdb *StateDB) Transfer(from, to string, amount interface{}) error {
	amountInt := toBigInt(amount)
	return sdb.accountDB.Transfer(from, to, amountInt)
}

// SetNonce sets account nonce.
func (sdb *StateDB) SetNonce(addr string, nonce uint64) {
	sdb.accountDB.SetNonce(addr, nonce)
}

// IncrementNonce increments account nonce.
func (sdb *StateDB) IncrementNonce(addr string) {
	sdb.accountDB.IncrementNonce(addr)
}

// SetCode sets contract code.
func (sdb *StateDB) SetCode(addr string, code []byte) {
	sdb.accountDB.SetCode(addr, code)
}

// SetStorage sets storage value.
func (sdb *StateDB) SetStorage(addr string, key, value []byte) {
	sdb.accountDB.SetStorage(addr, key, value)
}

// CreateAccount creates a new account.
func (sdb *StateDB) CreateAccount(addr string) {
	sdb.accountDB.CreateAccount(addr)
}

// DeleteAccount deletes an account.
func (sdb *StateDB) DeleteAccount(addr string) {
	sdb.accountDB.DeleteAccount(addr)
}

// Exists checks if account exists.
func (sdb *StateDB) Exists(addr string) bool {
	return sdb.accountDB.Exists(addr)
}

// GetAccountCount returns account count.
func (sdb *StateDB) GetAccountCount() int {
	return sdb.accountDB.GetAccountCount()
}

// Commit commits state changes.
func (sdb *StateDB) Commit() error {
	sdb.mu.Lock()
	defer sdb.mu.Unlock()

	// Commit account state
	if err := sdb.accountDB.Commit(); err != nil {
		return err
	}

	// Build state trie
	accounts := sdb.accountDB.GetAccounts()
	for _, acc := range accounts {
		addr := acc.Address()
		
		// Encode account data
		data := encodeAccount(acc)
		
		// Update trie
		sdb.trie.Update([]byte(addr), data)
	}

	// Commit trie
	root, err := sdb.trie.Commit()
	if err != nil {
		return err
	}

	// Save root to storage
	if err := sdb.db.Put([]byte("state_root"), root); err != nil {
		return err
	}

	return nil
}

// GetRoot returns the current state root.
func (sdb *StateDB) GetRoot() []byte {
	return sdb.trie.Root()
}

// Snapshot creates a state snapshot.
func (sdb *StateDB) Snapshot() int {
	sdb.mu.Lock()
	defer sdb.mu.Unlock()

	snapshot := sdb.accountDB.Snapshot()
	sdb.snapshots = append(sdb.snapshots, snapshot)
	sdb.snapshotIndex++

	return sdb.snapshotIndex
}

// RevertTo reverts to a snapshot.
func (sdb *StateDB) RevertTo(revision int) bool {
	sdb.mu.Lock()
	defer sdb.mu.Unlock()

	if revision < 0 || revision > sdb.snapshotIndex {
		return false
	}

	snapshot := sdb.snapshots[revision]
	if err := sdb.accountDB.Restore(snapshot); err != nil {
		return false
	}

	// Clear newer snapshots
	sdb.snapshots = sdb.snapshots[:revision+1]
	sdb.snapshotIndex = revision

	return true
}

// AddPreimage adds a preimage.
func (sdb *StateDB) AddPreimage(hash []byte, preimage []byte) {
	sdb.mu.Lock()
	defer sdb.mu.Unlock()
	sdb.preimages[string(hash)] = preimage
}

// GetPreimage returns a preimage.
func (sdb *StateDB) GetPreimage(hash []byte) []byte {
	sdb.mu.RLock()
	defer sdb.mu.RUnlock()
	return sdb.preimages[string(hash)]
}

// Close closes the state database.
func (sdb *StateDB) Close() error {
	sdb.mu.Lock()
	defer sdb.mu.Unlock()

	// Commit any pending changes
	if err := sdb.Commit(); err != nil {
		return err
	}

	// Close storage
	if sdb.db != nil {
		return sdb.db.Close()
	}

	return nil
}

// Helper functions

func encodeAccount(acc *account.StateObject) []byte {
	// Simple encoding: nonce|balance|codeHash
	data := make([]byte, 0)
	
	// Nonce (8 bytes)
	nonceBuf := make([]byte, 8)
	binary.BigEndian.PutUint64(nonceBuf, acc.Nonce())
	data = append(data, nonceBuf...)
	
	// Balance (variable)
	balanceBytes := acc.Balance().Bytes()
	balanceLenBuf := make([]byte, 2)
	binary.BigEndian.PutUint16(balanceLenBuf, uint16(len(balanceBytes)))
	data = append(data, balanceLenBuf...)
	data = append(data, balanceBytes...)
	
	// Code hash
	data = append(data, acc.CodeHash()...)
	
	return data
}

func decodeAccount(data []byte) (uint64, []byte, []byte, error) {
	if len(data) < 10 {
		return 0, nil, nil, fmt.Errorf("invalid account data")
	}

	offset := 0
	
	// Nonce
	nonce := binary.BigEndian.Uint64(data[offset : offset+8])
	offset += 8
	
	// Balance
	balanceLen := binary.BigEndian.Uint16(data[offset : offset+2])
	offset += 2
	balance := data[offset : offset+int(balanceLen)]
	offset += int(balanceLen)
	
	// Code hash
	codeHash := data[offset:]
	
	return nonce, balance, codeHash, nil
}

func toBigInt(v interface{}) interface{} {
	switch v := v.(type) {
	case int64:
		return v
	case uint64:
		return v
	default:
		return v
	}
}

var _ = fmt.Sprintf("") // Use fmt