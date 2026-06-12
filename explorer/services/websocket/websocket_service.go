// Package websocket provides WebSocket services for real-time data
package websocket

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync"
	"time"
)

// WebSocketService provides WebSocket connections
type WebSocketService struct {
	clients    map[string]*Client
	channels   map[string]*Channel
	mu        sync.RWMutex
	hub       *Hub
}

// Hub represents WebSocket hub
type Hub struct {
	clients    map[*Client]bool
	broadcast  chan *Message
	register   chan *Client
	unregister chan *Client
	mu        sync.RWMutex
}

// Client represents a WebSocket client
type Client struct {
	ID      string
	Conn    interface{}
	Send    chan []byte
	Filters map[string]interface{}
	Hub     *Hub
}

// Message represents a WebSocket message
type Message struct {
	Type      string          `json:"type"`
	Channel  string          `json:"channel"`
	Payload  json.RawMessage `json:"payload"`
	ClientID string          `json:"clientId,omitempty"`
	Timestamp time.Time     `json:"timestamp"`
}

// Channel represents a subscription channel
type Channel struct {
	Name      string
	Clients  map[string]*Client
	mu      sync.RWMutex
}

// SubscriptionRequest represents subscription request
type SubscriptionRequest struct {
	Channel string                 `json:"channel"`
	Filters map[string]interface{} `json:"filters"`
}

// NewWebSocketService creates new WebSocket service
func NewWebSocketService() *WebSocketService {
	hub := &Hub{
		clients:    make(map[*Client]bool),
		broadcast:  make(chan []byte, 256),
		register:   make(chan *Client),
		unregister: make(chan *Client),
	}
	
	return &WebSocketService{
		clients:  make(map[string]*Client),
		channels: initChannels(),
		hub:     hub,
	}
}

func initChannels() map[string]*Channel {
	return map[string]*Channel{
		"new_blocks": {Name: "new_blocks", Clients: make(map[string]*Client)},
		"new_transactions": {Name: "new_transactions", Clients: make(map[string]*Client)},
		"pending_transactions": {Name: "pending_transactions", Clients: make(map[string]*Client)},
		"new_logs": {Name: "new_logs", Clients: make(map[string]*Client)},
		"new_tokens": {Name: "new_tokens", Clients: make(map[string]*Client)},
		"new_nfts": {Name: "new_nfts", Clients: make(map[string]*Client)},
		"gas_updates": {Name: "gas_updates", Clients: make(map[string]*Client)},
		"price_updates": {Name: "price_updates", Clients: make(map[string]*Client)},
		"address_updates": {Name: "address_updates", Clients: make(map[string]*Client)},
	}
}

// StartHub starts the message hub
func (s *WebSocketService) StartHub(ctx context.Context) {
	go func() {
		for {
			select {
			case <-ctx.Done():
				return
			case client := <-s.hub.register:
				s.hub.mu.Lock()
				s.hub.clients[client] = true
				s.hub.mu.Unlock()
			case client := <-s.hub.unregister:
				s.hub.mu.Lock()
				if _, ok := s.hub.clients[client]; ok {
					delete(s.hub.clients, client)
					close(client.Send)
				}
				s.hub.mu.Unlock()
			case message := <-s.hub.broadcast:
				s.hub.mu.RLock()
				for client := range s.hub.clients {
					select {
					case client.Send <- message:
					default:
						close(client.Send)
						delete(s.hub.clients, client)
					}
				}
				s.hub.mu.RUnlock()
			}
		}
	}()
}

// Subscribe subscribes client to channel
func (s *WebSocketService) Subscribe(clientID, channel string, filters map[string]interface{}) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	client, ok := s.clients[clientID]
	if !ok {
		return fmt.Errorf("client not found")
	}
	
	channelObj, ok := s.channels[channel]
	if !ok {
		return fmt.Errorf("channel not found")
	}
	
	channelObj.mu.Lock()
	channelObj.Clients[clientID] = client
	channelObj.mu.Unlock()
	
	client.Filters = filters
	
	return nil
}

// Unsubscribe unsubscribes client from channel
func (s *WebSocketService) Unsubscribe(clientID, channel string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	channelObj, ok := s.channels[channel]
	if !ok {
		return fmt.Errorf("channel not found")
	}
	
	channelObj.mu.Lock()
	delete(channelObj.Clients, clientID)
	channelObj.mu.Unlock()
	
	return nil
}

// Publish publishes message to channel
func (s *WebSocketService) Publish(channel string, payload interface{}) error {
	data, err := json.Marshal(payload)
	if err != nil {
		return err
	}
	
	msg := Message{
		Type:      "message",
		Channel:  channel,
		Payload:  data,
		Timestamp: time.Now(),
	}
	
	msgBytes, err := json.Marshal(msg)
	if err != nil {
		return err
	}
	
	// Send to channel subscribers
	s.mu.RLock()
	channelObj, ok := s.channels[channel]
	s.mu.RUnlock()
	
	if ok {
		channelObj.mu.RLock()
		for _, client := range channelObj.Clients {
			select {
			case client.Send <- msgBytes:
			default:
			}
		}
		channelObj.mu.RUnlock()
	}
	
	// Also broadcast
	s.hub.broadcast <- msgBytes
	
	return nil
}

// HandleConnection handles new WebSocket connection
func (s *WebSocketService) HandleConnection(conn interface{}, clientID string) (*Client, error) {
	client := &Client{
		ID:   clientID,
		Conn: conn,
		Send: make(chan []byte, 256),
		Hub:  s.hub,
		Filters: make(map[string]interface{}),
	}
	
	s.mu.Lock()
	s.clients[clientID] = client
	s.mu.Unlock()
	
	s.hub.register <- client
	
	return client, nil
}

// HandleMessage handles incoming message
func (s *WebSocketService) HandleMessage(clientID string, data []byte) error {
	var msg Message
	if err := json.Unmarshal(data, &msg); err != nil {
		return err
	}
	
	switch msg.Type {
	case "subscribe":
		var req SubscriptionRequest
		if err := json.Unmarshal(msg.Payload, &req); err != nil {
			return err
		}
		return s.Subscribe(clientID, req.Channel, req.Filters)
	case "unsubscribe":
		var req SubscriptionRequest
		if err := json.Unmarshal(msg.Payload, &req); err != nil {
			return err
		}
		return s.Unsubscribe(clientID, req.Channel)
	case "ping":
		return s.SendToClient(clientID, []byte(`{"type":"pong"}`))
	}
	
	return nil
}

// SendToClient sends message to specific client
func (s *WebSocketService) SendToClient(clientID string, data []byte) error {
	s.mu.RLock()
	client, ok := s.clients[clientID]
	s.mu.RUnlock()
	
	if !ok {
		return fmt.Errorf("client not found")
	}
	
	select {
	case client.Send <- data:
		return nil
	default:
		return fmt.Errorf("client buffer full")
	}
}

// BroadcastBlock broadcasts new block
func (s *WebSocketService) BroadcastBlock(block *BlockEvent) error {
	return s.Publish("new_blocks", block)
}

// BroadcastTransaction broadcasts new transaction
func (s *WebSocketService) BroadcastTransaction(tx *TransactionEvent) error {
	return s.Publish("new_transactions", tx)
}

// BroadcastPendingTx broadcasts pending transaction
func (s *WebSocketService) BroadcastPendingTx(tx *TransactionEvent) error {
	return s.Publish("pending_transactions", tx)
}

// BroadcastLog broadcasts new log
func (s *WebSocketService) BroadcastLog(log *LogEvent) error {
	return s.Publish("new_logs", log)
}

// BroadcastToken broadcasts new token
func (s *WebSocketService) BroadcastToken(token *TokenEvent) error {
	return s.Publish("new_tokens", token)
}

// BroadcastGasUpdate broadcasts gas update
func (s *WebSocketService) BroadcastGasUpdate(gas *GasUpdate) error {
	return s.Publish("gas_updates", gas)
}

// BroadcastPriceUpdate broadcasts price update
func (s *WebSocketService) BroadcastPriceUpdate(price *PriceUpdate) error {
	return s.Publish("price_updates", price)
}

// DisconnectClient disconnects a client
func (s *WebSocketService) DisconnectClient(clientID string) error {
	s.mu.RLock()
	client, ok := s.clients[clientID]
	s.mu.RUnlock()
	
	if !ok {
		return fmt.Errorf("client not found")
	}
	
	s.hub.unregister <- client
	
	s.mu.Lock()
	delete(s.clients, clientID)
	s.mu.Unlock()
	
	return nil
}

// GetStats gets WebSocket stats
func (s *WebSocketService) GetStats() (*Stats, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	clientCount := len(s.clients)
	channelCounts := make(map[string]int)
	
	for name, ch := range s.channels {
		ch.mu.RLock()
		channelCounts[name] = len(ch.Clients)
		ch.mu.RUnlock()
	}
	
	return &Stats{
		TotalClients:  clientCount,
		Channels:    channelCounts,
	}, nil
}

// Stats represents WebSocket stats
type Stats struct {
	TotalClients int            `json:"totalClients"`
	Channels    map[string]int `json:"channels"`
}

// BlockEvent represents new block event
type BlockEvent struct {
	Number     uint64 `json:"number"`
	Hash       string `json:"hash"`
	ParentHash string `json:"parentHash"`
	Timestamp  int64  `json:"timestamp"`
	TxCount   int    `json:"txCount"`
}

// TransactionEvent represents new transaction event
type TransactionEvent struct {
	Hash       string `json:"hash"`
	From      string `json:"from"`
	To        string `json:"to"`
	Value     string `json:"value"`
	GasPrice  string `json:"gasPrice"`
	Timestamp int64  `json:"timestamp"`
}

// LogEvent represents new log event
type LogEvent struct {
	Address string   `json:"address"`
	Topics  []string `json:"topics"`
	Data    string   `json:"data"`
	Block   uint64   `json:"blockNumber"`
	TxHash  string   `json:"transactionHash"`
}

// TokenEvent represents new token event
type TokenEvent struct {
	Address  string `json:"address"`
	Symbol   string `json:"symbol"`
	Name     string `json:"name"`
	Decimals int   `json:"decimals"`
	Type     string `json:"type"`
}

// GasUpdate represents gas price update
type GasUpdate struct {
	Slow     int64 `json:"slow"`
	Standard int64 `json:"standard"`
	Fast     int64 `json:"fast"`
	BaseFee  int64 `json:"baseFee"`
}

// PriceUpdate represents price update
type PriceUpdate struct {
	Symbol string  `json:"symbol"`
	Price  float64 `json:"price"`
	Change float64 `json:"change24h"`
}

// InitWebSocketService initializes the service
func InitWebSocketService() (*WebSocketService, error) {
	return NewWebSocketService(), nil
}