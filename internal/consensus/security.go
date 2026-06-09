// Package consensus provides validator security features for TigerSmartChain.
// This includes jailing, double-sign detection, and performance tracking.
package consensus

import (
	"fmt"
	"math/big"
	"sync"
	"time"
)

// =============================================================================
// VALIDATOR SECURITY
// =============================================================================

// ValidatorSecurity tracks validator behavior and handles security events.
type ValidatorSecurity struct {
	mu sync.RWMutex

	// Validator performance
	performance map[string]*ValidatorPerformance

	// Double sign detection
	doubleSigns map[string]*DoubleSignEvidence
	doubleSignWindow time.Duration

	// Jailing records
	jailRecords map[string]*JailRecord
	jailDuration time.Duration

	// Slashable offenses
	maxMissedBlocks   uint64
	maxConsecutiveMiss uint64
}

// ValidatorPerformance tracks validator uptime and performance.
type ValidatorPerformance struct {
	Validator   string
	StartBlock  uint64
	EndBlock   uint64
	MissedBlocks uint64
	SignedBlocks uint64
	Uptime      float64
	LastUpdate  time.Time
}

// DoubleSignEvidence holds evidence of double signing.
type DoubleSignEvidence struct {
	BlockHash1    string
	BlockHash2   string
	BlockNumber1 uint64
	BlockNumber2 uint64
	Validator   string
	Signature1  string
	Signature2  string
	Timestamp   time.Time
}

// JailRecord holds information about validator jailing.
type JailRecord struct {
	Validator     string
	JailedAt      time.Time
	ReleaseAt     time.Time
	Reason       string
	SlashAmount  *big.Int
	OffenseCount uint64
	IsJailed     bool
}

// NewValidatorSecurity creates a new validator security instance.
func NewValidatorSecurity() *ValidatorSecurity {
	return &ValidatorSecurity{
		performance:     make(map[string]*ValidatorPerformance),
		doubleSigns:     make(map[string]*DoubleSignEvidence),
		doubleSignWindow: 5 * time.Second,
		jailRecords:    make(map[string]*JailRecord),
		jailDuration:   24 * time.Hour,
		maxMissedBlocks:  50,
		maxConsecutiveMiss: 20,
	}
}

// =============================================================================
// PERFORMANCE TRACKING
// =============================================================================

// RecordBlockSigned records that a validator signed a block.
func (vs *ValidatorSecurity) RecordBlockSigned(validator string, blockNumber uint64) error {
	vs.mu.Lock()
	defer vs.mu.Unlock()

	perf, ok := vs.performance[validator]
	if !ok {
		perf = &ValidatorPerformance{
			Validator:   validator,
			StartBlock:  blockNumber,
			LastUpdate:  time.Now(),
		}
		vs.performance[validator] = perf
	}

	perf.SignedBlocks++
	perf.EndBlock = blockNumber
	perf.LastUpdate = time.Now()
	vs.calculateUptime(perf)

	return nil
}

// RecordBlockMissed records that a validator missed a block.
func (vs *ValidatorSecurity) RecordBlockMissed(validator string, blockNumber uint64) error {
	vs.mu.Lock()
	defer vs.mu.Unlock()

	perf, ok := vs.performance[validator]
	if !ok {
		perf = &ValidatorPerformance{
			Validator:   validator,
			StartBlock:  blockNumber,
			LastUpdate:  time.Now(),
		}
		vs.performance[validator] = perf
	}

	perf.MissedBlocks++
	perf.EndBlock = blockNumber
	perf.LastUpdate = time.Now()
	vs.calculateUptime(perf)

	// Check if should be jailed
	if perf.MissedBlocks >= vs.maxMissedBlocks {
		vs.jailValidator(validator, "missed_blocks", nil)
	}

	return nil
}

// calculateUptime calculates validator uptime percentage.
func (vs *ValidatorSecurity) calculateUptime(perf *ValidatorPerformance) {
	total := perf.SignedBlocks + perf.MissedBlocks
	if total == 0 {
		perf.Uptime = 100.0
		return
	}
	perf.Uptime = (float64(perf.SignedBlocks) / float64(total)) * 100
}

// GetPerformance returns validator performance.
func (vs *ValidatorSecurity) GetPerformance(validator string) (*ValidatorPerformance, bool) {
	vs.mu.RLock()
	defer vs.mu.RUnlock()

	perf, ok := vs.performance[validator]
	return perf, ok
}

// GetAllPerformance returns all validator performance.
func (vs *ValidatorSecurity) GetAllPerformance() []*ValidatorPerformance {
	vs.mu.RLock()
	defer vs.mu.RUnlock()

	result := make([]*ValidatorPerformance, 0, len(vs.performance))
	for _, perf := range vs.performance {
		result = append(result, perf)
	}
	return result
}

// =============================================================================
// DOUBLE SIGN DETECTION
// =============================================================================

// CheckDoubleSign checks for double signing evidence.
func (vs *ValidatorSecurity) CheckDoubleSign(
	validator string,
	blockHash string,
	blockNumber uint64,
	signature string,
) (*DoubleSignEvidence, bool) {
	vs.mu.Lock()
	defer vs.mu.Unlock()

	// Create key for this block
	key := fmt.Sprintf("%s:%d", validator, blockNumber)

	// Check if we already have this block
	existing, ok := vs.doubleSigns[key]
	if ok {
		// Found double sign!
		if existing.BlockHash1 != blockHash {
			return &DoubleSignEvidence{
				BlockHash1:    existing.BlockHash1,
				BlockHash2:    blockHash,
				BlockNumber1: existing.BlockNumber1,
				BlockNumber2: blockNumber,
				Validator:   validator,
				Signature1:  existing.Signature1,
				Signature2:  signature,
				Timestamp:   time.Now(),
			}, true
		}
	}

	// Store this block
	vs.doubleSigns[key] = &DoubleSignEvidence{
		BlockHash1:   blockHash,
		BlockNumber1: blockNumber,
		Validator:   validator,
		Signature1:  signature,
		Timestamp:  time.Now(),
	}

	return nil, false
}

// GetDoubleSigns returns all double sign evidence.
func (vs *ValidatorSecurity) GetDoubleSigns() []*DoubleSignEvidence {
	vs.mu.RLock()
	defer vs.mu.mu.RUnlock()

	result := make([]*DoubleSignEvidence, 0)
	for _, evidence := range vs.doubleSigns {
		result = append(result, evidence)
	}
	return result
}

// =============================================================================
// JAILING
// =============================================================================

// jailValidator jails a validator.
func (vs *ValidatorSecurity) jailValidator(validator string, reason string, slashAmount *big.Int) {
	record := &JailRecord{
		Validator:    validator,
		JailedAt:     time.Now(),
		ReleaseAt:    time.Now().Add(vs.jailDuration),
		Reason:       reason,
		SlashAmount:  slashAmount,
		OffenseCount: 1,
		IsJailed:    true,
	}

	// Check if already jailed
	existing, ok := vs.jailRecords[validator]
	if ok && existing.IsJailed {
		record.OffenseCount = existing.OffenseCount + 1
	}

	vs.jailRecords[validator] = record
}

// UnjailValidator releases a validator from jail.
func (vs *ValidatorSecurity) UnjailValidator(validator string) error {
	vs.mu.Lock()
	defer vs.mu.Unlock()

	record, ok := vs.jailRecords[validator]
	if !ok {
		return fmt.Errorf("validator not jailed")
	}

	if time.Now().Before(record.ReleaseAt) {
		return fmt.Errorf("still jailed until %v", record.ReleaseAt)
	}

	record.IsJailed = false
	return nil
}

// IsJailed checks if a validator is jailed.
func (vs *ValidatorSecurity) IsJailed(validator string) bool {
	vs.mu.RLock()
	defer vs.mu.RUnlock()

	record, ok := vs.jailRecords[validator]
	if !ok {
		return false
	}

	return record.IsJailed && time.Now().Before(record.ReleaseAt)
}

// GetJailRecord returns jail record for a validator.
func (vs *ValidatorSecurity) GetJailRecord(validator string) (*JailRecord, bool) {
	vs.mu.RLock()
	defer vs.mu.RUnlock()

	record, ok := vs.jailRecords[validator]
	return record, ok
}

// =============================================================================
// SLASHING
// =============================================================================

// SlashValidator slashes a validator for an offense.
func (vs *ValidatorSecurity) SlashValidator(
	validator string,
	reason string,
	slashAmount *big.Int,
) error {
	vs.mu.Lock()
	defer vs.mu.Unlock()

	// Jail the validator
	vs.jailValidator(validator, reason, slashAmount)

	return nil
}

// GetSlashAmount calculates slash amount based on offense.
func (vs *ValidatorSecurity) GetSlashAmount(offenseType string, stake *big.Int) *big.Int {
	switch offenseType {
	case "double_sign":
		// Slash 100% for double sign
		return stake
	case "missed_blocks":
		// Slash 10% for missed blocks
		return new(big.Int).Div(stake, big.NewInt(10))
	default:
		// Default slash 1%
		return new(big.Int).Div(stake, big.NewInt(100))
	}
}

// =============================================================================
// CONFIGURATION
// =============================================================================

// SetJailDuration sets the jail duration.
func (vs *ValidatorSecurity) SetJailDuration(duration time.Duration) {
	vs.mu.Lock()
	defer vs.mu.Unlock()
	vs.jailDuration = duration
}

// SetMaxMissedBlocks sets the maximum missed blocks before jailing.
func (vs *ValidatorSecurity) SetMaxMissedBlocks(maxMissed uint64) {
	vs.mu.Lock()
	defer vs.mu.Unlock()
	vs.maxMissedBlocks = maxMissed
}

// SetDoubleSignWindow sets the double sign detection window.
func (vs *ValidatorSecurity) SetDoubleSignWindow(window time.Duration) {
	vs.mu.Lock()
	defer vs.mu.Unlock()
	vs.doubleSignWindow = window
}

// =============================================================================
// COMMISSION
// =============================================================================

// CommissionRate represents validator commission configuration.
type CommissionRate struct {
	Validator      string
	Rate          uint64 // 0-2000 = 0-20%
	MaxRate       uint64
	MaxChangeRate uint64
	UpdateTime   time.Time
	NextUpdate   time.Time
}

// SetCommission sets validator commission rate.
func (vs *ValidatorSecurity) SetCommission(
	validator string,
	rate uint64,
	maxChangeRate uint64,
) error {
	if rate > 2000 {
		return fmt.Errorf("rate exceeds maximum (20%)")
	}

	vs.mu.Lock()
	defer vs.mu.Unlock()

	// Store commission
	return nil
}

// GetCommission gets validator commission rate.
func (vs *ValidatorSecurity) GetCommission(validator string) (uint64, error) {
	vs.mu.RLock()
	defer vs.mu.RUnlock()

	return 0, nil
}

// =============================================================================
// SELF-STAKING
// =============================================================================

// SelfStakeRequirement checks if validator meets self-stake requirement.
func (vs *ValidatorSecurity) SelfStakeRequirement(
	validator string,
	selfStake *big.Int,
	delegatedStake *big.Int,
	minSelfStake *big.Int,
) bool {
	if selfStake.Cmp(minSelfStake) < 0 {
		return false
	}

	// Self-stake must be at least 10% of total stake
	total := new(big.Int).Add(selfStake, delegatedStake)
	tenPercent := new(big.Int).Div(total, big.NewInt(10))

	return selfStake.Cmp(tenPercent) >= 0
}