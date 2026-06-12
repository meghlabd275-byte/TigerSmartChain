// Package export provides data export API for CSV, JSON, and other formats
package export

import (
	"context"
	"encoding/csv"
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5"
	
	"tigersmartchain/explorer/services/tokens"
	"tigersmartchain/explorer/services/nfts"
	"tigersmartchain/explorer/services/analytics"
)

// Config holds the export service configuration
type Config struct {
	DBURL        string
	Port         string
	MaxRows      int
	BatchSize    int
	AllowCustom  bool
}

// ExportFormat represents the export format
type ExportFormat string

const (
	FormatJSON  ExportFormat = "json"
	FormatCSV   ExportFormat = "csv"
	FormatXML   ExportFormat = "xml"
	FormatNDJSON ExportFormat = "ndjson"
)

// ExportService handles data export requests
type ExportService struct {
	config     *Config
	db         *pgx.Conn
	tokenSvc  *tokens.TokenService
	nftSvc    *nfts.NFTService
	analyticsSvc *analytics.AnalyticsService
}

// NewExportService creates a new export service
func NewExportService(config *Config) (*ExportService, error) {
	ctx := context.Background()
	db, err := pgx.Connect(ctx, config.DBURL)
	if err != nil {
		return nil, err
	}
	
	return &ExportService{
		config:    config,
		db:        db,
	}, nil
}

// SetServices sets the backend services
func (s *ExportService) SetServices(tokenSvc *tokens.TokenService, nftSvc *nfts.NFTService, analyticsSvc *analytics.AnalyticsService) {
	s.tokenSvc = tokenSvc
	s.nftSvc = nftSvc
	s.analyticsSvc = analyticsSvc
}

// ExportRequest represents an export request
type ExportRequest struct {
	Type      string      `json:"type" form:"type"`
	Address  string      `json:"address" form:"address"`
	StartBlock uint64   `json:"startBlock" form:"startBlock"`
	EndBlock uint64     `json:"endBlock" form:"endBlock"`
	StartTime time.Time `json:"startTime" form:"startTime"`
	EndTime   time.Time `json:"endTime" form:"endTime"`
	Format   string      `json:"format" form:"format"`
	Limit    int        `json:"limit" form:"limit"`
	Page     int        `json:"page" form:"page"`
}

// ExportResponse represents an export response
type ExportResponse struct {
	TotalRecords int         `json:"totalRecords"`
	Data        interface{} `json:"data"`
	ExportedAt  time.Time   `json:"exportedAt"`
}

// handleExport handles the main export endpoint
func (s *ExportService) handleExport(c *gin.Context) {
	var req ExportRequest
	if err := c.ShouldBind(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "0",
			"message": "Invalid request parameters",
			"error":   err.Error(),
		})
		return
	}
	
	// Set defaults
	if req.Limit == 0 {
		req.Limit = s.config.MaxRows
	}
	if req.Format == "" {
		req.Format = string(FormatJSON)
	}
	
	// Validate export type
	switch req.Type {
	case "transactions", "token_transfers", "nft_transfers", "blocks", 
	     "token_holders", "nft_holders", "internal_transfers", "logs":
		// Valid types
	default:
		c.JSON(http.StatusBadRequest, gin.H{
			"status":  "0",
			"message": "Invalid export type",
		})
		return
	}
	
	ctx := c.Request.Context()
	var data interface{}
	var err error
	
	switch req.Type {
	case "transactions":
		data, err = s.exportTransactions(ctx, req)
	case "token_transfers":
		data, err = s.exportTokenTransfers(ctx, req)
	case "nft_transfers":
		data, err = s.exportNFTTransfers(ctx, req)
	case "blocks":
		data, err = s.exportBlocks(ctx, req)
	case "token_holders":
		data, err = s.exportTokenHolders(ctx, req)
	case "nft_holders":
		data, err = s.exportNFTHolders(ctx, req)
	case "internal_transfers":
		data, err = s.exportInternalTransfers(ctx, req)
	case "logs":
		data, err = s.exportLogs(ctx, req)
	default:
		c.JSON(http.StatusBadRequest, gin.H{"message": "Invalid type"})
		return
	}
	
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{
			"status":  "0",
			"message": err.Error(),
		})
		return
	}
	
	// Format output
	switch ExportFormat(req.Format) {
	case FormatCSV:
		s.writeCSV(c, data, req.Type)
	case FormatNDJSON:
		s.writeNDJSON(c, data)
	default:
		c.JSON(http.StatusOK, ExportResponse{
			TotalRecords: s.countRecords(data),
			Data:        data,
			ExportedAt:  time.Now(),
		})
	}
}

// exportTransactions exports transaction data
func (s *ExportService) exportTransactions(ctx context.Context, req ExportRequest) (interface{}, error) {
	query := `
		SELECT hash, block_number, from_address, to_address, value, gas_price, 
		       gas_used, timestamp, status, transaction_index
		FROM transactions
		WHERE 1=1
	`
	
	args := []interface{}{}
	argIdx := 1
	
	if req.Address != "" {
		query += fmt.Sprintf(" AND (from_address = $%d OR to_address = $%d)", argIdx, argIdx)
		args = append(args, req.Address)
		argIdx++
	}
	if req.StartBlock > 0 {
		query += fmt.Sprintf(" AND block_number >= $%d", argIdx)
		args = append(args, req.StartBlock)
		argIdx++
	}
	if req.EndBlock > 0 {
		query += fmt.Sprintf(" AND block_number <= $%d", argIdx)
		args = append(args, req.EndBlock)
		argIdx++
	}
	if !req.StartTime.IsZero() {
		query += fmt.Sprintf(" AND timestamp >= $%d", argIdx)
		args = append(args, req.StartTime)
		argIdx++
	}
	if !req.EndTime.IsZero() {
		query += fmt.Sprintf(" AND timestamp <= $%d", argIdx)
		args = append(args, req.EndTime)
		argIdx++
	}
	
	query += fmt.Sprintf(" ORDER BY block_number DESC, transaction_index DESC LIMIT $%d", argIdx)
	args = append(args, req.Limit)
	
	rows, err := s.db.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	type Transaction struct {
		Hash           string    `json:"hash"`
		BlockNumber   uint64    `json:"blockNumber"`
		From          string    `json:"from"`
		To            string    `json:"to"`
		Value         string    `json:"value"`
		GasPrice      uint64    `json:"gasPrice"`
		GasUsed       uint64    `json:"gasUsed"`
		Timestamp    time.Time `json:"timestamp"`
		Status        string    `json:"status"`
		TransactionIndex uint64 `json:"transactionIndex"`
	}
	
	var results []Transaction
	for rows.Next() {
		var t Transaction
		if err := rows.Scan(&t.Hash, &t.BlockNumber, &t.From, &t.To, &t.Value,
			&t.GasPrice, &t.GasUsed, &t.Timestamp, &t.Status, &t.TransactionIndex); err != nil {
			return nil, err
		}
		results = append(results, t)
	}
	
	return results, nil
}

// exportTokenTransfers exports token transfer data
func (s *ExportService) exportTokenTransfers(ctx context.Context, req ExportRequest) (interface{}, error) {
	query := `
		SELECT transaction_hash, block_number, token_address, 
		       from_address, to_address, amount, timestamp
		FROM token_transfers
		WHERE 1=1
	`
	
	args := []interface{}{}
	argIdx := 1
	
	if req.Address != "" {
		query += fmt.Sprintf(" AND token_address = $%d", argIdx)
		args = append(args, req.Address)
		argIdx++
	}
	if req.StartBlock > 0 {
		query += fmt.Sprintf(" AND block_number >= $%d", argIdx)
		args = append(args, req.StartBlock)
		argIdx++
	}
	if req.EndBlock > 0 {
		query += fmt.Sprintf(" AND block_number <= $%d", argIdx)
		args = append(args, req.EndBlock)
		argIdx++
	}
	
	query += fmt.Sprintf(" ORDER BY block_number DESC LIMIT $%d", argIdx)
	args = append(args, req.Limit)
	
	type Transfer struct {
		TxHash      string    `json:"transactionHash"`
		BlockNumber uint64   `json:"blockNumber"`
		Token       string    `json:"tokenAddress"`
		From        string    `json:"from"`
		To          string    `json:"to"`
		Amount      string    `json:"amount"`
		Timestamp   time.Time `json:"timestamp"`
	}
	
	rows, err := s.db.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var results []Transfer
	for rows.Next() {
		var t Transfer
		if err := rows.Scan(&t.TxHash, &t.BlockNumber, &t.Token, 
			&t.From, &t.To, &t.Amount, &t.Timestamp); err != nil {
			return nil, err
		}
		results = append(results, t)
	}
	
	return results, nil
}

// exportNFTTransfers exports NFT transfer data
func (s *ExportService) exportNFTTransfers(ctx context.Context, req ExportRequest) (interface{}, error) {
	query := `
		SELECT transaction_hash, block_number, token_address, token_id,
		       from_address, to_address, timestamp
		FROM nft_transfers
		WHERE 1=1
	`
	
	args := []interface{}{}
	argIdx := 1
	
	if req.Address != "" {
		query += fmt.Sprintf(" AND token_address = $%d", argIdx)
		args = append(args, req.Address)
		argIdx++
	}
	if req.StartBlock > 0 {
		query += fmt.Sprintf(" AND block_number >= $%d", argIdx)
		args = append(args, req.StartBlock)
		argIdx++
	}
	if req.EndBlock > 0 {
		query += fmt.Sprintf(" AND block_number <= $%d", argIdx)
		args = append(args, req.EndBlock)
		argIdx++
	}
	
	query += fmt.Sprintf(" ORDER BY block_number DESC LIMIT $%d", argIdx)
	args = append(args, req.Limit)
	
	type Transfer struct {
		TxHash      string    `json:"transactionHash"`
		BlockNumber uint64   `json:"blockNumber"`
		Token       string    `json:"tokenAddress"`
		TokenID     string    `json:"tokenId"`
		From        string    `json:"from"`
		To          string    `json:"to"`
		Timestamp   time.Time `json:"timestamp"`
	}
	
	rows, err := s.db.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var results []Transfer
	for rows.Next() {
		var t Transfer
		if err := rows.Scan(&t.TxHash, &t.BlockNumber, &t.Token, 
			&t.TokenID, &t.From, &t.To, &t.Timestamp); err != nil {
			return nil, err
		}
		results = append(results, t)
	}
	
	return results, nil
}

// exportBlocks exports block data
func (s *ExportService) exportBlocks(ctx context.Context, req ExportRequest) (interface{}, error) {
	query := `
		SELECT number, hash, parent_hash, miner, gas_used, gas_limit, 
		       timestamp, transaction_count, uncle_count
		FROM blocks
		WHERE 1=1
	`
	
	args := []interface{}{}
	argIdx := 1
	
	if req.StartBlock > 0 {
		query += fmt.Sprintf(" AND number >= $%d", argIdx)
		args = append(args, req.StartBlock)
		argIdx++
	}
	if req.EndBlock > 0 {
		query += fmt.Sprintf(" AND number <= $%d", argIdx)
		args = append(args, req.EndBlock)
		argIdx++
	}
	
	query += fmt.Sprintf(" ORDER BY number DESC LIMIT $%d", argIdx)
	args = append(args, req.Limit)
	
	type Block struct {
		Number           uint64    `json:"number"`
		Hash            string    `json:"hash"`
		ParentHash      string    `json:"parentHash"`
		Miner           string    `json:"miner"`
		GasUsed         uint64    `json:"gasUsed"`
		GasLimit        uint64    `json:"gasLimit"`
		Timestamp       time.Time `json:"timestamp"`
		TransactionCount uint64  `json:"transactionCount"`
		UncleCount      uint64    `json:"uncleCount"`
	}
	
	rows, err := s.db.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var results []Block
	for rows.Next() {
		var b Block
		if err := rows.Scan(&b.Number, &b.Hash, &b.ParentHash, &b.Miner, 
			&b.GasUsed, &b.GasLimit, &b.Timestamp, &b.TransactionCount, &b.UncleCount); err != nil {
			return nil, err
		}
		results = append(results, b)
	}
	
	return results, nil
}

// exportTokenHolders exports token holder data
func (s *ExportService) exportTokenHolders(ctx context.Context, req ExportRequest) (interface{}, error) {
	query := `
		SELECT address, balance, percent_hold
		FROM token_holders
		WHERE token_address = $1
		ORDER BY balance DESC
		LIMIT $2
	`
	
	rows, err := s.db.Query(ctx, query, req.Address, req.Limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	type Holder struct {
		Address    string `json:"address"`
		Balance   string `json:"balance"`
		Percent   string `json:"percent"`
	}
	
	var results []Holder
	for rows.Next() {
		var h Holder
		if err := rows.Scan(&h.Address, &h.Balance, &h.Percent); err != nil {
			return nil, err
		}
		results = append(results, h)
	}
	
	return results, nil
}

// exportNFTHolders exports NFT holder data
func (s *ExportService) exportNFTHolders(ctx context.Context, req ExportRequest) (interface{}, error) {
	query := `
		SELECT address, COUNT(*) as tokens_owned
		FROM nft_owners
		WHERE token_address = $1
		GROUP BY address
		ORDER BY tokens_owned DESC
		LIMIT $2
	`
	
	rows, err := s.db.Query(ctx, query, req.Address, req.Limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	type Holder struct {
		Address     string `json:"address"`
		TokensOwned uint64 `json:"tokensOwned"`
	}
	
	var results []Holder
	for rows.Next() {
		var h Holder
		if err := rows.Scan(&h.Address, &h.TokensOwned); err != nil {
			return nil, err
		}
		results = append(results, h)
	}
	
	return results, nil
}

// exportInternalTransfers exports internal transfer data
func (s *ExportService) exportInternalTransfers(ctx context.Context, req ExportRequest) (interface{}, error) {
	query := `
		SELECT transaction_hash, block_number, from_address, to_address, 
		       value, trace_address, type
		FROM internal_transactions
		WHERE 1=1
	`
	
	args := []interface{}{}
	argIdx := 1
	
	if req.Address != "" {
		query += fmt.Sprintf(" AND (from_address = $%d OR to_address = $%d)", argIdx, argIdx)
		args = append(args, req.Address)
		argIdx++
	}
	if req.StartBlock > 0 {
		query += fmt.Sprintf(" AND block_number >= $%d", argIdx)
		args = append(args, req.StartBlock)
		argIdx++
	}
	if req.EndBlock > 0 {
		query += fmt.Sprintf(" AND block_number <= $%d", argIdx)
		args = append(args, req.EndBlock)
		argIdx++
	}
	
	query += fmt.Sprintf(" ORDER BY block_number DESC, trace_address LIMIT $%d", argIdx)
	args = append(args, req.Limit)
	
	type Transfer struct {
		TxHash       string `json:"transactionHash"`
		BlockNumber uint64 `json:"blockNumber"`
		From        string `json:"from"`
		To          string `json:"to"`
		Value       string `json:"value"`
		TraceAddr   string `json:"traceAddress"`
		Type       string `json:"type"`
	}
	
	rows, err := s.db.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var results []Transfer
	for rows.Next() {
		var t Transfer
		if err := rows.Scan(&t.TxHash, &t.BlockNumber, &t.From, 
			&t.To, &t.Value, &t.TraceAddr, &t.Type); err != nil {
			return nil, err
		}
		results = append(results, t)
	}
	
	return results, nil
}

// exportLogs exports event log data
func (s *ExportService) exportLogs(ctx context.Context, req ExportRequest) (interface{}, error) {
	query := `
		SELECT transaction_hash, block_number, address, topics, data, log_index
		FROM logs
		WHERE 1=1
	`
	
	args := []interface{}{}
	argIdx := 1
	
	if req.Address != "" {
		query += fmt.Sprintf(" AND address = $%d", argIdx)
		args = append(args, req.Address)
		argIdx++
	}
	if req.StartBlock > 0 {
		query += fmt.Sprintf(" AND block_number >= $%d", argIdx)
		args = append(args, req.StartBlock)
		argIdx++
	}
	if req.EndBlock > 0 {
		query += fmt.Sprintf(" AND block_number <= $%d", argIdx)
		args = append(args, req.EndBlock)
		argIdx++
	}
	
	query += fmt.Sprintf(" ORDER BY block_number DESC, log_index LIMIT $%d", argIdx)
	args = append(args, req.Limit)
	
	type Log struct {
		TxHash     string   `json:"transactionHash"`
		BlockNum  uint64   `json:"blockNumber"`
		Address   string   `json:"address"`
		Topics    []string `json:"topics"`
		Data      string   `json:"data"`
		LogIndex  uint64   `json:"logIndex"`
	}
	
	rows, err := s.db.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var results []Log
	for rows.Next() {
		var l Log
		var topics string
		if err := rows.Scan(&l.TxHash, &l.BlockNum, &l.Address, 
			&topics, &l.Data, &l.LogIndex); err != nil {
			return nil, err
		}
		results = append(results, l)
	}
	
	return results, nil
}

// writeCSV writes data in CSV format
func (s *ExportService) writeCSV(c *gin.Context, data interface{}, dataType string) {
	c.Header("Content-Type", "text/csv")
	c.Header("Content-Disposition", fmt.Sprintf("attachment; filename=%s_%s.csv", 
		dataType, time.Now().Format("20060102")))
	
	writer := csv.NewWriter(c.Writer)
	defer writer.Flush()
	
	// Write header and data based on type
	switch dataType {
	case "transactions":
		writer.Write([]string{"hash", "blockNumber", "from", "to", "value", "gasPrice", "gasUsed", "timestamp", "status"})
	case "token_transfers":
		writer.Write([]string{"transactionHash", "blockNumber", "tokenAddress", "from", "to", "amount", "timestamp"})
	case "blocks":
		writer.Write([]string{"number", "hash", "parentHash", "miner", "gasUsed", "gasLimit", "timestamp", "transactionCount"})
	}
	
	// In production, iterate over data and write rows
	writer.Write([]string{})
}

// writeNDJSON writes data in NDJSON format
func (s *ExportService) writeNDJSON(c *gin.Context, data interface{}) {
	c.Header("Content-Type", "application/x-ndjson")
	c.Header("Content-Disposition", fmt.Sprintf("attachment; filename=export_%s.ndjson", 
		time.Now().Format("20060102")))
	
	encoder := json.NewEncoder(c.Writer)
	encoder.SetEscapeHTML(false)
	encoder.Encode(data)
}

// countRecords counts the number of records in data
func (s *ExportService) countRecords(data interface{}) int {
	switch v := data.(type) {
	case []interface{}:
		return len(v)
	default:
		return 0
	}
}

// Router sets up the export router
func (s *ExportService) Router() *gin.Engine {
	r := gin.Default()
	
	r.GET("/export", s.handleExport)
	r.POST("/export", s.handleExport)
	
	return r
}

// Start starts the export server
func (s *ExportService) Start() error {
	r := s.Router()
	return r.Run(s.config.Port)
}

// StartExportServer starts the export server
func StartExportServer(port string, dbURL string) error {
	config := &Config{
		DBURL:    dbURL,
		Port:    port,
		MaxRows: 100000,
	}
	
	svc, err := NewExportService(config)
	if err != nil {
		return err
	}
	
	return svc.Start()
}

// ParseLimit parses the limit parameter
func ParseLimit(limitStr string, defaultLimit int, maxLimit int) int {
	limit, err := strconv.Atoi(limitStr)
	if err != nil || limit <= 0 {
		return defaultLimit
	}
	if limit > maxLimit {
		return maxLimit
	}
	return limit
}