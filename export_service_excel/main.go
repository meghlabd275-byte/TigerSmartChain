package main

import (
	"bytes"
	"context"
	"database/sql"
	"encoding/csv"
	"encoding/json"
	"fmt"
	"io"
	"math"
	"math/big"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/google/uuid"
	"github.com/lib/pq"
	"golang.org/x/crypto/bcrypt"
	"golang.org/x/time/rate"
)

// ============================================
// Excel Export Service
// ============================================
// Advanced export functionality with Excel, CSV, JSON, and bulk download
// Supports: Transactions, Blocks, Addresses, Tokens, Contracts, Events

// ============================================
// Types
// ============================================

type ExportedData struct {
	ID        string          `json:"id"`
	Type     string          `json:"type"`
	Data     []byte          `json:"data"`
	Format   string         `json:"format"`
	Checksum string         `json:"checksum"`
	Size     int64          `json:"size"`
	Rows     int            `json:"rows"`
	Columns  int            `json:"columns"`
	Created  time.Time      `json:"created"`
}

type ExportJob struct {
	ID          string         `json:"id"`
	Type         string         `json:"type"`
	Status      string         `json:"status"`
	Format      string         `json:"format"`
	Filters     ExportFilters `json:"filters"`
	Progress   float64       `json:"progress"`
	Result     *ExportedData `json:"result,omitempty"`
	Error      string        `json:"error,omitempty"`
	CreatedAt  time.Time    `json:"created_at"`
	StartedAt  *time.Time   `json:"started_at,omitempty"`
	CompletedAt *time.Time  `json:"completed_at,omitempty"`
}

type ExportFilters struct {
	Address         string     `json:"address,omitempty"`
	FromBlock      uint64     `json:"from_block,omitempty"`
	ToBlock        uint64     `json:"to_block,omitempty"`
	FromDate       time.Time `json:"from_date,omitempty"`
	ToDate         time.Time `json:"to_date,omitempty"`
	TokenType      string     `json:"token_type,omitempty"`
	ContractType  string     `json:"contract_type,omitempty"`
	MinValue      *big.Int  `json:"min_value,omitempty"`
	MaxValue      *big.Int  `json:"max_value,omitempty"`
	MethodID      string     `json:"method_id,omitempty"`
	Status        string     `json:"status,omitempty"`
	IncludeFields []string  `json:"include_fields,omitempty"`
	ExcludeFields []string  `json:"exclude_fields,omitempty"`
}

type ExportRequest struct {
	Type     string         `json:"type"`
	Format   string         `json:"format"`
	Filters ExportFilters `json:"filters"`
	Limit   int           `json:"limit"`
	Offset  int           `json:"offset"`
}

// ============================================
// Excel Generator
// ============================================

type ExcelGenerator struct {
	sheets map[string]*ExcelSheet
	mu     sync.Mutex
}

type ExcelSheet struct {
	Name    string
	Rows   [][]interface{}
	Widths []int
}

func NewExcelGenerator() *ExcelGenerator {
	return &ExcelGenerator{
		sheets: make(map[string]*ExcelSheet),
	}
}

func (e *ExcelGenerator) AddSheet(name string, headers []string, data [][]interface{}) {
	widths := make([]int, len(headers))
	for _, row := range data {
		for i, cell := range row {
			if i < len(widths) {
				widths[i] = max(widths[i], len(fmt.Sprintf("%v", cell)))
			}
		}
	}
	
	e.sheets[name] = &ExcelSheet{
		Name:    name,
		Rows:   data,
		Widths: widths,
	}
}

// Generate Excel file in XLSX format (binary)
func (e *ExcelGenerator) Generate() ([]byte, error) {
	var buf bytes.Buffer
	
	// XLSX is a ZIP archive with XML files inside
	// For simplicity, we'll create a basic XLSX structure
	
	// XML declaration
	buf.WriteString(`<?xml version="1.0" encoding="UTF-8" standalone="yes"?>`)
	
	// Workbook
	buf.WriteString(`<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">`)
	
	for _, sheet := range e.sheets {
		buf.WriteString(fmt.Sprintf(`<sheet name="%s" sheetId="%d"/>`, sheet.Name, len(e.sheets)))
	}
	
	buf.WriteString(`</workbook>`)
	
	// Return as binary (simple format for demonstration)
	// In production, use proper xlsx library like "github.com/tealchen/xlsx"
	return buf.Bytes(), nil
}

// Generate CSV
func (e *ExcelGenerator) GenerateCSV() ([]byte, error) {
	var buf bytes.Buffer
	writer := csv.NewWriter(&buf)
	
	for _, sheet := range e.sheets {
		// Write sheet name as comment
		writer.Write([]string{fmt.Sprintf("# Sheet: %s", sheet.Name)})
		
		// Write data
		for _, row := range sheet.Rows {
			rowStr := make([]string, len(row))
			for i, cell := range row {
				rowStr[i] = fmt.Sprintf("%v", cell)
			}
			writer.Write(rowStr)
		}
		writer.Write([]string{}) // Empty row between sheets
	}
	
	writer.Flush()
	return buf.Bytes(), writer.Error()
}

// Generate JSON
func (e *ExcelGenerator) GenerateJSON() ([]byte, error) {
	data := make(map[string]interface{})
	
	for name, sheet := range e.sheets {
		data[name] = sheet.Rows
	}
	
	return json.Marshal(data)
}

// ============================================
// Export Service
// ============================================

type ExportService struct {
	db        *sql.DB
	jobs      map[string]*ExportJob
	jobsMu    sync.RWMutex
	limiter   *rate.Limiter
	cache     map[string]*ExportedData
	cacheMu   sync.RWMutex
	maxCache  int
}

func NewExportService(db *sql.DB) *ExportService {
	return &ExportService{
		db:       db,
		jobs:     make(map[string]*ExportJob),
		limiter:  rate.NewLimiter(rate.Limit(10), 20),
		cache:    make(map[string]*ExportedData),
		maxCache: 100,
	}
}

// CreateExportJob creates a new export job
func (s *ExportService) CreateExportJob(req ExportRequest) (*ExportJob, error) {
	// Rate limiting
	if err := s.limiter.Wait(context.Background()); err != nil {
		return nil, err
	}
	
	job := &ExportJob{
		ID:         uuid.New().String(),
		Type:       req.Type,
		Status:     "pending",
		Format:    req.Format,
		Filters:    req.Filters,
		Progress:  0,
		CreatedAt: time.Now(),
	}
	
	s.jobsMu.Lock()
	s.jobs[job.ID] = job
	s.jobsMu.Unlock()
	
	// Start processing in background
	go s.processJob(job.ID)
	
	return job, nil
}

func (s *ExportService) processJob(jobID string) {
	s.jobsMu.Lock()
	job := s.jobs[jobID]
	job.Status = "running"
	now := time.Now()
	job.StartedAt = &now
	s.jobsMu.Unlock()
	
	var err error
	var data *ExportedData
	
	switch job.Type {
	case "transactions":
		data, err = s.exportTransactions(job)
	case "blocks":
		data, err = s.exportBlocks(job)
	case "addresses":
		data, err = s.exportAddresses(job)
	case "tokens":
		data, err = s.exportTokens(job)
	case "contracts":
		data, err = s.exportContracts(job)
	case "events":
		data, err = s.exportEvents(job)
	default:
		err = fmt.Errorf("unknown export type: %s", job.Type)
	}
	
	s.jobsMu.Lock()
	if err != nil {
		job.Status = "failed"
		job.Error = err.Error()
	} else {
		job.Status = "completed"
		job.Result = data
		completed := time.Now()
		job.CompletedAt = &completed
	}
	job.Progress = 100
	s.jobsMu.Unlock()
}

// Export transactions
func (s *ExportService) exportTransactions(job *ExportJob) (*ExportedData, error) {
	query := `
		SELECT hash, block_number, from_address, to_address, value, 
		       gas_price, gas_used, input, status, timestamp
		FROM transactions
		WHERE 1=1
	`
	
	args := []interface{}{}
	argIdx := 1
	
	if job.Filters.FromBlock > 0 {
		query += fmt.Sprintf(" AND block_number >= $%d", argIdx)
		args = append(args, job.Filters.FromBlock)
		argIdx++
	}
	
	if job.Filters.ToBlock > 0 {
		query += fmt.Sprintf(" AND block_number <= $%d", argIdx)
		args = append(args, job.Filters.ToBlock)
		argIdx++
	}
	
	if job.Filters.Address != "" {
		query += fmt.Sprintf(" AND (from_address = $%d OR to_address = $%d)", argIdx, argIdx)
		args = append(args, job.Filters.Address)
		argIdx++
	}
	
	query += " ORDER BY block_number DESC"
	query += fmt.Sprintf(" LIMIT %d", 10000) // Limit for performance
	
	rows, err := s.db.Query(query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	generator := NewExcelGenerator()
	headers := []string{"Hash", "Block", "From", "To", "Value (ETH)", "Gas Price", "Gas Used", "Input", "Status", "Timestamp"}
	data := [][]interface{}{}
	
	for rows.Next() {
		var hash, from, to, input, status string
		var blockNumber, gasUsed uint64
		var value, gasPrice *big.Int
		var timestamp time.Time
		
		err := rows.Scan(&hash, &blockNumber, &from, &to, &value, &gasPrice, &gasUsed, &input, &status, &timestamp)
		if err != nil {
			continue
		}
		
		data = append(data, []interface{}{
			hash,
			blockNumber,
			from,
			to,
			weiToEth(value),
			weiToEth(gasPrice),
			gasUsed,
			truncateHex(input, 64),
			status,
			timestamp.Format(time.RFC3339),
		})
	}
	
	generator.AddSheet("Transactions", headers, data)
	
	return s.generateOutput(job, generator)
}

// Export blocks
func (s *ExportService) exportBlocks(job *ExportJob) (*ExportedData, error) {
	query := `
		SELECT number, hash, parent_hash, miner, gas_limit, gas_used, 
		       timestamp, transactions_count, size
		FROM blocks
		WHERE 1=1
	`
	
	args := []interface{}{}
	argIdx := 1
	
	if job.Filters.FromBlock > 0 {
		query += fmt.Sprintf(" AND number >= $%d", argIdx)
		args = append(args, job.Filters.FromBlock)
		argIdx++
	}
	
	if job.Filters.ToBlock > 0 {
		query += fmt.Sprintf(" AND number <= $%d", argIdx)
		args = append(args, job.Filters.ToBlock)
		argIdx++
	}
	
	query += " ORDER BY number DESC"
	query += fmt.Sprintf(" LIMIT %d", 10000)
	
	rows, err := s.db.Query(query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	generator := NewExcelGenerator()
	headers := []string{"Number", "Hash", "Parent Hash", "Miner", "Gas Limit", "Gas Used", "Timestamp", "Tx Count", "Size"}
	data := [][]interface{}{}
	
	for rows.Next() {
		var number, gasLimit, gasUsed, txCount, size uint64
		var hash, parentHash, miner string
		var timestamp time.Time
		
		err := rows.Scan(&number, &hash, &parentHash, &miner, &gasLimit, &gasUsed, &timestamp, &txCount, &size)
		if err != nil {
			continue
		}
		
		data = append(data, []interface{}{
			number,
			hash,
			parentHash,
			miner,
			gasLimit,
			gasUsed,
			timestamp.Format(time.RFC3339),
			txCount,
			size,
		})
	}
	
	generator.AddSheet("Blocks", headers, data)
	
	return s.generateOutput(job, generator)
}

// Export addresses
func (s *ExportService) exportAddresses(job *ExportJob) (*ExportedData, error) {
	query := `
		SELECT address, balance, nonce, code_hash, 
		       is_contract, created_at
		FROM addresses
		WHERE 1=1
	`
	
	args := []interface{}{}
	
	if job.Filters.Address != "" {
		query += " AND address = $1"
		args = append(args, job.Filters.Address)
	}
	
	if job.Filters.ContractType != "" {
		query += " AND is_contract = true"
	}
	
	query += " ORDER BY balance DESC"
	query += " LIMIT 10000"
	
	rows, err := s.db.Query(query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	generator := NewExcelGenerator()
	headers := []string{"Address", "Balance (ETH)", "Nonce", "Code Hash", "Is Contract", "Created At"}
	data := [][]interface{}{}
	
	for rows.Next() {
		var address, codeHash string
		var balance *big.Int
		var nonce uint64
		var isContract bool
		var createdAt time.Time
		
		err := rows.Scan(&address, &balance, &nonce, &codeHash, &isContract, &createdAt)
		if err != nil {
			continue
		}
		
		data = append(data, []interface{}{
			address,
			weiToEth(balance),
			nonce,
			codeHash,
			isContract,
			createdAt.Format(time.RFC3339),
		})
	}
	
	generator.AddSheet("Addresses", headers, data)
	
	return s.generateOutput(job, generator)
}

// Export tokens
func (s *ExportService) exportTokens(job *ExportJob) (*ExportedData, error) {
	query := `
		SELECT address, name, symbol, decimals, total_supply,
		       type, price_usd, volume_24h, holders
		FROM tokens
		WHERE 1=1
	`
	
	args := []interface{}{}
	
	if job.Filters.TokenType != "" {
		query += " AND type = $1"
		args = append(args, job.Filters.TokenType)
	}
	
	query += " ORDER BY volume_24h DESC"
	query += " LIMIT 10000"
	
	rows, err := s.db.Query(query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	generator := NewExcelGenerator()
	headers := []string{"Address", "Name", "Symbol", "Decimals", "Total Supply", "Type", "Price ($)", "Volume 24h", "Holders"}
	data := [][]interface{}{}
	
	for rows.Next() {
		var address, name, symbol, tokenType string
		var decimals uint8
		var totalSupply, priceUSD, volume24h *big.Int
		var holders uint64
		
		err := rows.Scan(&address, &name, &symbol, &decimals, &totalSupply, &tokenType, &priceUSD, &volume24h, &holders)
		if err != nil {
			continue
		}
		
		data = append(data, []interface{}{
			address,
			name,
			symbol,
			decimals,
			formatSupply(totalSupply, decimals),
			tokenType,
			formatUSD(priceUSD),
			formatUSD(volume24h),
			holders,
		})
	}
	
	generator.AddSheet("Tokens", headers, data)
	
	return s.generateOutput(job, generator)
}

// Export contracts
func (s *ExportService) exportContracts(job *ExportJob) (*ExportedData, error) {
	query := `
		SELECT address, name, compiler, version, optimization,
		       runs, evm_version, source_code
		FROM contracts
		WHERE 1=1
	`
	
	args := []interface{}{}
	
	if job.Filters.ContractType != "" {
		query += " AND name LIKE $1"
		args = append(args, "%"+job.Filters.ContractType+"%")
	}
	
	query += " ORDER BY address"
	query += " LIMIT 10000"
	
	rows, err := s.db.Query(query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	generator := NewExcelGenerator()
	headers := []string{"Address", "Name", "Compiler", "Version", "Optimized", "Runs", "EVM", "Has Source"}
	data := [][]interface{}{}
	
	for rows.Next() {
		var address, name, compiler, version, evmVersion string
		var optimization bool
		var runs uint64
		var sourceCode sql.NullString
		
		err := rows.Scan(&address, &name, &compiler, &version, &optimization, &runs, &evmVersion, &sourceCode)
		if err != nil {
			continue
		}
		
		data = append(data, []interface{}{
			address,
			name,
			compiler,
			version,
			optimization,
			runs,
			evmVersion,
			sourceCode.Valid,
		})
	}
	
	generator.AddSheet("Contracts", headers, data)
	
	return s.generateOutput(job, generator)
}

// Export events
func (s *ExportService) exportEvents(job *ExportJob) (*ExportedData, error) {
	query := `
		SELECT address, block_number, transaction_hash, 
		       log_index, topic0, topic1, topic2, topic3, data
		FROM event_logs
		WHERE 1=1
	`
	
	args := []interface{}{}
	argIdx := 1
	
	if job.Filters.Address != "" {
		query += fmt.Sprintf(" AND address = $%d", argIdx)
		args = append(args, job.Filters.Address)
		argIdx++
	}
	
	if job.Filters.FromBlock > 0 {
		query += fmt.Sprintf(" AND block_number >= $%d", argIdx)
		args = append(args, job.Filters.FromBlock)
		argIdx++
	}
	
	if job.Filters.ToBlock > 0 {
		query += fmt.Sprintf(" AND block_number <= $%d", argIdx)
		args = append(args, job.Filters.ToBlock)
		argIdx++
	}
	
	query += " ORDER BY block_number DESC, log_index"
	query += " LIMIT 10000"
	
	rows, err := s.db.Query(query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	generator := NewExcelGenerator()
	headers := []string{"Contract", "Block", "Transaction", "Log Index", "Topic 0", "Topic 1", "Topic 2", "Topic 3", "Data"}
	data := [][]interface{}{}
	
	for rows.Next() {
		var address, txHash, topic0, topic1, topic2, topic3, dataStr string
		var blockNumber, logIndex uint64
		
		err := rows.Scan(&address, &blockNumber, &txHash, &logIndex, &topic0, &topic1, &topic2, &topic3, &dataStr)
		if err != nil {
			continue
		}
		
		data = append(data, []interface{}{
			address,
			blockNumber,
			txHash,
			logIndex,
			topic0,
			topic1,
			topic2,
			topic3,
			truncateHex(dataStr, 64),
		})
	}
	
	generator.AddSheet("Events", headers, data)
	
	return s.generateOutput(job, generator)
}

func (s *ExportService) generateOutput(job *ExportJob, generator *ExcelGenerator) (*ExportedData, error) {
	var output []byte
	var err error
	
	switch job.Format {
	case "xlsx":
		output, err = generator.Generate()
	case "csv":
		output, err = generator.GenerateCSV()
	case "json":
		output, err = generator.GenerateJSON()
	default:
		output, err = generator.GenerateCSV()
	}
	
	if err != nil {
		return nil, err
	}
	
	// Calculate checksum (SHA256)
	// In production, use proper hash
	checksum := fmt.Sprintf("%x", len(output))
	
	// Get row/column counts
	var rows, cols int
	for _, sheet := range generator.sheets {
		rows = max(rows, len(sheet.Rows))
		if len(sheet.Rows) > 0 {
			cols = max(cols, len(sheet.Rows[0]))
		}
	}
	
	return &ExportedData{
		ID:        uuid.New().String(),
		Type:      job.Type,
		Data:      output,
		Format:    job.Format,
		Checksum:  checksum,
		Size:      int64(len(output)),
		Rows:      rows,
		Columns:   cols,
		Created:   time.Now(),
	}, nil
}

// Get job status
func (s *ExportService) GetJob(jobID string) (*ExportJob, error) {
	s.jobsMu.RLock()
	defer s.jobsMu.RUnlock()
	
	job, ok := s.jobs[jobID]
	if !ok {
		return nil, fmt.Errorf("job not found: %s", jobID)
	}
	
	return job, nil
}

// List jobs
func (s *ExportService) ListJobs() []*ExportJob {
	s.jobsMu.RLock()
	defer s.jobsMu.RUnlock()
	
	jobs := make([]*ExportJob, 0, len(s.jobs))
	for _, job := range s.jobs {
		jobs = append(jobs, job)
	}
	
	return jobs
}

// ============================================
// HTTP Handlers
// ============================================

func (s *ExportService) HandleCreateExport(w http.ResponseWriter, r *http.Request) {
	var req ExportRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	
	job, err := s.CreateExportJob(req)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(job)
}

func (s *ExportService) HandleGetExport(w http.ResponseWriter, r *http.Request) {
	jobID := strings.TrimPrefix(r.URL.Path, "/exports/")
	
	job, err := s.GetJob(jobID)
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}
	
	// Return job without data for status check
	type JobStatus struct {
		ID          string    `json:"id"`
		Type       string    `json:"type"`
		Status     string    `json:"status"`
		Progress   float64   `json:"progress"`
		Error      string    `json:"error,omitempty"`
		CreatedAt  time.Time `json:"created_at"`
		CompletedAt *time.Time `json:"completed_at,omitempty"`
	}
	
	status := JobStatus{
		ID:          job.ID,
		Type:        job.Type,
		Status:      job.Status,
		Progress:    job.Progress,
		Error:      job.Error,
		CreatedAt:  job.CreatedAt,
		CompletedAt: job.CompletedAt,
	}
	
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(status)
}

func (s *ExportService) HandleDownloadExport(w http.ResponseWriter, r *http.Request) {
	jobID := strings.TrimPrefix(r.URL.Path, "/exports/")
	
	job, err := s.GetJob(jobID)
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}
	
	if job.Status != "completed" || job.Result == nil {
		http.Error(w, "export not ready", http.StatusBadRequest)
		return
	}
	
	// Set headers for download
	filename := fmt.Sprintf("export_%s_%s.%s", job.Type, job.ID[:8], job.Format)
	w.Header().Set("Content-Disposition", fmt.Sprintf("attachment; filename=%s", filename))
	w.Header().Set("Content-Type", "application/octet-stream")
	w.Header().Set("Content-Length", strconv.FormatInt(job.Result.Size, 10))
	
	w.Write(job.Result.Data)
}

// ============================================
// Helper Functions
// ============================================

func weiToEth(wei *big.Int) string {
	if wei == nil {
		return "0"
	}
	
	eth := new(big.Float).SetInt(wei)
	eth.Mul(eth, big.NewFloat(1e-18))
	
	return eth.Text('f', 18)
}

func formatUSD(value *big.Int) string {
	if value == nil {
		return "$0.00"
	}
	
	dollars := new(big.Float).SetInt(value)
	dollars.Mul(dollars, big.NewFloat(1e-8))
	
	return fmt.Sprintf("$%.2f", dollars)
}

func formatSupply(supply *big.Int, decimals uint8) string {
	if supply == nil {
		return "0"
	}
	
	divisor := new(big.Int).Exp(big.NewInt(10), big.NewInt(int64(decimals)), nil)
	result := new(big.Float).SetInt(divisor)
	result.Mul(result, big.NewFloat(1e-18))
	
	return result.Text('f', int(decimals))
}

func truncateHex(hex string, maxLen int) string {
	if len(hex) <= maxLen {
		return hex
	}
	return hex[:maxLen] + "..."
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}

// ============================================
// Main
// ============================================

func main() {
	// Database connection
	db, err := sql.Open("postgres", os.Getenv("DATABASE_URL"))
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to connect to database: %v\n", err)
		os.Exit(1)
	}
	defer db.Close()
	
	// Create export service
	exportSvc := NewExportService(db)
	
	// HTTP server
	mux := http.NewServeMux()
	mux.HandleFunc("/exports", exportSvc.HandleCreateExport)
	mux.HandleFunc("/exports/", exportSvc.HandleGetExport)
	mux.HandleFunc("/downloads/", exportSvc.HandleDownloadExport)
	
	fmt.Println("Export service starting on :8080")
	http.ListenAndServe(":8080", mux)
}