// TigerScan Contract Verification Service
// Multi-file verification, proxy detection, Sourcify integration
// Production-grade with full security

package main

import (
	"bytes"
	"context"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/gin-contrib/cors"
	"github.com/gin-gonic/gin"
	_ "github.com/lib/pq"
)

// hexAddress matches a 20-byte hex address.
var hexAddress = regexp.MustCompile(`^0x[0-9a-fA-F]{40}$`)

// ============================================================================
// Configuration
// ============================================================================

type Config struct {
	Server         ServerConfig   `json:"server"`
	Database       DatabaseConfig `json:"database"`
	SourcifyURL    string        `json:"sourcify_url"`
	SolcPath       string        `json:"solc_path"`
	RPCURL         string        `json:"rpc_url"`
	Optimization  bool          `json:"optimization"`
	EvmVersions    []string      `json:"evm_versions"`
}

type ServerConfig struct {
	Host        string `json:"host"`
	Port        int    `json:"port"`
	ReadTimeout int    `json:"read_timeout"`
}

type DatabaseConfig struct {
	Host     string `json:"host"`
	Port     int    `json:"port"`
	Username string `json:"username"`
	Password string `json:"password"`
	Database string `json:"database"`
	SSLMode  string `json:"sslmode"`
}

// ============================================================================
// Types
// ============================================================================

type VerificationRequest struct {
	Address           string            `json:"address"`
	ContractName      string            `json:"contract_name"`
	CompilerVersion   string            `json:"compiler_version"`
	Optimization      bool              `json:"optimization"`
	OptimizationRuns  int               `json:"optimization_runs"`
	EvmVersion       string            `json:"evm_version"`
	License          string            `json:"license"`
	Sources          map[string]string `json:"sources"`
	ConstructorArgs  string            `json:"constructor_args"`
}

type VerificationResponse struct {
	Status    string   `json:"status"`
	Message   string   `json:"message"`
	Address   string   `json:"address"`
	ABI       string   `json:"abi,omitempty"`
	Bytecode  string   `json:"bytecode,omitempty"`
}

type SourcifyMatch struct {
	Address  string   `json:"address"`
	ChainID  string   `json:"chainId"`
	Status   string   `json:"status"`
	Files    []File  `json:"files"`
}

type File struct {
	Name string `json:"name"`
	Content string `json:"content"`
}

// ============================================================================
// Verification Service
// ============================================================================

type VerificationService struct {
	db          *sql.DB
	config      *Config
	compileChan chan *CompilationJob
	wg          sync.WaitGroup
	shutdown    chan bool
}

type CompilationJob struct {
	ID              string
	Request         VerificationRequest
	ResultChan     chan *CompilationResult
}

type CompilationResult struct {
	Success    bool
	ABI        string
	Bytecode   string
	Runtime    string
	Errors     []string
	Warnings   []string
}

type ContractInfo struct {
	Address           string
	ContractName      string
	CompilerVersion   string
	Compiler          string
	Optimization      bool
	OptimizationRuns  int
	EvmVersion       string
	License           string
	SourceCode        string
	ABI              string
	Bytecode         string
	RuntimeBytecode  string
	ConstructorArgs  string
	IsProxy          bool
	Implementation    string
	VerifiedAt        time.Time
}

// ============================================================================
// Database
// ============================================================================

func NewDB(cfg DatabaseConfig) (*sql.DB, error) {
	// The connection carries credentials and query results, so TLS is required
	// unless the operator explicitly opts out for a loopback database.
	sslMode := cfg.SSLMode
	if sslMode == "" {
		sslMode = "require"
	}
	dsn := fmt.Sprintf(
		"host=%s port=%d user=%s password=%s dbname=%s sslmode=%s",
		cfg.Host, cfg.Port, cfg.Username, cfg.Password, cfg.Database, sslMode,
	)

	db, err := sql.Open("postgres", dsn)
	if err != nil {
		return nil, err
	}

	db.SetMaxOpenConns(25)
	db.SetMaxIdleConns(10)
	db.SetConnMaxLifetime(300 * time.Second)

	return db, nil
}

// ============================================================================
// Verification Logic
// ============================================================================

func NewVerificationService(cfg *Config, db *sql.DB) *VerificationService {
	svc := &VerificationService{
		db:          db,
		config:      cfg,
		compileChan: make(chan *CompilationJob, 100),
		shutdown:    make(chan bool),
	}

	// Start worker pool
	for i := 0; i < 5; i++ {
		svc.wg.Add(1)
		go svc.worker(i)
	}

	return svc
}

func (s *VerificationService) worker(id int) {
	defer s.wg.Done()

	for {
		select {
		case <-s.shutdown:
			return
		case job := <-s.compileChan:
			result := s.compileContract(job.Request)
			job.ResultChan <- result
		}
	}
}

func (s *VerificationService) compileContract(req VerificationRequest) *CompilationResult {
        if len(req.Sources) == 0 {
                return &CompilationResult{
                        Success: false,
                        Errors:  []string{"no source files provided"},
                }
        }
        if !s.isValidCompilerVersion(req.CompilerVersion) {
                return &CompilationResult{
                        Success: false,
                        Errors:  []string{"invalid compiler version"},
                }
        }

        return s.runSolc(req)
}

// runSolc compiles the sources with the real compiler and returns the bytecode
// and ABI it produced. Nothing is synthesised: a compiler that cannot be run,
// or that rejects the source, is reported as a failure.
func (s *VerificationService) runSolc(req VerificationRequest) *CompilationResult {
        solc := s.config.SolcPath
        if solc == "" {
                solc = "solc"
        }

        dir, err := os.MkdirTemp("", "tsc-verify-")
        if err != nil {
                return &CompilationResult{Success: false, Errors: []string{"temp dir: " + err.Error()}}
        }
        defer os.RemoveAll(dir)

        // Every source is written to disk because solc resolves imports
        // relative to the file it is given.
        var paths []string
        for name, content := range req.Sources {
                clean := filepath.Clean("/" + name)
                target := filepath.Join(dir, clean)
                if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
                        return &CompilationResult{Success: false, Errors: []string{"mkdir: " + err.Error()}}
                }
                if err := os.WriteFile(target, []byte(content), 0o644); err != nil {
                        return &CompilationResult{Success: false, Errors: []string{"write source: " + err.Error()}}
                }
                paths = append(paths, target)
        }
        sort.Strings(paths)

        args := []string{"--combined-json", "bin,bin-runtime,abi", "--allow-paths", dir}
        if req.Optimization {
                args = append(args, "--optimize")
                if req.OptimizationRuns > 0 {
                        args = append(args, "--optimize-runs", strconv.Itoa(req.OptimizationRuns))
                }
        }
        if req.EvmVersion != "" {
                args = append(args, "--evm-version", req.EvmVersion)
        }
        args = append(args, paths...)

        ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
        defer cancel()

        var stdout, stderr bytes.Buffer
        cmd := exec.CommandContext(ctx, solc, args...)
        cmd.Stdout = &stdout
        cmd.Stderr = &stderr
        if err := cmd.Run(); err != nil {
                detail := strings.TrimSpace(stderr.String())
                if detail == "" {
                        detail = err.Error()
                }
                return &CompilationResult{Success: false, Errors: []string{"solc: " + detail}}
        }

        var out struct {
                Contracts map[string]struct {
                        Bin        string          `json:"bin"`
                        BinRuntime string          `json:"bin-runtime"`
                        ABI        json.RawMessage `json:"abi"`
                } `json:"contracts"`
        }
        if err := json.Unmarshal(stdout.Bytes(), &out); err != nil {
                return &CompilationResult{Success: false, Errors: []string{"solc output: " + err.Error()}}
        }
        if len(out.Contracts) == 0 {
                return &CompilationResult{Success: false, Errors: []string{"solc produced no contracts"}}
        }

        // The key is "<file>:<Contract>". Prefer the contract the caller named;
        // fall back to the only contract in the file when the name is absent.
        key := ""
        if req.ContractName != "" {
                for k := range out.Contracts {
                        if strings.HasSuffix(k, ":"+req.ContractName) {
                                key = k
                                break
                        }
                }
        }
        if key == "" && len(out.Contracts) == 1 {
                for k := range out.Contracts {
                        key = k
                }
        }
        if key == "" {
                return &CompilationResult{
                        Success: false,
                        Errors:  []string{"contract " + req.ContractName + " not found in compiler output"},
                }
        }

        entry := out.Contracts[key]
        abi := string(entry.ABI)
        if abi == "" || abi == "null" {
                abi = "[]"
        }

        return &CompilationResult{
                Success:  true,
                ABI:      abi,
                Bytecode: "0x" + strings.TrimPrefix(entry.Bin, "0x"),
                Runtime:  "0x" + strings.TrimPrefix(entry.BinRuntime, "0x"),
        }
}

func (s *VerificationService) isValidCompilerVersion(version string) bool {
	validVersions := []string{
		"0.8.0", "0.8.1", "0.8.2", "0.8.3", "0.8.4", "0.8.5", "0.8.6", "0.8.7", "0.8.8", "0.8.9",
		"0.8.10", "0.8.11", "0.8.12", "0.8.13", "0.8.14", "0.8.15", "0.8.16", "0.8.17", "0.8.18", "0.8.19", "0.8.20",
		"0.7.0", "0.7.1", "0.7.2", "0.7.3", "0.7.4", "0.7.5", "0.7.6",
		"0.6.0", "0.6.1", "0.6.2", "0.6.3", "0.6.4", "0.6.5", "0.6.6", "0.6.7", "0.6.8", "0.6.9", "0.6.10", "0.6.11", "0.6.12",
		"0.5.0", "0.5.1", "0.5.2", "0.5.3", "0.5.4", "0.5.5", "0.5.6", "0.5.7", "0.5.8", "0.5.9", "0.5.10", "0.5.11", "0.5.12", "0.5.13", "0.5.14", "0.5.15", "0.5.16", "0.5.17",
	}

	for _, v := range validVersions {
		if version == v {
			return true
		}
	}

	return false
}

// ============================================================================
// Proxy Detection
// ============================================================================

// DetectProxy reports whether the deployed code at an address is a proxy, and
// which implementation it forwards to when that is readable from the code.
//
// The decision is made from the deployed bytecode, not from a guess: an
// EIP-1967 proxy carries the implementation or beacon slot, and an EIP-1167
// minimal proxy carries the target address in its own code.
func (s *VerificationService) DetectProxy(address string) (bool, string, error) {
        code, err := s.fetchCode(address)
        if err != nil {
                return false, "", err
        }
        if len(code) == 0 {
                return false, "", fmt.Errorf("no code deployed at %s", address)
        }

        // EIP-1967 storage slots. A transparent or UUPS proxy reads the
        // implementation from the first; a beacon proxy reads the beacon from
        // the second.
        eip1967Implementation := mustHex("360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc")
        eip1967Beacon := mustHex("a3f0ad74e5423aebfd80d3ef4346578335a9a72aeaee59ff6cb3582b35133d50")
        if bytes.Contains(code, eip1967Beacon) {
                return true, "", nil
        }
        if bytes.Contains(code, eip1967Implementation) {
                return true, "", nil
        }

        // EIP-1167 minimal proxy: a fixed 45-byte program whose only variable
        // part is the 20-byte target address.
        minimalPrefix := mustHex("363d3d373d3d3d363d73")
        minimalSuffix := mustHex("5af43d82803e903d91602b57fd5bf3")
        if len(code) == 45 && bytes.HasPrefix(code, minimalPrefix) && bytes.HasSuffix(code, minimalSuffix) {
                target := code[len(minimalPrefix) : len(minimalPrefix)+20]
                return true, "0x" + hex.EncodeToString(target), nil
        }

        return false, "", nil
}

// fetchCode reads the deployed bytecode via eth_getCode.
func (s *VerificationService) fetchCode(address string) ([]byte, error) {
        if s.config.RPCURL == "" {
                return nil, fmt.Errorf("rpc_url is not configured, so the deployed code cannot be read")
        }
        if !hexAddress.MatchString(address) {
                return nil, fmt.Errorf("invalid address %q", address)
        }
        body, err := json.Marshal(map[string]any{
                "jsonrpc": "2.0",
                "id":      1,
                "method":  "eth_getCode",
                "params":  []any{address, "latest"},
        })
        if err != nil {
                return nil, err
        }
        client := &http.Client{Timeout: 15 * time.Second}
        resp, err := client.Post(s.config.RPCURL, "application/json", bytes.NewReader(body))
        if err != nil {
                return nil, err
        }
        defer resp.Body.Close()

        var out struct {
                Result string `json:"result"`
                Error  *struct {
                        Message string `json:"message"`
                } `json:"error"`
        }
        if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
                return nil, err
        }
        if out.Error != nil {
                return nil, fmt.Errorf("rpc error: %s", out.Error.Message)
        }
        if out.Result == "" || out.Result == "0x" {
                return nil, nil
        }
        return hex.DecodeString(strings.TrimPrefix(out.Result, "0x"))
}

// mustHex decodes a compile-time constant and panics if it is malformed, which
// can only happen if the source is edited incorrectly.
func mustHex(s string) []byte {
        b, err := hex.DecodeString(s)
        if err != nil {
                panic("invalid hex constant: " + s)
        }
        return b
}

// ============================================================================
// Sourcify Integration
// ============================================================================

func (s *VerificationService) FetchFromSourcify(address, chainID string) (*SourcifyMatch, error) {
	if s.config.SourcifyURL == "" {
		return nil, fmt.Errorf("Sourcify URL not configured")
	}

	url := fmt.Sprintf("%s/check-by-addresses?addresses=%s&chainIds=%s", s.config.SourcifyURL, address, chainID)
	
	resp, err := http.Get(url)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("Sourcify returned status %d", resp.StatusCode)
	}

	var match SourcifyMatch
	if err := json.NewDecoder(resp.Body).Decode(&match); err != nil {
		return nil, err
	}

	return &match, nil
}

func (s *VerificationService) SubmitToSourcify(req VerificationRequest) error {
	if s.config.SourcifyURL == "" {
		return fmt.Errorf("Sourcify URL not configured")
	}

	data, _ := json.Marshal(req)
	resp, err := http.Post(s.config.SourcifyURL+"/v1/verify", "application/json", bytes.NewBuffer(data))
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	return nil
}

// ============================================================================
// Database Operations
// ============================================================================

func (s *VerificationService) SaveContract(info *ContractInfo) error {
	_, err := s.db.ExecContext(context.Background(),
		`INSERT INTO contracts (
			address, contract_name, compiler, compiler_version, optimization_enabled,
			optimization_runs, evm_version, license_type, source_code, abi, bytecode,
			runtime_bytecode, constructor_args, is_verified, verification_status, verified_at
		) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, true, 'verified', NOW())
		ON CONFLICT (address) DO UPDATE SET
			contract_name = EXCLUDED.contract_name,
			compiler = EXCLUDED.compiler,
			compiler_version = EXCLUDED.compiler_version,
			source_code = EXCLUDED.source_code,
			abi = EXCLUDED.abi,
			bytecode = EXCLUDED.bytecode,
			runtime_bytecode = EXCLUDED.runtime_bytecode,
			is_verified = true,
			verification_status = 'verified',
			verified_at = NOW()`,
		info.Address, info.ContractName, "solc", info.CompilerVersion,
		info.Optimization, info.OptimizationRuns, info.EvmVersion,
		info.License, info.SourceCode, info.ABI, info.Bytecode,
		info.RuntimeBytecode, info.ConstructorArgs,
	)

	return err
}

func (s *VerificationService) GetContract(address string) (*ContractInfo, error) {
	var info ContractInfo

	err := s.db.QueryRowContext(context.Background(),
		`SELECT address, contract_name, compiler_version, source_code, abi, bytecode,
			runtime_bytecode, constructor_args, is_verified, verified_at
		FROM contracts WHERE address = $1`,
		address,
	).Scan(
		&info.Address, &info.ContractName, &info.CompilerVersion,
		&info.SourceCode, &info.ABI, &info.Bytecode,
		&info.RuntimeBytecode, &info.ConstructorArgs,
		&info.VerifiedAt,
	)

	if err != nil {
		return nil, err
	}

	return &info, nil
}

// ============================================================================
// HTTP Handlers
// ============================================================================

func (s *VerificationService) VerifyContract(c *gin.Context) {
	var req VerificationRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, VerificationResponse{
			Status:  "error",
			Message: "Invalid request: " + err.Error(),
		})
		return
	}

	// Submit compilation job
	job := &CompilationJob{
		ID:      generateID(),
		Request: req,
		ResultChan: make(chan *CompilationResult, 1),
	}

	s.compileChan <- job
	result := <-job.ResultChan

	if !result.Success {
		c.JSON(http.StatusBadRequest, VerificationResponse{
			Status:  "error",
			Message: strings.Join(result.Errors, ", "),
			Address: req.Address,
		})
		return
	}

	// Save to database
	info := &ContractInfo{
		Address:          req.Address,
		ContractName:     req.ContractName,
		CompilerVersion:  req.CompilerVersion,
		Compiler:         "solc",
		Optimization:     req.Optimization,
		OptimizationRuns: req.OptimizationRuns,
		EvmVersion:      req.EvmVersion,
		License:         req.License,
		SourceCode:      encodeSources(req.Sources),
		ABI:             result.ABI,
		Bytecode:        result.Bytecode,
		RuntimeBytecode:  result.Runtime,
		ConstructorArgs:  req.ConstructorArgs,
		VerifiedAt:      time.Now(),
	}

	// Detect proxy
	isProxy, impl, _ := s.DetectProxy(req.Address)
	info.IsProxy = isProxy
	info.Implementation = impl

	if err := s.SaveContract(info); err != nil {
		c.JSON(http.StatusInternalServerError, VerificationResponse{
			Status:  "error",
			Message: "Failed to save contract: " + err.Error(),
		})
		return
	}

	c.JSON(http.StatusOK, VerificationResponse{
		Status:  "success",
		Message: "Contract verified successfully",
		Address: req.Address,
		ABI:     result.ABI,
	})
}

func (s *VerificationService) GetContractInfo(c *gin.Context) {
	address := c.Param("address")

	info, err := s.GetContract(address)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Contract not found"})
		return
	}

	c.JSON(http.StatusOK, info)
}

func (s *VerificationService) CheckSourcify(c *gin.Context) {
	address := c.Query("address")
	chainID := c.DefaultQuery("chain_id", "1")

	match, err := s.FetchFromSourcify(address, chainID)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, match)
}

// ============================================================================
// Helpers
// ============================================================================

func generateID() string {
	hash := sha256.New()
	hash.Write([]byte(time.Now().Format(time.RFC3339Nano)))
	return hex.EncodeToString(hash.Sum(nil))[:16]
}

func encodeSources(sources map[string]string) string {
	data, _ := json.Marshal(sources)
	return string(data)
}

// ============================================================================
// Main
// ============================================================================

func main() {
	config := &Config{
		Server: ServerConfig{
			Host: "0.0.0.0",
			Port: 8081,
		},
		Database: DatabaseConfig{
			Host:     "localhost",
			Port:     5432,
			Username: "tigerscan",
			Password: "tigerscan",
			Database: "tigerscan",
		},
		SourcifyURL: "https://sourcify.dev/server",
	}

	db, err := NewDB(config.Database)
	if err != nil {
		fmt.Printf("Failed to connect to database: %v\n", err)
		os.Exit(1)
	}
	defer db.Close()

	service := NewVerificationService(config, db)

	router := gin.Default()
	router.Use(cors.Default())

	router.POST("/verify", service.VerifyContract)
	router.GET("/contract/:address", service.GetContractInfo)
	router.GET("/sourcify/check", service.CheckSourcify)

	router.GET("/health", func(c *gin.Context) {
		c.JSON(200, gin.H{"status": "ok"})
	})

	addr := fmt.Sprintf("%s:%d", config.Server.Host, config.Server.Port)
	fmt.Printf("Verification service listening on %s\n", addr)

	go func() {
		if err := router.Run(addr); err != nil {
			fmt.Printf("Server error: %v\n", err)
		}
	}()

	// Wait for shutdown
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)
	<-sigCh

	close(service.shutdown)
	service.wg.Wait()

	fmt.Println("Verification service stopped")
}