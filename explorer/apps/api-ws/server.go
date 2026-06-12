// Package websocket provides WebSocket API for TigerScan real-time updates.
package websocket

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"sync"
	"time"

	"github.com/gorilla/websocket"
	"github.com/tigersmartchain/tigersmartchain/explorer/databases/postgres"
)

// Upgrader configuration
var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool {
		return true // Allow all origins in development
	},
	ReadBufferSize:  1024,
	WriteBufferSize: 1024,
}

// =============================================================================
// MESSAGE TYPES
// =============================================================================

// Message represents a WebSocket message
type Message struct {
	Type    string          `json:"type"`
	Payload json.RawMessage `json:"payload"`
}

// NewBlockMessage represents a new block notification
type NewBlockMessage struct {
	Number        int64  `json:"number"`
	Hash         string `json:"hash"`
	ParentHash   string `json:"parentHash"`
	Timestamp    int64  `json:"timestamp"`
	Transactions int    `json:"transactions"`
	GasUsed      int64  `json:"gasUsed"`
	GasLimit     int64  `json:"gasLimit"`
	Miner        string `json:"miner"`
}

// NewTxMessage represents a new transaction notification
type NewTxMessage struct {
	Hash        string `json:"hash"`
	From        string `json:"from"`
	To          string `json:"to"`
	Value       string `json:"value"`
	GasPrice    int64  `json:"gasPrice"`
	Nonce       int64  `json:"nonce"`
	BlockNumber int64  `json:"blockNumber,omitempty"`
	Timestamp   int64  `json:"timestamp"`
}

// PendingTxMessage represents a pending transaction
type PendingTxMessage struct {
	Hash     string `json:"hash"`
	From     string `json:"from"`
	To       string `json:"to"`
	Value    string `json:"value"`
	GasPrice int64  `json:"gasPrice"`
	Nonce    int64  `json:"nonce"`
	Timestamp int64 `json:"timestamp"`
}

// PriceUpdateMessage represents token price update
type PriceUpdateMessage struct {
	TokenAddress string  `json:"tokenAddress"`
	Symbol       string  `json:"symbol"`
	PriceUSD     float64 `json:"priceUSD"`
	Change24h    float64 `json:"change24h"`
	Timestamp    int64   `json:"timestamp"`
}

// SubscriptionRequest represents a subscription request
type SubscriptionRequest struct {
	Action      string   `json:"action"` // "subscribe" or "unsubscribe"
	Channels    []string `json:"channels"`
}

// =============================================================================
// CLIENT
// =============================================================================

// Client represents a WebSocket client
type Client struct {
	ID        string
	Conn     *websocket.Conn
	Send     chan []byte
	Server   *Server
	Channels map[string]bool
	mu       sync.RWMutex
}

// NewClient creates a new client
func NewClient(id string, conn *websocket.Conn, server *Server) *Client {
	return &Client{
		ID:        id,
		Conn:      conn,
		Send:      make(chan []byte, 256),
		Server:    server,
		Channels: make(map[string]bool),
	}
}

// ReadPump pumps messages from the client
func (c *Client) ReadPump() {
	defer func() {
		c.Server.Unregister <- c
		c.Conn.Close()
	}()

	c.Conn.SetReadLimit(512)
	c.Conn.SetReadDeadline(time.Now().Add(60 * time.Second))
	c.Conn.SetPongHandler(func(string) error {
		c.Conn.SetReadDeadline(time.Now().Add(60 * time.Second))
		return nil
	})

	for {
		_, message, err := c.Conn.ReadMessage()
		if err != nil {
			if websocket.IsUnexpectedCloseError(err, websocket.CloseGoingAway, websocket.CloseAbnormalClosure) {
				log.Printf("WebSocket error: %v", err)
			}
			break
		}

		c.handleMessage(message)
	}
}

// WritePump pumps messages to the client
func (c *Client) WritePump() {
	ticker := time.NewTicker(54 * time.Second)
	defer func() {
		ticker.Stop()
		c.Conn.Close()
	}()

	for {
		select {
		case message, ok := <-c.Send:
			c.Conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
			if !ok {
				c.Conn.WriteMessage(websocket.CloseMessage, []byte{})
				return
			}

			w, err := c.Conn.NextWriter(websocket.TextMessage)
			if err != nil {
				return
			}
			w.Write(message)

			// Add queued messages to the current WebSocket message
			n := len(c.Send)
			for i := 0; i < n; i++ {
				w.Write([]byte{'\n'})
				w.Write(<-c.Send)
			}

			if err := w.Close(); err != nil {
				return
			}

		case <-ticker.C:
			c.Conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
			if err := c.Conn.WriteMessage(websocket.PingMessage, nil); err != nil {
				return
			}
		}
	}
}

// handleMessage handles incoming messages
func (c *Client) handleMessage(message []byte) {
	var msg Message
	if err := json.Unmarshal(message, &msg); err != nil {
		c.SendError("Invalid message format")
		return
	}

	switch msg.Type {
	case "subscribe":
		c.handleSubscribe(msg.Payload)
	case "unsubscribe":
		c.handleUnsubscribe(msg.Payload)
	case "ping":
		c.SendJSON("pong", map[string]interface{}{"timestamp": time.Now().Unix()})
	default:
		c.SendError("Unknown message type")
	}
}

// handleSubscribe handles subscription requests
func (c *Client) handleSubscribe(payload json.RawMessage) {
	var req SubscriptionRequest
	if err := json.Unmarshal(payload, &req); err != nil {
		c.SendError("Invalid subscription request")
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	for _, channel := range req.Channels {
		switch channel {
		case "new-blocks", "new-transactions", "pending-transactions", "price-updates":
			c.Channels[channel] = true
		default:
			c.SendError(fmt.Sprintf("Unknown channel: %s", channel))
			continue
		}
	}

	c.SendJSON("subscribed", map[string]interface{}{
		"channels": req.Channels,
	})
}

// handleUnsubscribe handles unsubscription requests
func (c *Client) handleUnsubscribe(payload json.RawMessage) {
	var req SubscriptionRequest
	if err := json.Unmarshal(payload, &req); err != nil {
		c.SendError("Invalid unsubscription request")
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	for _, channel := range req.Channels {
		delete(c.Channels, channel)
	}

	c.SendJSON("unsubscribed", map[string]interface{}{
		"channels": req.Channels,
	})
}

// SendJSON sends a JSON message
func (c *Client) SendJSON(type_ string, payload interface{}) {
	msg := Message{
		Type:    type_,
		Payload: mustMarshal(payload),
	}
	data, _ := json.Marshal(msg)
	c.Send <- data
}

// SendError sends an error message
func (c *Client) SendError(message string) {
	c.SendJSON("error", map[string]interface{}{"message": message})
}

// =============================================================================
// SERVER
// =============================================================================

// Server represents the WebSocket server
type Server struct {
	Clients    map[*Client]bool
	Broadcast  chan []byte
	Register   chan *Client
	Unregister chan *Client
	mu         sync.RWMutex

	// Subscriptions
	blockSubs    map[*Client]bool
	txSubs       map[*Client]bool
	pendingSubs  map[*Client]bool
	priceSubs   map[*Client]bool

	// Database
	db *postgres.DB

	// Context
	ctx context.Context
}

// NewServer creates a new WebSocket server
func NewServer(ctx context.Context, db *postgres.DB) *Server {
	return &Server{
		Clients:    make(map[*Client]bool),
		Broadcast:  make([]byte, 1024),
		Register:   make(chan *Client),
		Unregister: make(chan *Client),
		blockSubs:  make(map[*Client]bool),
		txSubs:    make(map[*Client]bool),
		pendingSubs: make(map[*Client]bool),
		priceSubs:  make(map[*Client]bool),
		db:         db,
		ctx:        ctx,
	}
}

// Run runs the WebSocket server
func (s *Server) Run() {
	for {
		select {
		case client := <-s.Register:
			s.mu.Lock()
			s.Clients[client] = true
			s.mu.Unlock()
			log.Printf("Client %s connected, total: %d", client.ID, len(s.Clients))

		case client := <-s.Unregister:
			s.mu.Lock()
			if _, ok := s.Clients[client]; ok {
				delete(s.Clients, client)
				close(client.Send)
				delete(s.blockSubs, client)
				delete(s.txSubs, client)
				delete(s.pendingSubs, client)
				delete(s.priceSubs, client)
			}
			s.mu.Unlock()
			log.Printf("Client %s disconnected, total: %d", client.ID, len(s.Clients))

		case message := <-s.Broadcast:
			s.mu.RLock()
			for client := range s.Clients {
				select {
				case client.Send <- message:
				default:
					close(client.Send)
					delete(s.Clients, client)
				}
			}
			s.mu.RUnlock()
		}
	}
}

// BroadcastBlock broadcasts a new block
func (s *Server) BroadcastBlock(block *NewBlockMessage) {
	msg := Message{
		Type:    "new-block",
		Payload: mustMarshal(block),
	}
	data, _ := json.Marshal(msg)

	s.mu.RLock()
	defer s.mu.RUnlock()

	for client := range s.blockSubs {
		select {
		case client.Send <- data:
		default:
		}
	}
}

// BroadcastTransaction broadcasts a new transaction
func (s *Server) BroadcastTransaction(tx *NewTxMessage) {
	msg := Message{
		Type:    "new-transaction",
		Payload: mustMarshal(tx),
	}
	data, _ := json.Marshal(msg)

	s.mu.RLock()
	defer s.mu.RUnlock()

	for client := range s.txSubs {
		select {
		case client.Send <- data:
		default:
		}
	}
}

// BroadcastPendingTransaction broadcasts a pending transaction
func (s *Server) BroadcastPendingTransaction(tx *PendingTxMessage) {
	msg := Message{
		Type:    "pending-transaction",
		Payload: mustMarshal(tx),
	}
	data, _ := json.Marshal(msg)

	s.mu.RLock()
	defer s.mu.RUnlock()

	for client := range s.pendingSubs {
		select {
		case client.Send <- data:
		default:
		}
	}
}

// BroadcastPriceUpdate broadcasts a price update
func (s *Server) BroadcastPriceUpdate(price *PriceUpdateMessage) {
	msg := Message{
		Type:    "price-update",
		Payload: mustMarshal(price),
	}
	data, _ := json.Marshal(msg)

	s.mu.RLock()
	defer s.mu.RUnlock()

	for client := range s.priceSubs {
		select {
		case client.Send <- data:
		default:
		}
	}
}

// =============================================================================
// HTTP HANDLER
// =============================================================================

// Handler returns the HTTP handler for WebSocket
func (s *Server) Handler() http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		conn, err := upgrader.Upgrade(w, r, nil)
		if err != nil {
			log.Printf("WebSocket upgrade error: %v", err)
			return
		}

		clientID := fmt.Sprintf("client-%d", time.Now().UnixNano())
		client := NewClient(clientID, conn, s)

		s.Register <- client

		go client.WritePump()
		go client.ReadPump()
	}
}

// =============================================================================
// HELPERS
// =============================================================================

func mustMarshal(v interface{}) json.RawMessage {
	data, _ := json.Marshal(v)
	return data
}

// =============================================================================
// HUB (Main WebSocket Hub)
// =============================================================================

// Hub manages all WebSocket connections
type Hub struct {
	Server    *Server
	ctx       context.Context
	cancel    context.CancelFunc
	clients   map[string]*Client
	clientMut sync.RWMutex
}

// NewHub creates a new hub
func NewHub(ctx context.Context, db *postgres.DB) *Hub {
	ctx, cancel := context.WithCancel(ctx)
	return &Hub{
		Server:  NewServer(ctx, db),
		ctx:     ctx,
		cancel:  cancel,
		clients: make(map[string]*Client),
	}
}

// Start starts the hub
func (h *Hub) Start() {
	go h.Server.Run()
}

// Stop stops the hub
func (h *Hub) Stop() {
	h.cancel()
}

// GetClient gets a client by ID
func (h *Hub) GetClient(id string) (*Client, bool) {
	h.clientMut.RLock()
	defer h.clientMut.RUnlock()
	client, ok := h.clients[id]
	return client, ok
}

// BroadcastToAll broadcasts to all connected clients
func (h *Hub) BroadcastToAll(msgType string, payload interface{}) {
	msg := Message{
		Type:    msgType,
		Payload: mustMarshal(payload),
	}
	data, _ := json.Marshal(msg)
	h.Server.Broadcast <- data
}

// GetStats returns connection statistics
func (h *Hub) GetStats() map[string]interface{} {
	h.Server.mu.RLock()
	defer h.Server.mu.RUnlock()

	return map[string]interface{}{
		"totalConnections":  len(h.Server.Clients),
		"blockSubscribers":  len(h.Server.blockSubs),
		"txSubscribers":     len(h.Server.txSubs),
		"pendingSubscribers": len(h.Server.pendingSubs),
		"priceSubscribers":  len(h.Server.priceSubs),
	}
}

// =============================================================================
// FILTERS
// =============================================================================

// Filter represents a log filter
type Filter struct {
	FromBlock *int64   `json:"fromBlock,omitempty"`
	ToBlock   *int64   `json:"toBlock,omitempty"`
	Address   string    `json:"address,omitempty"`
	Topics    []string `json:"topics,omitempty"`
	BlockHash string    `json:"blockHash,omitempty"`
}

// NewFilterRequest represents a new filter request
type NewFilterRequest struct {
	Type  string  `json:"type"` // "block", "pendingTransaction", "logs"
	Filter Filter `json:"filter"`
}

// FilterSubscription represents a filter subscription
type FilterSubscription struct {
	ID        string
	Type     string
	Filter   Filter
	Client   *Client
	Channel  string
	LastPoll time.Time
}

// CreateFilter creates a new filter
func (s *Server) CreateFilter(req *NewFilterRequest, client *Client) string {
	id := fmt.Sprintf("filter-%d", time.Now().UnixNano())
	return id
}

// GetFilterChanges gets filter changes
func (s *Server) GetFilterChanges(filterID string) interface{} {
	// Implementation would query the database for new logs/transactions
	return []interface{}{}
}

// UninstallFilter uninstalls a filter
func (s *Server) UninstallFilter(filterID string) bool {
	return true
}
