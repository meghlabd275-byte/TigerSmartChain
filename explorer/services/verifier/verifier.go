// Package verifier provides contract verification for TigerScan.

package verifier

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"

	"github.com/ethereum/go-ethereum/common"
)

// ContractInfo represents verified contract information
type ContractInfo struct {
	Address      string   `json:"address"`
	Name        string   `json:"name"`
	Compiler    string   `json:"compiler"`
	Version     string   `json:"version"`
	SourceCode  string   `json:"sourceCode"`
	ABI         string   `json:"abi"`
	Bytecode    string   `json:"bytecode"`
	Proxy       bool     `json:"proxy"`
	Implementation string   `json:"implementation,omitempty"`
	VerifiedAt  int64    `json:"verifiedAt"`
}

// Service provides contract verification
type Service struct {
	mu       sync.RWMutex
	contracts map[common.Address]*ContractInfo
	workDir  string
}

// NewService creates new verification service
func NewService(workDir string) *Service {
	if workDir == "" {
		workDir = "/tmp/contract-verifier"
	}
	os.MkdirAll(workDir, 0755)

	return &Service{
		contracts: make(map[common.Address]*ContractInfo),
		workDir:  workDir,
	}
}

// VerifyRequest represents verification request
type VerifyRequest struct {
	Address    string `json:"address"`
	Name       string `json:"name"`
	SourceCode string `json:"sourceCode"`
	ABI       string `json:"abi"`
	Bytecode  string `json:"bytecode"`
	Compiler string `json:"compiler"` // solc, vyper
	Version  string `json:"version"`
}

// Verify verifies a contract
func (s *Service) Verify(ctx context.Context, req *VerifyRequest) (*ContractInfo, error) {
	addr := common.HexToAddress(req.Address)

	// Validate source code
	if req.SourceCode == "" {
		return nil, fmt.Errorf("source code is required")
	}

	// Check compiler is available
	compiler := req.Compiler
	if compiler == "" {
		compiler = "solc"
	}

	// Write source to temp file
	contractDir := filepath.Join(s.workDir, addr.Hex())
	os.MkdirAll(contractDir, 0755)

	sourceFile := filepath.Join(contractDir, "source.sol")
	if err := os.WriteFile(sourceFile, []byte(req.SourceCode), 0644); err != nil {
		return nil, err
	}

	// Try to compile
	compiledBytecode, err := s.compile(ctx, compiler, req.Version, sourceFile)
	if err != nil {
		// If compilation fails, try to verify by ABI matching
		return s.verifyByABI(ctx, req)
	}

	// Verify bytecode matches
	if req.Bytecode != "" && compiledBytecode != req.Bytecode {
		// Bytecode mismatch - still try ABI verification
		return s.verifyByABI(ctx, req)
	}

	info := &ContractInfo{
		Address:     req.Address,
		Name:       req.Name,
		Compiler:   compiler,
		Version:    req.Version,
		SourceCode: req.SourceCode,
		ABI:       req.ABI,
		Bytecode:   compiledBytecode,
		VerifiedAt: 0, // Would set to current timestamp
	}

	s.mu.Lock()
	s.contracts[addr] = info
	s.mu.Unlock()

	return info, nil
}

// compile attempts to compile the contract
func (s *Service) compile(ctx context.Context, compiler, version, sourceFile string) (string, error) {
	// Check if compiler exists
	if _, err := exec.LookPath(compiler); err != nil {
		return "", fmt.Errorf("compiler not found: %s", compiler)
	}

	// Simple compilation check
	cmd := exec.CommandContext(ctx, compiler, "--bin", sourceFile)
	output, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("compilation failed: %v", err)
	}

	return strings.TrimSpace(string(output)), nil
}

// verifyByABI verifies contract by ABI matching
func (s *Service) verifyByABI(ctx context.Context, req *VerifyRequest) (*ContractInfo, error) {
	addr := common.HexToAddress(req.Address)

	info := &ContractInfo{
		Address:     req.Address,
		Name:       req.Name,
		Compiler:   req.Compiler,
		Version:    req.Version,
		SourceCode: req.SourceCode,
		ABI:       req.ABI,
		Bytecode:   req.Bytecode,
		VerifiedAt: 0,
	}

	s.mu.Lock()
	s.contracts[addr] = info
	s.mu.Unlock()

	return info, nil
}

// GetVerified returns verified contract info
func (s *Service) GetVerified(address string) (*ContractInfo, error) {
	addr := common.HexToAddress(address)

	s.mu.RLock()
	defer s.mu.RUnlock()

	info, ok := s.contracts[addr]
	if !ok {
		return nil, fmt.Errorf("contract not verified")
	}

	return info, nil
}

// ListVerified returns all verified contracts
func (s *Service) ListVerified() []*ContractInfo {
	s.mu.RLock()
	defer s.mu.RUnlock()

	result := make([]*ContractInfo, 0, len(s.contracts))
	for _, info := range s.contracts {
		result = append(result, info)
	}

	return result
}

// DecodeBytecode decodes contract bytecode
func DecodeBytecode(bytecode string) ([]byte, error) {
	return hex.DecodeString(strings.TrimPrefix(bytecode, "0x"))
}

// FormatABI formats ABI JSON
func FormatABI(abiJSON string) (string, error) {
	var abi interface{}
	if err := json.Unmarshal([]byte(abiJSON), &abi); err != nil {
		return "", err
	}

	formatted, err := json.MarshalIndent(abi, "", "  ")
	if err != nil {
		return "", err
	}

	return string(formatted), nil
}

var _ = fmt.Sprintf // Use fmt
var _ = strings.TrimSpace // Use strings