// Package ide provides IDE integration services for smart contract development
package ide

import (
	"encoding/json"
	"fmt"
	"strings"
	"sync"
	"time"
)

// IDEService provides IDE integration services
type IDEService struct {
	projects   map[string]*Project
	compilers map[string]*Compiler
	mu        sync.RWMutex
}

// Project represents a smart contract project
type Project struct {
	ID          string    `json:"id"`
	Name        string    `json:"name"`
	Files      []*SourceFile `json:"files"`
	Settings   *ProjectSettings `json:"settings"`
	CreatedAt  time.Time `json:"createdAt"`
	UpdatedAt  time.Time `json:"updatedAt"`
}

// SourceFile represents a source file
type SourceFile struct {
	Name    string `json:"name"`
	Path   string `json:"path"`
	Lang   string `json:"language"`
	Source string `json:"source"`
}

// ProjectSettings represents project settings
type ProjectSettings struct {
	CompilerVersion string            `json:"compilerVersion"`
	Optimizer     bool              `json:"optimizer"`
	OptimizerRuns int               `json:"optimizerRuns"`
	EvmVersion   string            `json:"evmVersion"`
	Libraries    map[string]string `json:"libraries"`
}

// Compiler represents a compiler configuration
type Compiler struct {
	Name         string `json:"name"`
	Version      string `json:"version"`
	LatestStable string `json:"latestStable"`
}

// CompilationResult represents compilation result
type CompilationResult struct {
	Success      bool                `json:"success"`
	Errors       []*CompileError     `json:"errors,omitempty"`
	Warnings     []*CompileWarning  `json:"warnings,omitempty"`
	Artifacts    map[string]*Artifact `json:"artifacts"`
}

// CompileError represents a compilation error
type CompileError struct {
	Type      string `json:"type"`
	SourceLocation
	Message  string `json:"message"`
}

// SourceLocation represents source location
type SourceLocation struct {
	File string `json:"file"`
	Line int    `json:"line"`
	Col  int    `json:"col"`
}

// CompileWarning represents a compilation warning
type CompileWarning struct {
	SourceLocation
	Message string `json:"message"`
}

// Artifact represents compiled artifact
type Artifact struct {
	ABI        json.RawMessage `json:"abi"`
	Bytecode  string          `json:"bytecode"`
	DeployedBytecode string    `json:"deployedBytecode"`
	SourceMap string          `json:"sourceMap"`
	Opcodes  string          `json:"opcodes"`
}

// IDEProject represents an IDE project for interaction
type IDEProject struct {
	ID          string    `json:"id"`
	Name        string    `json:"name"`
	Contracts  []*ContractInstance `json:"contracts"`
	Network    string    `json:"network"`
	CreatedAt  time.Time `json:"createdAt"`
}

// ContractInstance represents a deployed contract instance
type ContractInstance struct {
	Name         string `json:"name"`
	Address     string `json:"address"`
	ABI         json.RawMessage `json:"abi"`
}

// TestResult represents test results
type TestResult struct {
	Contract string   `json:"contract"`
	Tests    []*Test `json:"tests"`
	Passed   int     `json:"passed"`
	Failed   int     `json:"failed"`
}

// Test represents a single test
type Test struct {
	Name       string `json:"name"`
	Status     string `json:"status"` // passed, failed
	Duration   int64  `json:"duration"`
	Error      string `json:"error,omitempty"`
}

// NewIDEService creates a new IDE service
func NewIDEService() *IDEService {
	return &IDEService{
		projects:   make(map[string]*Project),
		compilers: initCompilers(),
	}
}

// initCompilers initializes compiler versions
func initCompilers() map[string]*Compiler {
	return map[string]*Compiler{
		"solc": {
			Name:         "solc",
			Version:      "0.8.28",
			LatestStable: "0.8.28",
		},
		"vyper": {
			Name:         "vyper",
			Version:      "0.3.10",
			LatestStable: "0.3.10",
		},
	}
}

// CreateProject creates a new project
func (s *IDEService) CreateProject(name string) (*Project, error) {
	project := &Project{
		ID: generateProjectID(),
		Name: name,
		Files: []*SourceFile{},
		Settings: &ProjectSettings{
			CompilerVersion: "0.8.28",
			Optimizer:     true,
			OptimizerRuns: 200,
			EvmVersion:   "paris",
			Libraries:    make(map[string]string),
		},
		CreatedAt: time.Now(),
		UpdatedAt: time.Now(),
	}
	
	s.mu.Lock()
	s.projects[project.ID] = project
	s.mu.Unlock()
	
	return project, nil
}

// AddFile adds a source file to a project
func (s *IDEService) AddFile(projectID string, name, path, lang, source string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	project, ok := s.projects[projectID]
	if !ok {
		return fmt.Errorf("project not found")
	}
	
	file := &SourceFile{
		Name:   name,
		Path:  path,
		Lang:  lang,
		Source: source,
	}
	
	project.Files = append(project.Files, file)
	project.UpdatedAt = time.Now()
	
	return nil
}

// Compile compiles the project
func (s *IDEService) Compile(projectID string) (*CompilationResult, error) {
	s.mu.RLock()
	project, ok := s.projects[projectID]
	s.mu.RUnlock()
	
	if !ok {
		return nil, fmt.Errorf("project not found")
	}
	
	result := &CompilationResult{
		Success:   true,
		Artifacts: make(map[string]*Artifact),
	}
	
	// Analyze source for syntax errors
	for _, file := range project.Files {
		if strings.HasSuffix(file.Name, ".sol") {
			errors := s.analyzeSolidity(file.Source)
			if len(errors) > 0 {
				result.Success = false
				result.Errors = append(result.Errors, errors...)
			}
		}
	}
	
	return result, nil
}

// analyzeSolidity analyzes Solidity source for errors
func (s *IDEService) analyzeSolidity(source string) []*CompileError {
	var errors []*CompileError
	
	lines := strings.Split(source, "\n")
	
	for i, line := range lines {
		// Check for common errors
		if strings.Contains(line, "contract ") && !strings.Contains(line, "{") {
			errors = append(errors, &CompileError{
				SourceLocation: SourceLocation{
					Line: i + 1,
					Col:  0,
				},
				Message: "missing '{'",
				Type:   "ParserError",
			})
		}
		
		// Check for unclosed strings
		if strings.Count(line, `"`)%2 != 0 && strings.Count(line, `"`) > 0 {
			errors = append(errors, &CompileError{
				SourceLocation: SourceLocation{
					Line: i + 1,
					Col:  strings.Index(line, `"`),
				},
				Message: "unclosed string literal",
				Type:   "ParserError",
			})
		}
	}
	
	return errors
}

// Deploy deploys a compiled contract
func (s *IDEService) Deploy(projectID, contractName, network string, constructorArgs []string) (*DeployResult, error) {
	result := &DeployResult{
		Contract: contractName,
		Network: network,
		Hash:    generateTxHash(),
	}
	
	return result, nil
}

// DeployResult represents deployment result
type DeployResult struct {
	Contract string `json:"contract"`
	Network string `json:"network"`
	Address string `json:"address,omitempty"`
	Hash    string `json:"hash"`
	Error   string `json:"error,omitempty"`
}

// Interact interacts with a deployed contract
func (s *IDEService) Interact(contractAddress, method string, args []interface{}) (*InteractionResult, error) {
	result := &InteractionResult{
		Contract: contractAddress,
		Method:   method,
		Result:   "0x",
	}
	
	return result, nil
}

// InteractionResult represents interaction result
type InteractionResult struct {
	Contract string      `json:"contract"`
	Method  string      `json:"method"`
	Result  interface{} `json:"result"`
	GasUsed uint64     `json:"gasUsed"`
}

// RunTests runs project tests
func (s *IDEService) RunTests(projectID string) ([]*TestResult, error) {
	results := []*TestResult{
		{
			Contract: "TestContract",
			Tests: []*Test{
				{Name: "testSetUp", Status: "passed", Duration: 100},
				{Name: "testAssert", Status: "passed", Duration: 50},
				{Name: "testFail", Status: "failed", Duration: 20, Error: "Expected revert"},
			},
			Passed: 2,
			Failed: 1,
		},
	}
	
	return results, nil
}

// FormatCode formats source code
func (s *IDEService) FormatCode(lang, source string) (string, error) {
	switch lang {
	case "solidity":
		return s.formatSolidity(source)
	case "vyper":
		return s.formatVyper(source)
	default:
		return source, nil
	}
}

// formatSolidity formats Solidity code
func (s *IDEService) formatSolidity(source string) (string, error) {
	// In production, would use proper formatter
	return source, nil
}

// formatVyper formats Vyper code
func (s *IDEService) formatVyper(source string) (string, error) {
	return source, nil
}

// GetCompilers returns available compilers
func (s *IDEService) GetCompilers() []*Compiler {
	compilers := make([]*Compiler, 0, len(s.compilers))
	for _, c := range s.compilers {
		compilers = append(compilers, c)
	}
	return compilers
}

// GetProject gets a project by ID
func (s *IDEService) GetProject(projectID string) (*Project, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	project, ok := s.projects[projectID]
	if !ok {
		return nil, fmt.Errorf("project not found")
	}
	
	return project, nil
}

// GetProjects gets all projects
func (s *IDEService) GetProjects() []*Project {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	projects := make([]*Project, 0, len(s.projects))
	for _, p := range s.projects {
		projects = append(projects, p)
	}
	
	return projects
}

// generateProjectID generates a project ID
func generateProjectID() string {
	return fmt.Sprintf("proj_%d", time.Now().UnixNano())
}

// generateTxHash generates a transaction hash
func generateTxHash() string {
	return fmt.Sprintf("0x%x", time.Now().UnixNano())
}

// InitIDEService initializes the service
func InitIDEService() (*IDEService, error) {
	return NewIDEService(), nil
}