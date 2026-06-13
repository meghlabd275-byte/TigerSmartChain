// Package ws provides WebSocket server for real-time blockchain data
// Built with Go for high performance and low latency
package ws

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"sync"
	"time"

	"github.com/gorilla/websocket"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds WebSocket server configuration
type Config struct {
	Port              string
	ReadBufferSize   int
	WriteBufferSize  int
	PoolMaxReco       int
	DBURL            string
	RedisURL         string
	HeartbeatInterval time.Duration
	MaxMessageSize   int64
}

// Server represents the WebSocket server
type Server struct {
	cfg       *Config
	upgrader  websocket.Upgrader
	pool      *pgxpool.Pool
	redis    *redis.Client
	clients  map[*websocket.Conn]*Client
	clientsMu sync.RWMutex
	ctx      context.Context
	cancel   context.CancelFunc
}

// Client represents a connected WebSocket client
type Client struct {
	conn       *websocket.Conn
	send       chan []byte
	 subscriptions map[string]bool
	mu         sync.Mutex
	addr       string
	joinedAt   time.Time
}

// Message represents a WebSocket message
type Message struct {
	Type    string          `json:"type"`
	Payload json.RawMessage `json:"payload"`
}

// BlockMessage represents a new block message
type BlockMessage struct {
	Number       uint64   `json:"number"`
	Hash         string  `json:"hash"`
	ParentHash   string  `json:"parentHash"`
	Timestamp   uint64   `json:"timestamp"`
	TxCount     int     `json:"txCount"`
	GasUsed     uint64  `json:"gasUsed"`
	Miner       string  `json:"miner"`
}

// TransactionMessage represents a new transaction message
type TransactionMessage struct {
	Hash        string `json:"hash"`
	From       string `json:"from"`
	To         string `json:"to"`
	Value      string `json:"value"`
	GasPrice   uint64 `json:"gasPrice"`
	GasUsed    uint64 `json:"gasUsed"`
	Timestamp  uint64 `json:"timestamp"`
	Status    string `json:"status"`
}

// PendingTXMessage represents a pending transaction message
type PendingTXMessage struct {
	Hash       string `json:"hash"`
	From       string `json:"from"`
	To         string `json:"to"`
	Value      string `json:"value"`
	GasPrice   uint64 `json:"gasPrice"`
	Nonce     uint64 `json:"nonce"`
	Timestamp uint64 `json:"timestamp"`
}

// NewServer creates a new WebSocket server
func NewServer(cfg *Config) (*Server, error) {
	ctx, cancel := context.WithCancel(context.Background())
	
	// Database connection
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	
	// Redis connection
	rdb := redis.NewClient(&redis.Options{
		Addr:     cfg.RedisURL,
		Password: "",
		DB:       0,
	})
	
	// Test Redis connection
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	
	srv := &Server{
		cfg:      cfg,
		pool:     pool,
		redis:    rdb,
		clients:  make(map[*websocket.Conn]*Client),
		ctx:      ctx,
		cancel:   cancel,
	}
	
	srv.upgrader = websocket.Upgrader{
		ReadBufferSize:  cfg.ReadBufferSize,
		WriteBufferSize: cfg.WriteBufferSize,
		CheckOrigin: func(r *http.Request) bool {
			return true // In production, implement proper origin checking
		},
	}
	
	return srv, nil
}

// Start starts the WebSocket server
func (s *Server) Start() error {
	// HTTP handler for WebSocket upgrades
	http.HandleFunc("/ws", s.handleWebSocket)
	
	// Health check
	http.HandleFunc("/health", s.handleHealth)
	
	addr := fmt.Sprintf(":%s", s.cfg.Port)
	srv := &http.Server{
		Addr:         addr,
		ReadTimeout:  10 * time.Second,
		WriteTimeout: 10 * time.Second,
		IdleTimeout:  120 * time.Second,
	}
	
	// Start heartbeat goroutine
	go s.heartbeatLoop()
	
	// Start Redis subscriber for real-time data
	go s.subscribeToRedis()
	
	return srv.ListenAndServe()
}

// handleWebSocket handles WebSocket connections
func (s *Server) handleWebSocket(w http.ResponseWriter, r *http.Request) {
	conn, err := s.upgrader.Upgrade(w, r, nil)
	if err != nil {
		return
	}
	
	client := &Client{
		conn:          conn,
		send:          make(chan []byte, 256),
		subscriptions: make(map[string]bool),
		addr:          r.RemoteAddr,
		joinedAt:     time.Now(),
	}
	
	// Register client
	s.clientsMu.Lock()
	s.clients[conn] = client
	s.clientsMu.Unlock()
	
	// Start read/write goroutines
	go s.readPump(client)
	go s.writePump(client)
	
	// Send welcome message
	welcome := Message{
		Type: "welcome",
		Payload: json.RawMessage(`{"status":"connected"}`),
	}
	msg, _ := json.Marshal(welcome)
	client.send <- msg
}

// readPump reads messages from the client
func (s *Server) readPump(client *Client) {
	defer func() {
		s.removeClient(client)
		client.conn.Close()
	}()
	
	client.conn.SetReadLimit(s.cfg.MaxMessageSize)
	client.conn.SetReadDeadline(time.Now().Add(60 * time.Second))
	
	for {
		_, msg, err := client.conn.ReadMessage()
		if err != nil {
			break
		}
		
		var message Message
		if err := json.Unmarshal(msg, &message); err != nil {
			continue
		}
		
		s.handleMessage(client, &message)
	}
}

// handleMessage handles incoming messages
func (s *Server) handleMessage(client *Client, message *Message) {
	switch message.Type {
	case "subscribe":
		var payload struct {
			Channels []string `json:"channels"`
		}
		json.Unmarshal(message.Payload, &payload)
		
		client.mu.Lock()
		for _, channel := range payload.Channels {
			client.subscriptions[channel] = true
		}
		client.mu.Unlock()
		
	case "unsubscribe":
		var payload struct {
			Channels []string `json:"channels"`
		}
		json.Unmarshal(message.Payload, &payload)
		
		client.mu.Lock()
		for _, channel := range payload.Channels {
			delete(client.subscriptions, channel)
		}
		client.mu.Unlock()
	}
}

// writePump writes messages to the client
func (s *Server) writePump(client *Client) {
	ticker := time.NewTicker(30 * time.Second)
	defer func() {
		ticker.Stop()
		client.conn.Close()
	}()
	
	for {
		select {
		case msg, ok := <-client.send:
			client.conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
			if !ok {
				client.conn.WriteMessage(websocket.CloseMessage, []byte{})
				return
			}
			
			if err := client.conn.WriteMessage(websocket.TextMessage, msg); err != nil {
				return
			}
			
		case <-ticker.C:
			client.conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
			if err := client.conn.WriteMessage(websocket.PingMessage, []byte{}); err != nil {
				return
			}
		}
	}
}

// removeClient removes a client from the server
func (s *Server) removeClient(client *Client) {
	s.clientsMu.Lock()
	delete(s.clients, client.conn)
	s.clientsMu.Unlock()
}

// heartbeatLoop sends heartbeats and cleans up stale connections
func (s *Server) heartbeatLoop() {
	ticker := time.NewTicker(s.cfg.HeartbeatInterval)
	defer ticker.Stop()
	
	for {
		select {
		case <-s.ctx.Done():
			return
		case <-ticker.C:
			s.clientsMu.Lock()
			for conn, client := range s.clients {
				if err := conn.WriteMessage(websocket.PingMessage, []byte{}); err != nil {
					delete(s.clients, client)
					conn.Close()
				}
			}
			s.clientsMu.Unlock()
		}
	}
}

// subscribeToRedis subscribes to Redis for real-time data
func (s *Server) subscribeToRedis() {
	pubsub := s.redis.Subscribe(s.ctx, "new_block", "new_transaction", "pending_transaction")
	defer pubsub.Close()
	
	ch := pubsub.Channel()
	
	for {
		select {
		case <-s.ctx.Done():
			return
		case msg := <-ch:
			s.broadcastToSubscribers(msg.Channel, []byte(msg.Payload))
		}
	}
}

// broadcastToSubscribers broadcasts to subscribed clients
func (s *Server) broadcastToSubscribers(channel string, data []byte) {
	s.clientsMu.RLock()
	defer s.clientsMu.RUnlock()
	
	for _, client := range s.clients {
		client.mu.Lock()
		subscribed := client.subscriptions[channel]
		client.mu.Unlock()
		
		if subscribed {
			select {
			case client.send <- data:
			default:
			}
		}
		}
	}
}

// handleHealth handles health check requests
func (s *Server) handleHealth(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"status":   "healthy",
		"clients": len(s.clients),
	})
}

// BroadcastNewBlock broadcasts a new block to subscribers
func (s *Server) BroadcastNewBlock(block *BlockMessage) error {
	data, err := json.Marshal(block)
	if err != nil {
		return err
	}
	
	msg := Message{
		Type:    "new_block",
		Payload: data,
	}
	
	msgData, _ := json.Marshal(msg)
	
	// Store in Redis for new connections
	s.redis.Set(s.ctx, "latest_block", string(data), time.Hour)
	
	// Broadcast to subscribers
	s.broadcastToSubscribers("new_block", msgData)
	
	return nil
}

// BroadcastNewTransaction broadcasts a new transaction to subscribers
func (s *Server) BroadcastNewTransaction(tx *TransactionMessage) error {
	data, err := json.Marshal(tx)
	if err != nil {
		return err
	}
	
	msg := Message{
		Type:    "new_transaction",
		Payload: data,
	}
	
	msgData, _ := json.Marshal(msg)
	
	// Store in Redis
	s.redis.Set(s.ctx, fmt.Sprintf("tx:%s", tx.Hash), string(data), time.Hour)
	
	// Broadcast to subscribers
	s.broadcastToSubscribers("new_transaction", msgData)
	
	return nil
}

// BroadcastPendingTransaction broadcasts a pending transaction
func (s *Server) BroadcastPendingTransaction(tx *PendingTXMessage) error {
	data, err := json.Marshal(tx)
	if err != nil {
		return err
	}
	
	msg := Message{
		Type:    "pending_transaction",
		Payload: data,
	}
	
	msgData, _ := json.Marshal(msg)
	
	// Broadcast to subscribers
	s.broadcastToSubscribers("pending_transaction", msgData)
	
	return nil
}

// Close closes the server and all connections
func (s *Server) Close() error {
	s.cancel()
	
	s.clientsMu.RLock()
	for conn := range s.clients {
		conn.Close()
	}
	s.clientsMu.RUnlock()
	
	s.pool.Close()
	s.redis.Close()
	
	return nil
}

var _ = fmt.Sprintf("")