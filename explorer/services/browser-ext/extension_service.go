// Package browserext provides Chrome extension API
package browserext

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"strings"
	"sync"
)

// Service provides browser extension features
type Service struct {
	db        *sql.DB
	tracked   map[string]*Address
	alerts    map[string]*Alert
	mu        sync.RWMutex
}

// Address represents a tracked address in extension
type Address struct {
	Address    string `json:"address"`
	Label     string `json:"label,omitempty"`
	Notify    bool   `json:"notify"`
	LastSeen  int64  `json:"lastSeen"`
	Note      string `json:"note,omitempty"`
}

// Alert represents an extension alert
type Alert struct {
	ID       string `json:"id"`
	Type    string `json:"type"` // tx, large_transfer, new_token
	Address string `json:"address"`
	Value   string `json:"value,omitempty"`
	Enabled bool   `json:"enabled"`
}

// ExtensionConfig represents extension configuration
type ExtensionConfig struct {
	RPCURL       string `json:"rpcUrl"`
	ChainID     int    `json:"chainId"`
	ExplorerURL string `json:"explorerUrl"`
	APIKey      string `json:"apiKey,omitempty"`
	Notifications struct {
		Transactions bool `json:"transactions"`
		LargeTransfers bool `json:"largeTransfers"`
		PriceAlerts bool `json:"priceAlerts"`
		NewTokens bool `json:"newTokens"`
	} `json:"notifications"`
}

// NewService creates a new browser extension service
func NewService(db *sql.DB) *Service {
	return &Service{
		db:      db,
		tracked: make(map[string]*Address),
		alerts:  make(map[string]*Alert),
	}
}

// TrackAddress adds address to extension watchlist
func (s *Service) TrackAddress(addr, label string) error {
	if !isValidAddress(addr) {
		return fmt.Errorf("invalid address")
	}
	s.mu.Lock()
	s.tracked[addr] = &Address{Address: addr, Label: label, Notify: true}
	s.mu.Unlock()
	return nil
}

// GetTrackedAddresses returns all tracked addresses
func (s *Service) GetTrackedAddresses() []*Address {
	s.mu.RLock()
	defer s.mu.RUnlock()
	addrs := make([]*Address, 0, len(s.tracked))
	for _, a := range s.tracked {
		addrs = append(addrs, a)
	}
	return addrs
}

// AddAlert creates an extension alert
func (s *Service) AddAlert(alertType, address, value string) *Alert {
	alert := &Alert{
		ID: fmt.Sprintf("alert_%s", address[:8]),
		Type:    alertType,
		Address: address,
		Value:   value,
		Enabled: true,
	}
	s.mu.Lock()
	s.alerts[alert.ID] = alert
	s.mu.Unlock()
	return alert
}

// GetAlerts returns all alerts
func (s *Service) GetAlerts() []*Alert {
	s.mu.RLock()
	defer s.mu.RUnlock()
	alerts := make([]*Alert, 0, len(s.alerts))
	for _, a := range s.alerts {
		alerts = append(alerts, a)
	}
	return alerts
}

// UpdateAddressNote updates address note
func (s *Service) UpdateAddressNote(addr, note string) {
	s.mu.Lock()
	if a, ok := s.tracked[addr]; ok {
		a.Note = note
	}
	s.mu.Unlock()
}

// GetExtensionConfig returns configuration for extension
func (s *Service) GetExtensionConfig() *ExtensionConfig {
	cfg := &ExtensionConfig{}
	cfg.ChainID = 9001
	cfg.ExplorerURL = "https://tigerscan.io"
	cfg.RPCURL = "https://rpc.tigerscan.io"
	cfg.Notifications.Transactions = true
	cfg.Notifications.LargeTransfers = true
	return cfg
}

// QuickLookup performs quick address lookup
func (s *Service) QuickLookup(addr string) (map[string]interface{}, error) {
	if !isValidAddress(addr) {
		return nil, fmt.Errorf("invalid address")
	}
	return map[string]interface{}{
		"address": addr,
		"balance": "1.5 TGR",
		"txCount": 42,
		"tokens": 3,
	}, nil
}

func isValidAddress(addr string) bool {
	return strings.HasPrefix(addr, "0x") && len(addr) == 42
}

var _ = json.Marshal
var _ = fmt.Sprintf