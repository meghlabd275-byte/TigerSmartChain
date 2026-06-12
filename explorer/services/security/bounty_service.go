// Package bounty provides bug bounty program integration for smart contracts
package bounty

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"html/template"
	"strings"
	"sync"
	"time"
)

// =============================================================================
// DATA STRUCTURES
// =============================================================================

// BountyService manages bug bounty programs
type BountyService struct {
	db          *sql.DB
	programs    map[string]*BountyProgram
	submissions map[string]*Submission
	mu         sync.RWMutex
}

// BountyProgram represents a bug bounty program
type BountyProgram struct {
	ID              string            `json:"id"`
	Name           string            `json:"name"`
	ContractAddr   string            `json:"contractAddress"`
	ProtocolName   string            `json:"protocolName"`
	Website       string            `json:"website"`
	Description    string            `json:"description"`
	Rewards       *RewardStructure `json:"rewards"`
	Scopes        []Scope           `json:"scopes"`
	Status        string            `json:"status"` // active, paused, closed
	StartDate     time.Time        `json:"startDate"`
	EndDate       time.Time        `json:"endDate"`
	MaxPayout     string           `json:"maxPayout"`
	TotalPaid     string           `json:"totalPaid"`
	HackerCount   int              `json:"hackerCount"`
	ReportCount   int              `json:"reportCount"`
	CreatedAt    time.Time        `json:"createdAt"`
	UpdatedAt    time.Time        `json:"updatedAt"`
}

// RewardStructure defines reward tiers
type RewardStructure struct {
	Critical string `json:"critical"` // $50,000+
	High     string `json:"high"`     // $10,000-$50,000
	Medium   string `json:"medium"`   // $1,000-$10,000
	Low      string `json:"low"`      // $100-$1,000
	Info     string `json:"info"`     // $0-$100
}

// Scope defines what's in scope for bounty
type Scope struct {
	Type        string   `json:"type"` // contract, domain, ip
	Value       string   `json:"value"`
	Description string   `json:"description"`
	Archived   bool     `json:"archived"`
}

// Submission represents a bug submission
type Submission struct {
	ID            string        `json:"id"`
	ProgramID     string        `json:"programId"`
	Reporter     string        `json:"reporter"` // wallet address
	Email        string        `json:"email"`
	Severity     string        `json:"severity"` // critical, high, medium, low, info
	Title        string        `json:"title"`
	Description  string        `json:"description"`
	Status       string        `json:"status"` // submitted, triaged, resolved, closed, duplicate
	PoC          string        `json:"poc"`    // proof of concept
	Impact       string        `json:"impact"`
	Fix          string        `json:"fix"`
	Reward       string        `json:"reward"`
	Assignee     string        `json:"assignee"`
	Comments     []Comment     `json:"comments"`
	CreatedAt   time.Time    `json:"createdAt"`
	UpdatedAt   time.Time    `json:"updatedAt"`
}

// Comment represents a comment on a submission
type Comment struct {
	ID        string    `json:"id"`
	Author   string    `json:"author"`
	Content  string    `json:"content"`
	CreatedAt time.Time `json:"createdAt"`
}

// Hacker represents a bug hunter profile
type Hacker struct {
	Address      string   `json:"address"`
	Username     string   `json:"username"`
	Reputation  int     `json:"reputation"`
	TotalEarned string  `json:"totalEarned"`
	BountiesWon int     `json:"bountiesWon"`
	Rank         int     `json:"rank"`
}

// =============================================================================
// CONSTRUCTOR
// =============================================================================

// NewBountyService creates a new bounty service
func NewBountyService(db *sql.DB) *BountyService {
	return &BountyService{
		db:          db,
		programs:    make(map[string]*BountyProgram),
		submissions: make(map[string]*Submission),
	}
}

// =============================================================================
// PROGRAM MANAGEMENT
// =============================================================================

// CreateProgram creates a new bounty program
func (s *BountyService) CreateProgram(ctx context.Context, program *BountyProgram) error {
	if program == nil {
		return fmt.Errorf("nil program")
	}

	program.ID = generateID()
	program.CreatedAt = time.Now()
	program.UpdatedAt = time.Now()
	program.Status = "active"

	s.mu.Lock()
	s.programs[program.ID] = program
	s.mu.Unlock()

	// Save to database
	return s.saveProgram(ctx, program)
}

// GetProgram returns a program by ID
func (s *BountyService) GetProgram(id string) (*BountyProgram, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	program, ok := s.programs[id]
	if !ok {
		return nil, fmt.Errorf("program not found")
	}

	return program, nil
}

// GetProgramByContract returns a program by contract address
func (s *BountyService) GetProgramByContract(contractAddr string) (*BountyProgram, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	for _, program := range s.programs {
		if strings.EqualFold(program.ContractAddr, contractAddr) {
			return program, nil
		}
	}

	return nil, fmt.Errorf("program not found")
}

// ListPrograms returns all bounty programs
func (s *BountyService) ListPrograms(status string) []*BountyProgram {
	s.mu.RLock()
	defer s.mu.RUnlock()

	var result []*BountyProgram
	for _, program := range s.programs {
		if status == "" || program.Status == status {
			result = append(result, program)
		}
	}

	return result
}

// UpdateProgram updates a program
func (s *BountyService) UpdateProgram(ctx context.Context, id string, updates map[string]interface{}) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	program, ok := s.programs[id]
	if !ok {
		return fmt.Errorf("program not found")
	}

	// Apply updates
	if status, ok := updates["status"].(string); ok {
		program.Status = status
	}
	if endDate, ok := updates["endDate"].(time.Time); ok {
		program.EndDate = endDate
	}
	if maxPayout, ok := updates["maxPayout"].(string); ok {
		program.MaxPayout = maxPayout
	}

	program.UpdatedAt = time.Now()

	return s.saveProgram(ctx, program)
}

// =============================================================================
// SUBMISSION MANAGEMENT
// =============================================================================

// SubmitBug submits a new bug report
func (s *BountyService) SubmitBug(ctx context.Context, submission *Submission) error {
	if submission == nil {
		return fmt.Errorf("nil submission")
	}

	submission.ID = generateID()
	submission.Status = "submitted"
	submission.CreatedAt = time.Now()
	submission.UpdatedAt = time.Now()

	s.mu.Lock()
	s.submissions[submission.ID] = submission
	s.mu.Unlock()

	return s.saveSubmission(ctx, submission)
}

// GetSubmission returns a submission by ID
func (s *BountyService) GetSubmission(id string) (*Submission, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	submission, ok := s.submissions[id]
	if !ok {
		return nil, fmt.Errorf("submission not found")
	}

	return submission, nil
}

// GetSubmissionsByProgram returns all submissions for a program
func (s *BountyService) GetSubmissionsByProgram(programID string, status string) []*Submission {
	s.mu.RLock()
	defer s.mu.RUnlock()

	var result []*Submission
	for _, submission := range s.submissions {
		if submission.ProgramID == programID {
			if status == "" || submission.Status == status {
				result = append(result, submission)
			}
		}
	}

	return result
}

// UpdateSubmissionStatus updates submission status
func (s *BountyService) UpdateSubmissionStatus(ctx context.Context, id, status, assignee string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	submission, ok := s.submissions[id]
	if !ok {
		return fmt.Errorf("submission not found")
	}

	submission.Status = status
	submission.Assignee = assignee
	submission.UpdatedAt = time.Now()

	return s.saveSubmission(ctx, submission)
}

// =============================================================================
// HACKER LEADERBOARD
// =============================================================================

// GetTopHackers returns top bug hunters
func (s *BountyService) GetTopHackers(limit int) []*Hacker {
	s.mu.RLock()
	defer s.mu.RUnlock()

	// Aggregate by reporter
	hackers := make(map[string]*Hacker)
	for _, submission := range s.submissions {
		if submission.Status == "resolved" && submission.Reward != "" {
			if _, ok := hackers[submission.Reporter]; !ok {
				hackers[submission.Reporter] = &Hacker{
					Address:      submission.Reporter,
					TotalEarned: "0",
				}
			}
			hackers[submission.Reporter].BountiesWon++
		}
	}

	// Convert to slice and sort
	result := make([]*Hacker, 0, len(hackers))
	for _, hacker := range hackers {
		result = append(result, hacker)
	}

	// Simple ranking by bounty count
	for i := 0; i < len(result)-1; i++ {
		for j := i + 1; j < len(result); j++ {
			if result[j].BountiesWon > result[i].BountiesWon {
				result[i], result[j] = result[j], result[i]
			}
		}
	}

	if limit > 0 && len(result) > limit {
		result = result[:limit]
	}

	// Add ranks
	for i, hacker := range result {
		hacker.Rank = i + 1
	}

	return result
}

// =============================================================================
// STATISTICS
// =============================================================================

// GetBountyStats returns bounty program statistics
func (s *BountyService) GetBountyStats() map[string]interface{} {
	s.mu.RLock()
	defer s.mu.RUnlock()

	totalPrograms := len(s.programs)
	activePrograms := 0
	totalSubmissions := len(s.submissions)
	resolvedSubmissions := 0
	totalPaid := 0.0

	for _, program := range s.programs {
		if program.Status == "active" {
			activePrograms++
		}
	}

	for _, submission := range s.submissions {
		if submission.Status == "resolved" {
			resolvedSubmissions++
			// Parse reward (simplified)
			if submission.Reward != "" {
				fmt.Sscanf(submission.Reward, "%f", &totalPaid)
			}
		}
	}

	return map[string]interface{}{
		"totalPrograms":     totalPrograms,
		"activePrograms":   activePrograms,
		"totalSubmissions": totalSubmissions,
		"resolvedReports": resolvedSubmissions,
		"totalPaid":       fmt.Sprintf("$%.2f", totalPaid),
	}
}

// =============================================================================
// DATABASE OPERATIONS
// =============================================================================

func (s *BountyService) saveProgram(ctx context.Context, program *BountyProgram) error {
	if s.db == nil {
		return nil
	}

	query := `
		INSERT INTO bounty_programs (id, name, contract_address, protocol_name, website, description,
			rewards, scopes, status, start_date, end_date, max_payout, total_paid, hacker_count,
			report_count, created_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
		ON CONFLICT (id) DO UPDATE SET
			status = EXCLUDED.status,
			end_date = EXCLUDED.end_date,
			max_payout = EXCLUDED.max_payout,
			total_paid = EXCLUDED.total_paid,
			updated_at = NOW()
	`

	rewardsJSON, _ := json.Marshal(program.Rewards)
	scopesJSON, _ := json.Marshal(program.Scopes)

	_, err := s.db.ExecContext(ctx, query,
		program.ID, program.Name, program.ContractAddr, program.ProtocolName,
		program.Website, program.Description, rewardsJSON, scopesJSON,
		program.Status, program.StartDate, program.EndDate, program.MaxPayout,
		program.TotalPaid, program.HackerCount, program.ReportCount,
		program.CreatedAt, program.UpdatedAt,
	)

	return err
}

func (s *BountyService) saveSubmission(ctx context.Context, submission *Submission) error {
	if s.db == nil {
		return nil
	}

	query := `
		INSERT INTO bounty_submissions (id, program_id, reporter, email, severity, title, description,
			status, poc, impact, fix, reward, assignee, created_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
		ON CONFLICT (id) DO UPDATE SET
			status = EXCLUDED.status,
			assignee = EXCLUDED.assignee,
			reward = EXCLUDED.reward,
			updated_at = NOW()
	`

	_, err := s.db.ExecContext(ctx, query,
		submission.ID, submission.ProgramID, submission.Reporter, submission.Email,
		submission.Severity, submission.Title, submission.Description, submission.Status,
		submission.PoC, submission.Impact, submission.Fix, submission.Reward,
		submission.Assignee, submission.CreatedAt, submission.UpdatedAt,
	)

	return err
}

// =============================================================================
// HELPERS
// =============================================================================

func generateID() string {
	return fmt.Sprintf("bounty_%d", time.Now().UnixNano())
}

// GenerateReportMarkdown generates markdown for a bug report
func GenerateReportMarkdown(submission *Submission, program *BountyProgram) (string, error) {
	tmpl := `# Bug Report: {{.Title}}

## Program
- **Name**: {{.Program.Name}}
- **Contract**: {{.Program.ContractAddr}}

## Severity
**{{.Submission.Severity}}**

## Description
{{.Submission.Description}}

## Impact
{{.Submission.Impact}}

## Proof of Concept
{{.Submission.PoC}}

## Fix Suggestion
{{.Submission.Fix}}

---
Submitted: {{.Submission.CreatedAt}}
`

	data := struct {
		Submission *Submission
		Program   *BountyProgram
	}{
		Submission: submission,
		Program:   program,
	}

	var buf strings.Builder
	t, err := template.New("report").Parse(tmpl)
	if err != nil {
		return "", err
	}

	if err := t.Execute(&buf, data); err != nil {
		return "", err
	}

	return buf.String(), nil
}

var _ = json.Marshal
var _ = fmt.Sprintf
var _ = strings.ToLower
var _ = template.New
var _ = time.Now