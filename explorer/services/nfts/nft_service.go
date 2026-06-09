// Package nfts provides NFT service for explorer.
package nfts

import (
	"context"
	"database/sql"
	"fmt"
	"time"
)

// Service provides NFT-related functionality.
type Service struct {
	db *sql.DB
}

// NFT represents an NFT.
type NFT struct {
	ID              int64
	Address         string
	TokenID         string
	Owner           string
	Creator         string
	Name            string
	Description     string
	ImageURL        string
	AnimationURL    string
	ExternalURL     string
	Attributes      []Attribute
	ContractType    string
	TokenURI        string
	Collection      *Collection
	BlockNumber     int64
	BlockHash       string
	TransactionHash string
	Timestamp       time.Time
}

// Collection represents an NFT collection.
type Collection struct {
	ID               int64
	Address          string
	Name             string
	Symbol           string
	Description      string
	ImageURL         string
	ExternalURL      string
	ContractType     string
	TotalSupply     int64
	OwnersCount      int64
	NFTsCount       int64
	FloorPrice       string
	Volume24h        string
	VolumeTotal      string
	Creator          string
	IsVerified       bool
	CreatedAt        time.Time
}

// Attribute represents NFT attribute.
type Attribute struct {
	TraitType   string `json:"trait_type"`
	Value       string `json:"value"`
	DisplayType string `json:"display_type,omitempty"`
}

// Holder represents NFT holder.
type Holder struct {
	Address      string
	Balance      int64
	LastUpdate   time.Time
}

// Transfer represents NFT transfer.
type Transfer struct {
	ID              int64
	Hash            string
	BlockNumber     int64
	TransactionHash string
	From            string
	To              string
	TokenID         string
	Timestamp       time.Time
}

// NewService creates a new NFT service.
func NewService(db *sql.DB) *Service {
	return &Service{db: db}
}

// GetNFT returns NFT by address and token ID.
func (s *Service) GetNFT(ctx context.Context, address, tokenID string) (*NFT, error) {
	query := `
		SELECT n.id, n.address, n.token_id, n.owner, n.creator, n.name, n.description,
		       n.image_url, n.animation_url, n.external_url, n.contract_type, 
		       n.token_uri, n.block_number, n.block_hash, n.transaction_hash, n.timestamp,
		       c.id, c.name, c.symbol, c.image_url, c.total_supply
		FROM nfts n
		LEFT JOIN collections c ON n.collection_address = c.address
		WHERE n.address = $1 AND n.token_id = $2
	`

	nft := &NFT{}
	var collectionID sql.NullInt64
	var collectionName, collectionSymbol, collectionImageURL sql.NullString
	var collectionTotalSupply sql.NullInt64

	err := s.db.QueryRowContext(ctx, query, address, tokenID).Scan(
		&nft.ID,
		&nft.Address,
		&nft.TokenID,
		&nft.Owner,
		&nft.Creator,
		&nft.Name,
		&nft.Description,
		&nft.ImageURL,
		&nft.AnimationURL,
		&nft.ExternalURL,
		&nft.ContractType,
		&nft.TokenURI,
		&nft.BlockNumber,
		&nft.BlockHash,
		&nft.TransactionHash,
		&nft.Timestamp,
		&collectionID,
		&collectionName,
		&collectionSymbol,
		&collectionImageURL,
		&collectionTotalSupply,
	)

	if err == sql.ErrNoRows {
		return nil, fmt.Errorf("NFT not found")
	}
	if err != nil {
		return nil, err
	}

	if collectionID.Valid {
		nft.Collection = &Collection{
			ID:          collectionID.Int64,
			Address:     address,
			Name:        collectionName.String,
			Symbol:      collectionSymbol.String,
			ImageURL:    collectionImageURL.String,
			TotalSupply: collectionTotalSupply.Int64,
		}
	}

	return nft, nil
}

// GetCollection returns collection by address.
func (s *Service) GetCollection(ctx context.Context, address string) (*Collection, error) {
	query := `
		SELECT id, address, name, symbol, description, image_url, external_url,
		       contract_type, total_supply, owners_count, nfts_count, floor_price,
		       volume_24h, volume_total, creator, is_verified, created_at
		FROM collections 
		WHERE address = $1
	`

	collection := &Collection{}
	err := s.db.QueryRowContext(ctx, query, address).Scan(
		&collection.ID,
		&collection.Address,
		&collection.Name,
		&collection.Symbol,
		&collection.Description,
		&collection.ImageURL,
		&collection.ExternalURL,
		&collection.ContractType,
		&collection.TotalSupply,
		&collection.OwnersCount,
		&collection.NFTsCount,
		&collection.FloorPrice,
		&collection.Volume24h,
		&collection.VolumeTotal,
		&collection.Creator,
		&collection.IsVerified,
		&collection.CreatedAt,
	)

	if err == sql.ErrNoRows {
		return nil, fmt.Errorf("collection not found")
	}
	if err != nil {
		return nil, err
	}

	return collection, nil
}

// ListCollections returns list of collections.
func (s *Service) ListCollections(ctx context.Context, limit, offset int) ([]*Collection, error) {
	query := `
		SELECT id, address, name, symbol, description, image_url, external_url,
		       contract_type, total_supply, owners_count, nfts_count, floor_price,
		       volume_24h, volume_total, creator, is_verified, created_at
		FROM collections 
		ORDER BY volume_total DESC
		LIMIT $1 OFFSET $2
	`

	rows, err := s.db.QueryContext(ctx, query, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var collections []*Collection
	for rows.Next() {
		collection := &Collection{}
		err := rows.Scan(
			&collection.ID,
			&collection.Address,
			&collection.Name,
			&collection.Symbol,
			&collection.Description,
			&collection.ImageURL,
			&collection.ExternalURL,
			&collection.ContractType,
			&collection.TotalSupply,
			&collection.OwnersCount,
			&collection.NFTsCount,
			&collection.FloorPrice,
			&collection.Volume24h,
			&collection.VolumeTotal,
			&collection.Creator,
			&collection.IsVerified,
			&collection.CreatedAt,
		)
		if err != nil {
			return nil, err
		}
		collections = append(collections, collection)
	}

	return collections, nil
}

// GetNFTsByOwner returns NFTs owned by address.
func (s *Service) GetNFTsByOwner(ctx context.Context, owner string, limit, offset int) ([]*NFT, error) {
	query := `
		SELECT id, address, token_id, owner, creator, name, description,
		       image_url, animation_url, external_url, contract_type,
		       token_uri, block_number, block_hash, transaction_hash, timestamp
		FROM nfts 
		WHERE owner = $1
		ORDER BY block_number DESC
		LIMIT $2 OFFSET $3
	`

	rows, err := s.db.QueryContext(ctx, query, owner, limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var nfts []*NFT
	for rows.Next() {
		nft := &NFT{}
		err := rows.Scan(
			&nft.ID,
			&nft.Address,
			&nft.TokenID,
			&nft.Owner,
			&nft.Creator,
			&nft.Name,
			&nft.Description,
			&nft.ImageURL,
			&nft.AnimationURL,
			&nft.ExternalURL,
			&nft.ContractType,
			&nft.TokenURI,
			&nft.BlockNumber,
			&nft.BlockHash,
			&nft.TransactionHash,
			&nft.Timestamp,
		)
		if err != nil {
			return nil, err
		}
		nfts = append(nfts, nft)
	}

	return nfts, nil
}

// GetTransfers returns NFT transfers.
func (s *Service) GetTransfers(ctx context.Context, address, tokenID string, limit, offset int) ([]*Transfer, error) {
	query := `
		SELECT id, hash, block_number, transaction_hash, from_address, to_address, 
		       token_id, timestamp
		FROM nft_transfers 
		WHERE nft_address = $1 AND token_id = $2
		ORDER BY block_number DESC, id DESC
		LIMIT $3 OFFSET $4
	`

	rows, err := s.db.QueryContext(ctx, query, address, tokenID, limit, offset)
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

// GetHolders returns NFT holders.
func (s *Service) GetHolders(ctx context.Context, address string, limit, offset int) ([]*Holder, error) {
	query := `
		SELECT address, balance, last_update
		FROM nft_holders 
		WHERE collection_address = $1
		ORDER BY balance DESC
		LIMIT $2 OFFSET $3
	`

	rows, err := s.db.QueryContext(ctx, query, address, limit, offset)
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
			&holder.LastUpdate,
		)
		if err != nil {
			return nil, err
		}
		holders = append(holders, holder)
	}

	return holders, nil
}

// SearchCollections searches collections by name or symbol.
func (s *Service) SearchCollections(ctx context.Context, query string, limit int) ([]*Collection, error) {
	searchQuery := `
		SELECT id, address, name, symbol, description, image_url, external_url,
		       contract_type, total_supply, owners_count, nfts_count, floor_price,
		       volume_24h, volume_total, creator, is_verified, created_at
		FROM collections 
		WHERE name ILIKE $1 OR symbol ILIKE $1
		ORDER BY volume_total DESC
		LIMIT $2
	`

	rows, err := s.db.QueryContext(ctx, searchQuery, "%"+query+"%", limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var collections []*Collection
	for rows.Next() {
		collection := &Collection{}
		err := rows.Scan(
			&collection.ID,
			&collection.Address,
			&collection.Name,
			&collection.Symbol,
			&collection.Description,
			&collection.ImageURL,
			&collection.ExternalURL,
			&collection.ContractType,
			&collection.TotalSupply,
			&collection.OwnersCount,
			&collection.NFTsCount,
			&collection.FloorPrice,
			&collection.Volume24h,
			&collection.VolumeTotal,
			&collection.Creator,
			&collection.IsVerified,
			&collection.CreatedAt,
		)
		if err != nil {
			return nil, err
		}
		collections = append(collections, collection)
	}

	return collections, nil
}

// GetTrendingCollections returns trending collections.
func (s *Service) GetTrendingCollections(ctx context.Context, limit int) ([]*Collection, error) {
	query := `
		SELECT id, address, name, symbol, description, image_url, external_url,
		       contract_type, total_supply, owners_count, nfts_count, floor_price,
		       volume_24h, volume_total, creator, is_verified, created_at
		FROM collections 
		ORDER BY volume_24h DESC
		LIMIT $1
	`

	rows, err := s.db.QueryContext(ctx, query, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var collections []*Collection
	for rows.Next() {
		collection := &Collection{}
		err := rows.Scan(
			&collection.ID,
			&collection.Address,
			&collection.Name,
			&collection.Symbol,
			&collection.Description,
			&collection.ImageURL,
			&collection.ExternalURL,
			&collection.ContractType,
			&collection.TotalSupply,
			&collection.OwnersCount,
			&collection.NFTsCount,
			&collection.FloorPrice,
			&collection.Volume24h,
			&collection.VolumeTotal,
			&collection.Creator,
			&collection.IsVerified,
			&collection.CreatedAt,
		)
		if err != nil {
			return nil, err
		}
		collections = append(collections, collection)
	}

	return collections, nil
}

var _ = context.Background // Use context
