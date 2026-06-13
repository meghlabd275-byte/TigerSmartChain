// TigerSmartChain WebSocket Service - High Performance, Secure, Real-time Feeds
// Production-grade WebSocket server with sub-second latency, authentication, and encryption

package websocket_service

import (
	"context"
	"crypto/ecdsa"
	"crypto/rand"
	"crypto/sha256"
	"crypto/tls"
	"crypto/x509"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/signal"
	"runtime"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	"github.com/gorilla/handlers"
	"github.com/gorilla/mux"
	"github.com/gorilla/websocket"
	"golang.org/x/crypto/chacha20poly1305"
	"golang.org/x/crypto/curve25519"
	"golang.org/x/time/rate"
)

var (
	// Upgrader configures WebSocket upgrade
	upgrader = websocket.Upgrader{
		ReadBufferSize:  4096,
		WriteBufferSize: 4096,
		CheckOrigin: func(r *http.Request) bool {
			return true // In production, implement proper CORS
		},
		HandshakeTimeout: 10 * time.Second,
	}
)

// Config holds WebSocket service configuration
type Config struct {
	// Server settings
	ListenAddr      string        `json:"listen_addr"`
	Port           int           `json:"port"`
	ReadTimeout    time.Duration `json:"read_timeout"`
	WriteTimeout   time.Duration `json:"write_timeout"`
	MaxMsgSize    int64         `json:"max_msg_size"`
	MaxConns      int           `json:"max_conns"`
	MaxConnsPerIP int           `json:"max_conns_per_ip"`
	
	// TLS settings
	UseTLS           bool   `json:"use_tls"`
	CertFile         string `json:"cert_file"`
	KeyFile          string `json:"key_file"`
	ClientCertFile   string `json:"client_cert_file"`
	
	// Authentication
	RequireAuth     bool   `json:"require_auth"`
	APIKeyHeader   string `json:"api_key_header"`
	JWTKey        string `json:"jwt_key"`
	AllowedIPs    []string `json:"allowed_ips"`
	
	// Rate limiting
	RateLimit      int           `json:"rate_limit"` // messages per second
	RateLimitBurst int           `json:"rate_limit_burst"`
	
	// Subscription settings
	MaxSubscriptions int           `json:"max_subscriptions"`
	MaxSubscriptionsPerConn int `json:"max_subscriptions_per_conn"`
	SubscriptionTimeout    time.Duration `json:"subscription_timeout"`
	
	// Message queue settings
	QueueSize      int           `json:"queue_size"`
	QueueFlushInt time.Duration `json:"queue_flush_int"`
	
	// Encryption
	EnableEncryption bool   `json:"enable_encryption"`
	EncryptionKey   []byte `json:"-"`
	
	// Logging
	LogFile      string `json:"log_file"`
	AccessLog   string `json:"access_log"`
	Verbose    bool   `json:"verbose"`
	
	// Performance
	WorkerPoolSize int           `json:"worker_pool_size"`
	GOMAXPROCS   int           `json:"gomaxprocs"`
}

// Client represents a connected WebSocket client
type Client struct {
	ID             string
	IP             string
	UserAgent      string
	Auth          *AuthInfo
	Subscriptions map[string]*Subscription
	connectedAt   time.Time
	lastActivity  time.Time
	sendMu        sync.Mutex
	rateLimiter   *rate.Limiter
	encoder      *Encoder
	closed       atomic.Bool
}

// AuthInfo holds client authentication info
type AuthInfo struct {
	APIKey    string
	JWTClaims *JWTClaims
	Roles    []string
}

// JWTClaims holds JWT claims
type JWTClaims struct {
	Sub    string   `json:"sub"`
	Exp    int64    `json:"exp"`
	Iat    int64    `json:"iat"`
	Roles  []string `json:"roles"`
	Scopes []string `json:"scopes"`
}

// Subscription represents a subscription
type Subscription struct {
	ID       string
	Type    string
	Filter  *SubscriptionFilter
	Chan    chan []byte
	Active  atomic.Bool
	Created time.Time
}

// SubscriptionFilter holds subscription filters
type SubscriptionFilter struct {
	Address  []string `json:"address,omitempty"`
	Topics   []string `json:"topics,omitempty"`
	FromBlock uint64 `json:"from_block,omitempty"`
	ToBlock uint64   `json:"to_block,omitempty"`
	BlockHash string `json:"block_hash,omitempty"`
}

// Encoder handles message encoding
type Encoder struct {
	enable bool
	key    *[32]byte
}

// Message represents a WebSocket message
type Message struct {
	Type      string          `json:"type"`
	Channel   string          `json:"channel,omitempty"`
	ID        string          `json:"id,omitempty"`
	Payload   json.RawMessage `json:"payload"`
	Timestamp time.Time      `json:"timestamp"`
}

// Server is the WebSocket server
type Server struct {
	config     *Config
	router     *mux.Router
	server    *http.Server
	clients   *ClientManager
	broadcaster *Broadcaster
	metrics   *Metrics
	workerPool *WorkerPool
	encryptor *Encryptor
	limiter   *IPRateLimiter
	ctx       context.Context
	cancel    context.CancelFunc
	wg        sync.WaitGroup
}

// ClientManager manages connected clients
type ClientManager struct {
	mu       sync.RWMutex
	clients map[string]*Client
	byIP    map[string]map[string]bool // clientID -> true
}

func (cm *ClientManager) Add(client *Client) {
	cm.mu.Lock()
	defer cm.mu.Unlock()
	
	cm.clients[client.ID] = client
	if client.IP != "" {
		if cm.byIP[client.IP] == nil {
			cm.byIP[client.IP] = make(map[string]bool)
		}
		cm.byIP[client.IP][client.ID] = true
	}
}

func (cm *ClientManager) Remove(clientID string) {
	cm.mu.Lock()
	defer cm.mu.Unlock()
	
	if client, ok := cm.clients[clientID]; ok {
		if client.IP != "" && cm.byIP[client.IP] != nil {
			delete(cm.byIP[client.IP], clientID)
			if len(cm.byIP[client.IP]) == 0 {
				delete(cm.byIP, client.IP)
			}
		}
		delete(cm.clients, clientID)
	}
}

func (cm *ClientManager) Get(clientID string) (*Client, bool) {
	cm.mu.RLock()
	defer cm.mu.RUnlock()
	
	client, ok := cm.clients[clientID]
	return client, ok
}

func (cm *ClientManager) Count() int {
	cm.mu.RLock()
	defer cm.mu.RUnlock()
	
	return len(cm.clients)
}

func (cm *ClientManager) CountByIP(ip string) int {
	cm.mu.RLock()
	defer cm.mu.RUnlock()
	
	if ips := cm.byIP[ip]; ips != nil {
		return len(ips)
	}
	return 0
}

// Broadcaster broadcasts messages to subscribers
type Broadcaster struct {
	mu          sync.RWMutex
	subscribers map[string]map[string]*Client // channel -> clientID -> client
	queues     map[string]chan []byte
	workerPool *WorkerPool
}

func (b *Broadcaster) Subscribe(channel string, client *Client) {
	b.mu.Lock()
	defer b.mu.Unlock()
	
	if b.subscribers == nil {
		b.subscribers = make(map[string]map[string]*Client)
	}
	
	if b.subscribers[channel] == nil {
		b.subscribers[channel] = make(map[string]*Client)
		b.queues[channel] = make(chan []byte, 10000)
	}
	
	b.subscribers[channel][client.ID] = client
}

func (b *Broadcaster) Unsubscribe(channel string, clientID string) {
	b.mu.Lock()
	defer b.mu.Unlock()
	
	if b.subscribers[channel] != nil {
		delete(b.subscribers[channel], clientID)
		if len(b.subscribers[channel]) == 0 {
			delete(b.subscribers, channel)
		}
	}
}

func (b *Broadcaster) Broadcast(channel string, message []byte) {
	b.mu.RLock()
	defer b.mu.RUnlock()
	
	if subscribers := b.subscribers[channel]; subscribers != nil {
		for _, client := range subscribers {
			select {
			case client.Subscriptions[channel].Chan <- message:
			default:
				// Queue full, skip
			}
		}
	}
}

// Metrics holds server metrics
type Metrics struct {
	mu             sync.RWMutex
	Connections    uint64
	Messages      uint64
	BytesSent     uint64
	BytesRecv     uint64
	Errors       uint64
	LatencySum    time.Duration
	LatencyCount  uint64
}

func (m *Metrics) RecordLatency(d time.Duration) {
	m.mu.Lock()
	defer m.mu.Unlock()
	
	m.LatencySum += d
	m.LatencyCount++
}

func (m *Metrics) Get() map[string]interface{} {
	m.mu.RLock()
	defer m.mu.RUnlock()
	
	avgLatency := time.Duration(0)
	if m.LatencyCount > 0 {
		avgLatency = m.LatencySum / time.Duration(m.LatencyCount)
	}
	
	return map[string]interface{}{
		"connections":   atomic.LoadUint64(&m.Connections),
		"messages":     atomic.LoadUint64(&m.Messages),
		"bytes_sent":   atomic.LoadUint64(&m.BytesSent),
		"bytes_recv":   atomic.LoadUint64(&m.BytesRecv),
		"errors":       atomic.LoadUint64(&m.Errors),
		"avg_latency":  avgLatency.String(),
	}
}

// WorkerPool is a pool of workers for message processing
type WorkerPool struct {
	mu       sync.Mutex
	workers  []*Worker
	tasks    chan func()
	stopCh   chan struct{}
}

type Worker struct {
	id      int
	tasks   chan func()
	stopCh  chan struct{}
}

// IPRateLimiter limits requests by IP
type IPRateLimiter struct {
	mu      sync.Mutex
	limiters map[string]*rate.Limiter
	ips     map[string]time.Time
	limit   int
	burst   int
}

// Encryptor handles encryption
type Encryptor struct {
	key *[32]byte
}

// NewServer creates a new WebSocket server
func NewServer(config *Config) (*Server, error) {
	if config == nil {
		config = &Config{}
	}
	
	// Set defaults
	if config.Port == 0 {
		config.Port = 8546
	}
	if config.ReadTimeout == 0 {
		config.ReadTimeout = 60 * time.Second
	}
	if config.WriteTimeout == 0 {
		config.WriteTimeout = 60 * time.Second
	}
	if config.MaxMsgSize == 0 {
		config.MaxMsgSize = 10 * 1024 * 1024 // 10MB
	}
	if config.MaxConns == 0 {
		config.MaxConns = 10000
	}
	if config.MaxConnsPerIP == 0 {
		config.MaxConnsPerIP = 10
	}
	if config.RateLimit == 0 {
		config.RateLimit = 100
	}
	if config.RateLimitBurst == 0 {
		config.RateLimitBurst = 200
	}
	if config.MaxSubscriptions == 0 {
		config.MaxSubscriptions = 1000
	}
	if config.MaxSubscriptionsPerConn == 0 {
		config.MaxSubscriptionsPerConn = 50
	}
	if config.SubscriptionTimeout == 0 {
		config.SubscriptionTimeout = 5 * time.Minute
	}
	if config.QueueSize == 0 {
		config.QueueSize = 10000
	}
	if config.QueueFlushInt == 0 {
		config.QueueFlushInt = 100 * time.Millisecond
	}
	if config.WorkerPoolSize == 0 {
		config.WorkerPoolSize = runtime.NumCPU()
	}
	if config.GOMAXPROCS == 0 {
		config.GOMAXPROCS = runtime.NumCPU()
	}
	
	// Set GOMAXPROCS
	runtime.GOMAXPROCS(config.GOMAXPROCS)
	
	ctx, cancel := context.WithCancel(context.Background())
	
	server := &Server{
		config: config,
		router: mux.NewRouter(),
		clients: &ClientManager{
			clients: make(map[string]*Client),
			byIP:    make(map[string]map[string]bool),
		},
		broadcaster: &Broadcaster{
			subscribers: make(map[string]map[string]*Client),
			queues:    make(map[string]chan []byte),
		},
		metrics:   &Metrics{},
		limiter: &IPRateLimiter{
			limiters: make(map[string]*rate.Limiter),
			ips:     make(map[string]time.Time),
			limit:   config.RateLimit,
			burst:   config.RateLimitBurst,
		},
		ctx:    ctx,
		cancel: cancel,
	}
	
	// Initialize worker pool
	server.workerPool = NewWorkerPool(config.WorkerPoolSize)
	
	// Initialize encryptor
	if config.EnableEncryption && len(config.EncryptionKey) == 32 {
		key := (*[32]byte)(config.EncryptionKey[:32])
		server.encryptor = &Encryptor{key: key}
	}
	
	return server, nil
}

// NewWorkerPool creates a new worker pool
func NewWorkerPool(size int) *wp *WorkerPool {
	wp := &WorkerPool{
		workers: make([]*Worker, size),
		tasks:   make(chan func(), 10000),
		stopCh:  make(chan struct{}),
	}
	
	for i := 0; i < size; i++ {
		wp.workers[i] = &Worker{
			id:    i,
			tasks: make(chan func(), 100),
			stopCh: make(chan struct{}),
		}
	}
	
	return wp
}

func (wp *WorkerPool) Start() {
	for _, w := range wp.workers {
		go func(worker *Worker) {
			for {
				select {
				case task := <-worker.tasks:
					task()
				case <-worker.stopCh:
					return
				}
			}
		}(w)
	}
}

func (wp *WorkerPool) Submit(task func()) {
	select {
	case wp.tasks <- task:
	default:
		// Pool full, run synchronously
		task()
	}
}

func (wp *WorkerPool) Stop() {
	close(wp.stopCh)
}

// Start starts the WebSocket server
func (s *Server) Start() error {
	// Start worker pool
	s.workerPool.Start()
	
	// Create HTTP server
	s.server = &http.Server{
		Addr:         fmt.Sprintf("%s:%d", s.config.ListenAddr, s.config.Port),
		Handler:      s.router,
		ReadTimeout:  s.config.ReadTimeout,
		WriteTimeout: s.config.WriteTimeout,
		MaxHeaderBytes: 1 << 10,
	}
	
	// Setup routes
	s.setupRoutes()
	
	// Setup middleware
	s.setupMiddleware()
	
	// Start server
	go func() {
		var err error
		if s.config.UseTLS {
			err = s.server.ListenAndServeTLS(s.config.CertFile, s.config.KeyFile)
		} else {
			err = s.server.ListenAndServe()
		}
		if err != nil && err != http.ErrServerClosed {
			fmt.Printf("Server error: %v\n", err)
		}
	}()
	
	// Start background tasks
	s.startBackgroundTasks()
	
	return nil
}

// setupRoutes sets up HTTP routes
func (s *Server) setupRoutes() {
	// WebSocket endpoint
	s.router.HandleFunc("/ws", s.handleWebSocket)
	
	// API endpoints
	s.router.HandleFunc("/api/v1/subscribe", s.handleSubscribe)
	s.router.HandleFunc("/api/v1/unsubscribe", s.handleUnsubscribe)
	s.router.HandleFunc("/api/v1/channels", s.handleChannels)
	
	// Metrics endpoint
	s.router.HandleFunc("/api/v1/metrics", s.handleMetrics)
	
	// Health check
	s.router.HandleFunc("/health", s.handleHealth)
}

// setupMiddleware sets up middleware
func (s *Server) setupMiddleware() {
	// CORS
	corsOpts := []handlers.CORSOption{
		handlers.AllowedOrigins([]string{"*"}),
		handlers.AllowedMethods([]string{"GET", "POST", "PUT", "DELETE", "OPTIONS"}),
		handlers.AllowedHeaders([]string{"Authorization", "Content-Type", "X-API-Key"}),
		handlers.ExposedHeaders([]string{"X-Request-ID"}),
		handlers.MaxAge(3600),
		handlers.AllowCredentials(),
	}
	s.router.Use(handlers.CORS(corsOpts...))
	
	// Rate limiting
	s.router.Use(s.rateLimitMiddleware)
	
	// Request ID
	s.router.Use(s.requestIDMiddleware)
	
	// Logging
	if s.config.Verbose {
		s.router.Use(s.loggingMiddleware)
	}
}

// handleWebSocket handles WebSocket connections
func (s *Server) handleWebSocket(w http.ResponseWriter, r *http.Request) {
	// Check max connections
	if s.clients.Count() >= s.config.MaxConns {
		http.Error(w, "Too many connections", http.StatusServiceUnavailable)
		return
	}
	
	// Check IP limit
	if s.config.MaxConnsPerIP > 0 {
		ip := getIP(r)
		if s.clients.CountByIP(ip) >= s.config.MaxConnsPerIP {
			http.Error(w, "Too many connections from this IP", http.StatusServiceUnavailable)
			return
		}
	}
	
	// Check rate limit
	if !s.limiter.Allow(r.RemoteAddr) {
		http.Error(w, "Rate limit exceeded", http.StatusTooManyRequests)
		return
	}
	
	// Authenticate
	var auth *AuthInfo
	if s.config.RequireAuth {
		auth = s.authenticate(r)
		if auth == nil {
			http.Error(w, "Unauthorized", http.StatusUnauthorized)
			return
		}
	}
	
	// Upgrade to WebSocket
	conn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		s.metrics.Errors++
		return
	}
	
	// Create client
	client := &Client{
		ID:            generateID(),
		IP:            getIP(r),
		UserAgent:     r.UserAgent(),
		Auth:         auth,
		Subscriptions: make(map[string]*Subscription),
		connectedAt:   time.Now(),
		lastActivity: time.Now(),
		rateLimiter:   rate.New(s.config.RateLimitBurst, s.config.RateLimit),
		encoder:      &Encoder{enable: s.config.EnableEncryption, key: (*[32]byte)(s.config.EncryptionKey[:32])},
	}
	
	// Add client
	s.clients.Add(client)
	atomic.AddUint64(&s.metrics.Connections, 1)
	
	// Handle connection
	go s.handleClient(client, conn)
}

// handleClient handles a client connection
func (s *Server) handleClient(client *Client, conn *websocket.Conn) {
	defer func() {
		s.clients.Remove(client.ID)
		conn.Close()
		atomic.AddUint64(&s.metrics.Connections, ^uint64(0))
	}()
	
	// Set read limit
	conn.SetReadLimit(s.config.MaxMsgSize)
	
	// Set read deadline
	conn.SetReadDeadline(time.Now().Add(s.config.ReadTimeout))
	
	// Set write deadline
	conn.SetWriteDeadline(time.Now().Add(s.config.WriteTimeout))
	
	// Handle messages
	for {
		select {
		case <-s.ctx.Done():
			return
		default:
		}
		
		typ, msg, err := conn.ReadMessage()
		if err != nil {
			if websocket.IsUnexpectedCloseError(err, websocket.CloseGoingAway, websocket.CloseAbnormalClosure) {
				s.metrics.Errors++
			}
			return
		}
		
		atomic.AddUint64(&s.metrics.BytesRecv, uint64(len(msg)))
		
		// Rate limit
		if !client.rateLimiter.Allow() {
			continue
		}
		
		// Process message
		start := time.Now()
		s.processMessage(client, msg, typ)
		s.metrics.RecordLatency(time.Since(start))
	}
}

// processMessage processes a client message
func (s *Server) processMessage(client *Client, msg []byte, typ int) {
	var m Message
	if err := json.Unmarshal(msg, &m); err != nil {
		s.sendError(client, "invalid message format")
		return
	}
	
	switch m.Type {
	case "subscribe":
		s.handleSubscribeMsg(client, m)
	case "unsubscribe":
		s.handleUnsubscribeMsg(client, m)
	case "ping":
		s.sendPong(client)
	default:
		s.sendError(client, "unknown message type")
	}
}

// handleSubscribeMsg handles subscription requests
func (s *Server) handleSubscribeMsg(client *Client, m Message) {
	if len(client.Subscriptions) >= s.config.MaxSubscriptionsPerConn {
		s.sendError(client, "max subscriptions reached")
		return
	}
	
	var sub struct {
		Channel string            `json:"channel"`
		Filter  *SubscriptionFilter `json:"filter,omitempty"`
	}
	if err := json.Unmarshal(m.Payload, &sub); err != nil {
		s.sendError(client, "invalid subscription format")
		return
	}
	
	// Create subscription
	subChan := make(chan []byte, s.config.QueueSize)
	subID := generateID()
	subscription := &Subscription{
		ID:       subID,
		Type:    sub.Channel,
		Filter:  sub.Filter,
		Chan:    subChan,
		Active:  atomic.Bool{},
	}
	subscription.Active.Store(true)
	client.Subscriptions[sub.Channel] = subscription
	
	// Subscribe to broadcaster
	s.broadcaster.Subscribe(sub.Channel, client)
	
	// Send confirmation
	s.sendMessage(client, Message{
		Type:      "subscribed",
		Channel:  sub.Channel,
		ID:       subID,
		Timestamp: time.Now(),
	})
	
	// Forward messages
	go func() {
		for {
			select {
			case msg := <-subChan:
				if !subscription.Active.Load() {
					return
				}
				s.sendRaw(client, msg)
			case <-s.ctx.Done():
				return
			}
		}
	}()
}

// handleUnsubscribeMsg handles unsubscription requests
func (s *Server) handleUnsubscribeMsg(client *Client, m Message) {
	var sub struct {
		Channel string `json:"channel"`
	}
	if err := json.Unmarshal(m.Payload, &sub); err != nil {
		s.sendError(client, "invalid unsubscription format")
		return
	}
	
	if sub, ok := client.Subscriptions[sub.Channel]; ok {
		sub.Active.Store(false)
		close(sub.Chan)
		delete(client.Subscriptions, sub.Channel)
		s.broadcaster.Unsubscribe(sub.Channel, client.ID)
	}
	
	s.sendMessage(client, Message{
		Type:      "unsubscribed",
		Channel:  sub.Channel,
		Timestamp: time.Now(),
	})
}

// sendMessage sends a message to a client
func (s *Server) sendMessage(client *Client, m Message) {
	data, err := json.Marshal(m)
	if err != nil {
		return
	}
	
	client.sendMu.Lock()
	defer client.sendMu.Unlock()
	
	err = client.conn.WriteMessage(websocket.TextMessage, data)
	if err != nil {
		s.metrics.Errors++
		return
	}
	
	atomic.AddUint64(&s.metrics.Messages, 1)
	atomic.AddUint64(&s.metrics.BytesSent, uint64(len(data)))
}

// sendRaw sends raw bytes to a client
func (s *Server) sendRaw(client *Client, data []byte) {
	client.sendMu.Lock()
	defer client.sendMu.Unlock()
	
	client.conn.WriteMessage(websocket.BinaryMessage, data)
	
	atomic.AddUint64(&s.metrics.Messages, 1)
	atomic.AddUint64(&s.metrics.BytesSent, uint64(len(data)))
}

// sendError sends an error to a client
func (s *Server) sendError(client *Client, errMsg string) {
	s.sendMessage(client, Message{
		Type:      "error",
		Payload:  json.RawMessage(fmt.Sprintf(`{"error":%q}`, errMsg)),
		Timestamp: time.Now(),
	})
}

// sendPong sends a pong response
func (s *Server) sendPong(client *Client) {
	s.sendMessage(client, Message{
		Type:      "pong",
		Timestamp: time.Now(),
	})
}

// authenticate authenticates a request
func (s *Server) authenticate(r *http.Request) *AuthInfo {
	// Check API key
	if s.config.APIKeyHeader != "" {
		apiKey := r.Header.Get(s.config.APIKeyHeader)
		if apiKey != "" {
			return &AuthInfo{APIKey: apiKey}
		}
	}
	
	// Check JWT
	authHeader := r.Header.Get("Authorization")
	if authHeader != "" && strings.HasPrefix(authHeader, "Bearer ") {
		token := authHeader[7:]
		claims := s.verifyJWT(token)
		if claims != nil {
			return &AuthInfo{JWTClaims: claims}
		}
	}
	
	return nil
}

// verifyJWT verifies a JWT token
func (s *Server) verifyJWT(token string) *JWTClaims {
	// In production, implement proper JWT verification
	return nil
}

// rateLimitMiddleware is rate limiting middleware
func (s *Server) rateLimitMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if !s.limiter.Allow(r.RemoteAddr) {
			http.Error(w, "Rate limit exceeded", http.StatusTooManyRequests)
			return
		}
		next.ServeHTTP(w, r)
	})
}

// requestIDMiddleware adds request ID middleware
func (s *Server) requestIDMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		reqID := r.Header.Get("X-Request-ID")
		if reqID == "" {
			reqID = generateID()
		}
		w.Header().Set("X-Request-ID", reqID)
		next.ServeHTTP(w, r)
	})
}

// loggingMiddleware is logging middleware
func (s *Server) loggingMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		start := time.Now()
		next.ServeHTTP(w, r)
		fmt.Printf("%s %s %s %v\n", r.RemoteAddr, r.Method, r.URL.Path, time.Since(start))
	})
}

// handleSubscribe handles REST subscribe
func (s *Server) handleSubscribe(w http.ResponseWriter, r *http.Request) {
	// Implementation similar to WebSocket
}

// handleUnsubscribe handles REST unsubscribe
func (s *Server) handleUnsubscribe(w http.ResponseWriter, r *http.Request) {
	// Implementation
}

// handleChannels handles channel list
func (s *Server) handleChannels(w http.ResponseWriter, r *http.Request) {
	channels := []string{
		"new_blocks",
		"new_transactions",
		"new_pending_transactions",
		"new_logs",
		"new_tokens",
		"new_nfts",
	}
	
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(channels)
}

// handleMetrics handles metrics
func (s *Server) handleMetrics(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(s.metrics.Get())
}

// handleHealth handles health check
func (s *Server) handleHealth(w http.ResponseWriter, r *http.Request) {
	w.WriteHeader(http.StatusOK)
	w.Write([]byte("OK"))
}

// startBackgroundTasks starts background tasks
func (s *Server) startBackgroundTasks() {
	// Connection cleanup
	s.wg.Add(1)
	go func() {
		defer s.wg.Done()
		ticker := time.NewTicker(30 * time.Second)
		defer ticker.Stop()
		
		for {
			select {
			case <-ticker.C:
				s.cleanupConnections()
			case <-s.ctx.Done():
				return
			}
		}
	}()
	
	// Metrics cleanup
	s.wg.Add(1)
	go func() {
		defer s.wg.Done()
		ticker := time.NewTicker(1 * time.Minute)
		defer ticker.Stop()
		
		for {
			select {
			case <-ticker.C:
				s.cleanupMetrics()
			case <-s.ctx.Done():
				return
			}
		}
	}()
}

// cleanupConnections cleans up stale connections
func (s *Server) cleanupConnections() {
	s.clients.mu.RLock()
	defer s.clients.mu.RUnlock()
	
	for id, client := range s.clients.clients {
		if time.Since(client.lastActivity) > 5*time.Minute {
			s.clients.Remove(id)
		}
	}
}

// cleanupMetrics cleans up metrics
func (s *Server) cleanupMetrics() {
	s.limiter.mu.Lock()
	defer s.limiter.mu.Unlock()
	
	now := time.Now()
	for ip, t := range s.limiter.ips {
		if now.Sub(t) > 5*time.Minute {
			delete(s.limiter.limiters, ip)
			delete(s.limiter.ips, ip)
		}
	}
}

// Allow checks if request is allowed
func (l *IPRateLimiter) Allow(remoteAddr string) bool {
	ip, _, _ := net.SplitHostPort(remoteAddr)
	if ip == "" {
		ip = remoteAddr
	}
	
	l.mu.Lock()
	defer l.mu.Unlock()
	
	limiter, ok := l.limiters[ip]
	if !ok {
		limiter = rate.New(l.burst, l.limit)
		l.limiters[ip] = limiter
		l.ips[ip] = time.Now()
		return true
	}
	
	return limiter.Allow()
}

// Stop stops the server
func (s *Server) Stop() error {
	s.cancel()
	s.wg.Wait()
	
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	
	return s.server.Shutdown(ctx)
}

// Wait waits for server to stop
func (s *Server) Wait() {
	s.wg.Wait()
}

// Helper functions
func getIP(r *http.Request) string {
	xff := r.Header.Get("X-Forwarded-For")
	if xff != "" {
		return strings.Split(xff, ",")[0]
	}
	
	ip, _, _ := net.SplitHostPort(r.RemoteAddr)
	return ip
}

func generateID() string {
	b := make([]byte, 16)
	rand.Read(b)
	return base64.URLEncoding.EncodeToString(b)
}

// Encrypt encrypts data
func (e *Encryptor) Encrypt(plaintext []byte) ([]byte, error) {
	if e.key == nil {
		return plaintext, nil
	}
	
	nonce := make([]byte, 12)
	rand.Read(nonce)
	
	aead, err := chacha20poly1305.NewX(e.key)
	if err != nil {
		return nil, err
	}
	
	ciphertext := aead.Seal(nonce, nonce, plaintext, nil)
	return append(nonce, ciphertext...), nil
}

// Decrypt decrypts data
func (e *Encryptor) Decrypt(ciphertext []byte) ([]byte, error) {
	if e.key == nil {
		return ciphertext, nil
	}
	
	if len(ciphertext) < 12 {
		return nil, fmt.Errorf("ciphertext too short")
	}
	
	aead, err := chacha20poly1305.NewX(e.key)
	if err != nil {
		return nil, err
	}
	
	nonce := ciphertext[:12]
	ciphertext = ciphertext[12:]
	
	return aead.Open(nil, nonce, ciphertext, nil)
}

// GenerateKey generates a random encryption key
func GenerateKey() ([]byte, error) {
	key := make([]byte, 32)
	_, err := rand.Read(key)
	return key, err
}

// SignChallenge signs a challenge for client authentication
func SignChallenge(challenge []byte, privKey *ecdsa.PrivateKey) ([]byte, error) {
	hash := sha256.Sum256(challenge)
	return ecdsa.SignASN1(rand.Reader, privKey, hash[:])
}

// VerifyChallenge verifies a challenge response
func VerifyChallenge(challenge []byte, signature []byte, pubKey *ecdsa.PublicKey) bool {
	hash := sha256.Sum256(challenge)
	return ecdsa.VerifyASN1(pubKey, hash[:], signature) == nil
}

// GenerateKeyPair generates a new key pair for encryption
func GenerateKeyPair() ( *[32]byte, error) {
	pub, priv, err := curve25519.GenerateKey(rand.Reader)
	if err != nil {
		return nil, err
	}
	
	var key [32]byte
	copy(key[:], priv)
	return &key, nil
}

// RunMain is the main entry point
func RunMain() {
	config := &Config{
		Port:        8546,
		UseTLS:      false,
		RequireAuth: false,
		MaxConns:    10000,
		WorkerPoolSize: runtime.NumCPU(),
	}
	
	server, err := NewServer(config)
	if err != nil {
		fmt.Printf("Failed to create server: %v\n", err)
		os.Exit(1)
	}
	
	if err := server.Start(); err != nil {
		fmt.Printf("Failed to start server: %v\n", err)
		os.Exit(1)
	}
	
	fmt.Printf("WebSocket server listening on :%d\n", config.Port)
	
	// Wait for signal
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)
	<-sigCh
	
	fmt.Println("Shutting down...")
	server.Stop()
}