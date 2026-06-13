// Package validator provides validator performance tracking service
// Built with Go for high performance
package validator

import (
	"context"
	"fmt"
	"math"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds validator service configuration
type Config struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
}

// ValidatorInfo represents validator information
type ValidatorInfo struct {
	Address           string  `json:"address"`
	Name             string  `json:"name"`
	Website          string  `json:"website"`
	CommissionRate   float64 `json:"commissionRate"`
	TotalStake      float64 `json:"totalStake"`
	SelfStake        float64 `json:"selfStake"`
	DelegatorsCount int     `json:"delegatorsCount"`
	BlocksProposed  int64   `json:"blocksProposed"`
	BlocksMissed    int64   `json:"blocksMissed"`
	Uptime          float64 `json:"uptime"`
	Rewards         float64 `json:"rewards"`
	SelfRewards     float64 `json:"selfRewards"`
	DelegatorRewards float64 `json:"delegatorRewards"`
	IsActive        bool    `json:"isActive"`
	IsJailed        bool    `json:"isJailed"`
	JailReason      string  `json:"jailReason,omitempty"`
	JailedUntil    *time.Time `json:"jailedUntil,omitempty"`
	LastProposed   int64   `json:"lastProposed"`
	FirstProposed  int64   `json:"firstProposed"`
}

// ValidatorPerformance represents validator performance metrics
type ValidatorPerformance struct {
	Address         string    `json:"address"`
	Period          string    `json:"period"` // daily, weekly, monthly
	BlocksProposed  int64     `json:"blocksProposed"`
	BlocksMissed    int64     `json:"blocksMissed"`
	Uptime          float64   `json:"uptime"`
	Rewards         float64   `json:"rewards"`
	AvgBlockTime    float64   `json:"avgBlockTime"`
	Participation   float64   `json:"participation"`
}

// SlashEvent represents a slashing event
type SlashEvent struct {
	Validator   string    `json:"validator"`
	BlockNumber int64     `json:"blockNumber"`
	Timestamp   time.Time `json:"timestamp"`
	Reason      string    `json:"reason"`
	Amount      float64   `json:"amount"`
}

// Server represents the validator service server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
}

// NewServer creates a new validator service server
func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 9})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	if err := createTables(ctx, pool); err != nil {
		return nil, fmt.Errorf("failed to create tables: %w", err)
	}
	srv := &Server{cfg: cfg, pool: pool, redis: rdb}
	go srv.startUpdater()
	return srv, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) error {
	queries := []string{
		`CREATE TABLE IF NOT EXISTS validator_performance (id SERIAL PRIMARY KEY, validator_address VARCHAR(42) NOT NULL, period VARCHAR(20) NOT NULL, blocks_proposed BIGINT DEFAULT 0, blocks_missed BIGINT DEFAULT 0, uptime DECIMAL(5,4), rewards DECIMAL(30,8), avg_block_time DECIMAL(10,4), participation DECIMAL(5,4), timestamp BIGINT NOT NULL, UNIQUE(validator_address, period, timestamp))`,
		`CREATE TABLE IF NOT EXISTS slash_events (id SERIAL PRIMARY KEY, validator_address VARCHAR(42) NOT NULL, block_number BIGINT NOT NULL, reason VARCHAR(100) NOT NULL, amount DECIMAL(30,8), timestamp BIGINT NOT NULL)`,
		`CREATE TABLE IF NOT EXISTS validator_rewards (id SERIAL PRIMARY KEY, validator_address VARCHAR(42) NOT NULL, self_rewards DECIMAL(30,8), delegator_rewards DECIMAL(30,8), total_rewards DECIMAL(30,8), block_number BIGINT NOT NULL, timestamp BIGINT NOT NULL)`,
		`CREATE INDEX IF NOT EXISTS idx_validator_perf_address ON validator_performance(validator_address)`,
		`CREATE INDEX IF NOT EXISTS idx_slash_validator ON slash_events(validator_address)`,
		`CREATE INDEX IF NOT EXISTS idx_validator_rewards ON validator_rewards(validator_address, timestamp)`,
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
		if err := s.updateValidatorMetrics(); err != nil {
			fmt.Printf("failed to update validator metrics: %v\n", err)
		}
	}
}

func (s *Server) updateValidatorMetrics() error {
	ctx := context.Background()
	
	// Get all validators
	rows, err := s.pool.Query(ctx, `SELECT address FROM validators`)
	if err != nil {
		return err
	}
	defer rows.Close()
	
	var validators []string
	for rows.Next() {
		var addr string
		if err := rows.Scan(&addr); err != nil {
			continue
		}
		validators = append(validators, addr)
	}
	
	for _, v := range validators {
		if err := s.calculateUptime(ctx, v); err != nil {
			fmt.Printf("failed to calculate uptime for %s: %v\n", v, err)
		}
		if err := s.calculateRewards(ctx, v); err != nil {
			fmt.Printf("failed to calculate rewards for %s: %v\n", v, err)
		}
	}
	
	return nil
}

func (s *Server) calculateUptime(ctx context.Context, validator string) error {
	// Get blocks proposed vs expected
	var proposed, missed int64
	err := s.pool.QueryRow(ctx, `
		SELECT 
			COALESCE(SUM(CASE WHEN miner = $1 THEN 1 ELSE 0 END), 0),
			COALESCE(SUM(CASE WHEN miner != $1 THEN 1 ELSE 0 END), 0)
		FROM blocks 
		WHERE timestamp > $2`,
		validator, time.Now().Unix()-86400,
	).Scan(&proposed, &missed)
	if err != nil {
		return err
	}
	
	total := proposed + missed
	uptime := float64(0)
	if total > 0 {
		uptime = float64(proposed) / float64(total) * 100
	}
	
	// Store in Redis for quick access
	s.redis.Set(ctx, fmt.Sprintf("validator:uptime:%s", validator), fmt.Sprintf("%.4f", uptime), time.Hour)
	
	return nil
}

func (s *Server) calculateRewards(ctx context.Context, validator string) error {
	// Calculate rewards based on blocks proposed
	var blocksProposed int64
	err := s.pool.QueryRow(ctx, `
		SELECT COUNT(*) FROM blocks WHERE miner = $1 AND timestamp > $2`,
		validator, time.Now().Unix()-86400,
	).Scan(&blocksProposed)
	if err != nil {
		return err
	}
	
	// Block reward (mock - should come from chain)
	blockReward := 0.0625 // TGR per block
	
	selfStake := blockReward * 0.1 // 10% self stake
	delegatorRewards := blockReward * 0.9 * 0.9 // 90% to delegators after commission
	
	totalRewards := float64(blocksProposed) * blockReward
	
	// Store rewards
	s.redis.Set(ctx, fmt.Sprintf("validator:rewards:%s", validator), fmt.Sprintf("%.8f", totalRewards), time.Hour)
	
	return nil
}

// GetValidators returns all validators with their metrics
func (s *Server) GetValidators(ctx context.Context) ([]ValidatorInfo, error) {
	rows, err := s.pool.Query(ctx, `
		SELECT v.address, v.name, v.commission_rate, v.total_stake, v.self_stake, 
			v.delegators_count, v.blocks_proposed, v.blocks_missed, v.uptime, 
			v.is_active, v.is_jailed, v.jail_reason
		FROM validators v
		ORDER BY v.total_stake DESC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var validators []ValidatorInfo
	for rows.Next() {
		var v ValidatorInfo
		if err := rows.Scan(&v.Address, &v.Name, &v.CommissionRate, &v.TotalStake, 
			&v.SelfStake, &v.DelegatorsCount, &v.BlocksProposed, &v.BlocksMissed, 
			&v.Uptime, &v.IsActive, &v.IsJailed, &v.JailReason); err != nil {
			continue
		}
		
		// Get uptime from Redis
		if uptime, err := s.redis.Get(ctx, fmt.Sprintf("validator:uptime:%s", v.Address)).Result(); err == nil {
			fmt.Sscanf(uptime, "%f", &v.Uptime)
		}
		
		// Get rewards from Redis
		if rewards, err := s.redis.Get(ctx, fmt.Sprintf("validator:rewards:%s", v.Address)).Result(); err == nil {
			fmt.Sscanf(rewards, "%f", &v.Rewards)
		}
		
		validators = append(validators, v)
	}
	
	return validators, nil
}

// GetValidator returns a specific validator with all metrics
func (s *Server) GetValidator(ctx context.Context, address string) (*ValidatorInfo, error) {
	var v ValidatorInfo
	err := s.pool.QueryRow(ctx, `
		SELECT address, name, website, commission_rate, total_stake, self_stake,
			delegators_count, blocks_proposed, blocks_missed, uptime,
			is_active, is_jailed, jail_reason, jailed_until
		FROM validators WHERE address = $1`,
		address,
	).Scan(&v.Address, &v.Name, &v.Website, &v.CommissionRate, &v.TotalStake,
		&v.SelfStake, &v.DelegatorsCount, &v.BlocksProposed, &v.BlocksMissed,
		&v.Uptime, &v.IsActive, &v.IsJailed, &v.JailReason, &v.JailedUntil)
	if err != nil {
		return nil, err
	}
	
	// Get real-time metrics from Redis
	if uptime, err := s.redis.Get(ctx, fmt.Sprintf("validator:uptime:%s", address)).Result(); err == nil {
		fmt.Sscanf(uptime, "%f", &v.Uptime)
	}
	
	if rewards, err := s.redis.Get(ctx, fmt.Sprintf("validator:rewards:%s", address)).Result(); err == nil {
		fmt.Sscanf(rewards, "%f", &v.Rewards)
	}
	
	return &v, nil
}

// GetValidatorPerformance returns validator performance for a period
func (s *Server) GetValidatorPerformance(ctx context.Context, address, period string) (*ValidatorPerformance, error) {
	var perf ValidatorPerformance
	err := s.pool.QueryRow(ctx, `
		SELECT validator_address, period, blocks_proposed, blocks_missed, uptime, rewards, avg_block_time, participation
		FROM validator_performance
		WHERE validator_address = $1 AND period = $2
		ORDER BY timestamp DESC LIMIT 1`,
		address, period,
	).Scan(&perf.Address, &perf.Period, &perf.BlocksProposed, &perf.BlocksMissed,
		&perf.Uptime, &perf.Rewards, &perf.AvgBlockTime, &perf.Participation)
	if err != nil {
		return nil, err
	}
	
	return &perf, nil
}

// GetSlashEvents returns slash events for a validator
func (s *Server) GetSlashEvents(ctx context.Context, address string, limit int) ([]SlashEvent, error) {
	rows, err := s.pool.Query(ctx, `
		SELECT validator_address, block_number, reason, amount, timestamp
		FROM slash_events
		WHERE validator_address = $1
		ORDER BY timestamp DESC LIMIT $2`,
		address, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var events []SlashEvent
	for rows.Next() {
		var e SlashEvent
		var timestamp int64
		if err := rows.Scan(&e.Validator, &e.BlockNumber, &e.Reason, &e.Amount, &timestamp); err != nil {
			continue
		}
		e.Timestamp = time.Unix(timestamp, 0)
		events = append(events, e)
	}
	
	return events, nil
}

// GetValidatorRewards returns validator rewards history
func (s *Server) GetValidatorRewards(ctx context.Context, address string, days int) ([]struct {
	SelfRewards     float64
	DelegatorRewards float64
	TotalRewards   float64
	BlockNumber    int64
	Timestamp      time.Time
}, error) {
	rows, err := s.pool.Query(ctx, `
		SELECT self_rewards, delegator_rewards, total_rewards, block_number, timestamp
		FROM validator_rewards
		WHERE validator_address = $1 AND timestamp > $2
		ORDER BY timestamp DESC`,
		address, time.Now().Unix()-int64(days*86400))
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var rewards []struct {
		SelfRewards     float64
		DelegatorRewards float64
		TotalRewards   float64
		BlockNumber    int64
		Timestamp      time.Time
	}
	
	for rows.Next() {
		var r struct {
			SelfRewards     float64
			DelegatorRewards float64
			TotalRewards   float64
			BlockNumber    int64
			Timestamp      time.Time
		}
		var timestamp int64
		if err := rows.Scan(&r.SelfRewards, &r.DelegatorRewards, &r.TotalRewards, &r.BlockNumber, &timestamp); err != nil {
			continue
		}
		r.Timestamp = time.Unix(timestamp, 0)
		rewards = append(rewards, r)
	}
	
	return rewards, nil
}

// CalculateAPY calculates the Annual Percentage Yield for staking
func CalculateAPY(dailyReward, totalStake float64) float64 {
	if totalStake == 0 {
		return 0
	}
	dailyRate := dailyReward / totalStake
	apy := math.Pow(1+dailyRate, 365) - 1
	return apy * 100
}

// FormatUptime formats uptime as percentage
func FormatUptime(uptime float64) string {
	return fmt.Sprintf("%.2f%%", uptime)
}

// FormatRewards formats rewards in human readable format
func FormatRewards(rewards float64) string {
	if rewards >= 1e6 {
		return fmt.Sprintf("%.2fM TGR", rewards/1e6)
	}
	if rewards >= 1e3 {
		return fmt.Sprintf("%.2fK TGR", rewards/1e3)
	}
	return fmt.Sprintf("%.4f TGR", rewards)
}