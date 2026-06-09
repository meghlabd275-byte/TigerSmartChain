// Package websocket provides WebSocket RPC server for TigerSmartChain.
package websocket

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/tigersmartchain/tigersmartchain/internal/blockchain/block"
)

// =============================================================================
// WEBSOCKET RPC SERVER
// =============================================================================

// Server represents the WebSocket RPC server.
type Server struct {
	mu sync.RWMutex

	// HTTP server
	httpServer *http.Server

	// Subscriptions
	subscriptions map[string]*Subscription
	channel     *Broadcaster

	// Backend
	backend Backend

	// Configuration
	config *Config
}

// Config holds WebSocket configuration.
type Config struct {
	// Listen address
	ListenAddress string

	// Read/Write timeouts
	ReadTimeout  time.Duration
	WriteTimeout time.Duration

	// Max connections
	MaxConnections int

	// Ping interval
	PingInterval time.Duration
}

// Backend defines the backend interface for subscriptions.
type Backend interface {
	// Subscribe to new blocks
	SubscribeNewHeads() (int, error)

	// Subscribe to pending transactions
	SubscribePendingTransactions() (int, error)

	// Subscribe to logs
	SubscribeLogs(filter *LogFilter) (int, error)

	// Unsubscribe
	Unsubscribe(subID int) error

	// Get new block header
	GetNewBlockHeader() (*block.Header, error)
}

// LogFilter represents a log filter.
type LogFilter struct {
	Address string
	Topics []string
}

// =============================================================================
// SUBSCRIPTION MANAGEMENT
// =============================================================================

// Subscription represents a subscription.
type Subscription struct {
	ID        int
	Type     string
	Filter   *LogFilter
	Params   json.RawMessage
	Channel  chan []byte
	Active   bool
	CreateAt time.Time
}

// Broadcaster handles message broadcasting.
type Broadcaster struct {
	mu sync.RWMutex

	subscribers map[int]chan []byte
	nextID    int
}

// NewBroadcaster creates a new broadcaster.
func NewBroadcaster() *Broadcaster {
	return &Broadcaster{
		subscribers: make(map[int]chan []byte),
		nextID:    1,
	}
}

// Subscribe adds a subscriber.
func (b *Broadcaster) Subscribe() int {
	b.mu.Lock()
	defer b.mu.Unlock()

	ch := make(chan []byte, 100)
	id := b.nextID
	b.nextID++
	b.subscribers[id] = ch

	return id
}

// Unsubscribe removes a subscriber.
func (b *Broadcaster) Unsubscribe(id int) {
	b.mu.Lock()
	defer b.mu.Unlock()

	if ch, ok := b.subscribers[id]; ok {
		close(ch)
		delete(b.subscribers, id)
	}
}

// Broadcast sends a message to all subscribers.
func (b *Broadcaster) Broadcast(msg []byte) {
	b.mu.RLock()
	defer b.mu.RUnlock()

	for _, ch := range b.subscribers {
		select {
		case ch <- msg:
		default:
			// Channel full, skip
		}
	}
}

// Subscribe subscribes to a channel.
func (b *Broadcaster) Subscribe(ch chan []byte) int {
	b.mu.Lock()
	defer b.mu.Unlock()

	id := b.nextID
	b.nextID++
	b.subscribers[id] = ch

	return id
}

// =============================================================================
// SERVER LIFECYCLE
// =============================================================================

// NewServer creates a new WebSocket RPC server.
func NewServer(config *Config) *Server {
	return &Server{
		subscriptions: make(map[string]*Subscription),
		channel:     NewBroadcaster(),
		config:     config,
	}
}

// SetBackend sets the backend.
func (s *Server) SetBackend(backend Backend) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.backend = backend
}

// Start starts the WebSocket server.
func (s *Server) Start() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Create HTTP server
	handler := http.HandlerFunc(s.handleWebSocket)
	s.httpServer = &http.Server{
		Addr:         s.config.ListenAddress,
		Handler:      handler,
		ReadTimeout:  s.config.ReadTimeout,
		WriteTimeout: s.config.WriteTimeout,
	}

	// Start server
	go s.httpServer.ListenAndServe()

	return nil
}

// Stop stops the WebSocket server.
func (s *Server) Stop() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.httpServer != nil {
		return s.httpServer.Close()
	}

	return nil
}

// handleWebSocket handles WebSocket connections.
func (s *Server) handleWebSocket(w http.ResponseWriter, r *http.Request) {
	// Upgrade to WebSocket
	if !strings.Contains(r.Header.Get("Upgrade"), "websocket") {
		http.Error(w, "not a websocket request", http.StatusBadRequest)
		return
	}

	// Check max connections
	if s.channel != nil {
		s.mu.RLock()
		count := len(s.channel.subscribers)
		s.mu.RUnlock()

		if count >= s.config.MaxConnections {
			http.Error(w, "too many connections", http.StatusServiceUnavailable)
			return
		}
	}

	// Create subscription channel
	ch := make(chan []byte, 100)
	subID := s.channel.Subscribe(ch)
	defer s.channel.Unsubscribe(subID)

	// Start ping goroutine
	go s.pingLoop(subID)

	// Handle connection
	for {
		select {
		case msg, ok := <-ch:
			if !ok {
				return
			}

			// Write message
			w.Write(msg)
		case <-r.Context().Done():
			return
		}
	}
}

// pingLoop sends periodic pings.
func (s *Server) pingLoop(subID int) {
	ticker := time.NewTicker(s.config.PingInterval)
	defer ticker.Stop()

	for range ticker.C {
		ping := []byte(`{"jsonrpc":"2.0","method":"eth_ping","params":[],"id":null}`)
		s.channel.Broadcast(ping)
	}
}

// =============================================================================
// SUBSCRIPTION METHODS
// =============================================================================

// SubscribeNewHeads subscribes to new block headers.
func (s *Server) SubscribeNewHeads(params json.RawMessage) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.backend == nil {
		return "", fmt.Errorf("no backend")
	}

	// Subscribe to backend
	subID, err := s.backend.SubscribeNewHeads()
	if err != nil {
		return "", err
	}

	// Create subscription
	sub := &Subscription{
		ID:        subID,
		Type:      "newHeads",
		Params:    params,
		Channel:  make(chan []byte, 100),
		Active:   true,
		CreateAt: time.Now(),
	}

	s.subscriptions[fmt.Sprintf("newHeads:%d", subID)] = sub

	// Start listener
	go s.listenNewHeads(sub)

	return fmt.Sprintf("0x%x", subID), nil
}

// listenNewHeads listens for new block headers.
func (s *Server) listenNewHeads(sub *Subscription) {
	for {
		if !sub.Active {
			return
		}

		header, err := s.backend.GetNewBlockHeader()
		if err != nil {
			time.Sleep(time.Second)
			continue
		}

		if header == nil {
			time.Sleep(time.Second)
			continue
		}

		// Format notification
		notification := map[string]interface{}{
			"jsonrpc": "2.0",
			"method": "eth_subscription",
			"params": map[string]interface{}{
				"subscription": fmt.Sprintf("0x%x", sub.ID),
				"result":     header,
			},
		}

		data, _ := json.Marshal(notification)
		sub.Channel <- data
		s.channel.Broadcast(data)
	}
}

// SubscribePendingTransactions subscribes to pending transactions.
func (s *Server) SubscribePendingTransactions(params json.RawMessage) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.backend == nil {
		return "", fmt.Errorf("no backend")
	}

	// Subscribe to backend
	subID, err := s.backend.SubscribePendingTransactions()
	if err != nil {
		return "", err
	}

	// Create subscription
	sub := &Subscription{
		ID:        subID,
		Type:      "pendingTransactions",
		Params:    params,
		Channel:  make(chan []byte, 100),
		Active:   true,
		CreateAt: time.Now(),
	}

	s.subscriptions[fmt.Sprintf("pendingTransactions:%d", subID)] = sub

	return fmt.Sprintf("0x%x", subID), nil
}

// SubscribeLogs subscribes to logs.
func (s *Server) SubscribeLogs(filter *LogFilter, params json.RawMessage) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.backend == nil {
		return "", fmt.Errorf("no backend")
	}

	// Subscribe to backend
	subID, err := s.backend.SubscribeLogs(filter)
	if err != nil {
		return "", err
	}

	// Create subscription
	sub := &Subscription{
		ID:       subID,
		Type:     "logs",
		Filter:   filter,
		Params:   params,
		Channel: make(chan []byte, 100),
		Active:  true,
		CreateAt: time.Now(),
	}

	s.subscriptions[fmt.Sprintf("logs:%d", subID)] = sub

	return fmt.Sprintf("0x%x", subID), nil
}

// Unsubscribe unsubscribes from a subscription.
func (s *Server) Unsubscribe(subID string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Parse subscription ID
	id := 0
	if _, err := fmt.Sscanf(subID, "0x%x", &id); err != nil {
		return err
	}

	// Find and remove subscription
	for key, sub := range s.subscriptions {
		if sub.ID == id {
			sub.Active = false
			close(sub.Channel)
			delete(s.subscriptions, key)

			if s.backend != nil {
				s.backend.Unsubscribe(id)
			}

			return nil
		}
	}

	return fmt.Errorf("subscription not found")
}

// =============================================================================
// RPC METHODS
// =============================================================================

// handleSubscription handles subscription RPC requests.
func (s *Server) handleSubscription(method string, params json.RawMessage) (json.RawMessage, error) {
	switch method {
	case "eth_subscribe":
		return s.handleEthSubscribe(params)
	case "eth_unsubscribe":
		return s.handleEthUnsubscribe(params)
	default:
		return nil, fmt.Errorf("method not found: %s", method)
	}
}

// handleEthSubscribe handles eth_subscribe.
func (s *Server) handleEthSubscribe(params json.RawMessage) (json.RawMessage, error) {
	var args struct {
		Subscription string          `json:"subscription"`
		Params     json.RawMessage `json:"params"`
	}
	if err := json.Unmarshal(params, &args); err != nil {
		return nil, err
	}

	var subID string
	var err error

	switch args.Subscription {
	case "newHeads":
		subID, err = s.SubscribeNewHeads(args.Params)
	case "pendingTransactions":
		subID, err = s.SubscribePendingTransactions(args.Params)
	case "logs":
		filter := &LogFilter{}
		subID, err = s.SubscribeLogs(filter, args.Params)
	default:
		return nil, fmt.Errorf("unknown subscription type: %s", args.Subscription)
	}

	if err != nil {
		return nil, err
	}

	return []byte(fmt.Sprintf(`"%s"`, subID)), nil
}

// handleEthUnsubscribe handles eth_unsubscribe.
func (s *Server) handleEthUnsubscribe(params json.RawMessage) (json.RawMessage, error) {
	var args struct {
		Subscription string `json:"subscription"`
	}
	if err := json.Unmarshal(params, &args); err != nil {
		return nil, err
	}

	if err := s.Unsubscribe(args.Subscription); err != nil {
		return []byte("false"), nil
	}

	return []byte("true"), nil
}

// =============================================================================
// STATUS
// =============================================================================

// GetSubscriptionCount returns the number of active subscriptions.
func (s *Server) GetSubscriptionCount() int {
	s.mu.RLock()
	defer s.mu.RUnlock()

	return len(s.subscriptions)
}

// GetConnectionCount returns the number of active connections.
func (s *Server) GetConnectionCount() int {
	s.mu.RLock()
	defer s.mu.RUnlock()

	if s.channel == nil {
		return 0
	}

	s.channel.mu.RLock()
	defer s.channel.mu.RUnlock()

	return len(s.channel.subscribers)
}