// Package validator provides validator management for PoSA consensus.
package validator

import (
	"math/big"
	"sync"
	"time"

	"github.com/tigersmartchain/tigersmartchain/pkg/crypto"
	"github.com/tigersmartchain/tigersmartchain/pkg/types"
)

// =============================================================================
// FERMI UPGRADE VALIDATOR REQUIREMENTS (2026)
// =============================================================================

// BNB Chain 2026 Fermi Validator Requirements
const (
	// Minimum self-stake (100 BNB minimum after Fermi)
	FermiMinSelfStake = 100000000000000000000 // 100 BNB in wei
	
	// Minimum total stake (10,000 BNB)
	FermiMinTotalStake = 10000000000000000000000 // 10,000 BNB in wei
	
	// Commission range (0-20%)
	FermiMinCommission = 0
	FermiMaxCommission = 20
	
	// Maximum validators (45+)
	FermiMaxValidators = 50
	FermiActiveValidators = 45
	
	// Self-stake requirement (must stake own 100+ BNB)
	FermiMinSelfStakePercent = 1 // 1% of total stake must be self-staked
)

// Validator represents a validator in the PoSA consensus.
type Validator struct {
	Address     types.Address
	PublicKey   *crypto.PublicKey
	Stake       *big.Int
	SelfStake   *big.Int // New: Self-stake amount
	Commission  uint8
	Jailed      bool
	Active      bool
	Delegators  uint64
	Uptime     float64
	LastSigned  uint64
	
	// Fermi requirements
	SelfStakePercent float64 // New: Percentage of stake that is self-staked
	MinSelfStakeMet bool    // New: Whether minimum self-stake requirement is met
}

// Manager manages validators.
type Manager struct {
	mu                   sync.RWMutex
	validators            map[types.Address]*Validator
	candidateValidators   []types.Address
	activeValidators    []types.Address
	totalStaked         *big.Int
	minStakedAmount    *big.Int
	
	// Fermi upgrade
	minSelfStakeAmount *big.Int // New: Minimum self-stake (100 BNB)
	maxCommission     uint8    // New: Maximum commission (20%)
	minCommission     uint8    // New: Minimum commission (0%)
}

// NewManager creates a new validator manager with Fermi requirements.
func NewManager() *Manager {
	onee18 := new(big.Int).Exp(big.NewInt(10), big.NewInt(18), nil)
	minStake := new(big.Int).Mul(big.NewInt(100), onee18) // Original: 100 BNB
	
	// Fermi: 100 BNB minimum self-stake
	minSelfStake := new(big.Int).Mul(big.NewInt(100), onee18)
	
	return &Manager{
		validators:         make(map[types.Address]*Validator),
		candidateValidators: []types.Address{},
		activeValidators:  []types.Address{},
		totalStaked:    big.NewInt(0),
		minStakedAmount: minStake,
		minSelfStakeAmount: minSelfStake, // Fermi requirement
		maxCommission:     FermiMaxCommission,
		minCommission:     FermiMinCommission,
	}
}

// AddValidator adds a new validator.
func (m *Manager) AddValidator(v *Validator) error {
	if v.Stake.Cmp(m.minStakedAmount) < 0 {
		return ErrInsufficientStake
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	m.validators[v.Address] = v
	m.totalStaked.Add(m.totalStaked, v.Stake)
	return nil
}

// RemoveValidator removes a validator.
func (m *Manager) RemoveValidator(addr types.Address) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	if v, ok := m.validators[addr]; ok {
		m.totalStaked.Sub(m.totalStaked, v.Stake)
		delete(m.validators, addr)
	}
	return nil
}

// GetValidator returns a validator by address.
func (m *Manager) GetValidator(addr types.Address) *Validator {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.validators[addr]
}

// GetActiveValidators returns all active validators.
func (m *Manager) GetActiveValidators() []*Validator {
	m.mu.RLock()
	defer m.mu.RUnlock()
	result := make([]*Validator, 0)
	for _, v := range m.validators {
		if v.Active && !v.Jailed {
			result = append(result, v)
		}
	}
	return result
}

// TotalStaked returns the total staked amount.
func (m *Manager) TotalStaked() *big.Int {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.totalStaked
}

var ErrInsufficientStake = &ValidatorError{"insufficient stake"}

type ValidatorError struct {
	msg string
}

func (e *ValidatorError) Error() string {
	return e.msg
}