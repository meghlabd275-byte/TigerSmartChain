// Package contractdiff provides smart contract comparison and diff viewer
package contractdiff

import (
	"context"
	"database/sql"
	"fmt"
	"regexp"
	"sort"
	"strings"
)

// =============================================================================
// DATA STRUCTURES
// =============================================================================

// DiffService provides contract comparison services
type DiffService struct {
	db          *sql.DB
	comparisons map[string]*Comparison
}

// Comparison represents a contract comparison result
type Comparison struct {
	ID           string           `json:"id"`
	ContractA   string          `json:"contractA"`
	ContractB   string          `json:"contractB"`
	Functions   []*FunctionDiff `json:"functions"`
	Variables   []*VariableDiff `json:"variables"`
	Events      []*EventDiff    `json:"events"`
	AddedLines  int             `json:"addedLines"`
	RemovedLines int            `json:"removedLines"`
	Similarity  float64         `json:"similarity"`
	CreatedAt  string          `json:"createdAt"`
}

// FunctionDiff represents a function difference
type FunctionDiff struct {
	Name       string `json:"name"`
	Visibility string `json:"visibility"`
	Status     string `json:"status"`
	SignatureA string `json:"signatureA"`
	SignatureB string `json:"signatureB"`
}

// VariableDiff represents a state variable difference
type VariableDiff struct {
	Name   string `json:"name"`
	Type   string `json:"type"`
	Status string `json:"status"`
}

// EventDiff represents an event difference
type EventDiff struct {
	Name   string `json:"name"`
	Status string `json:"status"`
}

// =============================================================================
// CONSTRUCTOR
// =============================================================================

// NewDiffService creates a new diff service
func NewDiffService(db *sql.DB) *DiffService {
	return &DiffService{
		db:          db,
		comparisons: make(map[string]*Comparison),
	}
}

// =============================================================================
// COMPARISON
// =============================================================================

// Compare compares two contract sources
func (s *DiffService) Compare(ctx context.Context, contractA, contractB, sourceA, sourceB string) (*Comparison, error) {
	if sourceA == "" || sourceB == "" {
		return nil, fmt.Errorf("source code required")
	}

	comparison := &Comparison{
		ID:         fmt.Sprintf("diff_%d", len(s.comparisons)+1),
		ContractA: contractA,
		ContractB: contractB,
	}

	functionsA := parseFunctions(sourceA)
	functionsB := parseFunctions(sourceB)
	comparison.Functions = compareFunctions(functionsA, functionsB)

	variablesA := parseVariables(sourceA)
	variablesB := parseVariables(sourceB)
	comparison.Variables = compareVariables(variablesA, variablesB)

	eventsA := parseEvents(sourceA)
	eventsB := parseEvents(sourceB)
	comparison.Events = compareEvents(eventsA, eventsB)

	comparison.AddedLines = countAdded(sourceA, sourceB)
	comparison.RemovedLines = countRemoved(sourceA, sourceB)
	comparison.Similarity = calculateSimilarity(sourceA, sourceB)
	comparison.CreatedAt = "now"

	s.comparisons[comparison.ID] = comparison

	return comparison, nil
}

// GetComparison returns a comparison by ID
func (s *DiffService) GetComparison(id string) (*Comparison, error) {
	comp, ok := s.comparisons[id]
	if !ok {
		return nil, fmt.Errorf("comparison not found")
	}
	return comp, nil
}

// =============================================================================
// PARSING
// =============================================================================

// parseFunctions extracts functions from source code
func parseFunctions(source string) map[string]*FunctionDef {
	functions := make(map[string]*FunctionDef)
	lines := strings.Split(source, "\n")
	funcPattern := regexp.MustCompile(`function\s+(\w+)\s*\(`)

	for i, line := range lines {
		matches := funcPattern.FindStringSubmatch(line)
		if matches != nil {
			fn := &FunctionDef{
				Name:       matches[1],
				LineNumber: i + 1,
			}
			if strings.Contains(line, "public") {
				fn.Visibility = "public"
			} else if strings.Contains(line, "private") {
				fn.Visibility = "private"
			}
			functions[fn.Name] = fn
		}
	}
	return functions
}

// FunctionDef represents a parsed function
type FunctionDef struct {
	Name       string
	Visibility string
	LineNumber int
}

// parseVariables extracts state variables
func parseVariables(source string) map[string]*VariableDef {
	variables := make(map[string]*VariableDef)
	lines := strings.Split(source, "\n")
	varPattern := regexp.MustCompile(`(?:uint|int|address|bool|string|bytes)(\d*)\s+(\w+)`)

	for i, line := range lines {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "function") || strings.HasPrefix(line, "event") {
			continue
		}
		matches := varPattern.FindStringSubmatch(line)
		if matches != nil {
			v := &VariableDef{
				Name:       matches[2],
				Type:       matches[1],
				LineNumber: i + 1,
			}
			variables[v.Name] = v
		}
	}
	return variables
}

// VariableDef represents a parsed variable
type VariableDef struct {
	Name       string
	Type       string
	LineNumber int
}

// parseEvents extracts events
func parseEvents(source string) map[string]*EventDef {
	events := make(map[string]*EventDef)
	lines := strings.Split(source, "\n")
	eventPattern := regexp.MustCompile(`event\s+(\w+)\s*\(`)

	for i, line := range lines {
		matches := eventPattern.FindStringSubmatch(line)
		if matches != nil {
			e := &EventDef{
				Name:       matches[1],
				LineNumber: i + 1,
			}
			events[e.Name] = e
		}
	}
	return events
}

// EventDef represents a parsed event
type EventDef struct {
	Name       string
	LineNumber int
}

// =============================================================================
// COMPARISON LOGIC
// =============================================================================

// compareFunctions compares two sets of functions
func compareFunctions(funcsA, funcsB map[string]*FunctionDef) []*FunctionDiff {
	var diffs []*FunctionDiff

	for name, fnB := range funcsB {
		if fnA, exists := funcsA[name]; exists {
			if fnA.Visibility != fnB.Visibility {
				diffs = append(diffs, &FunctionDiff{Name: name, Visibility: fnB.Visibility, Status: "modified"})
			} else {
				diffs = append(diffs, &FunctionDiff{Name: name, Visibility: fnB.Visibility, Status: "unchanged"})
			}
		} else {
			diffs = append(diffs, &FunctionDiff{Name: name, Visibility: fnB.Visibility, Status: "added"})
		}
	}

	for name, fnA := range funcsA {
		if _, exists := funcsB[name]; !exists {
			diffs = append(diffs, &FunctionDiff{Name: name, Visibility: fnA.Visibility, Status: "removed"})
		}
	}

	sort.Slice(diffs, func(i, j int) bool {
		priority := map[string]int{"removed": 0, "modified": 1, "added": 2, "unchanged": 3}
		return priority[diffs[i].Status] < priority[diffs[j].Status]
	})

	return diffs
}

// compareVariables compares state variables
func compareVariables(varsA, varsB map[string]*VariableDef) []*VariableDiff {
	var diffs []*VariableDiff

	for name, varB := range varsB {
		if varA, exists := varsA[name]; exists {
			if varA.Type != varB.Type {
				diffs = append(diffs, &VariableDiff{Name: name, Type: varB.Type, Status: "modified"})
			} else {
				diffs = append(diffs, &VariableDiff{Name: name, Type: varB.Type, Status: "unchanged"})
			}
		} else {
			diffs = append(diffs, &VariableDiff{Name: name, Type: varB.Type, Status: "added"})
		}
	}

	for name, varA := range varsA {
		if _, exists := varsB[name]; !exists {
			diffs = append(diffs, &VariableDiff{Name: name, Type: varA.Type, Status: "removed"})
		}
	}

	return diffs
}

// compareEvents compares events
func compareEvents(eventsA, eventsB map[string]*EventDef) []*EventDiff {
	var diffs []*EventDiff

	for name, eventB := range eventsB {
		if eventA, exists := eventsA[name]; exists {
			diffs = append(diffs, &EventDiff{Name: name, Status: "unchanged"})
		} else {
			diffs = append(diffs, &EventDiff{Name: name, Status: "added"})
		}
	}

	for name, eventA := range eventsA {
		if _, exists := eventsB[name]; !exists {
			diffs = append(diffs, &EventDiff{Name: name, Status: "removed"})
		}
	}

	return diffs
}

// =============================================================================
// SIMILARITY & COUNTS
// =============================================================================

func countAdded(sourceA, sourceB string) int {
	linesA := strings.Split(sourceA, "\n")
	linesB := strings.Split(sourceB, "\n")
	count := 0
	maxLines := len(linesA)
	if len(linesB) > maxLines {
		maxLines = len(linesB)
	}
	for i := 0; i < maxLines; i++ {
		if i >= len(linesA) || i >= len(linesB) {
			count++
		} else if linesA[i] != linesB[i] {
			count++
		}
	}
	return count
}

func countRemoved(sourceA, sourceB string) int {
	return countAdded(sourceB, sourceA)
}

// calculateSimilarity calculates similarity between two sources
func calculateSimilarity(sourceA, sourceB string) float64 {
	a := normalizeSource(sourceA)
	b := normalizeSource(sourceB)

	tokensA := tokenize(a)
	tokensB := tokenize(b)

	if len(tokensA) == 0 && len(tokensB) == 0 {
		return 1.0
	}

	intersection := 0
	seen := make(map[string]bool)
	for _, token := range tokensA {
		seen[token] = true
	}
	for _, token := range tokensB {
		if seen[token] {
			intersection++
		}
	}

	union := len(tokensA) + len(tokensB) - intersection
	if union == 0 {
		return 1.0
	}

	return float64(intersection) / float64(union)
}

func normalizeSource(source string) string {
	source = regexp.MustCompile(`//.*$`).ReplaceAllString(source, "")
	source = regexp.MustCompile(`/\*.*?\*/`).ReplaceAllString(source, "")
	source = regexp.MustCompile(`\s+`).ReplaceAllString(source, " ")
	return strings.ToLower(source)
}

func tokenize(source string) []string {
	return strings.Split(normalizeSource(source), " ")
}

var _ = fmt.Sprintf
var _ = strings.Split
var _ = regexp.MustCompile
var _ = sort.Slice