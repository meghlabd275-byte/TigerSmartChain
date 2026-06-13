// TigerSmartChain Smart Contract Verifier - Multi-file, Proxy, Libraries
// Production-grade contract verification service

package main

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
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/lib/pq"
)

// ============================================================================
// Configuration
// ============================================================================

type Config struct {
	Server     ServerConfig   `json:"server"`
	Database   DatabaseConfig `json:"database"`
	Compiler  CompilerConfig `json:"compiler"`
}

type ServerConfig struct {
	Host string `json:"host"`
	Port int    `json:"port"`
}

type DatabaseConfig struct {
	Host     string `json:"host"`
	Port     int    `json:"port"`
	Username string `json:"username"`
	Password string `json:"password"`
	Database string `json:"database"`
}

type CompilerConfig struct {
	SolidityURL       string `json:"solidity_url"`
	VyperURL         string `json:"vyper_url"`
	Timeout         int    `json:"timeout"`
	MaxSourcesSize   int    `json:"max_sources_size"`
}

// ============================================================================
// Data Models
// ============================================================================

// VerificationRequest represents a verification request
type VerificationRequest struct {
	Address         string            `json:"address"`
	ContractName    string            `json:"contract_name"`
	CompilerVersion string          `json:"compiler_version"`
	Optimization  bool              `json:"optimization"`
	OptimizerRuns uint32           `json:"optimizer_runs"`
	EvmVersion   string           `json:"evm_version"`
	License      string           `json:"license"`
	Sources      map[string]string `json:"sources"`
	LibraryLinks  map[string]string `json:"library_links"`
	ConstructorArgs string        `json:"constructor_args"`
	ABI         string          `json:"abi"`
}

// ProxyVerificationRequest represents a proxy contract verification
type ProxyVerificationRequest struct {
	ProxyAddress      string `json:"proxy_address"`
	Implementation  string `json:"implementation"`
	CompilerVersion string `json:"compiler_version"`
	ProxyType      string `json:"proxy_type"` // upgradeable, diamond
}

// LibraryInfo represents a library
type LibraryInfo struct {
	Name           string `json:"name"`
	Address        string `json:"address"`
	DeployedOn     uint64 `json:"deployed_on"`
}

// VerificationResult represents verification result
type VerificationResult struct {
	Address          string    `json:"address"`
	ContractName    string    `json:"contract_name"`
	CompilerVersion string   `json:"compiler_version"`
	Optimization  bool      `json:"optimization"`
	OptimizerRuns uint32   `json:"optimizer_runs"`
	EvmVersion    string    `json:"evm_version"`
	License       string    `json:"license"`
	VerifiedAt    time.Time `json:"verified_at"`
}

// ============================================================================
// Compiler Interface
// ============================================================================

type Compiler struct {
	client  *http.Client
	url     string
	timeout time.Duration
}

func NewCompiler(url string, timeout time.Duration) *Compiler {
	return &Compiler{
		client: &http.Client{Timeout: timeout},
		url:    url,
	}
}

func (c *Compiler) CompileSources(sources map[string]string, settings CompileSettings) (*CompilationResult, error) {
	// Prepare input
	input := map[string]interface{}{
		"language": "Solidity",
		"sources":  sources,
		"settings": settings,
	}
	
	inputJSON, err := json.Marshal(input)
	if err != nil {
		return nil, err
	}
	
	// Send compilation request
	resp, err := c.client.Post(c.url, "application/json", bytes.NewReader(inputJSON))
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	
	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("compiler error: %s", string(body))
	}
	
	var result CompilationResult
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, err
	}
	
	if len(result.Errors) > 0 {
		var errors []string
		for _, e := range result.Errors {
			if e.Severity == "error" {
				errors = append(errors, e.FormattedMessage)
			}
		}
		if len(errors) > 0 {
			return nil, fmt.Errorf("compilation errors: %s", strings.Join(errors, "\n"))
		}
	}
	
	return &result, nil
}

type CompileSettings struct {
	Optimizer  *OptimizerSettings `json:"optimizer,omitempty"`
	EvmVersion string          `json:"evmVersion,omitempty"`
	Libraries map[string]string `json:"libraries,omitempty"`
}

type OptimizerSettings struct {
	Enabled bool `json:"enabled"`
	Runs    uint32 `json:"runs"`
}

type CompilationResult struct {
	Contracts map[string]*ContractOutput `json:"contracts"`
	Errors   []CompilerError       `json:"errors"`
}

type ContractOutput struct {
	ABI            []interface{} `json:"abi"`
	Bytecode       string    `json:"bytecode"`
	DeployedBytecode string   `json:"deployedBytecode"`
}

type CompilerError struct {
	Severity         string `json:"severity"`
	FormattedMessage string `json:"formattedMessage"`
}

// ============================================================================
// Verifier Service
// ============================================================================

type Verifier struct {
	db        *sql.DB
	compiler *Compiler
	config   Config
}

func NewVerifier(config Config) (*Verifier, error) {
	dsn := fmt.Sprintf(
		"host=%s port=%d user=%s password=%s dbname=%s sslmode=disable",
		config.Database.Host, config.Database.Port,
		config.Database.Username, config.Database.Password, config.Database.Database,
	)
	
	db, err := sql.Open("postgres", dsn)
	if err != nil {
		return nil, err
	}
	
	compiler := NewCompiler(config.Compiler.SolidityURL, time.Duration(config.Compiler.Timeout)*time.Second)
	
	return &Verifier{
		db:       db,
		compiler: compiler,
		config:   config,
	}, nil
}

func (v *Verifier) Verify(request *VerificationRequest) (*VerificationResult, error) {
	// Validate request
	if err := v.validateRequest(request); err != nil {
		return nil, err
	}
	
	// Parse constructor args if provided
	var constructorArgs []byte
	if request.ConstructorArgs != "" {
		var err error
		constructorArgs, err = hex.DecodeString(strings.TrimPrefix(request.ConstructorArgs, "0x"))
		if err != nil {
			return nil, fmt.Errorf("invalid constructor args: %v", err)
		}
	}
	
	// Prepare compile settings
	settings := CompileSettings{
		Optimizer: &OptimizerSettings{
			Enabled: request.Optimization,
			Runs:    request.OptimizerRuns,
		},
		EvmVersion: request.EvmVersion,
		Libraries: request.LibraryLinks,
	}
	
	// Compile sources
	result, err := v.compiler.CompileSources(request.Sources, settings)
	if err != nil {
		return nil, err
	}
	
	// Find the contract
	contractName := request.ContractName
	if !strings.Contains(contractName, ":") {
		contractName = contractName + ":" + contractName
	}
	
	output, ok := result.Contracts[contractName]
	if !ok {
		return nil, fmt.Errorf("contract not found: %s", request.ContractName)
	}
	
	// Get deployed bytecode from chain
	storedBytecode, err := v.getStoredBytecode(request.Address)
	if err != nil {
		return nil, err
	}
	
	// Compare bytecodes
	expectedBytecode := strings.TrimPrefix(output.DeployedBytecode, "0x")
	if !bytes.Equal([]byte(storedBytecode), []byte(expectedBytecode)) {
		return nil, fmt.Errorf("bytecode mismatch: stored=%s, expected=%s", storedBytecode[:20], expectedBytecode[:20])
	}
	
	// Store verification result
	verifiedAt := time.Now()
	
	tx, err := v.db.Begin()
	if err != nil {
		return nil, err
	}
	defer tx.Rollback()
	
	// Insert contract metadata
	_, err = tx.ExecContext(context.Background(),
		`INSERT INTO contract_metadata (address, contract_name, compiler_version, optimizer, optimizer_runs, evm_version, license, source_code, abi, constructor_args, verified_at)
		 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
		 ON CONFLICT (address) DO UPDATE SET
			contract_name = EXCLUDED.contract_name,
			compiler_version = EXCLUDED.compiler_version,
			optimizer = EXCLUDED.optimizer,
			optimizer_runs = EXCLUDED.optimizer_runs,
			evm_version = EXCLUDED.evm_version,
			license = EXCLUDED.license,
			source_code = EXCLUDED.source_code,
			abi = EXCLUDED.abi,
			constructor_args = EXCLUDED.constructor_args,
			verified_at = EXCLUDED.verified_at`,
		request.Address, request.ContractName, request.CompilerVersion, request.Optimization, request.OptimizerRuns, request.EvmVersion, request.License, json.dumps(request.Sources), json.dumps(output.ABI), hex.EncodeToString(constructorArgs), verifiedAt,
	)
	if err != nil {
		return nil, err
	}
	
	// Insert verified sources
	for filename, source := range request.Sources {
		_, err = tx.ExecContext(context.Background(),
			`INSERT INTO verified_sources (address, file_name, source_code, language, compiler_version, abi)
			 VALUES ($1, $2, $3, $4, $5, $6)
			 ON CONFLICT (address, file_name) DO UPDATE SET
				source_code = EXCLUDED.source_code,
				compiler_version = EXCLUDED.compiler_version,
				abi = EXCLUDED.abi`,
			request.Address, filename, source, "Solidity", request.CompilerVersion, json.dumps(output.ABI),
		)
		if err != nil {
			return nil, err
		}
	}
	
	// Insert libraries
	for name, addr := range request.LibraryLinks {
		_, err = tx.ExecContext(context.Background(),
			`INSERT INTO libraries (name, address, deployed_on)
			 VALUES ($1, $2, (SELECT MAX(number) FROM blocks))
			 ON CONFLICT (name) DO NOTHING`,
			name, addr,
		)
		if err != nil {
			return nil, err
		}
	}
	
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	
	return &VerificationResult{
		Address:          request.Address,
		ContractName:    request.ContractName,
		CompilerVersion: request.CompilerVersion,
		Optimization:  request.Optimization,
		OptimizerRuns: request.OptimizerRuns,
		EvmVersion:    request.EvmVersion,
		License:      request.License,
		VerifiedAt:   verifiedAt,
	}, nil
}

func (v *Verifier) VerifyProxy(request *ProxyVerificationRequest) (*VerificationResult, error) {
	// Get implementation bytecode
	implBytecode, err := v.getStoredBytecode(request.Implementation)
	if err != nil {
		return nil, err
	}
	
	// Get proxy bytecode
	proxyBytecode, err := v.getStoredBytecode(request.ProxyAddress)
	if err != nil {
		return nil, err
	}
	
	// Verify based on proxy type
	switch request.ProxyType {
	case "upgradeable":
		// Check for EIP-1967 proxy
		if !strings.HasPrefix(proxyBytecode, "363d3d373d3d3f3f60203f8035") {
			return nil, fmt.Errorf("not a valid upgradeable proxy")
		}
	case "diamond":
		// Check for diamond proxy
		if !strings.Contains(proxyBytecode, "60c01b") {
			return nil, fmt.Errorf("not a valid diamond proxy")
		}
	}
	
	// Store proxy verification
	tx, err := v.db.Begin()
	if err != nil {
		return nil, err
	}
	defer tx.Rollback()
	
	_, err = tx.ExecContext(context.Background(),
		`INSERT INTO proxy_verifications (proxy_address, implementation, proxy_type, compiler_version, verified_at)
		 VALUES ($1, $2, $3, $4, NOW())
		 ON CONFLICT (proxy_address) DO UPDATE SET
			implementation = EXCLUDED.implementation,
			proxy_type = EXCLUDED.proxy_type,
			compiler_version = EXCLUDED.compiler_version,
			verified_at = EXCLUDED.verified_at`,
		request.ProxyAddress, request.Implementation, request.ProxyType, request.CompilerVersion,
	)
	if err != nil {
		return nil, err
	}
	
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	
	return &VerificationResult{
		Address:          request.ProxyAddress,
		ContractName:    "Proxy",
		CompilerVersion: request.CompilerVersion,
	}, nil
}

func (v *Verifier) validateRequest(request *VerificationRequest) error {
	if request.Address == "" {
		return fmt.Errorf("address required")
	}
	if request.ContractName == "" {
		return fmt.Errorf("contract name required")
	}
	if request.CompilerVersion == "" {
		return fmt.Errorf("compiler version required")
	}
	if len(request.Sources) == 0 {
		return fmt.Errorf("sources required")
	}
	
	// Validate compiler version format
	if ok, _ := regexp.MatchString(`^0\.[0-9]+\.[0-9]+$`, request.CompilerVersion); !ok {
		return fmt.Errorf("invalid compiler version format")
	}
	
	// Validate source size
	totalSize := 0
	for _, source := range request.Sources {
		totalSize += len(source)
	}
	if totalSize > v.config.Compiler.MaxSourcesSize {
		return fmt.Errorf("sources size exceeds maximum: %d", v.config.Compiler.MaxSourcesSize)
	}
	
	// Validate license
	validLicenses := []string{"MIT", "GPL-3.0", "LGPL-3.0", "BSD-3-Clause", "BSD-2-Clause", "Apache-2.0", "AGPL-3.0", "UNLICENSED"}
	if request.License != "" {
		found := false
		for _, l := range validLicenses {
			if l == request.License {
				found = true
				break
			}
		}
		if !found {
			return fmt.Errorf("invalid license: %s", request.License)
		}
	}
	
	return nil
}

func (v *Verifier) getStoredBytecode(address string) (string, error) {
	var bytecode string
	err := v.db.QueryRowContext(context.Background(),
		"SELECT code FROM contracts WHERE address = $1",
		address,
	).Scan(&bytecode)
	
	if err != nil {
		return "", err
	}
	
	return strings.TrimPrefix(bytecode, "0x"), nil
}

// ============================================================================
// Handlers
// ============================================================================

type Handler struct {
	verifier *Verifier
}

func NewHandler(verifier *Verifier) *Handler {
	return &Handler{verifier: verifier}
}

func (h *Handler) Verify(c *gin.Context) {
	var request VerificationRequest
	if err := c.ShouldBindJSON(&request); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	result, err := h.verifier.Verify(&request)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, result)
}

func (h *Handler) VerifyProxy(c *gin.Context) {
	var request ProxyVerificationRequest
	if err := c.ShouldBindJSON(&request); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	result, err := h.verifier.VerifyProxy(&request)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	
	c.JSON(http.StatusOK, result)
}

func (h *Handler) GetContract(c *gin.Context) {
	address := c.Param("address")
	
	var result struct {
		Address         string    `json:"address"`
		ContractName   string    `json:"contract_name"`
		CompilerVersion string   `json:"compiler_version"`
		Optimization  bool      `json:"optimization"`
		OptimizerRuns uint32    `json:"optimizer_runs"`
		EvmVersion    string    `json:"evm_version"`
		License       string    `json:"license"`
		VerifiedAt    time.Time `json:"verified_at"`
	}
	
	err := h.verifier.db.QueryRowContext(c.Request.Context(),
		"SELECT address, contract_name, compiler_version, optimizer, optimizer_runs, evm_version, license, verified_at FROM contract_metadata WHERE address = $1",
		address,
	).Scan(&result.Address, &result.ContractName, &result.CompilerVersion, &result.Optimization, &result.OptimizerRuns, &result.EvmVersion, &result.License, &result.VerifiedAt)
	
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Contract not found"})
		return
	}
	
	c.JSON(http.StatusOK, result)
}

func (h *Handler) GetSources(c *gin.Context) {
	address := c.Param("address")
	
	rows, err := h.verifier.db.QueryContext(c.Request.Context(),
		"SELECT file_name, source_code FROM verified_sources WHERE address = $1",
		address,
	)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Sources not found"})
		return
	}
	defer rows.Close()
	
	type Source struct {
		FileName   string `json:"file_name"`
		SourceCode string `json:"source_code"`
	}
	
	var sources []Source
	for rows.Next() {
		var s Source
		if err := rows.Scan(&s.FileName, &s.SourceCode); err != nil {
			continue
		}
		sources = append(sources, s)
	}
	
	c.JSON(http.StatusOK, gin.H{"data": sources})
}

func (h *Handler) GetABI(c *gin.Context) {
	address := c.Param("address")
	
	var abi string
	err := h.verifier.db.QueryRowContext(c.Request.Context(),
		"SELECT abi FROM contract_metadata WHERE address = $1",
		address,
	).Scan(&abi)
	
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "ABI not found"})
		return
	}
	
	c.JSON(http.StatusOK, gin.H{"abi": abi})
}

func (h *Handler) GetLibraries(c *gin.Context) {
	address := c.Param("address")
	
	rows, err := h.verifier.db.QueryContext(c.Request.Context(),
		`SELECT l.name, l.address, l.deployed_on 
		 FROM libraries l
		 JOIN contract_metadata cm ON cm.source_code LIKE '%' || l.name || '%'
		 WHERE cm.address = $1`,
		address,
	)
	if err != nil {
		c.JSON(http.StatusOK, gin.H{"data": []LibraryInfo{}})
		return
	}
	defer rows.Close()
	
	var libraries []LibraryInfo
	for rows.Next() {
		var l LibraryInfo
		if err := rows.Scan(&l.Name, &l.Address, &l.DeployedOn); err != nil {
			continue
		}
		libraries = append(libraries, l)
	}
	
	c.JSON(http.StatusOK, gin.H{"data": libraries})
}

// ============================================================================
// Main
// ============================================================================

func main() {
	gin.SetMode(gin.ReleaseMode)
	
	config := Config{
		Server: ServerConfig{
			Host: "0.0.0.0",
			Port: 8082,
		},
		Database: DatabaseConfig{
			Host:     "localhost",
			Port:     5432,
			Username: "tigerscan",
			Password: "tigerscan",
			Database: "tigerscan",
		},
		Compiler: CompilerConfig{
			SolidityURL:     "https://solc-bin.ethereum.org/linux-amd64/list.json",
			Timeout:       60,
			MaxSourcesSize: 1024 * 1024, // 1MB
		},
	}
	
	verifier, err := NewVerifier(config)
	if err != nil {
		fmt.Printf("Failed to create verifier: %v\n", err)
		os.Exit(1)
	}
	defer verifier.db.Close()
	
	handler := NewHandler(verifier)
	
	router := gin.New()
	router.Use(gin.Recovery())
	
	api := router.Group("/api/v1")
	{
		api.POST("/verify", handler.Verify)
		api.POST("/verify/proxy", handler.VerifyProxy)
		api.GET("/contracts/:address", handler.GetContract)
		api.GET("/contracts/:address/sources", handler.GetSources)
		api.GET("/contracts/:address/abi", handler.GetABI)
		api.GET("/contracts/:address/libraries", handler.GetLibraries)
	}
	
	addr := fmt.Sprintf("%s:%d", config.Server.Host, config.Server.Port)
	fmt.Printf("Verifier server listening on %s\n", addr)
	
	if err := router.Run(addr); err != nil {
		fmt.Printf("Server error: %v\n", err)
		os.Exit(1)
	}
}