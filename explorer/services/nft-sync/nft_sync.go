// Package nftsync provides NFT synchronization for TigerScan Explorer.
package nftsync

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"
	"time"
)

// =============================================================================
// NFT SYNC SERVICE
// =============================================================================

// Service provides NFT synchronization for the explorer.
type Service struct {
	mu sync.RWMutex
	ctx context.Context
	cancel context.CancelFunc

	config *Config
	store storage.Store
	stats *Stats

	onNFTUpdate func(*NFT) error
}

type Config struct {
	RPCURL      string
	PollInterval time.Duration
	BatchSize   int
}

type Stats struct {
	CollectionsProcessed uint64
	NFTsIndexed      uint64
	TransfersIndexed uint64
	Errors           uint64
}

type Collection struct {
	Address     string
	Name       string
	Symbol     string
	TotalSupply uint64
	Type       string // TEP721 or TEP1155
	Creator    string
	RoyaltyBPS uint16
	Website    string
	Description string
	LastUpdated time.Time
}

type NFT struct {
	TokenID    string
	Collection string
	Owner     string
	URI       string
	Metadata  string
	Creator   string
	BlockNumber uint64
	LastUpdated time.Time
}

type Transfer struct {
	TokenID    string
	Collection string
	From      string
	To        string
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

func (s *Service) OnNFTUpdate(handler func(*NFT) error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.onNFTUpdate = handler
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
			s.processCollections()
		}
	}
}

func (s *Service) processCollections() {
	s.mu.Lock()
	s.stats.CollectionsProcessed++
	s.mu.Unlock()
}

func (s *Service) StoreCollection(col *Collection) error {
	if s.store == nil {
		return fmt.Errorf("no store configured")
	}
	data, _ := json.Marshal(col)
	return s.store.Put([]byte("collection:"+col.Address), data)
}

func (s *Service) GetCollection(address string) (*Collection, error) {
	if s.store == nil {
		return nil, fmt.Errorf("no store configured")
	}
	data, err := s.store.Get([]byte("collection:" + address))
	if err != nil {
		return nil, err
	}
	col := &Collection{}
	json.Unmarshal(data, col)
	return col, nil
}

func (s *Service) StoreNFT(nft *NFT) error {
	if s.store == nil {
		return fmt.Errorf("no store configured")
	}
	key := fmt.Sprintf("nft:%s:%s", nft.Collection, nft.TokenID)
	data, _ := json.Marshal(nft)
	return s.store.Put([]byte(key), data)
}

func (s *Service) GetNFT(collection, tokenID string) (*NFT, error) {
	if s.store == nil {
		return nil, fmt.Errorf("no store configured")
	}
	key := fmt.Sprintf("nft:%s:%s", collection, tokenID)
	data, err := s.store.Get([]byte(key))
	if err != nil {
		return nil, err
	}
	nft := &NFT{}
	json.Unmarshal(data, nft)
	return nft, nil
}

func (s *Service) StoreTransfer(transfer *Transfer) error {
	if s.store == nil {
		return nil
	}
	data, _ := json.Marshal(transfer)
	return s.store.Put([]byte("nft_transfer:"+transfer.TxHash), data)
}

func (s *Service) GetTransfersByCollection(collection string) ([]*Transfer, error) {
	return []*Transfer{}, nil
}

func (s *Service) GetNFTsByOwner(owner string) ([]*NFT, error) {
	return []*NFT{}, nil
}

func (s *Service) GetFloorPrice(collection string) (uint64, error) {
	return 0, nil
}

func (s *Service) GetStats() *Stats {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.stats
}
