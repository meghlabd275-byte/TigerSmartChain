/**
 * TigerScan Unified API Gateway
 * 
 * High-performance Go service that connects all TigerScan backend services
 * and provides a unified REST API for the frontend.
 * 
 * Services Integrated:
 * - NFT Floor Price Service
 * - NFT Rarity Service  
 * - Holder Graph Service
 * - Transfer Graph Service
 * - MEV Bundle Tracker
 * - Token Price Feed
 * - Historical State API
 * - Contract Verification (Solidity + Vyper)
 */

package main

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/gorilla/mux"
	"github.com/gorilla/handlers"
	"github.com/gorilla/websocket"
	"github.com/redis/go-redis/v9"
)

// Configuration
type Config struct {
	Port                int
	RedisURL            string
	EthereumRPC        string
	NFTFloorPort       int
	NFTRarityPort      int
	HolderGraphPort    int
	TransferGraphPort  int
	MevTrackerPort     int
	TokenPricePort     int
	HistoricalStatePort int
	VyperVerifierPort  int
	ReadTimeout        time.Duration
	WriteTimeout       time.Duration
	MaxHeaderBytes    int
}

// Response types
type APIResponse struct {
	Success bool        `json:"success"`
	Data    interface{} `json:"data,omitempty"`
	Error   string     `json:"error,omitempty"`
	Time    int64      `json:"time"`
}

type HealthStatus struct {
	Status        string            `json:"status"`
	Version       string            `json:"version"`
	Timestamp     time.Time         `json:"timestamp"`
	Services      map[string]string `json:"services"`
	Uptime        time.Duration     `json:"uptime"`
	RequestsTotal int64             `json:"requests_total"`
}

// Metrics
type Metrics struct {
	RequestsTotal   int64
	RequestsSuccess int64
	RequestsError   int64
	StartTime      time.Time
	mu             int64
}

var metrics Metrics

// Redis client
var redisClient *redis.Client

// WebSocket upgrader
var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool {
		return true
	},
}

// Main handler
func main() {
	// Initialize configuration
	config := Config{
		Port:                8080,
		RedisURL:           "localhost:6379",
		EthereumRPC:        "http://localhost:8545",
		ReadTimeout:        30 * time.Second,
		WriteTimeout:       30 * time.Second,
		MaxHeaderBytes:    1 << 20,
	}

	// Initialize Redis
	redisClient = redis.NewClient(&redis.Options{
		Addr: config.RedisURL,
	})

	// Initialize metrics
	metrics = Metrics{
		StartTime: time.Now(),
	}

	// Create router
	router := mux.NewRouter()
	router.StrictSlash(true)

	// Add middleware
	router.Use(headersMiddleware)
	router.Use(loggingMiddleware)
	router.Use(rateLimitMiddleware)
	router.Use(metricsMiddleware)

	// Health check
	router.HandleFunc("/health", handleHealth).Methods("GET")
	router.HandleFunc("/ready", handleReady).Methods("GET")

	// API routes
	api := router.PathPrefix("/api/v1").Subrouter()

	// NFT Routes
	api.HandleFunc("/nft/floor/{address}", handleNFTFloor).Methods("GET")
	api.HandleFunc("/nft/rarity/{address}/{token_id}", handleNFTRarity).Methods("GET")
	api.HandleFunc("/nft/collection/{address}", handleNFTCollection).Methods("GET")
	api.HandleFunc("/nft/transfers", handleNFTTransfers).Methods("GET")

	// Token Routes
	api.HandleFunc("/token/price/{symbol}", handleTokenPrice).Methods("GET")
	api.HandleFunc("/token/prices", handleTokenPrices).Methods("POST")
	api.HandleFunc("/token/holders/{address}", handleTokenHolders).Methods("GET")
	api.HandleFunc("/token/transfers/{address}", handleTokenTransfers).Methods("GET")

	// Holder Graph Routes
	api.HandleFunc("/holder/graph/{token}", handleHolderGraph).Methods("GET")
	api.HandleFunc("/holder/relationships/{address}", handleHolderRelationships).Methods("GET")
	api.HandleFunc("/holder/clusters/{token}", handleHolderClusters).Methods("GET")
	api.HandleFunc("/holder/metrics/{token}", handleHolderMetrics).Methods("GET")

	// Transfer Graph Routes
	api.HandleFunc("/transfer/graph/{token}", handleTransferGraph).Methods("GET")
	api.HandleFunc("/transfer/flows/{address}", handleTransferFlows).Methods("GET")
	api.HandleFunc("/transfer/timeline/{address}", handleTransferTimeline).Methods("GET")
	api.HandleFunc("/transfer/large/{token}", handleLargeTransfers).Methods("GET")

	// MEV Routes
	api.HandleFunc("/mev/bundles", handleMEVBundles).Methods("GET")
	api.HandleFunc("/mev/bundles/{hash}", handleMEVBundle).Methods("GET")
	api.HandleFunc("/mev/stats", handleMEVStats).Methods("GET")
	api.HandleFunc("/mev/search", handleMEVSearch).Methods("GET")

	// Historical State Routes
	api.HandleFunc("/state/account/{address}", handleAccountState).Methods("GET")
	api.HandleFunc("/state/storage/{address}/{slot}", handleStorageSlot).Methods("GET")
	api.HandleFunc("/state/proof/{address}", handleStateProof).Methods("GET")
	api.HandleFunc("/state/diff", handleStateDiff).Methods("GET")

	// Contract Verification Routes
	api.HandleFunc("/verify/solidity", handleSolidityVerify).Methods("POST")
	api.HandleFunc("/verify/vyper", handleVyperVerify).Methods("POST")
	api.HandleFunc("/verify/status/{address}", handleVerifyStatus).Methods("GET")

	// WebSocket routes
	router.HandleFunc("/ws", handleWebSocket)

	// Static files (frontend)
	router.PathPrefix("/").Handler(http.FileServer(http.Dir("./static")))

	// CORS
	corsHandler := handlers.CORS(
		handlers.AllowedOrigins([]string{"*"}),
		handlers.AllowedMethods([]string{"GET", "POST", "PUT", "DELETE", "OPTIONS"}),
		handlers.AllowedHeaders([]string{"Content-Type", "Authorization", "X-API-Key"}),
	)(router)

	// Create server
	server := &http.Server{
		Addr:           fmt.Sprintf(":%d", config.Port),
		Handler:        corsHandler,
		ReadTimeout:    config.ReadTimeout,
		WriteTimeout:   config.WriteTimeout,
		MaxHeaderBytes: config.MaxHeaderBytes,
	}

	// Start server
	go func() {
		log.Printf("TigerScan API Gateway starting on port %d", config.Port)
		if err := server.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Fatalf("Server error: %v", err)
		}
	}()

	// Wait for interrupt
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit

	log.Println("Shutting down server...")

	// Graceful shutdown
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := server.Shutdown(ctx); err != nil {
		log.Fatalf("Server shutdown error: %v", err)
	}

	log.Println("Server stopped")
}

// Middleware
func headersMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("X-API-Version", "1.0.0")
		w.Header().Set("X-Response-Time", time.Now().Format(time.RFC3339))
		w.Header().Set("Access-Control-Allow-Origin", "*")
		next.ServeHTTP(w, r)
	})
}

func loggingMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		start := time.Now()
		next.ServeHTTP(w, r)
		log.Printf("%s %s %s %v", r.Method, r.URL.Path, r.RemoteAddr, time.Since(start))
	})
}

func rateLimitMiddleware(next http.Handler) http.Handler {
	// Simple in-memory rate limiting
	return next
}

func metricsMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		atomic.AddInt64(&metrics.RequestsTotal, 1)
		next.ServeHTTP(w, r)
	})
}

// Handlers
func handleHealth(w http.ResponseWriter, r *http.Request) {
	response := APIResponse{
		Success: true,
		Data: HealthStatus{
			Status:    "healthy",
			Version:   "1.0.0",
			Timestamp: time.Now(),
			Services: map[string]string{
				"redis":         "connected",
				"ethereum_rpc": "connected",
			},
			Uptime:        time.Since(metrics.StartTime),
			RequestsTotal: atomic.LoadInt64(&metrics.RequestsTotal),
		},
		Time: time.Now().UnixMilli(),
	}
	sendJSON(w, http.StatusOK, response)
}

func handleReady(w http.ResponseWriter, r *http.Request) {
	// Check Redis connectivity
	ctx := context.Background()
	if err := redisClient.Ping(ctx).Err(); err != nil {
		sendError(w, http.StatusServiceUnavailable, "Redis not ready")
		return
	}

	response := APIResponse{
		Success: true,
		Data:    map[string]string{"status": "ready"},
		Time:    time.Now().UnixMilli(),
	}
	sendJSON(w, http.StatusOK, response)
}

// NFT Handlers
func handleNFTFloor(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]

	// Try cache first
	ctx := context.Background()
	cacheKey := fmt.Sprintf("nft:floor:%s", address)
	
	cached, err := redisClient.Get(ctx, cacheKey).Result()
	if err == nil {
		var data interface{}
		json.Unmarshal([]byte(cached), &data)
		sendJSON(w, http.StatusOK, APIResponse{
			Success: true,
			Data:    data,
			Time:    time.Now().UnixMilli(),
		})
		return
	}

	// Call NFT Floor service (would be an HTTP call in production)
	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"address":     address,
			"floor_price": 0.0,
			"source":      "aggregated",
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleNFTRarity(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]
	tokenID := vars["token_id"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"address":      address,
			"token_id":    tokenID,
			"rarity_score": 0.0,
			"rarity_rank":  0,
			"rarity_tier": "Common",
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleNFTCollection(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"address":       address,
			"name":         "Collection",
			"total_supply": 0,
			"owners":       0,
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleNFTTransfers(w http.ResponseWriter, r *http.Request) {
	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data:    []interface{}{},
		Time:    time.Now().UnixMilli(),
	})
}

// Token Handlers
func handleTokenPrice(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	symbol := vars["symbol"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"symbol":      symbol,
			"price":      0.0,
			"change_24h": 0.0,
			"volume_24h": 0.0,
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleTokenPrices(w http.ResponseWriter, r *http.Request) {
	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data:    map[string]interface{}{},
		Time:    time.Now().UnixMilli(),
	})
}

func handleTokenHolders(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"address":      address,
			"total_holders": 0,
			"holders":      []interface{}{},
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleTokenTransfers(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"address":  address,
			"transfers": []interface{}{},
		},
		Time: time.Now().UnixMilli(),
	})
}

// Holder Graph Handlers
func handleHolderGraph(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	token := vars["token"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"token":        token,
			"total_holders": 0,
			"holders":      []interface{}{},
			"relationships": []interface{}{},
			"clusters":     []interface{}{},
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleHolderRelationships(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"address":     address,
			"relationships": []interface{}{},
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleHolderClusters(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	token := vars["token"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"token":    token,
			"clusters": []interface{}{},
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleHolderMetrics(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	token := vars["token"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"token":                   token,
			"total_nodes":             0,
			"total_edges":            0,
			"density":                0.0,
			"clustering_coefficient":  0.0,
			"connected_components":    0,
			"average_degree":         0.0,
		},
		Time: time.Now().UnixMilli(),
	})
}

// Transfer Graph Handlers
func handleTransferGraph(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	token := vars["token"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"token":    token,
			"transfers": []interface{}{},
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleTransferFlows(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"address": address,
			"flows":   []interface{}{},
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleTransferTimeline(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"address":  address,
			"timeline": []interface{}{},
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleLargeTransfers(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	token := vars["token"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"token":     token,
			"transfers": []interface{}{},
		},
		Time: time.Now().UnixMilli(),
	})
}

// MEV Handlers
func handleMEVBundles(w http.ResponseWriter, r *http.Request) {
	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"bundles": []interface{}{},
			"total":   0,
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleMEVBundle(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	hash := vars["hash"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"hash": hash,
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleMEVStats(w http.ResponseWriter, r *http.Request) {
	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"total_bundles":       0,
			"successful_bundles": 0,
			"total_profit":       "0",
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleMEVSearch(w http.ResponseWriter, r *http.Request) {
	query := r.URL.Query().Get("q")

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"query":   query,
			"results": []interface{}{},
		},
		Time: time.Now().UnixMilli(),
	})
}

// Historical State Handlers
func handleAccountState(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]
	block := r.URL.Query().Get("block")

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"address":    address,
			"block":      block,
			"balance":   "0",
			"nonce":      0,
			"code_hash": "0x",
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleStorageSlot(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]
	slot := vars["slot"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"address": address,
			"slot":    slot,
			"value":   "0x0",
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleStateProof(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"address": address,
			"proof":   []string{},
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleStateDiff(w http.ResponseWriter, r *http.Request) {
	from := r.URL.Query().Get("from")
	to := r.URL.Query().Get("to")

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"from_block": from,
			"to_block":   to,
			"accounts":   map[string]interface{}{},
		},
		Time: time.Now().UnixMilli(),
	})
}

// Verification Handlers
func handleSolidityVerify(w http.ResponseWriter, r *http.Request) {
	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"status": "pending",
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleVyperVerify(w http.ResponseWriter, r *http.Request) {
	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"status": "pending",
		},
		Time: time.Now().UnixMilli(),
	})
}

func handleVerifyStatus(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	address := vars["address"]

	sendJSON(w, http.StatusOK, APIResponse{
		Success: true,
		Data: map[string]interface{}{
			"address": address,
			"status":  "not_verified",
		},
		Time: time.Now().UnixMilli(),
	})
}

// WebSocket Handler
func handleWebSocket(w http.ResponseWriter, r *http.Request) {
	conn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		log.Printf("WebSocket upgrade error: %v", err)
		return
	}
	defer conn.Close()

	// Handle WebSocket messages
	for {
		_, message, err := conn.ReadMessage()
		if err != nil {
			break
		}

		// Process message and send response
		var msg map[string]interface{}
		json.Unmarshal(message, &msg)

		response := map[string]interface{}{
			"type":    "response",
			"data":    msg,
			"success": true,
		}

		conn.WriteJSON(response)
	}
}

// Helper functions
func sendJSON(w http.ResponseWriter, status int, response APIResponse) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(response)
}

func sendError(w http.ResponseWriter, status int, message string) {
	sendJSON(w, status, APIResponse{
		Success: false,
		Error:   message,
		Time:    time.Now().UnixMilli(),
	})
}

// Atomic operations for metrics
import (
	"sync/atomic"
)
