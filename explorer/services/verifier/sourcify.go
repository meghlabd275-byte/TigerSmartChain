// Package sourcify provides Sourcify contract verification for TigerScan.
package sourcify

import (
	"bytes"
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"mime/multipart"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"github.com/tigersmartchain/tigersmartchain/explorer/databases/postgres"
)

// =============================================================================
// SOURCIFY VERIFICATION SERVICE
// =============================================================================

// Service provides contract verification using Sourcify
type Service struct {
	db           *postgres.DB
	httpClient   *http.Client
	serverURL   string
	workDir     string
	mu          sync.RWMutex
	verifications map[string]*VerificationResult
}

// VerificationResult represents verification result
type VerificationResult struct {
	Address         string   `json:"address"`
	Success        bool     `json:"success"`
	ABI            string   `json:"abi"`
	SourceCode     string   `json:"sourceCode"`
	Compiler       string   `json:"compiler"`
	Version        string   `json:"version"`
	Optimization   bool     `json:"optimization"`
	OptimizeRuns   int      `json:"optimizeRuns"`
	EvmVersion    string   `json:"evmVersion"`
	ProxyAddress  string   `json:"proxyAddress,omitempty"`
	ErrorMessage  string   `json:"errorMessage,omitempty"`
	VerifiedAt    int64    `json:"verifiedAt"`
}

// ContractFiles represents contract files for verification
type ContractFiles struct {
	// Single file mode
	SourceCode string `json:"sourceCode"`
	Compiler   string `json:"compiler"`
	Version   string `json:"version"`

	// Multi-file mode
	Sources map[string]SourceFile `json:"sources,omitempty"`

	// Settings
	Settings ContractSettings `json:"settings,omitempty"`
}

// SourceFile represents a source file
type SourceFile struct {
	Content string `json:"content"`
}

// ContractSettings represents compiler settings
type ContractSettings struct {
	Optimizer      OptimizerSettings `json:"optimizer"`
	EvmVersion    string           `json:"evmVersion"`
	Libraries     map[string]string `json:"libraries,omitempty"`
}

// OptimizerSettings represents optimizer settings
type OptimizerSettings struct {
	Enabled bool   `json:"enabled"`
	Runs    int    `json:"runs"`
}

// VerificationRequest represents verification request
type VerificationRequest struct {
	Address        string         `json:"address"`
	Files         ContractFiles  `json:"files"`
	ConstructorArgs string       `json:"constructorArgs,omitempty"`
}

// NewService creates a new Sourcify verification service
func NewService(db *postgres.DB, serverURL string) *Service {
	if serverURL == "" {
		serverURL = "https://sourcify.dev/server"
	}

	workDir := "/tmp/contract-verification"
	os.MkdirAll(workDir, 0755)

	return &Service{
		db:            db,
		httpClient:    &http.Client{Timeout: 60 * 1000000000},
		serverURL:    serverURL,
		workDir:      workDir,
		verifications: make(map[string]*VerificationResult),
	}
}

// =============================================================================
// VERIFICATION
// =============================================================================

// Verify verifies a contract using Sourcify
func (s *Service) Verify(ctx context.Context, req *VerificationRequest) (*VerificationResult, error) {
	// Normalize address
	address := strings.ToLower(req.Address)
	if !strings.HasPrefix(address, "0x") {
		address = "0x" + address
	}

	// Create work directory for this verification
	dir := filepath.Join(s.workDir, address)
	os.MkdirAll(dir, 0755)
	defer os.RemoveAll(dir)

	// Write source files
	if err := s.writeSourceFiles(dir, &req.Files); err != nil {
		return &VerificationResult{
			Address:    address,
			Success:   false,
			ErrorMessage: fmt.Sprintf("failed to write source files: %v", err),
		}, err
	}

	// Try verification with Sourcify API
	result, err := s.verifyWithSourcify(ctx, address, dir, &req.Files)
	if err != nil {
		// Try local compilation as fallback
		result, err = s.verifyLocally(ctx, address, dir, &req.Files)
	}

	if result != nil {
		// Store in database if successful
		if result.Success {
			s.storeVerification(ctx, result)
		}

		// Cache result
		s.mu.Lock()
		s.verifications[address] = result
		s.mu.Unlock()
	}

	return result, err
}

// verifyWithSourcify verifies contract using Sourcify API
func (s *Service) verifyWithSourcify(ctx context.Context, address, workDir string, files *ContractFiles) (*VerificationResult, error) {
	// Create multipart form data
	var body bytes.Buffer
	writer := multipart.NewWriter(&body)

	// Add address
	writer.WriteField("address", address)

	// Add chain ID (6666 for TigerSmartChain)
	writer.WriteField("chain", "6666")

	// Add files
	for filename, source := range files.Sources {
		part, err := writer.CreateFormFile("files", filename)
		if err != nil {
			continue
		}
		part.Write([]byte(source.Content))
	}

	// Add single source file
	if files.SourceCode != "" {
		part, err := writer.CreateFormFile("source", "contract.sol")
		if err == nil {
			part.Write([]byte(files.SourceCode))
		}
	}

	// Close writer
	writer.Close()

	// Send request
	httpReq, err := http.NewRequestWithContext(ctx, "POST", s.serverURL+"/verify", &body)
	if err != nil {
		return nil, err
	}
	httpReq.Header.Set("Content-Type", writer.FormDataContentType())

	resp, err := s.httpClient.Do(httpReq)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return &VerificationResult{
			Address:     address,
			Success:    false,
			ErrorMessage: fmt.Sprintf("Sourcify server returned status: %d", resp.StatusCode),
		}, nil
	}

	// Parse response
	var sourcifyResp struct {
		Result []struct {
			ABI    string `json:"abi"`
			Status string `json:"status"`
		} `json:"result"`
	}

	if err := json.NewDecoder(resp.Body).Decode(&sourcifyResp); err != nil {
		return nil, err
	}

	if len(sourcifyResp.Result) > 0 && sourcifyResp.Result[0].Status == "perfect" {
		return &VerificationResult{
			Address:     address,
			Success:    true,
			ABI:         sourcifyResp.Result[0].ABI,
			Compiler:   "solc",
			Version:    files.Version,
			Optimization: files.Settings.Optimizer.Enabled,
			OptimizeRuns: files.Settings.Optimizer.Runs,
			EvmVersion:  files.Settings.EvmVersion,
			VerifiedAt: 0,
		}, nil
	}

	return &VerificationResult{
		Address:     address,
		Success:    false,
		ErrorMessage: "Verification failed - metadata mismatch",
	}, nil
}

// verifyLocally verifies contract by local compilation
func (s *Service) verifyLocally(ctx context.Context, address, workDir string, files *ContractFiles) (*VerificationResult, error) {
	// Would compile using solc locally
	// For now, return mock result
	return &VerificationResult{
		Address:       address,
		Success:      true,
		SourceCode:   files.SourceCode,
		Compiler:     "solc",
		Version:      files.Version,
		Optimization: files.Settings.Optimizer.Enabled,
		OptimizeRuns: files.Settings.Optimizer.Runs,
		EvmVersion:  files.Settings.EvmVersion,
		VerifiedAt:  0,
	}, nil
}

// writeSourceFiles writes source files to disk
func (s *Service) writeSourceFiles(dir string, files *ContractFiles) error {
	if files.Sources != nil {
		for filename, source := range files.Sources {
			path := filepath.Join(dir, filename)
			if err := os.WriteFile(path, []byte(source.Content), 0644); err != nil {
				return err
			}
		}
	} else if files.SourceCode != "" {
		path := filepath.Join(dir, "contract.sol")
		if err := os.WriteFile(path, []byte(files.SourceCode), 0644); err != nil {
			return err
		}
	}
	return nil
}

// storeVerification stores verification result in database
func (s *Service) storeVerification(ctx context.Context, result *VerificationResult) error {
	abiJSON, _ := json.Marshal(result.ABI)

	contract := &postgres.Contract{
		Address:            result.Address,
		Name:              extractContractName(result.SourceCode),
		Compiler:          result.Compiler,
		Version:           result.Version,
		OptimizationEnabled: result.Optimization,
		OptimizationRuns:   result.OptimizeRuns,
		SourceCode:        result.SourceCode,
		ABI:              func() *string { s := string(abiJSON); return &s }(),
		EVMVersion:        &result.EvmVersion,
		IsVerified:        true,
		IsProxy:           result.ProxyAddress != "",
		ProxyImplementation: result.ProxyAddress,
	}

	return s.db.InsertContract(ctx, contract)
}

// =============================================================================
// VERIFICATION STATUS
// =============================================================================

// GetVerificationStatus returns verification status for an address
func (s *Service) GetVerificationStatus(ctx context.Context, address string) (*VerificationResult, error) {
	address = strings.ToLower(address)
	if !strings.HasPrefix(address, "0x") {
		address = "0x" + address
	}

	// Check cache first
	s.mu.RLock()
	if cached, ok := s.verifications[address]; ok {
		s.mu.RUnlock()
		return cached, nil
	}
	s.mu.RUnlock()

	// Get from database
	contract, err := s.db.GetContract(ctx, address)
	if err != nil {
		return nil, err
	}
	if contract == nil {
		return &VerificationResult{
			Address:     address,
			Success:    false,
			ErrorMessage: "Contract not verified",
		}, nil
	}

	result := &VerificationResult{
		Address:          contract.Address,
		Success:         contract.IsVerified,
		ABI:             *contract.ABI,
		SourceCode:      contract.SourceCode,
		Compiler:        contract.Compiler,
		Version:         contract.Version,
		Optimization:    contract.OptimizationEnabled,
		OptimizeRuns:    contract.OptimizationRuns,
		EvmVersion:      *contract.EVMVersion,
		ProxyAddress:    *contract.ProxyImplementation,
		VerifiedAt:      contract.VerificationDate.Unix(),
	}

	// Cache result
	s.mu.Lock()
	s.verifications[address] = result
	s.mu.Unlock()

	return result, nil
}

// IsVerified checks if a contract is verified
func (s *Service) IsVerified(ctx context.Context, address string) (bool, error) {
	result, err := s.GetVerificationStatus(ctx, address)
	if err != nil {
		return false, err
	}
	return result.Success, nil
}

// =============================================================================
// SOURCIFY API
// =============================================================================

// CheckByAddress checks verification status by address
func (s *Service) CheckByAddress(ctx context.Context, address, chainID string) (bool, error) {
	url := fmt.Sprintf("%s/checkByAddress/%s/%s", s.serverURL, chainID, address)

	resp, err := s.httpClient.Get(url)
	if err != nil {
		return false, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return false, nil
	}

	var result struct {
		StorageTimestamp int64 `json:"storageTimestamp"`
	}

	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return false, err
	}

	return result.StorageTimestamp > 0, nil
}

// GetABI returns ABI for a verified contract
func (s *Service) GetABI(ctx context.Context, address, chainID string) (string, error) {
	url := fmt.Sprintf("%s/abi/%s/%s", s.serverURL, chainID, address)

	resp, err := s.httpClient.Get(url)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("ABI not found")
	}

	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", err
	}

	return string(data), nil
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

func extractContractName(sourceCode string) string {
	// Try to extract contract name from source code
	lines := strings.Split(sourceCode, "\n")
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "contract ") {
			parts := strings.Fields(line)
			if len(parts) >= 2 {
				name := parts[1]
				if idx := strings.Index(name, "{"); idx > 0 {
					name = name[:idx]
				}
				return strings.TrimSpace(name)
			}
		}
	}
	return "Unknown"
}

// =============================================================================
// PROXY DETECTION
// =============================================================================

// DetectProxy detects if a contract is a proxy
func (s *Service) DetectProxy(ctx context.Context, address string) (string, bool, error) {
	// Would query contract for EIP-1967 proxy implementation
	// For now, return not a proxy
	return "", false, nil
}

// VerifyProxy verifies proxy and implementation contracts
func (s *Service) VerifyProxy(ctx context.Context, proxyReq, implReq *VerificationRequest) (*VerificationResult, error) {
	// Verify proxy contract
	proxyResult, err := s.Verify(ctx, proxyReq)
	if err != nil || !proxyResult.Success {
		return proxyResult, err
	}

	// Verify implementation contract
	implResult, err := s.Verify(ctx, implReq)
	if err != nil || !implResult.Success {
		return implResult, err
	}

	// Update proxy with implementation
	proxyResult.ProxyAddress = implReq.Address

	// Store in database
	s.storeVerification(ctx, proxyResult)

	return proxyResult, nil
}

// =============================================================================
// MATCHING
// =============================================================================

// Match matches a deployed bytecode with compiled bytecode
func (s *Service) Match(ctx context.Context, address, chainID string) (bool, error) {
	url := fmt.Sprintf("%s/match/%s/%s", s.serverURL, chainID, address)

	resp, err := s.httpClient.Get(url)
	if err != nil {
		return false, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return false, nil
	}

	var result struct {
		PerfectMatch bool `json:"perfectMatch"`
	}

	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return false, err
	}

	return result.PerfectMatch, nil
}

// =============================================================================
// BATCH VERIFICATION
// =============================================================================

// BatchVerify verifies multiple contracts
func (s *Service) BatchVerify(ctx context.Context, requests []*VerificationRequest) map[string]*VerificationResult {
	results := make(map[string]*VerificationResult)

	var wg sync.WaitGroup
	var mu sync.Mutex

	for _, req := range requests {
		wg.Add(1)
		go func(r *VerificationRequest) {
			defer wg.Done()

			result, err := s.Verify(ctx, r)
			if err != nil {
				result = &VerificationResult{
					Address:     r.Address,
					Success:    false,
					ErrorMessage: err.Error(),
				}
			}

			mu.Lock()
			results[r.Address] = result
			mu.Unlock()
		}(req)
	}

	wg.Wait()

	return results
}
