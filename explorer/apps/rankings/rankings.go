// Package rankings provides rankings for TigerScan Explorer.
package rankings

import (
	"encoding/json"
	"net/http"
	"sort"
	"sync"
)

// Server provides rankings API.
type Server struct {
	mu sync.RWMutex
	db Database
}

type Database interface {
	GetTopAddresses() ([]Address, error)
	GetTopTokens() ([]Token, error)
	GetTopNFTs() ([]NFT, error)
	GetTopValidators() ([]Validator, error)
}

type Address struct {
	Rank      int
	Address  string
	Balance  string
	TxCount int
}

type Token struct {
	Rank       int
	Address   string
	Name      string
	Symbol    string
	Holders  int
	Volume24h string
}

type NFT struct {
	Rank     int
	Address string
	Name    string
	Volume  string
	Sales   int
}

type Validator struct {
	Rank        int
	Address    string
	Name       string
	Uptime    float64
	Delegators int
	Stake     string
}

func NewServer() *Server {
	return &Server{}
}

func (s *Server) SetDB(db Database) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.db = db
}

func (s *Server) HandleTopAddresses(w http.ResponseWriter, r *http.Request) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	addresses, _ := s.db.GetTopAddresses()
	sort.Slice(addresses, func(i, j int) bool {
		return addresses[i].TxCount > addresses[j].TxCount
	})
	for i := range addresses {
		addresses[i].Rank = i + 1
	}
	json.NewEncoder(w).Encode(addresses)
}

func (s *Server) HandleTopTokens(w http.ResponseWriter, r *http.Request) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	tokens, _ := s.db.GetTopTokens()
	sort.Slice(tokens, func(i, j int) bool {
		return tokens[i].Holders > tokens[j].Holders
	})
	for i := range tokens {
		tokens[i].Rank = i + 1
	}
	json.NewEncoder(w).Encode(tokens)
}

func (s *Server) HandleTopNFTs(w http.ResponseWriter, r *http.Request) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	nfts, _ := s.db.GetTopNFTs()
	sort.Slice(nfts, func(i, j int) bool {
		return nfts[i].Sales > nfts[j].Sales
	})
	for i := range nfts {
		nfts[i].Rank = i + 1
	}
	json.NewEncoder(w).Encode(nfts)
}

func (s *Server) HandleTopValidators(w http.ResponseWriter, r *http.Request) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	validators, _ := s.db.GetTopValidators()
	sort.Slice(validators, func(i, j int) bool {
		return validators[i].Uptime > validators[j].Uptime
	})
	for i := range validators {
		validators[i].Rank = i + 1
	}
	json.NewEncoder(w).Encode(validators)
}

func (s *Server) HandleRichList(w http.ResponseWriter, r *http.Request) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	addresses, _ := s.db.GetTopAddresses()
	sort.Slice(addresses, func(i, j int) bool {
		return addresses[i].Balance > addresses[j].Balance
	})
	for i := range addresses {
		addresses[i].Rank = i + 1
	}
	json.NewEncoder(w).Encode(addresses)
}
