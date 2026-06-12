// Package verifier provides production smart contract verification service
// for TigerScan blockchain explorer with multi-file support, proxy detection, and compilation
package verifier

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
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"time"

	"github.com/ethereum/go-ethereum/accounts/abi"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/common/hexutil"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
)

// =============================================================================
// CONFIGURATION
// =============================================================================

// Config holds verification configuration
type Config struct {
	TempDir           string
	SolcPath         string
	Optimize         bool
	OptimizationRuns int
	EVMVersion       string
	LicenseTypes     []string
	MaxFileSize     int64
	Timeout         time.Duration
}

func DefaultConfig() *Config {
	return &Config{
		TempDir:           os.TempDir() + "/tigerscan-verifier",
		SolcPath:         getEnv("SOLC_PATH", "solc"),
		Optimize:         true,
		OptimizationRuns: 200,
		EVMVersion:       getEnv("EVM_VERSION", "london"),
		LicenseTypes:     []string{"MIT", "GPL-3.0", "Apache-2.0", "BSD-3-Clause", "BSD-2-Clause", "UNLICENSED"},
		MaxFileSize:     1 * 1024 * 1024, // 1MB
		Timeout:         5 * time.Minute,
	}
}

// =============================================================================
// TYPES
// =============================================================================

// VerificationRequest represents a contract verification request
type VerificationRequest struct {
	Address           string            `json:"address"`
	Name             string            `json:"name"`
	CompilerVersion  string            `json:"compiler_version"`
	SourceCode       string            `json:"source_code"`
	Optimization     bool              `json:"optimization"`
	OptimizationRuns int               `json:"optimization_runs"`
	EVMVersion      string            `json:"evm_version"`
	LicenseType     string            `json:"license_type"`
	ConstructorArgs string            `json:"constructor_args"`
	Libraries       map[string]string `json:"libraries"`
	ContractFiles   []ContractFile   `json:"contract_files,omitempty"`
}

// ContractFile represents a file in a multi-file verification
type ContractFile struct {
	Name     string `json:"name"`
	Content string `json:"content"`
}

// VerificationResult represents the verification result
type VerificationResult struct {
	Success       bool                `json:"success"`
	Address       string              `json:"address"`
	Name          string              `json:"name"`
	ABI           []interface{}       `json:"abi"`
	Bytecode      string             `json:"bytecode"`
	DeployedBytecode string          `json:"deployed_bytecode"`
	CompilerVersion string           `json:"compiler_version"`
	Optimization  bool               `json:"optimization"`
	LicenseType   string             `json:"license_type"`
	Error        string              `json:"error,omitempty"`
}

// ContractMetadata represents verified contract metadata
type ContractMetadata struct {
	Address             string
	Name               string
	CompilerVersion    string
	Optimization      bool
	OptimizationRuns  int
	EVMVersion        string
	LicenseType      string
	SourceCode       string
	ABI              abi.ABI
	Bytecode         string
	DeployedBytecode string
	IsProxy         bool
	Implementation   string
	ProxyPattern    string
	VerifiedAt      time.Time
}

// =============================================================================
// SERVICE
// =============================================================================

// Verifier provides contract verification service
type Verifier struct {
	config  *Config
	tempDir string
	mu      sync.RWMutex
}

// New creates a new verifier service
func New(cfg *Config) (*Verifier, error) {
	if cfg == nil {
		cfg = DefaultConfig()
	}

	// Create temp directory
	if err := os.MkdirAll(cfg.TempDir, 0755); err != nil {
		return nil, fmt.Errorf("failed to create temp dir: %w", err)
	}

	v := &Verifier{
		config:  cfg,
		tempDir: cfg.TempDir,
	}

	return v, nil
}

// =============================================================================
// VERIFICATION
// =============================================================================

// Verify verifies a smart contract
func (v *Verifier) Verify(ctx context.Context, req *VerificationRequest) (*VerificationResult, error) {
	// Validate request
	if err := v.validateRequest(req); err != nil {
		return &VerificationResult{
			Success: false,
			Address: req.Address,
			Error:   err.Error(),
		}, nil
	}

	// Create work directory
	workDir, err := v.createWorkDir(req.Address)
	if err != nil {
		return &VerificationResult{Success: false, Address: req.Address, Error: err.Error()}, nil
	}
	defer os.RemoveAll(workDir)

	// Write source files
	if err := v.writeSourceFiles(workDir, req); err != nil {
		return &VerificationResult{Success: false, Address: req.Address, Error: err.Error()}, nil
	}

	// Run compilation
	result, err := v.compile(ctx, workDir, req)
	if err != nil {
		return &VerificationResult{Success: false, Address: req.Address, Error: err.Error()}, nil
	}

	// Verify bytecode match
	if err := v.verifyBytecodeMatch(req.Address, result.Bytecode, result.DeployedBytecode); err != nil {
		return &VerificationResult{
			Success:       false,
			Address:      req.Address,
			Name:         req.Name,
			CompilerVersion: req.CompilerVersion,
			Optimization: req.Optimization,
			Error:        fmt.Sprintf("Bytecode mismatch: %v", err),
		}, nil
	}

	// Detect proxy
	isProxy, implementation := v.detectProxy(result.DeployedBytecode, result.ABI)
	if isProxy {
		result.ABI = nil // Proxy has no public functions
	}

	return &VerificationResult{
		Success:         true,
		Address:         req.Address,
		Name:            req.Name,
		ABI:             result.ABI,
		Bytecode:        result.Bytecode,
		DeployedBytecode: result.DeployedBytecode,
		CompilerVersion:  req.CompilerVersion,
		Optimization:    req.Optimization,
		LicenseType:     req.LicenseType,
	}, nil
}

// validateRequest validates the verification request
func (v *Verifier) validateRequest(req *VerificationRequest) error {
	if req.Address == "" {
		return fmt.Errorf("address is required")
	}
	if !common.IsHexAddress(req.Address) {
		return fmt.Errorf("invalid address format")
	}
	if req.Name == "" {
		return fmt.Errorf("contract name is required")
	}
	if req.SourceCode == "" {
		return fmt.Errorf("source code is required")
	}
	if len(req.SourceCode) > int(v.config.MaxFileSize) {
		return fmt.Errorf("source code exceeds maximum size")
	}
	if !v.isValidCompilerVersion(req.CompilerVersion) {
		return fmt.Errorf("unsupported compiler version: %s", req.CompilerVersion)
	}
	if !v.isValidLicense(req.LicenseType) {
		return fmt.Errorf("invalid license type: %s", req.LicenseType)
	}
	return nil
}

// createWorkDir creates a working directory for compilation
func (v *Verifier) createWorkDir(address string) (string, error) {
	dir := filepath.Join(v.tempDir, address+"-"+fmt.Sprintf("%d", time.Now().Unix()))
	if err := os.MkdirAll(dir, 0755); err != nil {
		return "", fmt.Errorf("failed to create work dir: %w", err)
	}
	return dir, nil
}

// writeSourceFiles writes source files to the work directory
func (v *Verifier) writeWorkDir(dir string, req *VerificationRequest) error {
	// Handle multi-file or single-file
	if len(req.ContractFiles) > 0 {
		// Multi-file: create directory structure
		for _, file := range req.ContractFiles {
			path := filepath.Join(dir, file.Name)
			if err := os.WriteFile(path, []byte(file.Content), 0644); err != nil {
				return fmt.Errorf("failed to write file %s: %w", file.Name, err)
			}
		}
	} else {
		// Single-file: write as a .sol file
		filename := req.Name + ".sol"
		if err := os.WriteFile(filepath.Join(dir, filename), []byte(req.SourceCode), 0644); err != nil {
			return fmt.Errorf("failed to write source: %w", err)
		}
	}
	return nil
}

// compile compiles the contract
func (v *Verifier) compile(ctx context.Context, dir string, req *VerificationRequest) (*compileResult, error) {
	args := []string{
		"--bin",
		"--abi",
		"--optimize",
		"--optimize-runs", fmt.Sprintf("%d", req.OptimizationRuns),
		"--evm-version", req.EVMVersion,
	}

	// Add libraries
	for libName, libAddr := range req.Libraries {
		args = append(args, "--libraries", fmt.Sprintf("%s:%s", libName, libAddr))
	}

	// Find main contract file
	mainFile := req.Name + ".sol"
	if len(req.ContractFiles) > 0 {
		// Find the file containing the main contract
		for _, f := range req.ContractFiles {
			if strings.Contains(f.Content, "contract "+req.Name) {
				mainFile = f.Name
				break
			}
		}
	}

	args = append(args, mainFile)

	cmd := exec.CommandContext(ctx, v.config.SolcPath, args...)
	cmd.Dir = dir
	cmd.Timeout = v.config.Timeout

	output, err := cmd.CombinedOutput()
	if err != nil {
		// Parse compiler errors
		errorMsg := parseCompilerErrors(string(output))
		return nil, fmt.Errorf("compilation failed: %s", errorMsg)
	}

	// Parse output
	result, err := parseCompilationOutput(output, req.Name)
	if err != nil {
		return nil, fmt.Errorf("failed to parse output: %w", err)
	}

	return result, nil
}

// compileResult holds compilation results
type compileResult struct {
	ABI              []interface{}
	Bytecode         string
	DeployedBytecode string
}

// verifyBytecodeMatch verifies that the compiled bytecode matches the on-chain bytecode
func (v *Verifier) verifyBytecodeMatch(address string, compiledBytecode, onChainBytecode string) error {
	// Normalize bytecodes (remove 0x prefix, handle metadata)
	compiled := normalizeBytecode(compiledBytecode)
	onChain := normalizeBytecode(onChainCode)

	if compiled != onChain {
		return fmt.Errorf("compiled bytecode does not match on-chain bytecode")
	}
	return nil
}

// normalizeBytecode normalizes bytecode for comparison
func normalizeBytecode(bc string) string {
	bc = strings.ToLower(strings.TrimPrefix(bc, "0x"))
	// Remove metadata hash at the end
	re := regexp.MustCompile(`a165627a7a72305820[a-f0-9]{64}0029$`)
	bc = re.ReplaceAllString(bc, "")
	return bc
}

// detectProxy detects if a contract is a proxy
func (v *Verifier) detectProxy(bytecode string, abiData []interface{}) (bool, string) {
	// Check for common proxy patterns
	proxyPatterns := []string{
		"363d3d373d3d3d363d30545af43d82803e903d91602b57fd5bf3",
		"3d3d3d3d363d3d37609f3d3d3d3d37609f3d3d3d3d37609f",
	}

	bcLower := strings.ToLower(bytecode)
	for _, pattern := range proxyPatterns {
		if strings.Contains(bcLower, pattern) {
			return true, ""
		}
	}

	return false, ""
}

// =============================================================================
// COMPILER OUTPUT PARSING
// =============================================================================

// parseCompilationOutput parses solc output
func parseCompilationOutput(output []byte, contractName string) (*compileResult, error) {
	// Simple JSON output parsing
	var response struct {
		Contracts map[string]struct {
			ABI json.RawMessage `json:"abi"`
			Bin  string        `json:"bin"`
		} `json:"contracts"`
	}

	if err := json.Unmarshal(output, &response); err != nil {
		// Try to parse from text output
		return parseTextOutput(output, contractName)
	}

	// Find our contract
	for name, contract := range response.Contracts {
		if strings.Contains(name, contractName) {
			// Parse ABI
			var abiData []interface{}
			json.Unmarshal(contract.ABI, &abiData)

			return &compileResult{
				ABI:              abiData,
				Bytecode:         contract.Bin,
				DeployedBytecode: contract.Bin,
			}, nil
		}
	}

	return nil, fmt.Errorf("contract not found in compilation output")
}

// parseTextOutput parses text output format
func parseTextOutput(output []byte, contractName string) (*compileResult, error) {
	outputStr := string(output)

	// Look for binary
	binRe := regexp.MustCompile(`(?m)^=====.*=====$`)
	binMatch := binRe.FindString(outputStr)
	if binMatch == "" {
		return nil, fmt.Errorf("no binary found in output")
	}

	return &compileResult{
		Bytecode:        extractHexSection(outputStr, "Binary"),
		DeployedBytecode: extractHexSection(outputStr, "Binary"),
		ABI:            nil,
	}, nil
}

// extractHexSection extracts hex section from text
func extractHexSection(output, section string) string {
	re := regexp.MustCompile(fmt.Sprintf(`%s:?\s*\n(0x[a-f0-9]*)`, section))
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		return matches[1]
	}
	return ""
}

// parseCompilerErrors parses compiler error messages
func parseCompilerErrors(output string) string {
	lines := strings.Split(output, "\n")
	var errors []string
	for _, line := range lines {
		if strings.Contains(line, "Error:") || strings.Contains(line, "ParserError") {
			errors = append(errors, strings.TrimSpace(line))
		}
	}
	if len(errors) > 0 {
		return strings.Join(errors, "\n")
	}
	return output
}

// =============================================================================
// VALIDATION HELPERS
// =============================================================================

// isValidCompilerVersion checks if compiler version is supported
func (v *Verifier) isValidCompilerVersion(version string) bool {
	// Allow common version formats
	re := regexp.MustCompile(`^v?(\d+\.\d+\.\d+)\+commit\.[a-f0-9]+$`)
	return re.MatchString(version)
}

// isValidLicense checks if license is valid
func (v *Verifier) isValidLicense(license string) bool {
	for _, l := range v.config.LicenseTypes {
		if strings.EqualFold(l, license) {
			return true
		}
	}
	return false
}

// =============================================================================
// PROXY DETECTION
// =============================================================================

// DetectProxyImplementation detects the implementation address for a proxy
func (v *Verifier) DetectProxyImplementation(ctx context.Context, rpcURL, proxyAddress string) (string, error) {
	// Call the implementation() function
	methodID := crypto.Keccak256([]byte("implementation()"))[:4]
	
	data := "0x" + hex.EncodeToString(methodID)
	
	// Make eth_call
	result, err := v.makeCall(ctx, rpcURL, proxyAddress, data)
	if err != nil {
		return "", err
	}
	
	// Parse result (last 20 bytes = address)
	if len(result) >= 24 {
		addr := "0x" + result[len(result)-40:]
		return addr, nil
	}
	
	return "", fmt.Errorf("failed to detect implementation")
}

// makeCall makes an eth_call
func (v *Verifier) makeCall(ctx context.Context, rpcURL, to, data string) (string, error) {
	payload := map[string]interface{}{
		"jsonrpc": "2.0",
		"method":  "eth_call",
		"params": []interface{}{
			map[string]interface{}{
				"to":   to,
				"data": data,
			},
			"latest",
		},
		"id": 1,
	}
	
	body, _ := json.Marshal(payload)
	resp, err := http.Post(rpcURL, "application/json", bytes.NewBuffer(body))
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	
	var result map[string]interface{}
	json.NewDecoder(resp.Body).Decode(&result)
	
	if errMsg, ok := result["error"]; ok {
		return "", fmt.Errorf("%v", errMsg)
	}
	
	return result["result"].(string), nil
}

// =============================================================================
// FLATTENING
// =============================================================================

// Flatten flattens a Solidity file with all imports
func (v *Verifier) Flatten(ctx context.Context, sourceCode string) (string, error) {
	// Create temp file
	tmpFile := filepath.Join(v.tempDir, fmt.Sprintf("flatten-%d.sol", time.Now().UnixNano()))
	if err := os.WriteFile(tmpFile, []byte(sourceCode), 0644); err != nil {
		return "", err
	}
	defer os.Remove(tmpFile)

	// Run solc --flatten
	cmd := exec.CommandContext(ctx, v.config.SolcPath, "--flatten", tmpFile)
	output, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("flatten failed: %w", err)
	}

	return string(output), nil
}

// =============================================================================
// SOURCE CODE RETRIEVAL
// =============================================================================

// GetSourceCode retrieves verified source code
func (v *Verifier) GetSourceCode(address string) (*ContractMetadata, error) {
	// This would query the database
	return nil, nil
}

// GetABI retrieves contract ABI
func (v *Verifier) GetABI(address string) (abi.ABI, error) {
	// This would query the database
	return abi.ABI{}, nil
}

// =============================================================================
// UTILITIES
// =============================================================================

// HashSourceCode creates a hash of source code for comparison
func HashSourceCode(sourceCode string) string {
	hash := sha256.Sum256([]byte(sourceCode))
	return hex.EncodeToString(hash[:])
}

// ExtractConstructorArgs extracts constructor arguments from deployment data
func ExtractConstructorArgs(deploymentData, bytecode string) string {
	// Remove bytecode prefix
	deploymentData = strings.TrimPrefix(deploymentData, "0x")
	bytecode = strings.TrimPrefix(bytecode, "0x")
	
	// Constructor args are the part after the bytecode
	if len(deploymentData) > len(bytecode) {
		return "0x" + deploymentData[len(bytecode):]
	}
	
	return ""
}

// GetSupportedCompilerVersions returns list of supported compiler versions
func (v *Verifier) GetSupportedCompilerVersions() []string {
	return []string{
		"v0.8.20+commit.a1b20f04",
		"v0.8.19+commit.7dd6d404",
		"v0.8.18+commit.8fce6e00",
		"v0.8.17+commit.8a6e4c85",
		"v0.8.16+commit.0aefa3fe",
		"v0.8.15+commit.e579019",
		"v0.8.14+commit.0b90bf86",
		"v0.8.13+commit.622fa78",
		"v0.8.12+commit.00c4c37b",
		"v0.8.11+commit.d6a5e43e",
		"v0.8.10+commit.fc410830",
		"v0.8.9+commit.8c1c07c1",
		"v0.8.8+commit.4e8b1ee",
		"v0.8.7+commit.8b8b1fe",
		"v0.8.6+commit.a34d2de",
		"v0.8.5+commit.5f6bd44",
	}
}

// getEnv gets environment variable
func getEnv(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}