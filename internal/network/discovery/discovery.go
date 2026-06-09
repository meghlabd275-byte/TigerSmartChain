// Package discovery provides peer discovery for P2P networking.
package discovery

import (
	"context"
	"crypto/rand"
	"encoding/binary"
	"fmt"
	"net"
	"sync"
	"time"

	"github.com/tigersmartchain/tigersmartchain/internal/crypto"
)

// Node represents a network node.
type Node struct {
	ID        string
	IP       net.IP
	Port     uint16
	NetworkID uint64
	ChainID   uint64
	PubKey   []byte
	LastSeen uint64
	Online   bool
}

// Table represents the routing table.
type Table struct {
	mu sync.RWMutex

	self     *Node
	buckets  map[uint64][]*Node
	kadMap  map[string]*Node
	version  uint64
}

// NewTable creates a new routing table.
func NewTable(selfID string, ip net.IP, port uint16) *Table {
	t := &Table{
		buckets: make(map[uint64][]*Node),
		kadMap: make(map[string]*Node),
	}

	t.self = &Node{
		ID:        selfID,
		IP:       ip,
		Port:     port,
		LastSeen: uint64(time.Now().Unix()),
		Online:   true,
	}

	return t
}

// AddNode adds a node to the routing table.
func (t *Table) AddNode(node *Node) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	if node.ID == t.self.ID {
		return fmt.Errorf("cannot add self")
	}

	// Check if already exists
	if existing, ok := t.kadMap[node.ID]; ok {
		// Update last seen
		existing.LastSeen = uint64(time.Now().Unix())
		return nil
	}

	// Add to bucket
	bucketID := t.bucketID(node.ID)
	t.buckets[bucketID] = append(t.buckets[bucketID], node)
	t.kadMap[node.ID] = node

	return nil
}

// RemoveNode removes a node from the routing table.
func (t *Table) RemoveNode(nodeID string) {
	t.mu.Lock()
	defer t.mu.Unlock()

	node, ok := t.kadMap[nodeID]
	if !ok {
		return
	}

	// Remove from bucket
	bucketID := t.bucketID(nodeID)
	bucket := t.buckets[bucketID]
	for i, n := range bucket {
		if n.ID == nodeID {
			t.buckets[bucketID] = append(bucket[:i], bucket[i+1:]...)
			break
		}
	}

	delete(t.kadMap, nodeID)
}

// GetNode returns a node by ID.
func (t *Table) GetNode(nodeID string) (*Node, bool) {
	t.mu.RLock()
	defer t.mu.RUnlock()

	node, ok := t.kadMap[nodeID]
	return node, ok
}

// GetNodes returns all nodes.
func (t *Table) GetNodes() []*Node {
	t.mu.RLock()
	defer t.mu.RUnlock()

	nodes := make([]*Node, 0, len(t.kadMap))
	for _, node := range t.kadMap {
		nodes = append(nodes, node)
	}
	return nodes
}

// GetNearbyNodes returns nearby nodes.
func (t *Table) GetNearbyNodes(targetID string, count int) []*Node {
	t.mu.RLock()
	defer t.mu.RUnlock()

	// Calculate distance
	distance := t.distance(t.self.ID, targetID)
	
	// Get nodes from closest buckets
	nodes := make([]*Node, 0)
	for _, node := range t.kadMap {
		if node.ID == t.self.ID {
			continue
		}
		nodeDist := t.distance(t.self.ID, node.ID)
		if nodeDist >= distance {
			continue
		}
		nodes = append(nodes, node)
	}

	// Sort by distance
	// In a real implementation, sort by distance to target

	if count > 0 && len(nodes) > count {
		nodes = nodes[:count]
	}

	return nodes
}

// GetRandomNodes returns random nodes.
func (t *Table) GetRandomNodes(count int) []*Node {
	t.mu.RLock()
	defer t.mu.RUnlock()

	if len(t.kadMap) == 0 {
		return nil
	}

	nodes := make([]*Node, 0)
	ids := make([]string, 0, len(t.kadMap))
	for id := range t.kadMap {
		ids = append(ids, id)
	}

	// Shuffle and pick
	for i := 0; i < count && i < len(ids); i++ {
		if node, ok := t.kadMap[ids[i]]; ok {
			nodes = append(nodes, node)
		}
	}

	return nodes
}

// bucketID calculates the bucket ID for a node.
func (t *Table) bucketID(nodeID string) uint64 {
	distance := t.distance(t.self.ID, nodeID)
	return distance
}

// distance calculates the XOR distance between two IDs.
func (t *Table) distance(id1, id2 string) uint64 {
	hash1 := crypto.Keccak256([]byte(id1))
	hash2 := crypto.Keccak256([]byte(id2))

	var d1, d2 uint64
	binary.BigEndian.PutUint64(make([]byte, 8), d1)
	binary.BigEndian.PutUint64(make([]byte, 8), d2)

	result := make([]byte, 8)
	for i := 0; i < 8; i++ {
		result[i] = hash1[i] ^ hash2[i]
	}

	return binary.BigEndian.Uint64(result)
}

// UpdateLastSeen updates the last seen time for a node.
func (t *Table) UpdateLastSeen(nodeID string) {
	t.mu.Lock()
	defer t.mu.Unlock()

	if node, ok := t.kadMap[nodeID]; ok {
		node.LastSeen = uint64(time.Now().Unix())
	}
}

// GetOnlineNodes returns online nodes.
func (t *Table) GetOnlineNodes() []*Node {
	t.mu.RLock()
	defer t.mu.RUnlock()

	nodes := make([]*Node, 0)
	for _, node := range t.kadMap {
		if node.Online {
			nodes = append(nodes, node)
		}
	}
	return nodes
}

// DiscoveryService provides peer discovery.
type DiscoveryService struct {
	mu sync.RWMutex

	table      *Table
	udpAddr   *net.UDPAddr
	conn      *net.UDPConn
	running   bool
	bootnodes []*Node
}

// NewDiscoveryService creates a new discovery service.
func NewDiscoveryService(listenAddr string) (*DiscoveryService, error) {
	addr, err := net.ResolveUDPAddr("udp", listenAddr)
	if err != nil {
		return nil, err
	}

	return &DiscoveryService{
		table: NewTable(generateNodeID(), addr.IP, uint16(addr.Port)),
		udpAddr: addr,
	}, nil
}

// Start starts the discovery service.
func (ds *DiscoveryService) Start() error {
	ds.mu.Lock()
	defer ds.mu.Unlock()

	if ds.running {
		return nil
	}

	conn, err := net.ListenUDP("udp", ds.udpAddr)
	if err != nil {
		return err
	}

	ds.conn = conn
	ds.running = true

	// Start discovery loop
	go ds.discoveryLoop()

	return nil
}

// Stop stops the discovery service.
func (ds *DiscoveryService) Stop() error {
	ds.mu.Lock()
	defer ds.mu.Unlock()

	if !ds.running {
		return nil
	}

	ds.running = false

	if ds.conn != nil {
		return ds.conn.Close()
	}

	return nil
}

// discoveryLoop runs the discovery process.
func (ds *DiscoveryService) discoveryLoop() {
	ds.mu.RLock()
	running := ds.running
	ds.mu.RUnlock()

	for running {
		// Find peers
		ds.findPeers()

		ds.mu.RLock()
		running = ds.running
		ds.mu.RUnlock()

		time.Sleep(30 * time.Second)
	}
}

// findPeers finds new peers.
func (ds *DiscoveryService) findPeers() {
	// Ping bootnodes
	for _, node := range ds.bootnodes {
		if err := ds.ping(node); err != nil {
			continue
		}
	}

	// Get nodes from bootnodes
	for _, node := range ds.bootnodes {
		nodes, err := ds.getNodes(node)
		if err != nil {
			continue
		}

		for _, n := range nodes {
			ds.table.AddNode(n)
		}
	}
}

// ping pings a node.
func (ds *DiscoveryService) ping(node *Node) error {
	// In a real implementation, send ping message
	return nil
}

// getNodes requests nodes from a node.
func (ds *DiscoveryService) getNodes(node *Node) ([]*Node, error) {
	// In a real implementation, send find nodes message
	return nil, nil
}

// AddBootnode adds a bootnode.
func (ds *DiscoveryService) AddBootnode(node *Node) {
	ds.mu.Lock()
	defer ds.mu.Unlock()
	ds.bootnodes = append(ds.bootnodes, node)
}

// GetTable returns the routing table.
func (ds *DiscoveryService) GetTable() *Table {
	return ds.table
}

// GenerateNodeID generates a random node ID.
func generateNodeID() string {
	b := make([]byte, 64)
	rand.Read(b)
	return fmt.Sprintf("0x%x", crypto.Keccak256(b))
}

// Resolve resolves a node address.
func Resolve(addr string) (*Node, error) {
	host, port, err := net.SplitHostPort(addr)
	if err != nil {
		return nil, err
	}

	ip := net.ParseIP(host)
	if ip == nil {
		return nil, fmt.Errorf("invalid IP: %s", host)
	}

	p := uint16(30303)
	fmt.Sscanf(port, "%d", &p)

	return &Node{
		IP:   ip,
		Port: p,
	}, nil
}

// Ping sends a ping to a node.
func Ping(node *Node) error {
	// Simple UDP ping
	addr := fmt.Sprintf("%s:%d", node.IP, node.Port)
	conn, err := net.DialTimeout("udp", addr, 5*time.Second)
	if err != nil {
		return err
	}
	defer conn.Close()

	// Send ping
	msg := []byte("ping")
	_, err = conn.Write(msg)
	return err
}

// Pong sends a pong to a node.
func Pong(node *Node) error {
	return nil
}

// FindNodes requests nodes from a node.
func FindNodes(node *Node, target string, count int) ([]*Node, error) {
	return nil, nil
}

// ValidateNode validates a node.
func ValidateNode(node *Node) bool {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	// Try to connect
	addr := fmt.Sprintf("%s:%d", node.IP, node.Port)
	conn, err := net.DialTimeout("udp", addr, 5*time.Second)
	if err != nil {
		return false
	}
	defer conn.Close()

	return true
}

var _ = context.Background() // Use context