// Package election provides validator election for PoSA consensus.
package election

import (
	"crypto/sha256"
	"encoding/binary"
	"fmt"
	"math/big"
	"sort"
	"sync"
	"time"
)

const (
	// MaxValidators is the maximum number of validators
	MaxValidators = 21
	// MinStake is the minimum stake required
	MinStake = 1000000 // 1M tokens
	// ElectionInterval is the number of blocks between elections
	ElectionInterval = 200
)

// Validator represents a validator candidate.
type Validator struct {
	Address      string
	Stake        *big.Int
	Commission   uint8
	Delegators   uint64
	TotalStake   *big.Int
	SelfDelegate *big.Int
	Uptime       float64
	Jailed       bool
	JailUntil    uint64
	Registered  uint64
	LastVote    uint64
	Active      bool
}

// NewValidator creates a new validator.
func NewValidator(address string, stake *big.Int) *Validator {
	return &Validator{
		Address:      address,
		Stake:        new(big.Int).Set(stake),
		Commission:  10, // 10% default
		TotalStake:  new(big.Int).Set(stake),
		SelfDelegate: new(big.Int).Set(stake),
		Uptime:       100.0,
		Registered:  uint64(time.Now().Unix()),
		Active:      true,
	}
}

// ElectionResult represents the result of validator election.
type ElectionResult struct {
	Epoch       uint64
	Validators  []string
	BlockReward *big.Int
	Timestamp   uint64
}

// Election handles validator elections.
type Election struct {
	mu sync.RWMutex

	// Validator candidates
	candidates map[string]*Validator
	// Active validators
	validators []*Validator
	// Current epoch
	epoch uint64
	// Last election block
	lastElectionBlock uint64
	// Block time (seconds)
	blockTime uint64
}

// NewElection creates a new election instance.
func NewElection(blockTime uint64) *Election {
	return &Election{
		candidates:    make(map[string]*Validator),
		validators:    make([]*Validator, 0),
		blockTime:    blockTime,
		lastElectionBlock: 0,
	}
}

// RegisterValidator registers a new validator candidate.
func (e *Election) RegisterValidator(address string, stake *big.Int) error {
	e.mu.Lock()
	defer e.mu.Unlock()

	if stake.Cmp(big.NewInt(MinStake)) < 0 {
		return fmt.Errorf("stake below minimum: %d", MinStake)
	}

	if _, exists := e.candidates[address]; exists {
		return fmt.Errorf("validator already registered")
	}

	e.candidates[address] = NewValidator(address, stake)
	return nil
}

// Delegate delegates stake to a validator.
func (e *Election) Delegate(validator, delegator string, amount *big.Int) error {
	e.mu.Lock()
	defer e.mu.Unlock()

	v, exists := e.candidates[validator]
	if !exists {
		return fmt.Errorf("validator not found")
	}

	if v.Jailed {
		return fmt.Errorf("validator is jailed")
	}

	v.Delegators++
	v.TotalStake.Add(v.TotalStake, amount)

	return nil
}

// Undelegate undelegates stake from a validator.
func (e *Election) Undelegate(validator, delegator string, amount *big.Int) error {
	e.mu.Lock()
	defer e.mu.Unlock()

	v, exists := e.candidates[validator]
	if !exists {
		return fmt.Errorf("validator not found")
	}

	if v.TotalStake.Cmp(amount) < 0 {
		return fmt.Errorf("insufficient stake")
	}

	v.TotalStake.Sub(v.TotalStake, amount)
	v.Delegators--

	return nil
}

// UpdateStake updates validator stake.
func (e *Election) UpdateStake(address string, amount *big.Int) error {
	e.mu.Lock()
	defer e.mu.Unlock()

	v, exists := e.candidates[address]
	if !exists {
		return fmt.Errorf("validator not found")
	}

	v.Stake.Add(v.Stake, amount)
	v.TotalStake.Add(v.TotalStake, amount)

	return nil
}

// SetCommission sets validator commission rate.
func (e *Election) SetCommission(address string, commission uint8) error {
	e.mu.Lock()
	defer e.mu.Unlock()

	if commission > 100 {
		return fmt.Errorf("commission too high")
	}

	v, exists := e.candidates[address]
	if !exists {
		return fmt.Errorf("validator not found")
	}

	v.Commission = commission
	return nil
}

// UpdateUptime updates validator uptime.
func (e *Election) UpdateUptime(address string, signedBlocks, totalBlocks uint64) error {
	e.mu.Lock()
	defer e.mu.Unlock()

	v, exists := e.candidates[address]
	if !exists {
		return fmt.Errorf("validator not found")
	}

	if totalBlocks > 0 {
		v.Uptime = float64(signedBlocks) / float64(totalBlocks) * 100
	}

	return nil
}

// JailValidator jails a validator.
func (e *Election) JailValidator(address string, duration uint64) error {
	e.mu.Lock()
	defer e.mu.Unlock()

	v, exists := e.candidates[address]
	if !exists {
		return fmt.Errorf("validator not found")
	}

	v.Jailed = true
	v.JailUntil = uint64(time.Now().Unix()) + duration

	return nil
}

// UnjailValidator unjails a validator.
func (e *Election) UnjailValidator(address string) error {
	e.mu.Lock()
	defer e.mu.Unlock()

	v, exists := e.candidates[address]
	if !exists {
		return fmt.Errorf("validator not found")
	}

	v.Jailed = false
	v.JailUntil = 0

	return nil
}

// ElectValidators performs validator election.
func (e *Election) ElectValidators(blockNumber uint64) (*ElectionResult, error) {
	e.mu.Lock()
	defer e.mu.Unlock()

	// Check if election is needed
	if blockNumber-e.lastElectionBlock < ElectionInterval {
		return nil, nil
	}

	// Filter active validators
	activeCandidates := make([]*Validator, 0)
	for _, v := range e.candidates {
		if v.Active && !v.Jailed && v.TotalStake.Cmp(big.NewInt(MinStake)) >= 0 {
			activeCandidates = append(activeCandidates, v)
		}
	}

	// Not enough candidates
	if len(activeCandidates) < 1 {
		return nil, fmt.Errorf("not enough validators")
	}

	// Sort by stake (descending)
	sort.Slice(activeCandidates, func(i, j int) bool {
		if activeCandidates[i].TotalStake.Cmp(activeCandidates[j].TotalStake) != 0 {
			return activeCandidates[i].TotalStake.Cmp(activeCandidates[j].TotalStake) > 0
		}
		return activeCandidates[i].Uptime > activeCandidates[j].Uptime
	})

	// Select top validators
	count := MaxValidators
	if len(activeCandidates) < count {
		count = len(activeCandidates)
	}

	e.validators = activeCandidates[:count]
	e.lastElectionBlock = blockNumber
	e.epoch++

	// Get validator addresses
	addrs := make([]string, count)
	for i, v := range e.validators {
		addrs[i] = v.Address
	}

	return &ElectionResult{
		Epoch:      e.epoch,
		Validators: addrs,
		BlockReward: big.NewInt(1000000000000000000), // 1 TGR per block
		Timestamp:  uint64(time.Now().Unix()),
	}, nil
}

// GetValidators returns current validators.
func (e *Election) GetValidators() []string {
	e.mu.RLock()
	defer e.mu.RUnlock()

	addrs := make([]string, len(e.validators))
	for i, v := range e.validators {
		addrs[i] = v.Address
	}
	return addrs
}

// GetValidator returns validator info.
func (e *Election) GetValidator(address string) (*Validator, bool) {
	e.mu.RLock()
	defer e.mu.RUnlock()

	v, ok := e.candidates[address]
	return v, ok
}

// GetCandidates returns all validator candidates.
func (e *Election) GetCandidates() []*Validator {
	e.mu.RLock()
	defer e.mu.RUnlock()

	candidates := make([]*Validator, 0)
	for _, v := range e.candidates {
		candidates = append(candidates, v)
	}
	return candidates
}

// GetCandidateCount returns the number of candidates.
func (e *Election) GetCandidateCount() int {
	e.mu.RLock()
	defer e.mu.RUnlock()
	return len(e.candidates)
}

// IsValidator checks if address is a validator.
func (e *Election) IsValidator(address string) bool {
	e.mu.RLock()
	defer e.mu.RUnlock()

	for _, v := range e.validators {
		if v.Address == address {
			return true
		}
	}
	return false
}

// GetValidatorCount returns the number of validators.
func (e *Election) GetValidatorCount() int {
	e.mu.RLock()
	defer e.mu.RUnlock()
	return len(e.validators)
}

// GetEpoch returns current epoch.
func (e *Election) GetEpoch() uint64 {
	e.mu.RLock()
	defer e.mu.RUnlock()
	return e.epoch
}

// GetProposer returns the proposer for a block.
func (e *Election) GetProposer(blockNumber uint64) string {
	e.mu.RLock()
	defer e.mu.RUnlock()

	if len(e.validators) == 0 {
		return ""
	}

	// Rotate proposer based on block number
	index := blockNumber % uint64(len(e.validators))
	return e.validators[index].Address
}

// CalculateValidatorReward calculates validator reward.
func (e *Election) CalculateValidatorReward(validator string, blocksSigned uint64) (*big.Int, error) {
	e.mu.RLock()
	defer e.mu.RUnlock()

	v, exists := e.candidates[validator]
	if !exists {
		return nil, fmt.Errorf("validator not found")
	}

	// Calculate reward based on stake and uptime
	reward := big.NewInt(0)
	reward.Mul(big.NewInt(int64(blocksSigned)), big.NewInt(1e18)) // 1 TGR per block
	
	// Apply uptime factor
	uptimeFactor := big.NewInt(int64(v.Uptime * 100))
	reward.Mul(reward, uptimeFactor)
	reward.Div(reward, big.NewInt(10000))

	return reward, nil
}

// SelectProposerByHash selects proposer using block hash.
func (e *Election) SelectProposerByHash(blockHash string, blockNumber uint64) string {
	e.mu.RLock()
	defer e.mu.RUnlock()

	if len(e.validators) == 0 {
		return ""
	}

	// Hash-based selection
	hash := sha256.Sum256([]byte(blockHash))
	seed := binary.BigEndian.Uint64(hash[:8])
	index := seed % uint64(len(e.validators))

	return e.validators[index].Address
}

// SetValidators manually sets validators (for testing/genesis).
func (e *Election) SetValidators(addrs []string) {
	e.mu.Lock()
	defer e.mu.Unlock()

	e.validators = make([]*Validator, 0)
	for _, addr := range addrs {
		if v, ok := e.candidates[addr]; ok {
			e.validators = append(e.validators, v)
		}
	}
}

// GetActiveValidators returns active validators.
func (e *Election) GetActiveValidators() []*Validator {
	e.mu.RLock()
	defer e.mu.RUnlock()

	active := make([]*Validator, 0)
	for _, v := range e.validators {
		if v.Active && !v.Jailed {
			active = append(active, v)
		}
	}
	return active
}

var _ = fmt.Sprintf("") // Use fmt