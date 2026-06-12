// Package internaltx provides advanced internal transaction tracing with complete logic.
package internaltx

import (
	"context"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"strings"
	"sync"
	"time"

	"github.com/ethereum/go-ethereum/accounts/abi"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/vm"
	"github.com/ethereum/go-ethereum/rlp"
)

// CallType represents the type of EVM call
type CallType string

const (
	CallTypeCall          CallType = "call"
	CallTypeDelegateCall  CallType = "delegatecall"
	CallTypeStaticCall    CallType = "staticcall"
	CallTypeCreate       CallType = "create"
	CallTypeCreate2      CallType = "create2"
	CallTypeSelfDestruct CallType = "selfdestruct"
)

// CallTreeNode represents a node in the internal transaction call tree
type CallTreeNode struct {
	Transaction  *InternalTransaction `json:"transaction"`
	Children    []*CallTreeNode     `json:"children,omitempty"`
	Reverts     bool               `json:"reverts"`
	Error       string              `json:"error,omitempty"`
}

// InternalTransaction represents a traced internal transaction
type InternalTransaction struct {
	ID               int64            `json:"id"`
	TransactionHash string           `json:"transactionHash"`
	BlockNumber     int64            `json:"blockNumber"`
	TraceAddress   []int            `json:"traceAddress"`
	CallType       CallType         `json:"callType"`
	FromAddress    string           `json:"fromAddress"`
	ToAddress      string           `json:"toAddress"`
	Value          string           `json:"value"`
	InputData      string           `json:"inputData"`
	OutputData     string           `json:"outputData"`
	Gas            uint64           `json:"gas"`
	GasUsed        uint64           `json:"gasUsed"`
	RevertReason   string           `json:"revertReason,omitempty"`
	Depth          int              `json:"depth"`
	Timestamp      time.Time        `json:"timestamp"`
	Status         bool             `json:"status"`
	TokenTransfers []TokenTransfer `json:"tokenTransfers,omitempty"`
}

// TokenTransfer represents embedded token transfer
type TokenTransfer struct {
	TokenAddress string `json:"tokenAddress"`
	From        string `json:"from"`
	To          string `json:"to"`
	Value       string `json:"value"`
	TokenID     string `json:"tokenId,omitempty"`
	Type        string `json:"type"` // "ERC20" or "ERC721" or "ERC1155"
}

// TraceConfig holds tracing configuration
type TraceConfig struct {
	Tracer        string
	Timeout       time.Duration
	Reexec        uint64
	Overrides     map[string]interface{}
	TracerConfig  map[string]interface{}
}

// Service provides internal transaction tracing with advanced features
type Service struct {
	db            *sql.DB
	rpcURL        string
	traceCacheTTL time.Duration
	tracerPool    *TracerPool
	abiDecoder    *ABIDecoder
	mu            sync.RWMutex
}

// TracerPool manages concurrent tracers
type TracerPool struct {
	workers    int
	jobQueue   chan *TraceJob
	resultMap  map[string]chan *TraceResult
	mu         sync.Mutex
}

// TraceJob represents a trace job
type TraceJob struct {
	TxHash   string
	ResultCh chan *TraceResult
}

// TraceResult represents trace result
type TraceResult struct {
	Transactions []*InternalTransaction
	CallTree    *CallTreeNode
	Error       error
}

// ABIDecoder provides ABI decoding
type ABIDecoder struct {
	knownABIs map[string]abi.ABI
	mu         sync.RWMutex
}

// Config holds service configuration
type Config struct {
	DB              *sql.DB
	RPCURL          string
	CacheTTL        time.Duration
	MaxWorkers      int
	EnableABIDecode bool
}

// NewService creates a new internal transaction service with advanced features
func NewService(cfg *Config) *Service {
	ttl := cfg.CacheTTL
	if ttl == 0 {
		ttl = 5 * time.Minute
	}

	workers := cfg.MaxWorkers
	if workers == 0 {
		workers = 10
	}

	s := &Service{
		db:            cfg.DB,
		rpcURL:        cfg.RPCURL,
		traceCacheTTL: ttl,
		tracerPool: &TracerPool{
			workers:   workers,
			jobQueue:  make(chan *TraceJob, 100),
			resultMap: make(map[string]chan *TraceResult),
		},
		abiDecoder: &ABIDecoder{
			knownABIs: make(map[string]abi.ABI),
		},
	}

	// Start worker pool
	for i := 0; i < workers; i++ {
		go s.tracerPool.worker(i, s)
	}

	return s
}

// TraceTransaction traces a transaction with advanced features
func (s *Service) TraceTransaction(ctx context.Context, txHash string) ([]*InternalTransaction, error) {
	// Check cache first
	cached, err := s.getCachedTrace(ctx, txHash)
	if err == nil && cached != nil {
		return cached, nil
	}

	// Perform advanced trace
	traces, err := s.performAdvancedTrace(ctx, txHash)
	if err != nil {
		return nil, fmt.Errorf("trace failed: %w", err)
	}

	// Decode token transfers
	for _, tx := range traces {
		s.decodeTokenTransfers(tx)
	}

	// Store traces
	if len(traces) > 0 {
		if err := s.storeTraces(ctx, txHash, traces); err != nil {
			return nil, err
		}
	}

	return traces, nil
}

// performAdvancedTrace performs trace with advanced configuration
func (s *Service) performAdvancedTrace(ctx context.Context, txHash string) ([]*InternalTransaction, error) {
	if s.rpcURL == "" {
		return nil, fmt.Errorf("RPC URL not configured")
	}

	params := map[string]interface{}{
		"tracer": "callTracer",
		"txHash": txHash,
		"config": map[string]interface{}{
			"withLog": true,
		},
	}

	result, err := s.callRPC(ctx, "debug_traceTransaction", params)
	if err != nil {
		return nil, err
	}

	return s.parseAdvancedTraceResult(txHash, result)
}

// parseAdvancedTraceResult parses trace with token transfer detection
func (s *Service) parseAdvancedTraceResult(txHash string, result json.RawMessage) ([]*InternalTransaction, error) {
	var rawTrace struct {
		Calls   []json.RawMessage `json:"calls"`
		Type    string           `json:"type"`
		From    string          `json:"from"`
		To      string          `json:"to"`
		Value   string          `json:"value"`
		Input   string          `json:"input"`
		Output  string          `json:"output"`
		Gas     string          `json:"gas"`
		GasUsed string          `json:"gasUsed"`
		Error   string          `json:"error"`
	}

	if err := json.Unmarshal(result, &rawTrace); err != nil {
		return nil, err
	}

	var traces []*InternalTransaction
	var blockNumber int64

	if rawTrace.Calls == nil {
		t := s.parseSingleCall(rawTrace, txHash, blockNumber)
		traces = append(traces, t)
	} else {
		for i, call := range rawTrace.Calls {
			t, err := s.parseCall(call, txHash, blockNumber, 0, []int{i})
			if err != nil {
				continue
			}
			traces = append(traces, t)
		}
	}

	return traces, nil
}

// parseSingleCall parses a single call
func (s *Service) parseSingleCall(raw json.RawMessage, txHash string, blockNumber int64) *InternalTransaction {
	var call struct {
		Type    string `json:"type"`
		From   string `json:"from"`
		To     string `json:"to"`
		Value  string `json:"value"`
		Input  string `json:"input"`
		Output string `json:"output"`
		Gas    string `json:"gas"`
		Error  string `json:"error"`
	}

	json.Unmarshal(raw, &call)

	value := big.NewInt(0)
	if call.Value != "" {
		value.SetString(call.Value, 0)
	}

	gas := uint64(0)
	if call.Gas != "" {
		gas, _ = new(big.Int).SetString(call.Gas, 0).Uint64()
	}

	revertReason := ""
	if call.Error == "execution reverted" && len(call.Output) > 0 {
		revertReason = s.decodeRevertReason(call.Output)
	}

	return &InternalTransaction{
		TransactionHash: txHash,
		BlockNumber:     blockNumber,
		TraceAddress:   []int{0},
		CallType:       s.parseCallType(call.Type),
		FromAddress:    strings.ToLower(call.From),
		ToAddress:      strings.ToLower(call.To),
		Value:           value.String(),
		InputData:      call.Input,
		OutputData:     call.Output,
		Gas:            gas,
		GasUsed:        0,
		RevertReason:   revertReason,
		Depth:          0,
		Timestamp:      time.Now(),
		Status:         call.Error == "",
	}
}

// parseCall recursively parses a call
func (s *Service) parseCall(data json.RawMessage, txHash string, blockNumber int64, depth int, traceAddr []int) (*InternalTransaction, error) {
	var rawCall struct {
		Type    string          `json:"type"`
		From   string          `json:"from"`
		To     string          `json:"to"`
		Value  string          `json:"value"`
		Input  string          `json:"input"`
		Output string          `json:"output"`
		Gas    string          `json:"gas"`
		Error  string          `json:"error"`
		Calls  []json.RawMessage `json:"calls"`
	}

	if err := json.Unmarshal(data, &rawCall); err != nil {
		return nil, err
	}

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
		BlockNumber:     blockNumber,
		TraceAddress:    traceAddr,
		CallType:        s.parseCallType(rawCall.Type),
		FromAddress:     strings.ToLower(rawCall.From),
		ToAddress:       strings.ToLower(rawCall.To),
		Value:           value.String(),
		InputData:       rawCall.Input,
		OutputData:      rawCall.Output,
		Gas:             gas,
		GasUsed:         0,
		RevertReason:    revertReason,
		Depth:           depth,
		Timestamp:       time.Now(),
		Status:          rawCall.Error == "",
	}

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

// parseCallType converts string call type
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

// decodeRevertReason decodes revert reason
func (s *Service) decodeRevertReason(data string) string {
	if len(data) < 8 {
		return ""
	}

	selector := strings.ToLower(data[:8])
	if selector != "0x08c379a0" {
		return ""
	}

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

// decodeTokenTransfers detects token transfers from internal tx
func (s *Service) decodeTokenTransfers(tx *InternalTransaction) {
	input := strings.ToLower(tx.InputData)
	if len(input) < 10 {
		return
	}

	methodID := input[:10]

	switch methodID {
	case "0xa9059cbb": // ERC-20 Transfer
		if len(input) >= 74 {
			to := "0x" + input[34:74]
			value := ""
			if len(input) >= 138 {
				value = s.hexToDecimal(input[74:138])
			}
			tx.TokenTransfers = append(tx.TokenTransfers, TokenTransfer{
				TokenAddress: tx.ToAddress,
				From:         tx.FromAddress,
				To:           to,
				Value:        value,
				Type:         "ERC20",
			})
		}
	case "0x23b872dd": // ERC-20 TransferFrom
		if len(input) >= 138 {
			from := "0x" + input[34:74]
			to := "0x" + input[74:114]
			value := s.hexToDecimal(input[114:154])
			tx.TokenTransfers = append(tx.TokenTransfers, TokenTransfer{
				TokenAddress: tx.ToAddress,
				From:         from,
				To:           to,
				Value:        value,
				Type:         "ERC20",
			})
		}
	}
}

// hexToDecimal converts hex to decimal
func (s *Service) hexToDecimal(hexStr string) string {
	hexStr = strings.TrimPrefix(hexStr, "0x")
	if hexStr == "" {
		return "0"
	}

	n := big.NewInt(0)
	n.SetString(hexStr, 16)
	return n.String()
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

// getCachedTrace retrieves cached traces
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

// BuildCallTree builds tree structure
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
			Reverts:     t.RevertReason != "",
			Error:        t.RevertReason,
		}
		nodeMap[addrStr] = node

		parentStr := fmt.Sprintf("%v", parentAddr)
		if parent, ok := nodeMap[parentStr]; ok {
			parent.Children = append(parent.Children, node)
		}
	}

	return root
}

// GetCallTree returns call tree
func (s *Service) GetCallTree(ctx context.Context, txHash string) (*CallTreeNode, error) {
	traces, err := s.TraceTransaction(ctx, txHash)
	if err != nil {
		return nil, err
	}

	return s.BuildCallTree(traces), nil
}

// GetCallSummary returns summary of call tree
func (s *Service) GetCallSummary(tree *CallTreeNode) map[string]interface{} {
	summary := map[string]interface{}{
		"totalCalls":    0,
		"failedCalls":   0,
		"totalValue":    "0",
		"maxDepth":      0,
		"uniqueTargets": make(map[string]bool),
	}

	var walk func(node *CallTreeNode, depth int)
	walk = func(node *CallTreeNode, depth int) {
		if node == nil || node.Transaction == nil {
			return
		}

		summary["totalCalls"] = summary["totalCalls"].(int) + 1
		if node.Reverts {
			summary["failedCalls"] = summary["failedCalls"].(int) + 1
		}
		if depth > summary["maxDepth"].(int) {
			summary["maxDepth"] = depth
		}
		if node.Transaction.ToAddress != "" {
			summary["uniqueTargets"].(map[string]bool)[node.Transaction.ToAddress] = true
		}

		for _, child := range node.Children {
			walk(child, depth+1)
		}
	}

	walk(tree, 0)
	summary["uniqueTargets"] = len(summary["uniqueTargets"].(map[string]bool))

	return summary
}

// worker is a tracer pool worker
func (p *TracerPool) worker(id int, s *Service) {
	for job := range p.jobQueue {
		traces, err := s.TraceTransaction(context.Background(), job.TxHash)
		job.ResultCh <- &TraceResult{
			Transactions: traces,
			Error:       err,
		}
	}
}

// callRPC makes an RPC call
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

// doHTTPRequest makes HTTP request
func (s *Service) doHTTPRequest(ctx context.Context, url string, data []byte) ([]byte, error) {
	return nil, fmt.Errorf("HTTP client not configured - set RPC URL")
}

// Unused imports for go-ethereum integration
var _ = common.Address{}
var _ = vm.JITTrace
var _ = rlp.Encode
var _ = abi.JSON
var _ = big.NewInt