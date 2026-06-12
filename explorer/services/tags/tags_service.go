// Package tags provides community tagging and label services
package tags

import (
	"fmt"
	"strings"
	"sync"
	"time"
)

// TagsService provides community tagging services
type TagsService struct {
	tags      map[string]*Tag
	addresses map[string][]*Tag
	mu       sync.RWMutex
}

// Tag represents a tag/label
type Tag struct {
	ID        string    `json:"id"`
	Name      string    `json:"name"`
	Category string    `json:"category"`
	Color    string    `json:"color"`
	Address  string    `json:"address"`
	Votes    int       `json:"votes"`
	Verified bool      `json:"verified"`
	CreatedBy string  `json:"createdBy"`
	CreatedAt time.Time `json:"createdAt"`
}

// TagCategory represents tag categories
type TagCategory struct {
	ID          string `json:"id"`
	Name        string `json:"name"`
	Description string `json:"description"`
	Color       string `json:"color"`
}

// Vote represents a vote for a tag
type Vote struct {
	ID        string    `json:"id"`
	TagID    string    `json:"tagId"`
	Voter    string    `json:"voter"`
	VoteType int       `json:"voteType"` // 1 = upvote, -1 = downvote
	CreatedAt time.Time `json:"createdAt"`
}

// NewTagsService creates a new tags service
func NewTagsService() *TagsService {
	return &TagsService{
		tags:      initDefaultTags(),
		addresses: make(map[string][]*Tag),
	}
}

// initDefaultTags initializes default tags
func initDefaultTags() map[string]*Tag {
	return map[string]*Tag{
		"uniswap": {
			ID: "tag_001",
			Name: "Uniswap",
			Category: "defi",
			Color: "#FF007A",
			Address: "0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984",
			Verified: true,
		},
		"aave": {
			ID: "tag_002",
			Name: "Aave",
			Category: "defi",
			Color: "#2EBAC6",
			Address: "0x7Fc66500c84A76Ad7e9c93437bFc5Ac33E2DDaE9",
			Verified: true,
		},
	}
}

// CreateTag creates a new tag
func (s *TagsService) CreateTag(name, category, address, createdBy string) (*Tag, error) {
	tag := &Tag{
		ID: generateTagID(),
		Name: name,
		Category: category,
		Address: address,
		Votes: 1,
		Verified: false,
		CreatedBy: createdBy,
		CreatedAt: time.Now(),
	}
	
	s.mu.Lock()
	s.tags[tag.ID] = tag
	
	// Add to address index
	addrKey := strings.ToLower(address)
	s.addresses[addrKey] = append(s.addresses[addrKey], tag)
	s.mu.Unlock()
	
	return tag, nil
}

// GetTagsForAddress gets tags for an address
func (s *TagsService) GetTagsForAddress(address string) []*Tag {
	addrKey := strings.ToLower(address)
	
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	return s.addresses[addrKey]
}

// GetAllTags gets all tags
func (s *TagsService) GetAllTags() []*Tag {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	tags := make([]*Tag, 0, len(s.tags))
	for _, tag := range s.tags {
		tags = append(tags, tag)
	}
	
	return tags
}

// GetTagsByCategory gets tags by category
func (s *TagsService) GetTagsByCategory(category string) []*Tag {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	var result []*Tag
	for _, tag := range s.tags {
		if strings.ToLower(tag.Category) == strings.ToLower(category) {
			result = append(result, tag)
		}
	}
	
	return result
}

// VoteTag votes for a tag
func (s *TagsService) VoteTag(tagID, voter string, voteType int) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	tag, ok := s.tags[tagID]
	if !ok {
		return fmt.Errorf("tag not found")
	}
	
	// Update vote count
	tag.Votes += voteType
	
	// Check for verification threshold
	if tag.Votes > 100 && !tag.Verified {
		tag.Verified = true
	}
	
	return nil
}

// GetVerifiedTags gets verified tags
func (s *TagsService) GetVerifiedTags() []*Tag {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	var result []*Tag
	for _, tag := range s.tags {
		if tag.Verified {
			result = append(result, tag)
		}
	}
	
	return result
}

// SearchTags searches tags by name
func (s *TagsService) SearchTags(query string) []*Tag {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	query = strings.ToLower(query)
	var result []*Tag
	
	for _, tag := range s.tags {
		if strings.Contains(strings.ToLower(tag.Name), query) {
			result = append(result, tag)
		}
	}
	
	return result
}

// GetCategories gets all categories
func (s *TagsService) GetCategories() []*TagCategory {
	return []*TagCategory{
		{ID: "defi", Name: "DeFi", Description: "Decentralized Finance", Color: "#FF007A"},
		{ID: "nft", Name: "NFT", Description: "Non-Fungible Tokens", Color: "#8B5CF6"},
		{ID: "bridge", Name: "Bridge", Description: "Cross-chain Bridges", Color: "#3B82F6"},
		{ID: "cex", Name: "CEX", Description: "Centralized Exchange", Color: "#10B981"},
		{ID: "dao", Name: "DAO", Description: "Decentralized Organization", Color: "#F59E0B"},
		{ID: "multisig", Name: "Multisig", Description: "Multi-signature Wallet", Color: "#EC4899"},
		{ID: "attacker", Name: "Attacker", Description: "Known Attacker", Color: "#EF4444"},
		{ID: "phish", Name: "Phishing", Description: "Phishing Contract", Color: "#DC2626"},
	}
}

// ReportTag reports a tag as inappropriate
func (s *TagsService) ReportTag(tagID, reporter, reason string) error {
	// In production, would create report record
	return nil
}

// DeleteTag deletes a tag
func (s *TagsService) DeleteTag(tagID string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	if _, ok := s.tags[tagID]; !ok {
		return fmt.Errorf("tag not found")
	}
	
	delete(s.tags, tagID)
	return nil
}

// GetTopTags gets top voted tags
func (s *TagsService) GetTopTags(limit int) []*Tag {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	tags := make([]*Tag, 0, len(s.tags))
	for _, tag := range s.tags {
		tags = append(tags, tag)
	}
	
	// Sort by votes
	for i := 0; i < len(tags); i++ {
		for j := i + 1; j < len(tags); j++ {
			if tags[j].Votes > tags[i].Votes {
				tags[i], tags[j] = tags[j], tags[i]
			}
		}
	}
	
	if len(tags) > limit {
		tags = tags[:limit]
	}
	
	return tags
}

// generateTagID generates a tag ID
func generateTagID() string {
	return fmt.Sprintf("tag_%d", time.Now().UnixNano())
}

// InitTagsService initializes the service
func InitTagsService() (*TagsService, error) {
	return NewTagsService(), nil
}