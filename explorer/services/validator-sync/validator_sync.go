// Package validatorsync provides validator synchronization for TigerScan Explorer.
package validatorsync

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"
	"time"
)

// =============================================================================
// VALIDATOR SYNC SERVICE
// =============================================================================

// Service provides validator synchronization for the explorer.
type Service struct {
	mu sync.RWMutex
	ctx context.Context
	cancel context.CancelFunc

	config *Config
	store storage.Store
	stats *Stats

	onValidatorUpdate func(*Validator) error
}

type Config struct {
	RPCURL      string
	PollInterval time.Duration
}

type Stats struct {
	ValidatorsProcessed uint64
	MissedBlocksIndexed uint64
	Errors            uint64
}

type Validator struct {
	Address        string
	Name           string
	Commission     uint8
	SelfStake     uint64
	TotalStake    uint64
	Delegators    uint64
	Uptime        float64
	Status         string
	BlocksProduced uint64
	BlocksMissed  uint64
	LastUpdate    time.Time
}

type ValidatorSnapshot struct {
	BlockNumber uint64
	Validator  string
	Stake      uint64
	Uptime     float64
	Timestamp  time.Time
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

func (s *Service) OnValidatorUpdate(handler func(*Validator) error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.onValidatorUpdate = handler
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
			s.processValidators()
		}
	}
}

func (s *Service) processValidators() {
	s.mu.Lock()
	s.stats.ValidatorsProcessed++
	s.mu.Unlock()
}

func (s *Service) StoreValidator(v *Validator) error {
	if s.store == nil {
		return fmt.Errorf("no store configured")
	}
	data, _ := json.Marshal(v)
	return s.store.Put([]byte("validator:"+v.Address), data)
}

func (s *Service) GetValidator(address string) (*Validator, error) {
	if s.store == nil {
		return nil, fmt.Errorf("no store configured")
	}
	data, err := s.store.Get([]byte("validator:" + address))
	if err != nil {
		return nil, err
	}
	v := &Validator{}
	json.Unmarshal(data, v)
	return v, nil
}

func (s *Service) GetAllValidators() ([]*Validator, error) {
	// In production, query database
	return []*Validator{}, nil
}

func (s *Service) GetActiveValidators() ([]*Validator, error) {
	// In production, filter by status
	return []*Validator{}, nil
}

func (s *Service) GetValidatorSnapshots(address string, fromBlock, toBlock uint64) ([]*ValidatorSnapshot, error) {
	return []*ValidatorSnapshot{}, nil
}

func (s *Service) RecordMissedBlock(validator string, blockNumber uint64) error {
	s.mu.Lock()
	s.stats.MissedBlocksIndexed++
	s.mu.Unlock()

	if s.store == nil {
		return nil
	}

	snapshot := &ValidatorSnapshot{
		BlockNumber: blockNumber,
		Validator:  validator,
		Timestamp:  time.Now(),
	}

	data, _ := json.Marshal(snapshot)
	return s.store.Put([]byte(fmt.Sprintf("missed:%s:%d", validator, blockNumber)), data)
}

func (s *Service) RecordProducedBlock(validator string, blockNumber uint64) error {
	if s.store == nil {
		return nil
	}

	snapshot := &ValidatorSnapshot{
		BlockNumber: blockNumber,
		Validator:  validator,
		Timestamp:  time.Now(),
	}

	data, _ := json.Marshal(snapshot)
	return s.store.Put([]byte(fmt.Sprintf("produced:%s:%d", validator, blockNumber)), data)
}

func (s *Service) CalculateUptime(validator string) (float64, error) {
	// Get snapshots for last 1000 blocks
	// Calculate uptime based on produced vs missed
	return 99.5, nil
}

func (s *Service) GetStats() *Stats {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.stats
}
