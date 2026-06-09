// Package delegation provides validator delegation for TigerSmartChain.
// This allows token holders to delegate their stake to validators.

package delegation

import (
	"encoding/json"
	"fmt"
	"math/big"
	"sync"
	"time"

	"github.com/ethereum/go-ethereum/common"
)

// Delegation represents a delegation from a delegator to a validator
type Delegation struct {
	Delegator    common.Address `json:"delegator"`
	Validator   common.Address `json:"validator"`
	Amount     *big.Int    `json:"amount"`
	LockEndTime uint64      `json:"lockEndTime"`
	IsRedelegation bool      `json:"isRedelegation"`
	CreatedAt   uint64      `json:"createdAt"`
}

// Manager manages validator delegations
type Manager struct {
	mu          sync.RWMutex
	delegations map[common.Address]map[common.Address]*Delegation // delegator -> validator -> delegation
	byValidator map[common.Address][]common.Address // validator -> delegators
	totalStaked map[common.Address]*big.Int // validator -> total staked amount
	config     *Config
}

// Config holds delegation configuration
type Config struct {
	MinDelegation     *big.Int    `json:"minDelegation"`
	LockPeriod      uint64      `json:"lockPeriod"`       // seconds
	MaxLockPeriod   uint64      `json:"maxLockPeriod"`   // seconds
	UnbondPeriod   uint64      `json:"unbondPeriod"`   // seconds
	CommissionMin *big.Int    `json:"commissionMin"`
	CommissionMax *big.Int    `json:"commissionMax"`
}

// NewManager creates a new delegation manager
func NewManager(cfg *Config) *Manager {
	if cfg == nil {
		cfg = DefaultConfig()
	}

	return &Manager{
		delegations: make(map[common.Address]map[common.Address]*Delegation),
		byValidator: make(map[common.Address][]common.Address),
		totalStaked: make(map[common.Address]*big.Int),
		config:     cfg,
	}
}

// DefaultConfig returns default delegation config
func DefaultConfig() *Config {
	return &Config{
		MinDelegation:     big.NewInt(100),        // Min 100 TGR
		LockPeriod:        7 * 24 * 3600,      // 7 days
		MaxLockPeriod:     365 * 24 * 3600,    // 365 days
		UnbondPeriod:     14 * 24 * 3600,     // 14 days
		CommissionMin:    big.NewInt(0),          // 0%
		CommissionMax:    big.NewInt(2000),        // 20%
	}
}

// Delegate creates a new delegation
func (m *Manager) Delegate(delegator, validator common.Address, amount *big.Int, lockPeriod uint64) (*Delegation, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	// Validation
	if amount.Cmp(m.config.MinDelegation) < 0 {
		return nil, fmt.Errorf("amount below minimum delegation: %s", m.config.MinDelegation)
	}

	if lockPeriod < m.config.LockPeriod {
		return nil, fmt.Errorf("lock period too short: minimum %d seconds", m.config.LockPeriod)
	}

	if lockPeriod > m.config.MaxLockPeriod {
		return nil, fmt.Errorf("lock period too long: maximum %d seconds", m.config.MaxLockPeriod)
	}

	// Check for existing delegation
	existing := m.delegations[delegator]
	isRedelegation := false

	if existing != nil {
		// Check if delegating to same validator
		if existing[validator] != nil {
			return nil, fmt.Errorf("already delegated to this validator")
		}
		isRedelegation = true

		// Cannot redelegate during lock period
		if uint64(time.Now().Unix()) < existing[validator].LockEndTime {
			return nil, fmt.Errorf("cannot redelegate during lock period")
		}
	}

	delegation := &Delegation{
		Delegator:    delegator,
		Validator:  validator,
		Amount:    amount,
		LockEndTime: uint64(time.Now().Unix()) + lockPeriod,
		IsRedelegation: isRedelegation,
		CreatedAt:  uint64(time.Now().Unix()),
	}

	// Store delegation
	if m.delegations[delegator] == nil {
		m.delegations[delegator] = make(map[common.Address]*Delegation)
	}
	m.delegations[delegator][validator] = delegation

	// Update validator index
	found := false
	for _, d := range m.byValidator[validator] {
		if d == delegator {
			found = true
			break
		}
	}
	if !found {
		m.byValidator[validator] = append(m.byValidator[validator], delegator)
	}

	// Update total staked
	if m.totalStaked[validator] == nil {
		m.totalStaked[validator] = big.NewInt(0)
	}
	m.totalStaked[validator].Add(m.totalStaked[validator], amount)

	return delegation, nil
}

// Undelegate removes a delegation
func (m *Manager) Undelegate(delegator, validator common.Address) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	delegations := m.delegations[delegator]
	if delegations == nil {
		return fmt.Errorf("no delegation found")
	}

	delegation := delegations[validator]
	if delegation == nil {
		return fmt.Errorf("delegation to this validator not found")
	}

	// Check lock period
	if uint64(time.Now().Unix()) < delegation.LockEndTime {
		return fmt.Errorf("lock period not ended")
	}

	// Reduce total staked
	m.totalStaked[validator].Sub(m.totalStaked[validator], delegation.Amount)

	// Remove delegation
	delete(delegations, delegator)

	// Remove from validator index
	delegators := m.byValidator[validator]
	for i, d := range delegators {
		if d == delegator {
			m.byValidator[validator] = append(delegators[:i], delegators[i+1:]...)
			break
		}
	}

	return nil
}

// GetDelegation returns delegation for a delegator/validator pair
func (m *Manager) GetDelegation(delegator, validator common.Address) (*Delegation, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	delegations := m.delegations[delegator]
	if delegations == nil {
		return nil, fmt.Errorf("no delegation found")
	}

	delegation := delegations[validator]
	if delegation == nil {
		return nil, fmt.Errorf("delegation to this validator not found")
	}

	return delegation, nil
}

// GetDelegations returns all delegations for an address
func (m *Manager) GetDelegations(delegator common.Address) []*Delegation {
	m.mu.RLock()
	defer m.mu.RUnlock()

	var result []*Delegation
	for _, d := range m.delegations[delegator] {
		result = append(result, d)
	}

	return result
}

// GetValidatorDelegators returns all delegators for a validator
func (m *Manager) GetValidatorDelegators(validator common.Address) []common.Address {
	m.mu.RLock()
	defer m.mu.RUnlock()

	return m.byValidator[validator]
}

// GetTotalStaked returns total staked amount for a validator
func (m *Manager) GetTotalStaked(validator common.Address) *big.Int {
	m.mu.RLock()
	defer m.mu.RUnlock()

	if amount, ok := m.totalStaked[validator]; ok {
		return amount
	}

	return big.NewInt(0)
}

// GetDelegatorCount returns number of delegators for a validator
func (m *Manager) GetDelegatorCount(validator common.Address) int {
	m.mu.RLock()
	defer m.mu.RUnlock()

	return len(m.byValidator[validator])
}

// CalculateRewards calculates delegation rewards
func (m *Manager) CalculateRewards(delegator, validator common.Address, validatorReward *big.Int) (*big.Int, error) {
	delegation, err := m.GetDelegation(delegator, validator)
	if err != nil {
		return nil, err
	}

	totalStaked := m.GetTotalStaked(validator)
	if totalStaked.Sign() == 0 {
		return big.NewInt(0), nil
	}

	// Calculate share of validator's commission
	// Simplified - real implementation would use validator's actual commission rate
	commission := big.NewInt(1000) // 10%

	delegatorReward := new(big.Int).Mul(validatorReward, delegation.Amount)
	delegatorReward.Div(delegatorReward, totalStaked)
	delegatorReward.Mul(delegatorReward, commission)
	delegatorReward.Div(delegatorReward, big.NewInt(10000))

	return delegatorReward, nil
}

// MarshalJSON marshals delegation to JSON
func (d *Delegation) MarshalJSON() ([]byte, error) {
	return json.Marshal(map[string]interface{}{
		"delegator":     d.Delegator.Hex(),
		"validator":   d.Validator.Hex(),
		"amount":     d.Amount.String(),
		"lockEndTime": d.LockEndTime,
		"createdAt":  d.CreatedAt,
	})
}

// DelegationInfo returns formatted delegation info
func (d *Delegation) DelegationInfo() string {
	return fmt.Sprintf("Delegation(delegator=%s, validator=%s, amount=%s, lockEnds=%d)",
		d.Delegator.Hex()[:10], d.Validator.Hex()[:10], d.Amount.String(), d.LockEndTime)
}

// ManagerInfo returns manager statistics
func (m *Manager) ManagerInfo() string {
	m.mu.RLock()
	defer m.mu.RUnlock()

	totalDelegations := 0
	totalStaked := big.NewInt(0)

	for _, delegations := range m.delegations {
		totalDelegations += len(delegations)
	}

	for _, amount := range m.totalStaked {
		totalStaked.Add(totalStaked, amount)
	}

	return fmt.Sprintf("DelegationManager(totalDelegations=%d, totalStaked=%s)", totalDelegations, totalStaked.String())
}

var _ = json.Marshal // Use JSON