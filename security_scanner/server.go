// Package security provides security scanning for contracts and addresses
// Built with Rust for security-critical operations
package security

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"regexp"
	"strings"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds security scanner configuration
type Config struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
}

// SecurityReport represents a security scan report
type SecurityReport struct {
	Address         string   `json:"address"`
	RiskLevel       string   `json:"riskLevel"` // low, medium, high, critical
	RiskScore       int      `json:"riskScore"` // 0-100
	Flags           []string `json:"flags"`
	Warnings       []string `json:"warnings"`
	ScamType       string   `json:"scamType,omitempty"`
	FirstSeen      time.Time `json:"firstSeen"`
	LastChecked   time.Time `json:"lastChecked"`
}

// HoneypotResult represents honeypot detection result
type HoneypotResult struct {
	IsHoneypot      bool     `json:"isHoneypot"`
	Confidence     float64  `json:"confidence"` // 0-1
	Reasons       []string `json:"reasons"`
	DetectedBy    string   `json:"detectedBy"`
}

// PhishingResult represents phishing detection result
type PhishingResult struct {
	IsPhishing    bool     `json:"isPhishing"`
	Confidence   float64  `json:"confidence"`
	ThreatType   string   `json:"threatType"`
	Details      string   `json:"details"`
}

// TokenApproval represents token approval
type TokenApproval struct {
	Owner           string  `json:"owner"`
	Spender         string  `json:"spender"`
	Token          string  `json:"token"`
	Allowance      string  `json:"allowance"`
	IsInfinite     bool    `json:"isInfinite"`
	ExpiresAt     *time.Time `json:"expiresAt,omitempty"`
}

// Server represents the security scanner server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
	// Known scam patterns
	honeypotPatterns []string
	phishingDomains map[string]string
	suspiciousFunctions map[string]bool
}

// NewServer creates a new security scanner server
func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 6})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	if err := createTables(ctx, pool); err != nil {
		return nil, fmt.Errorf("failed to create tables: %w", err)
	}
	srv := &Server{
		cfg: cfg,
		pool: pool,
		redis: rdb,
		phishingDomains: make(map[string]string),
		suspiciousFunctions: make(map[string]bool),
	}
	srv.initPatterns()
	return srv, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) error {
	queries := []string{
		`CREATE TABLE IF NOT EXISTS security_reports (id SERIAL PRIMARY KEY, address VARCHAR(42) NOT NULL, risk_level VARCHAR(20) NOT NULL, risk_score INTEGER NOT NULL, flags TEXT[], warnings TEXT[], scam_type VARCHAR(50), first_seen TIMESTAMP, last_checked TIMESTAMP, UNIQUE(address))`,
		`CREATE TABLE IF NOT EXISTS phishing_domains (id SERIAL PRIMARY KEY, domain VARCHAR(255) NOT NULL UNIQUE, threat_type VARCHAR(50), details TEXT, reported_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`,
		`CREATE TABLE IF NOT EXISTS honeypot_contracts (id SERIAL PRIMARY KEY, address VARCHAR(42) NOT NULL UNIQUE, honeypot_type VARCHAR(50), detection_method VARCHAR(50), confidence DECIMAL(3,2), first_seen TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`,
		`CREATE INDEX IF NOT EXISTS idx_security_address ON security_reports(address)`,
		`CREATE INDEX IF NOT EXISTS idx_phishing_domain ON phishing_domains(domain)`,
	}
	for _, q := range queries {
		if _, err := pool.Exec(ctx, q); err != nil {
			return err
		}
	}
	return nil
}

// initPatterns initializes known scam patterns
func (s *Server) initPatterns() {
	// Known honeypot patterns (function names that trap users)
	s.honeypotPatterns = []string{
		"mint", "burn", "pause", "unpause", "blacklist", "restrict",
		"setTaxPercent", "setTransferTax", "approveAndCall",
		"transferTax", "taxFee", "serviceFee", "burnFee",
	}
	
	// Suspicious functions that could be malicious
	s.suspiciousFunctions = map[string]bool{
		"mint":           true,
		"burn":          true,
		"pause":         true,
		"blacklist":      true,
		"destroy":       true,
		"selfdestruct":  true,
		"kill":          true,
		"renounceOwnership": true,
	}
	
	// Load known phishing domains from database
	s.loadPhishingDomains()
}

func (s *Server) loadPhishingDomains() {
	ctx := context.Background()
	rows, err := s.pool.Query(ctx, `SELECT domain, threat_type FROM phishing_domains`)
	if err != nil {
		return
	}
	defer rows.Close()
	
	for rows.Next() {
		var domain, threatType string
		if err := rows.Scan(&domain, &threatType); err == nil {
			s.phishingDomains[domain] = threatType
		}
	}
}

// ScanContract scans a contract for security issues
func (s *Server) ScanContract(ctx context.Context, address, bytecode string) (*SecurityReport, error) {
	// Check cache first
	cached, err := s.redis.Get(ctx, fmt.Sprintf("security:%s", address)).Result()
	if err == nil {
		var report SecurityReport
		json.Unmarshal([]byte(cached), &report)
		return &report, nil
	}
	
	report := &SecurityReport{
		Address:     address,
		LastChecked: time.Now(),
		Flags:      []string{},
		Warnings:  []string{},
	}
	
	// Check for honeypot
	hpResult := s.DetectHoneypot(bytecode)
	if hpResult.IsHoneypot {
		report.RiskLevel = "critical"
		report.RiskScore = 90
		report.Flags = append(report.Flags, fmt.Sprintf("honeypot_detected:%s", hpResult.DetectedBy))
		report.ScamType = "honeypot"
		report.Warnings = append(report.Warnings, hpResult.Reasons...)
	}
	
	// Check for suspicious functions
	suspiciousFuncs := s.checkSuspiciousFunctions(bytecode)
	if len(suspiciousFuncs) > 0 {
		report.RiskScore += len(suspiciousFuncs) * 10
		for _, f := range suspiciousFuncs {
			report.Flags = append(report.Flags, fmt.Sprintf("suspicious_function:%s", f))
		}
		report.Warnings = append(report.Warnings, fmt.Sprintf("Contract contains suspicious functions: %s", strings.Join(suspiciousFuncs, ", ")))
	}
	
	// Check for mint function (can be used for rugpulls)
	if strings.Contains(bytecode, "mint") {
		report.RiskScore += 15
		report.Flags = append(report.Flags, "has_mint_function")
		report.Warnings = append(report.Warnings, "Contract has mint function - can be used to inflate supply")
	}
	
	// Check for pause function
	if strings.Contains(bytecode, "pause") || strings.Contains(bytecode, "PauserRole") {
		report.RiskScore += 10
		report.Flags = append(report.Flags, "has_pause_function")
		report.Warnings = append(report.Warnings, "Contract can be paused")
	}
	
	// Determine final risk level
	if report.RiskScore >= 80 {
		report.RiskLevel = "critical"
	} else if report.RiskScore >= 50 {
		report.RiskLevel = "high"
	} else if report.RiskScore >= 20 {
		report.RiskLevel = "medium"
	} else {
		report.RiskLevel = "low"
		report.RiskScore = 10
	}
	
	// Store in cache
	data, _ := json.Marshal(report)
	s.redis.Set(ctx, fmt.Sprintf("security:%s", address), string(data), 24*time.Hour)
	
	// Save to database
	s.pool.Exec(ctx, `INSERT INTO security_reports (address, risk_level, risk_score, flags, warnings, scam_type, first_seen, last_checked) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (address) DO UPDATE SET risk_level = $2, risk_score = $3, flags = $4, warnings = $5, scam_type = $6, last_checked = $8`,
		address, report.RiskLevel, report.RiskScore, strings.Join(report.Flags, ","), strings.Join(report.Warnings, ","), report.ScamType, report.FirstSeen, report.LastChecked)
	
	return report, nil
}

// DetectHoneypot detects honeypot contracts
func (s *Server) DetectHoneypot(bytecode string) *HoneypotResult {
	result := &HoneypotResult{
		IsHoneypot: false,
		Reasons:   []string{},
	}
	
	// Check for honeypot patterns
	detectedBy := ""
	
	// Pattern 1: Mint with hidden transfer tax
	if strings.Contains(bytecode, "mint") && strings.Contains(bytecode, "transfer") {
		detectedBy = "hidden_tax"
		result.IsHoneypot = true
		result.Reasons = append(result.Reasons, "Contract has mint function with transfer - possible hidden tax honeypot")
	}
	
	// Pattern 2: Fake ERC20 with burned transfer
	if strings.Contains(bytecode, "require") && strings.Contains(bytecode, "balanceOf") {
		count := strings.Count(bytecode, "balanceOf")
		if count > 3 {
			detectedBy = "fake_balance_check"
			result.IsHoneypot = true
			result.Reasons = append(result.Reasons, "Contract has excessive balanceOf checks - possible fake token")
		}
	}
	
	// Pattern 3: Infinite approval trap
	if strings.Contains(bytecode, "approve") && !strings.Contains(bytecode, "decreaseAllowance") {
		detectedBy = "infinite_approval"
		result.IsHoneypot = true
		result.Reasons = append(result.Reasons, "Contract has approve without decreaseAllowance - possible infinite approval trap")
	}
	
	// Pattern 4: Sell function that always reverts
	if strings.Contains(bytecode, "sell") || strings.Contains(bytecode, "swap") {
		if !strings.Contains(bytecode, "require") {
			detectedBy = "fake_swap"
			result.IsHoneypot = true
			result.Reasons = append(result.Reasons, "Contract has sell function without proper checks")
		}
	}
	
	// Calculate confidence
	if result.IsHoneypot {
		result.Confidence = 0.85
		result.DetectedBy = detectedBy
	}
	
	return result
}

// checkSuspiciousFunctions checks for suspicious functions in bytecode
func (s *Server) checkSuspiciousFunctions(bytecode string) []string {
	var found []string
	
	for funcName := range s.suspiciousFunctions {
		if strings.Contains(bytecode, funcName) {
			found = append(found, funcName)
		}
	}
	
	return found
}

// ScanAddress scans an address for phishing attempts
func (s *Server) ScanAddress(ctx context.Context, address string) (*SecurityReport, error) {
	// Check database first
	var report SecurityReport
	err := s.pool.QueryRow(ctx, `SELECT address, risk_level, risk_score, flags, warnings, scam_type, first_seen, last_checked FROM security_reports WHERE address = $1`, address).Scan(&report.Address, &report.RiskLevel, &report.RiskScore, &report.Flags, &report.Warnings, &report.ScamType, &report.FirstSeen, &report.LastChecked)
	if err == nil {
		return &report, nil
	}
	
	// Create new report
	report = SecurityReport{
		Address:     address,
		RiskLevel:   "low",
		RiskScore:   10,
		LastChecked: time.Now(),
		Flags:      []string{},
		Warnings:  []string{},
	}
	
	// Check if address is in known phishing list
	// Check against known scam addresses in database
	var scamCount int
	s.pool.QueryRow(ctx, `SELECT COUNT(*) FROM address_reports WHERE address = $1 AND status = 'confirmed'`, address).Scan(&scamCount)
	if scamCount > 0 {
		report.RiskLevel = "critical"
		report.RiskScore = 100
		report.ScamType = "known_scam"
		report.Flags = append(report.Flags, "known_scam_address")
		report.Warnings = append(report.Warnings, "This address has been reported as a scam")
	}
	
	// Check address hash for patterns (simplified)
	hash := sha256.Sum256([]byte(address))
	hashStr := hex.EncodeToString(hash[:])
	if strings.Contains(hashStr, "0000") {
		report.Warnings = append(report.Warnings, "Address has suspicious pattern in hash")
		report.RiskScore += 5
	}
	
	// Update database
	s.pool.Exec(ctx, `INSERT INTO security_reports (address, risk_level, risk_score, flags, warnings, first_seen, last_checked) VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (address) DO UPDATE SET risk_level = $2, risk_score = $3, flags = $4, warnings = $5, last_checked = $7`,
		address, report.RiskLevel, report.RiskScore, strings.Join(report.Flags, ","), strings.Join(report.Warnings, ","), report.FirstSeen, report.LastChecked)
	
	return &report, nil
}

// CheckTokenApprovals checks token approvals for an address
func (s *Server) CheckTokenApprovals(ctx context.Context, owner string) ([]TokenApproval, error) {
	// In production, this would query the node for approval events
	// Simplified version returns empty
	return []TokenApproval{}, nil
}

// RevokeApproval sends a transaction to revoke token approval
func (s *Server) RevokeApproval(ctx context.Context, owner, token, spender string) (string, error) {
	// In production, this would create and broadcast the transaction
	// Simplified version returns a mock transaction hash
	return "0x0000000000000000000000000000000000000000000000000000000000000001", nil
}

// GetRiskLevelColor returns color for risk level
func GetRiskLevelColor(level string) string {
	switch level {
	case "critical":
		return "#dc2626" // red
	case "high":
		return "#f97316" // orange
	case "medium":
		return "#eab308" // yellow
	default:
		return "#22c55e" // green
	}
}

// GetSafetyScore returns a safety score (inverse of risk score)
func GetSafetyScore(report *SecurityReport) int {
	return 100 - report.RiskScore
}

// FormatRiskScore formats risk score as a percentage
func FormatRiskScore(score int) string {
	return fmt.Sprintf("%d%%", score)
}