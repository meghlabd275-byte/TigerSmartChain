// Package validator provides validator analytics and monitoring services
package validator

import (
	"fmt"
	"math/big"
	"sort"
	"strings"
	"sync"
	"time"
)

// ValidatorService provides validator analytics
type ValidatorService struct {
	validators map[string]*Validator
	proposals  map[uint64][]*Proposal
	mu        sync.RWMutex
}

// Validator represents a blockchain validator
type Validator struct {
	Address      string    `json:"address"`
	Moniker     string    `json:"moniker"`
	Commission float64   `json:"commission"`
	Power      uint64    `json:"power"`
	Uptime     float64   `json:"uptime"`
	Delegators int      `json:"delegators"`
	TotalStake string   `json:"totalStake"`
	SelfStake  string   `json:"selfStake"`
	Reward    string   `json:"reward"`
	Jailed     bool      `json:"jailed"`
	Status    string    `json:"status"`
}

// Proposal represents a governance proposal
type Proposal struct {
	ID          uint64    `json:"id"`
	Title       string    `json:"title"`
	Description string    `json:"description"`
	Proposer    string    `json:"proposer"`
	Status     string    `json:"status"`
	VotesFor    string    `json:"votesFor"`
	VotesAgainst string   `json:"votesAgainst"`
	VoteCount  int       `json:"voteCount"`
	StartTime  time.Time `json:"startTime"`
	EndTime    time.Time `json:"endTime"`
}

// BlockReward represents block reward distribution
type BlockReward struct {
	BlockNumber uint64   `json:"blockNumber"`
	Proposer   string   `json:"proposer"`
	Reward    string   `json:"reward"`
	FeeReward string   `json:"feeReward"`
	Timestamp time.Time `json:"timestamp"`
}

// ValidatorPerformance represents validator performance metrics
type ValidatorPerformance struct {
	Address       string    `json:"address"`
	BlocksProposed int      `json:"blocksProposed"`
	BlocksMissed  int      `json:"blocksMissed"`
	Uptime       float64  `json:"uptime"`
	AvgBlockTime  float64  `json:"avgBlockTime"`
	LastBlock   uint64    `json:"lastBlock"`
}

// NewValidatorService creates a new validator service
func NewValidatorService() *ValidatorService {
	return &ValidatorService{
		validators: make(map[string]*Validator),
		proposals:  make(map[uint64][]*Proposal),
	}
}

// GetValidators returns all validators
func (s *ValidatorService) GetValidators() ([]*Validator, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	result := make([]*Validator, 0, len(s.validators))
	for _, v := range s.validators {
		result = append(result, v)
	}
	
	// Sort by power
	sort.Slice(result, func(i, j int) bool {
		return result[i].Power > result[j].Power
	})
	
	return result, nil
}

// GetValidator returns a specific validator
func (s *ValidatorService) GetValidator(address string) (*Validator, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	v, ok := s.validators[strings.ToLower(address)]
	if !ok {
		return nil, fmt.Errorf("validator not found")
	}
	
	return v, nil
}

// GetTopValidators returns top validators by stake
func (s *ValidatorService) GetTopValidators(limit int) ([]*Validator, error) {
	validators, err := s.GetValidators()
	if err != nil {
		return nil, err
	}
	
	if len(validators) > limit {
		validators = validators[:limit]
	}
	
	return validators, nil
}

// GetValidatorPerformance gets validator performance metrics
func (s *ValidatorService) GetValidatorPerformance(address string) (*ValidatorPerformance, error) {
	v, err := s.GetValidator(address)
	if err != nil {
		return nil, err
	}
	
	return &ValidatorPerformance{
		Address:      v.Address,
		BlocksProposed: 0,
		BlocksMissed:  0,
		Uptime:      v.Uptime,
		AvgBlockTime:  12.0,
		LastBlock:   0,
	}, nil
}

// GetProposals returns governance proposals
func (s *ValidatorService) GetProposals(status string) ([]*Proposal, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	var result []*Proposal
	for _, proposals := range s.proposals {
		for _, p := range proposals {
			if status == "" || strings.ToLower(p.Status) == strings.ToLower(status) {
				result = append(result, p)
			}
		}
	}
	
	// Sort by ID descending
	sort.Slice(result, func(i, j int) bool {
		return result[i].ID > result[j].ID
	})
	
	return result, nil
}

// GetProposal returns a specific proposal
func (s *ValidatorService) GetProposal(id uint64) (*Proposal, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	proposals, ok := s.proposals[id]
	if !ok || len(proposals) == 0 {
		return nil, fmt.Errorf("proposal not found")
	}
	
	return proposals[0], nil
}

// VoteOnProposal votes on a proposal
func (s *ValidatorService) VoteOnProposal(proposalID uint64, voter string, vote string, weight string) error {
	s.mu.Lock()
	defer s.mu.mu.Unlock()
	
	proposals, ok := s.proposals[proposalID]
	if !ok {
		return fmt.Errorf("proposal not found")
	}
	
	proposal := proposals[0]
	
	voteBI, ok := new(big.Int).SetString(weight, 10)
	if !ok {
		return fmt.Errorf("invalid vote weight")
	}
	
	switch strings.ToLower(vote) {
	case "yes", "for":
		current, _ := new(big.Int).SetString(proposal.VotesFor, 10)
		proposal.VotesFor = current.Add(current, voteBI).String()
	case "no", "against":
		current, _ := new(big.Int).SetString(proposal.VotesAgainst, 10)
		proposal.VotesAgainst = current.Add(current, voteBI).String()
	}
	
	proposal.VoteCount++
	
	return nil
}

// GetBlockRewards gets block rewards for a range
func (s *ValidatorService) GetBlockRewards(startBlock, endBlock uint64) ([]*BlockReward, error) {
	// In production, would query blockchain
	return []*BlockReward{}, nil
}

// GetValidatorUptime gets validator uptime
func (s *ValidatorService) GetValidatorUptime(address string) (float64, error) {
	v, err := s.GetValidator(address)
	if err != nil {
		return 0, err
	}
	
	return v.Uptime, nil
}

// GetDelegators gets delegators for a validator
func (s *ValidatorService) GetDelegators(validator string) ([]*Delegator, error) {
	// In production, would query delegator list
	return []*Delegator{}, nil
}

// Delegator represents a delegator
type Delegator struct {
	Address    string `json:"address"`
	Validator string `json:"validator"`
	Stake     string `json:"stake"`
	Reward    string `json:"reward"`
}

// AddValidator adds a validator
func (s *ValidatorService) AddValidator(v *Validator) error {
	if v == nil {
		return fmt.Errorf("nil validator")
	}
	
	s.mu.Lock()
	s.validators[strings.ToLower(v.Address)] = v
	s.mu.Unlock()
	
	return nil
}

// AddProposal adds a proposal
func (s *ValidatorService) AddProposal(p *Proposal) error {
	if p == nil {
		return fmt.Errorf("nil proposal")
	}
	
	s.mu.Lock()
	s.proposals[p.ID] = append(s.proposals[p.ID], p)
	s.mu.Unlock()
	
	return nil
}

// GetStats gets validator statistics
func (s *ValidatorService) GetStats() (*ValidatorStats, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	totalPower := uint64(0)
	activeValidators := 0
	jailed := 0
	
	for _, v := range s.validators {
		totalPower += v.Power
		if !v.Jailed {
			activeValidators++
		} else {
			jailed++
		}
	}
	
	return &ValidatorStats{
		TotalValidators: len(s.validators),
		ActiveValidators: activeValidators,
		Jailed:      jailed,
		TotalPower: totalPower,
	}, nil
}

// ValidatorStats represents validator statistics
type ValidatorStats struct {
	TotalValidators int     `json:"totalValidators"`
	ActiveValidators int `json:"activeValidators"`
	Jailed      int    `json:"jailed"`
	TotalPower uint64 `json:"totalPower"`
}

// GetUpcomingProposals gets upcoming proposals
func (s *ValidatorService) GetUpcomingProposals() ([]*Proposal, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	now := time.Now()
	var result []*Proposal
	
	for _, proposals := range s.proposals {
		for _, p := range proposals {
			if p.StartTime.After(now) {
				result = append(result, p)
			}
		}
	}
	
	return result, nil
}

// GetActiveProposals gets active proposals
func (s *ValidatorService) GetActiveProposals() ([]*Proposal, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	now := time.Now()
	var result []*Proposal
	
	for _, proposals := range s.proposals {
		for _, p := range proposals {
			if now.After(p.StartTime) && now.Before(p.EndTime) {
				result = append(result, p)
			}
		}
	}
	
	return result, nil
}

// MonitorValidator monitors validator performance
func (s *ValidatorService) MonitorValidator(address string) (*ValidatorMonitor, error) {
	v, err := s.GetValidator(address)
	if err != nil {
		return nil, err
	}
	
	return &ValidatorMonitor{
		Validator:    v,
		LastChecked: time.Now(),
		Alerts:      []string{},
	}, nil
}

// ValidatorMonitor represents validator monitoring data
type ValidatorMonitor struct {
	Validator  *Validator `json:"validator"`
	LastChecked time.Time `json:"lastChecked"`
	Alerts    []string  `json:"alerts"`
}

// InitValidatorService initializes the service
func InitValidatorService() (*ValidatorService, error) {
	return NewValidatorService(), nil
}