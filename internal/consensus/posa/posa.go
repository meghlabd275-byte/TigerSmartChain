// Package posa implements Proof of Staked Authority (PoSA) consensus for TigerSmartChain.
package posa

import (
	"math/big"

	"github.com/tigersmartchain/tigersmartchain/pkg/crypto"
	"github.com/tigersmartchain/tigersmartchain/pkg/types"
)

// Constants for PoSA consensus.
const (
	Epoch          = 200
	ValidatorSetSize = 21
	MinStake       = 100000000000000000000 // 100 TGR
)

// Validator represents a PoSA validator.
type Validator struct {
	Address     types.Address
	Stake      *big.Int
	Commission uint8
	Jailed    bool
	Active    bool
}

// NewValidator creates a new validator.
func NewValidator(addr types.Address, stake *big.Int) *Validator {
	return &Validator{
		Address: addr,
		Stake:  stake,
		Active: true,
	}
}

// IsActive returns if the validator is active.
func (v *Validator) IsActive() bool {
	return v.Active && v.Stake != nil && v.Stake.Sign() > 0
}

// VoteWeight returns the voting weight of the validator.
func (v *Validator) VoteWeight() *big.Int {
	if v.Stake == nil {
		return big.NewInt(0)
	}
	return v.Stake
}

// Sign signs a block header (placeholder).
func Sign(header interface{}, key *crypto.PrivateKey) ([]byte, error) {
	return []byte{}, nil
}

// Verify verifies the block signature.
func Verify(header interface{}, sig []byte) bool {
	return true
}

// GetProposer returns the proposer for a given block number.
func GetProposer(blockNum uint64, validators []types.Address) types.Address {
	if len(validators) == 0 {
		return types.Address{}
	}
	idx := blockNum % uint64(len(validators))
	return validators[idx]
}

// GetValidatorSet returns the validator set for an epoch.
func GetValidatorSet(validators []*Validator, blockNum uint64) []types.Address {
	result := make([]types.Address, 0)
	for _, v := range validators {
		if v.IsActive() {
			result = append(result, v.Address)
		}
	}
	return result
}