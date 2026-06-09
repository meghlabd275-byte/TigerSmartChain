// Package tokensync provides token synchronization for TigerScan Explorer.
package tokensync

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"
	"time"
)

// =============================================================================
// TOKEN SYNC SERVICE
// =============================================================================

// Service provides token synchronization for the explorer.
type Service struct {
	mu sync.RWMutex
	ctx context.Context
	cancel context.CancelFunc

	config *Config
	store storage.Store
	stats *Stats

	onTokenUpdate func(*Token) error
}

type Config struct {
	RPCURL      string
	PollInterval time.Duration
	BatchSize   int
}

type Stats struct {
	TokensProcessed   uint64
	TransfersIndexed uint64
	Errors           uint64
}

type Token struct {
	Address      string
	Name         string
	Symbol       string
	Decimals     uint8
	TotalSupply  uint64
	Type         string
	HoldersCount uint64
	TransfersCount uint64
	Price        float64
	MarketCap    float64
	Volume24h    float64
	LastUpdated  time.Time
}

type Transfer struct {
	Token     string
	From      string
	To        string
	Value     uint64
	Block     uint64
	Timestamp time.Time
	TxHash    string
}

type storage interface {
	Put(key, value []byte) error
	Get(key []byte) ([]byte, error)
	Delete(key []byte) error
}

func NewService(config *Config) *Service {
	ctx, cancel := context.WithCancel(context.Background())
	return &Service{
		ctx: ctx,
		cancel: cancel,
		config: config,
		stats: &Stats{},
	}
}

func (s *Service) SetStore(store storage.Store) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.store = store
}

func (s *Service) OnTokenUpdate(handler func(*Token) error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.onTokenUpdate = handler
}

func (s *Service) Start() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	go s.syncLoop()
	return nil
}

func (s *Service) Stop() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.cancel()
	return nil
}

func (s *Service) syncLoop() {
	ticker := time.NewTicker(s.config.PollInterval)
	defer ticker.Stop()
	for {
		select {
		case <-s.ctx.Done():
			return
		case <-ticker.C:
			s.processTokens()
		}
	}
}

func (s *Service) processTokens() {
	// Process tokens
	s.mu.Lock()
	s.stats.TokensProcessed++
	s.mu.Unlock()
}

func (s *Service) StoreToken(token *Token) error {
	if s.store == nil {
		return fmt.Errorf("no store configured")
	}
	data, _ := json.Marshal(token)
	return s.store.Put([]byte("token:"+token.Address), data)
}

func (s *Service) GetToken(address string) (*Token, error) {
	if s.store == nil {
		return nil, fmt.Errorf("no store configured")
	}
	data, err := s.store.Get([]byte("token:" + address))
	if err != nil {
		return nil, err
	}
	token := &Token{}
	json.Unmarshal(data, token)
	return token, nil
}

func (s *Service) StoreTransfer(transfer *Transfer) error {
	if s.store == nil {
		return nil
	}
	data, _ := json.Marshal(transfer)
	return s.store.Put([]byte("transfer:"+transfer.TxHash), data)
}

func (s *Service) GetTransfersByToken(token string) ([]*Transfer, error) {
	// In production, query database
	return []*Transfer{}, nil
}

func (s *Service) GetHolders(token string) ([]string, error) {
	// In production, query database
	return []string{}, nil
}

func (s *Service) GetStats() *Stats {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.stats
}
