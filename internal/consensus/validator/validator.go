// Package validator provides validator management for PoSA consensus.
package validator

import (
	"math/big"
	"sync"

	"github.com/tigersmartchain/tigersmartchain/pkg/crypto"
	"github.com/tigersmartchain/tigersmartchain/pkg/types"
)

// Validator represents a validator in the PoSA consensus.
type Validator struct {
	Address     types.Address
	PublicKey   *crypto.PublicKey
	Stake       *big.Int
	Commission  uint8
	Jailed      bool
	Active      bool
	Delegators  uint64
	Uptime     float64
	LastSigned  uint64
}

// Manager manages validators.
type Manager struct {
	mu                   sync.RWMutex
	validators            map[types.Address]*Validator
	candidateValidators   []types.Address
	activeValidators    []types.Address
	totalStaked         *big.Int
	minStakedAmount    *big.Int
}

// NewManager creates a new validator manager.
func NewManager() *Manager {
	onee18 := new(big.Int).Exp(big.NewInt(10), big.NewInt(18), nil)
	minStake := new(big.Int).Mul(big.NewInt(100), onee18)
	return &Manager{
		validators:         make(map[types.Address]*Validator),
		candidateValidators: []types.Address{},
		activeValidators:  []types.Address{},
		totalStaked:    big.NewInt(0),
		minStakedAmount: minStake,
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