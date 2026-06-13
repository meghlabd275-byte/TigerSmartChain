// Package export provides data export service for CSV, JSON, Excel
// Built with Go for high performance
package export

import (
	"context"
	"encoding/csv"
	"encoding/json"
	"fmt"
	"os"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

// Config holds export service configuration
type Config struct {
	DBURL          string
	RedisURL       string
	ExportDir     string
	MaxRows       int
	Timeout       time.Duration
}

// ExportJob represents an export job
type ExportJob struct {
	ID          string    `json:"id"`
	UserID     int       `json:"userId"`
	Type       string    `json:"type"` // blocks, transactions, tokens, etc
	Format     string    `json:"format"` // csv, json, xlsx
	Filters    string    `json:"filters"`
	Status     string    `json:"status"` // pending, processing, completed, failed
	FileURL    string    `json:"fileUrl,omitempty"`
	TotalRows  int       `json:"totalRows"`
	Progress   int       `json:"progress"`
	CreatedAt  time.Time `json:"createdAt"`
	CompletedAt *time.Time `json:"completedAt,omitempty"`
	Error      string    `json:"error,omitempty"`
}

// ExportData represents export data
type ExportData struct {
	Headers []string
	Rows    [][]interface{}
}

// Server represents the export service server
type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	jobs  map[string]*ExportJob
	mu    sync.RWMutex
}

// NewServer creates a new export service server
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
	if err := os.MkdirAll(cfg.ExportDir, 0755); err != nil {
		return nil, fmt.Errorf("failed to create export dir: %w", err)
	}
	if err := createTables(ctx, pool); err != nil {
		return nil, fmt.Errorf("failed to create tables: %w", err)
	}
	srv := &Server{cfg: cfg, pool: pool, redis: rdb, jobs: make(map[string]*ExportJob)}
	go srv.startProcessor()
	return srv, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) error {
	queries := []string{
		`CREATE TABLE IF NOT EXISTS export_jobs (id VARCHAR(36) PRIMARY KEY, user_id INTEGER NOT NULL, type VARCHAR(50) NOT NULL, format VARCHAR(20) NOT NULL, filters TEXT, status VARCHAR(20) DEFAULT 'pending', file_url TEXT, total_rows INTEGER DEFAULT 0, progress INTEGER DEFAULT 0, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, completed_at TIMESTAMP, error TEXT)`,
	}
	for _, q := range queries {
		if _, err := pool.Exec(ctx, q); err != nil {
			return err
		}
	}
	return nil
}

func (s *Server) startProcessor() {
	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()
	for range ticker.C {
		s.processJobs()
	}
}

func (s *Server) processJobs() {
	ctx := context.Background()
	
	// Get pending jobs
	rows, err := s.pool.Query(ctx, `SELECT id, user_id, type, format, filters FROM export_jobs WHERE status = 'pending' ORDER BY created_at ASC LIMIT 10`)
	if err != nil {
		return
	}
	defer rows.Close()
	
	type jobInfo struct {
		id     string
		userID int
		Type   string
		format string
		filters string
	}
	
	var pendingJobs []jobInfo
	for rows.Next() {
		var j jobInfo
		if err := rows.Scan(&j.id, &j.userID, &j.Type, &j.format, &j.filters); err != nil {
			continue
		}
		pendingJobs = append(pendingJobs, j)
	}
	
	for _, j := range pendingJobs {
		// Update status to processing
		s.pool.Exec(ctx, `UPDATE export_jobs SET status = 'processing' WHERE id = $1`, j.id)
		
		// Process export
		if err := s.processExport(ctx, j.id, j.Type, j.format, j.filters); err != nil {
			s.pool.Exec(ctx, `UPDATE export_jobs SET status = 'failed', error = $1 WHERE id = $2`, err.Error(), j.id)
		}
	}
}

func (s *Server) processExport(ctx context.Context, jobID, exportType, format, filters string) error {
	var data *ExportData
	var err error
	
	switch exportType {
	case "blocks":
		data, err = s.exportBlocks(ctx, filters)
	case "transactions":
		data, err = s.exportTransactions(ctx, filters)
	case "tokens":
		data, err = s.exportTokens(ctx, filters)
	case "token_transfers":
		data, err = s.exportTokenTransfers(ctx, filters)
	case "nfts":
		data, err = s.exportNFTs(ctx, filters)
	case "nft_transfers":
		data, err = s.exportNFTTransfers(ctx, filters)
	case "accounts":
		data, err = s.exportAccounts(ctx, filters)
	default:
		return fmt.Errorf("unknown export type: %s", exportType)
	}
	
	if err != nil {
		return err
	}
	
	// Write to file
	filename := fmt.Sprintf("%s/%s.%s", s.cfg.ExportDir, jobID, format)
	switch format {
	case "csv":
		err = s.writeCSV(filename, data)
	case "json":
		err = s.writeJSON(filename, data)
	default:
		return fmt.Errorf("unsupported format: %s", format)
	}
	
	if err != nil {
		return err
	}
	
	// Update job status
	now := time.Now()
	s.pool.Exec(ctx, `UPDATE export_jobs SET status = 'completed', file_url = $1, total_rows = $2, progress = 100, completed_at = $3 WHERE id = $4`,
		filename, len(data.Rows), now, jobID)
	
	return nil
}

func (s *Server) exportBlocks(ctx context.Context, filters string) (*ExportData, error) {
	rows, err := s.pool.Query(ctx, `SELECT number, hash, parent_hash, timestamp, tx_count, gas_used, gas_limit, miner, size, difficulty FROM blocks ORDER BY number DESC LIMIT $1`, s.cfg.MaxRows)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	data := &ExportData{
		Headers: []string{"Block Number", "Hash", "Parent Hash", "Timestamp", "Transactions", "Gas Used", "Gas Limit", "Miner", "Size", "Difficulty"},
		Rows: make([][]interface{}, 0),
	}
	
	for rows.Next() {
		var row []interface{}
		var number, timestamp, txCount, gasUsed, gasLimit, size, difficulty int64
		var hash, parentHash, miner string
		if err := rows.Scan(&number, &hash, &parentHash, &timestamp, &txCount, &gasUsed, &gasLimit, &miner, &size, &difficulty); err != nil {
			continue
		}
		row = []interface{}{number, hash, parentHash, time.Unix(timestamp, 0).Format(time.RFC3339), txCount, gasUsed, gasLimit, miner, size, difficulty}
		data.Rows = append(data.Rows, row)
	}
	
	return data, nil
}

func (s *Server) exportTransactions(ctx context.Context, filters string) (*ExportData, error) {
	rows, err := s.pool.Query(ctx, `SELECT hash, block_number, from_address, to_address, value, gas_price, gas_used, status, timestamp FROM transactions ORDER BY timestamp DESC LIMIT $1`, s.cfg.MaxRows)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	data := &ExportData{
		Headers: []string{"Hash", "Block", "From", "To", "Value", "Gas Price", "Gas Used", "Status", "Timestamp"},
		Rows: make([][]interface{}, 0),
	}
	
	for rows.Next() {
		var row []interface{}
		var hash, from, to, value string
		var blockNumber, gasPrice, gasUsed int64
		var status int
		var timestamp int64
		if err := rows.Scan(&hash, &blockNumber, &from, &to, &value, &gasPrice, &gasUsed, &status, &timestamp); err != nil {
			continue
		}
		row = []interface{}{hash, blockNumber, from, to, value, gasPrice, gasUsed, status, time.Unix(timestamp, 0).Format(time.RFC3339)}
		data.Rows = append(data.Rows, row)
	}
	
	return data, nil
}

func (s *Server) exportTokens(ctx context.Context, filters string) (*ExportData, error) {
	rows, err := s.pool.Query(ctx, `SELECT address, name, symbol, decimals, total_supply, holders_count, transfers_count, is_verified FROM tokens ORDER BY transfers_count DESC LIMIT $1`, s.cfg.MaxRows)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	data := &ExportData{
		Headers: []string{"Address", "Name", "Symbol", "Decimals", "Total Supply", "Holders", "Transfers", "Verified"},
		Rows: make([][]interface{}, 0),
	}
	
	for rows.Next() {
		var row []interface{}
		var address, name, symbol, totalSupply string
		var decimals, holdersCount, transfersCount int
		var isVerified bool
		if err := rows.Scan(&address, &name, &symbol, &decimals, &totalSupply, &holdersCount, &transfersCount, &isVerified); err != nil {
			continue
		}
		row = []interface{}{address, name, symbol, decimals, totalSupply, holdersCount, transfersCount, isVerified}
		data.Rows = append(data.Rows, row)
	}
	
	return data, nil
}

func (s *Server) exportTokenTransfers(ctx context.Context, filters string) (*ExportData, error) {
	rows, err := s.pool.Query(ctx, `SELECT token_address, hash, block_number, from_address, to_address, value, timestamp FROM token_transfers ORDER BY timestamp DESC LIMIT $1`, s.cfg.MaxRows)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	data := &ExportData{
		Headers: []string{"Token", "Transaction Hash", "Block", "From", "To", "Value", "Timestamp"},
		Rows: make([][]interface{}, 0),
	}
	
	for rows.Next() {
		var row []interface{}
		var tokenAddr, hash, from, to, value string
		var blockNumber, timestamp int64
		if err := rows.Scan(&tokenAddr, &hash, &blockNumber, &from, &to, &value, &timestamp); err != nil {
			continue
		}
		row = []interface{}{tokenAddr, hash, blockNumber, from, to, value, time.Unix(timestamp, 0).Format(time.RFC3339)}
		data.Rows = append(data.Rows, row)
	}
	
	return data, nil
}

func (s *Server) exportNFTs(ctx context.Context, filters string) (*ExportData, error) {
	rows, err := s.pool.Query(ctx, `SELECT address, token_id, name, owner, collection_address, image_url FROM nfts ORDER BY token_id DESC LIMIT $1`, s.cfg.MaxRows)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	data := &ExportData{
		Headers: []string{"Address", "Token ID", "Name", "Owner", "Collection", "Image URL"},
		Rows: make([][]interface{}, 0),
	}
	
	for rows.Next() {
		var row []interface{}
		var address, tokenID, name, owner, collectionAddr, imageURL string
		if err := rows.Scan(&address, &tokenID, &name, &owner, &collectionAddr, &imageURL); err != nil {
			continue
		}
		row = []interface{}{address, tokenID, name, owner, collectionAddr, imageURL}
		data.Rows = append(data.Rows, row)
	}
	
	return data, nil
}

func (s *Server) exportNFTTransfers(ctx context.Context, filters string) (*ExportData, error) {
	rows, err := s.pool.Query(ctx, `SELECT nft_address, token_id, hash, block_number, from_address, to_address, timestamp FROM nft_transfers ORDER BY timestamp DESC LIMIT $1`, s.cfg.MaxRows)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	data := &ExportData{
		Headers: []string{"NFT", "Token ID", "Transaction Hash", "Block", "From", "To", "Timestamp"},
		Rows: make([][]interface{}, 0),
	}
	
	for rows.Next() {
		var row []interface{}
		var nftAddr, tokenID, hash, from, to string
		var blockNumber, timestamp int64
		if err := rows.Scan(&nftAddr, &tokenID, &hash, &blockNumber, &from, &to, &timestamp); err != nil {
			continue
		}
		row = []interface{}{nftAddr, tokenID, hash, blockNumber, from, to, time.Unix(timestamp, 0).Format(time.RFC3339)}
		data.Rows = append(data.Rows, row)
	}
	
	return data, nil
}

func (s *Server) exportAccounts(ctx context.Context, filters string) (*ExportData, error) {
	rows, err := s.pool.Query(ctx, `SELECT address, balance, nonce, code_hash, is_contract, is_verified FROM accounts ORDER BY balance DESC LIMIT $1`, s.cfg.MaxRows)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	data := &ExportData{
		Headers: []string{"Address", "Balance", "Nonce", "Code Hash", "Is Contract", "Is Verified"},
		Rows: make([][]interface{}, 0),
	}
	
	for rows.Next() {
		var row []interface{}
		var address, balance, codeHash string
		var nonce int
		var isContract, isVerified bool
		if err := rows.Scan(&address, &balance, &nonce, &codeHash, &isContract, &isVerified); err != nil {
			continue
		}
		row = []interface{}{address, balance, nonce, codeHash, isContract, isVerified}
		data.Rows = append(data.Rows, row)
	}
	
	return data, nil
}

func (s *Server) writeCSV(filename string, data *ExportData) error {
	file, err := os.Create(filename)
	if err != nil {
		return err
	}
	defer file.Close()
	
	writer := csv.NewWriter(file)
	defer writer.Flush()
	
	// Write header
	writer.Write(data.Headers)
	
	// Write rows
	for _, row := range data.Rows {
		strRow := make([]string, len(row))
		for i, v := range row {
			strRow[i] = fmt.Sprintf("%v", v)
		}
		writer.Write(strRow)
	}
	
	return nil
}

func (s *Server) writeJSON(filename string, data *ExportData) error {
	file, err := os.Create(filename)
	if err != nil {
		return err
	}
	defer file.Close()
	
	// Convert to map for JSON
	result := make([]map[string]interface{}, len(data.Rows))
	for i, row := range data.Rows {
		item := make(map[string]interface{})
		for j, header := range data.Headers {
			item[header] = row[j]
		}
		result[i] = item
	}
	
	encoder := json.NewEncoder(file)
	encoder.SetIndent("", "  ")
	return encoder.Encode(result)
}

// CreateExportJob creates a new export job
func (s *Server) CreateExportJob(ctx context.Context, userID int, exportType, format, filters string) (*ExportJob, error) {
	jobID := fmt.Sprintf("%d-%d", userID, time.Now().Unix())
	
	job := &ExportJob{
		ID:         jobID,
		UserID:     userID,
		Type:       exportType,
		Format:     format,
		Filters:    filters,
		Status:     "pending",
		Progress:   0,
		CreatedAt: time.Now(),
	}
	
	_, err := s.pool.Exec(ctx, `INSERT INTO export_jobs (id, user_id, type, format, filters, status) VALUES ($1, $2, $3, $4, $5, 'pending')`,
		job.ID, job.UserID, job.Type, job.Format, job.Filters)
	if err != nil {
		return nil, err
	}
	
	s.mu.Lock()
	s.jobs[jobID] = job
	s.mu.Unlock()
	
	return job, nil
}

// GetExportJob returns export job status
func (s *Server) GetExportJob(ctx context.Context, jobID string) (*ExportJob, error) {
	var job ExportJob
	err := s.pool.QueryRow(ctx, `SELECT id, user_id, type, format, filters, status, file_url, total_rows, progress, created_at, completed_at, error FROM export_jobs WHERE id = $1`, jobID).Scan(
		&job.ID, &job.UserID, &job.Type, &job.Format, &job.Filters, &job.Status, &job.FileURL, &job.TotalRows, &job.Progress, &job.CreatedAt, &job.CompletedAt, &job.Error)
	if err != nil {
		return nil, err
	}
	return &job, nil
}

// GetUserExportJobs returns export jobs for a user
func (s *Server) GetUserExportJobs(ctx context.Context, userID int) ([]ExportJob, error) {
	rows, err := s.pool.Query(ctx, `SELECT id, user_id, type, format, filters, status, file_url, total_rows, progress, created_at, completed_at, error FROM export_jobs WHERE user_id = $1 ORDER BY created_at DESC LIMIT 20`, userID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var jobs []ExportJob
	for rows.Next() {
		var job ExportJob
		if err := rows.Scan(&job.ID, &job.UserID, &job.Type, &job.Format, &job.Filters, &job.Status, &job.FileURL, &job.TotalRows, &job.Progress, &job.CreatedAt, &job.CompletedAt, &job.Error); err != nil {
			continue
		}
		jobs = append(jobs, job)
	}
	
	return jobs, nil
}