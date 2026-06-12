// Package privacy provides privacy and security services for the blockchain explorer
// This service handles address obfuscation, transaction privacy, and anti-fraud detection
package privacy

import (
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"strings"
	"time"
)

// PrivacyService handles address and transaction privacy features
type PrivacyService struct {
	encryptionKey []byte
	maskEnabled  bool
}

// MaskedAddress represents a masked address for privacy display
type MaskedAddress struct {
	Original  string `json:"original"`
	Masked    string `json:"masked"`
	Hashed    string `json:"hashed"`
	CanReveal bool   `json:"canReveal"`
}

// PrivacyTransaction represents a transaction with privacy features
type PrivacyTransaction struct {
	Hash             string    `json:"hash"`
	From            string    `json:"from"`
	To              string    `json:"to"`
	Value           string    `json:"value"`
	IsPrivacyTx      bool      `json:"isPrivacyTx"`
	IsMixerRelated  bool      `json:"isMixerRelated"`
	IsTornadoCash   bool      `json:"isTornadoCash"`
	IsCoinJoin      bool      `json:"isCoinJoin"`
	RiskLevel       RiskLevel `json:"riskLevel"`
	IsHighRisk      bool      `json:"isHighRisk"`
	SuspiciousTypes []string  `json:"suspiciousTypes,omitempty"`
}

// RiskLevel represents transaction risk assessment
type RiskLevel int

const (
	RiskLow RiskLevel = iota
	RiskMedium
	RiskHigh
	RiskCritical
)

// SanctionedAddress represents a known sanctioned address
type SanctionedAddress struct {
	Address    string    `json:"address"`
	Entity    string    `json:"entity"`
	List      string    `json:"list"`
	AddedAt   time.Time `json:"addedAt"`
	Category  string    `json:"category"`
}

// NewPrivacyService creates a new privacy service
func NewPrivacyService() (*PrivacyService, error) {
	key := make([]byte, 32)
	if _, err := rand.Read(key); err != nil {
		return nil, fmt.Errorf("failed to generate key: %w", err)
	}
	
	return &PrivacyService{
		encryptionKey: key,
		maskEnabled:   true,
	}, nil
}

// NewPrivacyServiceWithKey creates a service with existing key
func NewPrivacyServiceWithKey(keyHex string) (*PrivacyService, error) {
	key, err := hex.DecodeString(strings.TrimPrefix(keyHex, "0x"))
	if err != nil {
		return nil, fmt.Errorf("invalid key: %w", err)
	}
	
	if len(key) != 32 {
		return nil, errors.New("key must be 32 bytes")
	}
	
	return &PrivacyService{
		encryptionKey: key,
		maskEnabled:   true,
	}, nil
}

// MaskAddress creates a masked version of an address
func (p *PrivacyService) MaskAddress(address string) (*MaskedAddress, error) {
	address = strings.ToLower(address)
	
	if !strings.HasPrefix(address, "0x") {
		address = "0x" + address
	}
	
	hashed := sha256.Sum256([]byte(address))
	hashedHex := hex.EncodeToString(hashed[:])
	
	masked := address[:6] + "..." + address[len(address)-4:]
	
	return &MaskedAddress{
		Original:  address,
		Masked:   masked,
		Hashed:   hashedHex[:16],
		CanReveal: true,
	}, nil
}

// EncryptAddress encrypts an address for secure storage
func (p *PrivacyService) EncryptAddress(address string) (string, error) {
	block, err := aes.NewCipher(p.encryptionKey)
	if err != nil {
		return "", err
	}
	
	address = strings.ToLower(address)
	if !strings.HasPrefix(address, "0x") {
		address = "0x" + address
	}
	
	// Pad address to block size
	padded := make([]byte, aes.BlockSize)
	copy(padded, address)
	
	ciphertext := make([]byte, aes.BlockSize)
	block.Encrypt(ciphertext, padded)
	
	return hex.EncodeToString(ciphertext), nil
}

// DecryptAddress decrypts an address
func (p *PrivacyService) DecryptAddress(encrypted string) (string, error) {
	encrypted = strings.TrimPrefix(encrypted, "0x")
	
	data, err := hex.DecodeString(encrypted)
	if err != nil {
		return "", err
	}
	
	block, err := aes.NewCipher(p.encryptionKey)
	if err != nil {
		return "", err
	}
	
	if len(data) != aes.BlockSize {
		return "", errors.New("invalid encrypted data")
	}
	
	plaintext := make([]byte, aes.BlockSize)
	block.Decrypt(plaintext, data)
	
	// Remove padding
	return strings.Trim(string(plaintext), "\x00"), nil
}

// AnalyzeTransactionRisk analyzes a transaction for risk factors
func (p *PrivacyService) AnalyzeTransactionRisk(tx *PrivacyTransaction) *PrivacyTransaction {
	if tx == nil {
		return tx
	}
	
	riskScore := 0
	tx.SuspiciousTypes = []string{}
	
	// Check for mixer/tornado cash related
	if tx.IsMixerRelated || tx.IsTornadoCash {
		riskScore += 2
		tx.SuspiciousTypes = append(tx.SuspiciousTypes, "mixer")
	}
	
	// Check for CoinJoin
	if tx.IsCoinJoin {
		riskScore += 1
		tx.SuspiciousTypes = append(tx.SuspiciousTypes, "coinjoin")
	}
	
	// Determine risk level
	switch {
	case riskScore >= 3:
		tx.RiskLevel = RiskCritical
		tx.IsHighRisk = true
	case riskScore == 2:
		tx.RiskLevel = RiskHigh
		tx.IsHighRisk = true
	case riskScore == 1:
		tx.RiskLevel = RiskMedium
	default:
		tx.RiskLevel = RiskLow
	}
	
	return tx
}

// CheckSanctionedAddress checks if an address is sanctioned
func (p *PrivacyService) CheckSanctionedAddress(address string) (*SanctionedAddress, bool) {
	address = strings.ToLower(address)
	
	// Known sanctioned addresses (in production would query database)
	sanctioned := map[string]SanctionedAddress{
		"0x0000000000000000000000000000000000000000": {
			Address:   "0x0000000000000000000000000000000000000000",
			Entity:   "Sample",
			List:     "OFAC",
			Category: "test",
		},
	}
	
	addr, exists := sanctioned[address]
	return &addr, exists
}

// CreateWhitelistEntry creates an encrypted whitelist entry
func (p *PrivacyService) CreateWhitelistEntry(address, label string) (*WhitelistEntry, error) {
	encrypted, err := p.EncryptAddress(address)
	if err != nil {
		return nil, err
	}
	
	hashed := sha256.Sum256([]byte(strings.ToLower(address)))
	hashedStr := hex.EncodeToString(hashed[:])
	
	return &WhitelistEntry{
		ID:          hashedStr[:16],
		AddressHash: encrypted,
		Label:      label,
		CreatedAt:   time.Now(),
	}, nil
}

// WhitelistEntry represents an address in a whitelist
type WhitelistEntry struct {
	ID          string    `json:"id"`
	AddressHash string    `json:"addressHash"`
	Label      string    `json:"label"`
	CreatedAt   time.Time `json:"createdAt"`
}

// HideTransaction creates a privacy-hidden transaction view
func (p *PrivacyService) HideTransaction(tx *PrivacyTransaction) *PrivacyTransaction {
	if tx == nil {
		return nil
	}
	
	maskedFrom, _ := p.MaskAddress(tx.From)
	maskedTo, _ := p.MaskAddress(tx.To)
	
	return &PrivacyTransaction{
		Hash:        tx.Hash[:8] + "...",
		From:       maskedFrom.Masked,
		To:         maskedTo.Masked,
		Value:      tx.Value,
		IsPrivacyTx: tx.IsPrivacyTx,
		RiskLevel:  tx.RiskLevel,
	}
}

// EnablePrivacy enables privacy masking
func (p *PrivacyService) EnablePrivacy() {
	p.maskEnabled = true
}

// DisablePrivacy disables privacy masking
func (p *PrivacyService) DisablePrivacy() {
	p.maskEnabled = false
}

// IsPrivacyEnabled returns privacy status
func (p *PrivacyService) IsPrivacyEnabled() bool {
	return p.maskEnabled
}

// InitPrivacyService initializes the privacy service
func InitPrivacyService() (*PrivacyService, error) {
	return NewPrivacyService()
}