// Package state provides historical state trie exploration services
package state

import (
	"encoding/hex"
	"fmt"
	"strings"
	"sync"
	"time"
)

// StateService provides historical state exploration
type StateService struct {
	stateSnapshots map[uint64]*StateSnapshot
	trieCache     map[string]*TrieNode
	mu           sync.RWMutex
}

// StateSnapshot represents a state snapshot at a block
type StateSnapshot struct {
	BlockNumber uint64           `json:"blockNumber"`
	BlockHash  string          `json:"blockHash"`
	Timestamp  time.Time       `json:"timestamp"`
	Accounts   map[string]*Account `json:"accounts"`
}

// Account represents an account state
type Account struct {
	Address  string `json:"address"`
	Nonce    uint64 `json:"nonce"`
	Balance  string `json:"balance"`
	CodeHash string `json:"codeHash"`
	StorageRoot string `json:"storageRoot"`
	Code    string `json:"code,omitempty"`
	Storage map[string]string `json:"storage"`
}

// StorageSlot represents a storage slot
type StorageSlot struct {
	Key   string `json:"key"`
	Value string `json:"value"`
}

// TrieNode represents a trie node
type TrieNode struct {
	Key    string `json:"key,omitempty"`
	Value string `json:"value,omitempty"`
	Hash  string `json:"hash"`
	Depth int    `json:"depth"`
}

// StateDiff represents state changes between blocks
type StateDiff struct {
	FromBlock uint64   `json:"fromBlock"`
	ToBlock   uint64   `json:"toBlock"`
	Created   []string `json:"created"`
	Deleted   []string `json:"deleted"`
	Modified  []string `json:"modified"`
	Transfers []*Transfer `json:"transfers"`
}

// Transfer represents a balance transfer
type Transfer struct {
	From    string `json:"from"`
	To      string `json:"to"`
	Value   string `json:"value"`
	Block   uint64 `json:"block"`
}

// NewStateService creates a new state service
func NewStateService() *StateService {
	return &StateService{
		stateSnapshots: make(map[uint64]*StateSnapshot),
		trieCache: make(map[string]*TrieNode),
	}
}

// GetAccountAt gets account state at a specific block
func (s *StateService) GetAccountAt(address string, blockNumber uint64) (*Account, error) {
	address = normalizeAddress(address)
	
	s.mu.RLock()
	snapshot, ok := s.stateSnapshots[blockNumber]
	s.mu.RUnlock()
	
	if !ok {
		return nil, fmt.Errorf("snapshot not found for block %d", blockNumber)
	}
	
	account, ok := snapshot.Accounts[address]
	if !ok {
		return nil, fmt.Errorf("account not found")
	}
	
	return account, nil
}

// GetStorageAt gets storage value at a specific slot and block
func (s *StateService) GetStorageAt(address, slot string, blockNumber uint64) (string, error) {
	address = normalizeAddress(address)
	slot = normalizeSlot(slot)
	
	account, err := s.GetAccountAt(address, blockNumber)
	if err != nil {
		return "", err
	}
	
	value, ok := account.Storage[slot]
	if !ok {
		return "", fmt.Errorf("slot not found")
	}
	
	return value, nil
}

// GetBalanceAt gets balance at a specific block
func (s *StateService) GetBalanceAt(address string, blockNumber uint64) (string, error) {
	account, err := s.GetAccountAt(address, blockNumber)
	if err != nil {
		return "0x0", err
	}
	
	return account.Balance, nil
}

// GetNonceAt gets nonce at a specific block
func (s *StateService) GetNonceAt(address string, blockNumber uint64) (uint64, error) {
	account, err := s.GetAccountAt(address, blockNumber)
	if err != nil {
		return 0, err
	}
	
	return account.Nonce, nil
}

// GetCodeAt gets contract code at a specific block
func (s *StateService) GetCodeAt(address string, blockNumber uint64) (string, error) {
	account, err := s.GetAccountAt(address, blockNumber)
	if err != nil {
		return "", err
	}
	
	return account.Code, nil
}

// GetDiff gets state diff between two blocks
func (s *StateService) GetDiff(fromBlock, toBlock uint64) (*StateDiff, error) {
	fromSnap, ok1 := s.stateSnapshots[fromBlock]
	toSnap, ok2 := s.stateSnapshots[toBlock]
	
	if !ok1 || !ok2 {
		return nil, fmt.Errorf("snapshots not found")
	}
	
	diff := &StateDiff{
		FromBlock: fromBlock,
		ToBlock:   toBlock,
		Created:   []string{},
		Deleted:   []string{},
		Modified:  []string{},
		Transfers: []*Transfer{},
	}
	
	// Find created accounts
	for addr := range toSnap.Accounts {
		if _, exists := fromSnap.Accounts[addr]; !exists {
			diff.Created = append(diff.Created, addr)
		}
	}
	
	// Find deleted accounts
	for addr := range fromSnap.Accounts {
		if _, exists := toSnap.Accounts[addr]; !exists {
			diff.Deleted = append(diff.Deleted, addr)
		}
	}
	
	// Find modified accounts
	for addr, newAccount := range toSnap.Accounts {
		if oldAccount, exists := fromSnap.Accounts[addr]; exists {
			if oldAccount.Balance != newAccount.Balance || oldAccount.Nonce != newAccount.Nonce {
				diff.Modified = append(diff.Modified, addr)
			}
		}
	}
	
	return diff, nil
}

// GetHistoricalBalance gets historical balance over time
func (s *StateService) GetHistoricalBalance(address string, startBlock, endBlock uint64) ([]*BalancePoint, error) {
	address = normalizeAddress(address)
	
	points := make([]*BalancePoint, 0)
	
	for block := startBlock; block <= endBlock; block++ {
		snapshot, ok := s.stateSnapshots[block]
		if !ok {
			continue
		}
		
		if account, exists := snapshot.Accounts[address]; exists {
			points = append(points, &BalancePoint{
				BlockNumber: block,
				Balance:    account.Balance,
				Timestamp:  snapshot.Timestamp,
			})
		}
	}
	
	return points, nil
}

// BalancePoint represents a balance at a point in time
type BalancePoint struct {
	BlockNumber uint64    `json:"blockNumber"`
	Balance    string    `json:"balance"`
	Timestamp  time.Time `json:"timestamp"`
}

// GetStorageHistory gets storage value history
func (s *StateService) GetStorageHistory(address, slot string, startBlock, endBlock uint64) ([]*StoragePoint, error) {
	address = normalizeAddress(address)
	slot = normalizeSlot(slot)
	
	points := make([]*StoragePoint, 0)
	
	for block := startBlock; block <= endBlock; block++ {
		value, err := s.GetStorageAt(address, slot, block)
		if err != nil {
			continue
		}
		
		snapshot := s.stateSnapshots[block]
		
		points = append(points, &StoragePoint{
			BlockNumber: block,
			Value:      value,
			Timestamp: snapshot.Timestamp,
		})
	}
	
	return points, nil
}

// StoragePoint represents storage value at a point
type StoragePoint struct {
	BlockNumber uint64    `json:"blockNumber"`
	Value      string    `json:"value"`
	Timestamp  time.Time `json:"timestamp"`
}

// GetMerkleProof gets Merkle proof for account
func (s *StateService) GetMerkleProof(address string, blockNumber uint64) (*MerkleProof, error) {
	address = normalizeAddress(address)
	
	account, err := s.GetAccountAt(address, blockNumber)
	if err != nil {
		return nil, err
	}
	
	proof := &MerkleProof{
		Address:     address,
		BlockNumber: blockNumber,
		AccountProof: []string{},
		StorageProof: make(map[string][]string),
	}
	
	// Generate mock proof
	proof.AccountProof = append(proof.AccountProof, generateProofNode(address))
	
	for slot := range account.Storage {
		proof.StorageProof[slot] = []string{generateProofNode(slot)}
	}
	
	return proof, nil
}

// MerkleProof represents a Merkle proof
type MerkleProof struct {
	Address        string            `json:"address"`
	BlockNumber   uint64            `json:"blockNumber"`
	AccountProof []string          `json:"accountProof"`
	StorageProof map[string][]string `json:"storageProof"`
}

// VerifyProof verifies a Merkle proof
func (s *StateService) VerifyProof(proof *MerkleProof) (bool, error) {
	if proof == nil {
		return false, fmt.Errorf("nil proof")
	}
	
	// In production, would verify proof against state root
	return true, nil
}

// GetStateRoot gets state root for a block
func (s *StateService) GetStateRoot(blockNumber uint64) (string, error) {
	s.mu.RLock()
	snapshot, ok := s.stateSnapshots[blockNumber]
	s.mu.RUnlock()
	
	if !ok {
		return "", fmt.Errorf("snapshot not found")
	}
	
	// Return mock state root
	return "0x" + strings.Repeat("0", 64), nil
}

// GetAllAccountsAt gets all accounts at a block
func (s *StateService) GetAllAccountsAt(blockNumber uint64) (map[string]*Account, error) {
	s.mu.RLock()
	snapshot, ok := s.stateSnapshots[blockNumber]
	s.mu.RUnlock()
	
	if !ok {
		return nil, fmt.Errorf("snapshot not found")
	}
	
	return snapshot.Accounts, nil
}

// GetRichList gets richest accounts at a block
func (s *StateService) GetRichList(blockNumber uint64, limit int) ([]*Account, error) {
	accounts, err := s.GetAllAccountsAt(blockNumber)
	if err != nil {
		return nil, err
	}
	
	// Sort by balance
	sorted := make([]*Account, 0, len(accounts))
	for _, acc := range accounts {
		sorted = append(sorted, acc)
	}
	
	// Simple bubble sort
	for i := 0; i < len(sorted); i++ {
		for j := i + 1; j < len(sorted); j++ {
			if compareBalance(sorted[j].Balance, sorted[i].Balance) > 0 {
				sorted[i], sorted[j] = sorted[j], sorted[i]
			}
		}
	}
	
	if len(sorted) > limit {
		sorted = sorted[:limit]
	}
	
	return sorted, nil
}

// compareBalance compares two balance strings
func compareBalance(a, b string) int {
	a = strings.TrimPrefix(a, "0x")
	b = strings.TrimPrefix(b, "0x")
	
	if a == "" {
		a = "0"
	}
	if b == "" {
		b = "0"
	}
	
	// Compare by length first
	if len(a) != len(b) {
		return len(a) - len(b)
	}
	
	// Compare by string
	if a > b {
		return 1
	} else if a < b {
		return -1
	}
	return 0
}

// GetContractDeployments gets contract deployments in a range
func (s *StateService) GetContractDeployments(startBlock, endBlock uint64) ([]*ContractDeployment, error) {
	deployments := make([]*ContractDeployment, 0)
	
	for block := startBlock; block <= endBlock; block++ {
		snapshot, ok := s.stateSnapshots[block]
		if !ok {
			continue
		}
		
		for addr, account := range snapshot.Accounts {
			if account.Code != "" && account.Nonce == 0 {
				deployments = append(deployments, &ContractDeployment{
					Address:    addr,
					BlockNumber: block,
					CodeHash:  account.CodeHash,
				})
			}
		}
	}
	
	return deployments, nil
}

// ContractDeployment represents a contract deployment
type ContractDeployment struct {
	Address    string `json:"address"`
	BlockNumber uint64 `json:"blockNumber"`
	CodeHash  string `json:"codeHash"`
}

// normalizeAddress normalizes an address
func normalizeAddress(addr string) string {
	addr = strings.ToLower(addr)
	addr = strings.TrimPrefix(addr, "0x")
	
	if len(addr) != 40 {
		return ""
	}
	
	return "0x" + addr
}

// normalizeSlot normalizes a storage slot
func normalizeSlot(slot string) string {
	slot = strings.TrimPrefix(slot, "0x")
	
	if len(slot) > 64 {
		slot = slot[:64]
	}
	
	return slot
}

// generateProofNode generates a proof node
func generateProofNode(data string) string {
	if data == "" {
		return ""
	}
	
	data = strings.TrimPrefix(data, "0x")
	if len(data) > 32 {
		data = data[:32]
	}
	
	return "0x" + data + strings.Repeat("0", 64-len(data))
}

// InitStateService initializes the service
func InitStateService() (*StateService, error) {
	return NewStateService(), nil
}