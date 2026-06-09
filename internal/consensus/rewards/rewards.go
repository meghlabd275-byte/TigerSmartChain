// Package rewards handles validator reward distribution.
package rewards

import (
	"fmt"
	"math/big"
	"sync"
	"time"
)

const (
	// BlockReward is the block reward in wei
	BlockReward = 1e18 // 1 TGR per block
	// EpochReward is the epoch reward
	EpochReward = 100e18 // 100 TGR per epoch
	// FoundationShare is the foundation share (10%)
	FoundationShare = 10
	// ValidatorShare is the validator share (90%)
	ValidatorShare = 90
)

// Distribution represents reward distribution.
type Distribution struct {
	Validator  string
	Amount     *big.Int
	Commission *big.Int
	Delegators *big.Int
}

// Rewards manages validator rewards.
type Rewards struct {
	mu sync.RWMutex

	// Pending rewards
	pending map[string]*big.Int
	// Distributed rewards
	distributed map[string]*big.Int
	// Total distributed
	total *big.Int
	// Treasury address
	treasury string
	// Foundation address
	foundation string
}

// NewRewards creates a new rewards manager.
func NewRewards(treasury, foundation string) *Rewards {
	return &Rewards{
		pending:     make(map[string]*big.Int),
		distributed: make(map[string]*big.Int),
		total:      big.NewInt(0),
		treasury:   treasury,
		foundation: foundation,
	}
}

// AddBlockReward adds a block reward.
func (r *Rewards) AddBlockReward(validator string) {
	r.mu.Lock()
	defer r.mu.Unlock()

	if _, ok := r.pending[validator]; !ok {
		r.pending[validator] = big.NewInt(0)
	}
	r.pending[validator].Add(r.pending[validator], big.NewInt(BlockReward))
	r.total.Add(r.total, big.NewInt(BlockReward))
}

// AddEpochReward adds an epoch reward.
func (r *Rewards) AddEpochReward(validator string) {
	r.mu.Lock()
	defer r.mu.Unlock()

	if _, ok := r.pending[validator]; !ok {
		r.pending[validator] = big.NewInt(0)
	}
	r.pending[validator].Add(r.pending[validator], big.NewInt(EpochReward))
	r.total.Add(r.total, big.NewInt(EpochReward))
}

// Distribute distributes pending rewards.
func (r *Rewards) Distribute(validator string, commission uint8, delegators map[string]*big.Int) ([]*Distribution, error) {
	r.mu.Lock()
	defer r.mu.Unlock()

	pending, ok := r.pending[validator]
	if !ok || pending.Sign() == 0 {
		return nil, fmt.Errorf("no pending rewards")
	}

	distributions := make([]*Distribution, 0)

	// Calculate validator commission
	commissionAmount := big.NewInt(0)
	commissionAmount.Mul(pending, big.NewInt(int64(commission)))
	commissionAmount.Div(commissionAmount, big.NewInt(100))

	// Delegator rewards
	delegatorReward := big.NewInt(0)
	delegatorReward.Sub(pending, commissionAmount)

	// Distribute to delegators proportionally
	if len(delegators) > 0 {
		totalDelegatorStake := big.NewInt(0)
		for _, stake := range delegators {
			totalDelegatorStake.Add(totalDelegatorStake, stake)
		}

		for addr, stake := range delegators {
			share := big.NewInt(0)
			share.Mul(delegatorReward, stake)
			share.Div(share, totalDelegatorStake)

			distributions = append(distributions, &Distribution{
				Validator:  addr,
				Amount:   share,
				Delegators: stake,
			})
		}
	}

	// Add validator's own reward
	distributions = append(distributions, &Distribution{
		Validator:  validator,
		Amount:    commissionAmount,
		Commission: big.NewInt(int64(commission)),
	})

	// Mark as distributed
	r.distributed[validator] = pending
	delete(r.pending, validator)

	return distributions, nil
}

// GetPendingReward returns pending reward for validator.
func (r *Rewards) GetPendingReward(validator string) *big.Int {
	r.mu.RLock()
	defer r.mu.RUnlock()

	if pending, ok := r.pending[validator]; ok {
		return new(big.Int).Set(pending)
	}
	return big.NewInt(0)
}

// GetDistributedReward returns distributed reward for validator.
func (r *Rewards) GetDistributedReward(validator string) *big.Int {
	r.mu.RLock()
	defer r.mu.RUnlock()

	if distributed, ok := r.distributed[validator]; ok {
		return new(big.Int).Set(distributed)
	}
	return big.NewInt(0)
}

// GetTotalDistributed returns total distributed rewards.
func (r *Rewards) GetTotalDistributed() *big.Int {
	r.mu.RLock()
	defer r.mu.RUnlock()
	return new(big.Int).Set(r.total)
}

// CalculateBlockReward calculates block reward with multipliers.
func (r *Rewards) CalculateBlockReward(validator string, uptime float64, missedBlocks uint64) *big.Int {
	r.mu.RLock()
	defer r.mu.RUnlock()

	reward := big.NewInt(BlockReward)

	// Apply uptime penalty
	if uptime < 100 {
		uptimeFactor := big.NewInt(int64(uptime * 100))
		reward.Mul(reward, uptimeFactor)
		reward.Div(reward, big.NewInt(10000))
	}

	// Deduct for missed blocks
	if missedBlocks > 0 {
		missedPenalty := big.NewInt(int64(missedBlocks * BlockReward / 100))
		reward.Sub(reward, missedPenalty)
		if reward.Sign() < 0 {
			reward = big.NewInt(0)
		}
	}

	return reward
}

// SetTreasury sets the treasury address.
func (r *Rewards) SetTreasury(address string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.treasury = address
}

// SetFoundation sets the foundation address.
func (r *Rewards) SetFoundation(address string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.foundation = address
}

// GetTreasury returns the treasury address.
func (r *Rewards) GetTreasury() string {
	r.mu.RLock()
	defer r.mu.RUnlock()
	return r.treasury
}

// GetFoundation returns the foundation address.
func (r *Rewards) GetFoundation() string {
	r.mu.RLock()
	defer r.mu.RUnlock()
	return r.foundation
}

// DistributeFoundation distributes foundation rewards.
func (r *Rewards) DistributeFoundation(amount *big.Int) *big.Int {
	r.mu.Lock()
	defer r.mu.Unlock()

	foundationReward := big.NewInt(0)
	foundationReward.Mul(amount, big.NewInt(FoundationShare))
	foundationReward.Div(foundationReward, big.NewInt(100))

	return foundationReward
}

// GetRewardHistory returns reward history.
func (r *Rewards) GetRewardHistory(validator string) ([]*RewardRecord, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	records := make([]*RewardRecord, 0)
	
	distributed, ok := r.distributed[validator]
	if ok {
		records = append(records, &RewardRecord{
			Validator: validator,
			Amount:   new(big.Int).Set(distributed),
			Epoch:    uint64(time.Now().Unix()),
		})
	}

	return records, nil
}

// RewardRecord represents a reward record.
type RewardRecord struct {
	Validator string
	Amount    *big.Int
	Epoch     uint64
	Type      string
}

// Reset resets pending rewards.
func (r *Rewards) Reset() {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.pending = make(map[string]*big.Int)
}

var _ = fmt.Sprintf("") // Use fmt