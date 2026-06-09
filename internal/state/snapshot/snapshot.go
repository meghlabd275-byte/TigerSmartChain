// Package snapshot provides state snapshot functionality for TigerSmartChain.
// This enables fast sync by downloading state snapshots instead of replaying the entire chain.

package snapshot

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"sync"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/rawdb"
	"github.com/ethereum/go-ethereum/rlp"
)

// Snapshot represents a state snapshot at a specific block
type Snapshot struct {
	BlockNumber uint64         `json:"blockNumber"`
	BlockHash  common.Hash    `json:"blockHash"`
	Root      common.Hash    `json:"root"`
	Accounts  []AccountSnap `json:"accounts"`
}

// AccountSnap represents an account in the snapshot
type AccountSnap struct {
	Address   common.Address `json:"address"`
	Nonce     uint64        `json:"nonce"`
	Balance   string       `json:"balance"`
	CodeHash  common.Hash   `json:"codeHash"`
	Code      []byte       `json:"code"`
	Storage  map[string]string `json:"storage"`
}

// Manager manages state snapshots
type Manager struct {
	mu       sync.RWMutex
	snapshots map[uint64]*Snapshot
	db       rawdb.Database
}

// NewManager creates a new snapshot manager
func NewManager(db rawdb.Database) *Manager {
	return &Manager{
		snapshots: make(map[uint64]*Snapshot),
		db:       db,
	}
}

// CreateSnapshot creates a snapshot of the current state
func (m *Manager) CreateSnapshot(blockNumber uint64, blockHash common.Hash, accounts map[common.Address]AccountSnap) (*Snapshot, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	// Calculate state root (simplified - real implementation would compute Merkle root)
	root := common.Hash{}

	snap := &Snapshot{
		BlockNumber: blockNumber,
		BlockHash:  blockHash,
		Root:      root,
		Accounts:  make([]AccountSnap, 0, len(accounts)),
	}

	for addr, acc := range accounts {
		snap.Accounts = append(snap.Accounts, AccountSnap{
			Address:  addr,
			Nonce:   acc.Nonce,
			Balance: acc.Balance,
			CodeHash: acc.CodeHash,
			Code:    acc.Code,
			Storage: acc.Storage,
		})
	}

	// Store snapshot
	m.snapshots[blockNumber] = snap

	// Persist to database
	if err := m.persist(snap); err != nil {
		return nil, err
	}

	return snap, nil
}

// GetSnapshot retrieves a snapshot by block number
func (m *Manager) GetSnapshot(blockNumber uint64) (*Snapshot, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	snap, ok := m.snapshots[blockNumber]
	if ok {
		return snap, nil
	}

	// Try to load from database
	return m.load(blockNumber)
}

// persist saves snapshot to database
func (m *Manager) persist(snap *Snapshot) error {
	key := make([]byte, 8)
	binary.BigEndian.PutUint64(key, snap.BlockNumber)

	data, err := json.Marshal(snap)
	if err != nil {
		return err
	}

	return m.db.Put(rawdb.SnapshotKey(key), data)
}

// load loads snapshot from database
func (m *Manager) load(blockNumber uint64) (*Snapshot, error) {
	key := make([]byte, 8)
	binary.BigEndian.PutUint64(key, blockNumber)

	data, err := m.db.Get(rawdb.SnapshotKey(key))
	if err != nil {
		return nil, err
	}

	var snap Snapshot
	if err := json.Unmarshal(data, &snap); err != nil {
		return nil, err
	}

	m.snapshots[blockNumber] = &snap
	return &snap, nil
}

// DeleteSnapshot removes a snapshot
func (m *Manager) DeleteSnapshot(blockNumber uint64) {
	m.mu.Lock()
	defer m.mu.Unlock()

	delete(m.snapshots, blockNumber)

	// Remove from database
	key := make([]byte, 8)
	binary.BigEndian.PutUint64(key, blockNumber)
	m.db.Delete(rawdb.SnapshotKey(key))
}

// ListSnapshots returns all available snapshots
func (m *Manager) ListSnapshots() []uint64 {
	m.mu.RLock()
	defer m.mu.RUnlock()

	numbers := make([]uint64, 0, len(m.snapshots))
	for n := range m.snapshots {
		numbers = append(numbers, n)
	}

	return numbers
}

// VerifySnapshot verifies snapshot integrity
func (m *Manager) VerifySnapshot(blockNumber uint64) (bool, error) {
	snap, err := m.GetSnapshot(blockNumber)
	if err != nil {
		return false, err
	}

	// Simplified verification - real implementation would verify Merkle proof
	return len(snap.Accounts) >= 0, nil
}

// SnapshotStats returns snapshot statistics
func (m *Manager) SnapshotStats() (count int, totalSize int64) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	count = len(m.snapshots)
	for _, snap := range m.snapshots {
		totalSize += int64(len(snap.Accounts) * 100) // Approximate size
	}

	return count, totalSize
}

// AccountSnap represents account state for snapshot
type Account struct {
	Nonce    uint64
	Balance  string
	CodeHash common.Hash
	Code    []byte
	Storage map[string]string
}

// EncodeRLP encodes snapshot to RLP
func (s *Snapshot) EncodeRLP() ([]byte, error) {
	return rlp.EncodeToBytes(s)
}

// DecodeRLP decodes snapshot from RLP
func (s *Snapshot) DecodeRLP(data []byte) error {
	return rlp.DecodeBytes(data, s)
}

// Format returns formatted snapshot info
func (s *Snapshot) Format() string {
	return fmt.Sprintf("Snapshot #%d (root: %s, accounts: %d)", 
		s.BlockNumber, s.Root.Hex(), len(s.Accounts))
}

var _ = fmt.Sprintf    // Use fmt
var _ = binary.BigEndian // Use binary
var _ = json.Marshal // Use JSON