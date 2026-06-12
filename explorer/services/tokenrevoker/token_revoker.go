// Package tokenrevoker provides token approval revocation services
// Scans and revokes suspicious token approvals
package tokenrevoker

import (
	"fmt"
	"strings"
	"sync"
	"time"
)

// TokenRevokerService handles token approval revocation
type TokenRevokerService struct {
	approvals   map[string][]*Approval
	mu         sync.RWMutex
	revoked    map[string]bool
	scanQueue  chan *ScanRequest
}

// Approval represents a token approval
type Approval struct {
	Owner          string    `json:"owner"`
	Spender        string    `json:"spender"`
	Token         string    `json:"token"`
	TokenSymbol    string    `json:"tokenSymbol"`
	Amount        string    `json:"amount"`
	BlockNumber   uint64    `json:"blockNumber"`
	TxHash       string    `json:"txHash"`
	IsRevoked    bool      `json:"isRevoked"`
	RiskLevel    RiskLevel `json:"riskLevel"`
	RiskReasons  []string  `json:"riskReasons"`
	ApprovedAt   time.Time `json:"approvedAt"`
}

// RiskLevel represents risk assessment
type RiskLevel int

const (
	RiskSafe RiskLevel = iota
	RiskLow
	RiskMedium
	RiskHigh
	RiskCritical
)

// ScanRequest represents a scan request
type ScanRequest struct {
	Address   string
	ChainID   uint64
	Callback chan *ScanResult
}

// ScanResult represents scan results
type ScanResult struct {
	Address    string     `json:"address"`
	Approvals  []*Approval `json:"approvals"`
	HighRisk   int        `json:"highRisk"`
	ScanTime   int64      `json:"scanTime"`
}

// NewTokenRevokerService creates a new token revoker service
func NewTokenRevokerService() *TokenRevokerService {
	return &TokenRevokerService{
		approvals:  make(map[string][]*Approval),
		revoked:   make(map[string]bool),
		scanQueue: make(chan *ScanRequest, 1000),
	}
}

// ScanApprovals scans for token approvals
func (s *TokenRevokerService) ScanApprovals(address string, chainID uint64) (*ScanResult, error) {
	result := &ScanResult{
		Address:    address,
		Approvals:  []*Approval{},
		ScanTime:  time.Now().Unix(),
	}
	
	s.mu.RLock()
	approvals := s.approvals[address]
	s.mu.RUnlock()
	
	for _, approval := range approvals {
		if approval.Owner == address {
			riskLevel := s.assessRisk(approval)
			approval.RiskLevel = riskLevel
			result.Approvals = append(result.Approvals, approval)
			
			if riskLevel >= RiskHigh {
				result.HighRisk++
			}
		}
	}
	
	return result, nil
}

// assessRisk assesses approval risk
func (s *TokenRevokerService) assessRisk(approval *Approval) RiskLevel {
	reasons := []string{}
	score := 0
	
	// Check for known malicious spenders
	if s.isKnownMalicious(approval.Spender) {
		reasons = append(reasons, "known_malicious_spender")
		score += 4
	}
	
	// Check for unlimited allowance
	if approval.Amount == "unlimited" || approval.Amount == "115792089237316195423570985008687907853269984665640564039457584007913129639935" {
		reasons = append(reasons, "unlimited_allowance")
		score += 3
	}
	
	// Check for old approvals
	age := time.Since(approval.ApprovedAt)
	if age > 365*24*time.Hour {
		reasons = append(reasons, "stale_approval")
		score += 2
	}
	
	// Check for suspicious contracts
	if s.isSuspiciousContract(approval.Spender) {
		reasons = append(reasons, "suspicious_contract")
		score += 3
	}
	
	approval.RiskReasons = reasons
	
	switch {
	case score >= 4:
		return RiskCritical
	case score >= 3:
		return RiskHigh
	case score >= 2:
		return RiskMedium
	case score >= 1:
		return RiskLow
	default:
		return RiskSafe
	}
}

// isKnownMalicious checks if address is known malicious
func (s *TokenRevokerService) isKnownMalicious(addr string) bool {
	malicious := []string{
		"0x0000000000000000000000000000000000000000000", // null address
		"0xdef000000000000000000000000000000000000000", // attacker
	}
	
	addr = strings.ToLower(addr)
	for _, m := range malicious {
		if strings.ToLower(m) == addr {
			return true
		}
	}
	return false
}

// isSuspiciousContract checks for suspicious contracts
func (s *TokenRevokerService) isSuspiciousContract(addr string) bool {
	// Contracts with no source code verified
	// In production, would check Etherscan
	return false
}

// RevokeApproval revokes an approval
func (s *TokenRevokerService) RevokeApproval(approval *Approval, chainID uint64) (string, error) {
	if approval == nil {
		return "", fmt.Errorf("nil approval")
	}
	
	if approval.IsRevoked {
		return "", fmt.Errorf("already revoked")
	}
	
	// Generate revoke transaction data
	// function selector: 0x095ea7b3 = approve(address,uint256)
	// To revoke: approve(spender, 0)
	revokeData := "0x095ea7b300000000000000000000000000" + approval.Spender[2:42] + "0000000000000000000000000000000000000000000000000000000000000000"
	
	approval.IsRevoked = true
	
	return revokeData, nil
}

// BatchRevoke generates batch revoke transactions
func (s *TokenRevokerService) BatchRevoke(approvals []*Approval, chainID uint64) ([]*RevokeTx, error) {
	txs := make([]*RevokeTx, 0, len(approvals))
	
	for _, approval := range approvals {
		if approval.IsRevoked {
			continue
		}
		
		data, err := s.RevokeApproval(approval, chainID)
		if err != nil {
			continue
		}
		
		txs = append(txs, &RevokeTx{
			To:         approval.Token,
			Data:       data,
			Approval:   approval,
		})
	}
	
	return txs, nil
}

// RevokeTx represents a revoke transaction
type RevokeTx struct {
	To       string     `json:"to"`
	Data    string     `json:"data"`
	Approval *Approval `json:"approval"`
}

// GetHighRiskApprovals gets all high-risk approvals
func (s *TokenRevokerService) GetHighRiskApprovals(address string) ([]*Approval, error) {
	result, err := s.ScanApprovals(address, 1)
	if err != nil {
		return nil, err
	}
	
	highRisk := make([]*Approval, 0)
	for _, app := range result.Approvals {
		if app.RiskLevel >= RiskHigh {
			highRisk = append(highRisk, app)
		}
	}
	
	return highRisk, nil
}

// GetTokenApprovals gets all approvals for a specific token
func (s *TokenRevokerService) GetTokenApprovals(token string) ([]*Approval, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	var result []*Approval
	for _, approvals := range s.approvals {
		for _, app := range approvals {
			if strings.ToLower(app.Token) == strings.ToLower(token) {
				result = append(result, app)
			}
		}
	}
	
	return result, nil
}

// AddApproval adds an approval to tracking
func (s *TokenRevokerService) AddApproval(approval *Approval) error {
	if approval == nil {
		return fmt.Errorf("nil approval")
	}
	
	s.mu.Lock()
	s.approvals[approval.Owner] = append(s.approvals[approval.Owner], approval)
	s.mu.Unlock()
	
	return nil
}

// GetStats gets revocation statistics
func (s *TokenRevokerService) GetStats() (*RevokerStats, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	totalApprovals := 0
	highRisk := 0
	revoked := 0
	
	for _, approvals := range s.approvals {
		for _, app := range approvals {
			totalApprovals++
			if app.RiskLevel >= RiskHigh {
				highRisk++
			}
			if app.IsRevoked {
				revoked++
			}
		}
	}
	
	return &RevokerStats{
		TotalApprovals: totalApprovals,
		HighRisk:    highRisk,
		Revoked:    revoked,
	}, nil
}

// RevokerStats represents revocation statistics
type RevokerStats struct {
	TotalApprovals int `json:"totalApprovals"`
	HighRisk      int `json:"highRisk"`
	Revoked      int `json:"revoked"`
}

// AddMaliciousSpender adds a malicious spender to blocklist
func (s *TokenRevokerService) AddMaliciousSpender(address string) error {
	// In production, would persist to database
	return nil
}

// GetMaliciousSpenders gets list of malicious spenders
func (s *TokenRevokerService) GetMaliciousSpenders() ([]string, error) {
	return []string{
		"0x0000000000000000000000000000000000000000",
	}, nil
}

// StartMonitoring starts approval monitoring
func (s *TokenRevokerService) StartMonitoring() {
	go func() {
		for {
			select {
			case req := <-s.scanQueue:
				result, _ := s.ScanApprovals(req.Address, req.ChainID)
				req.Callback <- result
			}
		}
	}()
}

// QueueScan queues a scan request
func (s *TokenRevokerService) QueueScan(address string, chainID uint64) chan *ScanResult {
	callback := make(chan *ScanResult, 1)
	s.scanQueue <- &ScanRequest{
		Address:   address,
		ChainID:   chainID,
		Callback: callback,
	}
	return callback
}

// InitTokenRevokerService initializes the service
func InitTokenRevokerService() (*TokenRevokerService, error) {
	return NewTokenRevokerService(), nil
}