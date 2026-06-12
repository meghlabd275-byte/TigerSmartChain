// Package aml provides Anti-Money Laundering services
package aml

import (
	"fmt"
	"strings"
	"sync"
	"time"
)

// AMLService provides AML analysis services
type AMLService struct {
	blacklist map[string]*BlacklistEntry
	risks     map[string]*RiskScore
	mu        sync.RWMutex
}

// BlacklistEntry represents a blacklisted entity
type BlacklistEntry struct {
	Address   string    `json:"address"`
	Entity   string    `json:"entity"`
	List     string    `json:"list"`
	Category string    `json:"category"`
	AddedAt  time.Time `json:"addedAt"`
	Reason   string    `json:"reason"`
}

// RiskScore represents risk assessment
type RiskScore struct {
	Address     string    `json:"address"`
	Score      int       `json:"score"` // 0-100
	Level      string    `json:"level"` // low, medium, high, critical
	Factors    []string  `json:"factors"`
	LastUpdate time.Time `json:"lastUpdate"`
}

// TransactionAnalysis represents transaction analysis
type TransactionAnalysis struct {
	Address      string    `json:"address"`
	TxCount     int       `json:"txCount"`
	TotalVolume float64   `json:"totalVolume"`
	RisingScore int       `json:"riskScore"`
	Flags       []string  `json:"flags"`
}

// NewAMLService creates a new AML service
func NewAMLService() *AMLService {
	return &AMLService{
		blacklist: initBlacklist(),
		risks:    make(map[string]*RiskScore),
	}
}

func initBlacklist() map[string]*BlacklistEntry {
	return map[string]*BlacklistEntry{
		"0x0000000000000000000000000000000000000000": {
			Address: "0x0000000000000000000000000000000000000000", Entity: "Zero Address", Category: "system", Reason: "System address"},
	}
}

// CheckAddress checks an address against blacklist
func (s *AMLService) CheckAddress(address string) (*BlacklistEntry, bool) {
	addr := strings.ToLower(address)
	
	s.mu.RLock()
	entry, blocked := s.blacklist[addr]
	s.mu.RUnlock()
	
	return entry, blocked
}

// AssessRisk assesses risk for an address
func (s *AMLService) AssessRisk(address string, txHistory []*Transaction) *RiskScore {
	score := 0
	factors := []string{}
	
	// Check blacklist
	if entry, blocked := s.CheckAddress(address); blocked {
		score += 100
		factors = append(factors, "blacklisted:"+entry.List)
	}
	
	// Analyze transaction patterns
	volume := 0.0
	for _, tx := range txHistory {
		volume += tx.Value
	}
	
	if volume > 10000000 {
		score += 20
		factors = append(factors, "high_volume")
	}
	
	// Determine risk level
	level := "low"
	if score >= 80 {
		level = "critical"
	} else if score >= 60 {
		level = "high"
	} else if score >= 40 {
		level = "medium"
	}
	
	risk := &RiskScore{
		Address: address,
		Score: score,
		Level: level,
		Factors: factors,
		LastUpdate: time.Now(),
	}
	
	s.mu.Lock()
	s.risks[address] = risk
	s.mu.Unlock()
	
	return risk
}

// GetRiskScore gets risk score for an address
func (s *AMLService) GetRiskScore(address string) *RiskScore {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	return s.risks[address]
}

// AddToBlacklist adds address to blacklist
func (s *AMLService) AddToBlacklist(entry *BlacklistEntry) error {
	s.mu.Lock()
	s.blacklist[strings.ToLower(entry.Address)] = entry
	s.mu.Unlock()
	return nil
}

// Transaction represents a transaction for analysis
type Transaction struct {
	Hash   string
	Value  float64
	From   string
	To    string
	Block uint64
}

// InitAMLService initializes the service
func InitAMLService() (*AMLService, error) {
	return NewAMLService(), nil
}