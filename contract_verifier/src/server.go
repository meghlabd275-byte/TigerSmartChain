// Package contractverifier provides smart contract verification service
// Built with Go for high performance
package contractverifier

import (
	"bytes"
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds contract verifier configuration
type Config struct {
	DBURL        string
	RedisURL     string
	SolidityPath string
	TempDir      string
	Timeout      time.Duration
}

// VerificationRequest represents a verification request
type VerificationRequest struct {
	Address         string `json:"address"`
	SourceCode      string `json:"sourceCode"`
	CompilerVersion string `json:"compilerVersion"`
	ContractName    string `json:"contractName"`
	Optimizer       bool   `json:"optimizer"`
	OptimizerRuns   int    `json:"optimizerRuns"`
	ABI             string `json:"abi"`
	Bytecode        string `json:"bytecode"`
	Arguments       string `json:"constructorArguments"`
}

// VerificationResult represents verification result
type VerificationResult struct {
	Address         string    `json:"address"`
	Success         bool      `json:"success"`
	Message         string    `json:"message"`
	SourceCode      string    `json:"sourceCode,omitempty"`
	ABI             string    `json:"abi,omitempty"`
	Bytecode        string    `json:"bytecode,omitempty"`
	CompilerVersion string    `json:"compilerVersion,omitempty"`
	VerifiedAt      time.Time `json:"verifiedAt"`
}

// ContractInfo represents verified contract info
type ContractInfo struct {
	Address         string    `json:"address"`
	Name            string    `json:"name"`
	CompilerVersion string    `json:"compilerVersion"`
	Optimization    bool      `json:"optimization"`
	OptimizerRuns   int       `json:"optimizerRuns"`
	SourceCode      string    `json:"sourceCode"`
	ABI             string    `json:"abi"`
	Bytecode        string    `json:"bytecode"`
	ConstructorArgs string    `json:"constructorArgs,omitempty"`
	IsVerified      bool      `json:"isVerified"`
	VerifiedAt      time.Time `json:"verifiedAt"`
}

// SupportedCompiler represents a supported compiler version
type SupportedCompiler struct {
	Version   string `json:"version"`
	BuildDate string `json:"buildDate"`
}

// Server represents the contract verifier server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
}

// NewServer creates a new contract verifier server
func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 11})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, fmt.Errorf("failed to connect to redis: %w", err)
	}
	if err := createTables(ctx, pool); err != nil {
		return nil, fmt.Errorf("failed to create tables: %w", err)
	}

	// Create temp directory
	if err := os.MkdirAll(cfg.TempDir, 0755); err != nil {
		return nil, fmt.Errorf("failed to create temp dir: %w", err)
	}

	srv := &Server{cfg: cfg, pool: pool, redis: rdb}
	return srv, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) error {
	queries := []string{
		`CREATE TABLE IF NOT EXISTS verified_contracts (id SERIAL PRIMARY KEY, address VARCHAR(42) NOT NULL UNIQUE, name VARCHAR(255) NOT NULL, compiler_version VARCHAR(50) NOT NULL, optimization BOOLEAN DEFAULT TRUE, optimizer_runs INTEGER DEFAULT 200, source_code LONGTEXT NOT NULL, abi JSONB, bytecode LONGTEXT, constructor_args TEXT, verified_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`,
		`CREATE TABLE IF NOT EXISTS verification_requests (id SERIAL PRIMARY KEY, address VARCHAR(42) NOT NULL, source_code LONGTEXT, compiler_version VARCHAR(50), status VARCHAR(20) DEFAULT 'pending', message TEXT, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`,
		`CREATE INDEX IF NOT EXISTS idx_verified_address ON verified_contracts(address)`,
		`CREATE INDEX IF NOT EXISTS idx_verification_status ON verification_requests(status)`,
	}
	for _, q := range queries {
		if _, err := pool.Exec(ctx, q); err != nil {
			return err
		}
	}
	return nil
}

// VerifyContract verifies a smart contract
func (s *Server) VerifyContract(ctx context.Context, req *VerificationRequest) (*VerificationResult, error) {
	// Create temp directory for compilation
	tempDir := fmt.Sprintf("%s/%d", s.cfg.TempDir, time.Now().Unix())
	if err := os.MkdirAll(tempDir, 0755); err != nil {
		return &VerificationResult{Address: req.Address, Success: false, Message: "Failed to create temp directory"}, err
	}
	defer os.RemoveAll(tempDir)

	// Write source code to file
	contractFile := fmt.Sprintf("%s/%s.sol", tempDir, req.ContractName)
	if err := os.WriteFile(contractFile, []byte(req.SourceCode), 0644); err != nil {
		return &VerificationResult{Address: req.Address, Success: false, Message: "Failed to write source code"}, err
	}

	// Compile contract
	compiled, err := s.compileContract(tempDir, req.ContractName, req.CompilerVersion, req.Optimizer, req.OptimizerRuns)
	if err != nil {
		return &VerificationResult{Address: req.Address, Success: false, Message: err.Error()}, err
	}

	// Compare bytecode
	if !s.compareBytecode(req.Bytecode, compiled.bytecode) {
		return &VerificationResult{Address: req.Address, Success: false, Message: "Bytecode mismatch - the compiled bytecode does not match the on-chain bytecode"}, nil
	}

	// Store verified contract
	_, err = s.pool.Exec(ctx, `
		INSERT INTO verified_contracts (address, name, compiler_version, optimization, optimizer_runs, source_code, abi, bytecode, constructor_args)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
		ON CONFLICT (address) DO UPDATE SET name = $2, compiler_version = $3, optimization = $4, optimizer_runs = $5, source_code = $6, abi = $7, bytecode = $8, constructor_args = $9`,
		req.Address, req.ContractName, req.CompilerVersion, req.Optimizer, req.OptimizerRuns, req.SourceCode, req.ABI, compiled.bytecode, req.Arguments)
	if err != nil {
		return &VerificationResult{Address: req.Address, Success: false, Message: "Failed to save verification"}, err
	}

	// Cache in Redis
	data, _ := json.Marshal(ContractInfo{
		Address: req.Address, Name: req.ContractName, CompilerVersion: req.CompilerVersion,
		Optimization: req.Optimizer, OptimizerRuns: req.OptimizerRuns,
		SourceCode: req.SourceCode, ABI: req.ABI, Bytecode: compiled.bytecode,
		IsVerified: true, VerifiedAt: time.Now(),
	})
	s.redis.Set(ctx, fmt.Sprintf("contract:verified:%s", req.Address), string(data), 0)

	return &VerificationResult{
		Address:         req.Address,
		Success:         true,
		Message:         "Contract verified successfully",
		SourceCode:      req.SourceCode,
		ABI:             req.ABI,
		CompilerVersion: req.CompilerVersion,
		VerifiedAt:      time.Now(),
	}, nil
}

type compileResult struct {
	bytecode string
	abi      string
}

func (s *Server) compileContract(tempDir, contractName, compilerVersion string, optimizer bool, optimizerRuns int) (*compileResult, error) {
	// solc resolves sources relative to its working directory, so the source is
	// named by an absolute path and solc runs inside the temp directory.
	srcPath := filepath.Join(tempDir, contractName+".sol")

	args := []string{"--combined-json", "abi,bin,bin-runtime"}
	if optimizer {
		args = append(args, "--optimize")
		if optimizerRuns > 0 {
			args = append(args, "--optimize-runs", strconv.Itoa(optimizerRuns))
		}
	}
	args = append(args, srcPath)

	cmd := exec.Command(s.solcPath(compilerVersion), args...)
	cmd.Dir = tempDir
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	if err := cmd.Run(); err != nil {
		// A compiler that is missing or that rejects the source must surface as
		// a failure. Returning bytecode here would mark an unverifiable
		// contract as verified, which is the one outcome this service exists to
		// prevent.
		detail := strings.TrimSpace(stderr.String())
		if detail == "" {
			detail = err.Error()
		}
		return nil, fmt.Errorf("solc failed: %s", detail)
	}

	// solc --combined-json emits:
	//   {"contracts":{"<file>:<Name>":{"abi":...,"bin":"..","bin-runtime":".."}}}
	type solcContract struct {
		ABI        json.RawMessage `json:"abi"`
		Bin        string          `json:"bin"`
		BinRuntime string          `json:"bin-runtime"`
	}
	var out struct {
		Contracts map[string]solcContract `json:"contracts"`
	}
	if err := json.Unmarshal(stdout.Bytes(), &out); err != nil {
		return nil, fmt.Errorf("solc produced unreadable JSON: %w", err)
	}
	if len(out.Contracts) == 0 {
		return nil, fmt.Errorf("solc produced no contracts; check the source and compiler version")
	}

	// Prefer the contract the caller named; the key is "<file>:<Name>".
	var chosen solcContract
	found := false
	for key, entry := range out.Contracts {
		if strings.HasSuffix(key, ":"+contractName) {
			chosen, found = entry, true
			break
		}
	}
	if !found {
		// Falling back is only safe when the source defines exactly one
		// contract. Picking arbitrarily among several would compare the wrong
		// bytecode and report a mismatch that is not real.
		if len(out.Contracts) != 1 {
			return nil, fmt.Errorf("contract %q not found in solc output", contractName)
		}
		for _, entry := range out.Contracts {
			chosen = entry
		}
	}

	abi := string(chosen.ABI)
	if abi == "" || abi == "null" {
		abi = "[]"
	}

	// The creation bytecode is what an address was deployed with, so that is
	// what a verifier compares against.
	if chosen.Bin == "" {
		return nil, fmt.Errorf("solc returned no creation bytecode for %q", contractName)
	}
	return &compileResult{
		bytecode: "0x" + strings.TrimPrefix(chosen.Bin, "0x"),
		abi:      abi,
	}, nil
}

// solcPath resolves the compiler binary for a requested version.
//
// A deployment can pin one binary per version through SOLC_PATH_<version with
// dots replaced by underscores>, or one binary for everything through
// SOLC_PATH. The bare name "solc" is the last resort so a machine with a single
// compiler installed still works.
func (s *Server) solcPath(version string) string {
	if version != "" {
		if path := os.Getenv("SOLC_PATH_" + strings.ReplaceAll(version, ".", "_")); path != "" {
			return path
		}
	}
	if path := os.Getenv("SOLC_PATH"); path != "" {
		return path
	}
	if s.cfg.SolidityPath != "" {
		return s.cfg.SolidityPath
	}
	return "solc"
}

func (s *Server) readSource(tempDir, contractName string) string {
	data, _ := os.ReadFile(fmt.Sprintf("%s/%s.sol", tempDir, contractName))
	return string(data)
}

func (s *Server) compareBytecode(onchain, compiled string) bool {
	// Normalize bytecodes
	onchain = strings.ToLower(strings.TrimPrefix(onchain, "0x"))
	compiled = strings.ToLower(strings.TrimPrefix(compiled, "0x"))

	// Remove library links
	onchain = regexp.MustCompile(`__\w{38}`).ReplaceAllString(onchain, "0000000000000000000000000000000000000000")
	compiled = regexp.MustCompile(`__\w{38}`).ReplaceAllString(compiled, "0000000000000000000000000000000000000000")

	return onchain == compiled
}

// GetVerifiedContract returns verified contract info
func (s *Server) GetVerifiedContract(ctx context.Context, address string) (*ContractInfo, error) {
	// Try cache first
	if data, err := s.redis.Get(ctx, fmt.Sprintf("contract:verified:%s", address)).Result(); err == nil {
		var info ContractInfo
		json.Unmarshal([]byte(data), &info)
		return &info, nil
	}

	var info ContractInfo
	err := s.pool.QueryRow(ctx, `
		SELECT address, name, compiler_version, optimization, optimizer_runs, source_code, abi, bytecode, constructor_args, verified_at
		FROM verified_contracts WHERE address = $1`,
		address,
	).Scan(&info.Address, &info.Name, &info.CompilerVersion, &info.Optimization, &info.OptimizerRuns,
		&info.SourceCode, &info.ABI, &info.Bytecode, &info.ConstructorArgs, &info.VerifiedAt)
	if err != nil {
		return nil, err
	}

	info.IsVerified = true

	// Cache result
	data, _ := json.Marshal(info)
	s.redis.Set(ctx, fmt.Sprintf("contract:verified:%s", address), string(data), 0)

	return &info, nil
}

// GetVerifiedContracts returns all verified contracts
func (s *Server) GetVerifiedContracts(ctx context.Context, limit, offset int) ([]ContractInfo, error) {
	rows, err := s.pool.Query(ctx, `
		SELECT address, name, compiler_version, optimization, optimizer_runs, source_code, abi, bytecode, constructor_args, verified_at
		FROM verified_contracts
		ORDER BY verified_at DESC
		LIMIT $1 OFFSET $2`,
		limit, offset)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var contracts []ContractInfo
	for rows.Next() {
		var c ContractInfo
		if err := rows.Scan(&c.Address, &c.Name, &c.CompilerVersion, &c.Optimization, &c.OptimizerRuns,
			&c.SourceCode, &c.ABI, &c.Bytecode, &c.ConstructorArgs, &c.VerifiedAt); err != nil {
			continue
		}
		c.IsVerified = true
		contracts = append(contracts, c)
	}

	return contracts, nil
}

// GetSupportedCompilers reports the compilers this server can actually run.
//
// The list is discovered by asking each candidate binary for its version, not
// asserted. A version this server cannot execute is not a version it supports:
// offering it would let a user pick a compiler whose use then fails.
// Candidates are a bare "solc" plus anything named in SOLC_VERSIONS.
func (s *Server) GetSupportedCompilers(ctx context.Context) ([]SupportedCompiler, error) {
	candidates := []string{"solc"}
	if extra := os.Getenv("SOLC_VERSIONS"); extra != "" {
		for _, v := range strings.Split(extra, ",") {
			if v = strings.TrimSpace(v); v != "" {
				candidates = append(candidates, v)
			}
		}
	}

	seen := make(map[string]bool)
	var supported []SupportedCompiler
	for _, candidate := range candidates {
		if seen[candidate] {
			continue
		}
		seen[candidate] = true
		version, err := s.solcVersion(candidate)
		if err != nil {
			continue
		}
		supported = append(supported, SupportedCompiler{
			Version:   version,
			BuildDate: s.solcBuildDate(candidate),
		})
	}

	if len(supported) == 0 {
		return nil, fmt.Errorf("no solc binary available; set SOLC_PATH or install solc")
	}
	return supported, nil
}

// solcVersion asks a compiler for its version, for example "0.8.20".
func (s *Server) solcVersion(binary string) (string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	out, err := exec.CommandContext(ctx, s.solcPath(binary), "--version").Output()
	if err != nil {
		return "", err
	}
	// The output carries a line like "Version: 0.8.20+commit.a1b79de6".
	for _, line := range strings.Split(string(out), "\n") {
		line = strings.TrimSpace(line)
		if !strings.HasPrefix(line, "Version:") {
			continue
		}
		fields := strings.Fields(strings.TrimPrefix(line, "Version:"))
		if len(fields) > 0 {
			return strings.TrimSpace(strings.SplitN(fields[0], "+", 2)[0]), nil
		}
	}
	return "", fmt.Errorf("could not read a version from %s", binary)
}

// solcBuildDate reads the build identifier solc reports, or an empty string.
func (s *Server) solcBuildDate(binary string) string {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	out, err := exec.CommandContext(ctx, s.solcPath(binary), "--version").Output()
	if err != nil {
		return ""
	}
	for _, line := range strings.Split(string(out), "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "Version:") {
			if i := strings.Index(line, "+"); i >= 0 {
				return strings.TrimSpace(line[i+1:])
			}
		}
	}
	return ""
}

// DecodeConstructorArgs splits constructor arguments out of a creation input.
//
// A creation input is the creation bytecode followed by the ABI-encoded
// constructor arguments. The bytecode length is known, so the arguments are the
// remainder. The ABI names each value; a value the ABI cannot describe is
// returned as hex rather than dropped.
func (s *Server) DecodeConstructorArgs(abi, bytecode, args string) (string, error) {
	input := strings.TrimPrefix(strings.TrimSpace(args), "0x")
	code := strings.TrimPrefix(strings.TrimSpace(bytecode), "0x")
	if input == "" {
		return "{}", nil
	}

	// An input that does not begin with the creation bytecode cannot be split
	// this way. Returning it whole is honest; returning "{}" would look like a
	// contract with no arguments.
	if code != "" && !strings.HasPrefix(strings.ToLower(input), strings.ToLower(code)) {
		return args, nil
	}
	argHex := input[len(code):]
	if len(argHex)%2 != 0 {
		return "", fmt.Errorf("constructor arguments are not whole bytes")
	}
	argBytes, err := hex.DecodeString(argHex)
	if err != nil {
		return "", fmt.Errorf("constructor arguments are not valid hex: %w", err)
	}

	// Find the constructor's inputs in the ABI.
	var entries []struct {
		Type   string `json:"type"`
		Inputs []struct {
			Name string `json:"name"`
			Type string `json:"type"`
		} `json:"inputs"`
	}
	if err := json.Unmarshal([]byte(abi), &entries); err != nil {
		// An unreadable ABI is not fatal: the raw arguments are still useful.
		return "0x" + hex.EncodeToString(argBytes), nil
	}

	var params []struct{ Name, Type string }
	for _, e := range entries {
		if e.Type == "constructor" {
			for _, in := range e.Inputs {
				params = append(params, struct{ Name, Type string }{in.Name, in.Type})
			}
			break
		}
	}
	if len(params) == 0 {
		return "0x" + hex.EncodeToString(argBytes), nil
	}

	decoded := make(map[string]string, len(params))
	offset := 0
	for i, p := range params {
		name := p.Name
		if name == "" {
			name = fmt.Sprintf("arg%d", i)
		}
		value, size, err := decodeABIWord(p.Type, argBytes[offset:])
		if err != nil {
			// Stop at the first value that cannot be read, keeping the rest.
			decoded[name] = fmt.Sprintf("undecodable(%s)", p.Type)
			break
		}
		decoded[name] = value
		offset += size
	}
	out, err := json.Marshal(decoded)
	if err != nil {
		return "", err
	}
	return string(out), nil
}

// decodeABIWord decodes one static ABI value and reports the bytes it consumed.
//
// A dynamic type (string, bytes, T[]) is reported as its offset word, because
// resolving it needs the whole argument block rather than a single word.
func decodeABIWord(typ string, data []byte) (string, int, error) {
	if len(data) < 32 {
		return "", 0, fmt.Errorf("need 32 bytes for %s, have %d", typ, len(data))
	}
	word := data[:32]
	switch {
	case typ == "address":
		return "0x" + hex.EncodeToString(word[12:]), 32, nil
	case typ == "bool":
		for _, b := range word[:31] {
			if b != 0 {
				return "", 0, fmt.Errorf("bool is not zero padded")
			}
		}
		if word[31] > 1 {
			return "", 0, fmt.Errorf("bool is neither 0 nor 1")
		}
		if word[31] == 1 {
			return "true", 32, nil
		}
		return "false", 32, nil
	case typ == "string" || typ == "bytes" || strings.HasSuffix(typ, "[]"):
		return "0x" + hex.EncodeToString(word), 32, nil
	case strings.HasPrefix(typ, "uint") || strings.HasPrefix(typ, "int"):
		return new(big.Int).SetBytes(word).String(), 32, nil
	default:
		// bytesN and anything unrecognised are returned as the raw word.
		return "0x" + hex.EncodeToString(word), 32, nil
	}
}

// SearchByABI searches contracts by ABI
func (s *Server) SearchByABI(ctx context.Context, abiPattern string) ([]ContractInfo, error) {
	rows, err := s.pool.Query(ctx, `
		SELECT address, name, compiler_version, optimization, optimizer_runs, source_code, abi, bytecode, constructor_args, verified_at
		FROM verified_contracts
		WHERE abi ILIKE $1`,
		"%"+abiPattern+"%")
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var contracts []ContractInfo
	for rows.Next() {
		var c ContractInfo
		if err := rows.Scan(&c.Address, &c.Name, &c.CompilerVersion, &c.Optimization, &c.OptimizerRuns,
			&c.SourceCode, &c.ABI, &c.Bytecode, &c.ConstructorArgs, &c.VerifiedAt); err != nil {
			continue
		}
		c.IsVerified = true
		contracts = append(contracts, c)
	}

	return contracts, nil
}

// FormatBytecode formats bytecode for display
func FormatBytecode(bytecode string) string {
	if len(bytecode) > 32 {
		return bytecode[:32] + "..."
	}
	return bytecode
}

// ParseSourceCode extracts contract name from source
func ParseSourceCode(sourceCode string) (string, error) {
	re := regexp.MustCompile(`pragma\s+solidity\s+[\^>=<]+;\s*\n\s*contract\s+(\w+)`)
	matches := re.FindStringSubmatch(sourceCode)
	if len(matches) > 1 {
		return matches[1], nil
	}
	return "", fmt.Errorf("could not find contract name")
}
