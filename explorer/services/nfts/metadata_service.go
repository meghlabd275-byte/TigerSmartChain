// Package metadata provides NFT metadata service for TigerScan.
package metadata

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"math/big"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/tigersmartchain/tigersmartchain/explorer/databases/postgres"
)

// =============================================================================
// NFT METADATA SERVICE
// =============================================================================

// Service provides NFT metadata fetching and caching
type Service struct {
	db           *postgres.DB
	mu           sync.RWMutex
	metadataCache map[string]*NFTTokenMetadata
	httpClient   *http.Client
	baseURLs    []string
}

// NFTTokenMetadata represents NFT metadata
type NFTTokenMetadata struct {
	Name        string            `json:"name"`
	Description string            `json:"description"`
	ImageURL   string            `json:"image_url"`
	ExternalURL string          `json:"external_url"`
	AnimationURL string         `json:"animation_url"`
	Attributes []NFTAttribute    `json:"attributes"`
	BackgroundColor string     `json:"background_color"`
	YoutubeURL string         `json:"youtube_url"`
	IPFSGateway string        `json:"ipfs_gateway"`
	RawJSON    json.RawMessage `json:"raw_json"`
	FetchedAt time.Time       `json:"fetched_at"`
}

// NFTAttribute represents an NFT attribute/trait
type NFTAttribute struct {
	TraitType   string      `json:"trait_type"`
	Value       interface{} `json:"value"`
	DisplayType string      `json:"display_type,omitempty"`
}

// CollectionMetadata represents collection metadata
type CollectionMetadata struct {
	Name        string `json:"name"`
	Symbol      string `json:"symbol"`
	Description string `json:"description"`
	ImageURL   string `json:"image_url"`
	ExternalURL string `json:"external_url"`
	BannerURL  string `json:"banner_url"`
	FloorPrice string `json:"floor_price"`
}

// NewService creates a new metadata service
func NewService(db *postgres.DB) *Service {
	return &Service{
		db:            db,
		metadataCache: make(map[string]*NFTTokenMetadata),
		baseURLs: []string{
			"https://ipfs.io/ipfs/",
			"https://gateway.pinata.cloud/ipfs/",
			"https://nftstorage.link/ipfs/",
		},
		httpClient: &http.Client{
			Timeout: 10 * time.Second,
		},
	}
}

// =============================================================================
// METADATA FETCHING
// =============================================================================

// FetchMetadata fetches metadata for an NFT
func (s *Service) FetchMetadata(ctx context.Context, tokenURI string) (*NFTTokenMetadata, error) {
	// Check cache first
	s.mu.RLock()
	if cached, ok := s.metadataCache[tokenURI]; ok {
		s.mu.RUnlock()
		if time.Since(cached.FetchedAt) < 24*time.Hour {
			return cached, nil
		}
	}
	s.mu.RUnlock()

	// Fetch from URI
	metadata, err := s.fetchFromURI(ctx, tokenURI)
	if err != nil {
		return nil, err
	}

	// Update cache
	s.mu.Lock()
	s.metadataCache[tokenURI] = metadata
	s.mu.Unlock()

	return metadata, nil
}

// fetchFromURI fetches metadata from URI
func (s *Service) fetchFromURI(ctx context.Context, tokenURI string) (*NFTTokenMetadata, error) {
	var data []byte
	var err error

	// Handle different URI schemes
	if strings.HasPrefix(tokenURI, "ipfs://") {
		// Fetch from IPFS
		ipfsHash := strings.TrimPrefix(tokenURI, "ipfs://")
		data, err = s.fetchFromIPFS(ctx, ipfsHash)
	} else if strings.HasPrefix(tokenURI, "ar://") {
		// Fetch from Arweave
		arHash := strings.TrimPrefix(tokenURI, "ar://")
		data, err = s.fetchFromArweave(ctx, arHash)
	} else if strings.HasPrefix(tokenURI, "http://") || strings.HasPrefix(tokenURI, "https://") {
		// Fetch from HTTP
		data, err = s.fetchFromHTTP(ctx, tokenURI)
	} else {
		return nil, fmt.Errorf("unsupported URI scheme: %s", tokenURI)
	}

	if err != nil {
		return nil, err
	}

	// Parse metadata
	var metadata NFTTokenMetadata
	if err := json.Unmarshal(data, &metadata); err != nil {
		return nil, fmt.Errorf("failed to parse metadata: %w", err)
	}

	// Process image URL
	if metadata.ImageURL != "" {
		metadata.ImageURL = s.processMediaURL(metadata.ImageURL)
	}

	// Process animation URL
	if metadata.AnimationURL != "" {
		metadata.AnimationURL = s.processMediaURL(metadata.AnimationURL)
	}

	metadata.FetchedAt = time.Now()
	metadata.RawJSON = data

	return &metadata, nil
}

// fetchFromIPFS fetches data from IPFS
func (s *Service) fetchFromIPFS(ctx context.Context, hash string) ([]byte, error) {
	// Try different gateways
	for _, gateway := range s.baseURLs {
		url := gateway + hash
		resp, err := s.httpClient.Get(url)
		if err != nil {
			continue
		}
		defer resp.Body.Close()

		if resp.StatusCode == http.StatusOK {
			return io.ReadAll(resp.Body)
		}
	}

	return nil, fmt.Errorf("failed to fetch from IPFS: %s", hash)
}

// fetchFromArweave fetches data from Arweave
func (s *Service) fetchFromArweave(ctx context.Context, txID string) ([]byte, error) {
	url := fmt.Sprintf("https://arweave.net/%s", txID)
	resp, err := s.httpClient.Get(url)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("arweave request failed: %d", resp.StatusCode)
	}

	return io.ReadAll(resp.Body)
}

// fetchFromHTTP fetches data from HTTP URL
func (s *Service) fetchFromHTTP(ctx context.Context, url string) ([]byte, error) {
	req, err := http.NewRequestWithContext(ctx, "GET", url, nil)
	if err != nil {
		return nil, err
	}

	req.Header.Set("User-Agent", "TigerScan/1.0")

	resp, err := s.httpClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("HTTP request failed: %d", resp.StatusCode)
	}

	return io.ReadAll(resp.Body)
}

// processMediaURL processes and resolves media URLs
func (s *Service) processMediaURL(url string) string {
	if strings.HasPrefix(url, "ipfs://") {
		hash := strings.TrimPrefix(url, "ipfs://")
		return s.baseURLs[0] + hash
	}
	return url
}

// =============================================================================
// METADATA STORAGE
// =============================================================================

// StoreMetadata stores NFT metadata in database
func (s *Service) StoreMetadata(ctx context.Context, collection, tokenID string, metadata *NFTTokenMetadata) error {
	// Convert to JSON
	attributesJSON, _ := json.Marshal(metadata.Attributes)

	// Get existing NFT
	nft, err := s.db.GetNFT(ctx, collection, tokenID)
	if err != nil {
		return err
	}

	if nft == nil {
		return fmt.Errorf("NFT not found: %s/%s", collection, tokenID)
	}

	// Update NFT with metadata
	nft.Name = metadata.Name
	nft.Description = &metadata.Description
	nft.ImageURL = &metadata.ImageURL
	nft.ExternalURL = &metadata.ExternalURL
	nft.AnimationURL = &metadata.AnimationURL
	nft.Attributes = &attributesJSON

	return s.db.InsertNFT(ctx, &postgres.NFTToken{
		Address:     collection,
		TokenID:     tokenID,
		Name:        nft.Name,
		Description: nft.Description,
		ImageURL:   nft.ImageURL,
		ExternalURL: nft.ExternalURL,
		AnimationURL: nft.AnimationURL,
		Attributes:  string(attributesJSON),
	})
}

// =============================================================================
// BATCH FETCHING
// =============================================================================

// BatchFetchMetadata fetches metadata for multiple NFTs
func (s *Service) BatchFetchMetadata(ctx context.Context, items []MetadataRequest) map[string]*NFTTokenMetadata {
	results := make(map[string]*NFTTokenMetadata)

	// Process in parallel
	var wg sync.WaitGroup
	var mu sync.Mutex

	for _, item := range items {
		wg.Add(1)
		go func(req MetadataRequest) {
			defer wg.Done()

			metadata, err := s.FetchMetadata(ctx, req.TokenURI)
			if err != nil {
				return
			}

			mu.Lock()
			results[req.Key] = metadata
			mu.Unlock()
		}(item)
	}

	wg.Wait()

	return results
}

// MetadataRequest represents a metadata fetch request
type MetadataRequest struct {
	Collection string
	TokenID   string
	TokenURI  string
	Key      string
}

// =============================================================================
// COLLECTION METADATA
// =============================================================================

// FetchCollectionMetadata fetches metadata for a collection
func (s *Service) FetchCollectionMetadata(ctx context.Context, contractAddress string) (*CollectionMetadata, error) {
	// Get collection from database
	collection, err := s.db.GetCollection(ctx, contractAddress)
	if err != nil {
		return nil, err
	}
	if collection == nil {
		return nil, fmt.Errorf("collection not found")
	}

	metadata := &CollectionMetadata{
		Name:        collection.Name,
		Symbol:      *collection.Symbol,
		Description: *collection.Description,
		ImageURL:   *collection.ImageURL,
		ExternalURL: *collection.ExternalURL,
		FloorPrice: *collection.FloorPrice,
	}

	return metadata, nil
}

// UpdateCollectionStats updates collection statistics
func (s *Service) UpdateCollectionStats(ctx context.Context, contractAddress string) error {
	// Would calculate stats from NFT transfers
	// - Total volume
	// - Floor price
	// - Unique owners
	// - etc.
	return nil
}

// =============================================================================
// FLOOR PRICE CALCULATION
// =============================================================================

// CalculateFloorPrice calculates floor price for a collection
func (s *Service) CalculateFloorPrice(ctx context.Context, contractAddress string) (string, error) {
	// Get recent transfers
	transfers, err := s.db.GetNFTTransfers(ctx, contractAddress, 100, 0)
	if err != nil {
		return "0", err
	}

	if len(transfers) == 0 {
		return "0", nil
	}

	// Find minimum price
	var minPrice *big.Int
	for _, transfer := range transfers {
		price := new(big.Int)
		// Price would be in the transfer value
		if minPrice == nil || price.Cmp(minPrice) < 0 {
			minPrice = price
		}
	}

	if minPrice == nil {
		return "0", nil
	}

	return minPrice.String(), nil
}

// =============================================================================
// ROYALTY INFO
// =============================================================================

// RoyaltyInfo represents royalty information
type RoyaltyInfo struct {
	Recipient string  `json:"recipient"`
	RoyaltyBPS int    `json:"royaltyBPS"` // Basis points
}

// GetRoyaltyInfo retrieves royalty info for an NFT
func (s *Service) GetRoyaltyInfo(ctx context.Context, contractAddress string) (*RoyaltyInfo, error) {
	// Would query contract for EIP-2981 royalty info
	// For now, return from database if available
	collection, err := s.db.GetCollection(ctx, contractAddress)
	if err != nil {
		return nil, err
	}

	return &RoyaltyInfo{
		Recipient: collection.Creator,
		RoyaltyBPS: collection.RoyaltyBPS,
	}, nil
}

// =============================================================================
// IPFS GATEWAY MANAGEMENT
// =============================================================================

// AddGateway adds a new IPFS gateway
func (s *Service) AddGateway(gateway string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.baseURLs = append(s.baseURLs, gateway)
}

// RemoveGateway removes an IPFS gateway
func (s *Service) RemoveGateway(gateway string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	newURLs := make([]string, 0)
	for _, url := range s.baseURLs {
		if url != gateway {
			newURLs = append(newURLs, url)
		}
	}
	s.baseURLs = newURLs
}

// GetGateways returns list of IPFS gateways
func (s *Service) GetGateways() []string {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	urls := make([]string, len(s.baseURLs))
	copy(urls, s.baseURLs)
	return urls
}

// =============================================================================
// CACHE MANAGEMENT
// =============================================================================

// ClearCache clears the metadata cache
func (s *Service) ClearCache() {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.metadataCache = make(map[string]*NFTTokenMetadata)
}

// GetCacheSize returns the number of cached items
func (s *Service) GetCacheSize() int {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return len(s.metadataCache)
}
