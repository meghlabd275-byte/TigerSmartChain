// Package approvals provides ERC-20 approval and allowance tracking.
package approvals

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"math/big"
	"strings"
	"time"
)

// Approval represents an ERC-20 approval event
type Approval struct {
	ID            int64     `json:"id"`
	Hash          string   `json:"hash"`
	BlockNumber  int64    `json:"blockNumber"`
	TransactionHash string  `json:"transactionHash"`
	TokenAddress string   `json:"tokenAddress"`
	Owner        string   `json:"owner"`
	Spender      string   `json:"spender"`
	Value        string   `json:"value"`
	IsIncrease   bool     `json:"isIncrease"`
	Timestamp    time.Time `json:"timestamp"`
}

// Allowance represents current allowance
type Allowance struct {
	TokenAddress string   `json:"tokenAddress"`
	Owner      string   `json:"owner"`
	Spender    string   `json:"spender"`
	Value     string   `json:"value"`
	LastUpdate time.Time `json:"lastUpdate"`
}

// Service provides approval tracking
type Service struct {
	db *sql.DB
}

// Config holds service configuration
type Config struct {
	DB *sql.DB
}

// NewService creates a new approval tracking service
func NewService(cfg *Config) *Service {
	return &Service{db: cfg.DB}
}

// IndexApproval indexes an approval event
func (s *Service) IndexApproval(ctx context.Context, approval *Approval) error {
	query := `
		INSERT INTO token_approvals (hash, block_number, transaction_hash, token_address, 
		                        owner_address, spender_address, value, is_increase, timestamp)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
		ON CONFLICT DO NOTHING
		RETURNING id
	`

	return s.db.QueryRowContext(ctx, query,
		approval.Hash, approval.BlockNumber, approval.TransactionHash,
		approval.TokenAddress, approval.Owner, approval.Spender,
		approval.Value, approval.IsIncrease, approval.Timestamp,
	).Scan(&approval.ID)
}

// GetApprovalHistory returns approval history for a token
func (s *Service) GetApprovalHistory(ctx context.Context, tokenAddress, owner string, limit, offset int) ([]*Approval, error) {
	query := `
		SELECT id, hash, block_number, transaction_hash, token_address, 
		       owner_address, spender_address, value, is_increase, timestamp
		FROM token_approvals
		WHERE token_address = $1 AND owner_address = $2
		ORDER BY block_number DESC, id DESC
		LIMIT $3 OFFSET $4
	`

	rows, err := s.db.QueryContext(ctx, query, tokenAddress, owner, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var approvals []*Approval
	for rows.Next() {
		a := &Approval{}
		err := rows.Scan(
			&a.ID, &a.Hash, &a.BlockNumber, &a.TransactionHash,
			&a.TokenAddress, &a.Owner, &a.Spender,
			&a.Value, &a.IsIncrease, &a.Timestamp,
		)
		if err != nil {
			return nil, err
		}
		approvals = append(approvals, a)
	}

	return approvals, rows.Err()
}

// GetAllowance returns current allowance
func (s *Service) GetAllowance(ctx context.Context, tokenAddress, owner, spender string) (*Allowance, error) {
	query := `
		SELECT token_address, owner_address, spender_address, value, last_update
		FROM token_allowances
		WHERE token_address = $1 AND owner_address = $2 AND spender_address = $3
	`

	a := &Allowance{}
	err := s.db.QueryRowContext(ctx, query, tokenAddress, owner, spender).Scan(
		&a.TokenAddress, &a.Owner, &a.Spender, &a.Value, &a.LastUpdate,
	)
	if err != nil {
		return nil, err
	}

	return a, nil
}

// UpdateAllowance updates an allowance
func (s *Service) UpdateAllowance(ctx context.Context, tokenAddress, owner, spender, value string) error {
	query := `
		INSERT INTO token_allowances (token_address, owner_address, spender_address, value, last_update)
		VALUES ($1, $2, $3, $4, $5)
		ON CONFLICT (token_address, owner_address, spender_address) DO UPDATE SET
			value = EXCLUDED.value,
			last_update = EXCLUDED.last_update
	`

	_, err := s.db.ExecContext(ctx, query, tokenAddress, owner, spender, value, time.Now())
	return err
}

// GetSpenderAllowances returns all allowances for a spender
func (s *Service) GetSpenderAllowances(ctx context.Context, spender string) ([]*Allowance, error) {
	query := `
		SELECT token_address, owner_address, spender_address, value, last_update
		FROM token_allowances
		WHERE spender_address = $1 AND value != '0'
	`

	rows, err := s.db.QueryContext(ctx, query, spender)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var allowances []*Allowance
	for rows.Next() {
		a := &Allowance{}
		err := rows.Scan(
			&a.TokenAddress, &a.Owner, &a.Spender, &a.Value, &a.LastUpdate,
		)
		if err != nil {
			return nil, err
		}
		allowances = append(allowances, a)
	}

	return allowances, rows.Err()
}

// GetTopSpenders returns top spenders by allowance volume
func (s *Service) GetTopSpenders(ctx context.Context, limit int) ([]struct {
	Spender string `json:"spender"`
	Total  string `json:"total"`
}, error) {
	query := `
		SELECT spender_address, SUM(value::numeric) as total
		FROM token_allowances
		GROUP BY spender_address
		ORDER BY total DESC
		LIMIT $1
	`

	rows, err := s.db.QueryContext(ctx, query, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var result []struct {
		Spender string `json:"spender"`
		Total  string `json:"total"`
	}

	for rows.Next() {
		var r struct {
			Spender string `json:"spender"`
			Total  string `json:"total"`
		}
		rows.Scan(&r.Spender, &r.Total)
		result = append(result, r)
	}

	return result, rows.Err()
}

// ProcessApprovalEvent processes an Approval event from logs
func (s *Service) ProcessApprovalEvent(ctx context.Context, log *ApprovalEvent) error {
	owner := strings.ToLower(log.Owner)
	spender := strings.ToLower(log.Spender)

	approval := &Approval{
		Hash:            log.TransactionHash,
		BlockNumber:    log.BlockNumber,
		TransactionHash: log.TransactionHash,
		TokenAddress:   log.TokenAddress,
		Owner:         owner,
		Spender:       spender,
		Value:         log.Value,
		IsIncrease:    true,
		Timestamp:     time.Now(),
	}

	// Check if approval was revoked (value = 0)
	if log.Value == "0" || log.Value == "0x0" {
		approval.Value = "0"
		approval.IsIncrease = false
	}

	// Index approval
	if err := s.IndexApproval(ctx, approval); err != nil {
		return err
	}

	// Update current allowance
	return s.UpdateAllowance(ctx, approval.TokenAddress, approval.Owner, approval.Spender, approval.Value)
}

// ApprovalEvent represents raw approval event data
type ApprovalEvent struct {
	TransactionHash string
	BlockNumber    int64
	TokenAddress  string
	Owner        string
	Spender      string
	Value       string
}

var _ = big.NewInt   // Use big.Int
var _ = json.Marshal // Use JSON
var _ = fmt.Sprintf // Use fmt