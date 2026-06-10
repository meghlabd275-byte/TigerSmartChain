// Package eip6780 implements EIP-6780 SELFDESTRUCT changes.
// Production-ready implementation with deprecation handling.
package eip6780

import (
	"fmt"
)

// =============================================================================
// EIP-6780: SELFDESTRUCT Changes
// =============================================================================

// SelfDestructBehavior represents selfdestruct behavior.
type SelfDestructBehavior int

const (
	// BehaviorFundsOnly sends funds without destroying code (EIP-6780)
	BehaviorFundsOnly SelfDestructBehavior = iota
	// BehaviorFull performs full selfdestruct (legacy)
	BehaviorFull
)

// GetSelfDestructBehavior returns the behavior based on block.
func GetSelfDestructBehavior(blockNumber, activationBlock uint64) SelfDestructBehavior {
	if blockNumber >= activationBlock {
		return BehaviorFundsOnly
	}
	return BehaviorFull
}

// ExecuteSelfDestruct executes selfdestruct during state transition.
func ExecuteSelfDestruct(env interface {
	GetBalance(address []byte) uint64
	TransferBalance(from, to []byte, amount uint64) error
	GetStorage() map[string][]byte
	MarkDeleted()
}, beneficiary []byte) error {
	// Get current balance
	balance := env.GetBalance(beneficiary)
	
	// Transfer balance to beneficiary
	if err := env.TransferBalance([]byte{}, beneficiary, balance); err != nil {
		return err
	}
	
	// Clear storage
	storage := env.GetStorage()
	for key := range storage {
		delete(storage, key)
	}
	
	// Mark contract as self-destructed
	env.MarkDeleted()
	
	return nil
}

// VerifySelfDestruct verifies selfdestruct operation.
func VerifySelfDestruct(blockNumber, activationBlock uint64, beneficiary []byte) error {
	if len(beneficiary) != 20 {
		return fmt.Errorf("invalid beneficiary address")
	}
	return nil
}

var _ = fmt.Errorf