// Package websocket provides WebSocket API for real-time data streaming
package websocket

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/go-redis/redis/v8"
	"github.com/gorilla/websocket"
	
	"tigersmartchain/explorer/services/tokens"
	"tigersmartchain/explorer/services/nfts"
	"tigersmartchain/explorer/services/analytics"
	"tigersmartchain/explorer/services/blocks"
)

// Config holds the WebSocket API configuration
type Config struct {
	Port            string
	RedisURL        string
	AllowedOrigins []string
	MaxConnections int
	PingInterval    time.Duration
	PongTimeout    time.Duration
}

// EventType represents the type of WebSocket event
type EventType string

const (
	EventNewBlock         EventType = "new_block"
	EventNewTransaction  EventType = "new_transaction"
	EventPendingTx      EventType = "pending_transaction"
	EventTokenTransfer  EventType = "token_transfer"
	EventNFTTransfer   EventType = "nft_transfer"
	EventGasUpdate     EventType = "gas_update"
	EventPriceUpdate   EventType = "price_update"
	EventValidatorUpdate EventType = "validator_update"
)

// WSMessage represents a WebSocket message
type WSMessage struct {
	Type      EventType   `json:"type"`
	Timestamp time.Time  `json:"timestamp"`
	Data      interface{} `json:"data"`
}

// Subscription represents a WebSocket subscription
type Subscription struct {
	Event    EventType `json:"event"`
	Filter  string   `json:"filter,omitempty"`
}

// Client represents a WebSocket client
type Client struct {
	ID          string
	Conn       *websocket.Conn
	Subscribed map[EventType]bool
	Send       chan []byte
	mu         sync.Mutex
}

// Hub maintains active WebSocket connections
type Hub struct {
	clients    map[*Client]bool
	broadcast  chan []byte
	register chan *Client
	unregister chan *Client
	mu        sync.RWMutex
	pubsub    *redis.Client
	config   *Config
	blockSvc *blocks.BlockService
	txSvc    *analytics.AnalyticsService
	tokenSvc *tokens.TokenService
	nftSvc  *nfts.NFTService
}

// NewHub creates a new WebSocket hub
func NewHub(config *Config, pubsub *redis.Client) *Hub {
	return &Hub{
		broadcast:   make(chan []byte),
		register:  make(chan *Client),
		unregister: make(chan *Client),
		clients:   make(map[*Client]bool),
		pubsub:    pubsub,
		config:   config,
	}
}

// SetServices sets the backend services
func (h *Hub) SetServices(blockSvc *blocks.BlockService, txSvc *analytics.AnalyticsService, tokenSvc *tokens.TokenService, nftSvc *nfts.NFTService) {
	h.blockSvc = blockSvc
	h.txSvc = txSvc
	h.tokenSvc = tokenSvc
	h.nftSvc = nftSvc
}

// Run starts the hub's main loop
func (h *Hub) Run(ctx context.Context) {
	ticker := time.NewTicker(h.config.PingInterval)
	defer func() {
		ticker.Stop()
	}()
	
	for {
		select {
		case client := <-h.register:
			h.mu.Lock()
			h.clients[client] = true
			h.mu.Unlock()
			
		case client := <-h.unregister:
			h.mu.Lock()
			if _, ok := h.clients[client]; ok {
				delete(h.clients, client)
				close(client.Send)
			}
			h.mu.Unlock()
			
		case message := <-h.broadcast:
			h.mu.RLock()
			for client := range h.clients {
				select {
				case client.Send <- message:
				default:
					close(client.Send)
					delete(h.clients, client)
				}
			}
			h.mu.RUnlock()
			
		case <-ticker.C:
			h.mu.RLock()
			for client := range h.clients {
				client.mu.Lock()
				client.Conn.SetWriteDeadline(time.Now().Add(h.config.PongTimeout))
				err := client.Conn.WriteMessage(websocket.PingMessage, []byte{})
				if err != nil {
					close(client.Send)
					delete(h.clients, client)
				}
				client.mu.Unlock()
			}
			h.mu.RUnlock()
			
		case <-ctx.Done():
			return
		}
	}
}

// BroadcastMessage broadcasts a message to all connected clients
func (h *Hub) BroadcastMessage(eventType EventType, data interface{}) {
	msg := WSMessage{
		Type:      eventType,
		Timestamp: time.Now(),
		Data:      data,
	}
	
	jsonData, err := json.Marshal(msg)
	if err != nil {
		return
	}
	
	h.broadcast <- jsonData
}

// Subscribe subscribes a client to an event type
func (h *Hub) Subscribe(client *Client, event EventType) {
	client.mu.Lock()
	client.Subscribed[event] = true
	client.mu.Unlock()
}

// Unsubscribe unsubscribes a client from an event type
func (h *Hub) Unsubscribe(client *Client, event EventType) {
	client.mu.Lock()
	delete(client.Subscribed, event)
	client.mu.Unlock()
}

// Upgrader configures WebSocket upgrade settings
var upgrader = websocket.Upgrader{
	ReadBufferSize:  1024,
	WriteBufferSize: 1024,
	CheckOrigin: func(r *http.Request) bool {
		return true // In production, check against allowed origins
	},
}

// WSHandler handles WebSocket connections
type WSHandler struct {
	hub *Hub
}

// NewWSHandler creates a new WebSocket handler
func NewWSHandler(hub *Hub) *WSHandler {
	return &WSHandler{hub: hub}
}

// handleWebSocket handles the WebSocket connection
func (h *WSHandler) handleWebSocket(c *gin.Context) {
	conn, err := upgrader.Upgrade(c.Writer, c.Request, nil)
	if err != nil {
		return
	}
	
	client := &Client{
		ID:          fmt.Sprintf("client_%d", time.Now().UnixNano()),
		Conn:       conn,
		Subscribed: make(map[EventType]bool),
		Send:      make(chan []byte, 256),
	}
	
	h.hub.register <- client
	
	go h.writePump(client)
	go h.readPump(client)
}

// readPump reads messages from the WebSocket
func (h *WSHandler) readPump(client *Client) {
	defer func() {
		h.hub.unregister <- client
		client.Conn.Close()
	}()
	
	client.Conn.SetReadLimit(512)
	client.Conn.SetReadDeadline(time.Now().Add(60 * time.Second))
	client.Conn.SetPongHandler(func() error {
		client.Conn.SetReadDeadline(time.Now().Add(60 * time.Second))
		return nil
	})
	
	for {
		_, message, err := client.Conn.ReadMessage()
		if err != nil {
			break
		}
		
		var sub Subscription
		if err := json.Unmarshal(message, &sub); err != nil {
			continue
		}
		
		switch sub.Event {
		case EventNewBlock:
			h.hub.Subscribe(client, EventNewBlock)
		case EventNewTransaction:
			h.hub.Subscribe(client, EventNewTransaction)
		case EventPendingTx:
			h.hub.Subscribe(client, EventPendingTx)
		case EventTokenTransfer:
			h.hub.Subscribe(client, EventTokenTransfer)
		case EventNFTTransfer:
			h.hub.Subscribe(client, EventNFTTransfer)
		case EventGasUpdate:
			h.hub.Subscribe(client, EventGasUpdate)
		case EventPriceUpdate:
			h.hub.Subscribe(client, EventPriceUpdate)
		}
	}
}

// writePump writes messages to the WebSocket
func (h *WSHandler) writePump(client *Client) {
	ticker := time.NewTicker(30 * time.Second)
	defer func() {
		ticker.Stop()
		client.Conn.Close()
	}()
	
	for {
		select {
		case message, ok := <-client.Send:
			client.Conn.SetWriteDeadline(time.Now.Add(10 * time.Second))
			if !ok {
				client.Conn.WriteMessage(websocket.CloseMessage, []byte{})
				return
			}
			
			w, err := client.Conn.NextWriter(websocket.TextMessage)
			if err != nil {
				return
			}
			w.Write(message)
			
			if err := w.Close(); err != nil {
				return
			}
		case <-ticker.C:
			client.Conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
			if err := client.Conn.WriteMessage(websocket.PingMessage, []byte{}); err != nil {
				return
			}
		}
	}
}

// EventSubscriptionHandler handles event subscriptions
func (h *WSHandler) EventSubscriptionHandler(c *gin.Context) {
	var sub Subscription
	if err := json.NewDecoder(c.Request.Body).Decode(&sub); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid subscription"})
		return
	}
	
	// Return the subscription details
	c.JSON(http.StatusOK, gin.H{
		"status":    "subscribed",
		"event":    sub.Event,
		"filter":   sub.Filter,
	})
}

// GetSubscriptionsHandler returns the current subscriptions
func (h *WSHandler) GetSubscriptionsHandler(c *gin.Context) {
	// Return available event types
	c.JSON(http.StatusOK, gin.H{
		"events": []EventType{
			EventNewBlock,
			EventNewTransaction,
			EventPendingTx,
			EventTokenTransfer,
			EventNFTTransfer,
			EventGasUpdate,
			EventPriceUpdate,
			EventValidatorUpdate,
		},
	})
}

// MetricsHandler returns WebSocket metrics
func (h *WSHandler) MetricsHandler(c *gin.Context) {
	h.hub.mu.RLock()
	count := len(h.hub.clients)
	h.hub.mu.RUnlock()
	
	c.JSON(http.StatusOK, gin.H{
		"active_connections": count,
		"max_connections":   h.hub.config.MaxConnections,
	})
}

// EventHandler handles individual event streams
func (h *WSHandler) EventHandler(c *gin.Context, eventType EventType) {
	conn, err := upgrader.Upgrade(c.Writer, c.Request, nil)
	if err != nil {
		return
	}
	
	client := &Client{
		ID:          fmt.Sprintf("event_%s_%d", eventType, time.Now().UnixNano()),
		Conn:       conn,
		Subscribed: map[EventType]bool{eventType: true},
		Send:      make(chan []byte, 256),
	}
	
	h.hub.register <- client
	
	go h.writePump(client)
	go h.streamEvents(client, eventType)
}

// streamEvents streams specific events to the client
func (h *WSHandler) streamEvents(client *Client, eventType EventType) {
	ctx, cancel := context.WithCancel(context.Background())
	defer func() {
		cancel()
		h.hub.unregister <- client
		client.Conn.Close()
	}()
	
	ticker := time.NewTicker(time.Second)
	defer ticker.Stop()
	
	for {
		select {
		case <-ctx.Done():
			return
		case message := <-client.Send:
			client.mu.Lock()
			client.Conn.WriteJSON(message)
			client.mu.Unlock()
		case <-ticker.C:
			// Send periodic data based on event type
			var data interface{}
			switch eventType {
			case EventNewBlock:
				if h.hub.blockSvc != nil {
					data = h.hub.blockSvc.GetLatestBlock()
				}
			case EventNewTransaction:
				if h.hub.txSvc != nil {
					data = h.hub.txSvc.GetLatestTransactions()
				}
			case EventPendingTx:
				if h.hub.txSvc != nil {
					data = h.hub.txSvc.GetPendingTransactions()
				}
			case EventTokenTransfer:
				if h.hub.tokenSvc != nil {
					data = h.hub.tokenSvc.GetLatestTransfers()
				}
			case EventNFTTransfer:
				if h.hub.nftSvc != nil {
					data = h.hub.nftSvc.GetLatestTransfers()
				}
			}
			
			if data != nil {
				msg := WSMessage{
					Type:      eventType,
					Timestamp: time.Now(),
					Data:      data,
				}
				client.mu.Lock()
				client.Conn.WriteJSON(msg)
				client.mu.Unlock()
			}
		}
	}
}

// Router sets up the WebSocket router
func (h *WSHandler) Router() *gin.Engine {
	r := gin.Default()
	
	r.GET("/ws", h.handleWebSocket)
	r.GET("/ws/events", h.handleWebSocket)
	r.GET("/ws/events/:type", func(c *gin.Context) {
		eventType := EventType(c.Param("type"))
		h.EventHandler(c, eventType)
	})
	r.GET("/ws/subscribe", h.handleWebSocket)
	r.GET("/ws/subscriptions", h.GetSubscriptionsHandler)
	r.GET("/ws/metrics", h.MetricsHandler)
	r.POST("/ws/subscribe", h.EventSubscriptionHandler)
	
	return r
}

// StartWSServer starts the WebSocket server
func StartWSServer(port string, redisURL string) error {
	config := &Config{
		Port:            port,
		RedisURL:        redisURL,
		MaxConnections: 10000,
		PingInterval:    30 * time.Second,
		PongTimeout:    60 * time.Second,
	}
	
	rdb, err := redis.ParseURL(redisURL)
	if err != nil {
		return err
	}
	
	pubsub := redis.NewClient(rdb)
	hub := NewHub(config, pubsub)
	
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	
	go hub.Run(ctx)
	
	handler := NewWSHandler(hub)
	router := handler.Router()
	
	return router.Run(port)
}

// ParseEventType parses an event type string
func ParseEventType(event string) (EventType, error) {
	switch event {
	case "new_block":
		return EventNewBlock, nil
	case "new_transaction":
		return EventNewTransaction, nil
	case "pending_transaction":
		return EventPendingTx, nil
	case "token_transfer":
		return EventTokenTransfer, nil
	case "nft_transfer":
		return EventNFTTransfer, nil
	case "gas_update":
		return EventGasUpdate, nil
	case "price_update":
		return EventPriceUpdate, nil
	case "validator_update":
		return EventValidatorUpdate, nil
	default:
		return "", fmt.Errorf("unknown event type: %s", event)
	}
}

// GetEventTypes returns all available event types
func GetEventTypes() []string {
	return []string{
		"new_block",
		"new_transaction",
		"pending_transaction",
		"token_transfer",
		"nft_transfer",
		"gas_update",
		"price_update",
		"validator_update",
	}
}

// SubscriptionRequest represents a subscription request
type SubscriptionRequest struct {
	Event  string `json:"event"`
	ID     string `json:"id,omitempty"`
	Filter string `json:"filter,omitempty"`
}

// SubscriptionResponse represents a subscription response
type SubscriptionResponse struct {
	Status  string `json:"status"`
	Event   string `json:"event"`
	ID      string `json:"id,omitempty"`
	Message string `json:"message,omitempty"`
}

// HandleSubscription handles subscription requests
func HandleSubscription(c *gin.Context) {
	var req SubscriptionRequest
	if err := json.NewDecoder(c.Request.Body).Decode(&req); err != nil {
		c.JSON(http.StatusBadRequest, SubscriptionResponse{
			Status:  "error",
			Message: "Invalid request",
		})
		return
	}
	
	_, err := ParseEventType(req.Event)
	if err != nil {
		c.JSON(http.StatusBadRequest, SubscriptionResponse{
			Status:  "error",
			Message: err.Error(),
		})
		return
	}
	
	c.JSON(http.StatusOK, SubscriptionResponse{
		Status: "subscribed",
		Event:  req.Event,
		ID:     req.ID,
	})
}

// GetFilters returns available filters for an event type
func GetFilters(eventType EventType) []string {
	switch eventType {
	case EventTokenTransfer:
		return []string{"token_address", "from_address", "to_address"}
	case EventNFTTransfer:
		return []string{"token_address", "token_id", "from_address", "to_address"}
	case EventNewTransaction:
		return []string{"from_address", "to_address"}
	default:
		return []string{}
	}
}

// ParseLimit parses the limit parameter
func ParseLimit(limitStr string) int {
	limit, err := strconv.Atoi(limitStr)
	if err != nil || limit <= 0 {
		return 100
	}
	if limit > 1000 {
		return 1000
	}
	return limit
}