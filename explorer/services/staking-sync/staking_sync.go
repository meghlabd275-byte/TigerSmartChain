// Package stakingsync provides staking synchronization for TigerScan Explorer.
package stakingsync

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"
	"time"
)

// Service provides staking data synchronization for the explorer.
type Service struct {
	mu sync.RWMutex
	ctx context.Context
	cancel context.CancelFunc
	config *Config
	store storage.Store
	stats *Stats
}

type Config struct {
	RPCURL      string
	PollInterval time.Duration
}

type Stats struct {
	StakeUpdates    uint64
	DelegationUpdates uint64
	Errors          uint64
}

type StakingPool struct {
	Address       string
	TotalStaked   uint64
	RewardPool    uint64
	DelegatorCount uint64
	Apr           float64
	LastUpdated   time.Time
}

type Delegation struct {
	Delegator    string
	Pool        string
	Amount      uint64
	PendingReward uint64
	StartTime   time.Time
}

type storage interface {
	Put(key, value []byte) error
	Get(key []byte) ([]byte, error)
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
			s.processStakingData()
		}
	}
}

func (s *Service) processStakingData() {
	s.mu.Lock()
	s.stats.StakeUpdates++
	s.mu.Unlock()
}

func (s *Service) StorePool(pool *StakingPool) error {
	if s.store == nil {
		return fmt.Errorf("no store configured")
	}
	data, _ := json.Marshal(pool)
	return s.store.Put([]byte("staking_pool:"+pool.Address), data)
}

func (s *Service) GetPool(address string) (*StakingPool, error) {
	if s.store == nil {
		return nil, fmt.Errorf("no store configured")
	}
	data, err := s.store.Get([]byte("staking_pool:" + address))
	if err != nil {
		return nil, err
	}
	pool := &StakingPool{}
	json.Unmarshal(data, pool)
	return pool, nil
}

func (s *Service) StoreDelegation(d *Delegation) error {
	if s.store == nil {
		return nil
	}
	key := fmt.Sprintf("delegation:%s:%s", d.Pool, d.Delegator)
	data, _ := json.Marshal(d)
	return s.store.Put([]byte(key), data)
}

func (s *Service) GetDelegationsByPool(pool string) ([]*Delegation, error) {
	return []*Delegation{}, nil
}

func (s *Service) GetDelegationsByUser(user string) ([]*Delegation, error) {
	return []*Delegation{}, nil
}

func (s *Service) GetStats() *Stats {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.stats
}
