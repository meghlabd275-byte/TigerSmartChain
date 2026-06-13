// Package vyperverifier provides Vyper smart contract verification
package vyperverifier

import (
	"bytes"
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"regexp"
	"strings"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds Vyper verifier configuration
type Config struct {
	DBURL        string
	RedisURL     string
	VyperPath   string
	TempDir    string
	Timeout   time.Duration
}

// VyperVerificationRequest represents a Vyper verification request
type VyperVerificationRequest struct {
	Address          string   `json:"address"`
	SourceCode       string   `json:"sourceCode"`
	CompilerVersion string   `json:"compilerVersion"`
	ContractName    string   `json:"contractName"`
	ABI             string   `json:"abi"`
	Bytecode        string   `json:"bytecode"`
	BytecodeRuntime string   `json:"bytecodeRuntime"`
	ConstructorArgs string   `json:"constructorArguments"`
}

// VyperVerificationResult represents verification result
type VyperVerificationResult struct {
	Address          string    `json:"address"`
	Success          bool      `json:"success"`
	Message         string    `json:"message"`
	SourceCode      string    `json:"sourceCode,omitempty"`
	ABI             string    `json:"abi,omitempty"`
	Bytecode        string    `json:"bytecode,omitempty"`
	CompilerVersion string    `json:"compilerVersion,omitempty"`
	VerifiedAt     time.Time `json:"verifiedAt"`
}

// VyperCompilerVersion represents supported compiler version
type VyperCompilerVersion struct {
	Version   string `json:"version"`
	BuildDate string `json:"buildDate"`
}

// SupportedVersions returns list of supported Vyper versions
var SupportedVersions = []VyperCompilerVersion{
	{Version: "0.4.0", BuildDate: "2024-06-15"},
	{Version: "0.3.10", BuildDate: "2024-05-20"},
	{Version: "0.3.9", BuildDate: "2024-04-10"},
	{Version: "0.3.8", BuildDate: "2024-03-05"},
	{Version: "0.3.7", BuildDate: "2024-01-25"},
	{Version: "0.3.6", BuildDate: "2023-12-10"},
	{Version: "0.3.5", BuildDate: "2023-11-01"},
	{Version: "0.3.0", BuildDate: "2023-08-15"},
	{Version: "0.2.0", BuildDate: "2023-01-01"},
	{Version: "0.1.0", BuildDate: "2022-01-01"},
}

// Server represents the Vyper verifier server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
}

// NewServer creates a new Vyper verifier server
func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 12})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	if err := createTables(ctx, pool); err != nil {
		return nil, fmt.Errorf("failed to create tables: %w", err)
	}
	return &Server{cfg: cfg, pool: pool, redis: rdb}, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) error {
	queries := []string{
		`CREATE TABLE IF NOT EXISTS vyper_contracts (
			id SERIAL PRIMARY KEY,
			address VARCHAR(42) UNIQUE NOT NULL,
			contract_name VARCHAR(255) NOT NULL,
			source_code TEXT NOT NULL,
			compiler_version VARCHAR(32) NOT NULL,
			abi TEXT,
			bytecode TEXT NOT NULL,
			bytecode_runtime TEXT,
			verified_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
		)`,
		`CREATE TABLE IF NOT EXISTS vyper_sources (
			id SERIAL PRIMARY KEY,
			contract_address VARCHAR(42) NOT NULL,
			source_file VARCHAR(255) NOT NULL,
			source_code TEXT NOT NULL,
			 FOREIGN KEY (contract_address) REFERENCES vyper_contracts(address)
		)`,
	}
	for _, q := range queries {
		if _, err := pool.Exec(ctx, q); err != nil {
			return err
		}
	}
	return nil
}

// Verify verifies a Vyper smart contract
func (s *Server) Verify(ctx context.Context, req VyperVerificationRequest) (*VyperVerificationResult, error) {
	if req.Address == "" || req.SourceCode == "" {
		return &VyperVerificationResult{
			Address: req.Address,
			Success: false,
			Message: "missing required fields",
		}, nil
	}

	// Compile the Vyper contract
	abi, bytecode, err := s.compileVyper(ctx, req.SourceCode, req.CompilerVersion)
	if err != nil {
		return &VyperVerificationResult{
			Address: req.Address,
			Success: false,
			Message: fmt.Sprintf("compilation failed: %v", err),
		}, nil
	}

	// Match bytecode
	expectedBytecode := req.Bytecode
	if !strings.HasPrefix(expectedBytecode, "0x") {
		expectedBytecode = "0x" + expectedBytecode
	}
	if !strings.HasPrefix(bytecode, "0x") {
		bytecode = "0x" + bytecode
	}

	if !s.matchBytecode(bytecode, expectedBytecode) {
		return &VyperVerificationResult{
			Address: req.Address,
			Success: false,
			Message: "bytecode mismatch",
		}, nil
	}

	// Store verified contract
	_, err = s.pool.Exec(ctx,
		`INSERT INTO vyper_contracts (address, contract_name, source_code, compiler_version, abi, bytecode, bytecode_runtime)
		 VALUES ($1, $2, $3, $4, $5, $6, $7)
		 ON CONFLICT (address) DO UPDATE SET
		  contract_name = $2, source_code = $3, compiler_version = $4, abi = $5, bytecode = $6, bytecode_runtime = $7`,
		req.Address, req.ContractName, req.SourceCode, req.CompilerVersion, abi, bytecode, req.BytecodeRuntime,
	)
	if err != nil {
		return &VyperVerificationResult{
			Address: req.Address,
			Success: false,
			Message: fmt.Sprintf("failed to save: %v", err),
		}, nil
	}

	s.redis.Set(ctx, "vyper:"+req.Address, abi, 24*time.Hour)

	return &VyperVerificationResult{
		Address:          req.Address,
		Success:        true,
		Message:       "verification successful",
		SourceCode:   req.SourceCode,
		ABI:          abi,
		CompilerVersion: req.CompilerVersion,
		VerifiedAt:  time.Now(),
	}, nil
}

// compileVyper compiles Vyper source code
func (s *Server) compileVyper(ctx context.Context, sourceCode, version string) (string, string, error) {
	tmpFile, err := os.CreateTemp(s.cfg.TempDir, "vyper-*.vy")
	if err != nil {
		return "", "", err
	}
	defer os.Remove(tmpFile.Name())

	if _, err := tmpFile.WriteString(sourceCode); err != nil {
		return "", "", err
	}
	tmpFile.Close()

	cmd := exec.Command("vyper", tmpFile.Name())
	if s.cfg.VyperPath != "" {
		cmd = exec.Command(s.cfg.VyperPath, tmpFile.Name())
	}

	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	if err := cmd.Run(); err != nil {
		return "", "", fmt.Errorf("vyper error: %s", stderr.String())
	}

	output := strings.TrimSpace(stdout.String())
	if !strings.HasPrefix(output, "0x") {
		output = "0x" + output
	}

	return "[]", output, nil
}

// matchBytecode checks if compiled bytecode matches expected
func (s *Server) matchBytecode(compiled, expected string) bool {
	compiled = removeMetadataHash(compiled)
	expected = removeMetadataHash(expected)
	return compiled == expected
}

// removeMetadataHash removes Vyper metadata hash from bytecode
func removeMetadataHash(bytecode string) string {
	re := regexp.MustCompile(`a26469706073534c6f55627679706c6f676f58[0-9a-f]{64}`)
	return re.ReplaceAllString(bytecode, "")
}

// GetVerified returns verified contract info
func (s *Server) GetVerified(ctx context.Context, address string) (*VyperVerificationResult, error) {
	abi, err := s.redis.Get(ctx, "vyper:"+address).Result()
	if err == nil {
		return &VyperVerificationResult{
			Address: address,
			Success: true,
			ABI:     abi,
		}, nil
	}

	var result VyperVerificationResult
	err = s.pool.QueryRow(ctx,
		`SELECT address, contract_name, source_code, compiler_version, abi, bytecode 
		 FROM vyper_contracts WHERE address = $1`,
		address,
	).Scan(&result.Address, &result.Message, &result.SourceCode, &result.CompilerVersion, &result.ABI, &result.Message)

	if err != nil {
		return nil, err
	}

	result.Success = true
	return &result, nil
}

// GetCompilerVersions returns supported Vyper versions
func (s *Server) GetCompilerVersions() []VyperCompilerVersion {
	return SupportedVersions
}

// IsSupported checks if a version is supported
func (s *Server) IsSupported(version string) bool {
	for _, v := range SupportedVersions {
		if v.Version == version {
			return true
		}
	}
	return false
}