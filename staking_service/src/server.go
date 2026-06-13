// Package staking provides staking service with rewards tracking
// Built with Go for high performance
package staking

import (
	"context"
	"fmt"
	"math"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds staking service configuration
type Config struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
	MinStakeAmount float64
	LockPeriod     time.Duration
}

// StakingPool represents a staking pool
type StakingPool struct {
	Address         string  `json:"address"`
	Name           string  `json:"name"`
	Protocol       string  `json:"protocol"`
	TotalStaked   float64 `json:"totalStaked"`
	TotalDelegators int   `json:"totalDelegators"`
	APR           float64 `json:"apr"`
	MinStake      float64 `json:"minStake"`
	LockPeriod     int     `json:"lockPeriodDays"`
	RewardToken    string  `json:"rewardToken"`
	IsActive       bool    `json:"isActive"`
}

// Delegation represents a delegation
type Delegation struct {
	Delegator     string  `json:"delegator"`
	Pool          string  `json:"pool"`
	Amount        float64 `json:"amount"`
	Rewards       float64 `json:"rewards"`
	PendingRewards float64 `json:"pendingRewards"`
	LockedUntil  *time.Time `json:"lockedUntil,omitempty"`
	CreatedAt     time.Time `json:"createdAt"`
}

// StakingReward represents staking rewards
type StakingReward struct {
	Delegator    string    `json:"delegator"`
	Pool        string    `json:"pool"`
	Amount      float64   `json:"amount"`
	BlockNumber int64     `json:"blockNumber"`
	Timestamp   time.Time `json:"timestamp"`
}

// Server represents the staking service server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
}

// NewServer creates a new staking service server
func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	dbpool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 10})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	if err := createTables(ctx, dbpool); err != nil {
		return nil, fmt.Errorf("failed to create tables: %w", err)
	}
	srv := &Server{cfg: cfg, pool: dbpool, redis: rdb}
	go srv.startUpdater()
	return srv, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) error {
	queries := []string{
		`CREATE TABLE IF NOT EXISTS staking_pools (id SERIAL PRIMARY KEY, address VARCHAR(42) NOT NULL UNIQUE, name VARCHAR(255) NOT NULL, protocol VARCHAR(50), total_staked DECIMAL(30,8) DEFAULT 0, total_delegators INTEGER DEFAULT 0, apr DECIMAL(10,4) DEFAULT 0, min_stake DECIMAL(30,8), lock_period_days INTEGER DEFAULT 0, reward_token VARCHAR(42), is_active BOOLEAN DEFAULT TRUE, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`,
		`CREATE TABLE IF NOT EXISTS delegations (id SERIAL PRIMARY KEY, delegator VARCHAR(42) NOT NULL, pool_address VARCHAR(42) NOT NULL, amount DECIMAL(30,8) NOT NULL, rewards DECIMAL(30,8) DEFAULT 0, pending_rewards DECIMAL(30,8) DEFAULT 0, locked_until TIMESTAMP, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, UNIQUE(delegator, pool_address))`,
		`CREATE TABLE IF NOT EXISTS staking_rewards (id SERIAL PRIMARY KEY, delegator VARCHAR(42) NOT NULL, pool_address VARCHAR(42) NOT NULL, amount DECIMAL(30,8) NOT NULL, block_number BIGINT NOT NULL, timestamp BIGINT NOT NULL)`,
		`CREATE INDEX IF NOT EXISTS idx_delegations_delegator ON delegations(delegator)`,
		`CREATE INDEX IF NOT EXISTS idx_delegations_pool ON delegations(pool_address)`,
		`CREATE INDEX IF NOT EXISTS idx_staking_rewards_delegator ON staking_rewards(delegator)`,
	}
	for _, q := range queries {
		if _, err := pool.Exec(ctx, q); err != nil {
			return err
		}
	}
	return nil
}

func (s *Server) startUpdater() {
	ticker := time.NewTicker(s.cfg.UpdateInterval)
	defer ticker.Stop()
	for range ticker.C {
		if err := s.updateStakingMetrics(); err != nil {
			fmt.Printf("failed to update staking metrics: %v\n", err)
		}
	}
}

func (s *Server) updateStakingMetrics() error {
	ctx := context.Background()
	
	rows, err := s.pool.Query(ctx, `SELECT address FROM staking_pools WHERE is_active = true`)
	if err != nil {
		return err
	}
	defer rows.Close()
	
	var pools []string
	for rows.Next() {
		var addr string
		if err := rows.Scan(&addr); err != nil {
			continue
		}
		pools = append(pools, addr)
	}
	
	for _, poolAddr := range pools {
		if err := s.calculatePoolAPR(ctx, poolAddr); err != nil {
			fmt.Printf("failed to calculate APR for pool %s: %v\n", poolAddr, err)
		}
		if err := s.distributeRewards(ctx, poolAddr); err != nil {
			fmt.Printf("failed to distribute rewards for pool %s: %v\n", poolAddr, err)
		}
	}
	
	return nil
}

func (s *Server) calculatePoolAPR(ctx context.Context, poolAddr string) error {
	// Get pool info
	var totalStaked float64
	err := s.pool.QueryRow(ctx, `SELECT COALESCE(SUM(amount), 0) FROM delegations WHERE pool_address = $1`, poolAddr).Scan(&totalStaked)
	if err != nil {
		return err
	}
	
	// Calculate APR based on rewards (simplified)
	// In production, this would come from actual reward distribution
	apr := 0.0
	if totalStaked > 0 {
		// Mock APR calculation
		apr = 12.5 + (totalStaked / 1e6) * 0.5
		if apr > 25 {
			apr = 25
		}
	}
	
	// Update pool APR
	_, err = s.pool.Exec(ctx, `UPDATE staking_pools SET apr = $1 WHERE address = $2`, apr, poolAddr)
	if err != nil {
		return err
	}
	
	// Cache in Redis
	s.redis.Set(ctx, fmt.Sprintf("staking:apr:%s", poolAddr), fmt.Sprintf("%.2f", apr), time.Hour)
	
	return nil
}

func (s *Server) distributeRewards(ctx context.Context, poolAddr string) error {
	// Get delegators
	rows, err := s.pool.Query(ctx, `SELECT delegator, amount FROM delegations WHERE pool_address = $1`, poolAddr)
	if err != nil {
		return err
	}
	defer rows.Close()
	
	var delegators []struct {
		address string
		amount  float64
	}
	
	for rows.Next() {
		var d struct {
			address string
			amount  float64
		}
		if err := rows.Scan(&d.address, &d.amount); err != nil {
			continue
		}
		delegators = append(delegators, d)
	}
	
	// Get APR
	var apr float64
	if cachedAPR, err := s.redis.Get(ctx, fmt.Sprintf("staking:apr:%s", poolAddr)).Result(); err == nil {
		fmt.Sscanf(cachedAPR, "%f", &apr)
	}
	
	dailyRate := apr / 100 / 365
	
	// Distribute rewards
	for _, d := range delegators {
		reward := d.amount * dailyRate
		
		// Update pending rewards
		_, err = s.pool.Exec(ctx, `
			UPDATE delegations SET pending_rewards = pending_rewards + $1
			WHERE delegator = $2 AND pool_address = $3`,
			reward, d.address, poolAddr)
		if err != nil {
			return err
		}
	}
	
	return nil
}

// GetStakingPools returns all staking pools
func (s *Server) GetStakingPools(ctx context.Context) ([]StakingPool, error) {
	rows, err := s.pool.Query(ctx, `
		SELECT address, name, protocol, total_staked, total_delegators, apr, min_stake, lock_period_days, reward_token, is_active
		FROM staking_pools
		WHERE is_active = true
		ORDER BY total_staked DESC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var pools []StakingPool
	for rows.Next() {
		var p StakingPool
		if err := rows.Scan(&p.Address, &p.Name, &p.Protocol, &p.TotalStaked, 
			&p.TotalDelegators, &p.APR, &p.MinStake, &p.LockPeriod, &p.RewardToken, &p.IsActive); err != nil {
			continue
		}
		
		// Get real-time APR from Redis
		if cachedAPR, err := s.redis.Get(ctx, fmt.Sprintf("staking:apr:%s", p.Address)).Result(); err == nil {
			fmt.Sscanf(cachedAPR, "%f", &p.APR)
		}
		
		pools = append(pools, p)
	}
	
	return pools, nil
}

// GetDelegation returns a delegation
func (s *Server) GetDelegation(ctx context.Context, delegator, poolAddr string) (*Delegation, error) {
	var d Delegation
	err := s.pool.QueryRow(ctx, `
		SELECT delegator, pool_address, amount, rewards, pending_rewards, locked_until, created_at
		FROM delegations
		WHERE delegator = $1 AND pool_address = $2`,
		delegator, poolAddr,
	).Scan(&d.Delegator, &d.Pool, &d.Amount, &d.Rewards, &d.PendingRewards, &d.LockedUntil, &d.CreatedAt)
	if err != nil {
		return nil, err
	}
	
	return &d, nil
}

// GetDelegatorDelegations returns all delegations for a delegator
func (s *Server) GetDelegatorDelegations(ctx context.Context, delegator string) ([]Delegation, error) {
	rows, err := s.pool.Query(ctx, `
		SELECT delegator, pool_address, amount, rewards, pending_rewards, locked_until, created_at
		FROM delegations
		WHERE delegator = $1
		ORDER BY amount DESC`,
		delegator,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var delegations []Delegation
	for rows.Next() {
		var d Delegation
		if err := rows.Scan(&d.Delegator, &d.Pool, &d.Amount, &d.Rewards, &d.PendingRewards, &d.LockedUntil, &d.CreatedAt); err != nil {
			continue
		}
		delegations = append(delegations, d)
	}
	
	return delegations, nil
}

// GetStakingRewards returns staking rewards for a delegator
func (s *Server) GetStakingRewards(ctx context.Context, delegator string, days int) ([]StakingReward, error) {
	rows, err := s.pool.Query(ctx, `
		SELECT delegator, pool_address, amount, block_number, timestamp
		FROM staking_rewards
		WHERE delegator = $1 AND timestamp > $2
		ORDER BY timestamp DESC`,
		delegator, time.Now().Unix()-int64(days*86400),
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var rewards []StakingReward
	for rows.Next() {
		var r StakingReward
		var timestamp int64
		if err := rows.Scan(&r.Delegator, &r.Pool, &r.Amount, &r.BlockNumber, &timestamp); err != nil {
			continue
		}
		r.Timestamp = time.Unix(timestamp, 0)
		rewards = append(rewards, r)
	}
	
	return rewards, nil
}

// ClaimRewards claims pending rewards for a delegator
func (s *Server) ClaimRewards(ctx context.Context, delegator, poolAddr string) (float64, error) {
	var pendingRewards float64
	err := s.pool.QueryRow(ctx, `
		UPDATE delegations SET rewards = rewards + pending_rewards, pending_rewards = 0
		WHERE delegator = $1 AND pool_address = $2
		RETURNING pending_rewards`,
		delegator, poolAddr,
	).Scan(&pendingRewards)
	if err != nil {
		return 0, err
	}
	
	return pendingRewards, nil
}

// GetTotalStaked returns total staked amount
func (s *Server) GetTotalStaked(ctx context.Context) (float64, error) {
	var total float64
	err := s.pool.QueryRow(ctx, `SELECT COALESCE(SUM(amount), 0) FROM delegations`).Scan(&total)
	return total, err
}

// CalculateRewards calculates rewards for a given stake amount and period
func CalculateRewards(amount, apr float64, days int) float64 {
	dailyRate := apr / 100 / 365
	return amount * dailyRate * float64(days)
}

// CalculateLockPeriod calculates lock period end time
func CalculateLockPeriod(days int) time.Time {
	return time.Now().Add(time.Duration(days) * 24 * time.Hour)
}

// FormatAPR formats APR
func FormatAPR(apr float64) string {
	return fmt.Sprintf("%.2f%%", apr)
}

// FormatStake formats stake amount
func FormatStake(amount float64) string {
	if amount >= 1e6 {
		return fmt.Sprintf("%.2fM TGR", amount/1e6)
	}
	if amount >= 1e3 {
		return fmt.Sprintf("%.2fK TGR", amount/1e3)
	}
	return fmt.Sprintf("%.4f TGR", amount)
}

// FormatRewards formats rewards
func FormatRewards(rewards float64) string {
	if rewards >= 1e3 {
		return fmt.Sprintf("%.4f TGR", rewards)
	}
	return fmt.Sprintf("%.6f TGR", rewards)
}

// CalculateAPY calculates APY from APR
func CalculateAPY(apr float64) float64 {
	return (math.Pow(1+apr/100/365, 365) - 1) * 100
}