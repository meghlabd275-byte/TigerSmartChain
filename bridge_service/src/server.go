// Package bridge provides cross-chain bridge service
package bridge

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

type Config struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
}

type BridgeTransfer struct {
	ID             string    `json:"id"`
	SourceChain   string    `json:"sourceChain"`
	TargetChain   string    `json:"targetChain"`
	Sender        string    `json:"sender"`
	Recipient     string    `json:"recipient"`
	Token        string    `json:"token"`
	Amount        float64   `json:"amount"`
	Status        string    `json:"status"`
	SourceTXHash string    `json:"sourceTxHash"`
	TargetTXHash string    `json:"targetTxHash"`
	Confirmations int      `json:"confirmations"`
	RequiredConf  int      `json:"requiredConf"`
	Fee           float64   `json:"fee"`
	Timestamp     time.Time `json:"timestamp"`
}

type BridgeStats struct {
	TotalVolume     float64 `json:"totalVolume"`
	TotalTransfers int     `json:"totalTransfers"`
	Volume24h      float64 `json:"volume24h"`
	Transfers24h  int     `json:"transfers24h"`
	SuccessRate    float64 `json:"successRate"`
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
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 14})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, err
	}
	createTables(ctx, pool)
	return &Server{cfg: cfg, pool: pool, redis: rdb}, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) {
	pool.Exec(ctx, `CREATE TABLE IF NOT EXISTS bridge_transfers (id VARCHAR(66) PRIMARY KEY, source_chain VARCHAR(20) NOT NULL, target_chain VARCHAR(20) NOT NULL, sender VARCHAR(42) NOT NULL, recipient VARCHAR(42) NOT NULL, token VARCHAR(42) NOT NULL, amount DECIMAL(30,8) NOT NULL, status VARCHAR(20) DEFAULT 'pending', source_tx_hash VARCHAR(66), target_tx_hash VARCHAR(66), confirmations INTEGER DEFAULT 0, required_conf INTEGER DEFAULT 12, fee DECIMAL(20,8), timestamp BIGINT NOT NULL)`)
	pool.Exec(ctx, `CREATE INDEX IF NOT EXISTS idx_bridge_sender ON bridge_transfers(sender)`)
}

func (s *Server) GetTransfers(ctx context.Context, sender string, limit int) ([]BridgeTransfer, error) {
	rows, err := s.pool.Query(ctx, "SELECT id, source_chain, target_chain, sender, recipient, token, amount, status, source_tx_hash, target_tx_hash, confirmations, required_conf, fee, timestamp FROM bridge_transfers WHERE sender = $1 ORDER BY timestamp DESC LIMIT $2", sender, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var transfers []BridgeTransfer
	for rows.Next() {
		var t BridgeTransfer
		var timestamp int64
		if err := rows.Scan(&t.ID, &t.SourceChain, &t.TargetChain, &t.Sender, &t.Recipient, &t.Token, &t.Amount, &t.Status, &t.SourceTXHash, &t.TargetTXHash, &t.Confirmations, &t.RequiredConf, &t.Fee, &timestamp); err != nil {
			continue
		}
		t.Timestamp = time.Unix(timestamp, 0)
		transfers = append(transfers, t)
	}
	return transfers, nil
}

func (s *Server) GetTransfer(ctx context.Context, id string) (*BridgeTransfer, error) {
	var t BridgeTransfer
	var timestamp int64
	err := s.pool.QueryRow(ctx, "SELECT id, source_chain, target_chain, sender, recipient, token, amount, status, source_tx_hash, target_tx_hash, confirmations, required_conf, fee, timestamp FROM bridge_transfers WHERE id = $1", id).Scan(&t.ID, &t.SourceChain, &t.TargetChain, &t.Sender, &t.Recipient, &t.Token, &t.Amount, &t.Status, &t.SourceTXHash, &t.TargetTXHash, &t.Confirmations, &t.RequiredConf, &t.Fee, &timestamp)
	if err != nil {
		return nil, err
	}
	t.Timestamp = time.Unix(timestamp, 0)
	return &t, nil
}

func (s *Server) GetBridgeStats(ctx context.Context) (*BridgeStats, error) {
	var stats BridgeStats
	s.pool.QueryRow(ctx, "SELECT COALESCE(SUM(amount), 0), COUNT(*) FROM bridge_transfers").Scan(&stats.TotalVolume, &stats.TotalTransfers)
	s.pool.QueryRow(ctx, "SELECT COALESCE(SUM(amount), 0), COUNT(*) FROM bridge_transfers WHERE timestamp > $1", time.Now().Unix()-86400).Scan(&stats.Volume24h, &stats.Transfers24h)
	var completed, failed int
	s.pool.QueryRow(ctx, "SELECT COUNT(CASE WHEN status = 'completed' THEN 1 END), COUNT(CASE WHEN status = 'failed' THEN 1 END) FROM bridge_transfers").Scan(&completed, &failed)
	total := completed + failed
	if total > 0 {
		stats.SuccessRate = float64(completed) / float64(total) * 100
	}
	return &stats, nil
}

func (s *Server) UpdateTransferStatus(ctx context.Context, id, status, targetTXHash string, confirmations int) error {
	_, err := s.pool.Exec(ctx, "UPDATE bridge_transfers SET status = $1, target_tx_hash = $2, confirmations = $3 WHERE id = $4", status, targetTXHash, confirmations, id)
	return err
}

func (s *Server) GetSupportedChains(ctx context.Context) []string {
	return []string{"Ethereum", "BSC", "Polygon", "Arbitrum", "Optimism", "Avalanche", "Base", "TigerSmartChain"}
}

func FormatAmount(amount float64, decimals int) string {
	return fmt.Sprintf("%.0f", amount)
}

func CalculateFee(amount, feePercent float64) float64 {
	return amount * feePercent / 100
}