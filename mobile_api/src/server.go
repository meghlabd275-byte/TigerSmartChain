// Package mobileapi provides mobile API with optimized endpoints
package mobileapi

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

type Config struct {
	DBURL          string
	RedisURL       string
	Port           string
	JWTSecret      string
}

type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
}

func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, err
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 15})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, err
	}
	return &Server{cfg: cfg, pool: pool, redis: rdb}, nil
}

func (s *Server) Start() error {
	http.HandleFunc("/api/mobile/v1/blocks", s.handleBlocks)
	http.HandleFunc("/api/mobile/v1/transactions", s.handleTransactions)
	http.HandleFunc("/api/mobile/v1/tokens", s.handleTokens)
	http.HandleFunc("/api/mobile/v1/account", s.handleAccount)
	http.HandleFunc("/api/mobile/v1/search", s.handleSearch)
	http.HandleFunc("/api/mobile/v1/stats", s.handleStats)
	return http.ListenAndServe(":"+s.cfg.Port, nil)
}

func (s *Server) handleBlocks(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	limit := r.URL.Query().Get("limit")
	if limit == "" {
		limit = "20"
	}
	
	rows, err := s.pool.Query(ctx, "SELECT number, hash, timestamp, tx_count, gas_used FROM blocks ORDER BY number DESC LIMIT $1", limit)
	if err != nil {
		http.Error(w, err.Error(), 500)
		return
	}
	defer rows.Close()
	
	type Block struct {
		Number    int    `json:"number"`
		Hash     string `json:"hash"`
		Timestamp int64 `json:"timestamp"`
		TxCount  int    `json:"txCount"`
		GasUsed  int64  `json:"gasUsed"`
	}
	
	var blocks []Block
	for rows.Next() {
		var b Block
		rows.Scan(&b.Number, &b.Hash, &b.Timestamp, &b.TxCount, &b.GasUsed)
		blocks = append(blocks, b)
	}
	
	json.NewEncoder(w).Encode(blocks)
}

func (s *Server) handleTransactions(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	limit := r.URL.Query().Get("limit")
	if limit == "" {
		limit = "20"
	}
	
	rows, err := s.pool.Query(ctx, "SELECT hash, from_address, to_address, value, gas_price, status, timestamp FROM transactions ORDER BY timestamp DESC LIMIT $1", limit)
	if err != nil {
		http.Error(w, err.Error(), 500)
		return
	}
	defer rows.Close()
	
	type Transaction struct {
		Hash      string `json:"hash"`
		From     string `json:"from"`
		To       string `json:"to"`
		Value    string `json:"value"`
		GasPrice int64  `json:"gasPrice"`
		Status   int    `json:"status"`
		Timestamp int64 `json:"timestamp"`
	}
	
	var txs []Transaction
	for rows.Next() {
		var t Transaction
		rows.Scan(&t.Hash, &t.From, &t.To, &t.Value, &t.GasPrice, &t.Status, &t.Timestamp)
		txs = append(txs, t)
	}
	
	json.NewEncoder(w).Encode(txs)
}

func (s *Server) handleTokens(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	limit := r.URL.Query().Get("limit")
	if limit == "" {
		limit = "20"
	}
	
	rows, err := s.pool.Query(ctx, "SELECT address, name, symbol, price FROM tokens WHERE is_active = true ORDER BY transfers_count DESC LIMIT $1", limit)
	if err != nil {
		http.Error(w, err.Error(), 500)
		return
	}
	defer rows.Close()
	
	type Token struct {
		Address string  `json:"address"`
		Name    string  `json:"name"`
		Symbol  string  `json:"symbol"`
		Price   float64 `json:"price"`
	}
	
	var tokens []Token
	for rows.Next() {
		var t Token
		rows.Scan(&t.Address, &t.Name, &t.Symbol, &t.Price)
		tokens = append(tokens, t)
	}
	
	json.NewEncoder(w).Encode(tokens)
}

func (s *Server) handleAccount(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	address := r.URL.Query().Get("address")
	if address == "" {
		http.Error(w, "address required", 400)
		return
	}
	
	type Account struct {
		Address   string `json:"address"`
		Balance  string `json:"balance"`
		Nonce    int    `json:"nonce"`
		IsContract bool  `json:"isContract"`
	}
	
	var acc Account
	err := s.pool.QueryRow(ctx, "SELECT address, balance, nonce, is_contract FROM accounts WHERE address = $1", address).Scan(&acc.Address, &acc.Balance, &acc.Nonce, &acc.IsContract)
	if err != nil {
		http.Error(w, "account not found", 404)
		return
	}
	
	json.NewEncoder(w).Encode(acc)
}

func (s *Server) handleSearch(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	query := r.URL.Query().Get("q")
	if query == "" {
		http.Error(w, "query required", 400)
		return
	}
	
	type SearchResult struct {
		Type    string `json:"type"`
		Value   string `json:"value"`
	}
	
	var results []SearchResult
	
	// Check if transaction
	var txCount int
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM transactions WHERE hash = $1", query).Scan(&txCount)
	if txCount > 0 {
		results = append(results, SearchResult{Type: "transaction", Value: query})
	}
	
	// Check if block
	var blockCount int
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM blocks WHERE hash = $1 OR number = $1", query).Scan(&blockCount)
	if blockCount > 0 {
		results = append(results, SearchResult{Type: "block", Value: query})
	}
	
	// Check if address
	var addressCount int
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM accounts WHERE address = $1", query).Scan(&addressCount)
	if addressCount > 0 {
		results = append(results, SearchResult{Type: "address", Value: query})
	}
	
	json.NewEncoder(w).Encode(results)
}

func (s *Server) handleStats(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	
	type Stats struct {
		TotalBlocks      int     `json:"totalBlocks"`
		TotalTransactions int     `json:"totalTransactions"`
		TotalAddresses  int     `json:"totalAddresses"`
		TPS             float64 `json:"tps"`
		GasPrice        int64   `json:"gasPrice"`
	}
	
	var stats Stats
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM blocks").Scan(&stats.TotalBlocks)
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM transactions").Scan(&stats.TotalTransactions)
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM accounts").Scan(&stats.TotalAddresses)
	s.pool.QueryRow(ctx, "SELECT AVG(gas_price) FROM transactions WHERE timestamp > $1", time.Now().Unix()-3600).Scan(&stats.GasPrice)
	
	var txsLastHour int
	s.pool.QueryRow(ctx, "SELECT COUNT(*) FROM transactions WHERE timestamp > $1", time.Now().Unix()-3600).Scan(&txsLastHour)
	stats.TPS = float64(txsLastHour) / 3600.0
	
	json.NewEncoder(w).Encode(stats)
}