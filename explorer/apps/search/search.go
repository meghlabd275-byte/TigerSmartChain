// Package search provides search functionality for TigerScan Explorer.
package search

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"sync"
)

// Server provides search API.
type Server struct {
	mu sync.RWMutex
	db Database
}

type Database interface {
	SearchBlocks(query string) ([]Block, error)
	SearchTransactions(query string) ([]Transaction, error)
	SearchTokens(query string) ([]Token, error)
	SearchAddresses(query string) ([]Address, error)
}

type Block struct {
	Number    uint64
	Hash     string
	TxCount  int
}

type Transaction struct {
	Hash    string
	From   string
	To     string
	Value  string
}

type Token struct {
	Address string
	Name   string
	Symbol string
}

type Address struct {
	Address string
	Balance string
	Type   string
}

func NewServer() *Server {
	return &Server{}
}

func (s *Server) SetDB(db Database) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.db = db
}

func (s *Server) HandleSearch(w http.ResponseWriter, r *http.Request) {
	query := r.URL.Query().Get("q")
	if query == "" {
		http.Error(w, "missing query", http.StatusBadRequest)
		return
	}

	results := s.search(query)
	json.NewEncoder(w).Encode(results)
}

func (s *Server) search(query string) SearchResults {
	s.mu.RLock()
	defer s.mu.RUnlock()

	results := SearchResults{}

	// Determine search type based on query format
	if strings.HasPrefix(query, "0x") && len(query) == 66 {
		// Transaction hash
		txs, _ := s.db.SearchTransactions(query)
		results.Transactions = txs
	} else if strings.HasPrefix(query, "0x") && len(query) == 42 {
		// Address or token
		tokens, _ := s.db.SearchTokens(query)
		results.Tokens = tokens
		addrs, _ := s.db.SearchAddresses(query)
		results.Addresses = addrs
	} else if strings.HasPrefix(query, "0x") && len(query) == 64 {
		// Block hash
		blocks, _ := s.db.SearchBlocks(query)
		results.Blocks = blocks
	} else {
		// Search all
		blocks, _ := s.db.SearchBlocks(query)
		results.Blocks = blocks
		txs, _ := s.db.SearchTransactions(query)
		results.Transactions = txs
		tokens, _ := s.db.SearchTokens(query)
		results.Tokens = tokens
	}

	return results
}

type SearchResults struct {
	Blocks      []Block      `json:"blocks"`
	Transactions []Transaction `json:"transactions"`
	Tokens     []Token     `json:"tokens"`
	Addresses  []Address   `json:"addresses"`
}

func (s *Server) HandleAutoComplete(w http.ResponseWriter, r *http.Request) {
	query := r.URL.Query().Get("q")
	if len(query) < 2 {
		json.NewEncoder(w).Encode([]string{})
		return
	}

	suggestions := []string{
		fmt.Sprintf("%s (Block)", query),
		fmt.Sprintf("%s (Transaction)", query),
	}
	json.NewEncoder(w).Encode(suggestions)
}
