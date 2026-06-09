// Package websocket provides WebSocket RPC server implementation.
package websocket

import (
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"sync"
	"time"

	"github.com/gorilla/websocket"
)

var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool {
		return true // Allow all origins for development
	},
	ReadBufferSize:  1024,
	WriteBufferSize: 1024,
}

// Server represents WebSocket RPC server.
type Server struct {
	addr       string
	httpServer *http.Server
	conns      map[string]*Client
	mu         sync.RWMutex

	// Subscriptions
	subs     map[string]*Subscription
	subMu    sync.RWMutex

	// Handlers
	handlers map[string]Handler

	// Events
	NewHeads       chan *BlockEvent
	Logs           chan *LogEvent
	PendingTxs     chan *PendingTxEvent
	Synced         chan *SyncedEvent
}

// Handler is WebSocket method handler.
type Handler func(params []interface{}) (interface{}, error)

// Client represents connected WebSocket client.
type Client struct {
	ID       string
	conn    *websocket.Conn
	send    chan []byte
	server  *Server
	closing bool
}

// Subscription represents client subscription.
type Subscription struct {
	ID        string
	ClientID string
	Type     string
	Params   []interface{}
}

// Events
type BlockEvent struct {
	Type    string `json:"type"`
	Block   Block  `json:"block"`
	Removed bool   `json:"removed"`
}

type Block struct {
	Number       string `json:"number"`
	Hash        string `json:"hash"`
	ParentHash  string `json:"parentHash"`
	Timestamp   string `json:"timestamp"`
	Transactions []string `json:"transactions"`
	GasUsed    string `json:"gasUsed"`
	GasLimit   string `json:"gasLimit"`
	Miner      string `json:"miner"`
}

type LogEvent struct {
	Type      string        `json:"type"`
	Address   string        `json:"address"`
	Topics    []string      `json:"topics"`
	Data      string        `json:"data"`
	BlockHash string        `json:"blockHash"`
	BlockNumber string     `json:"blockNumber"`
	TxHash    string        `json:"transactionHash"`
	LogIndex  string        `json:"logIndex"`
	Removed   bool          `json:"removed"`
}

type PendingTxEvent struct {
	Type string `json:"type"`
	Tx   Transaction `json:"transaction"`
}

type Transaction struct {
	Hash   string `json:"hash"`
	From  string `json:"from"`
	To    string `json:"to"`
	Value string `json:"value"`
	Gas   string `json:"gas"`
	Input string `json:"input"`
}

type SyncedEvent struct {
	Type   string `json:"type"`
	Synced bool   `json:"synced"`
}

// NewServer creates new WebSocket server.
func NewServer(addr string) *Server {
	s := &Server{
		addr:   addr,
		conns: make(map[string]*Client),
		subs:  make(map[string]*Subscription),
		handlers: make(map[string]Handler),
		NewHeads: make(chan *BlockEvent, 100),
		Logs: make(chan *LogEvent, 100),
		PendingTxs: make(chan *PendingTxEvent, 100),
		Synced: make(chan *SyncedEvent, 10),
	}

	// Register default handlers
	s.registerHandlers()

	return s
}

// registerHandlers registers default handlers.
func (s *Server) registerHandlers() {
	s.handlers["eth_subscribe"] = s.handleSubscribe
	s.handlers["eth_unsubscribe"] = s.handleUnsubscribe
	s.handlers["eth_subscription"] = s.handleSubscription
}

// Start starts WebSocket server.
func (s *Server) Start() error {
	mux := http.NewServeMux()
	mux.HandleFunc("/", s.handleConnection)

	s.httpServer = &http.Server{
		Addr:         s.addr,
		Handler:      mux,
		ReadTimeout:  30 * time.Second,
		WriteTimeout: 30 * time.Second,
		IdleTimeout:  120 * time.Second,
	}

	go s.eventLoop()
	go s.cleanupLoop()

	log.Printf("WebSocket server starting on %s", s.addr)
	return s.httpServer.ListenAndServe()
}

// Stop stops WebSocket server.
func (s *Server) Stop() error {
	s.mu.RLock()
	defer s.mu.RUnlock()

	for _, client := range s.conns {
		client.Close()
	}

	return s.httpServer.Close()
}

// handleConnection handles WebSocket connections.
func (s *Server) handleConnection(w http.ResponseWriter, r *http.Request) {
	conn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		log.Printf("WebSocket upgrade error: %v", err)
		return
	}

	client := &Client{
		ID:      generateID(),
		conn:   conn,
		send:   make(chan []byte, 256),
		server: s,
	}

	s.mu.Lock()
	s.conns[client.ID] = client
	s.mu.Unlock()

	go client.readPump()
	go client.writePump()

	log.Printf("Client connected: %s", client.ID)
}

// readPump reads messages from client.
func (c *Client) readPump() {
	defer func() {
		c.Close()
		c.server.mu.Lock()
		delete(c.server.conns, c.ID)
		c.server.mu.Unlock()
	}()

	for {
		_, message, err := c.conn.ReadMessage()
		if err != nil {
			if websocket.IsUnexpectedCloseError(err, websocket.CloseGoingAway, websocket.CloseAbnormalClosure) {
				log.Printf("WebSocket error: %v", err)
			}
			break
		}

		c.handleMessage(message)
	}
}

// writePump writes messages to client.
func (c *Client) writePump() {
	ticker := time.NewTicker(30 * time.Second)
	defer func() {
		ticker.Stop()
		c.conn.Close()
	}()

	for {
		select {
		case message, ok := <-c.send:
			c.conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
			if !ok {
				c.conn.WriteMessage(websocket.CloseMessage, []byte{})
				return
			}

			if err := c.conn.WriteMessage(websocket.TextMessage, message); err != nil {
				return
			}

		case <-ticker.C:
			c.conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
			if err := c.conn.WriteMessage(websocket.PingMessage, nil); err != nil {
				return
			}
		}
	}
}

// handleMessage handles incoming message.
func (c *Client) handleMessage(data []byte) {
	var msg JSONRPCRequest
	if err := json.Unmarshal(data, &msg); err != nil {
		c.sendError(msg.ID, -32700, "Parse error")
		return
	}

	handler, ok := c.server.handlers[msg.Method]
	if !ok {
		c.sendError(msg.ID, -32601, "Method not found")
		return
	}

	result, err := handler(msg.Params)
	if err != nil {
		c.sendError(msg.ID, -32000, err.Error())
		return
	}

	c.sendResult(msg.ID, result)
}

// sendResult sends successful result.
func (c *Client) sendResult(id interface{}, result interface{}) {
	resp := JSONRPCResponse{
		JSONRPC: "2.0",
		ID:      id,
		Result:  result,
	}

	data, _ := json.Marshal(resp)
	c.send <- data
}

// sendError sends error response.
func (c *Client) sendError(id interface{}, code int, message string) {
	resp := JSONRPCResponse{
		JSONRPC: "2.0",
		ID: id,
		Error: &JSONRPCError{
			Code:    code,
			Message: message,
		},
	}

	data, _ := json.Marshal(resp)
	c.send <- data
}

// sendNotification sends subscription notification.
func (c *Client) sendNotification(subscriptionType string, data interface{}) {
	notification := SubscriptionNotification{
		JSONRPC: "2.0",
		Method:  "eth_subscription",
		Params: SubscriptionParams{
			Subscription: subscriptionType,
			Result:      data,
		},
	}

	msg, _ := json.Marshal(notification)
	c.send <- msg
}

// Close closes client connection.
func (c *Client) Close() {
	if c.closing {
		return
	}
	c.closing = true
	close(c.send)
}

// handleSubscribe handles eth_subscribe.
func (s *Server) handleSubscribe(params []interface{}) (interface{}, error) {
	if len(params) < 1 {
		return nil, fmt.Errorf("missing subscription type")
	}

	subscriptionType, ok := params[0].(string)
	if !ok {
		return nil, fmt.Errorf("invalid subscription type")
	}

	var subscriptionParams []interface{}
	if len(params) > 1 {
		subscriptionParams, _ = params[1].([]interface{})
	}

	subscription := &Subscription{
		ID:        generateID(),
		Type:      subscriptionType,
		Params:   subscriptionParams,
	}

	s.subMu.Lock()
	s.subs[subscription.ID] = subscription
	s.subMu.Unlock()

	// Start subscription
	go s.startSubscription(subscription)

	return subscription.ID, nil
}

// handleUnsubscribe handles eth_unsubscribe.
func (s *Server) handleUnsubscribe(params []interface{}) (interface{}, error) {
	if len(params) < 1 {
		return nil, fmt.Errorf("missing subscription ID")
	}

	subscriptionID, ok := params[0].(string)
	if !ok {
		return nil, fmt.Errorf("invalid subscription ID")
	}

	s.subMu.Lock()
	defer s.subMu.Unlock()

	if _, ok := s.subs[subscriptionID]; !ok {
		return false, nil
	}

	delete(s.subs, subscriptionID)
	return true, nil
}

// handleSubscription handles subscription method.
func (s *Server) handleSubscription(params []interface{}) (interface{}, error) {
	return nil, nil
}

// startSubscription starts subscription event stream.
func (s *Server) startSubscription(sub *Subscription) {
	switch sub.Type {
	case "newHeads":
		for {
			select {
			case event := <-s.NewHeads:
				s.sendToSubscribers(sub.ID, event)
			}
		}
	case "logs":
		for {
			select {
			case event := <-s.Logs:
				s.sendToSubscribers(sub.ID, event)
			}
		}
	case "pendingTransactions":
		for {
			select {
			case event := <-s.PendingTxs:
				s.sendToSubscribers(sub.ID, event)
			}
		}
	case "synced":
		for {
			select {
			case event := <-s.Synced:
				s.sendToSubscribers(sub.ID, event)
			}
		}
	}
}

// sendToSubscribers sends event to subscribers.
func (s *Server) sendToSubscribers(subscriptionID string, data interface{}) {
	s.subMu.RLock()
	defer s.subMu.RUnlock()

	s.mu.RLock()
	defer s.mu.RUnlock()

	for _, sub := range s.subs {
		if sub.ID == subscriptionID {
			for _, client := range s.conns {
				client.sendNotification(subscriptionID, data)
			}
		}
	}
}

// eventLoop handles blockchain events.
func (s *Server) eventLoop() {
	// This would be connected to blockchain events
	// For now, just drain channels to prevent blocking
	for {
		select {
		case <-s.NewHeads:
		case <-s.Logs:
		case <-s.PendingTxs:
		case <-s.Synced:
		}
	}
}

// cleanupLoop removes stale subscriptions.
func (s *Server) cleanupLoop() {
	ticker := time.NewTicker(5 * time.Minute)
	defer ticker.Stop()

	for range ticker.C {
		s.subMu.Lock()
		for id, sub := range s.subs {
			// Remove stale subscriptions
			_ = id
			_ = sub
			// TODO: Implement expiration check
		}
		s.subMu.Unlock()
	}
}

// generateID generates unique ID.
func generateID() string {
	return fmt.Sprintf("%d-%d", time.Now().UnixNano(), time.Now().Nanosecond())
}

// JSONRPCRequest represents JSON-RPC request.
type JSONRPCRequest struct {
	JSONRPC string          `json:"jsonrpc"`
	Method  string         `json:"method"`
	Params  []interface{}  `json:"params"`
	ID      interface{}    `json:"id"`
}

// JSONRPCResponse represents JSON-RPC response.
type JSONRPCResponse struct {
	JSONRPC string      `json:"jsonrpc"`
	Result  interface{} `json:"result,omitempty"`
	Error   *JSONRPCError `json:"error,omitempty"`
	ID      interface{} `json:"id"`
}

// JSONRPCError represents JSON-RPC error.
type JSONRPCError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

// SubscriptionNotification represents subscription notification.
type SubscriptionNotification struct {
	JSONRPC string           `json:"jsonrpc"`
	Method  string          `json:"method"`
	Params  SubscriptionParams `json:"params"`
}

// SubscriptionParams represents subscription parameters.
type SubscriptionParams struct {
	Subscription string      `json:"subscription"`
	Result       interface{} `json:"result"`
}

var _ = json.Marshal // Use json
