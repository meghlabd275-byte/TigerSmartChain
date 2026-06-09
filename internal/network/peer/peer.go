// Package peer provides peer management for P2P networking.
package peer

import (
	"context"
	"encoding/binary"
	"fmt"
	"sync"
	"time"

	"github.com/tigersmartchain/tigersmartchain/internal/crypto"
)

// Status represents peer connection status.
type Status int

const (
	StatusHandshake Status = iota
	StatusConnected
	StatusDisconnected
)

// Peer represents a connected peer.
type Peer struct {
	ID        string
	IP       string
	Port     uint16
	NetworkID uint64
	ChainID  uint64
	Head     uint64
	Difficulty uint64
	TD       uint64
	Status   Status
	LastPing uint64
	Latency  time.Duration
	Conn     interface{}
	Version  string
	Caps     []string
}

// NewPeer creates a new peer.
func NewPeer(id, ip string, port uint16) *Peer {
	return &Peer{
		ID:        id,
		IP:       ip,
		Port:     port,
		Status:   StatusHandshake,
		LastPing: uint64(time.Now().Unix()),
		Version:  "1.0.0",
		Caps:     []string{"eth", "lesh"},
	}
}

// Ping calculates ping time.
func (p *Peer) Ping() time.Duration {
	return p.Latency
}

// PeerManager manages connected peers.
type PeerManager struct {
	mu sync.RWMutex

	// Connected peers by ID
	peers map[string]*Peer
	// Peer by IP
	peersByIP map[string]*Peer
	// Discovery service
	discovery interface{}
	// Max peers
	maxPeers int
	// Trusted peers
	trusted map[string]bool
}

// NewPeerManager creates a new peer manager.
func NewPeerManager() *PeerManager {
	return &PeerManager{
		peers:     make(map[string]*Peer),
		peersByIP: make(map[string]*Peer),
		trusted:  make(map[string]bool),
		maxPeers: 50,
	}
}

// AddPeer adds a peer.
func (pm *PeerManager) AddPeer(peer *Peer) error {
	pm.mu.Lock()
	defer pm.mu.Unlock()

	// Check max peers
	if len(pm.peers) >= pm.maxPeers {
		return fmt.Errorf("max peers reached")
	}

	// Check if already connected
	if _, exists := pm.peers[peer.ID]; exists {
		return fmt.Errorf("peer already connected")
	}

	// Add peer
	pm.peers[peer.ID] = peer
	pm.peersByIP[peer.IP] = peer

	return nil
}

// RemovePeer removes a peer.
func (pm *PeerManager) RemovePeer(peerID string) error {
	pm.mu.Lock()
	defer pm.mu.Unlock()

	peer, ok := pm.peers[peerID]
	if !ok {
		return fmt.Errorf("peer not found")
	}

	// Remove from maps
	delete(pm.peers, peerID)
	delete(pm.peersByIP, peer.IP)

	return nil
}

// GetPeer returns a peer by ID.
func (pm *PeerManager) GetPeer(peerID string) (*Peer, bool) {
	pm.mu.RLock()
	defer pm.mu.RUnlock()

	peer, ok := pm.peers[peerID]
	return peer, ok
}

// GetPeers returns all peers.
func (pm *PeerManager) GetPeers() []*Peer {
	pm.mu.RLock()
	defer pm.mu.RUnlock()

	peers := make([]*Peer, 0, len(pm.peers))
	for _, peer := range pm.peers {
		peers = append(peers, peer)
	}
	return peers
}

// GetPeerCount returns the number of peers.
func (pm *PeerManager) GetPeerCount() int {
	pm.mu.RLock()
	defer pm.mu.RUnlock()
	return len(pm.peers)
}

// GetPeerByIP returns peer by IP.
func (pm *PeerManager) GetPeerByIP(ip string) (*Peer, bool) {
	pm.mu.RLock()
	defer pm.mu.RUnlock()

	peer, ok := pm.peersByIP[ip]
	return peer, ok
}

// SetTrusted marks a peer as trusted.
func (pm *PeerManager) SetTrusted(peerID string, trusted bool) {
	pm.mu.Lock()
	defer pm.mu.Unlock()
	pm.trusted[peerID] = trusted
}

// IsTrusted checks if peer is trusted.
func (pm *PeerManager) IsTrusted(peerID string) bool {
	pm.mu.RLock()
	defer pm.mu.RUnlock()
	return pm.trusted[peerID]
}

// UpdatePeer updates peer info.
func (pm *PeerManager) UpdatePeer(peerID string, head uint64, td uint64) error {
	pm.mu.Lock()
	defer pm.mu.Unlock()

	peer, ok := pm.peers[peerID]
	if !ok {
		return fmt.Errorf("peer not found")
	}

	peer.Head = head
	peer.TD = td

	return nil
}

// SetMaxPeers sets maximum peers.
func (pm *PeerManager) SetMaxPeers(max int) {
	pm.mu.Lock()
	defer pm.mu.Unlock()
	pm.maxPeers = max
}

// GetBestPeer returns the peer with highest TD.
func (pm *PeerManager) GetBestPeer() *Peer {
	pm.mu.RLock()
	defer pm.mu.RUnlock()

	if len(pm.peers) == 0 {
		return nil
	}

	best := (*Peer)(nil)
	for _, peer := range pm.peers {
		if best == nil || peer.TD > best.TD {
			best = peer
		}
	}

	return best
}

// GetPeersByTD returns peers sorted by TD.
func (pm *PeerManager) GetPeersByTD() []*Peer {
	pm.mu.RLock()
	defer pm.mu.RUnlock()

	peers := make([]*Peer, 0, len(pm.peers))
	for _, peer := range pm.peers {
		peers = append(peers, peer)
	}

	// Sort by TD descending
	for i := 0; i < len(peers)-1; i++ {
		for j := i + 1; j < len(peers); j++ {
			if peers[j].TD > peers[i].TD {
				peers[i], peers[j] = peers[j], peers[i]
			}
		}
	}

	return peers
}

// Handshake performs peer handshake.
func (pm *PeerManager) Handshake(peer *Peer, networkID, chainID uint64) error {
	// Check network ID
	if peer.NetworkID != networkID {
		return fmt.Errorf("network ID mismatch: %d != %d", peer.NetworkID, networkID)
	}

	// Check chain ID
	if peer.ChainID != chainID {
		return fmt.Errorf("chain ID mismatch: %d != %d", peer.ChainID, chainID)
	}

	// Mark as connected
	peer.Status = StatusConnected

	return nil
}

// SendMessage sends a message to a peer.
func (pm *PeerManager) SendMessage(peerID string, msgType uint64, data []byte) error {
	pm.mu.RLock()
	defer pm.mu.RUnlock()

	peer, ok := pm.peers[peerID]
	if !ok {
		return fmt.Errorf("peer not found")
	}

	// In a real implementation, encode and send message
	return nil
}

// Broadcast broadcasts a message to all peers.
func (pm *PeerManager) Broadcast(msgType uint64, data []byte) {
	pm.mu.RLock()
	peers := make([]*Peer, 0, len(pm.peers))
	for _, p := range pm.peers {
		peers = append(peers, p)
	}
	pm.mu.RUnlock()

	for _, peer := range peers {
		go pm.SendMessage(peer.ID, msgType, data)
	}
}

// DisconnectAll disconnects all peers.
func (pm *PeerManager) DisconnectAll() {
	pm.mu.Lock()
	defer pm.mu.Unlock()

	for _, peer := range pm.peers {
		peer.Status = StatusDisconnected
	}
}

// MessageHandler handles incoming messages.
type MessageHandler func(peerID string, msgType uint64, data []byte) error

// SetMessageHandler sets the message handler.
func (pm *PeerManager) SetMessageHandler(handler MessageHandler) {
	// In a real implementation, this would be used
}

// PeerConnection represents a peer connection.
type PeerConnection struct {
	Peer   *Peer
	Reader *MessageReader
	Writer *MessageWriter
}

// MessageReader reads messages from peer.
type MessageReader struct {
	conn interface{}
	buf []byte
}

// Read reads a message.
func (mr *MessageReader) Read() (uint64, []byte, error) {
	// In a real implementation, read from connection
	return 0, nil, nil
}

// MessageWriter writes messages to peer.
type MessageWriter struct {
	conn interface{}
}

// Write writes a message.
func (mw *MessageWriter) Write(msgType uint64, data []byte) error {
	// In a real implementation, encode and write
	return nil
}

// EncodeMessage encodes a message.
func EncodeMessage(msgType uint64, data []byte) []byte {
	msg := make([]byte, 8)
	binary.BigEndian.PutUint64(msg[:8], msgType)
	msg = append(msg, data...)
	return msg
}

// DecodeMessage decodes a message.
func DecodeMessage(data []byte) (uint64, []byte, error) {
	if len(data) < 8 {
		return 0, nil, fmt.Errorf("message too short")
	}

	msgType := binary.BigEndian.Uint64(data[:8])
	msgData := data[8:]

	return msgType, msgData, nil
}

// GeneratePeerID generates a peer ID.
func GeneratePeerID() string {
	return fmt.Sprintf("0x%x", crypto.Keccak256([]byte(time.Now().String())))
}

// GetVersion returns peer version.
func (p *Peer) GetVersion() string {
	return p.Version
}

// HasCapability checks if peer has capability.
func (p *Peer) HasCapability(cap string) bool {
	for _, c := range p.Caps {
		if c == cap {
			return true
		}
	}
	return false
}

// SetLatency sets peer latency.
func (p *Peer) SetLatency(latency time.Duration) {
	p.Latency = latency
}

var _ = context.Background() // Use context