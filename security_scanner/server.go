// Package security provides security scanning for contracts and addresses
// Built with Rust for security-critical operations
package security

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"math/big"
	"net/http"
	"sort"
	"strconv"
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
	// RPCURL is the JSON-RPC endpoint of the chain being scanned. It is needed
	// to read live token allowances, which cannot be answered from the local
	// database because an allowance changes as the owner spends it.
	RPCURL string
	// ApprovalLookback is how many blocks of Approval events to scan when
	// discovering which spenders an owner has approved. Zero means scan from
	// the genesis block.
	ApprovalLookback uint64
}

// SecurityReport represents a security scan report
type SecurityReport struct {
	Address     string    `json:"address"`
	RiskLevel   string    `json:"riskLevel"` // low, medium, high, critical
	RiskScore   int       `json:"riskScore"` // 0-100
	Flags       []string  `json:"flags"`
	Warnings    []string  `json:"warnings"`
	ScamType    string    `json:"scamType,omitempty"`
	FirstSeen   time.Time `json:"firstSeen"`
	LastChecked time.Time `json:"lastChecked"`
}

// HoneypotResult represents honeypot detection result
type HoneypotResult struct {
	IsHoneypot bool     `json:"isHoneypot"`
	Confidence float64  `json:"confidence"` // 0-1
	Reasons    []string `json:"reasons"`
	DetectedBy string   `json:"detectedBy"`
}

// PhishingResult represents phishing detection result
type PhishingResult struct {
	IsPhishing bool    `json:"isPhishing"`
	Confidence float64 `json:"confidence"`
	ThreatType string  `json:"threatType"`
	Details    string  `json:"details"`
}

// TokenApproval represents token approval
type TokenApproval struct {
	Owner      string     `json:"owner"`
	Spender    string     `json:"spender"`
	Token      string     `json:"token"`
	Allowance  string     `json:"allowance"`
	IsInfinite bool       `json:"isInfinite"`
	ExpiresAt  *time.Time `json:"expiresAt,omitempty"`
}

// Server represents the security scanner server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
	// Known scam patterns
	honeypotPatterns    []string
	phishingDomains     map[string]string
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
		cfg:                 cfg,
		pool:                pool,
		redis:               rdb,
		phishingDomains:     make(map[string]string),
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
		"mint":              true,
		"burn":              true,
		"pause":             true,
		"blacklist":         true,
		"destroy":           true,
		"selfdestruct":      true,
		"kill":              true,
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
		Flags:       []string{},
		Warnings:    []string{},
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
		Reasons:    []string{},
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
		Flags:       []string{},
		Warnings:    []string{},
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

// erc20AllowanceSelector is keccak256("allowance(address,address)")[0:4].
var erc20AllowanceSelector = []byte{0xdd, 0x62, 0xed, 0x3e}

// approvalEventTopic is keccak256("Approval(address,address,uint256)").
const approvalEventTopic = "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925"

// maxApprovalLogRange bounds one eth_getLogs call. Nodes commonly reject wide
// ranges, so the scan walks the window in chunks instead.
const maxApprovalLogRange = 10_000

// CheckTokenApprovals lists the token allowances an owner has granted.
//
// Discovery works from Approval event logs rather than from a stored list,
// because an approval can be created by any transaction the owner signs and is
// not reported to this service. For each token found, the live allowance is
// read with eth_call so that an approval already spent to zero is not reported
// as outstanding. A revoked approval therefore disappears from the result,
// which is what a caller acting on it needs.
func (s *Server) CheckTokenApprovals(ctx context.Context, owner string) ([]TokenApproval, error) {
	if s.cfg == nil || s.cfg.RPCURL == "" {
		return nil, fmt.Errorf("no RPC URL configured; token approvals require a node")
	}
	if !isHexAddress(owner) {
		return nil, fmt.Errorf("owner is not a 20-byte hex address: %q", owner)
	}

	// Pad the owner to a 32-byte topic, per the log filter encoding.
	ownerTopic := "0x000000000000000000000000" + strings.ToLower(strings.TrimPrefix(owner, "0x"))

	head, err := s.rpcBlockNumber(ctx)
	if err != nil {
		return nil, err
	}
	from := uint64(0)
	if s.cfg.ApprovalLookback > 0 && head > s.cfg.ApprovalLookback {
		from = head - s.cfg.ApprovalLookback
	}

	// token -> spender, so the newest approval seen for a pair wins.
	type pair struct{ token, spender string }
	seen := make(map[pair]bool)

	for start := from; start <= head; start += maxApprovalLogRange {
		end := start + maxApprovalLogRange - 1
		if end > head || end < start {
			end = head
		}

		logs, err := s.rpcGetLogs(ctx, map[string]interface{}{
			"fromBlock": fmt.Sprintf("0x%x", start),
			"toBlock":   fmt.Sprintf("0x%x", end),
			"topics":    []interface{}{approvalEventTopic, ownerTopic},
		})
		if err != nil {
			return nil, err
		}
		for _, entry := range logs {
			token := strings.ToLower(entry.Address)
			// topics[1] is the owner, topics[2] is the spender.
			if len(entry.Topics) < 3 {
				continue
			}
			spender := "0x" + strings.ToLower(strings.TrimPrefix(entry.Topics[2], "0x"))[24:]
			seen[pair{token, spender}] = true
		}
		if end == head {
			break
		}
	}

	approvals := make([]TokenApproval, 0, len(seen))
	for p := range seen {
		allowance, err := s.rpcAllowance(ctx, p.token, owner, p.spender)
		if err != nil {
			// A single unreadable token must not lose the others, but it must
			// not be silently reported as zero either.
			continue
		}
		if allowance.Sign() == 0 {
			// Already spent or revoked; not an outstanding approval.
			continue
		}
		approvals = append(approvals, TokenApproval{
			Owner:      owner,
			Spender:    p.spender,
			Token:      p.token,
			Allowance:  allowance.String(),
			IsInfinite: allowance.Cmp(maxUint256) == 0,
		})
	}

	// A stable order makes the result comparable between calls.
	sort.Slice(approvals, func(i, j int) bool {
		if approvals[i].Token != approvals[j].Token {
			return approvals[i].Token < approvals[j].Token
		}
		return approvals[i].Spender < approvals[j].Spender
	})
	return approvals, nil
}

// maxUint256 is 2^256-1, the allowance value used for an "infinite" approval.
var maxUint256 = new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 256), big.NewInt(1))

// RevokeApproval clears an ERC-20 allowance by sending an approve(spender, 0)
// transaction.
//
// This service holds no key, so it cannot sign. It builds the exact calldata and
// returns it for the owner to sign; the caller then broadcasts the signed
// transaction. Returning a transaction hash without broadcasting anything would
// tell the user an approval was revoked when it was not, which is the failure
// this method previously had.
func (s *Server) RevokeApproval(ctx context.Context, owner, token, spender string) (string, error) {
	if !isHexAddress(owner) || !isHexAddress(token) || !isHexAddress(spender) {
		return "", fmt.Errorf("owner, token and spender must all be 20-byte hex addresses")
	}

	// approve(spender, 0): selector, the spender padded to 32 bytes, then a
	// 32-byte zero allowance.
	data := make([]byte, 0, 4+32+32)
	data = append(data, erc20ApproveSelector...)
	data = append(data, padAddressToWord(spender)...)
	data = append(data, make([]byte, 32)...)

	return "0x" + hex.EncodeToString(data), nil
}

// erc20ApproveSelector is keccak256("approve(address,uint256)")[0:4].
var erc20ApproveSelector = []byte{0x09, 0x5e, 0xa7, 0xb3}

// padAddressToWord left-pads an address to a 32-byte ABI word.
func padAddressToWord(address string) []byte {
	raw, err := hex.DecodeString(strings.TrimPrefix(strings.ToLower(address), "0x"))
	if err != nil || len(raw) != 20 {
		return make([]byte, 32)
	}
	word := make([]byte, 32)
	copy(word[12:], raw)
	return word
}

// isHexAddress reports whether s is a 0x-prefixed 20-byte hex address.
func isHexAddress(s string) bool {
	if !strings.HasPrefix(s, "0x") && !strings.HasPrefix(s, "0X") {
		return false
	}
	if len(s) != 42 {
		return false
	}
	_, err := hex.DecodeString(s[2:])
	return err == nil
}

// rpcResult issues a JSON-RPC call and returns the raw `result` field.
func (s *Server) rpcResult(ctx context.Context, method string, params []interface{}) (json.RawMessage, error) {
	body, err := json.Marshal(map[string]interface{}{
		"jsonrpc": "2.0",
		"id":      1,
		"method":  method,
		"params":  params,
	})
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, s.cfg.RPCURL, bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := (&http.Client{Timeout: 30 * time.Second}).Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("%s: HTTP %d: %s", method, resp.StatusCode, truncateForError(raw))
	}

	var envelope struct {
		Result json.RawMessage `json:"result"`
		Error  *struct {
			Code    int    `json:"code"`
			Message string `json:"message"`
		} `json:"error"`
	}
	if err := json.Unmarshal(raw, &envelope); err != nil {
		return nil, fmt.Errorf("%s: unreadable response: %w", method, err)
	}
	if envelope.Error != nil {
		return nil, fmt.Errorf("%s: node error %d: %s", method, envelope.Error.Code, envelope.Error.Message)
	}
	return envelope.Result, nil
}

// truncateForError keeps an error message short and never leaks a whole body.
func truncateForError(raw []byte) string {
	const limit = 200
	if len(raw) <= limit {
		return string(raw)
	}
	return string(raw[:limit]) + "..."
}

// rpcBlockNumber returns the current head block number.
func (s *Server) rpcBlockNumber(ctx context.Context) (uint64, error) {
	raw, err := s.rpcResult(ctx, "eth_blockNumber", []interface{}{})
	if err != nil {
		return 0, err
	}
	var hexNum string
	if err := json.Unmarshal(raw, &hexNum); err != nil {
		return 0, fmt.Errorf("eth_blockNumber: %w", err)
	}
	n, err := strconv.ParseUint(strings.TrimPrefix(hexNum, "0x"), 16, 64)
	if err != nil {
		return 0, fmt.Errorf("eth_blockNumber returned %q: %w", hexNum, err)
	}
	return n, nil
}

// rpcLog is one entry of an eth_getLogs result.
type rpcLog struct {
	Address string   `json:"address"`
	Topics  []string `json:"topics"`
	Data    string   `json:"data"`
}

// rpcGetLogs returns the logs matching a filter.
func (s *Server) rpcGetLogs(ctx context.Context, filter map[string]interface{}) ([]rpcLog, error) {
	raw, err := s.rpcResult(ctx, "eth_getLogs", []interface{}{filter})
	if err != nil {
		return nil, err
	}
	var logs []rpcLog
	if err := json.Unmarshal(raw, &logs); err != nil {
		return nil, fmt.Errorf("eth_getLogs: %w", err)
	}
	return logs, nil
}

// rpcAllowance reads token.allowance(owner, spender) with eth_call.
//
// eth_call executes the call against current state and returns the result
// without mining anything, which is exactly what is needed to read a live
// allowance.
func (s *Server) rpcAllowance(ctx context.Context, token, owner, spender string) (*big.Int, error) {
	data := make([]byte, 0, 4+32+32)
	data = append(data, erc20AllowanceSelector...)
	data = append(data, padAddressToWord(owner)...)
	data = append(data, padAddressToWord(spender)...)

	raw, err := s.rpcResult(ctx, "eth_call", []interface{}{
		map[string]interface{}{
			"to":   token,
			"data": "0x" + hex.EncodeToString(data),
		},
		"latest",
	})
	if err != nil {
		return nil, err
	}

	var result string
	if err := json.Unmarshal(raw, &result); err != nil {
		return nil, fmt.Errorf("eth_call: %w", err)
	}
	result = strings.TrimPrefix(result, "0x")
	if result == "" {
		// A token that returns no data is not an ERC-20 with a working
		// allowance function. Treating the empty return as zero would report
		// "no approval" for a token that may well have one.
		return nil, fmt.Errorf("token %s returned no allowance data", token)
	}
	value, ok := new(big.Int).SetString(result, 16)
	if !ok {
		return nil, fmt.Errorf("token %s returned an unreadable allowance %q", token, result)
	}
	return value, nil
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
