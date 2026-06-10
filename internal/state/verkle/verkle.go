// Package verkle implements Verkle Trees for state management.
package verkle

import (
	"crypto/sha256"
	"fmt"
)

// VerkleTree represents a Verkle tree.
type VerkleTree struct {
	root  *Node
	depth int
}

// Node represents a Verkle tree node.
type Node struct {
	Hash      [32]byte
	Children  []*Node
	Stem      []byte
}

// NewVerkleTree creates a new Verkle tree.
func NewVerkleTree(depth int) *VerkleTree {
	if depth == 0 {
		depth = 256
	}
	return &VerkleTree{
		root:  newNode(nil),
		depth: depth,
	}
}

func newNode(stem []byte) *Node {
	return &Node{
		Children: make([]*Node, 128),
		Stem:    stem,
	}
}

// Insert inserts a key-value pair.
func (vt *VerkleTree) Insert(key, value []byte) error {
	stem := key
	if len(stem) < 31 {
		stem = make([]byte, 31)
		copy(stem, key)
	}
	vt.insertNode(vt.root, stem, value, 0)
	return nil
}

func (vt *VerkleTree) insertNode(node *Node, stem []byte, value []byte, depth int) error {
	if depth >= len(stem) {
		node.Hash = sha256Hash(value)
		return nil
	}
	index := int(stem[depth])
	if index < 0 || index >= 128 {
		return fmt.Errorf("invalid index")
	}
	if node.Children[index] == nil {
		childStem := append([]byte{}, stem[:depth+1]...)
		node.Children[index] = newNode(childStem)
	}
	return vt.insertNode(node.Children[index], stem, value, depth+1)
}

// Get retrieves a value.
func (vt *VerkleTree) Get(key []byte) ([]byte, error) {
	return vt.getNode(vt.root, key, 0)
}

func (vt *VerkleTree) getNode(node *Node, stem []byte, depth int) ([]byte, error) {
	if depth >= len(stem) {
		return node.Hash[:], nil
	}
	index := int(stem[depth])
	if node.Children[index] == nil {
		return nil, nil
	}
	return vt.getNode(node.Children[index], stem, depth+1)
}

// RootHash returns the root hash.
func (vt *VerkleTree) RootHash() [32]byte {
	return vt.root.Hash
}

// StateExpiry manages state expiration.
type StateExpiry struct {
	epochs map[uint64]*Epoch
	currentEpoch uint64
}

// Epoch represents a state epoch.
type Epoch struct {
	Number     uint64
	ExpiryTime uint64
	roots    map[uint64][32]byte
}

// NewStateExpiry creates a new state expiry manager.
func NewStateExpiry() *StateExpiry {
	return &StateExpiry{
		epochs: make(map[uint64]*Epoch),
	}
}

// NewEpoch creates a new epoch.
func (se *StateExpiry) NewEpoch(number uint64) error {
	if _, exists := se.epochs[number]; exists {
		return fmt.Errorf("epoch already exists")
	}
	se.epochs[number] = &Epoch{
		Number:     number,
		ExpiryTime: number * 225,
		roots:     make(map[uint64][32]byte),
	}
	return nil
}

// AddRoot adds a state root to epoch.
func (se *StateExpiry) AddRoot(epoch, blockNum uint64, root [32]byte) error {
	ep, exists := se.epochs[epoch]
	if !exists {
		return fmt.Errorf("epoch not found")
	}
	ep.roots[blockNum] = root
	return nil
}

// GetRoot retrieves a state root.
func (se *StateExpiry) GetRoot(epoch, blockNum uint64) ([32]byte, bool) {
	ep, exists := se.epochs[epoch]
	if !exists {
		return [32]byte{}, false
	}
	root, ok := ep.roots[blockNum]
	return root, ok
}

// Prune removes expired state.
func (se *StateExpiry) Prune(currentEpoch uint64) {
	for epoch := range se.epochs {
		if epoch < currentEpoch {
			delete(se.epochs, epoch)
		}
	}
}

func sha256Hash(data []byte) [32]byte {
	var hash [32]byte
	h := sha256.Sum256(data)
	copy(hash[:], h[:32])
	return hash
}

var _ = fmt.Errorf
var _ = sha256.Sum256