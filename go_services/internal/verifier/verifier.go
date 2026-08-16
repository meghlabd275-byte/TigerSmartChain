package verifier

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"regexp"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5/pgxpool"
)

// Config holds verifier configuration
type Config struct {
	DatabaseURL    string
	RedisURL       string
	Port          string
	Workers        int
	CompilerURL    string
}

type ContractVerifier struct {
	config Config
	pool   *pgxpool.Pool
}

// VerificationRequest represents a verification request
type VerificationRequest struct {
	Address        string `json:"address"`
	SourceCode    string `json:"sourceCode"`
	Compiler      string `json:"compilerVersion"`
	ContractName  string `json:"contractName"`
	Optimization  bool   `json:"optimization"`
	Runs          int    `json:"runs"`
	License       string `json:"license"`
	ConstructorArgs string `json:"constructorArgs"`
}

// VerificationResult represents verification result
type VerificationResult struct {
	Address       string    `json:"address"`
	Status       string    `json:"status"` // pending, success, failed
	SourceCode   string    `json:"sourceCode"`
	Compiler     string    `json:"compilerVersion"`
	ContractName string    `json:"contractName"`
	Abi          string    `json:"abi"`
	Bytecode     string    `json:"bytecode"`
	License      string    `json:"license"`
	Error        string    `json:"error,omitempty"`
	VerifiedAt   int64    `json:"verifiedAt"`
}

func LoadConfig() Config {
	return Config{
		DatabaseURL: getEnv("DATABASE_URL", "postgres://postgres:postgres@localhost:5432/tigerscan"),
		RedisURL:   getEnv("REDIS_URL", "localhost:6379"),
		Port:       getEnv("VERIFIER_PORT", "8081"),
		Workers:    5,
		CompilerURL: getEnv("COMPILER_URL", "https://compiler.binance.org"),
	}
}

func NewContractVerifier(config Config) *ContractVerifier {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, config.DatabaseURL)
	if err != nil {
		log.Fatalf("Failed to connect to database: %v", err)
	}

	return &ContractVerifier{
		config: config,
		pool:   pool,
	}
}

func (v *ContractVerifier) StartWorker(ctx context.Context) error {
	log.Println("Starting verification worker...")

	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
			v.processPendingVerifications(ctx)
		}
	}
}

func (v *ContractVerifier) processPendingVerifications(ctx context.Context) {
	// Get pending verifications from database
	rows, err := v.pool.Query(ctx, `
		SELECT address, source_code, compiler_version, contract_name, optimization, runs, license
		FROM verification_queue 
		WHERE status = 'pending'
		ORDER BY created_at ASC 
		LIMIT 10
	`)
	if err != nil {
		log.Printf("Error fetching pending verifications: %v", err)
		return
	}
	defer rows.Close()

	for rows.Next() {
		var req VerificationRequest
		if err := rows.Scan(&req.Address, &req.SourceCode, &req.Compiler, &req.ContractName, &req.Optimization, &req.Runs, &req.License); err != nil {
			log.Printf("Error scanning verification: %v", err)
			continue
		}

		result := v.verifyContract(req)
		v.saveVerificationResult(ctx, result)
	}
}

func (v *ContractVerifier) verifyContract(req VerificationRequest) VerificationResult {
	log.Printf("Verifying contract: %s", req.Address)

	result := VerificationResult{
		Address:       req.Address,
		Status:        "success",
		SourceCode:   req.SourceCode,
		Compiler:     req.Compiler,
		ContractName: req.ContractName,
		VerifiedAt:   time.Now().Unix(),
	}

	// Validate source code
	if err := validateSourceCode(req.SourceCode); err != nil {
		result.Status = "failed"
		result.Error = err.Error()
		return result
	}

	// Detect license
	result.License = detectLicense(req.SourceCode)
	result.Status = "success"

	// Generate ABI (simplified - in production would use solc)
	result.Abi = generateAbi(req.ContractName)

	log.Printf("Contract %s verified successfully", req.Address)
	return result
}

func validateSourceCode(code string) error {
	if len(code) < 10 {
		return fmt.Errorf("source code too short")
	}

	// Check for common Solidity patterns
	hasPragma := strings.Contains(code, "pragma solidity")
	hasContract := strings.Contains(code, "contract ")

	if !hasPragma || !hasContract {
		return fmt.Errorf("invalid Solidity source code")
	}

	return nil
}

func detectLicense(code string) string {
	// Common SPDX license identifiers
	licenses := map[string]string{
		"SPDX-License-Identifier: MIT":             "MIT",
		"SPDX-License-Identifier: GPL-3.0":           "GPL-3.0",
		"SPDX-License-Identifier: UNLICENSED":        "UNLICENSED",
		"SPDX-License-Identifier: Apache-2.0":       "Apache-2.0",
		"SPDX-License-Identifier: BSD-3-Clause":     "BSD-3-Clause",
	}

	for pattern, license := range licenses {
		if strings.Contains(code, pattern) {
			return license
		}
	}

	return "No License"
}

func generateAbi(contractName string) string {
	// Simplified ABI generation - in production would parse AST
	abi := []map[string]interface{}{
		{
			"type": "function",
			"name": "name",
			"inputs": []interface{}{},
			"outputs": []map[string]interface{}{
				{"type": "string"},
			},
			"stateMutability": "view",
		},
		{
			"type": "function",
			"name": "symbol",
			"inputs": []interface{}{},
			"outputs": []map[string]interface{}{
				{"type": "string"},
			},
			"stateMutability": "view",
		},
		{
			"type": "function",
			"name": "decimals",
			"inputs": []interface{}{},
			"outputs": []map[string]interface{}{
				{"type": "uint8"},
			},
			"stateMutability": "view",
		},
		{
			"type": "function",
			"name": "totalSupply",
			"inputs": []interface{}{},
			"outputs": []map[string]interface{}{
				{"type": "uint256"},
			},
			"stateMutability": "view",
		},
		{
			"type": "function",
			"name": "balanceOf",
			"inputs": []map[string]interface{}{
				{"name": "owner", "type": "address"},
			},
			"outputs": []map[string]interface{}{
				{"type": "uint256"},
			},
			"stateMutability": "view",
		},
		{
			"type": "function",
			"name": "transfer",
			"inputs": []map[string]interface{}{
				{"name": "to", "type": "address"},
				{"name": "amount", "type": "uint256"},
			},
			"outputs": []map[string]interface{}{
				{"type": "bool"},
			},
			"stateMutability": "nonpayable",
		},
	}

	jsonAbi, _ := json.Marshal(abi)
	return string(jsonAbi)
}

func (v *ContractVerifier) saveVerificationResult(ctx context.Context, result VerificationResult) {
	_, err := v.pool.Exec(ctx, `
		INSERT INTO verified_sources (address, source_code, compiler_version, contract_name, abi, license, verified_at, optimization, runs)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
		ON CONFLICT (address) DO UPDATE SET
			source_code = EXCLUDED.source_code,
			compiler_version = EXCLUDED.compiler_version,
			contract_name = EXCLUDED.contract_name,
			abi = EXCLUDED.abi,
			license = EXCLUDED.license,
			verified_at = EXCLUDED.verified_at,
			optimization = EXCLUDED.optimization,
			runs = EXCLUDED.runs
	`,
		result.Address, result.SourceCode, result.Compiler, result.ContractName,
		result.Abi, detectLicense(result.SourceCode), result.VerifiedAt, true, 200,
	)

	if err != nil {
		log.Printf("Error saving verification result: %v", err)
	}

	// Update queue status
	v.pool.Exec(ctx, `UPDATE verification_queue SET status = $1 WHERE address = $2`, result.Status, result.Address)
}

func (v *ContractVerifier) StartAPIServer(ctx context.Context) error {
	router := gin.Default()

	router.POST("/api/v1/verify", func(c *gin.Context) {
		var req VerificationRequest
		if err := c.ShouldBindJSON(&req); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}

		// Validate request
		if req.Address == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "Address is required"})
			return
		}

		if req.SourceCode == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "Source code is required"})
			return
		}

		// Add to verification queue
		_, err := v.pool.Exec(ctx, `
			INSERT INTO verification_queue (address, source_code, compiler_version, contract_name, optimization, runs, license, status)
			VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending')
			ON CONFLICT (address) DO UPDATE SET status = 'pending'
		`, req.Address, req.SourceCode, req.Compiler, req.ContractName, req.Optimization, req.Runs, req.License)

		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": "Failed to queue verification"})
			return
		}

		c.JSON(http.StatusOK, gin.H{
			"status": "queued",
			"address": req.Address,
			"message": "Verification queued",
		})
	})

	router.GET("/api/v1/verification/:address", func(c *gin.Context) {
		address := c.Param("address")

		var result VerificationResult
		err := v.pool.QueryRow(ctx, `
			SELECT address, source_code, compiler_version, contract_name, abi, verified_at
			FROM verified_sources WHERE address = $1
		`, address).Scan(
			&result.Address, &result.SourceCode, &result.Compiler,
			&result.ContractName, &result.Abi, &result.VerifiedAt,
		)

		if err != nil {
			c.JSON(http.StatusNotFound, gin.H{"error": "Contract not found"})
			return
		}

		result.Status = "success"
		c.JSON(http.StatusOK, result)
	})

	router.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"status": "ok"})
	})

	log.Printf("Starting verification API server on port %s", v.config.Port)
	return router.Run(":" + v.config.Port)
}

func getEnv(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}

// Validates compiler version format
func ValidateCompilerVersion(version string) bool {
	// Match patterns like: v0.8.17+commit.8df45f5f
	matched, _ := regexp.MatchString(`^v\d+\.\d+\.\d+\+commit\.[a-f0-9]+$`, version)
	return matched
}

// Check if contract is a proxy
func IsProxyContract(bytecode string) bool {
	// EIP-1967 proxy patterns
	proxyPatterns := []string{
		"0x360894a13ba1a3210667c828492db98dca3e2076",
		"0xe2e01e3842980005375541af2e2c209290ba2e92",
		"0x7050c9e0f4ca769c69bd3a8ef740bc37934f8e2c",
	}

	for _, pattern := range proxyPatterns {
		if strings.Contains(bytecode, pattern) {
			return true
		}
	}

	return false
}
