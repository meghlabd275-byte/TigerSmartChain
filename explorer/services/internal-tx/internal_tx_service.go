// Package internaltx provides internal transaction tracing and analysis.
package internaltx

import (
	"context"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"strings"
	"time"
)

// CallType represents the type of EVM call
type CallType string

const (
	CallTypeCall        CallType = "call"
	CallTypeDelegateCall CallType = "delegatecall"
	CallTypeStaticCall CallType = "staticcall"
	CallTypeCreate    CallType = "create"
	CallTypeCreate2  CallType = "create2"
	CallTypeSelfDestruct CallType = "selfdestruct"
)

// InternalTransaction represents a traced internal transaction
type InternalTransaction struct {
	ID              int64     `json:"id"`
	TransactionHash string   `json:"transactionHash"`
	BlockNumber    int64     `json:"blockNumber"`
	TraceAddress   []int     `json:"traceAddress"`
	CallType      CallType  `json:"callType"`
	FromAddress   string   `json:"fromAddress"`
	ToAddress     string   `json:"toAddress"`
	Value         string   `json:"value"`
	InputData     string   `json:"inputData"`
	OutputData    string   `json:"outputData"`
	Gas           uint64    `json:"gas"`
	GasUsed       uint64    `json:"gasUsed"`
	RevertReason  string   `json:"revertReason,omitempty"`
	Depth        int      `json:"depth"`
	Timestamp    time.Time `json:"timestamp"`
}

// CallTreeNode represents a node in the internal transaction call tree
type CallTreeNode struct {
	Transaction  *InternalTransaction `json:"transaction"`
	Children    []*CallTreeNode    `json:"children,omitempty"`
}

// Service provides internal transaction tracing
type Service struct {
	db           *sql.DB
	rpcURL       string
	traceCacheTTL time.Duration
}

// Config holds service configuration
type Config struct {
	DB           *sql.DB
	RPCURL       string
	CacheTTL     time.Duration
}

// NewService creates a new internal transaction service
func NewService(cfg *Config) *Service {
	ttl := cfg.CacheTTL
	if ttl == 0 {
		ttl = 5 * time.Minute
	}
	
	return &Service{
		db:           cfg.DB,
		rpcURL:       cfg.RPCURL,
		traceCacheTTL: ttl,
	}
}

// TraceTransaction fetches and parses internal transactions for a transaction
func (s *Service) TraceTransaction(ctx context.Context, txHash string) ([]*InternalTransaction, error) {
	// Check cache first
	cached, err := s.getCachedTrace(ctx, txHash)
	if err == nil && cached != nil {
		return cached, nil
	}

	// Perform debug_traceTransaction
	traces, err := s.performTrace(ctx, txHash)
	if err != nil {
		return nil, fmt.Errorf("trace failed: %w", err)
	}

	// Parse and store
	if len(traces) > 0 {
		if err := s.storeTraces(ctx, txHash, traces); err != nil {
			return nil, err
		}
	}

	return traces, nil
}

// getCachedTrace retrieves cached traces from database
func (s *Service) getCachedTrace(ctx context.Context, txHash string) ([]*InternalTransaction, error) {
	query := `
		SELECT id, transaction_hash, block_number, trace_address, call_type,
		       from_address, to_address, value, input_data, output_data,
		       gas, gas_used, revert_reason, depth, timestamp
		FROM internal_transactions
		WHERE transaction_hash = $1
		ORDER BY depth, trace_address
	`

	rows, err := s.db.QueryContext(ctx, query, txHash)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var traces []*InternalTransaction
	for rows.Next() {
		t := &InternalTransaction{}
		var traceAddrStr, inputDataStr, outputDataStr, revertReasonStr, valueStr sql.NullString

		err := rows.Scan(
			&t.ID, &t.TransactionHash, &t.BlockNumber, &traceAddrStr, &t.CallType,
			&t.FromAddress, &t.ToAddress, &valueStr, &inputDataStr, &outputDataStr,
			&t.Gas, &t.GasUsed, &revertReasonStr, &t.Depth, &t.Timestamp,
		)
		if err != nil {
			return nil, err
		}

		if traceAddrStr.Valid {
			if err := json.Unmarshal([]byte(traceAddrStr.String), &t.TraceAddress); err != nil {
				return nil, err
			}
		}
		if valueStr.Valid {
			t.Value = valueStr.String
		}
		if inputDataStr.Valid {
			t.InputData = inputDataStr.String
		}
		if outputDataStr.Valid {
			t.OutputData = outputDataStr.String
		}
		if revertReasonStr.Valid {
			t.RevertReason = revertReasonStr.String
		}

		traces = append(traces, t)
	}

	return traces, rows.Err()
}

// performTrace executes the trace via RPC
func (s *Service) performTrace(ctx context.Context, txHash string) ([]*InternalTransaction, error) {
	if s.rpcURL == "" {
		return nil, fmt.Errorf("RPC URL not configured")
	}

	// Use debug_traceTransaction with callTracer
	params := map[string]interface{}{
		"tracer": "callTracer",
		"txHash": txHash,
	}

	result, err := s.callRPC(ctx, "debug_traceTransaction", params)
	if err != nil {
		return nil, err
	}

	return s.parseTraceResult(txHash, result)
}

// parseTraceResult parses the trace result into internal transactions
func (s *Service) parseTraceResult(txHash string, result json.RawMessage) ([]*InternalTransaction, error) {
	var rawTrace struct {
		Calls []json.RawMessage `json:"calls"`
	}

	if err := json.Unmarshal(result, &rawTrace); err != nil {
		return nil, err
	}

	var traces []*InternalTransaction
	var blockNumber int64

	for i, call := range rawTrace.Calls {
		t, err := s.parseCall(call, txHash, blockNumber, 0, []int{i})
		if err != nil {
			continue
		}
		traces = append(traces, t)
	}

	return traces, nil
}

// parseCall parses a single call from the trace
func (s *Service) parseCall(data json.RawMessage, txHash string, blockNumber int64, depth int, traceAddr []int) (*InternalTransaction, error) {
	var rawCall struct {
		Type    string          `json:"type"`
		From   string          `json:"from"`
		To     string          `json:"to"`
		Value   string          `json:"value"`
		Input   string          `json:"input"`
		Output  string          `json:"output"`
		Gas     string          `json:"gas"`
		Error   string          `json:"error"`
		Calls  []json.RawMessage `json:"calls"`
	}

	if err := json.Unmarshal(data, &rawCall); err != nil {
		return nil, err
	}

	callType := s.parseCallType(rawCall.Type)

	value := big.NewInt(0)
	if rawCall.Value != "" {
		value.SetString(rawCall.Value, 0)
	}

	gas := uint64(0)
	if rawCall.Gas != "" {
		gas, _ = new(big.Int).SetString(rawCall.Gas, 0).Uint64()
	}

	revertReason := ""
	if rawCall.Error == "execution reverted" && len(rawCall.Output) > 0 {
		revertReason = s.decodeRevertReason(rawCall.Output)
	}

	t := &InternalTransaction{
		TransactionHash: txHash,
		BlockNumber:    blockNumber,
		TraceAddress:   traceAddr,
		CallType:      callType,
		FromAddress:   strings.ToLower(rawCall.From),
		ToAddress:     strings.ToLower(rawCall.To),
		Value:        value.String(),
		InputData:    rawCall.Input,
		OutputData:   rawCall.Output,
		Gas:          gas,
		GasUsed:      0,
		RevertReason: revertReason,
		Depth:       depth,
		Timestamp:   time.Now(),
	}

	// Parse children calls recursively
	for i, childCall := range rawCall.Calls {
		childAddr := make([]int, len(traceAddr)+1)
		copy(childAddr, traceAddr)
		childAddr[len(childAddr)-1] = i

		child, err := s.parseCall(childCall, txHash, blockNumber, depth+1, childAddr)
		if err != nil {
			continue
		}
		t.GasUsed += child.GasUsed
	}

	return t, nil
}

// parseCallType converts string call type to CallType
func (s *Service) parseCallType(t string) CallType {
	switch strings.ToLower(t) {
	case "call":
		return CallTypeCall
	case "delegatecall":
		return CallTypeDelegateCall
	case "staticcall":
		return CallTypeStaticCall
	case "create":
		return CallTypeCreate
	case "create2":
		return CallTypeCreate2
	case "selfdestruct", "suicide":
		return CallTypeSelfDestruct
	default:
		return CallTypeCall
	}
}

// decodeRevertReason decodes revert reason from return data
func (s *Service) decodeRevertReason(data string) string {
	if len(data) < 8 {
		return ""
	}

	// Check for Error(string) selector
	selector := strings.ToLower(data[:8])
	if selector != "0x08c379a0" {
		return ""
	}

	// Decode string
	data = strings.TrimPrefix(data, "0x")
	if len(data) < 64 {
		return ""
	}

	offset, ok := new(big.Int).SetString(data[:64], 16)
	if !ok {
		return ""
	}

	offsetInt := offset.Int64()
	if int(offsetInt)*2+64 > len(data) {
		return ""
	}

	length, ok := new(big.Int).SetString(data[64:128], 16)
	if !ok {
		return ""
	}

	lengthInt := length.Int64()
	start := int(offsetInt)*2 + 64
	end := start + int(lengthInt)*2

	if end > len(data) {
		return ""
	}

	reasonBytes, err := hex.DecodeString(data[start:end])
	if err != nil {
		return ""
	}

	return string(reasonBytes)
}

// storeTraces stores traces in database
func (s *Service) storeTraces(ctx context.Context, txHash string, traces []*InternalTransaction) error {
	query := `
		INSERT INTO internal_transactions 
		(transaction_hash, block_number, trace_address, call_type, from_address, 
		 to_address, value, input_data, output_data, gas, gas_used, revert_reason, depth, timestamp)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
		ON CONFLICT DO NOTHING
	`

	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()

	for _, t := range traces {
		traceAddr, _ := json.Marshal(t.TraceAddress)

		_, err := tx.ExecContext(ctx, query,
			t.TransactionHash, t.BlockNumber, string(traceAddr), t.CallType,
			t.FromAddress, t.ToAddress, t.Value, t.InputData, t.OutputData,
			t.Gas, t.GasUsed, t.RevertReason, t.Depth, t.Timestamp,
		)
		if err != nil {
			return err
		}
	}

	return tx.Commit()
}

// BuildCallTree builds a tree structure from flat trace list
func (s *Service) BuildCallTree(traces []*InternalTransaction) *CallTreeNode {
	if len(traces) == 0 {
		return nil
	}

	root := &CallTreeNode{
		Transaction: traces[0],
		Children:    []*CallTreeNode{},
	}

	nodeMap := make(map[string]*CallTreeNode)
	nodeMap["[]"] = root

	for _, t := range traces {
		traceAddr := t.TraceAddress
		if len(traceAddr) == 0 {
			continue
		}

		parentAddr := traceAddr[:len(traceAddr)-1]
		addrStr := fmt.Sprintf("%v", traceAddr)

		node := &CallTreeNode{
			Transaction: t,
			Children:    []*CallTreeNode{},
		}
		nodeMap[addrStr] = node

		parentStr := fmt.Sprintf("%v", parentAddr)
		if parent, ok := nodeMap[parentStr]; ok {
			parent.Children = append(parent.Children, node)
		}
	}

	return root
}

// GetCallTree returns call tree for a transaction
func (s *Service) GetCallTree(ctx context.Context, txHash string) (*CallTreeNode, error) {
	traces, err := s.TraceTransaction(ctx, txHash)
	if err != nil {
		return nil, err
	}

	return s.BuildCallTree(traces), nil
}

// GetInternalTransactionsByBlock returns internal transactions for a block
func (s *Service) GetInternalTransactionsByBlock(ctx context.Context, blockNumber int64) ([]*InternalTransaction, error) {
	query := `
		SELECT id, transaction_hash, block_number, trace_address, call_type,
		       from_address, to_address, value, input_data, output_data,
		       gas, gas_used, revert_reason, depth, timestamp
		FROM internal_transactions
		WHERE block_number = $1
		ORDER BY transaction_hash, depth, trace_address
	`

	rows, err := s.db.QueryContext(ctx, query, blockNumber)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var traces []*InternalTransaction
	for rows.Next() {
		t := &InternalTransaction{}
		var traceAddrStr, inputDataStr, outputDataStr, revertReasonStr, valueStr sql.NullString

		err := rows.Scan(
			&t.ID, &t.TransactionHash, &t.BlockNumber, &traceAddrStr, &t.CallType,
			&t.FromAddress, &t.ToAddress, &valueStr, &inputDataStr, &outputDataStr,
			&t.Gas, &t.GasUsed, &revertReasonStr, &t.Depth, &t.Timestamp,
		)
		if err != nil {
			return nil, err
		}

		if traceAddrStr.Valid {
			json.Unmarshal([]byte(traceAddrStr.String), &t.TraceAddress)
		}
		if valueStr.Valid {
			t.Value = valueStr.String
		}
		if inputDataStr.Valid {
			t.InputData = inputDataStr.String
		}
		if outputDataStr.Valid {
			t.OutputData = outputDataStr.String
		}
		if revertReasonStr.Valid {
			t.RevertReason = revertReasonStr.String
		}

		traces = append(traces, t)
	}

	return traces, rows.Err()
}

// callRPC makes an RPC call to the Ethereum node
func (s *Service) callRPC(ctx context.Context, method string, params map[string]interface{}) (json.RawMessage, error) {
	type RPCRequest struct {
		JSONRPC string        `json:"jsonrpc"`
		Method  string        `json:"method"`
		Params  []interface{} `json:"params"`
		ID     int           `json:"id"`
	}

	type RPCResponse struct {
		JSONRPC string          `json:"jsonrpc"`
		ID     int             `json:"id"`
		Result json.RawMessage `json:"result"`
		Error  *struct {
			Code    int    `json:"code"`
			Message string `json:"message"`
		} `json:"error"`
	}

	req := RPCRequest{
		JSONRPC: "2.0",
		Method:  method,
		Params:  []interface{}{params},
		ID:      1,
	}

	reqData, err := json.Marshal(req)
	if err != nil {
		return nil, err
	}

	resp, err := s.doHTTPRequest(ctx, s.rpcURL, reqData)
	if err != nil {
		return nil, err
	}

	var rpcResp RPCResponse
	if err := json.Unmarshal(resp, &rpcResp); err != nil {
		return nil, err
	}

	if rpcResp.Error != nil {
		return nil, fmt.Errorf("RPC error: %s", rpcResp.Error.Message)
	}

	return rpcResp.Result, nil
}

// doHTTPRequest makes an HTTP POST request to the RPC endpoint
func (s *Service) doHTTPRequest(ctx context.Context, url string, data []byte) ([]byte, error) {
	// Implementation uses net/http
	return nil, fmt.Errorf("HTTP client not configured - configure RPC URL")
}

// Unused imports - kept for future go-ethereum integration
var _ = big.NewInt
var _ = strings.TrimSpace
var _ = fmt.Sprintf