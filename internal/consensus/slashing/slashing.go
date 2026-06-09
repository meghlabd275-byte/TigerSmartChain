// Package slashing handles validator slashing for misbehavior.
package slashing

import (
	"fmt"
	"math/big"
	"sync"
	"time"
)

const (
	// DoubleSignSlashRatio is the slash ratio for double signing (100%)
	DoubleSignSlashRatio = 100
	// DowntimeSlashRatio is the slash ratio for downtime (5%)
	DowntimeSlashRatio = 5
	// MissedBlocksSlashRatio is the slash ratio for missed blocks (1%)
	MissedBlocksSlashRatio = 1
	// UnavailableSlashRatio is the slash ratio for unavailability (2%)
	UnavailableSlashRatio = 2

	// DoubleSignJailDuration is the jail duration for double signing (1 day)
	DoubleSignJailDuration = 86400
	// DowntimeJailDuration is the jail duration for downtime (1 hour)
	DowntimeJailDuration = 3600
	// MissedBlocksJailDuration is the jail duration for missed blocks (30 min)
	MissedBlocksJailDuration = 1800
)

// SlashReason represents the reason for slashing.
type SlashReason int

const (
	SlashReasonDoubleSign SlashReason = iota
	SlashReasonDowntime
	SlashReasonMissedBlocks
	SlashReasonUnavailable
	SlashReasonMalicious
)

// SlashingEvent represents a slashing event.
type SlashingEvent struct {
	Validator  string
	Reason    SlashReason
	Amount    *big.Int
	Timestamp uint64
	Evidence  []byte
}

// SlashManager manages validator slashing.
type SlashManager struct {
	mu sync.RWMutex

	// Slashing events
	events map[string][]*SlashingEvent
	// Validator stakes
	stakes map[string]*big.Int
	// Jailed validators
	jailed map[string]*JailInfo
	// Slash history
	history []*SlashingEvent
	// Total slashed
	total *big.Int
}

// JailInfo represents jail information.
type JailInfo struct {
	Validator   string
	Reason      SlashReason
	ReleaseTime uint64
	SlashAmount *big.Int
}

// NewSlashManager creates a new slash manager.
func NewSlashManager() *SlashManager {
	return &SlashManager{
		events:  make(map[string][]*SlashingEvent),
		stakes:  make(map[string]*big.Int),
		jailed: make(map[string]*JailInfo),
		history: make([]*SlashingEvent, 0),
		total:  big.NewInt(0),
	}
}

// SetStake sets validator stake.
func (sm *SlashManager) SetStake(validator string, stake *big.Int) {
	sm.mu.Lock()
	defer sm.mu.Unlock()
	sm.stakes[validator] = new(big.Int).Set(stake)
}

// SlashDoubleSign slashes for double signing.
func (sm *SlashManager) SlashDoubleSign(validator string, evidence []byte) (*big.Int, error) {
	sm.mu.Lock()
	defer sm.mu.Unlock()

	stake, ok := sm.stakes[validator]
	if !ok {
		return nil, fmt.Errorf("validator not found")
	}

	// Calculate slash amount (100% of stake)
	slashAmount := new(big.Int).Set(stake)

	// Create event
	event := &SlashingEvent{
		Validator:  validator,
		Reason:    SlashReasonDoubleSign,
		Amount:   slashAmount,
		Timestamp: uint64(time.Now().Unix()),
		Evidence: evidence,
	}

	// Record event
	sm.events[validator] = append(sm.events[validator], event)
	sm.history = append(sm.history, event)

	// Update stake
	stake.Sub(stake, slashAmount)
	sm.stakes[validator] = stake

	// Add to jail
	sm.jailed[validator] = &JailInfo{
		Validator:   validator,
		Reason:    SlashReasonDoubleSign,
		ReleaseTime: uint64(time.Now().Unix()) + DoubleSignJailDuration,
		SlashAmount: slashAmount,
	}

	// Update total
	sm.total.Add(sm.total, slashAmount)

	return slashAmount, nil
}

// SlashDowntime slashes for downtime.
func (sm *SlashManager) SlashDowntime(validator string, missedBlocks uint64) (*big.Int, error) {
	sm.mu.Lock()
	defer sm.mu.Unlock()

	stake, ok := sm.stakes[validator]
	if !ok {
		return nil, fmt.Errorf("validator not found")
	}

	// Calculate slash amount
	slashAmount := big.NewInt(0)
	slashAmount.Mul(stake, big.NewInt(DowntimeSlashRatio))
	slashAmount.Div(slashAmount, big.NewInt(100))

	// Create event
	event := &SlashingEvent{
		Validator:  validator,
		Reason:    SlashReasonDowntime,
		Amount:   slashAmount,
		Timestamp: uint64(time.Now().Unix()),
	}

	// Record event
	sm.events[validator] = append(sm.events[validator], event)
	sm.history = append(sm.history, event)

	// Update stake
	stake.Sub(stake, slashAmount)
	sm.stakes[validator] = stake

	// Add to jail if needed
	if missedBlocks > 100 {
		sm.jailed[validator] = &JailInfo{
			Validator:   validator,
			Reason:    SlashReasonDowntime,
			ReleaseTime: uint64(time.Now().Unix()) + DowntimeJailDuration,
			SlashAmount: slashAmount,
		}
	}

	// Update total
	sm.total.Add(sm.total, slashAmount)

	return slashAmount, nil
}

// SlashMissedBlocks slashes for missed blocks.
func (sm *SlashManager) SlashMissedBlocks(validator string, missedBlocks uint64) (*big.Int, error) {
	sm.mu.Lock()
	defer sm.mu.Unlock()

	stake, ok := sm.stakes[validator]
	if !ok {
		return nil, fmt.Errorf("validator not found")
	}

	// Calculate slash amount
	slashAmount := big.NewInt(0)
	slashAmount.Mul(stake, big.NewInt(MissedBlocksSlashRatio))
	slashAmount.Div(slashAmount, big.NewInt(100))
	slashAmount.Mul(slashAmount, big.NewInt(int64(missedBlocks)))
	slashAmount.Div(slashAmount, big.NewInt(100))

	// Create event
	event := &SlashingEvent{
		Validator:  validator,
		Reason:    SlashReasonMissedBlocks,
		Amount:   slashAmount,
		Timestamp: uint64(time.Now().Unix()),
	}

	// Record event
	sm.events[validator] = append(sm.events[validator], event)
	sm.history = append(sm.history, event)

	// Update stake
	stake.Sub(stake, slashAmount)
	sm.stakes[validator] = stake

	// Add to jail if too many missed
	if missedBlocks > 50 {
		sm.jailed[validator] = &JailInfo{
			Validator:   validator,
			Reason:    SlashReasonMissedBlocks,
			ReleaseTime: uint64(time.Now().Unix()) + MissedBlocksJailDuration,
			SlashAmount: slashAmount,
		}
	}

	// Update total
	sm.total.Add(sm.total, slashAmount)

	return slashAmount, nil
}

// SlashUnavailable slashes for unavailability.
func (sm *SlashManager) SlashUnavailable(validator string, duration uint64) (*big.Int, error) {
	sm.mu.Lock()
	defer sm.mu.Unlock()

	stake, ok := sm.stakes[validator]
	if !ok {
		return nil, fmt.Errorf("validator not found")
	}

	// Calculate slash amount
	slashAmount := big.NewInt(0)
	slashAmount.Mul(stake, big.NewInt(UnavailableSlashRatio))
	slashAmount.Div(slashAmount, big.NewInt(100))

	// Create event
	event := &SlashingEvent{
		Validator:  validator,
		Reason:    SlashReasonUnavailable,
		Amount:   slashAmount,
		Timestamp: uint64(time.Now().Unix()),
	}

	// Record event
	sm.events[validator] = append(sm.events[validator], event)
	sm.history = append(sm.history, event)

	// Update stake
	stake.Sub(stake, slashAmount)
	sm.stakes[validator] = stake

	// Add to jail
	sm.jailed[validator] = &JailInfo{
		Validator:   validator,
		Reason:    SlashReasonUnavailable,
		ReleaseTime: uint64(time.Now().Unix()) + duration,
		SlashAmount: slashAmount,
	}

	// Update total
	sm.total.Add(sm.total, slashAmount)

	return slashAmount, nil
}

// IsJailed checks if validator is jailed.
func (sm *SlashManager) IsJailed(validator string) bool {
	sm.mu.RLock()
	defer sm.mu.RUnlock()

	jail, ok := sm.jailed[validator]
	if !ok {
		return false
	}

	// Check if release time has passed
	if uint64(time.Now().Unix()) >= jail.ReleaseTime {
		return false
	}

	return true
}

// GetJailInfo returns jail information.
func (sm *SlashManager) GetJailInfo(validator string) (*JailInfo, bool) {
	sm.mu.RLock()
	defer sm.mu.RUnlock()

	jail, ok := sm.jailed[validator]
	return jail, ok
}

// Release releases a validator from jail.
func (sm *SlashManager) Release(validator string) error {
	sm.mu.Lock()
	defer sm.mu.Unlock()

	if _, ok := sm.jailed[validator]; !ok {
		return fmt.Errorf("validator not jailed")
	}

	delete(sm.jailed, validator)
	return nil
}

// GetSlashEvents returns slashing events for validator.
func (sm *SlashManager) GetSlashEvents(validator string) []*SlashingEvent {
	sm.mu.RLock()
	defer sm.mu.RUnlock()

	return sm.events[validator]
}

// GetSlashHistory returns full slashing history.
func (sm *SlashManager) GetSlashHistory() []*SlashingEvent {
	sm.mu.RLock()
	defer sm.mu.RUnlock()
	return sm.history
}

// GetTotalSlashed returns total slashed amount.
func (sm *SlashManager) GetTotalSlashed() *big.Int {
	sm.mu.RLock()
	defer sm.mu.RUnlock()
	return new(big.Int).Set(sm.total)
}

// GetStake returns validator stake.
func (sm *SlashManager) GetStake(validator string) *big.Int {
	sm.mu.RLock()
	defer sm.mu.RUnlock()

	if stake, ok := sm.stakes[validator]; ok {
		return new(big.Int).Set(stake)
	}
	return big.NewInt(0)
}

// CalculateSlashAmount calculates slash amount for given reason.
func (sm *SlashManager) CalculateSlashAmount(validator string, reason SlashReason) *big.Int {
	sm.mu.RLock()
	defer sm.mu.RUnlock()

	stake, ok := sm.stakes[validator]
	if !ok {
		return big.NewInt(0)
	}

	ratio := 0
	switch reason {
	case SlashReasonDoubleSign:
		ratio = DoubleSignSlashRatio
	case SlashReasonDowntime:
		ratio = DowntimeSlashRatio
	case SlashReasonMissedBlocks:
		ratio = MissedBlocksSlashRatio
	case SlashReasonUnavailable:
		ratio = UnavailableSlashRatio
	default:
		ratio = 0
	}

	slashAmount := big.NewInt(0)
	slashAmount.Mul(stake, big.NewInt(ratio))
	slashAmount.Div(slashAmount, big.NewInt(100))

	return slashAmount
}

// GetSlashCount returns number of slash events.
func (sm *SlashManager) GetSlashCount(validator string) int {
	sm.mu.RLock()
	defer sm.mu.RUnlock()
	return len(sm.events[validator])
}

// ClearHistory clears slash history.
func (sm *SlashManager) ClearHistory() {
	sm.mu.Lock()
	defer sm.mu.Unlock()
	sm.history = make([]*SlashingEvent, 0)
}

var _ = fmt.Sprintf("") // Use fmt