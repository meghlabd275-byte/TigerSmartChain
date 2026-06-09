// Package tokens provides token service for explorer.
package tokens

import (
	"context"
	"database/sql"
	"fmt"
	"math/big"
	"time"
)

// Service provides token-related functionality.
type Service struct {
	db *sql.DB
}

// Token represents a token.
type Token struct {
	ID              int64
	Address         string
	Name            string
	Symbol          string
	Decimals        int
	TotalSupply     string
	HoldersCount    int
	TransfersCount  int
	Creator         string
	ContractType    string
	IsVerified      bool
	Price           string
	MarketCap       string
	Volume24h       string
	PriceChange24h  string
	CreatedAt       time.Time
	UpdatedAt       time.Time
}

// Holder represents a token holder.
type Holder struct {
	Address    string
	Balance    string
	Percent    float64
	LastUpdate time.Time
}

// Transfer represents a token transfer.
type Transfer struct {
	ID              int64
	Hash            string
	BlockNumber     int64
	TransactionHash string
	From            string
	To              string
	Value           string
	TokenID         *int64
	Timestamp       time.Time
}

// NewService creates a new token service.
func NewService(db *sql.DB) *Service {
	return &Service{db: db}
}

// GetToken returns token by address.
func (s *Service) GetToken(ctx context.Context, address string) (*Token, error) {
	query := `
		SELECT id, address, name, symbol, decimals, total_supply, holders_count, 
		       transfers_count, creator, contract_type, is_verified, price, 
		       market_cap, volume_24h, price_change_24h, created_at, updated_at
		FROM tokens 
		WHERE address = $1
	`

	token := &Token{}
	err := s.db.QueryRowContext(ctx, query, address).Scan(
		&token.ID,
		&token.Address,
		&token.Name,
		&token.Symbol,
		&token.Decimals,
		&token.TotalSupply,
		&token.HoldersCount,
		&token.TransfersCount,
		&token.Creator,
		&token.ContractType,
		&token.IsVerified,
		&token.Price,
		&token.MarketCap,
		&token.Volume24h,
		&token.PriceChange24h,
		&token.CreatedAt,
		&token.UpdatedAt,
	)

	if err == sql.ErrNoRows {
		return nil, fmt.Errorf("token not found")
	}
	if err != nil {
		return nil, err
	}

	return token, nil
}

// ListTokens returns list of tokens.
func (s *Service) ListTokens(ctx context.Context, limit, offset int) ([]*Token, error) {
	query := `
		SELECT id, address, name, symbol, decimals, total_supply, holders_count, 
		       transfers_count, creator, contract_type, is_verified, price, 
		       market_cap, volume_24h, price_change_24h, created_at, updated_at
		FROM tokens 
		ORDER BY transfers_count DESC
		LIMIT $1 OFFSET $2
	`

	rows, err := s.db.QueryContext(ctx, query, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var tokens []*Token
	for rows.Next() {
		token := &Token{}
		err := rows.Scan(
			&token.ID,
			&token.Address,
			&token.Name,
			&token.Symbol,
			&token.Decimals,
			&token.TotalSupply,
			&token.HoldersCount,
			&token.TransfersCount,
			&token.Creator,
			&token.ContractType,
			&token.IsVerified,
			&token.Price,
			&token.MarketCap,
			&token.Volume24h,
			&token.PriceChange24h,
			&token.CreatedAt,
			&token.UpdatedAt,
		)
		if err != nil {
			return nil, err
		}
		tokens = append(tokens, token)
	}

	return tokens, nil
}

// GetHolders returns token holders.
func (s *Service) GetHolders(ctx context.Context, tokenAddress string, limit, offset int) ([]*Holder, error) {
	query := `
		SELECT address, balance, percent, last_update
		FROM token_holders 
		WHERE token_address = $1
		ORDER BY balance DESC
		LIMIT $2 OFFSET $3
	`

	rows, err := s.db.QueryContext(ctx, query, tokenAddress, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var holders []*Holder
	for rows.Next() {
		holder := &Holder{}
		err := rows.Scan(
			&holder.Address,
			&holder.Balance,
			&holder.Percent,
			&holder.LastUpdate,
		)
		if err != nil {
			return nil, err
		}
		holders = append(holders, holder)
	}

	return holders, nil
}

// GetTransfers returns token transfers.
func (s *Service) GetTransfers(ctx context.Context, tokenAddress string, limit, offset int) ([]*Transfer, error) {
	query := `
		SELECT id, hash, block_number, transaction_hash, from_address, to_address, 
		       value, token_id, timestamp
		FROM token_transfers 
		WHERE token_address = $1
		ORDER BY block_number DESC, id DESC
		LIMIT $2 OFFSET $3
	`

	rows, err := s.db.QueryContext(ctx, query, tokenAddress, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var transfers []*Transfer
	for rows.Next() {
		transfer := &Transfer{}
		err := rows.Scan(
			&transfer.ID,
			&transfer.Hash,
			&transfer.BlockNumber,
			&transfer.TransactionHash,
			&transfer.From,
			&transfer.To,
			&transfer.Value,
			&transfer.TokenID,
			&transfer.Timestamp,
		)
		if err != nil {
			return nil, err
		}
		transfers = append(transfers, transfer)
	}

	return transfers, nil
}

// SearchTokens searches tokens by name or symbol.
func (s *Service) SearchTokens(ctx context.Context, query string, limit int) ([]*Token, error) {
	searchQuery := `
		SELECT id, address, name, symbol, decimals, total_supply, holders_count, 
		       transfers_count, creator, contract_type, is_verified, price, 
		       market_cap, volume_24h, price_change_24h, created_at, updated_at
		FROM tokens 
		WHERE name ILIKE $1 OR symbol ILIKE $1
		ORDER BY transfers_count DESC
		LIMIT $2
	`

	rows, err := s.db.QueryContext(ctx, searchQuery, "%"+query+"%", limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var tokens []*Token
	for rows.Next() {
		token := &Token{}
		err := rows.Scan(
			&token.ID,
			&token.Address,
			&token.Name,
			&token.Symbol,
			&token.Decimals,
			&token.TotalSupply,
			&token.HoldersCount,
			&token.TransfersCount,
			&token.Creator,
			&token.ContractType,
			&token.IsVerified,
			&token.Price,
			&token.MarketCap,
			&token.Volume24h,
			&token.PriceChange24h,
			&token.CreatedAt,
			&token.UpdatedAt,
		)
		if err != nil {
			return nil, err
		}
		tokens = append(tokens, token)
	}

	return tokens, nil
}

// GetTokenSupply returns total and circulating supply.
func (s *Service) GetTokenSupply(ctx context.Context, tokenAddress string) (total, circulating *big.Int, err error) {
	total = big.NewInt(0)
	circulating = big.NewInt(0)

	// Get total supply
	var totalSupply string
	err = s.db.QueryRowContext(ctx,
		"SELECT total_supply FROM tokens WHERE address = $1",
		tokenAddress,
	).Scan(&totalSupply)
	if err != nil {
		return nil, nil, err
	}

	total.SetString(totalSupply, 10)

	// Get burned amount
	var burned string
	err = s.db.QueryRowContext(ctx,
		"SELECT COALESCE(SUM(value), '0') FROM token_transfers WHERE token_address = $1 AND to_address = '0x0000000000000000000000000000000000000000'",
		tokenAddress,
	).Scan(&burned)
	if err != nil && err != sql.ErrNoRows {
		return nil, nil, err
	}

	burnedAmount := big.NewInt(0)
	burnedAmount.SetString(burned, 10)

	circulating.Sub(total, burnedAmount)

	return total, circulating, nil
}

// GetTopTokens returns top tokens by various metrics.
func (s *Service) GetTopTokens(ctx context.Context, metric string, limit int) ([]*Token, error) {
	validMetrics := map[string]string{
		"transfers": "transfers_count",
		"holders":   "holders_count",
		"volume":    "volume_24h",
		"marketcap": "market_cap",
	}

	orderBy, ok := validMetrics[metric]
	if !ok {
		orderBy = "transfers_count"
	}

	query := fmt.Sprintf(`
		SELECT id, address, name, symbol, decimals, total_supply, holders_count, 
		       transfers_count, creator, contract_type, is_verified, price, 
		       market_cap, volume_24h, price_change_24h, created_at, updated_at
		FROM tokens 
		ORDER BY %s DESC
		LIMIT $1
	`, orderBy)

	rows, err := s.db.QueryContext(ctx, query, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var tokens []*Token
	for rows.Next() {
		token := &Token{}
		err := rows.Scan(
			&token.ID,
			&token.Address,
			&token.Name,
			&token.Symbol,
			&token.Decimals,
			&token.TotalSupply,
			&token.HoldersCount,
			&token.TransfersCount,
			&token.Creator,
			&token.ContractType,
			&token.IsVerified,
			&token.Price,
			&token.MarketCap,
			&token.Volume24h,
			&token.PriceChange24h,
			&token.CreatedAt,
			&token.UpdatedAt,
		)
		if err != nil {
			return nil, err
		}
		tokens = append(tokens, token)
	}

	return tokens, nil
}

var _ = context.Background // Use context
var _ = big.NewInt       // Use big.Int
